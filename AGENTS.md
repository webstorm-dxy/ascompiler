# Repository Guidelines

## Project Structure & Module Organization

This repository is a Cargo workspace for the Chinese-language “问源” compiler. The root `Cargo.toml` is the workspace manifest. The main compiler crate lives in `ascompiler/`, with support crates in sibling directories.

- `ascompiler/src/main.rs` is the `asc` binary entrypoint.
- `ascompiler/src/cli.rs` wires the command-line flow: read `.as` source, parse, generate LLVM IR, emit object code, and link with `cc`.
- `ascompiler/src/lexer.rs` tokenizes source text, including Chinese keywords and punctuation variants.
- `ascompiler/src/parser.rs` defines the AST and parser.
- `ascompiler/src/semantic.rs` validates modules, imports, and callable resolution.
- `ascompiler/src/codegen.rs` emits LLVM IR through `inkwell`.
- `salt/src/main.rs` implements the Salt package manager/toolchain for creating, building, and running 问源 projects.
- `wenyuan-ffi` and `wenyuan-ffi-macros` provide Rust FFI support.

Unit tests are colocated in each Rust module under `#[cfg(test)]`. Sample source programs live in `demo/`. Build output is generated under `target/` and should not be edited.

## Build, Test, and Development Commands

- `cargo build` compiles the debug binary at `target/debug/asc`.
- `cargo build --release` builds an optimized compiler.
- `cargo build -p salt` builds the Salt package manager/toolchain.
- `cargo test --workspace` runs all Rust unit tests across workspace members.
- `cargo fmt` formats the Rust codebase.
- `cargo run -p salt -- --help` checks the Salt CLI entrypoint.
- `cargo run -- demo/condition.as --ir` prints generated LLVM IR for the sample file.
- `cargo run -- demo/condition.as -o /tmp/wenyuan_condition` compiles `demo/condition.as` to a native executable.

The project depends on LLVM 20 via `inkwell`; `.cargo/config.toml` sets the local LLVM prefix.

## Coding Style & Naming Conventions

Use standard Rust formatting with `cargo fmt`. Prefer small, explicit functions that match the compiler pipeline stage they belong to. Rust identifiers should use `snake_case`; types and enum variants should use `CamelCase`. Keep AST data structures in `ascompiler/src/parser.rs`, semantic resolution in `ascompiler/src/semantic.rs`, and LLVM-specific logic in `ascompiler/src/codegen.rs`.

When adding language syntax, update the lexer, parser, semantic analysis, code generation, and tests together where applicable.

When updating compiler behavior, update the Salt toolchain code at the same time if the change affects project creation, build/run flows, source discovery, configuration, diagnostics, linking, or user-visible compiler behavior.

## Codex Operating Principles

- 以暗猜接口为耻，以认真查阅为荣。
- 以模糊执行为耻，以寻求确认为荣。
- 以盲想业务为耻，以人类确认为荣。
- 以创造接口为耻，以复用现有为荣。
- 以跳过验证为耻，以主动测试为荣。
- 以破坏架构为耻，以遵循规范为荣。
- 以假装理解为耻，以诚实无知为荣。
- 以盲目修改为耻，以谨慎重构为荣。

## Testing Guidelines

Use Rust’s built-in test framework. Add focused tests near the module being changed, for example lexer token tests in `ascompiler/src/lexer.rs` and parser AST tests in `ascompiler/src/parser.rs`. Test names should describe behavior, such as `test_parse_module_import_and_execute`.

Run `cargo test --workspace` before submitting changes. For codegen behavior, also compile and run a small `.as` program when practical.

When compiler changes are expected to work through Salt, test the relevant Salt workflow as well, such as building `salt`, running it against a temporary/sample Salt project, and confirming `salt build` or `salt run` produces the intended behavior.

## Commit & Pull Request Guidelines

The current history is minimal, with an imperative-style initial commit. Use concise commit messages such as `Add module import resolution` or `Fix string literal parsing`.

Pull requests should include a short description, the language behavior changed, tests run, and any LLVM or platform assumptions. Include sample `.as` input/output when changing user-visible compiler behavior.
