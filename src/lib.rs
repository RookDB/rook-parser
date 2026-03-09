use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

mod models;
mod utils;

use crate::models::QuerySummary;
use crate::utils::build_query_summary;

pub fn parse_sql(sql: &str) -> Result<Vec<QuerySummary>, String> {
    let dialect = GenericDialect {};

    let statements = Parser::parse_sql(&dialect, sql)
        .map_err(|e| e.to_string())?;

    let summaries = statements
        .iter()
        .map(build_query_summary)
        .collect::<Vec<_>>();

    Ok(summaries)
}