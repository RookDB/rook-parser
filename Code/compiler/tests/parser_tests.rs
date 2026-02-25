use lexical_parser::ast::*;
use lexical_parser::parser::{ParseError, SyntacticParser};
use lexical_parser::LexicalParser;

fn parse_statement(sql: &str) -> Statement {
    let mut lexer = LexicalParser::new(sql.to_string());
    let tokens = lexer.tokenize().expect("tokenization should succeed");
    let mut parser = SyntacticParser::new(tokens);
    parser.parse().expect("parsing should succeed")
}

fn parse_result(sql: &str) -> Result<Statement, ParseError> {
    let mut lexer = LexicalParser::new(sql.to_string());
    let tokens = lexer.tokenize().expect("tokenization should succeed");
    let mut parser = SyntacticParser::new(tokens);
    parser.parse()
}

#[test]
fn parses_basic_select_star() {
    let statement = parse_statement("SELECT * FROM users;");
    match statement {
        Statement::Select(select_stmt) => {
            assert!(!select_stmt.distinct);
            assert!(select_stmt.where_clause.is_none());
            assert_eq!(select_stmt.select_list.len(), 1);
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_distinct() {
    let statement = parse_statement("SELECT DISTINCT name FROM users;");
    match statement {
        Statement::Select(select_stmt) => assert!(select_stmt.distinct),
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_with_expression_alias() {
    let statement = parse_statement("SELECT price * qty AS total FROM sales;");
    match statement {
        Statement::Select(select_stmt) => match &select_stmt.select_list[0] {
            SelectItem::Column { alias, .. } => assert_eq!(alias.as_deref(), Some("total")),
            _ => panic!("expected column expression"),
        },
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_with_function_call() {
    let statement = parse_statement("SELECT COUNT(id) FROM users;");
    match statement {
        Statement::Select(select_stmt) => match &select_stmt.select_list[0] {
            SelectItem::Column { expr, .. } => match expr {
                Expression::FunctionCall { name, args } => {
                    assert_eq!(name, "COUNT");
                    assert_eq!(args.len(), 1);
                }
                _ => panic!("expected function call expression"),
            },
            _ => panic!("expected column expression"),
        },
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_with_group_having_order_limit_offset() {
    let statement = parse_statement(
        "SELECT dept, COUNT(id) FROM employees GROUP BY dept HAVING COUNT(id) > 2 ORDER BY dept DESC LIMIT 5 OFFSET 2;",
    );
    match statement {
        Statement::Select(select_stmt) => {
            assert!(select_stmt.group_by_clause.is_some());
            assert!(select_stmt.having_clause.is_some());
            assert!(select_stmt.order_by_clause.is_some());
            assert!(select_stmt.limit_clause.is_some());
            let limit = select_stmt.limit_clause.expect("limit expected");
            assert_eq!(limit.limit, 5);
            assert_eq!(limit.offset, Some(2));
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_with_implicit_table_alias() {
    let statement = parse_statement("SELECT u.id FROM users u;");
    match statement {
        Statement::Select(select_stmt) => {
            let from = select_stmt.from_clause.expect("from clause expected");
            assert_eq!(from.table.alias.as_deref(), Some("u"));
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_select_with_parenthesized_expression() {
    let statement = parse_statement("SELECT (price + tax) FROM invoice;");
    match statement {
        Statement::Select(select_stmt) => match &select_stmt.select_list[0] {
            SelectItem::Column { expr, .. } => {
                assert!(matches!(expr, Expression::Parenthesized(_)));
            }
            _ => panic!("expected column expression"),
        },
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_where_or_and_precedence() {
    let statement = parse_statement("SELECT * FROM users WHERE a = 1 OR b = 2 AND c = 3;");
    match statement {
        Statement::Select(select_stmt) => {
            let where_clause = select_stmt.where_clause.expect("where clause expected");
            match where_clause {
                Expression::BinaryOp { op, right, .. } => {
                    assert_eq!(op, BinaryOperator::Or);
                    assert!(matches!(*right, Expression::BinaryOp { op: BinaryOperator::And, .. }));
                }
                _ => panic!("expected binary expression"),
            }
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_where_arithmetic_precedence() {
    let statement = parse_statement("SELECT * FROM calc WHERE 1 + 2 * 3 = 7;");
    match statement {
        Statement::Select(select_stmt) => {
            let where_clause = select_stmt.where_clause.expect("where clause expected");
            match where_clause {
                Expression::BinaryOp { left, op, .. } => {
                    assert_eq!(op, BinaryOperator::Equal);
                    match *left {
                        Expression::BinaryOp {
                            op: left_op,
                            right,
                            ..
                        } => {
                            assert_eq!(left_op, BinaryOperator::Plus);
                            assert!(matches!(
                                *right,
                                Expression::BinaryOp {
                                    op: BinaryOperator::Multiply,
                                    ..
                                }
                            ));
                        }
                        _ => panic!("expected additive expression on left side"),
                    }
                }
                _ => panic!("expected binary expression"),
            }
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_full_join() {
    let statement = parse_statement("SELECT u.id FROM users u FULL JOIN orders o ON u.id = o.user_id;");

    match statement {
        Statement::Select(select_stmt) => {
            let first_join = select_stmt
                .from_clause
                .expect("from clause expected")
                .joins
                .first()
                .cloned()
                .expect("join expected");
            assert_eq!(first_join.join_type, JoinType::Full);
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_left_outer_join() {
    let statement = parse_statement("SELECT * FROM users LEFT OUTER JOIN orders ON users.id = orders.user_id;");
    match statement {
        Statement::Select(select_stmt) => {
            let first_join = select_stmt
                .from_clause
                .expect("from clause expected")
                .joins
                .first()
                .cloned()
                .expect("join expected");
            assert_eq!(first_join.join_type, JoinType::Left);
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_right_join() {
    let statement = parse_statement("SELECT * FROM users RIGHT JOIN orders ON users.id = orders.user_id;");
    match statement {
        Statement::Select(select_stmt) => {
            let first_join = select_stmt
                .from_clause
                .expect("from clause expected")
                .joins
                .first()
                .cloned()
                .expect("join expected");
            assert_eq!(first_join.join_type, JoinType::Right);
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_plain_join_as_inner_join() {
    let statement = parse_statement("SELECT * FROM users JOIN orders ON users.id = orders.user_id;");
    match statement {
        Statement::Select(select_stmt) => {
            let first_join = select_stmt
                .from_clause
                .expect("from clause expected")
                .joins
                .first()
                .cloned()
                .expect("join expected");
            assert_eq!(first_join.join_type, JoinType::Inner);
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_cross_join() {
    let statement = parse_statement("SELECT * FROM users CROSS JOIN orders;");

    match statement {
        Statement::Select(select_stmt) => {
            let first_join = select_stmt
                .from_clause
                .expect("from clause expected")
                .joins
                .first()
                .cloned()
                .expect("join expected");
            assert_eq!(first_join.join_type, JoinType::Cross);
            assert!(first_join.on_condition.is_none());
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_is_not_equal_comparison() {
    let statement = parse_statement("SELECT * FROM users WHERE status != 'inactive';");
    match statement {
        Statement::Select(select_stmt) => {
            let where_clause = select_stmt.where_clause.expect("where clause expected");
            assert!(matches!(
                where_clause,
                Expression::BinaryOp {
                    op: BinaryOperator::NotEqual,
                    ..
                }
            ));
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_is_null_expression() {
    let statement = parse_statement("SELECT * FROM users WHERE deleted_at IS NULL;");

    match statement {
        Statement::Select(select_stmt) => {
            let where_clause = select_stmt.where_clause.expect("where clause expected");
            match where_clause {
                Expression::BinaryOp { op, right, .. } => {
                    assert_eq!(op, BinaryOperator::Is);
                    assert_eq!(*right, Expression::Null);
                }
                _ => panic!("expected binary expression in WHERE"),
            }
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_in_like_between_operators() {
    let in_stmt = parse_statement("SELECT * FROM users WHERE role IN ('admin');");
    let like_stmt = parse_statement("SELECT * FROM users WHERE name LIKE 'A%';");
    let between_stmt = parse_statement("SELECT * FROM users WHERE age BETWEEN 18;");

    match in_stmt {
        Statement::Select(select_stmt) => {
            assert!(matches!(
                select_stmt.where_clause,
                Some(Expression::BinaryOp {
                    op: BinaryOperator::In,
                    ..
                })
            ));
        }
        _ => panic!("expected SELECT statement"),
    }

    match like_stmt {
        Statement::Select(select_stmt) => {
            assert!(matches!(
                select_stmt.where_clause,
                Some(Expression::BinaryOp {
                    op: BinaryOperator::Like,
                    ..
                })
            ));
        }
        _ => panic!("expected SELECT statement"),
    }

    match between_stmt {
        Statement::Select(select_stmt) => {
            assert!(matches!(
                select_stmt.where_clause,
                Some(Expression::BinaryOp {
                    op: BinaryOperator::Between,
                    ..
                })
            ));
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_unary_plus_minus() {
    let statement = parse_statement("SELECT -amount, +tax FROM ledger;");

    match statement {
        Statement::Select(select_stmt) => {
            assert_eq!(select_stmt.select_list.len(), 2);
            match &select_stmt.select_list[0] {
                SelectItem::Column { expr, .. } => match expr {
                    Expression::UnaryOp { op, .. } => assert_eq!(*op, UnaryOperator::Minus),
                    _ => panic!("expected unary minus"),
                },
                _ => panic!("expected column expression"),
            }

            match &select_stmt.select_list[1] {
                SelectItem::Column { expr, .. } => match expr {
                    Expression::UnaryOp { op, .. } => assert_eq!(*op, UnaryOperator::Plus),
                    _ => panic!("expected unary plus"),
                },
                _ => panic!("expected column expression"),
            }
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_unary_not_expression() {
    let statement = parse_statement("SELECT * FROM users WHERE NOT active;");
    match statement {
        Statement::Select(select_stmt) => {
            let where_clause = select_stmt.where_clause.expect("where clause expected");
            assert!(matches!(
                where_clause,
                Expression::UnaryOp {
                    op: UnaryOperator::Not,
                    ..
                }
            ));
        }
        _ => panic!("expected SELECT statement"),
    }
}

#[test]
fn parses_insert_single_row_without_columns() {
    let statement = parse_statement("INSERT INTO users VALUES (1, 'John');");
    match statement {
        Statement::Insert(insert_stmt) => {
            assert_eq!(insert_stmt.table, "users");
            assert!(insert_stmt.columns.is_none());
            assert_eq!(insert_stmt.values.len(), 1);
        }
        _ => panic!("expected INSERT statement"),
    }
}

#[test]
fn parses_insert_multi_row_with_columns() {
    let statement = parse_statement("INSERT INTO users (id, name) VALUES (1, 'A'), (2, 'B');");
    match statement {
        Statement::Insert(insert_stmt) => {
            assert_eq!(
                insert_stmt.columns.as_ref().expect("columns expected").len(),
                2
            );
            assert_eq!(insert_stmt.values.len(), 2);
        }
        _ => panic!("expected INSERT statement"),
    }
}

#[test]
fn parses_update_without_where() {
    let statement = parse_statement("UPDATE users SET name = 'Jane';");
    match statement {
        Statement::Update(update_stmt) => {
            assert_eq!(update_stmt.assignments.len(), 1);
            assert!(update_stmt.where_clause.is_none());
        }
        _ => panic!("expected UPDATE statement"),
    }
}

#[test]
fn parses_update_with_multiple_assignments_and_where() {
    let statement = parse_statement("UPDATE users SET name = 'Jane', age = 30 WHERE id = 1;");
    match statement {
        Statement::Update(update_stmt) => {
            assert_eq!(update_stmt.assignments.len(), 2);
            assert!(update_stmt.where_clause.is_some());
        }
        _ => panic!("expected UPDATE statement"),
    }
}

#[test]
fn parses_delete_without_where() {
    let statement = parse_statement("DELETE FROM users;");
    match statement {
        Statement::Delete(delete_stmt) => {
            assert_eq!(delete_stmt.table, "users");
            assert!(delete_stmt.where_clause.is_none());
        }
        _ => panic!("expected DELETE statement"),
    }
}

#[test]
fn parses_delete_with_where() {
    let statement = parse_statement("DELETE FROM users WHERE id = 99;");
    match statement {
        Statement::Delete(delete_stmt) => {
            assert_eq!(delete_stmt.table, "users");
            assert!(delete_stmt.where_clause.is_some());
        }
        _ => panic!("expected DELETE statement"),
    }
}

#[test]
fn parses_create_table_with_constraints() {
    let statement = parse_statement(
        "CREATE TABLE users (id INT PRIMARY KEY, email TEXT UNIQUE, age INT NOT NULL);",
    );
    match statement {
        Statement::Create(create_stmt) => {
            assert_eq!(create_stmt.name, "users");
            assert_eq!(create_stmt.columns.len(), 3);
            assert_eq!(
                create_stmt.columns[0].constraints,
                vec!["PRIMARY".to_string(), "KEY".to_string()]
            );
            assert_eq!(
                create_stmt.columns[2].constraints,
                vec!["NOT".to_string(), "NULL".to_string()]
            );
        }
        _ => panic!("expected CREATE statement"),
    }
}

#[test]
fn parses_drop_table_statement() {
    let statement = parse_statement("DROP TABLE users;");
    match statement {
        Statement::Drop(drop_stmt) => {
            assert_eq!(drop_stmt.object_type.to_uppercase(), "TABLE");
            assert_eq!(drop_stmt.name, "users");
            assert!(!drop_stmt.if_exists);
        }
        _ => panic!("expected DROP statement"),
    }
}

#[test]
fn parses_drop_with_custom_object_type() {
    let statement = parse_statement("DROP VIEW active_users;");
    match statement {
        Statement::Drop(drop_stmt) => {
            assert_eq!(drop_stmt.object_type.to_uppercase(), "VIEW");
            assert_eq!(drop_stmt.name, "active_users");
        }
        _ => panic!("expected DROP statement"),
    }
}

#[test]
fn parses_alter_add_column() {
    let statement = parse_statement("ALTER TABLE users ADD COLUMN nickname TEXT;");
    match statement {
        Statement::Alter(alter_stmt) => match alter_stmt.action {
            AlterAction::Add(col) => {
                assert_eq!(col.name, "nickname");
                assert_eq!(col.data_type, "TEXT");
            }
            _ => panic!("expected add column action"),
        },
        _ => panic!("expected ALTER statement"),
    }
}

#[test]
fn parses_alter_drop_column() {
    let statement = parse_statement("ALTER TABLE users DROP COLUMN nickname;");
    match statement {
        Statement::Alter(alter_stmt) => match alter_stmt.action {
            AlterAction::Drop(name) => assert_eq!(name, "nickname"),
            _ => panic!("expected drop column action"),
        },
        _ => panic!("expected ALTER statement"),
    }
}

#[test]
fn parses_alter_rename_column() {
    let statement = parse_statement("ALTER TABLE users RENAME COLUMN fname TO first_name;");

    match statement {
        Statement::Alter(alter_stmt) => {
            assert_eq!(alter_stmt.table, "users");
            match alter_stmt.action {
                AlterAction::Rename { old_name, new_name } => {
                    assert_eq!(old_name, "fname");
                    assert_eq!(new_name, "first_name");
                }
                _ => panic!("expected rename alter action"),
            }
        }
        _ => panic!("expected ALTER statement"),
    }
}

#[test]
fn parses_alter_rename_without_column_keyword() {
    let statement = parse_statement("ALTER TABLE users RENAME fname TO first_name;");
    match statement {
        Statement::Alter(alter_stmt) => match alter_stmt.action {
            AlterAction::Rename { old_name, new_name } => {
                assert_eq!(old_name, "fname");
                assert_eq!(new_name, "first_name");
            }
            _ => panic!("expected rename action"),
        },
        _ => panic!("expected ALTER statement"),
    }
}

#[test]
fn fails_on_empty_input() {
    let result = parse_result("");
    assert!(result.is_err());
    let err = result.expect_err("error expected");
    assert!(err.message.contains("Empty input"));
}

#[test]
fn fails_on_unexpected_start_token() {
    let result = parse_result("WHERE id = 1;");
    assert!(result.is_err());
}

#[test]
fn fails_on_select_alias_without_identifier() {
    let result = parse_result("SELECT price AS FROM sales;");
    assert!(result.is_err());
    let err = result.expect_err("error expected");
    assert!(err.message.contains("Expected identifier after AS"));
}

#[test]
fn fails_on_insert_without_values() {
    let result = parse_result("INSERT INTO users;");
    assert!(result.is_err());
}

#[test]
fn fails_on_update_without_assignment_expression() {
    let result = parse_result("UPDATE users SET name = ;");
    assert!(result.is_err());
}

#[test]
fn fails_on_delete_without_table() {
    let result = parse_result("DELETE FROM ;");
    assert!(result.is_err());
}

#[test]
fn fails_on_create_missing_right_paren() {
    let result = parse_result("CREATE TABLE users (id INT, name TEXT;");
    assert!(result.is_err());
}

#[test]
fn fails_on_drop_without_name() {
    let result = parse_result("DROP TABLE ;");
    assert!(result.is_err());
}

#[test]
fn fails_on_alter_without_action() {
    let result = parse_result("ALTER TABLE users;");
    assert!(result.is_err());
}

#[test]
fn fails_on_alter_rename_missing_to() {
    let result = parse_result("ALTER TABLE users RENAME COLUMN old_name new_name;");
    assert!(result.is_err());
    let err = result.expect_err("error expected");
    assert!(err.message.contains("Expected TO"));
}

#[test]
fn fails_on_join_without_table() {
    let result = parse_result("SELECT * FROM users JOIN ON users.id = 1;");
    assert!(result.is_err());
}
