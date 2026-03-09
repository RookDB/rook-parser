use serde::Serialize;
use sqlparser::ast::*;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::io::{self, Write};

#[derive(Debug, Serialize)]
enum Category {
    DDL,
    DML,
    DQL,
    UNKNOWN,
}

#[derive(Debug, Serialize)]
enum StatementType {
    Select,
    Insert,
    CreateTable,
    CreateDatabase,
    ShowTables,
    ShowDatabases,
    Unknown,
}

#[derive(Debug, Serialize)]
struct QuerySummary {
    category: Category,

    #[serde(rename = "type")]
    stmt_type: StatementType,

    params: Params,
}

/* ---------------- PARAM STRUCTS ---------------- */

#[derive(Debug, Serialize)]
struct SelectParams {
    tables: Vec<String>,
    columns: Vec<String>,
    joins: Vec<String>,
    filters: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InsertParams {
    table: String,
    columns: Vec<String>,
    values: Vec<Vec<String>>,
    row_count: usize,
}

#[derive(Debug, Serialize)]
struct CreateDatabaseParams {
    database: String,
    if_not_exists: bool,
}

#[derive(Debug, Serialize)]
struct ColumnParam {
    name: String,
    data_type: String,
    constraints: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TableConstraintParam {
    definition: String,
}

#[derive(Debug, Serialize)]
struct CreateTableParams {
    table: String,
    if_not_exists: bool,
    columns: Vec<ColumnParam>,
    constraints: Vec<TableConstraintParam>,
}

#[derive(Debug, Serialize)]
struct ShowTablesParams;

#[derive(Debug, Serialize)]
struct ShowDatabasesParams;

#[derive(Debug, Serialize)]
struct UnknownParams;

/* ---------------- PARAM ENUM ---------------- */

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Params {
    Select(SelectParams),
    Insert(InsertParams),
    CreateTable(CreateTableParams),
    CreateDatabase(CreateDatabaseParams),
    ShowTables(ShowTablesParams),
    ShowDatabases(ShowDatabasesParams),
    Unknown(UnknownParams),
}

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

fn classify_statement(stmt: &Statement) -> (Category, StatementType) {
    match stmt {
        Statement::Query(_) => (Category::DQL, StatementType::Select),
        Statement::Insert(_) => (Category::DML, StatementType::Insert),
        Statement::CreateTable(_) => (Category::DDL, StatementType::CreateTable),
        Statement::CreateDatabase { .. } => (Category::DDL, StatementType::CreateDatabase),
        Statement::ShowTables { .. } => (Category::DQL, StatementType::ShowTables),
        Statement::ShowDatabases { .. } => (Category::DQL, StatementType::ShowDatabases),
        _ => (Category::UNKNOWN, StatementType::Unknown),
    }
}

/* ---------------- SELECT PARAM EXTRACTION ---------------- */

fn extract_select_params(select: &Select) -> SelectParams {
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

fn extract_insert_params(insert: &Insert) -> InsertParams {
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

/* ---------------- CREATE TABLE PARAM EXTRACTION ---------------- */

fn extract_create_table_params(create: &CreateTable) -> CreateTableParams {
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

fn build_query_summary(stmt: &Statement) -> QuerySummary {
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

        Statement::CreateTable(create) => {
            Params::CreateTable(extract_create_table_params(create))
        }

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

        _ => Params::Unknown(UnknownParams),
    };

    QuerySummary {
        category,
        stmt_type,
        params,
    }
}

/* ---------------- MAIN ---------------- */

fn main() {
    let dialect = GenericDialect {};

    loop {
        let mut query = String::new();

        print!("Enter SQL query (or 'exit'): ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut query).unwrap();

        let query = query.trim();

        if query.eq_ignore_ascii_case("exit") {
            break;
        }

        if query.is_empty() {
            continue;
        }

        match Parser::parse_sql(&dialect, query) {
            Ok(statements) => {
                for statement in statements {
                    println!("\nAST Debug:");
                    println!("{:#?}", statement);

                    let summary = build_query_summary(&statement);

                    println!("\nCustom JSON:");
                    let json = serde_json::to_string_pretty(&summary).unwrap();
                    println!("{}", json);
                }
            }
            Err(e) => {
                println!("Parse error: {}", e);
            }
        }
    }
}