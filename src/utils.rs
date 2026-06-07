use sqlparser::ast::*;

use crate::models::{
    Category, ColumnParam, CreateDatabaseParams, CreateTableParams, DeleteParams,
    InsertParams, Params, QuerySummary, SelectParams, ShowDatabasesParams, ShowTablesParams,
    StatementType, TableConstraintParam, UnknownParams, UpdateParams, UseDatabaseParams,
};

/* ---------------- HELPERS ---------------- */

fn expr_to_simple_string(expr: &Expr) -> String {
    match expr {
        Expr::Value(v) => match &v.value {
            Value::SingleQuotedString(s) => s.clone(),
            Value::DoubleQuotedString(s) => s.clone(),
            Value::Number(n, _) => n.clone(),
            Value::Boolean(b) => b.to_string(),
            Value::Null => "NULL".to_string(),
            _ => expr.to_string(),
        },
        _ => expr.to_string(),
    }
}

fn extract_column_constraints(options: &[ColumnOptionDef]) -> Vec<String> {
    options.iter().map(|opt| opt.option.to_string()).collect()
}

/* ---------------- CLASSIFICATION ---------------- */

pub fn classify_statement(stmt: &Statement) -> (Category, StatementType) {
    match stmt {
        Statement::Query(_) => (Category::DQL, StatementType::Select),
        Statement::Insert(_) => (Category::DML, StatementType::Insert),
        Statement::Update { .. } => (Category::DML, StatementType::Update),
        Statement::Delete { .. } => (Category::DML, StatementType::Delete),
        Statement::CreateTable(_) => (Category::DDL, StatementType::CreateTable),
        Statement::CreateDatabase { .. } => (Category::DDL, StatementType::CreateDatabase),
        Statement::ShowTables { .. } => (Category::DQL, StatementType::ShowTables),
        Statement::ShowDatabases { .. } => (Category::DQL, StatementType::ShowDatabases),
        Statement::Use(_) => (Category::DDL, StatementType::USEDatabase),
        _ => (Category::UNKNOWN, StatementType::Unknown),
    }
}

/* ---------------- SELECT PARAM EXTRACTION ---------------- */

pub fn extract_select_params(select: &Select) -> SelectParams {
    let mut tables = Vec::new();
    let mut columns = Vec::new();
    let mut joins = Vec::new();
    let mut filters = Vec::new();

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
                columns.push(id.value.clone());
            }
            SelectItem::UnnamedExpr(expr) => {
                columns.push(expr.to_string());
            }
            SelectItem::ExprWithAlias { alias, .. } => {
                columns.push(alias.value.clone());
            }
            SelectItem::Wildcard(_) => {
                columns.push("*".to_string());
            }
            SelectItem::QualifiedWildcard(name, _) => {
                columns.push(format!("{name}.*"));
            }
        }
    }

    for table in &select.from {
        if let TableFactor::Table { name, .. } = &table.relation {
            tables.push(name.to_string());
        }

        for join in &table.joins {
            if let TableFactor::Table { name, .. } = &join.relation {
                joins.push(name.to_string());
            }
        }
    }

    if let Some(selection) = &select.selection {
        filters.push(selection.to_string());
    }

    SelectParams {
        tables,
        columns,
        joins,
        filters,
    }
}

/* ---------------- INSERT PARAM EXTRACTION ---------------- */

pub fn extract_insert_params(insert: &Insert) -> InsertParams {
    let columns = insert
        .columns
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>();

    let values = if let Some(source) = &insert.source {
        match &*source.body {
            SetExpr::Values(v) => v
                .rows
                .iter()
                .map(|row| row.iter().map(expr_to_simple_string).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let row_count = values.len();

    InsertParams {
        table: insert.table.to_string(),
        columns,
        values,
        row_count,
    }
}

/* ---------------- UPDATE PARAM EXTRACTION ---------------- */

pub fn extract_update_params(update_stmt: &sqlparser::ast::Update) -> UpdateParams {
    let table_name = if let TableFactor::Table { name, .. } = &update_stmt.table.relation {
        name.to_string()
    } else {
        String::new()
    };

    let assignment_strs = update_stmt
        .assignments
        .iter()
        .map(|a| {
            let col_name = match &a.target {
                sqlparser::ast::AssignmentTarget::ColumnName(name) => name.to_string(),
                sqlparser::ast::AssignmentTarget::Tuple(_) => "tuple".to_string(),
            };
            format!("{} = {}", col_name, a.value)
        })
        .collect::<Vec<_>>();

    let filters = update_stmt
        .selection
        .as_ref()
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    UpdateParams {
        table: table_name,
        assignments: assignment_strs,
        filters,
    }
}

/* ---------------- DELETE PARAM EXTRACTION ---------------- */

pub fn extract_delete_params(delete_stmt: &sqlparser::ast::Delete) -> DeleteParams {
    let mut table_name = String::new();
    let tables = match &delete_stmt.from {
        sqlparser::ast::FromTable::WithFromKeyword(t) | sqlparser::ast::FromTable::WithoutKeyword(t) => t,
    };
    if let Some(table) = tables.first() {
        if let sqlparser::ast::TableFactor::Table { name, .. } = &table.relation {
            table_name = name.to_string();
        }
    }
    if table_name.is_empty() {
        table_name = delete_stmt
            .tables
            .first()
            .map(|t| t.to_string())
            .unwrap_or_default();
    }

    let filters = delete_stmt
        .selection
        .as_ref()
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    DeleteParams {
        table: table_name,
        filters,
    }
}

/* ---------------- CREATE TABLE PARAM EXTRACTION ---------------- */

pub fn extract_create_table_params(create: &CreateTable) -> CreateTableParams {
    let columns = create
        .columns
        .iter()
        .map(|col| ColumnParam {
            name: col.name.to_string(),
            data_type: col.data_type.to_string(),
            constraints: extract_column_constraints(&col.options),
        })
        .collect::<Vec<_>>();

    let constraints = create
        .constraints
        .iter()
        .map(|c| TableConstraintParam {
            definition: c.to_string(),
        })
        .collect::<Vec<_>>();

    CreateTableParams {
        table: create.name.to_string(),
        if_not_exists: create.if_not_exists,
        columns,
        constraints,
    }
}

/* ---------------- BUILD SUMMARY ---------------- */

pub fn build_query_summary(stmt: &Statement) -> QuerySummary {
    let (category, stmt_type) = classify_statement(stmt);

    let params = match stmt {
        Statement::Query(query) => {
            if let SetExpr::Select(select) = &*query.body {
                Params::Select(extract_select_params(select))
            } else {
                Params::Unknown(UnknownParams)
            }
        }

        Statement::Insert(insert) => Params::Insert(extract_insert_params(insert)),

        Statement::Update(update) => Params::Update(extract_update_params(update)),

        Statement::Delete(delete) => Params::Delete(extract_delete_params(delete)),

        Statement::CreateTable(create) => Params::CreateTable(extract_create_table_params(create)),

        Statement::CreateDatabase {
            db_name,
            if_not_exists,
            ..
        } => Params::CreateDatabase(CreateDatabaseParams {
            database: db_name.to_string(),
            if_not_exists: *if_not_exists,
        }),

        Statement::ShowTables { .. } => Params::ShowTables(ShowTablesParams),

        Statement::ShowDatabases { .. } => Params::ShowDatabases(ShowDatabasesParams),

        Statement::Use(use_stmt) => Params::UseDatabase(UseDatabaseParams {
            database: use_stmt.to_string(),
        }),

        _ => Params::Unknown(UnknownParams),
    };

    QuerySummary {
        category,
        stmt_type,
        params,
    }
}
