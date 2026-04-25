/// Code generation: walks the AST and emits LLVM IR via inkwell.
use crate::parser::{ExecuteStmt, Expr, FunctionDef, ImportDecl, Program, Stmt, Type, VarDecl};
use crate::semantic::{self, STD_OUTPUT_FUNCTION};
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::types::BasicType;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};

/// Generate LLVM IR for the entire program.
pub fn generate<'ctx>(
    program: &Program,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    declare_standard_library(context, module);
    for func in &program.functions {
        declare_function(func, context, module)?;
    }
    for func in &program.functions {
        compile_function(func, program, context, module)?;
    }
    Ok(())
}

/// Map a 问源 type to an LLVM basic type.
fn as_llvm_type<'ctx>(ty: &Type, context: &'ctx Context) -> BasicTypeEnum<'ctx> {
    match ty {
        Type::Void => context.i32_type().into(),
        Type::Int => context.i32_type().into(),
        Type::Double => context.f64_type().into(),
        Type::Float => context.f32_type().into(),
        Type::Bool => context.bool_type().into(),
        Type::Char => context.i8_type().into(),
    }
}

/// Sanitize a variable/function name for LLVM IR identifiers.
/// LLVM identifiers must match `[%@][-a-zA-Z$._][-a-zA-Z$._0-9]*`.
/// Chinese characters are encoded as their Unicode code point.
fn sanitize_llvm_name(name: &str) -> String {
    // If the name is already a valid LLVM identifier, keep it as-is
    if !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'$' || b == b'-')
    {
        return name.to_string();
    }
    // Encode non-ASCII characters
    let mut s = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            s.push(ch);
        } else {
            s.push('_');
            s.push_str(&format!("{:x}", ch as u32));
            s.push('_');
        }
    }
    s
}

fn llvm_function_name(func: &FunctionDef) -> String {
    if func.is_entry {
        "main".to_string()
    } else {
        sanitize_llvm_name(&semantic::function_path(func))
    }
}

fn declare_standard_library<'ctx>(context: &'ctx Context, module: &Module<'ctx>) {
    if module.get_function("puts").is_none() {
        let ptr_type = context.ptr_type(AddressSpace::from(0u16));
        let puts_type = context.i32_type().fn_type(&[ptr_type.into()], false);
        module.add_function("puts", puts_type, None);
    }
}

fn declare_function<'ctx>(
    func: &FunctionDef,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<FunctionValue<'ctx>, String> {
    let llvm_name = llvm_function_name(func);
    if let Some(function) = module.get_function(&llvm_name) {
        return Ok(function);
    }

    let return_type: BasicTypeEnum = if func.is_entry {
        context.i32_type().into()
    } else {
        as_llvm_type(&func.return_type, context)
    };

    let param_types: Vec<BasicMetadataTypeEnum> = func
        .params
        .iter()
        .map(|p| as_llvm_type(&p.param_type, context).into())
        .collect();

    let fn_type = return_type.fn_type(&param_types, false);
    Ok(module.add_function(&llvm_name, fn_type, None))
}

/// Compile a single function definition to LLVM IR.
fn compile_function<'ctx>(
    func: &FunctionDef,
    program: &Program,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let builder = context.create_builder();

    let llvm_name = llvm_function_name(func);
    let function = module
        .get_function(&llvm_name)
        .ok_or_else(|| format!("Missing function declaration: {}", llvm_name))?;

    // Set parameter names
    for (i, param) in func.params.iter().enumerate() {
        if let Some(pv) = function.get_nth_param(i as u32) {
            pv.set_name(&sanitize_llvm_name(&param.name));
        }
    }

    // Create entry basic block
    let entry_block = context.append_basic_block(function, "entry");
    builder.position_at_end(entry_block);

    // Compile function body statements
    let mut scoped_imports = program.imports.clone();
    for stmt in &func.body {
        compile_stmt(
            stmt,
            func,
            program,
            &mut scoped_imports,
            &function,
            &builder,
            context,
            module,
        )?;
    }

    // Build return instruction
    if func.is_entry || func.return_type == Type::Void {
        let _ = builder.build_return(Some(&context.i32_type().const_int(0, false)));
    } else {
        match func.return_type {
            Type::Int => {
                let _ = builder.build_return(Some(&context.i32_type().const_int(0, false)));
            }
            Type::Double => {
                let _ = builder.build_return(Some(&context.f64_type().const_float(0.0)));
            }
            Type::Float => {
                let _ = builder.build_return(Some(&context.f32_type().const_float(0.0)));
            }
            Type::Bool => {
                let _ = builder.build_return(Some(&context.bool_type().const_int(0, false)));
            }
            Type::Char => {
                let _ = builder.build_return(Some(&context.i8_type().const_int(0, false)));
            }
            Type::Void => unreachable!(),
        }
    }

    Ok(())
}

/// Compile a single statement.
fn compile_stmt<'ctx>(
    stmt: &Stmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    _function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    match stmt {
        Stmt::VarDecl(var) => compile_var_decl(var, builder, context),
        Stmt::Import(import) => {
            scoped_imports.push(import.clone());
            Ok(())
        }
        Stmt::Execute(exec) => compile_execute(
            exec,
            current_func,
            program,
            scoped_imports,
            builder,
            context,
            module,
        ),
    }
}

/// Compile a variable declaration: alloca + store initializer.
fn compile_var_decl<'ctx>(
    var: &VarDecl,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
) -> Result<(), String> {
    // Determine the LLVM type
    let llvm_type = match &var.var_type {
        Some(t) => as_llvm_type(t, context),
        None => infer_type_from_expr(&var.init, context),
    };

    // Alloca
    let sanitized_name = sanitize_llvm_name(&var.name);
    let alloca = builder
        .build_alloca(llvm_type, &sanitized_name)
        .map_err(|e| format!("build_alloca failed: {:?}", e))?;

    // Compile initializer value and store
    let value = compile_expr(&var.init, context)?;
    let _ = builder.build_store(alloca, value);

    Ok(())
}

/// Compile an expression, returning the LLVM value.
fn compile_expr<'ctx>(expr: &Expr, context: &'ctx Context) -> Result<BasicValueEnum<'ctx>, String> {
    match expr {
        Expr::IntLiteral(val) => Ok(context.i32_type().const_int(*val as u64, true).into()),
        Expr::StringLiteral(_) => {
            Err("String literals are only supported as execute arguments".to_string())
        }
    }
}

/// Infer LLVM type from an expression (for type inference).
fn infer_type_from_expr<'ctx>(expr: &Expr, context: &'ctx Context) -> BasicTypeEnum<'ctx> {
    match expr {
        Expr::IntLiteral(_) => context.i32_type().into(),
        Expr::StringLiteral(_) => context.ptr_type(AddressSpace::from(0u16)).into(),
    }
}

fn compile_execute<'ctx>(
    exec: &ExecuteStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let resolved =
        semantic::resolve_execute_target(program, current_func, scoped_imports, &exec.target)
            .ok_or_else(|| format!("未找到模块或方法: {}", exec.target))?;

    if resolved == STD_OUTPUT_FUNCTION {
        return compile_std_output(exec, builder, module);
    }

    let function = program
        .functions
        .iter()
        .find(|func| semantic::function_path(func) == resolved)
        .and_then(|func| module.get_function(&llvm_function_name(func)))
        .ok_or_else(|| format!("未找到方法: {}", resolved))?;

    let args: Vec<BasicMetadataValueEnum> = exec
        .args
        .iter()
        .map(|arg| compile_expr(arg, context).map(Into::into))
        .collect::<Result<_, _>>()?;

    builder
        .build_call(function, &args, "call")
        .map_err(|e| format!("build_call failed: {:?}", e))?;

    Ok(())
}

fn compile_std_output<'ctx>(
    exec: &ExecuteStmt,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
) -> Result<(), String> {
    if exec.args.len() != 1 {
        return Err("标准库-输入输出-输出 expects exactly one argument".to_string());
    }

    let text = match &exec.args[0] {
        Expr::StringLiteral(text) => text,
        _ => return Err("标准库-输入输出-输出 currently expects a string literal".to_string()),
    };

    let puts = module
        .get_function("puts")
        .ok_or_else(|| "Missing standard library function: puts".to_string())?;
    let global = builder
        .build_global_string_ptr(text, "as.str")
        .map_err(|e| format!("build_global_string_ptr failed: {:?}", e))?;
    let arg = global.as_pointer_value().into();

    builder
        .build_call(puts, &[arg], "puts_call")
        .map_err(|e| format!("build_call failed: {:?}", e))?;

    Ok(())
}
