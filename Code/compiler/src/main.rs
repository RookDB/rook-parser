use lexical_parser::parser::SyntacticParser;
use lexical_parser::LexicalParser;
use std::io::{self, Read, Write};

fn run_query(query: &str) {
    println!("\n");
    println!("{}", "=".repeat(70));
    println!("SQL Query: {}", query);
    println!("{}", "=".repeat(70));

    let mut lexer = LexicalParser::new(query.to_string());

    match lexer.tokenize() {
        Ok(_) => {
            let tokens = lexer.get_tokens().to_vec();
            let mut syntactic_parser = SyntacticParser::new(tokens);

            match syntactic_parser.parse() {
                Ok(statement) => {
                    println!(
                        "\n✓ Lexical Analysis: SUCCESS ({} tokens)",
                        lexer.get_filtered_tokens().len()
                    );
                    println!("✓ Syntactic Analysis: SUCCESS");
                    println!("\n📊 ABSTRACT SYNTAX TREE:");
                    println!("{:#?}", statement);
                    println!("\n📝 AST Display Format:");
                    println!("{}", statement);
                }
                Err(e) => {
                    println!(
                        "\n✓ Lexical Analysis: SUCCESS ({} tokens)",
                        lexer.get_filtered_tokens().len()
                    );
                    println!("✗ Syntactic Analysis: FAILED");
                    println!("Error: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Lexical Analysis: FAILED");
            eprintln!("Error: {}", e);
        }
    }
}

fn main() {
    env_logger::init();

    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if args.first().is_some_and(|arg| arg == "--stdin") {
        let mut stdin_query = String::new();
        match io::stdin().read_to_string(&mut stdin_query) {
            Ok(_) if !stdin_query.trim().is_empty() => {
                run_query(stdin_query.trim());
                return;
            }
            Ok(_) => {
                eprintln!("No SQL was provided on stdin.");
                return;
            }
            Err(err) => {
                eprintln!("Failed to read SQL from stdin: {}", err);
                return;
            }
        }
    }

    let cli_query = args.join(" ");

    if !cli_query.trim().is_empty() {
        run_query(&cli_query);
        return;
    }

    print!("Enter SQL query (press Enter for demo queries): ");
    if let Err(err) = io::stdout().flush() {
        eprintln!("Failed to flush stdout: {}", err);
    }

    let mut interactive_query = String::new();
    match io::stdin().read_line(&mut interactive_query) {
        Ok(_) if !interactive_query.trim().is_empty() => {
            run_query(interactive_query.trim());
            return;
        }
        Ok(_) => {}
        Err(err) => {
            eprintln!("Failed to read SQL from interactive input: {}", err);
        }
    }

    let demo_queries = vec![
        "SELECT id, name FROM users WHERE age > 18;",
        "SELECT * FROM products WHERE category = 'Electronics' AND price < 1000;",
        "INSERT INTO customers (id, name, email) VALUES (1, 'John Doe', 'john@example.com');",
        "UPDATE orders SET status = 'completed' WHERE order_id = 42;",
        "DELETE FROM users WHERE age < 13;",
        "SELECT u.id, u.name, o.order_id FROM users u LEFT JOIN orders o ON u.id = o.user_id;",
        "SELECT DISTINCT category FROM products ORDER BY category ASC LIMIT 10;",
    ];

    for query in demo_queries {
        run_query(query);
    }
}
