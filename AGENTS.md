# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust compiler for the Chinese-language “问源” language. Source files live in `src/`:

- `src/main.rs` wires the command-line flow: read `.as` source, parse, generate LLVM IR, emit object code, and link with `cc`.
- `src/lexer.rs` tokenizes source text, including Chinese keywords and punctuation variants.
- `src/parser.rs` defines the AST and parser.
- `src/semantic.rs` validates modules, imports, and callable resolution.
- `src/codegen.rs` emits LLVM IR through `inkwell`.

Unit tests are colocated in each Rust module under `#[cfg(test)]`. `test.as` is a sample source program. Build output is generated under `target/` and should not be edited.

## Build, Test, and Development Commands

- `cargo build` compiles the debug binary at `target/debug/asc`.
- `cargo build --release` builds an optimized compiler.
- `cargo test` runs all Rust unit tests.
- `cargo fmt` formats the Rust codebase.
- `cargo run -- test.as --ir` prints generated LLVM IR for the sample file.
- `cargo run -- test.as -o test` compiles `test.as` to a native executable named `test`.

The project depends on LLVM 20 via `inkwell`; `.cargo/config.toml` sets the local LLVM prefix.

## Coding Style & Naming Conventions

Use standard Rust formatting with `cargo fmt`. Prefer small, explicit functions that match the compiler pipeline stage they belong to. Rust identifiers should use `snake_case`; types and enum variants should use `CamelCase`. Keep AST data structures in `parser.rs`, semantic resolution in `semantic.rs`, and LLVM-specific logic in `codegen.rs`.

When adding language syntax, update the lexer, parser, semantic analysis, code generation, and tests together where applicable.

## Testing Guidelines

Use Rust’s built-in test framework. Add focused tests near the module being changed, for example lexer token tests in `src/lexer.rs` and parser AST tests in `src/parser.rs`. Test names should describe behavior, such as `test_parse_module_import_and_execute`.

Run `cargo test` before submitting changes. For codegen behavior, also compile and run a small `.as` program when practical.

## Commit & Pull Request Guidelines

The current history is minimal, with an imperative-style initial commit. Use concise commit messages such as `Add module import resolution` or `Fix string literal parsing`.

Pull requests should include a short description, the language behavior changed, tests run, and any LLVM or platform assumptions. Include sample `.as` input/output when changing user-visible compiler behavior.
