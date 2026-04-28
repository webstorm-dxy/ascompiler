mod codegen;
mod lexer;
mod parser;
mod semantic;
mod stdlib;

use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use lexer::Lexer;
use parser::Parser;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: asc <源文件.as> [-o <输出文件>] [--ir]");
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
    if input_path.extension().map(|e| e != "as").unwrap_or(true) {
        eprintln!("警告: 输入文件扩展名不是 .as: {}", input_path.display());
    }

    let ir_mode = args.iter().any(|a| a == "--ir");

    // Default output: strip .as extension (not used in --ir mode)
    let output_path = if ir_mode {
        PathBuf::new()
    } else {
        parse_output_arg(&args).unwrap_or_else(|| {
            let mut p = input_path.clone();
            p.set_extension("");
            PathBuf::from(p.file_name().unwrap_or(std::ffi::OsStr::new("a.out")))
        })
    };

    // --- Read source ---
    let source = match fs::read_to_string(&input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("错误: 无法读取文件 {}: {}", input_path.display(), e);
            process::exit(1);
        }
    };

    // --- Lex + Parse ---
    let lexer = Lexer::new_with_name(&source, input_path.display().to_string());
    let parser = Parser::new(lexer);
    let program = match parser.parse_program() {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    let program = match stdlib::merge_with_standard_library(program) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("标准库解析错误:\n{}", e);
            process::exit(1);
        }
    };

    if let Err(e) = semantic::analyze_with_source(
        &program,
        Some(&source),
        Some(&input_path.display().to_string()),
    ) {
        eprintln!("{}", e);
        process::exit(1);
    }

    if !ir_mode && !program.has_entry {
        eprintln!(
            "错误: 程序没有入口点\n --> {}\n  = 帮助: 在主方法前添加一行 `@声明 入口`，例如：\n\n@声明 入口\n定义 方法 主（）返回 无：\n。。",
            input_path.display()
        );
        process::exit(1);
    }

    // --- Codegen to LLVM module ---
    let context = Context::create();
    let module = context.create_module("ascompiler");

    if let Err(e) = codegen::generate(&program, &context, &module) {
        eprintln!("代码生成错误:\n{}", e);
        process::exit(1);
    }

    if let Err(e) = module.verify() {
        eprintln!("LLVM 模块验证失败:\n{}", e);
        process::exit(1);
    }

    // --- IR mode: print LLVM IR to stdout and exit ---
    if ir_mode {
        module.print_to_stderr();
        return;
    }

    // --- Emit object file ---
    let obj_path = output_path.with_extension("o");

    Target::initialize_native(&InitializationConfig::default()).expect("初始化本地目标失败");

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).expect("获取目标架构失败");

    let target_machine = target
        .create_target_machine(
            &triple,
            "",
            "",
            inkwell::OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .expect("创建目标机器失败");

    target_machine
        .write_to_file(&module, FileType::Object, &obj_path)
        .expect("写入目标文件失败");

    // --- Link to executable ---
    let status = Command::new("cc")
        .arg(&obj_path)
        .arg(runtime_source_path())
        .arg("-o")
        .arg(&output_path)
        .status()
        .expect("调用链接器失败");

    if !status.success() {
        eprintln!("错误: 链接失败");
        let _ = fs::remove_file(&obj_path);
        process::exit(1);
    }

    // --- Clean up object file ---
    let _ = fs::remove_file(&obj_path);

    println!("编译成功 → {}", output_path.display());
}

fn runtime_source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/std_io.c")
}

/// Parse `-o <output>` from arguments.
fn parse_output_arg(args: &[String]) -> Option<PathBuf> {
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "-o" {
            return Some(PathBuf::from(&args[i + 1]));
        }
    }
    None
}
