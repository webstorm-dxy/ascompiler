# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                  # compile (binary: target/debug/asc)
cargo build --release        # optimized build (binary: target/release/asc)
cargo run -- <source.as>     # compile + run with args
cargo test                   # run all tests
```

## Usage

```bash
asc 源文件.as                # compile to executable (same name as source, no extension)
asc 源文件.as -o 输出         # compile to named executable
```

## Architecture

A Chinese-language ("问源") imperative language compiler targeting LLVM via `inkwell` (Rust LLVM bindings). Pipeline:

1. **Lexer** (`src/lexer.rs`) — tokenization, handles Chinese/English punctuation and CJK keywords
2. **Parser** (`src/parser.rs`) — syntax analysis, produces AST
3. **Codegen** (`src/codegen.rs`) — walks AST, emits LLVM IR via inkwell

`main.rs` wires the pipeline end-to-end: reads `.as` source file → lex → parse → codegen → emit object file → link to native executable via `cc`.

## LLVM / Inkwell Conventions

- **LLVM**: 20.1.8, Homebrew path `/opt/homebrew/opt/llvm@20/`
- **inkwell**: 0.8.0 (edition 2021) with features `llvm20-1`, `target-aarch64`
- **Env**: `LLVM_SYS_201_PREFIX` set in `.cargo/config.toml` so `llvm-sys-201` finds Homebrew LLVM.
- Every `Context` owns all LLVM values created within it. Builders, modules, types, and values are tied to the context's lifetime.
- `build_global_string_ptr` returns `Result` in inkwell 0.8 — unwrap or propagate.
- `build_alloca` returns `Result` — use `?` or handle explicitly.
- Use `context.ptr_type(AddressSpace::default())` for pointer types (not `IntType::ptr_type` — deprecated).
- `build_return` returns `Result`, must be handled (`let _ = ...` or `?`).
- Rust edition 2024 is used for this project's own crate.
