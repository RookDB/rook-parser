/// Syntactic Parser - Builds an Abstract Syntax Tree (AST) from tokens
use crate::{LexicalToken, TokenType};
use crate::ast::*;

/// Parser errors
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse Error at position {}: {}", self.position, self.message)
    }
}

/// Syntactic Parser for SQL
pub struct SyntacticParser {
    tokens: Vec<LexicalToken>,
    current: usize,
}

impl SyntacticParser {
    /// Creates a new syntactic parser
    pub fn new(tokens: Vec<LexicalToken>) -> Self {
        SyntacticParser {
            tokens,
            current: 0,
        }
    }
    
    /// Parses tokens into a Statement
    pub fn parse(&mut self) -> Result<Statement, ParseError> {
        self.skip_whitespace();
        
        if self.current >= self.tokens.len() {
            return Err(ParseError {
                message: "Empty input".to_string(),
                position: 0,
            });
        }
        
        let token = &self.tokens[self.current];
        
        match &token.token_type {
            TokenType::Select => self.parse_select(),
            TokenType::Insert => self.parse_insert(),
            TokenType::Update => self.parse_update(),
            TokenType::Delete => self.parse_delete(),
            TokenType::Create => self.parse_create(),
            TokenType::Drop => self.parse_drop(),
            TokenType::Alter => self.parse_alter(),
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", token.token_type),
                position: self.current,
            }),
        }
    }
    
    /// Parse SELECT statement
    fn parse_select(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Select)?;
        
        // Check for DISTINCT
        self.skip_whitespace();
        let distinct = if self.check(&TokenType::Distinct) {
            self.advance();
            true
        } else {
            false
        };
        
        // Parse SELECT list
        self.skip_whitespace();
        let select_list = self.parse_select_list()?;
        
        // Parse FROM clause
        self.skip_whitespace();
        let from_clause = if self.check(&TokenType::From) {
            self.advance();
            Some(self.parse_from_clause()?)
        } else {
            None
        };
        
        // Parse WHERE clause
        self.skip_whitespace();
        let where_clause = if self.check(&TokenType::Where) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        // Parse GROUP BY clause
        self.skip_whitespace();
        let group_by_clause = if self.check(&TokenType::Group) {
            self.advance();
            self.expect(TokenType::By)?;
            Some(self.parse_expression_list()?)
        } else {
            None
        };
        
        // Parse HAVING clause
        self.skip_whitespace();
        let having_clause = if self.check(&TokenType::Having) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        // Parse ORDER BY clause
        self.skip_whitespace();
        let order_by_clause = if self.check(&TokenType::Order) {
            self.advance();
            self.skip_whitespace();
            self.expect(TokenType::By)?;
            Some(self.parse_order_by()?)
        } else {
            None
        };
        
        // Parse LIMIT clause
        self.skip_whitespace();
        let limit_clause = if self.check(&TokenType::Limit) {
            self.advance();
            Some(self.parse_limit()?)
        } else {
            None
        };
        
        Ok(Statement::Select(SelectStatement {
            distinct,
            select_list,
            from_clause,
            where_clause,
            group_by_clause,
            having_clause,
            order_by_clause,
            limit_clause,
        }))
    }
    
    /// Parse SELECT list
    fn parse_select_list(&mut self) -> Result<Vec<SelectItem>, ParseError> {
        let mut items = Vec::new();
        
        loop {
            self.skip_whitespace();
            
            // Check for *
            if self.check(&TokenType::Star) {
                self.advance();
                items.push(SelectItem::AllColumns);
            } else {
                let expr = self.parse_expression()?;
                
                // Check for alias
                self.skip_whitespace();
                let alias = if self.check(&TokenType::As) {
                    self.advance();
                    self.skip_whitespace();
                    if let TokenType::Identifier(name) = &self.peek().token_type {
                        let name = name.clone();
                        self.advance();
                        Some(name)
                    } else {
                        return Err(ParseError {
                            message: "Expected identifier after AS".to_string(),
                            position: self.current,
                        });
                    }
                } else {
                    None
                };
                
                items.push(SelectItem::Column { expr, alias });
            }
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance(); // consume comma
        }
        
        Ok(items)
    }
    
    /// Parse FROM clause
    fn parse_from_clause(&mut self) -> Result<FromClause, ParseError> {
        self.skip_whitespace();
        
        // Parse table name
        let table_name = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected table name in FROM clause".to_string(),
                position: self.current,
            });
        };
        
        // Check for alias
        self.skip_whitespace();
        let alias = if self.check(&TokenType::As) || self.is_alias_identifier_candidate() {
            if self.check(&TokenType::As) {
                self.advance();
            }
            self.skip_whitespace();
            if let TokenType::Identifier(a) = &self.peek().token_type {
                let a = a.clone();
                self.advance();
                Some(a)
            } else {
                None
            }
        } else {
            None
        };
        
        let table = TableReference {
            name: table_name,
            alias,
        };
        
        // Parse JOINs
        let mut joins = Vec::new();
        loop {
            self.skip_whitespace();
            
            if self.check(&TokenType::Inner) {
                self.advance();
                self.expect(TokenType::Join)?;
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Inner,
                    table,
                    on_condition,
                });
            } else if self.check(&TokenType::Left) {
                self.advance();
                self.skip_whitespace();
                let _ = self.check(&TokenType::Outer); // OUTER is optional
                if self.check(&TokenType::Outer) {
                    self.advance();
                }
                self.expect(TokenType::Join)?;
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Left,
                    table,
                    on_condition,
                });
            } else if self.check(&TokenType::Right) {
                self.advance();
                self.skip_whitespace();
                let _ = self.check(&TokenType::Outer);
                if self.check(&TokenType::Outer) {
                    self.advance();
                }
                self.expect(TokenType::Join)?;
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Right,
                    table,
                    on_condition,
                });
            } else if self.check(&TokenType::Join) {
                self.advance();
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Inner,
                    table,
                    on_condition,
                });
            } else if self.check_identifier_keyword("FULL") {
                self.advance();
                self.skip_whitespace();
                if self.check(&TokenType::Outer) {
                    self.advance();
                }
                self.expect(TokenType::Join)?;
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Full,
                    table,
                    on_condition,
                });
            } else if self.check_identifier_keyword("CROSS") {
                self.advance();
                self.expect(TokenType::Join)?;
                let (table, on_condition) = self.parse_join_target()?;
                joins.push(Join {
                    join_type: JoinType::Cross,
                    table,
                    on_condition,
                });
            } else {
                break;
            }
        }
        
        Ok(FromClause { table, joins })
    }
    
    /// Parse JOIN target (table and ON condition)
    fn parse_join_target(&mut self) -> Result<(TableReference, Option<Expression>), ParseError> {
        self.skip_whitespace();
        
        let table_name = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected table name in JOIN".to_string(),
                position: self.current,
            });
        };
        
        // Check for alias
        self.skip_whitespace();
        let alias = if self.check(&TokenType::As) {
            self.advance();
            self.skip_whitespace();
            if let TokenType::Identifier(a) = &self.peek().token_type {
                let a = a.clone();
                self.advance();
                Some(a)
            } else {
                None
            }
        } else {
            None
        };
        
        let table = TableReference {
            name: table_name,
            alias,
        };
        
        // Parse ON condition
        self.skip_whitespace();
        let on_condition = if self.check(&TokenType::On) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        Ok((table, on_condition))
    }
    
    /// Parse ORDER BY clause
    fn parse_order_by(&mut self) -> Result<Vec<OrderByItem>, ParseError> {
        let mut items = Vec::new();
        
        loop {
            self.skip_whitespace();
            let expr = self.parse_expression()?;
            
            self.skip_whitespace();
            let direction = if self.check(&TokenType::Asc) {
                self.advance();
                SortDirection::Asc
            } else if self.check(&TokenType::Desc) {
                self.advance();
                SortDirection::Desc
            } else {
                SortDirection::Asc
            };
            
            items.push(OrderByItem { expr, direction });
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        Ok(items)
    }
    
    /// Parse LIMIT clause
    fn parse_limit(&mut self) -> Result<LimitClause, ParseError> {
        self.skip_whitespace();
        
        let limit = if let TokenType::Number(n) = &self.peek().token_type {
            n.parse::<i64>().map_err(|_| ParseError {
                message: "Invalid LIMIT value".to_string(),
                position: self.current,
            })?
        } else {
            return Err(ParseError {
                message: "Expected number after LIMIT".to_string(),
                position: self.current,
            });
        };
        self.advance();
        
        // Check for OFFSET
        self.skip_whitespace();
        let offset = if self.check(&TokenType::Offset) {
            self.advance();
            self.skip_whitespace();
            if let TokenType::Number(n) = &self.peek().token_type {
                let offset = n.parse::<i64>().map_err(|_| ParseError {
                    message: "Invalid OFFSET value".to_string(),
                    position: self.current,
                })?;
                self.advance();
                Some(offset)
            } else {
                return Err(ParseError {
                    message: "Expected number after OFFSET".to_string(),
                    position: self.current,
                });
            }
        } else {
            None
        };
        
        Ok(LimitClause { limit, offset })
    }
    
    /// Parse INSERT statement
    fn parse_insert(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Insert)?;
        self.skip_whitespace();
        self.expect(TokenType::Into)?;
        self.skip_whitespace();
        
        let table = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected table name after INSERT INTO".to_string(),
                position: self.current,
            });
        };
        
        // Parse column list (optional)
        self.skip_whitespace();
        let columns = if self.check(&TokenType::LeftParen) {
            self.advance();
            let cols = self.parse_identifier_list()?;
            self.expect(TokenType::RightParen)?;
            Some(cols)
        } else {
            None
        };
        
        // Parse VALUES
        self.skip_whitespace();
        self.expect(TokenType::Values)?;
        
        let mut values = Vec::new();
        loop {
            self.skip_whitespace();
            self.expect(TokenType::LeftParen)?;
            let row = self.parse_expression_list()?;
            self.expect(TokenType::RightParen)?;
            values.push(row);
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        Ok(Statement::Insert(InsertStatement {
            table,
            columns,
            values,
        }))
    }
    
    /// Parse UPDATE statement
    fn parse_update(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Update)?;
        self.skip_whitespace();
        
        let table = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected table name after UPDATE".to_string(),
                position: self.current,
            });
        };
        
        self.skip_whitespace();
        self.expect(TokenType::Set)?;
        self.skip_whitespace();
        
        let mut assignments = Vec::new();
        loop {
            self.skip_whitespace();
            
            let col = if let TokenType::Identifier(name) = &self.peek().token_type {
                let name = name.clone();
                self.advance();
                name
            } else {
                return Err(ParseError {
                    message: "Expected column name".to_string(),
                    position: self.current,
                });
            };
            
            self.skip_whitespace();
            self.expect(TokenType::Equal)?;
            self.skip_whitespace();
            let expr = self.parse_expression()?;
            
            assignments.push((col, expr));
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        // Parse WHERE clause
        self.skip_whitespace();
        let where_clause = if self.check(&TokenType::Where) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        Ok(Statement::Update(UpdateStatement {
            table,
            assignments,
            where_clause,
        }))
    }
    
    /// Parse DELETE statement
    fn parse_delete(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Delete)?;
        self.skip_whitespace();
        self.expect(TokenType::From)?;
        self.skip_whitespace();
        
        let table = if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(ParseError {
                message: "Expected table name after DELETE FROM".to_string(),
                position: self.current,
            });
        };
        
        self.skip_whitespace();
        let where_clause = if self.check(&TokenType::Where) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        Ok(Statement::Delete(DeleteStatement { table, where_clause }))
    }
    
    /// Parse CREATE statement
    fn parse_create(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Create)?;
        self.expect(TokenType::Table)?;
        self.skip_whitespace();
        
        let name = if let TokenType::Identifier(n) = &self.peek().token_type {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected table name after CREATE TABLE".to_string(),
                position: self.current,
            });
        };
        
        self.expect(TokenType::LeftParen)?;
        
        let mut columns = Vec::new();
        loop {
            self.skip_whitespace();
            
            let col_name = if let TokenType::Identifier(n) = &self.peek().token_type {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected column name".to_string(),
                    position: self.current,
                });
            };
            
            self.skip_whitespace();
            let data_type = if let TokenType::Identifier(dt) = &self.peek().token_type {
                let dt = dt.clone();
                self.advance();
                dt
            } else {
                return Err(ParseError {
                    message: "Expected data type".to_string(),
                    position: self.current,
                });
            };
            
            // Parse constraints (simplified)
            let mut constraints = Vec::new();
            self.skip_whitespace();
            loop {
                match &self.peek().token_type {
                    TokenType::Identifier(keyword)
                        if keyword.eq_ignore_ascii_case("PRIMARY")
                            || keyword.eq_ignore_ascii_case("UNIQUE") =>
                    {
                        constraints.push(keyword.clone());
                        self.advance();
                        self.skip_whitespace();
                        if let TokenType::Identifier(kw2) = &self.peek().token_type {
                            if kw2.eq_ignore_ascii_case("KEY") || kw2.eq_ignore_ascii_case("NULL") {
                                constraints.push(kw2.clone());
                                self.advance();
                                self.skip_whitespace();
                            }
                        }
                    }
                    TokenType::Not => {
                        constraints.push("NOT".to_string());
                        self.advance();
                        self.skip_whitespace();
                        if let TokenType::Identifier(kw2) = &self.peek().token_type {
                            if kw2.eq_ignore_ascii_case("NULL") {
                                constraints.push(kw2.clone());
                                self.advance();
                                self.skip_whitespace();
                            }
                        }
                    }
                    _ => break,
                }
            }
            
            columns.push(ColumnDefinition {
                name: col_name,
                data_type,
                constraints,
            });
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        self.expect(TokenType::RightParen)?;
        
        Ok(Statement::Create(CreateStatement { name, columns }))
    }
    
    /// Parse DROP statement
    fn parse_drop(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Drop)?;
        self.skip_whitespace();
        
        let object_type = if let TokenType::Identifier(t) = &self.peek().token_type {
            let t = t.clone();
            self.advance();
            t
        } else if self.check(&TokenType::Table) {
            self.advance();
            "TABLE".to_string()
        } else {
            return Err(ParseError {
                message: "Expected object type after DROP".to_string(),
                position: self.current,
            });
        };
        
        // Check for IF EXISTS
        self.skip_whitespace();
        let if_exists = if self.check(&TokenType::Identifier("IF".to_string())) 
            || (self.current + 1 < self.tokens.len() 
                && matches!(self.peek().token_type, TokenType::Identifier(ref s) if s.to_uppercase() == "IF"))
        {
            // Simplified check for IF EXISTS
            false
        } else {
            false
        };
        
        let name = if let TokenType::Identifier(n) = &self.peek().token_type {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected name after DROP".to_string(),
                position: self.current,
            });
        };
        
        Ok(Statement::Drop(DropStatement {
            object_type,
            name,
            if_exists,
        }))
    }
    
    /// Parse ALTER statement
    fn parse_alter(&mut self) -> Result<Statement, ParseError> {
        self.expect(TokenType::Alter)?;
        self.expect(TokenType::Table)?;
        self.skip_whitespace();
        
        let table = if let TokenType::Identifier(n) = &self.peek().token_type {
            let n = n.clone();
            self.advance();
            n
        } else {
            return Err(ParseError {
                message: "Expected table name after ALTER TABLE".to_string(),
                position: self.current,
            });
        };
        
        // Parse action (ADD COLUMN, DROP COLUMN, etc.)
        self.skip_whitespace();
        let action = if self.check(&TokenType::Add) {
            self.advance();
            self.expect(TokenType::Column)?;
            self.skip_whitespace();
            
            let col_name = if let TokenType::Identifier(n) = &self.peek().token_type {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected column name".to_string(),
                    position: self.current,
                });
            };
            
            self.skip_whitespace();
            let data_type = if let TokenType::Identifier(dt) = &self.peek().token_type {
                let dt = dt.clone();
                self.advance();
                dt
            } else {
                return Err(ParseError {
                    message: "Expected data type".to_string(),
                    position: self.current,
                });
            };
            
            AlterAction::Add(ColumnDefinition {
                name: col_name,
                data_type,
                constraints: Vec::new(),
            })
        } else if self.check(&TokenType::Drop) {
            self.advance();
            self.expect(TokenType::Column)?;
            self.skip_whitespace();
            
            let col_name = if let TokenType::Identifier(n) = &self.peek().token_type {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected column name".to_string(),
                    position: self.current,
                });
            };
            
            AlterAction::Drop(col_name)
        } else if self.check_identifier_keyword("RENAME") {
            self.advance();
            self.skip_whitespace();

            if self.check(&TokenType::Column) || self.check_identifier_keyword("COLUMN") {
                self.advance();
            }

            self.skip_whitespace();
            let old_name = if let TokenType::Identifier(n) = &self.peek().token_type {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected old column name after RENAME COLUMN".to_string(),
                    position: self.current,
                });
            };

            self.skip_whitespace();
            if self.check_identifier_keyword("TO") {
                self.advance();
            } else {
                return Err(ParseError {
                    message: "Expected TO in RENAME COLUMN clause".to_string(),
                    position: self.current,
                });
            }

            self.skip_whitespace();
            let new_name = if let TokenType::Identifier(n) = &self.peek().token_type {
                let n = n.clone();
                self.advance();
                n
            } else {
                return Err(ParseError {
                    message: "Expected new column name after TO".to_string(),
                    position: self.current,
                });
            };

            AlterAction::Rename { old_name, new_name }
        } else {
            return Err(ParseError {
                message: "Expected ALTER action (ADD, DROP, RENAME)".to_string(),
                position: self.current,
            });
        };
        
        Ok(Statement::Alter(AlterStatement { table, action }))
    }
    
    /// Parse expression with operators
    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_or_expression()
    }
    
    fn parse_or_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_and_expression()?;
        
        loop {
            self.skip_whitespace();
            if self.check(&TokenType::Or) {
                self.advance();
                let right = self.parse_and_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::Or,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        
        Ok(left)
    }
    
    fn parse_and_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_comparison_expression()?;
        
        loop {
            self.skip_whitespace();
            if self.check(&TokenType::And) {
                self.advance();
                let right = self.parse_comparison_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::And,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        
        Ok(left)
    }
    
    fn parse_comparison_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_additive_expression()?;
        
        loop {
            self.skip_whitespace();
            let op = if self.check(&TokenType::Equal) {
                self.advance();
                BinaryOperator::Equal
            } else if self.check(&TokenType::NotEqual) {
                self.advance();
                BinaryOperator::NotEqual
            } else if self.check(&TokenType::LessThan) {
                self.advance();
                BinaryOperator::LessThan
            } else if self.check(&TokenType::LessThanOrEqual) {
                self.advance();
                BinaryOperator::LessThanOrEqual
            } else if self.check(&TokenType::GreaterThan) {
                self.advance();
                BinaryOperator::GreaterThan
            } else if self.check(&TokenType::GreaterThanOrEqual) {
                self.advance();
                BinaryOperator::GreaterThanOrEqual
            } else if self.check(&TokenType::Like) {
                self.advance();
                BinaryOperator::Like
            } else if self.check(&TokenType::In) {
                self.advance();
                BinaryOperator::In
            } else if self.check(&TokenType::Between) {
                self.advance();
                BinaryOperator::Between
            } else if self.check_identifier_keyword("IS") {
                self.advance();
                BinaryOperator::Is
            } else {
                break;
            };
            
            let right = self.parse_additive_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_additive_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_multiplicative_expression()?;
        
        loop {
            self.skip_whitespace();
            let op = if self.check(&TokenType::Plus) {
                self.advance();
                BinaryOperator::Plus
            } else if self.check(&TokenType::Minus) {
                self.advance();
                BinaryOperator::Minus
            } else {
                break;
            };
            
            let right = self.parse_multiplicative_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_multiplicative_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_primary_expression()?;
        
        loop {
            self.skip_whitespace();
            let op = if self.check(&TokenType::Star) {
                self.advance();
                BinaryOperator::Multiply
            } else if self.check(&TokenType::Slash) {
                self.advance();
                BinaryOperator::Divide
            } else if self.check(&TokenType::Percent) {
                self.advance();
                BinaryOperator::Modulo
            } else {
                break;
            };
            
            let right = self.parse_primary_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        self.skip_whitespace();
        
        match &self.peek().token_type {
            TokenType::Number(n) => {
                let n = n.clone();
                self.advance();
                Ok(Expression::Number(n))
            }
            TokenType::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expression::String(s))
            }
            TokenType::Star => {
                self.advance();
                Ok(Expression::Star)
            }
            TokenType::Identifier(id) => {
                let id = id.clone();

                if id.eq_ignore_ascii_case("NULL") {
                    self.advance();
                    return Ok(Expression::Null);
                }

                self.advance();
                
                // Check for function call or table.column
                self.skip_whitespace();
                if self.check(&TokenType::LeftParen) {
                    self.advance();
                    let args = if self.check(&TokenType::RightParen) {
                        Vec::new()
                    } else {
                        self.parse_expression_list()?
                    };
                    self.expect(TokenType::RightParen)?;
                    Ok(Expression::FunctionCall { name: id, args })
                } else if self.check(&TokenType::Dot) {
                    self.advance();
                    if let TokenType::Identifier(col) = &self.peek().token_type {
                        let col = col.clone();
                        self.advance();
                        Ok(Expression::Column {
                            table: Some(id),
                            name: col,
                        })
                    } else {
                        Err(ParseError {
                            message: "Expected column name after .".to_string(),
                            position: self.current,
                        })
                    }
                } else {
                    Ok(Expression::Column {
                        table: None,
                        name: id,
                    })
                }
            }
            TokenType::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(TokenType::RightParen)?;
                Ok(Expression::Parenthesized(Box::new(expr)))
            }
            TokenType::Not => {
                self.advance();
                let expr = self.parse_primary_expression()?;
                Ok(Expression::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(expr),
                })
            }
            TokenType::Minus => {
                self.advance();
                let expr = self.parse_primary_expression()?;
                Ok(Expression::UnaryOp {
                    op: UnaryOperator::Minus,
                    expr: Box::new(expr),
                })
            }
            TokenType::Plus => {
                self.advance();
                let expr = self.parse_primary_expression()?;
                Ok(Expression::UnaryOp {
                    op: UnaryOperator::Plus,
                    expr: Box::new(expr),
                })
            }
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", self.peek().token_type),
                position: self.current,
            }),
        }
    }
    
    /// Parse expression list (comma-separated expressions)
    fn parse_expression_list(&mut self) -> Result<Vec<Expression>, ParseError> {
        let mut exprs = Vec::new();
        
        loop {
            self.skip_whitespace();
            exprs.push(self.parse_expression()?);
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        Ok(exprs)
    }
    
    /// Parse identifier list (comma-separated identifiers)
    fn parse_identifier_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut identifiers = Vec::new();
        
        loop {
            self.skip_whitespace();
            if let TokenType::Identifier(id) = &self.peek().token_type {
                identifiers.push(id.clone());
                self.advance();
            } else {
                return Err(ParseError {
                    message: "Expected identifier".to_string(),
                    position: self.current,
                });
            }
            
            self.skip_whitespace();
            if !self.check(&TokenType::Comma) {
                break;
            }
            self.advance();
        }
        
        Ok(identifiers)
    }
    
    // Helper methods
    
    fn peek(&self) -> LexicalToken {
        self.tokens.get(self.current).cloned().unwrap_or_else(|| LexicalToken {
            token_type: TokenType::Eof,
            value: String::new(),
            position: self.current,
        })
    }
    
    fn advance(&mut self) {
        self.current += 1;
    }
    
    fn check(&self, token_type: &TokenType) -> bool {
        let current = &self.peek().token_type;
        std::mem::discriminant(current) == std::mem::discriminant(token_type)
    }

    fn check_identifier_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek().token_type, TokenType::Identifier(ref value) if value.eq_ignore_ascii_case(keyword))
    }

    fn is_alias_identifier_candidate(&self) -> bool {
        if let TokenType::Identifier(value) = &self.peek().token_type {
            !matches!(
                value.to_ascii_uppercase().as_str(),
                "JOIN"
                    | "INNER"
                    | "LEFT"
                    | "RIGHT"
                    | "FULL"
                    | "CROSS"
                    | "WHERE"
                    | "GROUP"
                    | "HAVING"
                    | "ORDER"
                    | "LIMIT"
                    | "OFFSET"
                    | "ON"
            )
        } else {
            false
        }
    }
    
    fn expect(&mut self, token_type: TokenType) -> Result<(), ParseError> {
        self.skip_whitespace();
        let current_token = self.peek();
        
        if std::mem::discriminant(&current_token.token_type) == std::mem::discriminant(&token_type) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, got {:?}", token_type, current_token.token_type),
                position: self.current,
            })
        }
    }
    
    fn skip_whitespace(&mut self) {
        while self.current < self.tokens.len() {
            let token = &self.tokens[self.current];
            if token.token_type == TokenType::Whitespace 
                || (token.token_type == TokenType::Unknown && token.value.trim().is_empty()) {
                self.current += 1;
            } else {
                break;
            }
        }
    }
}
