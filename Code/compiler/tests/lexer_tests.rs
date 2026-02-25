use lexical_parser::{LexicalParser, TokenType};

#[test]
fn test_simple_select() {
    let mut parser = LexicalParser::new("SELECT * FROM users;".to_string());
    let tokens = parser.tokenize();
    assert!(tokens.is_ok());
    assert!(!parser.get_tokens().is_empty());
}

#[test]
fn test_select_with_where() {
    let mut parser = LexicalParser::new("SELECT id, name FROM users WHERE age > 18;".to_string());
    let tokens = parser.tokenize();
    assert!(tokens.is_ok());
    let token_count = parser.get_filtered_tokens().len();
    assert!(token_count > 0);
}

#[test]
fn test_insert_statement() {
    let mut parser = LexicalParser::new("INSERT INTO users VALUES (1, 'John');".to_string());
    let tokens = parser.tokenize();
    assert!(tokens.is_ok());
}

#[test]
fn test_keyword_token_classification() {
    let mut parser = LexicalParser::new("SELECT name FROM users WHERE age >= 21;".to_string());
    parser.tokenize().expect("tokenization should succeed");
    let token_types: Vec<TokenType> = parser
        .get_filtered_tokens()
        .iter()
        .map(|t| t.token_type.clone())
        .collect();

    assert!(token_types.iter().any(|t| matches!(t, TokenType::Select)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::From)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Where)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::GreaterThanOrEqual)));
}

#[test]
fn test_literal_token_classification() {
    let mut parser = LexicalParser::new("SELECT 123, 'abc', user_name FROM t;".to_string());
    parser.tokenize().expect("tokenization should succeed");
    let token_types: Vec<TokenType> = parser
        .get_filtered_tokens()
        .iter()
        .map(|t| t.token_type.clone())
        .collect();

    assert!(token_types.iter().any(|t| matches!(t, TokenType::Number(n) if n == "123")));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::String(s) if s == "abc")));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Identifier(id) if id == "user_name")));
}

#[test]
fn test_operator_and_delimiter_classification() {
    let mut parser = LexicalParser::new("SELECT (a + b) * c / d % 2, x != y, p <= q, m <> n FROM t;".to_string());
    parser.tokenize().expect("tokenization should succeed");
    let token_types: Vec<TokenType> = parser
        .get_filtered_tokens()
        .iter()
        .map(|t| t.token_type.clone())
        .collect();

    assert!(token_types.iter().any(|t| matches!(t, TokenType::LeftParen)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::RightParen)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Plus)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Star)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Slash)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Percent)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::NotEqual)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::LessThanOrEqual)));
}

#[test]
fn test_join_related_keywords_classification() {
    let mut parser = LexicalParser::new(
        "SELECT * FROM u LEFT OUTER JOIN o ON u.id = o.uid ORDER BY u.id DESC LIMIT 5 OFFSET 1;"
            .to_string(),
    );
    parser.tokenize().expect("tokenization should succeed");
    let token_types: Vec<TokenType> = parser
        .get_filtered_tokens()
        .iter()
        .map(|t| t.token_type.clone())
        .collect();

    assert!(token_types.iter().any(|t| matches!(t, TokenType::Left)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Outer)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Join)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::On)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Order)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::By)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Desc)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Limit)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Offset)));
}

#[test]
fn test_ddl_dml_keywords_classification() {
    let mut parser = LexicalParser::new(
        "CREATE TABLE t (id INT); ALTER TABLE t ADD COLUMN name TEXT; DROP TABLE t; UPDATE t SET id = 2; DELETE FROM t;"
            .to_string(),
    );
    parser.tokenize().expect("tokenization should succeed");
    let token_types: Vec<TokenType> = parser
        .get_filtered_tokens()
        .iter()
        .map(|t| t.token_type.clone())
        .collect();

    assert!(token_types.iter().any(|t| matches!(t, TokenType::Create)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Table)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Alter)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Add)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Column)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Drop)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Update)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Set)));
    assert!(token_types.iter().any(|t| matches!(t, TokenType::Delete)));
}
