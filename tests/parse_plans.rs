//! Parser contract tests for the typed `QueryPlan` wire format.
//!
//! These tests pin down how representative SQL statements map onto
//! `rook_ast` types. They run without any storage engine involvement.

use rook_ast::*;
use rook_parser::parse_sql;

#[test]
fn select_star_with_where() {
    let plan = parse_sql("SELECT * FROM users WHERE age > 25").unwrap();
    match plan {
        QueryPlan::Select(sp) => {
            assert_eq!(sp.from.len(), 1);
            assert_eq!(sp.from[0].name, "users");
            assert!(matches!(sp.projections[0], SelectExpr::Wildcard));
            assert!(sp.selection.is_some());
        }
        other => panic!("expected Select plan, got {:?}", other),
    }
}

#[test]
fn insert_values_become_constants() {
    let plan = parse_sql("INSERT INTO users VALUES (1, 'Alice', 30.5)").unwrap();
    match plan {
        QueryPlan::Insert(p) => {
            assert_eq!(p.table, "users");
            assert_eq!(p.values.len(), 1);
            assert_eq!(p.values[0][0], ExprNode::Constant(ConstantValue::Int(1)));
            assert_eq!(
                p.values[0][1],
                ExprNode::Constant(ConstantValue::Text("Alice".into()))
            );
            assert_eq!(
                p.values[0][2],
                ExprNode::Constant(ConstantValue::Float(30.5))
            );
        }
        other => panic!("expected Insert plan, got {:?}", other),
    }
}

#[test]
fn update_carries_assignments_and_predicate() {
    let plan = parse_sql("UPDATE users SET age = 31 WHERE id = 1").unwrap();
    match plan {
        QueryPlan::Update(p) => {
            assert_eq!(p.table, "users");
            assert_eq!(p.assignments[0].column, "age");
            assert!(p.selection.is_some());
        }
        other => panic!("expected Update plan, got {:?}", other),
    }
}

#[test]
fn delete_without_where_has_no_selection() {
    let plan = parse_sql("DELETE FROM users").unwrap();
    match plan {
        QueryPlan::Delete(p) => {
            assert_eq!(p.table, "users");
            assert!(p.selection.is_none());
        }
        other => panic!("expected Delete plan, got {:?}", other),
    }
}

#[test]
fn create_table_keeps_column_constraints() {
    let plan =
        parse_sql("CREATE TABLE t (id INT NOT NULL, name VARCHAR(50) UNIQUE, score DOUBLE PRECISION DEFAULT 0)")
            .unwrap();
    match plan {
        QueryPlan::CreateTable(p) => {
            assert_eq!(p.table, "t");
            assert_eq!(p.columns.len(), 3);
            let id_constraints = p.columns[0].constraints.join(" ");
            assert!(id_constraints.contains("NOT NULL"));
            let name_constraints = p.columns[1].constraints.join(" ");
            assert!(name_constraints.to_uppercase().contains("UNIQUE"));
        }
        other => panic!("expected CreateTable plan, got {:?}", other),
    }
}

#[test]
fn future_statement_types_are_typed_not_json() {
    // These statements must produce typed plans even though execution
    // arrives in later stages — the parser is already SQL-99 aware.
    for sql in [
        "CREATE INDEX idx ON t(col)",
        "DROP INDEX idx ON t",
        "DROP TABLE t",
        "TRUNCATE TABLE t",
        "ALTER TABLE t ADD COLUMN c INT",
        "CREATE VIEW v AS SELECT * FROM t",
        "DROP VIEW v",
        "CREATE TABLE t2 AS SELECT * FROM t",
        "SELECT a FROM t UNION SELECT b FROM u",
        "DROP DATABASE dbx",
    ] {
        let plan = parse_sql(sql).unwrap_or_else(|e| panic!("{} failed: {}", sql, e));
        assert!(!matches!(plan, QueryPlan::Unknown(_)), "{} parsed as Unknown", sql);
    }
}

#[test]
fn syntax_errors_are_reported() {
    assert!(parse_sql("SELEC * FROM t").is_err());
}
