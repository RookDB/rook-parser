use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::io::{self, Write};

mod models;
mod utils;
use utils::build_query_summary;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--help" | "-h" => {
                if args.len() > 2 {
                    print_statement_help(&args[2]);
                } else {
                    print_help();
                }
                return;
            }
            _ => {}
        }
    }

    let dialect = GenericDialect {};

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
    println!("Rook Parser CLI\n");
    println!("Usage: cargo run -- [OPTIONS]\n");
    println!("Options:");
    println!("  -h, --help                 Show this help message and exit");
    println!(
        "  --help <statement>         Show help for a statement type (e.g., select, insert, create-table)"
    );
    println!("\nDescription:");
    println!("  Interactive SQL parser using Apache DataFusion's sqlparser.\n");
    println!("  Enter SQL queries to see their AST and a custom JSON summary.\n");
    println!("  Type 'exit' to quit the program.\n");
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

