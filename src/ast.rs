/// Abstract Syntax Tree (AST) structures for SQL statements
use std::fmt;

/// Represents a complete SQL statement
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Select(SelectStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
    Create(CreateStatement),
    Drop(DropStatement),
    Alter(AlterStatement),
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Select(s) => write!(f, "SELECT: {}", s),
            Statement::Insert(s) => write!(f, "INSERT: {}", s),
            Statement::Update(s) => write!(f, "UPDATE: {}", s),
            Statement::Delete(s) => write!(f, "DELETE: {}", s),
            Statement::Create(s) => write!(f, "CREATE: {}", s),
            Statement::Drop(s) => write!(f, "DROP: {}", s),
            Statement::Alter(s) => write!(f, "ALTER: {}", s),
        }
    }
}

/// SELECT statement
#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    pub distinct: bool,
    pub select_list: Vec<SelectItem>,
    pub from_clause: Option<FromClause>,
    pub where_clause: Option<Expression>,
    pub group_by_clause: Option<Vec<Expression>>,
    pub having_clause: Option<Expression>,
    pub order_by_clause: Option<Vec<OrderByItem>>,
    pub limit_clause: Option<LimitClause>,
}

impl fmt::Display for SelectStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SELECT{}", if self.distinct { " DISTINCT" } else { "" })?;
        write!(f, " [{}]", self.select_list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(", "))?;
        
        if let Some(from) = &self.from_clause {
            write!(f, " FROM {}", from)?;
        }
        if let Some(where_expr) = &self.where_clause {
            write!(f, " WHERE {}", where_expr)?;
        }
        if let Some(group_by) = &self.group_by_clause {
            write!(f, " GROUP BY [{}]", group_by.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", "))?;
        }
        if let Some(having) = &self.having_clause {
            write!(f, " HAVING {}", having)?;
        }
        if let Some(order_by) = &self.order_by_clause {
            write!(f, " ORDER BY [{}]", order_by.iter()
                .map(|o| o.to_string())
                .collect::<Vec<_>>()
                .join(", "))?;
        }
        if let Some(limit) = &self.limit_clause {
            write!(f, " {}", limit)?;
        }
        Ok(())
    }
}

/// SELECT list item
#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    /// All columns (*)
    AllColumns,
    /// Column name with optional alias
    Column {
        expr: Expression,
        alias: Option<String>,
    },
}

impl fmt::Display for SelectItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectItem::AllColumns => write!(f, "*"),
            SelectItem::Column { expr, alias } => {
                if let Some(a) = alias {
                    write!(f, "{} AS {}", expr, a)
                } else {
                    write!(f, "{}", expr)
                }
            }
        }
    }
}

/// FROM clause with table and joins
#[derive(Debug, Clone, PartialEq)]
pub struct FromClause {
    pub table: TableReference,
    pub joins: Vec<Join>,
}

impl fmt::Display for FromClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.table)?;
        for join in &self.joins {
            write!(f, " {}", join)?;
        }
        Ok(())
    }
}

/// Table reference with optional alias
#[derive(Debug, Clone, PartialEq)]
pub struct TableReference {
    pub name: String,
    pub alias: Option<String>,
}

impl fmt::Display for TableReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(alias) = &self.alias {
            write!(f, " {}", alias)?;
        }
        Ok(())
    }
}

/// JOIN clause
#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinType::Inner => write!(f, "INNER"),
            JoinType::Left => write!(f, "LEFT"),
            JoinType::Right => write!(f, "RIGHT"),
            JoinType::Full => write!(f, "FULL"),
            JoinType::Cross => write!(f, "CROSS"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub join_type: JoinType,
    pub table: TableReference,
    pub on_condition: Option<Expression>,
}

impl fmt::Display for Join {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} JOIN {}", self.join_type, self.table)?;
        if let Some(cond) = &self.on_condition {
            write!(f, " ON {}", cond)?;
        }
        Ok(())
    }
}

/// ORDER BY item
#[derive(Debug, Clone, PartialEq)]
pub struct OrderByItem {
    pub expr: Expression,
    pub direction: SortDirection,
}

impl fmt::Display for OrderByItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.expr, self.direction)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl fmt::Display for SortDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortDirection::Asc => write!(f, "ASC"),
            SortDirection::Desc => write!(f, "DESC"),
        }
    }
}

/// LIMIT clause
#[derive(Debug, Clone, PartialEq)]
pub struct LimitClause {
    pub limit: i64,
    pub offset: Option<i64>,
}

impl fmt::Display for LimitClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LIMIT {}", self.limit)?;
        if let Some(offset) = self.offset {
            write!(f, " OFFSET {}", offset)?;
        }
        Ok(())
    }
}

/// Expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    // Literals
    Number(String),
    String(String),
    
    // Compound expressions
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    Column {
        table: Option<String>,
        name: String,
    },
    /// (expression)
    Parenthesized(Box<Expression>),
    
    // Special
    Star,
    Null,
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Number(n) => write!(f, "{}", n),
            Expression::String(s) => write!(f, "'{}'", s),
            Expression::Star => write!(f, "*"),
            Expression::Null => write!(f, "NULL"),
            Expression::BinaryOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            Expression::UnaryOp { op, expr } => {
                write!(f, "{} {}", op, expr)
            }
            Expression::FunctionCall { name, args } => {
                write!(f, "{}({})", name, args.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", "))
            }
            Expression::Column { table, name } => {
                if let Some(t) = table {
                    write!(f, "{}.{}", t, name)
                } else {
                    write!(f, "{}", name)
                }
            }
            Expression::Parenthesized(expr) => {
                write!(f, "({})", expr)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    // Comparison
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    
    // Logical
    And,
    Or,
    
    // Arithmetic
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    
    // String
    Like,
    In,
    Between,
    Is,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOperator::Equal => write!(f, "="),
            BinaryOperator::NotEqual => write!(f, "!="),
            BinaryOperator::LessThan => write!(f, "<"),
            BinaryOperator::LessThanOrEqual => write!(f, "<="),
            BinaryOperator::GreaterThan => write!(f, ">"),
            BinaryOperator::GreaterThanOrEqual => write!(f, ">="),
            BinaryOperator::And => write!(f, "AND"),
            BinaryOperator::Or => write!(f, "OR"),
            BinaryOperator::Plus => write!(f, "+"),
            BinaryOperator::Minus => write!(f, "-"),
            BinaryOperator::Multiply => write!(f, "*"),
            BinaryOperator::Divide => write!(f, "/"),
            BinaryOperator::Modulo => write!(f, "%"),
            BinaryOperator::Like => write!(f, "LIKE"),
            BinaryOperator::In => write!(f, "IN"),
            BinaryOperator::Between => write!(f, "BETWEEN"),
            BinaryOperator::Is => write!(f, "IS"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Minus,
    Plus,
}

impl fmt::Display for UnaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOperator::Not => write!(f, "NOT"),
            UnaryOperator::Minus => write!(f, "-"),
            UnaryOperator::Plus => write!(f, "+"),
        }
    }
}

/// INSERT statement
#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub values: Vec<Vec<Expression>>,
}

impl fmt::Display for InsertStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "INTO {} ", self.table)?;
        if let Some(cols) = &self.columns {
            write!(f, "({})", cols.join(", "))?;
        }
        write!(f, " VALUES")?;
        for (idx, row) in self.values.iter().enumerate() {
            if idx > 0 {
                write!(f, ",")?;
            }
            write!(f, " ({})", row.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", "))?;
        }
        Ok(())
    }
}

/// UPDATE statement
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStatement {
    pub table: String,
    pub assignments: Vec<(String, Expression)>,
    pub where_clause: Option<Expression>,
}

impl fmt::Display for UpdateStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} SET ", self.table)?;
        write!(f, "{}", self.assignments.iter()
            .map(|(col, expr)| format!("{} = {}", col, expr))
            .collect::<Vec<_>>()
            .join(", "))?;
        if let Some(where_expr) = &self.where_clause {
            write!(f, " WHERE {}", where_expr)?;
        }
        Ok(())
    }
}

/// DELETE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub table: String,
    pub where_clause: Option<Expression>,
}

impl fmt::Display for DeleteStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FROM {} ", self.table)?;
        if let Some(where_expr) = &self.where_clause {
            write!(f, "WHERE {}", where_expr)?;
        }
        Ok(())
    }
}

/// CREATE TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct CreateStatement {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
}

impl fmt::Display for CreateStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TABLE {} (", self.name)?;
        write!(f, "{})", self.columns.iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", "))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: String,
    pub constraints: Vec<String>,
}

impl fmt::Display for ColumnDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.data_type)?;
        if !self.constraints.is_empty() {
            write!(f, " {}", self.constraints.join(" "))?;
        }
        Ok(())
    }
}

/// DROP statement
#[derive(Debug, Clone, PartialEq)]
pub struct DropStatement {
    pub object_type: String, // TABLE, DATABASE, etc.
    pub name: String,
    pub if_exists: bool,
}

impl fmt::Display for DropStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", 
            self.object_type, 
            if self.if_exists { "IF EXISTS" } else { "" },
            self.name)?;
        Ok(())
    }
}

/// ALTER statement
#[derive(Debug, Clone, PartialEq)]
pub struct AlterStatement {
    pub table: String,
    pub action: AlterAction,
}

impl fmt::Display for AlterStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TABLE {} {}", self.table, self.action)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlterAction {
    Add(ColumnDefinition),
    Drop(String),
    Rename { old_name: String, new_name: String },
}

impl fmt::Display for AlterAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlterAction::Add(col) => write!(f, "ADD COLUMN {}", col),
            AlterAction::Drop(name) => write!(f, "DROP COLUMN {}", name),
            AlterAction::Rename { old_name, new_name } => {
                write!(f, "RENAME COLUMN {} TO {}", old_name, new_name)
            }
        }
    }
}
