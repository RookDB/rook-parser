use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

pub fn parse_sql(sql: &str) -> Result<Vec<Statement>, String> {
    let dialect = GenericDialect {};
    Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())
}