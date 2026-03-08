# Rook Parser

Rook Parser provides the **query processor layer for RookDB**.  
It uses **Apache DataFusion's `sqlparser`** to parse SQL queries and generate an **Abstract Syntax Tree (AST)**.

The generated AST is then decoded and used by **RookDB** to process queries according to its internal query execution logic.

Crate available at:  
https://crates.io/crates/rook-parser

---

## Overview

Rook Parser performs:

- SQL parsing (syntactic analysis)
- AST generation using DataFusion's `sqlparser`
- AST decoding for RookDB query processing

Instead of implementing a custom SQL parser, Rook Parser relies on the **Apache DataFusion SQL parser**, which provides a robust and well-tested SQL grammar.

---

## Getting Started

Run the interactive SQL parser:

```bash
cargo run
```