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
fn inline_column_references_become_table_level_fk() {
    // Column-level `REFERENCES tbl(col)` is valid SQL for foreign keys.
    // The parser must normalise it into the table-level FOREIGN KEY
    // constraint string the engine enforces — previously it was stringified
    // onto the column and silently dropped (orphans inserted freely).
    let plan = parse_sql(
        "CREATE TABLE c (id INT, pid INT REFERENCES p(id) ON DELETE CASCADE)",
    )
    .unwrap();
    match plan {
        QueryPlan::CreateTable(p) => {
            // The REFERENCES option must NOT remain on the column…
            let pid_constraints = p.columns[1].constraints.join(" ");
            assert!(
                !pid_constraints.to_uppercase().contains("REFERENCES"),
                "inline REFERENCES leaked onto the column: {}",
                pid_constraints
            );
            // …and must appear as a table-level FK naming BOTH columns.
            let fk = p
                .constraints
                .iter()
                .map(|c| c.definition.to_uppercase())
                .find(|d| d.starts_with("FOREIGN KEY"))
                .expect("inline REFERENCES did not produce a table-level FOREIGN KEY");
            assert!(fk.contains("PID"), "child column missing: {}", fk);
            assert!(fk.contains("REFERENCES P"), "parent table missing: {}", fk);
            assert!(fk.contains("(ID)"), "parent column missing: {}", fk);
            assert!(fk.contains("ON DELETE CASCADE"), "action missing: {}", fk);
        }
        other => panic!("expected CreateTable plan, got {:?}", other),
    }
}

#[test]
fn inline_references_fill_implied_child_column() {
    // sqlparser leaves ForeignKeyConstraint.columns empty for column-level
    // definitions (the column is implied). The synthesised table-level
    // string must name the child column explicitly — an empty list made
    // the engine reject every row.
    let plan =
        parse_sql("CREATE TABLE c (pid INT REFERENCES p(id))").unwrap();
    match plan {
        QueryPlan::CreateTable(p) => {
            let fk = p
                .constraints
                .iter()
                .map(|c| c.definition.to_uppercase())
                .find(|d| d.starts_with("FOREIGN KEY"))
                .expect("no FOREIGN KEY constraint produced");
            assert!(
                fk.contains("(PID)"),
                "implied child column not filled in: {}",
                fk
            );
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

// ── Multi-line statements ─────────────────────────────────────────────────────
//
// The REPL buffers lines until a statement terminator, so SELECTs routinely
// arrive with embedded newlines. Newlines inside a statement are plain
// whitespace to the tokenizer — every clause boundary below must therefore
// parse identically to its single-line form. These tests pin that behaviour
// (each case once failed with `Expected: end of statement` when newlines
// were stripped instead of preserved, e.g. "FROM emp" + "GROUP BY dept"
// concatenating into "empGROUP BY dept").

fn assert_parses(sql: &str) {
    if let Err(e) = parse_sql(sql) {
        panic!("multi-line statement failed to parse: {}\n--- SQL ---\n{}", e, sql);
    }
}

#[test]
fn multiline_select_at_every_clause_boundary() {
    assert_parses(
        "SELECT dept, COUNT(*)\nFROM emp\nGROUP BY dept\nHAVING COUNT(*) > 0\nORDER BY 2 DESC\nLIMIT 5",
    );
}

#[test]
fn multiline_where_after_from() {
    assert_parses("SELECT *\nFROM emp\nWHERE dept = 10");
}

#[test]
fn multiline_join_boundaries() {
    assert_parses(
        "SELECT e.name, d.name\nFROM emp e\nJOIN dept d ON e.dept = d.id\nWHERE d.name = 'eng'\nORDER BY e.name",
    );
}

#[test]
fn multiline_cte_and_derived_table() {
    assert_parses("WITH big AS (\nSELECT * FROM emp WHERE dept = 10)\nSELECT * FROM big");
    assert_parses("SELECT *\nFROM (\nSELECT id FROM emp\n) t");
}

#[test]
fn multiline_comments_and_strings() {
    // Line comments, block comments spanning lines, and string literals
    // containing newlines must not confuse statement handling.
    assert_parses(
        "SELECT name -- pick the name\nFROM emp /* inline\ncomment */\nWHERE name = 'multi\nline'",
    );
}

#[test]
fn multiline_insert_and_create_still_work() {
    assert_parses("INSERT INTO emp VALUES (\n1, 'ada', 10, 3500\n)");
    assert_parses("CREATE TABLE t (\n  id INT,\n  name VARCHAR(20)\n)");
}

#[test]
fn multiline_select_is_plan_equivalent_to_single_line() {
    let single = parse_sql("SELECT name, sal FROM emp WHERE dept = 10 ORDER BY sal LIMIT 3").unwrap();
    let multi = parse_sql(
        "SELECT name, sal\nFROM emp\nWHERE dept = 10\nORDER BY sal\nLIMIT 3",
    )
    .unwrap();
    assert_eq!(
        format!("{:?}", single),
        format!("{:?}", multi),
        "whitespace between clauses must not change the plan"
    );
}
