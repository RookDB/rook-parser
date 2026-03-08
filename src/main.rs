use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::io::{self, Write};

fn main() {
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
                    println!("\nAST:");
                    println!("{:#?}", statement);

                    println!("\nDisplay:");
                    println!("{}", statement);
                }
            }
            Err(e) => {
                println!("Parse error: {}", e);
            }
        }
    }
}