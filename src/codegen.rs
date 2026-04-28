/// Code generation: walks the AST and emits LLVM IR via inkwell.
use crate::parser::{
    BinaryOp, ExecuteStmt, Expr, FormatPart, FunctionDef, IfStmt, ImportDecl, Program, Stmt, Type,
    UnaryOp, VarDecl,
};
use crate::semantic;
use crate::stdlib;
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::types::BasicType;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use std::collections::HashMap;

#[derive(Clone)]
enum LocalValue<'ctx> {
    Pointer(PointerValue<'ctx>, Type),
    Value(BasicValueEnum<'ctx>, Type),
}

/// Generate LLVM IR for the entire program.
pub fn generate<'ctx>(
    program: &Program,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
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
        Type::String => context.ptr_type(AddressSpace::from(0u16)).into(),
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
    } else if func.is_external {
        stdlib::external_symbol_for(&semantic::function_path(func))
            .unwrap_or_else(|| sanitize_llvm_name(&semantic::function_path(func)))
    } else {
        sanitize_llvm_name(&semantic::function_path(func))
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
    if func.is_external {
        return Ok(());
    }

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
    let mut locals = HashMap::new();
    for (i, param) in func.params.iter().enumerate() {
        if let Some(value) = function.get_nth_param(i as u32) {
            locals.insert(
                param.name.clone(),
                LocalValue::Value(value, param.param_type.clone()),
            );
        }
    }

    for stmt in &func.body {
        compile_stmt(
            stmt,
            func,
            program,
            &mut scoped_imports,
            &mut locals,
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
            Type::String => {
                let _ = builder.build_return(Some(
                    &context.ptr_type(AddressSpace::from(0u16)).const_null(),
                ));
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
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    match stmt {
        Stmt::VarDecl(var) => compile_var_decl(var, locals, builder, context, module),
        Stmt::Import(import) => {
            scoped_imports.push(import.clone());
            Ok(())
        }
        Stmt::Execute(exec) => compile_execute(
            exec,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Stmt::If(if_stmt) => compile_if_stmt(
            if_stmt,
            current_func,
            program,
            scoped_imports,
            locals,
            function,
            builder,
            context,
            module,
        ),
    }
}

/// Compile a variable declaration: alloca + store initializer.
fn compile_var_decl<'ctx>(
    var: &VarDecl,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    // Determine the LLVM type
    let var_type = match &var.var_type {
        Some(t) => t.clone(),
        None => infer_type_from_expr(&var.init, locals)?,
    };
    let llvm_type = as_llvm_type(&var_type, context);

    // Alloca
    let sanitized_name = sanitize_llvm_name(&var.name);
    let alloca = builder
        .build_alloca(llvm_type, &sanitized_name)
        .map_err(|e| format!("build_alloca failed: {:?}", e))?;

    // Compile initializer value and store
    let value = compile_expr(&var.init, locals, builder, context, module)?;
    let _ = builder.build_store(alloca, value);
    locals.insert(var.name.clone(), LocalValue::Pointer(alloca, var_type));

    Ok(())
}

fn compile_if_stmt<'ctx>(
    if_stmt: &IfStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let merge_block = context.append_basic_block(*function, "if.end");
    let mut next_condition_block = None;
    let mut else_block_to_compile = None;

    for (index, branch) in if_stmt.branches.iter().enumerate() {
        if let Some(block) = next_condition_block.take() {
            builder.position_at_end(block);
        }

        let then_block = context.append_basic_block(*function, &format!("if.then.{}", index));
        let else_block = if index + 1 == if_stmt.branches.len() {
            if if_stmt.else_body.is_some() {
                let block = context.append_basic_block(*function, "if.else");
                else_block_to_compile = Some(block);
                block
            } else {
                merge_block
            }
        } else {
            let block = context.append_basic_block(*function, &format!("if.cond.{}", index + 1));
            next_condition_block = Some(block);
            block
        };

        let condition = compile_bool_expr(&branch.condition, locals, builder, context, module)?;
        builder
            .build_conditional_branch(condition, then_block, else_block)
            .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?;

        builder.position_at_end(then_block);
        let mut branch_imports = scoped_imports.clone();
        let mut branch_locals = locals.clone();
        for stmt in &branch.body {
            compile_stmt(
                stmt,
                current_func,
                program,
                &mut branch_imports,
                &mut branch_locals,
                function,
                builder,
                context,
                module,
            )?;
        }
        if builder
            .get_insert_block()
            .and_then(|block| block.get_terminator())
            .is_none()
        {
            builder
                .build_unconditional_branch(merge_block)
                .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;
        }
    }

    if let Some(else_body) = &if_stmt.else_body {
        if let Some(block) = else_block_to_compile {
            builder.position_at_end(block);
        }
        let mut branch_imports = scoped_imports.clone();
        let mut branch_locals = locals.clone();
        for stmt in else_body {
            compile_stmt(
                stmt,
                current_func,
                program,
                &mut branch_imports,
                &mut branch_locals,
                function,
                builder,
                context,
                module,
            )?;
        }
        if builder
            .get_insert_block()
            .and_then(|block| block.get_terminator())
            .is_none()
        {
            builder
                .build_unconditional_branch(merge_block)
                .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;
        }
    }

    builder.position_at_end(merge_block);
    Ok(())
}

/// Compile an expression, returning the LLVM value.
fn compile_expr<'ctx>(
    expr: &Expr,
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match expr {
        Expr::IntLiteral(val) => Ok(context.i32_type().const_int(*val as u64, true).into()),
        Expr::Ident(name) => {
            let local = locals
                .get(name)
                .ok_or_else(|| format!("未定义的变量: {}", name))?;
            load_local_value(local.clone(), name, builder, context)
        }
        Expr::StringLiteral(text) => {
            let global = builder
                .build_global_string_ptr(text, "as.str")
                .map_err(|e| format!("build_global_string_ptr failed: {:?}", e))?;
            Ok(global.as_pointer_value().into())
        }
        Expr::FormattedString(parts) => {
            compile_formatted_string(parts, locals, builder, context, module)
        }
        Expr::Unary { op, expr } => {
            let value = compile_expr(expr, locals, builder, context, module)?;
            match op {
                UnaryOp::Neg => {
                    let int_value = value.into_int_value();
                    builder
                        .build_int_neg(int_value, "negtmp")
                        .map(Into::into)
                        .map_err(|e| format!("build_int_neg failed: {:?}", e))
                }
                UnaryOp::Not => compile_bool_expr(expr, locals, builder, context, module).and_then(
                    |bool_value| {
                        builder
                            .build_not(bool_value, "nottmp")
                            .map(Into::into)
                            .map_err(|e| format!("build_not failed: {:?}", e))
                    },
                ),
            }
        }
        Expr::Binary { left, op, right } => match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let lhs = compile_expr(left, locals, builder, context, module)?.into_int_value();
                let rhs = compile_expr(right, locals, builder, context, module)?.into_int_value();
                let result = match op {
                    BinaryOp::Add => builder.build_int_add(lhs, rhs, "addtmp"),
                    BinaryOp::Sub => builder.build_int_sub(lhs, rhs, "subtmp"),
                    BinaryOp::Mul => builder.build_int_mul(lhs, rhs, "multmp"),
                    BinaryOp::Div => builder.build_int_signed_div(lhs, rhs, "divtmp"),
                    BinaryOp::Rem => builder.build_int_signed_rem(lhs, rhs, "remtmp"),
                    _ => unreachable!(),
                };
                result
                    .map(Into::into)
                    .map_err(|e| format!("integer operation failed: {:?}", e))
            }
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Less
            | BinaryOp::LessEq
            | BinaryOp::Greater
            | BinaryOp::GreaterEq => {
                let lhs = compile_expr(left, locals, builder, context, module)?.into_int_value();
                let rhs = compile_expr(right, locals, builder, context, module)?.into_int_value();
                let pred = match op {
                    BinaryOp::Eq => IntPredicate::EQ,
                    BinaryOp::NotEq => IntPredicate::NE,
                    BinaryOp::Less => IntPredicate::SLT,
                    BinaryOp::LessEq => IntPredicate::SLE,
                    BinaryOp::Greater => IntPredicate::SGT,
                    BinaryOp::GreaterEq => IntPredicate::SGE,
                    _ => unreachable!(),
                };
                builder
                    .build_int_compare(pred, lhs, rhs, "cmptmp")
                    .map(Into::into)
                    .map_err(|e| format!("build_int_compare failed: {:?}", e))
            }
            BinaryOp::And | BinaryOp::Or => {
                compile_logical_expr(left, op, right, locals, builder, context, module)
            }
        },
    }
}

fn compile_logical_expr<'ctx>(
    left: &Expr,
    op: &BinaryOp,
    right: &Expr,
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let left_value = compile_bool_expr(left, locals, builder, context, module)?;
    let left_block = builder
        .get_insert_block()
        .ok_or_else(|| "Missing insert block for logical expression".to_string())?;
    let function = left_block
        .get_parent()
        .ok_or_else(|| "Missing parent function for logical expression".to_string())?;

    let right_block = context.append_basic_block(function, "logic.rhs");
    let end_block = context.append_basic_block(function, "logic.end");
    match op {
        BinaryOp::And => builder
            .build_conditional_branch(left_value, right_block, end_block)
            .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?,
        BinaryOp::Or => builder
            .build_conditional_branch(left_value, end_block, right_block)
            .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?,
        _ => unreachable!(),
    };

    builder.position_at_end(right_block);
    let right_value = compile_bool_expr(right, locals, builder, context, module)?;
    let right_end_block = builder
        .get_insert_block()
        .ok_or_else(|| "Missing right-hand block for logical expression".to_string())?;
    builder
        .build_unconditional_branch(end_block)
        .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;

    builder.position_at_end(end_block);
    let fallback = match op {
        BinaryOp::And => context.bool_type().const_int(0, false),
        BinaryOp::Or => context.bool_type().const_int(1, false),
        _ => unreachable!(),
    };
    let phi = builder
        .build_phi(context.bool_type(), "logicaltmp")
        .map_err(|e| format!("build_phi failed: {:?}", e))?;
    phi.add_incoming(&[(&fallback, left_block), (&right_value, right_end_block)]);
    Ok(phi.as_basic_value())
}

fn compile_bool_expr<'ctx>(
    expr: &Expr,
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, String> {
    let value = compile_expr(expr, locals, builder, context, module)?.into_int_value();
    if value.get_type().get_bit_width() == 1 {
        Ok(value)
    } else {
        builder
            .build_int_compare(
                IntPredicate::NE,
                value,
                value.get_type().const_zero(),
                "booltmp",
            )
            .map_err(|e| format!("build_int_compare failed: {:?}", e))
    }
}

/// Infer LLVM type from an expression (for type inference).
fn infer_type_from_expr<'ctx>(
    expr: &Expr,
    locals: &HashMap<String, LocalValue<'ctx>>,
) -> Result<Type, String> {
    match expr {
        Expr::IntLiteral(_) => Ok(Type::Int),
        Expr::Ident(name) => locals
            .get(name)
            .map(local_type)
            .ok_or_else(|| format!("未定义的变量: {}", name)),
        Expr::StringLiteral(_) | Expr::FormattedString(_) => {
            if let Expr::FormattedString(parts) = expr {
                for part in parts {
                    if let FormatPart::Placeholder(name) = part {
                        locals
                            .get(name)
                            .ok_or_else(|| format!("未定义的格式化变量: {}", name))?;
                    }
                }
            }
            Ok(Type::String)
        }
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => infer_type_from_expr(expr, locals),
            UnaryOp::Not => Ok(Type::Bool),
        },
        Expr::Binary { op, .. } => match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                Ok(Type::Int)
            }
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Less
            | BinaryOp::LessEq
            | BinaryOp::Greater
            | BinaryOp::GreaterEq
            | BinaryOp::And
            | BinaryOp::Or => Ok(Type::Bool),
        },
    }
}

fn compile_formatted_string<'ctx>(
    parts: &[FormatPart],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let mut format = String::new();
    let mut args = Vec::new();

    for part in parts {
        match part {
            FormatPart::Text(text) => format.push_str(&text.replace('%', "%%")),
            FormatPart::Placeholder(name) => {
                let local = locals
                    .get(name)
                    .ok_or_else(|| format!("未定义的格式化变量: {}", name))?;
                match local_type(local) {
                    Type::Int => format.push_str("%d"),
                    Type::String => format.push_str("%s"),
                    other => {
                        return Err(format!(
                            "暂不支持在格式化字符串中使用 {:?}: {}",
                            other, name
                        ));
                    }
                }
                args.push(load_local_value(local.clone(), name, builder, context)?);
            }
        }
    }

    let format_ptr = builder
        .build_global_string_ptr(&format, "as.fmt")
        .map_err(|e| format!("build_global_string_ptr failed: {:?}", e))?
        .as_pointer_value();

    let as_format = declare_format_function(context, module);
    let mut call_args: Vec<BasicMetadataValueEnum> = vec![format_ptr.into()];
    call_args.extend(args.into_iter().map(BasicMetadataValueEnum::from));

    let value = builder
        .build_call(as_format, &call_args, "as.format")
        .map_err(|e| format!("build_call failed: {:?}", e))?
        .try_as_basic_value()
        .unwrap_basic();

    Ok(value)
}

fn local_type<'ctx>(local: &LocalValue<'ctx>) -> Type {
    match local {
        LocalValue::Pointer(_, ty) | LocalValue::Value(_, ty) => ty.clone(),
    }
}

fn load_local_value<'ctx>(
    local: LocalValue<'ctx>,
    name: &str,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
) -> Result<BasicValueEnum<'ctx>, String> {
    match local {
        LocalValue::Pointer(ptr, ty) => builder
            .build_load(
                as_llvm_type(&ty, context),
                ptr,
                &format!("load.{}", sanitize_llvm_name(name)),
            )
            .map_err(|e| format!("build_load failed: {:?}", e)),
        LocalValue::Value(value, _) => Ok(value),
    }
}

fn declare_format_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> FunctionValue<'ctx> {
    if let Some(function) = module.get_function("as_format") {
        return function;
    }

    let ptr_type = context.ptr_type(AddressSpace::from(0u16));
    let fn_type = ptr_type.fn_type(&[ptr_type.into()], true);
    module.add_function("as_format", fn_type, None)
}

fn compile_execute<'ctx>(
    exec: &ExecuteStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let resolved =
        semantic::resolve_execute_target(program, current_func, scoped_imports, &exec.target)
            .ok_or_else(|| format!("未找到模块或方法: {}", exec.target))?;

    let function = program
        .functions
        .iter()
        .find(|func| semantic::function_path(func) == resolved)
        .and_then(|func| module.get_function(&llvm_function_name(func)))
        .ok_or_else(|| format!("未找到方法: {}", resolved))?;

    let args: Vec<BasicMetadataValueEnum> = exec
        .args
        .iter()
        .map(|arg| compile_expr(arg, locals, builder, context, module).map(Into::into))
        .collect::<Result<_, _>>()?;

    builder
        .build_call(function, &args, "call")
        .map_err(|e| format!("build_call failed: {:?}", e))?;

    Ok(())
}
