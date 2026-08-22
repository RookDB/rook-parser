use rook_ast::QueryPlan;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

mod utils;

use crate::utils::build_query_plan;

/// Strip MySQL-style INDEX constraints from CREATE TABLE statements.
///
/// Syntaxes handled:
///   INDEX(col)
///   INDEX index_name (col)
///   INDEX USING BTREE (col)
///   INDEX index_name USING BTREE (col)
///   KEY(col)
///   KEY index_name (col)
///   FULLTEXT INDEX (col)
///   SPATIAL INDEX (col)
///
/// These are replaced with inline comments so the sqlparser `GenericDialect`
/// can parse the rest of the statement without errors.
/// Check if `s` starts with `keyword` followed by a valid constraint boundary
/// (space, opening paren, or end-of-string). This ensures we only match
/// MySQL constraints like `INDEX(col)`, `KEY name (col)`, or `INDEX` alone,
/// without matching column names like `KEY_ID`, `INDEXER`, or `FULLTEXT_MATCH`.
fn is_keyword_at_boundary(s: &str, keyword: &str) -> bool {
    if !s.starts_with(keyword) {
        return false;
    }
    if s.len() == keyword.len() {
        return true; // bare keyword at end of part (e.g. just "INDEX")
    }
    match s.as_bytes()[keyword.len()] {
        b' ' | b'(' => true,   // INDEX(col), INDEX name(col), KEY(col), KEY name(col)
        _ => false,             // e.g. KEY_ID, INDEXER — not constraint keywords
    }
}

fn preprocess_sql(sql: &str) -> String {
    // Use a simple regex-like approach: remove INDEX/KEY/FULLTEXT/SPATIAL
    // table-level constraints before the closing paren of CREATE TABLE.
    let sql = sql.trim();
    if !sql.to_uppercase().starts_with("CREATE TABLE") {
        return sql.to_string();
    }

    // Find the column/constraint section between the first '(' and the last ')'
    let open_paren = match sql.find('(') {
        Some(p) => p,
        None => return sql.to_string(),
    };

    let body = &sql[open_paren..];
    // We need to find the matching closing paren accounting for nested parens
    let close_idx = find_outer_paren_end(body);
    if close_idx == body.len() {
        return sql.to_string(); // no matching close paren found
    }

    // The body is everything between (inclusive) open_paren and the matching close paren
    let inner = &body[1..close_idx]; // strip surrounding parens

    // Split by commas at the top level (not inside nested parens)
    let parts = split_by_top_level_comma(inner);
    let mut stripped: Vec<String> = Vec::new();
    let filtered: Vec<&str> = parts.iter().copied()
        .filter(|part| {
            let trimmed = part.trim();
            let tu = trimmed.to_uppercase();
            let is_index_keyword = is_keyword_at_boundary(&tu, "INDEX")
                || is_keyword_at_boundary(&tu, "KEY")
                || is_keyword_at_boundary(&tu, "FULLTEXT")
                || is_keyword_at_boundary(&tu, "SPATIAL");
            if is_index_keyword {
                stripped.push(trimmed.to_string());
            }
            !is_index_keyword
        })
        .collect();

    if !stripped.is_empty() {
        eprintln!(
            "Warning: MySQL constraint(s) ignored: [{}]. Use CREATE INDEX to create indexes explicitly.",
            stripped.join(", ")
        );
    }

    let new_inner = filtered.join(", ");
    let before = &sql[..open_paren];
    // Find what comes after the body's close paren
    let after = &sql[open_paren + close_idx + 1..];

    format!("{}({}){}", before.trim_end(), new_inner, after)
}

/// Find the position of the matching close-paren for a string that starts with '('.
fn find_outer_paren_end(s: &str) -> usize {
    if !s.starts_with('(') { return s.len(); }
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Split a string by commas at the top level (not inside parentheses or quotes).
fn split_by_top_level_comma(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Parse a SQL string and return a typed `QueryPlan`.
///
/// This replaces the earlier `parse_sql` that returned a JSON string —
/// now callers get a strongly-typed AST directly.
pub fn parse_sql(sql: &str) -> Result<QueryPlan, String> {
    let dialect = GenericDialect {};

    // Pre-process to remove MySQL INDEX/KEY constraints that GenericDialect
    // doesn't understand. Users can create indexes explicitly with CREATE INDEX.
    let processed = preprocess_sql(sql);

    let statements = Parser::parse_sql(&dialect, &processed).map_err(|e| e.to_string())?;

    let statement = statements
        .first()
        .ok_or("No SQL statement found")?;

    build_query_plan(statement)
}

/// Parse a SQL string and return the JSON representation (legacy compatibility).
///
/// Useful for the standalone CLI debug tool and any scripts that consume JSON.
/// Prefer `parse_sql` for production use.
pub fn parse_sql_json(sql: &str) -> Result<String, String> {
    let plan = parse_sql(sql)?;
    serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_from_subquery() {
        // Verify that a FROM subquery is parsed correctly.
        // The parser should extract the inner SelectPlan and store it
        // as a CTE-like entry with a generated name (__derived__0).
        let sql = "SELECT * FROM (SELECT 1 AS x) AS t";
        match parse_sql(sql) {
            Ok(QueryPlan::Select(plan)) => {
                // The from field should reference the derived subquery via __cte__:__derived__0
                assert_eq!(plan.from.len(), 1, "Should have one FROM entry");
                let from_ref = &plan.from[0];
                assert!(from_ref.name.starts_with("__cte__:__derived__"),
                    "FROM should reference derived subquery via __cte__:__derived__ prefix, got: {}",
                    from_ref.name);
                assert_eq!(from_ref.alias, Some("t".to_string()), "Alias should be 't'");

                // The ctes list should contain the derived subquery definition
                assert!(!plan.ctes.is_empty(), "Should have at least one CTE (derived subquery)");
                let derived_cte = plan.ctes.iter().find(|c| c.name.starts_with("__derived__"));
                assert!(derived_cte.is_some(), "Should have a __derived__ CTE entry");
                let cte = derived_cte.unwrap();
                assert_eq!(cte.recursive_term, None, "Derived subquery should not have recursive term");
                assert!(!cte.union_all, "Derived subquery should not be a UNION");
            }
            Ok(other) => panic!("Expected Select plan, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_from_subquery_with_join() {
        // Verify that a JOIN with a derived subquery on one side is parsed.
        let sql = "SELECT * FROM t1 JOIN (SELECT 1 AS x) AS t2 ON t1.id = t2.x";
        match parse_sql(sql) {
            Ok(QueryPlan::Select(plan)) => {
                // The from should have t1
                assert_eq!(plan.from.len(), 1);
                assert_eq!(plan.from[0].name, "t1");

                // The joins should have the derived subquery reference
                assert_eq!(plan.joins.len(), 1, "Should have one JOIN");
                let join_ref = &plan.joins[0].relation;
                assert!(join_ref.name.starts_with("__cte__:__derived__"),
                    "JOIN should reference derived subquery, got: {}", join_ref.name);
                assert_eq!(join_ref.alias, Some("t2".to_string()));

                // The ctes should contain the derived subquery
                assert!(!plan.ctes.is_empty());
                assert!(plan.ctes.iter().any(|c| c.name.starts_with("__derived__")));
            }
            Ok(other) => panic!("Expected Select plan, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_simple_select_still_works() {
        // Verify that basic SELECT without subqueries still parses normally
        let sql = "SELECT id, name FROM users";
        match parse_sql(sql) {
            Ok(QueryPlan::Select(plan)) => {
                assert!(plan.ctes.is_empty(), "No CTEs expected for simple SELECT");
                assert_eq!(plan.from.len(), 1);
                assert_eq!(plan.from[0].name, "users");
            }
            Ok(other) => panic!("Expected Select plan, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_count_distinct() {
        let sql = "SELECT COUNT(DISTINCT id) FROM users";
        match parse_sql(sql) {
            Ok(QueryPlan::Select(plan)) => {
                assert_eq!(plan.projections.len(), 1);
                match &plan.projections[0] {
                    rook_ast::SelectExpr::UnnamedExpr(rook_ast::ExprNode::Function { name, args, distinct }) => {
                        assert_eq!(name, "COUNT");
                        assert_eq!(args.len(), 1);
                        assert!(*distinct, "distinct flag should be true for COUNT(DISTINCT id)");
                    }
                    other => panic!("Expected function projection, got: {:?}", other),
                }
            }
            Ok(other) => panic!("Expected Select plan, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }
}
