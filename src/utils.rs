use sqlparser::ast::*;

use rook_ast::{
    AlterTableAction, AlterTablePlan, ArithOp, BinaryOp, BooleanTest, ColumnDef, ComparisonOp, ConstantValue,
    CreateDatabasePlan, CreateIndexPlan, CreateTablePlan, CreateViewPlan, CteDef, DeletePlan,
    DropDatabasePlan, DropIndexPlan, DropTablePlan, DropViewPlan, ExprNode, FunctionArg, InsertPlan, JoinClause, JoinType,
    OrderByExpr, PredicateNode, QueryPlan, SelectExpr, SelectPlan, SetAssignment,
    TableConstraintDef, TableRef, TruncatePlan, UpdatePlan,
};

// Re-import sqlparser's LimitClause explicitly to avoid ambiguity with rook_ast::LimitClause
use sqlparser::ast::LimitClause as SqlLimitClause;
use rook_ast::LimitClause as RookLimitClause;

// ── Top-level dispatch ────────────────────────────────────────────────────────

/// Validate an identifier that will be used as part of a filesystem path.
///
/// Defense-in-depth alongside the engine's own `name_validation` module:
/// rejects empty names, path separators (`/`, `\\`), `..` segments, leading
/// dots, NUL/control characters, and overlong names before any plan is built.
fn check_identifier(name: &str, kind: &str) -> Result<(), String> {
    fn invalid(name: &str, kind: &str, why: &str) -> String {
        format!("Invalid {} name '{}': {}", kind, name, why)
    }
    if name.is_empty() {
        return Err(invalid(name, kind, "cannot be empty"));
    }
    if name.len() > 255 {
        return Err(invalid(name, kind, "exceeds the 255 character limit"));
    }
    if name.chars().any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control()) {
        return Err(invalid(name, kind, "contains forbidden characters"));
    }
    if name.starts_with('.') {
        return Err(invalid(name, kind, "cannot start with a dot"));
    }
    if name == ".."
        || name.split('/').any(|seg| seg == "..")
        || name.split('\\').any(|seg| seg == "..")
    {
        return Err(invalid(name, kind, "cannot contain '..' path segments"));
    }
    Ok(())
}

/// Public wrapper around [`check_identifier`] for statement types matched
/// outside the grammar dispatch (e.g. VACUUM).
///
/// `allow(dead_code)`: the standalone parser CLI binary also compiles this
/// module without calling it.
#[allow(dead_code)]
pub fn check_identifier_public(name: &str, kind: &str) -> Result<(), String> {
    check_identifier(name, kind)
}

/// Validate every identifier segment of a (possibly qualified) object name.
///
/// Uses the *raw* identifier values — `ObjectName`'s Display re-quotes
/// delimited identifiers (e.g. `` `..` `` renders as `".."`), which would
/// silently bypass the path-segment checks.
fn check_object_name(name: &ObjectName, kind: &str) -> Result<(), String> {
    for part in &name.0 {
        match part.as_ident() {
            Some(ident) => check_identifier(&ident.value, kind)?,
            None => {
                return Err(format!(
                    "Invalid {} name '{}': unsupported name form",
                    kind,
                    part
                ))
            }
        }
    }
    Ok(())
}

/// Build a `QueryPlan` from a parsed sqlparser `Statement`.
pub fn build_query_plan(stmt: &Statement) -> Result<QueryPlan, String> {
    match stmt {
        Statement::Query(query) => {
            let set_expr = &*query.body;
            match set_expr {
                SetExpr::Select(_) => {
                    let select_plan = extract_select_params(query)?;
                    Ok(QueryPlan::Select(select_plan))
                }
                SetExpr::SetOperation { op, left, right, set_quantifier } => {
                    // Build a synthetic Query around each side without outer ORDER BY / LIMIT
                    let left_query = Query {
                        with: query.with.clone(),
                        body: Box::new(left.as_ref().clone()),
                        order_by: None,
                        limit_clause: None,
                        fetch: None,
                        locks: query.locks.clone(),
                        for_clause: query.for_clause.clone(),
                        settings: None,
                        format_clause: None,
                        pipe_operators: Vec::new(),
                    };
                    let right_query = Query {
                        with: None,
                        body: Box::new(right.as_ref().clone()),
                        order_by: None,
                        limit_clause: None,
                        fetch: None,
                        locks: Vec::new(),
                        for_clause: None,
                        settings: None,
                        format_clause: None,
                        pipe_operators: Vec::new(),
                    };

                    let left_plan = extract_select_params(&left_query)?;
                    let right_plan = extract_select_params(&right_query)?;

                    let all = *set_quantifier == SetQuantifier::All;
                    let op_str = match op {
                        SetOperator::Union => "UNION",
                        SetOperator::Intersect => "INTERSECT",
                        SetOperator::Except | SetOperator::Minus => "EXCEPT",
                    };

                    let order_by = extract_order_by_from_query(&query.order_by)?;
                    let limit = extract_limit_from_query(&query.limit_clause);

                    Ok(QueryPlan::SetOperation(rook_ast::SetOperationPlan {
                        left: left_plan,
                        right: right_plan,
                        op: op_str.to_string(),
                        all,
                        ctes: Vec::new(), // CTEs extracted from the original query above
                        order_by,
                        limit,
                    }))
                }
                _ => Err("Unsupported query body type".to_string()),
            }
        }
        Statement::Insert(insert) => {
            let plan = extract_insert_params(insert)?;
            Ok(QueryPlan::Insert(plan))
        }
        Statement::Update(update) => {
            // `update.table` is `TableWithJoins` in sqlparser 0.61.0
            let table_name = match &update.table.relation {
                TableFactor::Table { name, .. } => name.to_string(),
                _ => update.table.relation.to_string(),
            };
            // `Assignment` has fields `target` (AssignmentTarget) and `value` (Expr)
            let set_assignments: Vec<SetAssignment> = update
                .assignments
                .iter()
                .map(|a| {
                    // Extract column name from the assignment target
                    let col_name = match &a.target {
                        AssignmentTarget::ColumnName(name) => {
                            name.to_string()
                        }
                        _ => a.target.to_string(),
                    };
                    Ok(SetAssignment {
                        column: col_name,
                        value: convert_expr(&a.value)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let selection = update
                .selection
                .as_ref()
                .map(convert_predicate)
                .transpose()?;
            Ok(QueryPlan::Update(UpdatePlan {
                table: table_name,
                assignments: set_assignments,
                selection,
            }))
        }
        Statement::Delete(delete) => {
            // `delete.tables` is `Vec<ObjectName>` (MySQL multi-delete),
            // `delete.from` is `FromTable` (standard SQL FROM clause)
            let table = extract_delete_table(delete);
            let selection = delete
                .selection
                .as_ref()
                .map(convert_predicate)
                .transpose()?;
            Ok(QueryPlan::Delete(DeletePlan { table, selection }))
        }
        Statement::CreateTable(create) => {
            // If the CREATE TABLE has a query body, it's CREATE TABLE ... AS SELECT
            check_object_name(&create.name, "table")?;
            let table_name = create.name.to_string();
            if let Some(query) = &create.query {
                let select_plan = extract_select_params(query)?;
                return Ok(QueryPlan::CreateTableAsSelect(
                    rook_ast::CreateTableAsSelectPlan {
                        table: table_name,
                        query: Box::new(select_plan),
                    },
                ));
            }
            let plan = extract_create_table_params(create)?;
            Ok(QueryPlan::CreateTable(plan))
        }
        Statement::CreateDatabase {
            db_name,
            if_not_exists,
            ..
        } => {
            check_object_name(db_name, "database")?;
            let database = db_name.to_string();
            Ok(QueryPlan::CreateDatabase(CreateDatabasePlan {
                database,
                if_not_exists: *if_not_exists,
            }))
        }
        Statement::CreateIndex(ci) => {
            check_object_name(&ci.table_name, "table")?;
            let table_name = ci.table_name.to_string();
            if let Some(n) = &ci.name {
                check_object_name(n, "index")?;
            }
            // Collect every indexed column — multiple columns form a
            // composite key. Expressions are not supported.
            let mut columns = Vec::new();
            for oe in &ci.columns {
                match &oe.column.expr {
                    Expr::Identifier(ident) => columns.push(ident.value.clone()),
                    _ => {
                        return Err(
                            "CREATE INDEX requires simple column references, not expressions"
                                .to_string(),
                        )
                    }
                }
            }
            if columns.is_empty() {
                return Err("CREATE INDEX requires at least one column".to_string());
            }
            Ok(QueryPlan::CreateIndex(CreateIndexPlan {
                index_name: ci.name.as_ref().map(|n| n.to_string()).unwrap_or_default(),
                table_name,
                columns,
            }))
        }
        Statement::Drop {
            object_type,
            if_exists,
            names,
            cascade,
            restrict: _,
            purge: _,
            temporary: _,
            table,
        } => {
            match object_type {
                // DROP TABLE
                ObjectType::Table => {
                    if let Some(name) = names.first() {
                        check_object_name(name, "table")?;
                    }
                    let table_name = names
                        .first()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    Ok(QueryPlan::DropTable(DropTablePlan {
                        table: table_name,
                        if_exists: *if_exists,
                        cascade: *cascade,
                    }))
                }
                // DROP INDEX
                ObjectType::Index => {
                    if let Some(name) = names.first() {
                        check_object_name(name, "index")?;
                    }
                    let index_name = names
                        .first()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    let tbl_name = table
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    Ok(QueryPlan::DropIndex(DropIndexPlan {
                        index_name,
                        table_name: tbl_name,
                        column_name: String::new(),
                        if_exists: *if_exists,
                    }))
                }
                // DROP VIEW
                ObjectType::View => {
                    let view_name = names
                        .first()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    Ok(QueryPlan::DropView(DropViewPlan {
                        name: view_name,
                        if_exists: *if_exists,
                    }))
                }
                // DROP DATABASE
                ObjectType::Database => {
                    if let Some(name) = names.first() {
                        check_object_name(name, "database")?;
                    }
                    let db_name = names
                        .first()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    Ok(QueryPlan::DropDatabase(DropDatabasePlan {
                        database: db_name,
                        if_exists: *if_exists,
                    }))
                }
                _ => Ok(QueryPlan::Unknown(stmt.to_string())),
            }
        }
        Statement::AlterTable(alter_table) => {
            check_object_name(&alter_table.name, "table")?;
            let table = alter_table.name.to_string();
            // Only handle the first operation for now
            let action = match alter_table.operations.first() {
                Some(AlterTableOperation::AddColumn {
                    column_def, ..
                }) => AlterTableAction::AddColumn {
                    column_def: ColumnDef {
                        name: column_def.name.to_string(),
                        data_type: column_def.data_type.to_string(),
                        constraints: column_def
                            .options
                            .iter()
                            .map(|opt| opt.option.to_string())
                            .collect(),
                    },
                },
                Some(AlterTableOperation::DropColumn {
                    column_names,
                    ..
                }) => {
                    let column = column_names
                        .first()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    AlterTableAction::DropColumn { column }
                }
                Some(AlterTableOperation::RenameColumn {
                    old_column_name,
                    new_column_name,
                    ..
                }) => AlterTableAction::RenameColumn {
                    old_name: old_column_name.to_string(),
                    new_name: new_column_name.to_string(),
                },
                Some(AlterTableOperation::RenameTable {
                    table_name,
                }) => {
                    // sqlparser 0.61.0 uses RenameTableNameKind which has Display that
                    // prepends "TO " or "AS " to the name (e.g. "TO products").
                    // We need to extract just the plain name.
                    let new_name = match table_name {
                        RenameTableNameKind::To(name) | RenameTableNameKind::As(name) => name.to_string(),
                    };
                    check_identifier(&new_name, "table")?;
                    AlterTableAction::RenameTable { new_name }
                },
                Some(AlterTableOperation::AlterColumn {
                    column_name,
                    op,
                }) => {
                    match op {
                        AlterColumnOperation::SetDefault { value } => {
                            let default_text = format!("{}", value);
                            AlterTableAction::SetDefault {
                                column: column_name.to_string(),
                                default_expr: default_text,
                            }
                        }
                        AlterColumnOperation::DropDefault => {
                            AlterTableAction::DropDefault {
                                column: column_name.to_string(),
                            }
                        }
                        AlterColumnOperation::SetNotNull => {
                            AlterTableAction::SetNotNull {
                                column: column_name.to_string(),
                            }
                        }
                        AlterColumnOperation::DropNotNull => {
                            AlterTableAction::DropNotNull {
                                column: column_name.to_string(),
                            }
                        }
                        _ => {
                            return Ok(QueryPlan::Unknown(format!(
                                "Unsupported ALTER COLUMN operation: {}",
                                op
                            )));
                        }
                    }
                },
                Some(op) => {
                    return Ok(QueryPlan::Unknown(format!(
                        "Unsupported ALTER TABLE operation: {}",
                        op
                    )));
                }
                None => {
                    return Ok(QueryPlan::Unknown(
                        "ALTER TABLE with no operations".to_string(),
                    ));
                }
            };
            Ok(QueryPlan::AlterTable(AlterTablePlan { table, action }))
        }
        Statement::CreateView(create_view) => {
            let select_plan = extract_select_params(&create_view.query)?;
            Ok(QueryPlan::CreateView(CreateViewPlan {
                name: create_view.name.to_string(),
                or_replace: create_view.or_replace,
                query: Box::new(select_plan),
            }))
        }
        Statement::Truncate(truncate) => {
            if let Some(target) = truncate.table_names.first() {
                check_object_name(&target.name, "table")?;
            }
            let table = truncate
                .table_names
                .first()
                .map(|t| t.to_string())
                .unwrap_or_default();
            Ok(QueryPlan::Truncate(TruncatePlan { table }))
        }
        Statement::ShowTables { .. } => Ok(QueryPlan::ShowTables),
        Statement::ShowDatabases { .. } => Ok(QueryPlan::ShowDatabases),
        Statement::Use(use_stmt) => {
            // sqlparser 0.61.0 `Use` is an enum, and the Display impl formats as
            // `"USE <name>"`.  We extract everything after "USE " to get the
            // database/schema name.  This is simpler than matching on every
            // possible variant (Database, Schema, Catalog, etc.).
            let text = format!("{}", use_stmt);
            // Remove leading "USE " (case-insensitive but sqlparser always
            // produces uppercase "USE").
            let db_name = if let Some(stripped) = text.strip_prefix("USE ") {
                stripped.to_string()
            } else {
                text.trim().to_string()
            };
            // Strip any quoting the Display impl may have added.
            let unquoted = db_name
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            check_identifier(&unquoted, "database")?;
            Ok(QueryPlan::UseDatabase(db_name))
        }
        _ => Ok(QueryPlan::Unknown(stmt.to_string())),
    }
}

// ── SELECT extraction ─────────────────────────────────────────────────────────

pub(crate) fn extract_order_by_from_query(
    query_order_by: &Option<sqlparser::ast::OrderBy>,
) -> Result<Vec<OrderByExpr>, String> {
    match query_order_by {
        Some(order_by) => match &order_by.kind {
            OrderByKind::Expressions(exprs) => exprs
                .iter()
                .map(|o| {
                    Ok::<rook_ast::OrderByExpr, String>(OrderByExpr {
                        expr: convert_expr(&o.expr)?,
                        ascending: o.options.asc.unwrap_or(true),
                    })
                })
                .collect::<Result<Vec<_>, _>>(),
            _ => Ok(Vec::new()),
        },
        None => Ok(Vec::new()),
    }
}

pub(crate) fn extract_limit_from_query(
    limit_clause: &Option<SqlLimitClause>,
) -> Option<RookLimitClause> {
    limit_clause.as_ref().and_then(|lc| {
        match lc {
            SqlLimitClause::LimitOffset { limit: limit_opt, offset: offset_opt, limit_by: _ } => {
                let offset: u64 = match offset_opt {
                    Some(sqlparser::ast::Offset { value: Expr::Value(v), .. }) => {
                        v.value.to_string().parse::<u64>().unwrap_or(0)
                    }
                    _ => 0,
                };
                match limit_opt {
                    Some(Expr::Value(v)) => {
                        let limit_val = v.value.to_string().parse::<u64>().ok()?;
                        Some(RookLimitClause { limit: limit_val, offset: Some(offset) })
                    }
                    None => {
                        if offset > 0 {
                            Some(RookLimitClause { limit: u64::MAX, offset: Some(offset) })
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            SqlLimitClause::OffsetCommaLimit { limit: limit_expr, .. } => {
                let limit_val = match limit_expr {
                    Expr::Value(v) => v.value.to_string().parse::<u64>().ok()?,
                    _ => return None,
                };
                Some(RookLimitClause { limit: limit_val, offset: Some(0) })
            }
        }
    })
}

/// Extract a full `SelectPlan` from a `Query`, capturing ORDER BY, LIMIT, GROUP BY,
/// HAVING, and DISTINCT that were previously discarded.
pub fn extract_select_params(query: &Query) -> Result<SelectPlan, String> {
    // ── CTEs (WITH clause) ────────────────────────────────────────────────────
    //
    // Collect non-recursive CTEs from the WITH clause if present.
    // Each CTE body is parsed independently and stored as a `CteDef`.
    // CTE names are also tracked so that FROM-clause references can be
    // flagged with the special `__cte__:<name>` prefix understood by the
    // logical planner.
    let mut ctes: Vec<CteDef> = Vec::new();
    let mut cte_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(with) = &query.with {
        for cte_tbl in &with.cte_tables {
            let name = cte_tbl.alias.name.value.clone();

            if with.recursive {
                // ── Recursive CTE: parse the UNION ALL/UNION structure ──
                //
                // The CTE body must be a SetOperation (e.g. non_recursive UNION ALL recursive).
                // Register the CTE name FIRST so the recursive term's FROM-clause references
                // get the `__cte__:name` prefix.
                cte_names.insert(name.to_ascii_lowercase());

                let (non_recursive, recursive_term, union_all) =
                    match &*cte_tbl.query.body {
                    SetExpr::SetOperation {
                        op: SetOperator::Union,
                        left,
                        right,
                        set_quantifier,
                    } => {
                            // Extract the non-recursive term (left side of UNION)
                            let non_rec = match left.as_ref() {
                                SetExpr::Select(s) => extract_select_inner(
                                    s, &mut cte_names
                                )?,
                                _ => return Err(
                                    "Non-recursive term of recursive CTE must be a SELECT".to_string()
                                ),
                            };
                            // Extract the recursive term (right side of UNION)
                            // The CTE name is now in cte_names, so FROM cte references will get __cte__: prefix
                            let rec_term = match right.as_ref() {
                                SetExpr::Select(s) => extract_select_inner(
                                    s, &mut cte_names
                                )?,
                                _ => return Err(
                                    "Recursive term of recursive CTE must be a SELECT".to_string()
                                ),
                            };
                            (non_rec, Some(Box::new(rec_term)), *set_quantifier == SetQuantifier::All)
                        }
                        _ => {
                            return Err(format!(
                                "Recursive CTE '{}' body must be a UNION ALL/UNION of two SELECTs",
                                name
                            ));
                        }
                    };

                ctes.push(CteDef {
                    name: name.clone(),
                    query: Box::new(non_recursive),
                    recursive_term,
                    union_all,
                });
                // Name was already inserted at the start of the recursive block
            } else {
                // Non-recursive CTE: normal parsing
                let inner_plan = extract_select_params(&cte_tbl.query)?;
                ctes.push(CteDef {
                    name: name.clone(),
                    query: Box::new(inner_plan),
                    recursive_term: None,
                    union_all: false,
                });
                cte_names.insert(name.to_ascii_lowercase());
            }
        }
    }

    let set_expr = &*query.body;
    let select = match set_expr {
        SetExpr::Select(s) => s,
        SetExpr::SetOperation { .. } => {
            // Set operations are handled at the build_query_plan level.
            // This function is called recursively for CTE bodies and subqueries
            // where UNION/INTERSECT/EXCEPT are not yet supported.
            return Err("Set operations (UNION/INTERSECT/EXCEPT) at this level are handled by build_query_plan".to_string());
        }
        _ => return Err("Unsupported query body type".to_string()),
    };

    // Extract the inner SELECT fields (projections, FROM, WHERE, etc.)
    let inner = extract_select_inner(select, &mut cte_names)?;

    // ── ORDER BY (from the outer Query, NOT from Select) ──────────────────────
    let order_by: Vec<OrderByExpr> = extract_order_by_from_query(&query.order_by)?;

    // ── LIMIT (from the outer Query) ──────────────────────────────────────────
    let limit: Option<RookLimitClause> = extract_limit_from_query(&query.limit_clause);

    // Merge WITH-clause CTEs with derived subqueries from FROM/JOIN clauses.
    let mut all_ctes = ctes;
    all_ctes.extend(inner.ctes);

    Ok(SelectPlan {
        ctes: all_ctes,
        projections: inner.projections,
        from: inner.from,
        joins: inner.joins,
        selection: inner.selection,
        group_by: inner.group_by,
        having: inner.having,
        order_by,
        limit,
        distinct: inner.distinct,
    })
}

/// Extract a `SelectPlan` from just the inner parts of a `Select` node.
///
/// Used for the non-recursive and recursive terms of a recursive CTE body.
/// Does NOT include WITH, ORDER BY, or LIMIT (those come from the outer Query).
fn extract_select_inner(
    select: &sqlparser::ast::Select,
    cte_names: &mut std::collections::HashSet<String>,
) -> Result<SelectPlan, String> {
    // ── Projections ───────────────────────────────────────────────────────────
    let projections: Vec<SelectExpr> = select
        .projection
        .iter()
        .map(convert_select_item)
        .collect::<Result<Vec<_>, _>>()?;

    // ── FROM tables ───────────────────────────────────────────────────────────
    let mut from: Vec<TableRef> = Vec::new();
    let mut joins: Vec<JoinClause> = Vec::new();
    // Collect derived subqueries found in the FROM clause.
    // These will be returned in the ctes field of the SelectPlan and merged
    // with WITH-clause CTEs by the caller (extract_select_params).
    let mut derived_subqueries: Vec<CteDef> = Vec::new();

    for table_with_joins in &select.from {
        if let TableFactor::Table { name, alias, .. } = &table_with_joins.relation {
            let raw_name = name.to_string();
            // If the table name matches a CTE name, use the __cte__:<name> prefix
            // so the planner can distinguish it from a real heap table.
            let resolved_name = if cte_names.contains(&raw_name.to_ascii_lowercase()) {
                format!("__cte__:{}", raw_name)
            } else {
                raw_name
            };
            from.push(TableRef {
                name: resolved_name,
                alias: alias.as_ref().map(|a| a.name.value.clone()),
            });
        } else if let TableFactor::Derived { subquery, alias, .. } = &table_with_joins.relation {
            // Recursively parse the derived table's inner SELECT and store it
            // as a CTE-like definition. Use a synthetic name with "__cte__:"
            // prefix so the logical planner resolves it via the CTE registry.
            let inner_plan = extract_select_params(subquery)?;
            let derived_name = format!("__derived__{}", derived_subqueries.len());
            derived_subqueries.push(CteDef {
                name: derived_name.clone(),
                query: Box::new(inner_plan),
                recursive_term: None,
                union_all: false,
            });
            cte_names.insert(derived_name.to_ascii_lowercase());
            from.push(TableRef {
                name: format!("__cte__:{}", derived_name),
                alias: alias.as_ref().map(|a| a.name.value.clone()),
            });
        } else if let TableFactor::TableFunction { .. } = &table_with_joins.relation {
            from.push(TableRef {
                name: "<function>".to_string(),
                alias: None,
            });
        }

        for join in &table_with_joins.joins {
            // Resolve the join relation — could be a table or a derived subquery
            let (join_relation_name, join_relation_alias) = match &join.relation {
                TableFactor::Table { name, alias, .. } => {
                    (name.to_string(), alias.as_ref().map(|a| a.name.value.clone()))
                }
                TableFactor::Derived { subquery, alias, .. } => {
                    // Recursively parse the derived table and add to CTE definitions
                    let inner_plan = extract_select_params(subquery)?;
                    let derived_name = format!("__derived__{}", derived_subqueries.len());
                    derived_subqueries.push(CteDef {
                        name: derived_name.clone(),
                        query: Box::new(inner_plan),
                        recursive_term: None,
                        union_all: false,
                    });
                    cte_names.insert(derived_name.to_ascii_lowercase());
                    (format!("__cte__:{}", derived_name), alias.as_ref().map(|a| a.name.value.clone()))
                }
                _ => (join.relation.to_string(), None),
            };

            let join_type = match &join.join_operator {
                JoinOperator::Inner(constraint)
                | JoinOperator::Left(constraint)
                | JoinOperator::LeftOuter(constraint)
                | JoinOperator::Right(constraint)
                | JoinOperator::RightOuter(constraint)
                | JoinOperator::FullOuter(constraint)
                | JoinOperator::Join(constraint)
                | JoinOperator::Semi(constraint)
                | JoinOperator::LeftSemi(constraint)
                | JoinOperator::RightSemi(constraint)
                | JoinOperator::Anti(constraint)
                | JoinOperator::LeftAnti(constraint)
                | JoinOperator::RightAnti(constraint) => {
                    // NATURAL JOIN is detected via JoinConstraint::Natural
                    match constraint {
                        JoinConstraint::Natural => JoinType::Natural,
                        _ => match join.join_operator {
                            JoinOperator::Inner(_) => JoinType::Inner,
                            JoinOperator::Left(_) | JoinOperator::LeftOuter(_) => JoinType::Left,
                            JoinOperator::Right(_) | JoinOperator::RightOuter(_) => JoinType::Right,
                            JoinOperator::FullOuter(_) => JoinType::Full,
                            JoinOperator::Join(_) => JoinType::Inner,
                            _ => JoinType::Inner,
                        },
                    }
                }
                JoinOperator::CrossJoin(_) => JoinType::Cross,
                _ => JoinType::Inner,
            };

            let condition = match &join.join_operator {
                JoinOperator::Inner(constraint)
                | JoinOperator::Left(constraint)
                | JoinOperator::LeftOuter(constraint)
                | JoinOperator::Right(constraint)
                | JoinOperator::RightOuter(constraint)
                | JoinOperator::FullOuter(constraint)
                | JoinOperator::Join(constraint)
                | JoinOperator::Semi(constraint)
                | JoinOperator::LeftSemi(constraint)
                | JoinOperator::RightSemi(constraint)
                | JoinOperator::Anti(constraint)
                | JoinOperator::LeftAnti(constraint)
                | JoinOperator::RightAnti(constraint) => match constraint {
                    JoinConstraint::On(expr) => Some(convert_predicate(expr)?),
                    // NATURAL JOIN has no explicit ON condition (handled by physical planner)
                    JoinConstraint::Natural => None,
                    _ => None,
                },
                _ => None,
            };

            joins.push(JoinClause {
                relation: TableRef {
                    name: join_relation_name,
                    alias: join_relation_alias,
                },
                join_type,
                condition,
            });
        }
    }

    // ── WHERE (selection) ─────────────────────────────────────────────────────
    let selection = select
        .selection
        .as_ref()
        .map(convert_predicate)
        .transpose()?;

    // ── GROUP BY ──────────────────────────────────────────────────────────────
    let group_by: Vec<ExprNode> = match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => exprs
            .iter()
            .map(convert_expr)
            .collect::<Result<Vec<_>, _>>()?,
        GroupByExpr::All(_) => Vec::new(),
    };

    // ── HAVING ────────────────────────────────────────────────────────────────
    let having = select
        .having
        .as_ref()
        .map(convert_predicate)
        .transpose()?;

    Ok(SelectPlan {
        ctes: derived_subqueries,
        projections,
        from,
        joins,
        selection,
        group_by,
        having,
        order_by: Vec::new(),
        limit: None,
        distinct: select.distinct.is_some(),
    })
}

// ── DELETE table extraction ────────────────────────────────────────────────────

/// Extract the primary table name from a `Delete` statement.
/// Standard SQL stores it in `from`, MySQL multi-delete uses `tables`.
fn extract_delete_table(delete: &Delete) -> String {
    // Standard SQL: DELETE FROM table WHERE ...
    let from_tables = match &delete.from {
        FromTable::WithFromKeyword(tables) => tables,
        FromTable::WithoutKeyword(tables) => tables,
    };
    if let Some(first) = from_tables.first()
        && let TableFactor::Table { name, .. } = &first.relation {
            return name.to_string();
        }
    // MySQL multi-delete: DELETE t1, t2 FROM ...
    if let Some(first) = delete.tables.first() {
        return first.to_string();
    }
    // Fallback
    "<unknown>".to_string()
}

// ── INSERT extraction ─────────────────────────────────────────────────────────

fn extract_insert_params(insert: &Insert) -> Result<InsertPlan, String> {
    let columns = insert
        .columns
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>();

    let mut source_select: Option<Box<SelectPlan>> = None;
    let mut values = Vec::new();

    if let Some(source) = &insert.source {
        match &*source.body {
            SetExpr::Values(v) => {
                values = v
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(convert_expr)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            }
            SetExpr::Select(_) | SetExpr::SetOperation { .. } => {
                // INSERT INTO ... SELECT: parse the source query as a SelectPlan
                let sel = extract_select_params(source)?;
                source_select = Some(Box::new(sel));
            }
            _ => {
                return Err(format!("Unsupported INSERT source: {:?}", source.body));
            }
        }
    }

    Ok(InsertPlan {
        table: insert.table.to_string(),
        columns,
        values,
        source_select,
    })
}

// ── CREATE TABLE extraction ───────────────────────────────────────────────────

fn extract_create_table_params(create: &CreateTable) -> Result<CreateTablePlan, String> {
    // Column-level `REFERENCES tbl(cols)` is valid SQL for foreign keys.
    // The engine's FK machinery consumes table-level constraint strings, so
    // inline definitions are normalised into that same form here (appended
    // to `constraints`) and dropped from the column's own option list —
    // previously they were stringified onto the column and silently
    // ignored, creating tables whose FKs never enforced.
    let mut constraints: Vec<TableConstraintDef> = create
        .constraints
        .iter()
        .map(|c| TableConstraintDef {
            definition: c.to_string(),
        })
        .collect();

    let columns = create
        .columns
        .iter()
        .map(|col| {
            let mut col_constraints = Vec::new();
            for opt in &col.options {
                match &opt.option {
                    ColumnOption::ForeignKey(fk) => {
                        // Renders as `FOREIGN KEY (col) REFERENCES tbl (refs)
                        // [ON DELETE …] [ON UPDATE …]` — exactly the string
                        // shape the engine's table-level FK parser accepts.
                        //
                        // For column-level definitions sqlparser leaves
                        // `columns` empty (the column is implied by
                        // position); the engine's FK checks need the child
                        // column name, so fill it in explicitly. Without
                        // this the synthesized constraint has an empty
                        // column list and rejects EVERY row.
                        let mut fk = fk.clone();
                        if fk.columns.is_empty() {
                            fk.columns = vec![col.name.clone()];
                        }
                        constraints.push(TableConstraintDef {
                            definition: fk.to_string(),
                        });
                    }
                    other => col_constraints.push(other.to_string()),
                }
            }
            Ok(ColumnDef {
                name: col.name.to_string(),
                data_type: col.data_type.to_string(),
                constraints: col_constraints,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(CreateTablePlan {
        table: create.name.to_string(),
        if_not_exists: create.if_not_exists,
        columns,
        constraints,
    })
}

// ── sqlparser → rook-ast conversion helpers ───────────────────────────────────

/// Convert a sqlparser `Expr` into `rook_ast::ExprNode`.
fn convert_expr(expr: &Expr) -> Result<ExprNode, String> {
    match expr {
        Expr::Identifier(ident) => Ok(ExprNode::Column(ident.value.clone())),
        Expr::CompoundIdentifier(idents) => {
            let parts: Vec<String> = idents.iter().map(|i| i.value.clone()).collect();
            Ok(ExprNode::Compound(parts))
        }
        Expr::Value(v) => {
            let constant = convert_value(&v.value)?;
            Ok(ExprNode::Constant(constant))
        }
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::Plus => Ok(ExprNode::Binary {
                left: Box::new(convert_expr(left)?),
                op: ArithOp::Add,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Minus => Ok(ExprNode::Binary {
                left: Box::new(convert_expr(left)?),
                op: ArithOp::Sub,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Multiply => Ok(ExprNode::Binary {
                left: Box::new(convert_expr(left)?),
                op: ArithOp::Mul,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Divide => Ok(ExprNode::Binary {
                left: Box::new(convert_expr(left)?),
                op: ArithOp::Div,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::StringConcat => Ok(ExprNode::Function {
                name: "CONCAT".to_string(),
                args: vec![
                    FunctionArg::Expr(Box::new(convert_expr(left)?)),
                    FunctionArg::Expr(Box::new(convert_expr(right)?)),
                ],
                distinct: false,
            }),
            BinaryOperator::Eq => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Eq,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::NotEq => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Ne,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Lt => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Lt,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::LtEq => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Le,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Gt => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Gt,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::GtEq => Ok(ExprNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Ge,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::And => Ok(ExprNode::Logical {
                left: Box::new(convert_expr(left)?),
                op: BinaryOp::And,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Or => Ok(ExprNode::Logical {
                left: Box::new(convert_expr(left)?),
                op: BinaryOp::Or,
                right: Box::new(convert_expr(right)?),
            }),
            _ => Err(format!("Unsupported expression: {:?}", expr)),
        },
        Expr::Nested(inner) => convert_expr(inner),
        Expr::Cast { expr: cast_expr, data_type, .. } => {
            let inner = convert_expr(cast_expr)?;
            Ok(ExprNode::Cast {
                expr: Box::new(inner),
                data_type: data_type.to_string(),
            })
        }
        Expr::Subquery(subquery) => {
            let select_plan = extract_select_params(subquery)?;
            Ok(ExprNode::ScalarSubquery(rook_ast::SubqueryInfo {
                select: Box::new(select_plan),
            }))
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            use sqlparser::ast::FunctionArguments;
            let mut args = Vec::new();
            match &func.args {                        FunctionArguments::List(arg_list) => {
                    for arg in &arg_list.args {
                        match arg {
                            sqlparser::ast::FunctionArg::Unnamed(ua) => match ua {
                                sqlparser::ast::FunctionArgExpr::Wildcard => {
                                    args.push(FunctionArg::Star);
                                }
                                sqlparser::ast::FunctionArgExpr::Expr(inner) => {
                                    args.push(FunctionArg::Expr(Box::new(convert_expr(inner)?)));
                                }
                                _ => {
                                    return Err(format!(
                                        "Unsupported function argument type: {:?}", ua
                                    ));
                                }
                            },
                            sqlparser::ast::FunctionArg::ExprNamed { .. } => {
                                return Err("ExprNamed function arguments not supported".to_string());
                            }
                            sqlparser::ast::FunctionArg::Named { .. } => {
                                return Err("Named function arguments not supported".to_string());
                            }
                        }
                    }
                }
                // CURRENT_DATE, CURRENT_TIME etc. without parens produce None
                FunctionArguments::None => {
                    // Zero-argument function: no args
                }
                _ => {
                    return Err(format!(
                        "Unsupported function arguments: {:?}", func.args
                    ));
                }
            }
            let distinct = match &func.args {
                FunctionArguments::List(arg_list) => {
                    matches!(arg_list.duplicate_treatment, Some(DuplicateTreatment::Distinct))
                }
                _ => false,
            };
            Ok(ExprNode::Function {
                name,
                args,
                distinct,
            })
        }
        |        Expr::Case {
            case_token: _,
            end_token: _,
            operand: _,
            conditions,
            else_result,
        } => {
            let mut when_then_pairs = Vec::new();
            for when in conditions {
                let cond_node = convert_expr(&when.condition)?;
                let res_node = convert_expr(&when.result)?;
                when_then_pairs.push((Box::new(cond_node), Box::new(res_node)));
            }
            let else_node = match else_result {
                Some(expr) => Some(Box::new(convert_expr(expr)?)),
                None => None,
            };
            Ok(ExprNode::Case {
                when_then_pairs,
                else_result: else_node,
            })
        }
        Expr::Extract { field, syntax: _, expr } => {
            // EXTRACT(YEAR FROM date_col) → Function("EXTRACT", ["YEAR", expr])
            let part_str = format!("{}", field);
            let part_expr = ExprNode::Constant(ConstantValue::Text(part_str.to_uppercase()));
            let value_expr = convert_expr(expr)?;
            Ok(ExprNode::Function {
                name: "EXTRACT".to_string(),
                args: vec![
                    FunctionArg::Expr(Box::new(part_expr)),
                    FunctionArg::Expr(Box::new(value_expr)),
                ],
                distinct: false,
            })
        }
        Expr::Position { expr: search_expr, r#in: target_expr } => {
            // POSITION('x' IN name) → Function("POSITION", [search, target])
            // arg[0] = substring to find, arg[1] = string to search in
            let search = convert_expr(search_expr)?;
            let target = convert_expr(target_expr)?;
            Ok(ExprNode::Function {
                name: "POSITION".to_string(),
                args: vec![
                    FunctionArg::Expr(Box::new(search)),
                    FunctionArg::Expr(Box::new(target)),
                ],
                distinct: false,
            })
        }
        Expr::Trim { expr, trim_where: _, trim_what: _, trim_characters: _ } => {
            // TRIM(name) → Function("TRIM", [Column("name")])
            let inner = convert_expr(expr)?;
            Ok(ExprNode::Function {
                name: "TRIM".to_string(),
                args: vec![FunctionArg::Expr(Box::new(inner))],
                distinct: false,
            })
        }
        Expr::Substring { expr, substring_from, substring_for, special: _, shorthand: _ } => {
            // SUBSTRING(name FROM 1 FOR 3) → Function("SUBSTRING", [col, start, len])
            let inner = convert_expr(expr)?;
            let mut args = vec![FunctionArg::Expr(Box::new(inner))];
            if let Some(from) = substring_from {
                args.push(FunctionArg::Expr(Box::new(convert_expr(from)?)));
            }
            if let Some(for_len) = substring_for {
                args.push(FunctionArg::Expr(Box::new(convert_expr(for_len)?)));
            }
            Ok(ExprNode::Function {
                name: "SUBSTRING".to_string(),
                args,
                distinct: false,
            })
        }        Expr::UnaryOp { op: UnaryOperator::Minus, expr } => {
            // -1 → 0 - 1
            let inner = convert_expr(expr)?;
            Ok(ExprNode::Binary {
                left: Box::new(ExprNode::Constant(ConstantValue::Int(0))),
                op: ArithOp::Sub,
                right: Box::new(inner),
            })
        }
        Expr::UnaryOp { op: UnaryOperator::Not, expr } => {
            let inner = convert_expr(expr)?;
            Ok(ExprNode::Not(Box::new(inner)))
        }
        Expr::IsNull(inner) => {
            let inner = convert_expr(inner)?;
            Ok(ExprNode::IsNull(Box::new(inner)))
        }
        Expr::IsNotNull(inner) => {
            let inner = convert_expr(inner)?;
            Ok(ExprNode::IsNotNull(Box::new(inner)))
        }
        // FLOOR(val) and CEIL(val) / CEILING(val) — sqlparser parses as special
        // Expr variants instead of function calls.
        //
        // When `field` is Some(DateTimeField), this is temporal truncation:
        //   FLOOR(date TO MONTH) → FLOOR(date, 'MONTH')
        //   CEIL(date TO YEAR)   → CEIL(date, 'YEAR')
        // When `field` is None, it's numeric floor/ceiling:
        //   FLOOR(3.7) → 3
        Expr::Floor { expr: floor_expr, field } => {
            let inner = convert_expr(floor_expr)?;
            let mut args = vec![FunctionArg::Expr(Box::new(inner))];
            // sqlparser 0.61.0 CeilFloorKind has two variants:
            //   DateTimeField(DateTimeField) — temporal truncation (TO unit)
            //   Scale(Value) — numeric precision
            // When no TO unit/scale is specified, field is DateTimeField(NoDateTime).
            match field {
                CeilFloorKind::DateTimeField(dtf) => match dtf {
                    DateTimeField::NoDateTime => {
                        // Numeric floor: no second argument
                    }
                    _ => {
                        // Temporal truncation: pass the field as second arg
                        args.push(FunctionArg::Expr(Box::new(
                            ExprNode::Constant(ConstantValue::Text(
                                format!("{}", dtf).to_uppercase()
                            ))
                        )));
                    }
                },
                CeilFloorKind::Scale(_val) => {
                    // Numeric scale: not yet supported in executor, skip
                }
            }
            Ok(ExprNode::Function {
                name: "FLOOR".to_string(),
                args,
                distinct: false,
            })
        }
        Expr::Ceil { expr: ceil_expr, field } => {
            let inner = convert_expr(ceil_expr)?;
            let mut args = vec![FunctionArg::Expr(Box::new(inner))];
            // sqlparser 0.61.0 CeilFloorKind has two variants:
            //   DateTimeField(DateTimeField) — temporal truncation (TO unit)
            //   Scale(Value) — numeric precision
            // When no TO unit/scale is specified, field is DateTimeField(NoDateTime).
            match field {
                CeilFloorKind::DateTimeField(dtf) => match dtf {
                    DateTimeField::NoDateTime => {
                        // Numeric ceil: no second argument
                    }
                    _ => {
                        // Temporal truncation: pass the field as second arg
                        args.push(FunctionArg::Expr(Box::new(
                            ExprNode::Constant(ConstantValue::Text(
                                format!("{}", dtf).to_uppercase()
                            ))
                        )));
                    }
                },
                CeilFloorKind::Scale(_val) => {
                    // Numeric scale: not yet supported in executor, skip
                }
            }
            // sqlparser may produce either "CEIL" or "CEILING" — use "CEIL"
            // (the Volcano evaluator handles both "CEIL" and "CEILING")
            Ok(ExprNode::Function {
                name: "CEIL".to_string(),
                args,
                distinct: false,
            })
        }
        _ => Err(format!("Unsupported expression: {:?}", expr)),
    }
}

/// Convert a sqlparser `Value` inner variant into `rook_ast::ConstantValue`.
fn convert_value(value: &Value) -> Result<ConstantValue, String> {
    match value {
        Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(ConstantValue::Int(i))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(ConstantValue::Float(f))
            } else {
                Err(format!("Unsupported numeric literal: {}", n))
            }
        }
        Value::SingleQuotedString(s) => Ok(ConstantValue::Text(s.clone())),
        Value::DoubleQuotedString(s) => Ok(ConstantValue::Text(s.clone())),
        Value::Null => Ok(ConstantValue::Null),
        Value::Boolean(b) => Ok(ConstantValue::Boolean(*b)),
        _ => Err(format!("Unsupported value: {:?}", value)),
    }
}

/// Convert a sqlparser `Expr` (which may be a predicate context) into `rook_ast::PredicateNode`.
fn convert_predicate(expr: &Expr) -> Result<PredicateNode, String> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => Ok(PredicateNode::BinaryOp {
                left: Box::new(convert_predicate(left)?),
                op: BinaryOp::And,
                right: Box::new(convert_predicate(right)?),
            }),
            BinaryOperator::Or => Ok(PredicateNode::BinaryOp {
                left: Box::new(convert_predicate(left)?),
                op: BinaryOp::Or,
                right: Box::new(convert_predicate(right)?),
            }),
            BinaryOperator::Eq => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Eq,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::NotEq => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Ne,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Lt => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Lt,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::LtEq => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Le,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::Gt => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Gt,
                right: Box::new(convert_expr(right)?),
            }),
            BinaryOperator::GtEq => Ok(PredicateNode::Compare {
                left: Box::new(convert_expr(left)?),
                op: ComparisonOp::Ge,
                right: Box::new(convert_expr(right)?),
            }),
            _ => Err(format!(
                "Unsupported predicate operator: {:?}",
                Expr::BinaryOp {
                    left: left.clone(),
                    op: op.clone(),
                    right: right.clone()
                }
            )),
        },
        Expr::UnaryOp { op: UnaryOperator::Not, expr } => {
            Ok(PredicateNode::Not(Box::new(convert_predicate(expr)?)))
        }
        Expr::IsNull(expr) => Ok(PredicateNode::IsNull(Box::new(convert_expr(expr)?))),
        Expr::IsNotNull(expr) => Ok(PredicateNode::IsNotNull(Box::new(convert_expr(expr)?))),
        Expr::Between {
            expr: bet_expr,
            negated,
            low,
            high,
        } => {
            let between = PredicateNode::Between {
                expr: Box::new(convert_expr(bet_expr)?),
                low: Box::new(convert_expr(low)?),
                high: Box::new(convert_expr(high)?),
            };
            if *negated {
                Ok(PredicateNode::Not(Box::new(between)))
            } else {
                Ok(between)
            }
        }
        Expr::InList {
            expr: in_expr,
            list,
            negated,
        } => {
            let items: Vec<ExprNode> = list
                .iter()
                .map(convert_expr)
                .collect::<Result<Vec<_>, _>>()?;
            let in_pred = PredicateNode::InList {
                expr: Box::new(convert_expr(in_expr)?),
                list: items,
            };
            if *negated {
                Ok(PredicateNode::Not(Box::new(in_pred)))
            } else {
                Ok(in_pred)
            }
        }
        Expr::Like {
            negated,
            expr: like_expr,
            pattern,
            escape_char,
            ..
        } => {
            let pattern_text = match pattern.as_ref() {
                Expr::Value(v) => match &v.value {
                    Value::SingleQuotedString(s) => s.clone(),
                    _ => return Err(format!("Invalid LIKE pattern: {:?}", pattern)),
                },
                _ => return Err(format!("Invalid LIKE pattern: {:?}", pattern)),
            };
            let like = PredicateNode::Like {
                expr: Box::new(convert_expr(like_expr)?),
                pattern: pattern_text,
                // escape_char is Option<Value> in sqlparser 0.61.0 — unwrap the actual char
                escape_char: escape_char.as_ref().and_then(|v| match v {
                    Value::SingleQuotedString(s) => s.chars().next(),
                    Value::DoubleQuotedString(s) => s.chars().next(),
                    _ => None,
                }),
            };
            if *negated {
                Ok(PredicateNode::Not(Box::new(like)))
            } else {
                Ok(like)
            }
        }
        Expr::IsDistinctFrom(left, right) => {
            Ok(PredicateNode::IsDistinctFrom {
                left: Box::new(convert_expr(left)?),
                right: Box::new(convert_expr(right)?),
            })
        }
        Expr::IsTrue(expr) => {
            // Unwrap Nested to handle `(x > 5) IS TRUE`
            let inner = match expr.as_ref() {
                Expr::Nested(nested) => nested.as_ref(),
                other => other,
            };
            // If the inner expression is a comparison (e.g. `x > 5 IS TRUE`),
            // use the comparison directly as a predicate — comparisons already
            // produce boolean results. `x > 5 IS TRUE` is equivalent to `x > 5`
            // in WHERE context (UNKNOWN IS TRUE → false, same as WHERE rejecting UNKNOWN).
            if matches!(inner, Expr::BinaryOp { op: BinaryOperator::Eq | BinaryOperator::NotEq
                | BinaryOperator::Lt | BinaryOperator::LtEq | BinaryOperator::Gt | BinaryOperator::GtEq, .. }) {
                return convert_predicate(inner);
            }
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::True,
                negated: false,
            })
        }
        Expr::IsNotTrue(expr) => {
            let inner = match expr.as_ref() {
                Expr::Nested(nested) => nested.as_ref(),
                other => other,
            };
            if matches!(inner, Expr::BinaryOp { op: BinaryOperator::Eq | BinaryOperator::NotEq
                | BinaryOperator::Lt | BinaryOperator::LtEq | BinaryOperator::Gt | BinaryOperator::GtEq, .. }) {
                return Ok(PredicateNode::Not(Box::new(convert_predicate(inner)?)));
            }
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::True,
                negated: true,
            })
        }
        Expr::IsFalse(expr) => {
            let inner = match expr.as_ref() {
                Expr::Nested(nested) => nested.as_ref(),
                other => other,
            };
            if matches!(inner, Expr::BinaryOp { op: BinaryOperator::Eq | BinaryOperator::NotEq
                | BinaryOperator::Lt | BinaryOperator::LtEq | BinaryOperator::Gt | BinaryOperator::GtEq, .. }) {
                return Ok(PredicateNode::Not(Box::new(convert_predicate(inner)?)));
            }
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::False,
                negated: false,
            })
        }
        Expr::IsNotFalse(expr) => {
            let inner = match expr.as_ref() {
                Expr::Nested(nested) => nested.as_ref(),
                other => other,
            };
            if matches!(inner, Expr::BinaryOp { op: BinaryOperator::Eq | BinaryOperator::NotEq
                | BinaryOperator::Lt | BinaryOperator::LtEq | BinaryOperator::Gt | BinaryOperator::GtEq, .. }) {
                return convert_predicate(inner);
            }
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::False,
                negated: true,
            })
        }
        Expr::IsUnknown(expr) => {
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::Unknown,
                negated: false,
            })
        }
        Expr::IsNotUnknown(expr) => {
            Ok(PredicateNode::IsBoolean {
                expr: Box::new(convert_expr(expr)?),
                test: BooleanTest::Unknown,
                negated: true,
            })
        }
        Expr::Nested(inner) => convert_predicate(inner),
        Expr::Exists { subquery, negated } => {
            let select_plan = extract_select_params(subquery)?;
            let exists = PredicateNode::Exists(rook_ast::SubqueryInfo {
                select: Box::new(select_plan),
            });
            if *negated {
                Ok(PredicateNode::Not(Box::new(exists)))
            } else {
                Ok(exists)
            }
        }
        Expr::InSubquery {
            expr: in_expr,
            subquery,
            negated,
        } => {
            let select_plan = extract_select_params(subquery)?;
            Ok(PredicateNode::InSubquery {
                expr: Box::new(convert_expr(in_expr)?),
                subquery: rook_ast::SubqueryInfo {
                    select: Box::new(select_plan),
                },
                negated: *negated,
            })
        }
        Expr::Subquery(_)
        | Expr::Function(_)
        | Expr::Case { .. } => {
            Err(format!("Unsupported predicate expression: {:?}", expr))
        }
        _ => Err(format!("Unsupported predicate expression: {:?}", expr)),
    }
}

/// Convert a sqlparser `SelectItem` into `rook_ast::SelectExpr`.
fn convert_select_item(item: &SelectItem) -> Result<SelectExpr, String> {
    match item {
        SelectItem::UnnamedExpr(Expr::Wildcard(_)) => Ok(SelectExpr::Wildcard),
        SelectItem::UnnamedExpr(expr) => Ok(SelectExpr::UnnamedExpr(convert_expr(expr)?)),
        SelectItem::ExprWithAlias { expr, alias } => Ok(SelectExpr::ExprWithAlias {
            expr: convert_expr(expr)?,
            alias: alias.value.clone(),
        }),
        SelectItem::QualifiedWildcard(kind, _) => {
            let prefix = match kind {
                SelectItemQualifiedWildcardKind::ObjectName(obj) => obj.to_string(),
                SelectItemQualifiedWildcardKind::Expr(expr) => expr.to_string(),
            };
            Ok(SelectExpr::QualifiedWildcard(prefix))
        }
        SelectItem::Wildcard(_) => Ok(SelectExpr::Wildcard),
    }
}
