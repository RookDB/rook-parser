# RookDb SQL Compiler (`lexical_parser`)

Lightweight SQL lexer + parser in Rust that converts SQL queries into an Abstract Syntax Tree (AST).

![Rust](https://img.shields.io/badge/Rust-2021-blue)
![Status](https://img.shields.io/badge/Status-Active-success)

## 📖 Overview

This project is a small SQL front-end designed for learning, experimentation, and compiler-course style work.

It helps you:
- tokenize SQL input (lexical analysis),
- parse supported SQL grammar (syntactic analysis),
- inspect AST output in debug and readable formats.

**Problem solved:** it provides a clean way to understand how SQL text becomes structured syntax trees.

**Who it is for:**
- students learning compiler construction,
- beginners learning Rust parsing patterns,
- developers prototyping SQL tooling.

## ✨ Features

- SQL tokenization using `sqlparser` tokenizer + custom token classification.
- Recursive-descent parser with expression precedence handling.
- AST generation for common statement families.
- Supports `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `CREATE`, `DROP`, `ALTER`.
- Join support: `INNER`, `LEFT`, `RIGHT`, `FULL`, `CROSS`.
- Multiple runtime input modes:
  - command argument SQL,
  - `--stdin` piped SQL,
  - interactive prompt,
  - demo-query fallback.
- Separate integration test suites for lexer and parser.

## 🛠 Tech Stack

| Category | Stack |
|---|---|
| Language | Rust (Edition 2021) |
| Parsing helper library | `sqlparser = 0.48` (tokenizer) |
| Logging | `log`, `env_logger` |
| Build/Test tooling | Cargo (`cargo build`, `cargo run`, `cargo test`, `cargo clippy`) |

## 📦 Prerequisites / Required Installations

### System requirements
- Windows, Linux, or macOS
- Terminal/command prompt

### Required software
- Rust toolchain (recommended stable)
- Cargo (bundled with Rust)

### Install Rust (if needed)

```bash
# Linux/macOS
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# Install from: https://rustup.rs/
```

Verify installation:

```bash
rustc --version
cargo --version
```

> **Assumption:** Rust stable `1.75+` is sufficient (exact minimum version is not pinned in this repo).

## 🚀 Installation

1. Clone the repository:

```bash
git clone <your-repo-url>
cd RookDb_IS/Code/compiler
```

2. Build once to fetch dependencies:

```bash
cargo build
```

3. (Optional) Run tests to verify setup:

```bash
cargo test
```

### Environment variables

No `.env` file is required.

> **Assumption:** Default logger configuration is sufficient for local usage.

## ▶️ How to Run

### Development run

```bash
cargo run
```

Behavior with no arguments:
1. prompts for one SQL query,
2. if you press Enter without input, it runs built-in demo queries.

### Run a single SQL query (argument mode)

```bash
cargo run -- "SELECT id, name FROM users WHERE age > 18;"
```

### Run from stdin (recommended for pipes)

```bash
echo "SELECT name FROM users;" | cargo run -- --stdin
```

### Production-style run

```bash
cargo run --release -- "SELECT * FROM users;"
```

## ⚙️ Configuration

This project currently has minimal runtime configuration.

| Config | Where | Description |
|---|---|---|
| Logging | `env_logger` | Uses default environment behavior; no required variables |
| Input mode | CLI args / `--stdin` / interactive | Determines where SQL is read from |

No custom config file is required.

## 🧠 How It Works (Architecture / Code Flow)

High-level flow:

1. **Input collection** (`src/main.rs`)
  - read SQL from argument, `--stdin`, interactive input, or demo list.
2. **Lexical analysis** (`src/lib.rs`)
  - `LexicalParser` tokenizes raw SQL and maps tokens to `TokenType`.
3. **Syntactic analysis** (`src/parser.rs`)
  - `SyntacticParser` consumes tokens and builds AST nodes.
4. **AST output** (`src/main.rs`)
  - prints both debug AST and display-formatted AST.

Key modules:
- `src/lib.rs`: exported lexer/token API + module exports.
- `src/parser.rs`: recursive-descent SQL parser.
- `src/ast.rs`: AST types for statements/expressions/clauses.
- `src/main.rs`: CLI runtime entry point.

## 📁 Project Structure

```text
compiler/
├─ Cargo.toml
├─ README.md
├─ src/
│  ├─ main.rs        # CLI/runtime entry
│  ├─ lib.rs         # Public lexer/token API
│  ├─ parser.rs      # Syntactic parser
│  └─ ast.rs         # AST definitions
├─ tests/
│  ├─ lexer_tests.rs
│  └─ parser_tests.rs
└─ target/           # Build artifacts (generated)
```

## 🧪 Testing

Test framework: Rust built-in test framework (`cargo test`).

Run all tests:

```bash
cargo test
```

Run with strict linting:

```bash
cargo clippy --all-targets -- -D warnings
```

## 🚧 Known Limitations / Future Improvements

- Supports a practical SQL subset, not full SQL standard compliance.
- Error diagnostics can be improved (friendlier messages, richer context).
- No benchmark/performance profiling pipeline yet.
- No CI workflow file included yet.
- No fuzz/property-based parser robustness tests yet.

<!-- ## 🤝 Contributing

1. Fork and create a feature branch.
2. Make focused changes with tests.
3. Run quality checks:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

4. Open a pull request with a clear description. -->



