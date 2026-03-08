use std::fmt;
use sqlparser::dialect::GenericDialect;
use sqlparser::tokenizer::Tokenizer;

pub mod ast;
pub mod parser;
use crate::ast::Statement;
use crate::parser::SyntacticParser;


/// Represents a lexical token in the SQL query
#[derive(Debug, Clone, PartialEq)]
pub struct LexicalToken {
    pub token_type: TokenType,
    pub value: String,
    pub position: usize,
}

/// Types of tokens in SQL
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Select,
    From,
    Where,
    And,
    Or,
    Not,
    In,
    Like,
    Between,
    Join,
    Inner,
    Left,
    Right,
    Outer,
    On,
    Group,
    By,
    Having,
    Order,
    Asc,
    Desc,
    Limit,
    Offset,
    Insert,
    Into,
    Values,
    Update,
    Set,
    Delete,
    Create,
    Table,
    Drop,
    Alter,
    Add,
    Column,
    As,
    Distinct,
    All,

    // Operators
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // Delimiters
    LeftParen,
    RightParen,
    Comma,
    Dot,
    Semicolon,

    // Literals
    Number(String),
    String(String),
    Identifier(String),

    // Special
    Whitespace,
    Unknown,
    Eof,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Number(n) => write!(f, "Number({})", n),
            TokenType::String(s) => write!(f, "String({})", s),
            TokenType::Identifier(id) => write!(f, "Identifier({})", id),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Lexical Parser for SQL queries
pub struct LexicalParser {
    input: String,
    tokens: Vec<LexicalToken>,
}

impl LexicalParser {
    /// Creates a new lexical parser
    pub fn new(input: String) -> Self {
        LexicalParser {
            input,
            tokens: Vec::new(),
        }
    }

    /// Tokenizes the input SQL string using sqlparser's tokenizer
    pub fn tokenize(&mut self) -> Result<Vec<LexicalToken>, String> {
        let dialect = GenericDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, &self.input);

        match tokenizer.tokenize() {
            Ok(tokens) => {
                self.tokens = tokens
                    .iter()
                    .enumerate()
                    .map(|(idx, token)| {
                        let token_str = format!("{}", token);
                        LexicalToken {
                            token_type: self.classify_token(&token_str),
                            value: token_str,
                            position: idx,
                        }
                    })
                    .collect();
                Ok(self.tokens.clone())
            }
            Err(e) => Err(format!("Tokenization error: {}", e)),
        }
    }

    /// Classifies a token string into a TokenType
    fn classify_token(&self, token: &str) -> TokenType {
        let lower = token.to_uppercase();

        match lower.as_str() {
            "SELECT" => TokenType::Select,
            "FROM" => TokenType::From,
            "WHERE" => TokenType::Where,
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "NOT" => TokenType::Not,
            "IN" => TokenType::In,
            "LIKE" => TokenType::Like,
            "BETWEEN" => TokenType::Between,
            "JOIN" => TokenType::Join,
            "INNER" => TokenType::Inner,
            "LEFT" => TokenType::Left,
            "RIGHT" => TokenType::Right,
            "OUTER" => TokenType::Outer,
            "ON" => TokenType::On,
            "GROUP" => TokenType::Group,
            "BY" => TokenType::By,
            "HAVING" => TokenType::Having,
            "ORDER" => TokenType::Order,
            "ASC" => TokenType::Asc,
            "DESC" => TokenType::Desc,
            "LIMIT" => TokenType::Limit,
            "OFFSET" => TokenType::Offset,
            "INSERT" => TokenType::Insert,
            "INTO" => TokenType::Into,
            "VALUES" => TokenType::Values,
            "UPDATE" => TokenType::Update,
            "SET" => TokenType::Set,
            "DELETE" => TokenType::Delete,
            "CREATE" => TokenType::Create,
            "TABLE" => TokenType::Table,
            "DROP" => TokenType::Drop,
            "ALTER" => TokenType::Alter,
            "ADD" => TokenType::Add,
            "COLUMN" => TokenType::Column,
            "AS" => TokenType::As,
            "DISTINCT" => TokenType::Distinct,
            "ALL" => TokenType::All,
            "=" => TokenType::Equal,
            "!=" | "<>" => TokenType::NotEqual,
            "<" => TokenType::LessThan,
            "<=" => TokenType::LessThanOrEqual,
            ">" => TokenType::GreaterThan,
            ">=" => TokenType::GreaterThanOrEqual,
            "+" => TokenType::Plus,
            "-" => TokenType::Minus,
            "*" => TokenType::Star,
            "/" => TokenType::Slash,
            "%" => TokenType::Percent,
            "(" => TokenType::LeftParen,
            ")" => TokenType::RightParen,
            "," => TokenType::Comma,
            "." => TokenType::Dot,
            ";" => TokenType::Semicolon,
            "EOF" => TokenType::Eof,
            _ => {
                if token.parse::<f64>().is_ok() {
                    TokenType::Number(token.to_string())
                } else if (token.starts_with('\'') && token.ends_with('\''))
                    || (token.starts_with('"') && token.ends_with('"'))
                {
                    TokenType::String(token[1..token.len() - 1].to_string())
                } else if token.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    TokenType::Identifier(token.to_string())
                } else {
                    TokenType::Unknown
                }
            }
        }
    }

    /// Returns the tokens found by the parser
    pub fn get_tokens(&self) -> &[LexicalToken] {
        &self.tokens
    }

    /// Returns non-whitespace tokens
    pub fn get_filtered_tokens(&self) -> Vec<&LexicalToken> {
        self.tokens
            .iter()
            .filter(|t| t.token_type != TokenType::Whitespace)
            .collect()
    }

    /// Prints a formatted token list
    pub fn print_tokens(&self) {
        println!("\n{}", "=".repeat(50));
        println!("            LEXICAL TOKENS");
        println!("{}", "=".repeat(50));

        let filtered = self.get_filtered_tokens();

        for (idx, token) in filtered.iter().enumerate() {
            println!(
                "[{:3}] {:20} | Value: '{}'",
                idx,
                format!("{:?}", token.token_type),
                token.value
            );
        }

        println!("{}", "=".repeat(50));
        println!("Total tokens: {}\n", filtered.len());
    }
}

pub fn parse_sql(sql: &str) -> Result<Statement, String> {
    let mut lexer = LexicalParser::new(sql.to_string());
    lexer.tokenize()?;

    let tokens = lexer.get_tokens().to_vec();
    let mut parser = SyntacticParser::new(tokens);

    parser.parse().map_err(|e| e.to_string())
}