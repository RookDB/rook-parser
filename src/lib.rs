use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

mod models;
mod utils;

use crate::utils::build_query_summary;

pub fn parse_sql(sql: &str) -> Result<String, String> {
    let dialect = GenericDialect {};

    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;

    let statement = statements.first().ok_or("No SQL statement found")?;

    let summary = build_query_summary(statement);

    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// Parse a WHERE clause by wrapping it in a SELECT statement
/// Takes a WHERE clause string like "id > 0" and returns the expression
pub fn parse_where_clause(where_clause: &str) -> Result<String, String> {
    if where_clause.is_empty() {
        return Ok("".to_string());
    }

    let dialect = GenericDialect {};

    // Wrap the WHERE clause in a SELECT statement so it can be parsed
    let full_sql = format!("SELECT 1 FROM dummy WHERE {}", where_clause);

    let statements = Parser::parse_sql(&dialect, &full_sql)
        .map_err(|e| format!("Failed to parse WHERE clause: {}", e))?;

    let statement = statements.first().ok_or("No SQL statement found")?;

    // Extract the WHERE clause expression as a string
    if let sqlparser::ast::Statement::Query(query) = statement {
        if let sqlparser::ast::SetExpr::Select(select) = query.body.as_ref() {
            if let Some(selection) = &select.selection {
                return Ok(selection.to_string());
            }
        }
    }

    Err("Could not extract WHERE clause from parsed SQL".to_string())
}
