use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::{codegen, semantic, stdlib};
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FfiLinkOptions {
    libs: Vec<PathBuf>,
    search_paths: Vec<PathBuf>,
    rpaths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompileOptions {
    input_path: PathBuf,
    output_path: PathBuf,
    ir_mode: bool,
    ffi: FfiLinkOptions,
}

pub fn run(args: Vec<String>) {
    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{}", message);
            eprintln!("{}", usage());
            process::exit(1);
        }
    };

    if options
        .input_path
        .extension()
        .map(|e| e != "as")
        .unwrap_or(true)
    {
        eprintln!(
            "警告: 输入文件扩展名不是 .as: {}",
            options.input_path.display()
        );
    }

    let source = match fs::read_to_string(&options.input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("错误: 无法读取文件 {}: {}", options.input_path.display(), e);
            process::exit(1);
        }
    };

    compile_source(source, options);
}

fn compile_source(source: String, options: CompileOptions) {
    let input_path = options.input_path;
    let output_path = options.output_path;
    let ir_mode = options.ir_mode;
    let ffi_options = options.ffi;

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

    if ir_mode {
        module.print_to_stderr();
        return;
    }

    emit_and_link(&module, &output_path, &ffi_options);
}

fn emit_and_link(
    module: &inkwell::module::Module<'_>,
    output_path: &PathBuf,
    ffi: &FfiLinkOptions,
) {
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
        .write_to_file(module, FileType::Object, &obj_path)
        .expect("写入目标文件失败");

    let mut command = Command::new("cc");
    command.arg(&obj_path).arg(runtime_source_path());
    append_ffi_link_args(&mut command, ffi);
    command.arg("-o").arg(output_path);

    let status = command.status().expect("调用链接器失败");

    if !status.success() {
        eprintln!(
            "错误: 链接失败\n  = 帮助: 如果使用 FFI，请确认 `@声明 外部(\"...\")` 中的符号名存在，并用 `--ffi-lib` / `--ffi-search` / `--ffi-rpath` 传入 Rust 库和动态库搜索路径。"
        );
        let _ = fs::remove_file(&obj_path);
        process::exit(1);
    }

    let _ = fs::remove_file(&obj_path);
    println!("编译成功，生成二进制 → {}", output_path.display());
}

fn append_ffi_link_args(command: &mut Command, ffi: &FfiLinkOptions) {
    for search_path in &ffi.search_paths {
        command.arg(format!("-L{}", search_path.display()));
    }
    for rpath in &ffi.rpaths {
        append_rpath_arg(command, rpath);
    }
    for lib in &ffi.libs {
        command.arg(lib);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn append_rpath_arg(command: &mut Command, rpath: &PathBuf) {
    command.arg(format!("-Wl,-rpath,{}", rpath.display()));
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn append_rpath_arg(_command: &mut Command, _rpath: &PathBuf) {}

fn runtime_source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/std_io.c")
}

fn parse_args(args: &[String]) -> Result<CompileOptions, String> {
    if args.len() < 2 {
        return Err("错误: 缺少源文件".to_string());
    }

    let input_path = PathBuf::from(&args[1]);
    let mut output_path = default_output_path(&input_path);
    let mut ir_mode = false;
    let mut ffi = FfiLinkOptions::default();

    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--ir" => {
                ir_mode = true;
                index += 1;
            }
            "-o" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "错误: `-o` 后缺少输出文件路径".to_string())?;
                output_path = PathBuf::from(value);
                index += 2;
            }
            "--ffi-lib" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "错误: `--ffi-lib` 后缺少库文件路径".to_string())?;
                ffi.libs.push(PathBuf::from(value));
                index += 2;
            }
            "--ffi-search" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "错误: `--ffi-search` 后缺少目录路径".to_string())?;
                ffi.search_paths.push(PathBuf::from(value));
                index += 2;
            }
            "--ffi-rpath" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "错误: `--ffi-rpath` 后缺少目录路径".to_string())?;
                ffi.rpaths.push(PathBuf::from(value));
                index += 2;
            }
            other => {
                return Err(format!("错误: 未识别的参数 `{}`", other));
            }
        }
    }

    if ir_mode {
        output_path = PathBuf::new();
    }

    Ok(CompileOptions {
        input_path,
        output_path,
        ir_mode,
        ffi,
    })
}

fn default_output_path(input_path: &PathBuf) -> PathBuf {
    let mut p = input_path.clone();
    p.set_extension("");
    PathBuf::from(p.file_name().unwrap_or(std::ffi::OsStr::new("a.out")))
}

fn usage() -> &'static str {
    "用法: asc <源文件.as> [-o <输出文件>] [--ir] [--ffi-lib <库路径>] [--ffi-search <目录>] [--ffi-rpath <目录>]"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn test_parse_repeated_ffi_link_args() {
        let options = parse_args(&args(&[
            "asc",
            "demo/ffi.as",
            "-o",
            "demo/ffi",
            "--ffi-lib",
            "target/debug/libdemo.a",
            "--ffi-lib",
            "target/debug/libdemo.dylib",
            "--ffi-search",
            "target/debug",
            "--ffi-rpath",
            "target/debug",
        ]))
        .expect("parse args failed");

        assert_eq!(options.input_path, PathBuf::from("demo/ffi.as"));
        assert_eq!(options.output_path, PathBuf::from("demo/ffi"));
        assert!(!options.ir_mode);
        assert_eq!(
            options.ffi.libs,
            vec![
                PathBuf::from("target/debug/libdemo.a"),
                PathBuf::from("target/debug/libdemo.dylib")
            ]
        );
        assert_eq!(
            options.ffi.search_paths,
            vec![PathBuf::from("target/debug")]
        );
        assert_eq!(options.ffi.rpaths, vec![PathBuf::from("target/debug")]);
    }

    #[test]
    fn test_parse_ffi_arg_requires_value() {
        let err = parse_args(&args(&["asc", "demo/ffi.as", "--ffi-lib"]))
            .expect_err("expected missing value error");
        assert!(err.contains("--ffi-lib"));
    }
}
