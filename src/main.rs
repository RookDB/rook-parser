use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::io::{self, Write};

mod models;
mod utils;
use utils::build_query_summary;

fn main() {
    let dialect = GenericDialect {};

    println!("Rook Parser CLI");
    println!("Type 'help' for available commands.");
    println!("Type 'help <statement>' for statement help.");
    println!("Type 'exit' to quit.\n");

    loop {
        let mut query = String::new();

        print!("Enter SQL query (or 'exit'): ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut query).unwrap();

        let query = query.trim();

        if query.eq_ignore_ascii_case("exit") {
            break;
        }

        if query.is_empty() {
            continue;
        }

        // Handle help commands interactively
        if query.eq_ignore_ascii_case("help") {
            print_help();
            continue;
        }

        if query.to_lowercase().starts_with("help ") {
            let statement = query[5..].trim();
            print_statement_help(statement);
            continue;
        }

        match Parser::parse_sql(&dialect, query) {
            Ok(statements) => {
                for statement in statements {
                    println!("\nAST Debug:");
                    println!("{:#?}", statement);

                    let summary = build_query_summary(&statement);

                    println!("\nCustom JSON:");
                    let json = serde_json::to_string_pretty(&summary).unwrap();
                    println!("{}", json);
                }
            }
            Err(e) => {
                println!("Parse error: {}", e);
            }
        }
    }
}

fn print_help() {
    println!("\nAvailable Commands:\n");

    println!("  help");
    println!("      Show general help.\n");

    println!("  help <statement>");
    println!("      Show help for a specific SQL statement.\n");

    println!("Supported statements:");
    println!("  select");
    println!("  insert");
    println!("  create-table");
    println!("  create-database");
    println!("  show-tables");
    println!("  show-databases");
    println!("  use\n");

    println!("Examples:");
    println!("  help select");
    println!("  help insert");
    println!("  SELECT * FROM users;\n");
}

fn print_statement_help(statement: &str) {
    match statement.to_lowercase().as_str() {
        "select" => {
            println!("SELECT Statement\n");
            println!("Usage: SELECT column1, column2 FROM table WHERE condition;\n");
            println!("Description: Retrieves data from one or more tables.\n");
            println!("Example: SELECT name, age FROM users WHERE age > 18;\n");
        }
        "insert" => {
            println!("INSERT Statement\n");
            println!("Usage: INSERT INTO table (column1, column2) VALUES (value1, value2);\n");
            println!("Description: Inserts new rows into a table.\n");
            println!("Example: INSERT INTO users (name, age) VALUES ('Alice', 30);\n");
        }
        "create-table" | "createtable" => {
            println!("CREATE TABLE Statement\n");
            println!("Usage: CREATE TABLE table_name (column1 TYPE, column2 TYPE, ...);\n");
            println!("Description: Defines a new table and its columns.\n");
            println!("Example: CREATE TABLE users (id INT, name TEXT, age INT);\n");
        }
        "create-database" | "createdatabase" => {
            println!("CREATE DATABASE Statement\n");
            println!("Usage: CREATE DATABASE db_name;\n");
            println!("Description: Creates a new database.\n");
            println!("Example: CREATE DATABASE testdb;\n");
        }
        "show-tables" | "showtables" => {
            println!("SHOW TABLES Statement\n");
            println!("Usage: SHOW TABLES;\n");
            println!("Description: Lists all tables in the current database.\n");
        }
        "show-databases" | "showdatabases" => {
            println!("SHOW DATABASES Statement\n");
            println!("Usage: SHOW DATABASES;\n");
            println!("Description: Lists all databases.\n");
        }
        "use" | "use-database" | "usedatabase" => {
            println!("USE DATABASE Statement\n");
            println!("Usage: USE database_name;\n");
            println!("Description: Changes the current database context.\n");
            println!("Example: USE testdb;\n");
        }
        _ => {
            println!(
                "No help entry for '{}'. Supported: select, insert, create-table, create-database, show-tables, show-databases, use",
                statement
            );
        }
    }
}