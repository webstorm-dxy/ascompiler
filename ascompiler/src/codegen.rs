/// Code generation: walks the AST and emits LLVM IR via inkwell.
use crate::parser::{
    ArrayAssignStmt, AssignStmt, BinaryOp, ExecuteStmt, Expr, FieldAssignStmt, FormatPart,
    FunctionDef, IfStmt, ImportDecl, LoopStmt, Program, ReturnStmt, SelectStmt, Stmt, Type,
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
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use std::collections::HashMap;

#[derive(Clone)]
enum LocalValue<'ctx> {
    Pointer(PointerValue<'ctx>, Type, bool, bool),
}

/// Generate LLVM IR for the entire program.
pub fn generate<'ctx>(
    program: &Program,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    for func in &program.functions {
        declare_function(func, program, context, module)?;
    }
    for func in &program.functions {
        compile_function(func, program, context, module)?;
    }
    Ok(())
}

/// Map a 问源 type to an LLVM basic type.
fn as_llvm_type<'ctx>(
    ty: &Type,
    context: &'ctx Context,
    program: &Program,
) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Type::Void => Ok(context.i32_type().into()),
        Type::Int => Ok(context.i32_type().into()),
        Type::Double => Ok(context.f64_type().into()),
        Type::Float => Ok(context.f32_type().into()),
        Type::Bool => Ok(context.bool_type().into()),
        Type::Char => Ok(context.i8_type().into()),
        Type::String => Ok(context.ptr_type(AddressSpace::from(0u16)).into()),
        Type::Struct(name) => {
            if is_object_name(program, name) {
                return Ok(context.ptr_type(AddressSpace::from(0u16)).into());
            }
            let struct_def = program
                .structs
                .iter()
                .find(|candidate| candidate.name == *name)
                .ok_or_else(|| format!("未定义的结构: {}", name))?;
            let field_types = struct_def
                .fields
                .iter()
                .map(|field| as_llvm_type(&field.field_type, context, program))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(context.struct_type(&field_types, false).into())
        }
        Type::Array {
            element_type,
            length,
        } => Ok(as_llvm_type(element_type, context, program)?
            .array_type(length.expect("array type must have a known length") as u32)
            .into()),
    }
}

fn is_object_name(program: &Program, name: &str) -> bool {
    program.objects.iter().any(|object| object.name == name)
}

fn find_object_method<'a>(
    program: &'a Program,
    object_name: &str,
    method: &str,
) -> Option<&'a crate::parser::ObjectMethod> {
    program
        .objects
        .iter()
        .find(|object| object.name == object_name)?
        .methods
        .iter()
        .find(|candidate| candidate.function.name == method)
}

fn aggregate_fields<'a>(
    program: &'a Program,
    name: &str,
) -> Result<&'a [crate::parser::StructField], String> {
    if let Some(struct_def) = program
        .structs
        .iter()
        .find(|candidate| candidate.name == name)
    {
        return Ok(&struct_def.fields);
    }
    if let Some(object) = program
        .objects
        .iter()
        .find(|candidate| candidate.name == name)
    {
        return Ok(&object.fields);
    }
    Err(format!("未定义的结构或对象: {}", name))
}

fn aggregate_struct_type<'ctx>(
    name: &str,
    context: &'ctx Context,
    program: &Program,
) -> Result<inkwell::types::StructType<'ctx>, String> {
    let fields = aggregate_fields(program, name)?;
    let field_types = fields
        .iter()
        .map(|field| as_llvm_type(&field.field_type, context, program))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(context.struct_type(&field_types, false))
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
    } else if let Some(symbol) = &func.external_symbol {
        symbol.clone()
    } else if func.is_external {
        stdlib::external_symbol_for(&semantic::function_path(func))
            .unwrap_or_else(|| sanitize_llvm_name(&semantic::function_path(func)))
    } else {
        sanitize_llvm_name(&semantic::function_path(func))
    }
}

fn declare_function<'ctx>(
    func: &FunctionDef,
    program: &Program,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<FunctionValue<'ctx>, String> {
    let llvm_name = llvm_function_name(func);
    if let Some(function) = module.get_function(&llvm_name) {
        return Ok(function);
    }

    let param_types: Vec<BasicMetadataTypeEnum> = func
        .params
        .iter()
        .map(|p| Ok(as_llvm_type(&p.param_type, context, program)?.into()))
        .collect::<Result<_, String>>()?;

    let fn_type = if func.is_entry {
        context.i32_type().fn_type(&param_types, false)
    } else if func.return_type == Type::Void {
        context.void_type().fn_type(&param_types, false)
    } else {
        as_llvm_type(&func.return_type, context, program)?.fn_type(&param_types, false)
    };
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
            let alloca = builder
                .build_alloca(
                    as_llvm_type(&param.param_type, context, program)?,
                    &sanitize_llvm_name(&param.name),
                )
                .map_err(|e| format!("build_alloca failed: {:?}", e))?;
            builder
                .build_store(alloca, value)
                .map_err(|e| format!("build_store failed: {:?}", e))?;
            locals.insert(
                param.name.clone(),
                LocalValue::Pointer(alloca, param.param_type.clone(), false, false),
            );
        }
    }

    for stmt in &func.body {
        if builder
            .get_insert_block()
            .and_then(|block| block.get_terminator())
            .is_some()
        {
            break;
        }
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

    if builder
        .get_insert_block()
        .and_then(|block| block.get_terminator())
        .is_none()
    {
        free_owned_objects(&locals, program, &builder, context)?;
        // Build default return instruction
        if func.is_entry {
            let _ = builder.build_return(Some(&context.i32_type().const_int(0, false)));
        } else if func.return_type == Type::Void {
            let _ = builder.build_return(None);
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
                Type::Array { .. } => {
                    let _ = builder.build_return(Some(
                        &as_llvm_type(&func.return_type, context, program)?.const_zero(),
                    ));
                }
                Type::Struct(_) => {
                    let _ = builder.build_return(Some(
                        &as_llvm_type(&func.return_type, context, program)?.const_zero(),
                    ));
                }
                Type::Void => unreachable!(),
            }
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
        Stmt::VarDecl(var) => compile_var_decl(
            var,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Stmt::Assign(assign) => compile_assign_stmt(
            assign,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Stmt::ArrayAssign(assign) => compile_array_assign_stmt(
            assign,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Stmt::FieldAssign(assign) => compile_field_assign_stmt(
            assign,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Stmt::Return(ret) => compile_return_stmt(
            ret,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
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
        Stmt::Select(select_stmt) => compile_select_stmt(
            select_stmt,
            current_func,
            program,
            scoped_imports,
            locals,
            function,
            builder,
            context,
            module,
        ),
        Stmt::Loop(loop_stmt) => compile_loop_stmt(
            loop_stmt,
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
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    // Determine the LLVM type
    let var_type = resolve_var_type(var, current_func, program, scoped_imports, locals)?;
    let llvm_type = as_llvm_type(&var_type, context, program)?;

    // Alloca
    let sanitized_name = sanitize_llvm_name(&var.name);
    let alloca = builder
        .build_alloca(llvm_type, &sanitized_name)
        .map_err(|e| format!("build_alloca failed: {:?}", e))?;

    // Compile initializer value and store when this declaration has one.
    if let Some(init) = &var.init {
        let value = compile_expr(
            init,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        builder
            .build_store(alloca, value)
            .map_err(|e| format!("build_store failed: {:?}", e))?;
    }
    locals.insert(
        var.name.clone(),
        LocalValue::Pointer(
            alloca,
            var_type.clone(),
            var.is_mutable,
            matches!(var.init, Some(Expr::ObjectCreate { .. })),
        ),
    );

    Ok(())
}

fn compile_assign_stmt<'ctx>(
    assign: &AssignStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let local = locals
        .get(&assign.name)
        .cloned()
        .ok_or_else(|| format!("未定义的变量: {}", assign.name))?;
    let LocalValue::Pointer(ptr, _, is_mutable, _) = local;
    if !is_mutable {
        return Err(format!("不可变变量不能重新赋值: {}", assign.name));
    }
    let value = compile_expr(
        &assign.value,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_store(ptr, value)
        .map(|_| ())
        .map_err(|e| format!("build_store failed: {:?}", e))
}

fn compile_array_assign_stmt<'ctx>(
    assign: &ArrayAssignStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let (element_ptr, element_type) = compile_array_element_ptr(
        &assign.name,
        &assign.index,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    let value = compile_expr(
        &assign.value,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_store(element_ptr, value)
        .map(|_| ())
        .map_err(|e| {
            format!(
                "build_store array element failed for {}: {:?}",
                type_name(&element_type),
                e
            )
        })
}

fn compile_field_assign_stmt<'ctx>(
    assign: &FieldAssignStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let (field_ptr, field_type) = compile_field_ptr(
        &assign.base,
        &assign.field,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    let value = compile_expr(
        &assign.value,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_store(field_ptr, value)
        .map(|_| ())
        .map_err(|e| {
            format!(
                "build_store field failed for {}: {:?}",
                type_name(&field_type),
                e
            )
        })
}

fn compile_return_stmt<'ctx>(
    ret: &ReturnStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    if current_func.is_entry {
        free_owned_objects(locals, program, builder, context)?;
        return builder
            .build_return(Some(&context.i32_type().const_int(0, false)))
            .map(|_| ())
            .map_err(|e| format!("build_return failed: {:?}", e));
    }
    if current_func.return_type == Type::Void {
        free_owned_objects(locals, program, builder, context)?;
        return builder
            .build_return(None)
            .map(|_| ())
            .map_err(|e| format!("build_return failed: {:?}", e));
    }

    let expr = ret
        .value
        .as_ref()
        .ok_or_else(|| "返回语句缺少返回值".to_string())?;
    let value = compile_expr(
        expr,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    free_owned_objects(locals, program, builder, context)?;
    builder
        .build_return(Some(&value))
        .map(|_| ())
        .map_err(|e| format!("build_return failed: {:?}", e))
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

        let condition = compile_bool_expr(
            &branch.condition,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
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

fn compile_select_stmt<'ctx>(
    select_stmt: &SelectStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let target_local = locals
        .get(&select_stmt.target)
        .cloned()
        .ok_or_else(|| format!("未定义的变量: {}", select_stmt.target))?;
    let target_type = local_type(&target_local);
    let merge_block = context.append_basic_block(*function, "select.end");
    let mut next_case_block = None;
    let mut default_block_to_compile = None;

    for (index, case) in select_stmt.cases.iter().enumerate() {
        if let Some(block) = next_case_block.take() {
            builder.position_at_end(block);
        }

        let case_body_block =
            context.append_basic_block(*function, &format!("select.case.{}", index));
        let fallback_block = if index + 1 == select_stmt.cases.len() {
            if select_stmt.default_body.is_some() {
                let block = context.append_basic_block(*function, "select.default");
                default_block_to_compile = Some(block);
                block
            } else {
                merge_block
            }
        } else {
            let block =
                context.append_basic_block(*function, &format!("select.check.{}", index + 1));
            next_case_block = Some(block);
            block
        };

        let condition = compile_select_case_condition(
            &target_local,
            &target_type,
            &case.value,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        builder
            .build_conditional_branch(condition, case_body_block, fallback_block)
            .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?;

        builder.position_at_end(case_body_block);
        let mut branch_imports = scoped_imports.clone();
        let mut branch_locals = locals.clone();
        for stmt in &case.body {
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

    if let Some(default_body) = &select_stmt.default_body {
        if let Some(block) = default_block_to_compile {
            builder.position_at_end(block);
        }
        let mut branch_imports = scoped_imports.clone();
        let mut branch_locals = locals.clone();
        for stmt in default_body {
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

fn compile_select_case_condition<'ctx>(
    target_local: &LocalValue<'ctx>,
    target_type: &Type,
    case_value: &Expr,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<IntValue<'ctx>, String> {
    let target_value = load_local_value(
        target_local.clone(),
        "select.target",
        builder,
        context,
        program,
    )?;
    let case_value = compile_expr(
        case_value,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;

    match target_type {
        Type::String => {
            let strcmp = declare_strcmp_function(context, module);
            let result = builder
                .build_call(
                    strcmp,
                    &[target_value.into(), case_value.into()],
                    "select.strcmp",
                )
                .map_err(|e| format!("build_call failed: {:?}", e))?
                .try_as_basic_value()
                .unwrap_basic()
                .into_int_value();
            builder
                .build_int_compare(
                    IntPredicate::EQ,
                    result,
                    context.i32_type().const_zero(),
                    "select.str.eq",
                )
                .map_err(|e| format!("build_int_compare failed: {:?}", e))
        }
        Type::Int | Type::Bool | Type::Char => builder
            .build_int_compare(
                IntPredicate::EQ,
                target_value.into_int_value(),
                case_value.into_int_value(),
                "select.eq",
            )
            .map_err(|e| format!("build_int_compare failed: {:?}", e)),
        other => Err(format!("暂不支持对 {} 类型使用选择语句", type_name(other))),
    }
}

fn compile_loop_stmt<'ctx>(
    loop_stmt: &LoopStmt,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    match loop_stmt {
        LoopStmt::Count {
            var_name,
            end,
            body,
        } => compile_count_loop(
            var_name,
            end,
            body,
            current_func,
            program,
            scoped_imports,
            locals,
            function,
            builder,
            context,
            module,
        ),
        LoopStmt::Iterate {
            var_name,
            start,
            end,
            body,
        } => compile_iterate_loop(
            var_name,
            start,
            end,
            body,
            current_func,
            program,
            scoped_imports,
            locals,
            function,
            builder,
            context,
            module,
        ),
        LoopStmt::Condition { condition, body } => compile_condition_loop(
            condition,
            body,
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

fn compile_count_loop<'ctx>(
    var_name: &str,
    end: &Expr,
    body: &[Stmt],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let sanitized_name = sanitize_llvm_name(var_name);
    let counter_alloca = builder
        .build_alloca(context.i32_type(), &sanitized_name)
        .map_err(|e| format!("build_alloca failed: {:?}", e))?;
    builder
        .build_store(counter_alloca, context.i32_type().const_zero())
        .map_err(|e| format!("build_store failed: {:?}", e))?;

    let condition_block = context.append_basic_block(*function, "loop.count.cond");
    let body_block = context.append_basic_block(*function, "loop.count.body");
    let end_block = context.append_basic_block(*function, "loop.count.end");

    builder
        .build_unconditional_branch(condition_block)
        .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;

    builder.position_at_end(condition_block);
    let counter_value = builder
        .build_load(
            context.i32_type(),
            counter_alloca,
            &format!("load.{}", sanitized_name),
        )
        .map_err(|e| format!("build_load failed: {:?}", e))?
        .into_int_value();
    let end_value = compile_expr(
        end,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?
    .into_int_value();
    let condition = builder
        .build_int_compare(
            IntPredicate::SLT,
            counter_value,
            end_value,
            "loop.count.cmp",
        )
        .map_err(|e| format!("build_int_compare failed: {:?}", e))?;
    builder
        .build_conditional_branch(condition, body_block, end_block)
        .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?;

    builder.position_at_end(body_block);
    let mut loop_imports = scoped_imports.clone();
    let mut loop_locals = locals.clone();
    loop_locals.insert(
        var_name.to_string(),
        LocalValue::Pointer(counter_alloca, Type::Int, false, false),
    );
    for stmt in body {
        compile_stmt(
            stmt,
            current_func,
            program,
            &mut loop_imports,
            &mut loop_locals,
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
        let current_counter = builder
            .build_load(
                context.i32_type(),
                counter_alloca,
                &format!("load.{}", sanitized_name),
            )
            .map_err(|e| format!("build_load failed: {:?}", e))?
            .into_int_value();
        let next_counter = builder
            .build_int_add(
                current_counter,
                context.i32_type().const_int(1, false),
                "loop.count.next",
            )
            .map_err(|e| format!("build_int_add failed: {:?}", e))?;
        builder
            .build_store(counter_alloca, next_counter)
            .map_err(|e| format!("build_store failed: {:?}", e))?;
        builder
            .build_unconditional_branch(condition_block)
            .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;
    }

    builder.position_at_end(end_block);
    Ok(())
}

fn compile_iterate_loop<'ctx>(
    var_name: &str,
    start: &Expr,
    end: &Expr,
    body: &[Stmt],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let sanitized_name = sanitize_llvm_name(var_name);
    let iterator_alloca = builder
        .build_alloca(context.i32_type(), &sanitized_name)
        .map_err(|e| format!("build_alloca failed: {:?}", e))?;
    let start_value = compile_expr(
        start,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?
    .into_int_value();
    builder
        .build_store(iterator_alloca, start_value)
        .map_err(|e| format!("build_store failed: {:?}", e))?;

    let condition_block = context.append_basic_block(*function, "loop.iterate.cond");
    let body_block = context.append_basic_block(*function, "loop.iterate.body");
    let end_block = context.append_basic_block(*function, "loop.iterate.end");

    builder
        .build_unconditional_branch(condition_block)
        .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;

    builder.position_at_end(condition_block);
    let iterator_value = builder
        .build_load(
            context.i32_type(),
            iterator_alloca,
            &format!("load.{}", sanitized_name),
        )
        .map_err(|e| format!("build_load failed: {:?}", e))?
        .into_int_value();
    let end_value = compile_expr(
        end,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?
    .into_int_value();
    let condition = builder
        .build_int_compare(
            IntPredicate::SLT,
            iterator_value,
            end_value,
            "loop.iterate.cmp",
        )
        .map_err(|e| format!("build_int_compare failed: {:?}", e))?;
    builder
        .build_conditional_branch(condition, body_block, end_block)
        .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?;

    builder.position_at_end(body_block);
    let mut loop_imports = scoped_imports.clone();
    let mut loop_locals = locals.clone();
    loop_locals.insert(
        var_name.to_string(),
        LocalValue::Pointer(iterator_alloca, Type::Int, false, false),
    );
    for stmt in body {
        compile_stmt(
            stmt,
            current_func,
            program,
            &mut loop_imports,
            &mut loop_locals,
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
        let current_iterator = builder
            .build_load(
                context.i32_type(),
                iterator_alloca,
                &format!("load.{}", sanitized_name),
            )
            .map_err(|e| format!("build_load failed: {:?}", e))?
            .into_int_value();
        let next_iterator = builder
            .build_int_add(
                current_iterator,
                context.i32_type().const_int(1, false),
                "loop.iterate.next",
            )
            .map_err(|e| format!("build_int_add failed: {:?}", e))?;
        builder
            .build_store(iterator_alloca, next_iterator)
            .map_err(|e| format!("build_store failed: {:?}", e))?;
        builder
            .build_unconditional_branch(condition_block)
            .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;
    }

    builder.position_at_end(end_block);
    Ok(())
}

fn compile_condition_loop<'ctx>(
    condition: &Expr,
    body: &[Stmt],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalValue<'ctx>>,
    function: &FunctionValue<'ctx>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let condition_block = context.append_basic_block(*function, "loop.condition.cond");
    let body_block = context.append_basic_block(*function, "loop.condition.body");
    let end_block = context.append_basic_block(*function, "loop.condition.end");

    builder
        .build_unconditional_branch(condition_block)
        .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;

    builder.position_at_end(condition_block);
    let condition_value = compile_bool_expr(
        condition,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_conditional_branch(condition_value, body_block, end_block)
        .map_err(|e| format!("build_conditional_branch failed: {:?}", e))?;

    builder.position_at_end(body_block);
    let mut loop_imports = scoped_imports.clone();
    let mut loop_locals = locals.clone();
    for stmt in body {
        compile_stmt(
            stmt,
            current_func,
            program,
            &mut loop_imports,
            &mut loop_locals,
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
            .build_unconditional_branch(condition_block)
            .map_err(|e| format!("build_unconditional_branch failed: {:?}", e))?;
    }

    builder.position_at_end(end_block);
    Ok(())
}

/// Compile an expression, returning the LLVM value.
fn compile_expr<'ctx>(
    expr: &Expr,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match expr {
        Expr::IntLiteral(val) => Ok(context.i32_type().const_int(*val as u64, true).into()),
        Expr::DoubleLiteral(val) => Ok(context.f64_type().const_float(*val).into()),
        Expr::Ident(name) => {
            let local = locals
                .get(name)
                .ok_or_else(|| format!("未定义的变量: {}", name))?;
            load_local_value(local.clone(), name, builder, context, program)
        }
        Expr::Call { target, args, .. } => compile_call_expr(
            target,
            args,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::StringLiteral(text) => {
            let global = builder
                .build_global_string_ptr(text, "as.str")
                .map_err(|e| format!("build_global_string_ptr failed: {:?}", e))?;
            Ok(global.as_pointer_value().into())
        }
        Expr::FormattedString(parts) => compile_formatted_string(
            parts,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::ArrayLiteral(elements) => compile_array_literal(
            elements,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::StructLiteral { name, fields } => compile_struct_literal(
            name,
            fields,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::ObjectCreate { name, args } => compile_object_create(
            name,
            args,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::Index { array, index } => compile_array_index(
            array,
            index,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::FieldAccess { base, field } => compile_field_access(
            base,
            field,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => compile_method_call(
            receiver,
            method,
            args,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        ),
        Expr::Unary { op, expr } => {
            let value = compile_expr(
                expr,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            )?;
            match op {
                UnaryOp::Neg => {
                    let value_type =
                        infer_type_from_expr(expr, current_func, program, scoped_imports, locals)?;
                    if value_type == Type::Double {
                        builder
                            .build_float_neg(value.into_float_value(), "fnegtmp")
                            .map(Into::into)
                            .map_err(|e| format!("build_float_neg failed: {:?}", e))
                    } else {
                        builder
                            .build_int_neg(value.into_int_value(), "negtmp")
                            .map(Into::into)
                            .map_err(|e| format!("build_int_neg failed: {:?}", e))
                    }
                }
                UnaryOp::Not => compile_bool_expr(
                    expr,
                    current_func,
                    program,
                    scoped_imports,
                    locals,
                    builder,
                    context,
                    module,
                )
                .and_then(|bool_value| {
                    builder
                        .build_not(bool_value, "nottmp")
                        .map(Into::into)
                        .map_err(|e| format!("build_not failed: {:?}", e))
                }),
            }
        }
        Expr::Binary { left, op, right } => match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let left_ty =
                    infer_type_from_expr(left, current_func, program, scoped_imports, locals)?;
                if left_ty == Type::Double {
                    let lhs = compile_expr(
                        left,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_float_value();
                    let rhs = compile_expr(
                        right,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_float_value();
                    let result = match op {
                        BinaryOp::Add => builder.build_float_add(lhs, rhs, "faddtmp"),
                        BinaryOp::Sub => builder.build_float_sub(lhs, rhs, "fsubtmp"),
                        BinaryOp::Mul => builder.build_float_mul(lhs, rhs, "fmultmp"),
                        BinaryOp::Div => builder.build_float_div(lhs, rhs, "fdivtmp"),
                        BinaryOp::Rem => builder.build_float_rem(lhs, rhs, "fremtmp"),
                        _ => unreachable!(),
                    };
                    result
                        .map(Into::into)
                        .map_err(|e| format!("float operation failed: {:?}", e))
                } else {
                    let lhs = compile_expr(
                        left,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_int_value();
                    let rhs = compile_expr(
                        right,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_int_value();
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
            }
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Less
            | BinaryOp::LessEq
            | BinaryOp::Greater
            | BinaryOp::GreaterEq => {
                let left_ty =
                    infer_type_from_expr(left, current_func, program, scoped_imports, locals)?;
                if left_ty == Type::Double {
                    let lhs = compile_expr(
                        left,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_float_value();
                    let rhs = compile_expr(
                        right,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_float_value();
                    let pred = match op {
                        BinaryOp::Eq => inkwell::FloatPredicate::OEQ,
                        BinaryOp::NotEq => inkwell::FloatPredicate::ONE,
                        BinaryOp::Less => inkwell::FloatPredicate::OLT,
                        BinaryOp::LessEq => inkwell::FloatPredicate::OLE,
                        BinaryOp::Greater => inkwell::FloatPredicate::OGT,
                        BinaryOp::GreaterEq => inkwell::FloatPredicate::OGE,
                        _ => unreachable!(),
                    };
                    builder
                        .build_float_compare(pred, lhs, rhs, "fcmptmp")
                        .map(Into::into)
                        .map_err(|e| format!("build_float_compare failed: {:?}", e))
                } else {
                    let lhs = compile_expr(
                        left,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_int_value();
                    let rhs = compile_expr(
                        right,
                        current_func,
                        program,
                        scoped_imports,
                        locals,
                        builder,
                        context,
                        module,
                    )?
                    .into_int_value();
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
            }
            BinaryOp::And | BinaryOp::Or => compile_logical_expr(
                left,
                op,
                right,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            ),
        },
    }
}

fn compile_call_expr<'ctx>(
    target: &str,
    args: &[Expr],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let resolved = semantic::resolve_execute_target(program, current_func, scoped_imports, target)
        .ok_or_else(|| format!("未找到模块或方法: {}", target))?;
    let called_func = program
        .functions
        .iter()
        .find(|func| semantic::function_path(func) == resolved)
        .ok_or_else(|| format!("未找到方法: {}", resolved))?;
    if called_func.return_type == Type::Void {
        return Err(format!("方法没有返回值: {}", target));
    }
    let function = module
        .get_function(&llvm_function_name(called_func))
        .ok_or_else(|| format!("未找到方法: {}", resolved))?;
    let call_args: Vec<BasicMetadataValueEnum> = args
        .iter()
        .map(|arg| {
            compile_expr(
                arg,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            )
            .map(Into::into)
        })
        .collect::<Result<_, _>>()?;

    Ok(builder
        .build_call(function, &call_args, "call.value")
        .map_err(|e| format!("build_call failed: {:?}", e))?
        .try_as_basic_value()
        .unwrap_basic())
}

fn compile_method_call<'ctx>(
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let receiver_type =
        infer_type_from_expr(receiver, current_func, program, scoped_imports, locals)?;
    let Type::Struct(object_name) = receiver_type else {
        return Err(format!(
            "不能对 {} 类型调用对象方法",
            type_name(&receiver_type)
        ));
    };
    let method_def = find_object_method(program, &object_name, method)
        .ok_or_else(|| format!("对象 `{}` 没有方法 `{}`", object_name, method))?;
    if method_def.function.return_type == Type::Void {
        return Err(format!("方法没有返回值: {}", method));
    }
    let function = module
        .get_function(&llvm_function_name(&method_def.function))
        .ok_or_else(|| format!("未找到对象方法: {}->{}", object_name, method))?;
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(
        compile_expr(
            receiver,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?
        .into(),
    );
    for arg in args {
        call_args.push(
            compile_expr(
                arg,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            )?
            .into(),
        );
    }

    Ok(builder
        .build_call(function, &call_args, "method.call")
        .map_err(|e| format!("build_call failed: {:?}", e))?
        .try_as_basic_value()
        .unwrap_basic())
}

fn compile_logical_expr<'ctx>(
    left: &Expr,
    op: &BinaryOp,
    right: &Expr,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let left_value = compile_bool_expr(
        left,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
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
    let right_value = compile_bool_expr(
        right,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
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
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, String> {
    let value = compile_expr(
        expr,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?
    .into_int_value();
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
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
) -> Result<Type, String> {
    match expr {
        Expr::IntLiteral(_) => Ok(Type::Int),
        Expr::DoubleLiteral(_) => Ok(Type::Double),
        Expr::Ident(name) => locals
            .get(name)
            .map(local_type)
            .ok_or_else(|| format!("未定义的变量: {}", name)),
        Expr::Call { target, .. } => {
            let resolved =
                semantic::resolve_execute_target(program, current_func, scoped_imports, target)
                    .ok_or_else(|| format!("未找到模块或方法: {}", target))?;
            program
                .functions
                .iter()
                .find(|func| semantic::function_path(func) == resolved)
                .map(|func| func.return_type.clone())
                .ok_or_else(|| format!("未找到方法: {}", resolved))
        }
        Expr::StringLiteral(_) | Expr::FormattedString(_) => {
            if let Expr::FormattedString(parts) = expr {
                for part in parts {
                    if let FormatPart::Placeholder(name) = part {
                        infer_format_placeholder_type(name, program, locals)?;
                    }
                }
            }
            Ok(Type::String)
        }
        Expr::StructLiteral { name, .. } => Ok(Type::Struct(name.clone())),
        Expr::ObjectCreate { name, .. } => Ok(Type::Struct(name.clone())),
        Expr::ArrayLiteral(elements) => {
            if elements.is_empty() {
                return Err("数组字面量不能为空".to_string());
            }
            for element in elements {
                let element_type =
                    infer_type_from_expr(element, current_func, program, scoped_imports, locals)?;
                if element_type != Type::Int {
                    return Err(format!(
                        "数组元素暂只支持整数，实际为 {}",
                        type_name(&element_type)
                    ));
                }
            }
            Ok(Type::Array {
                element_type: Box::new(Type::Int),
                length: Some(elements.len()),
            })
        }
        Expr::Index { array, index } => {
            let array_type =
                infer_type_from_expr(array, current_func, program, scoped_imports, locals)?;
            let index_type =
                infer_type_from_expr(index, current_func, program, scoped_imports, locals)?;
            if index_type != Type::Int {
                return Err(format!(
                    "数组下标必须是整数，实际为 {}",
                    type_name(&index_type)
                ));
            }
            match array_type {
                Type::Array { element_type, .. } => Ok(*element_type),
                other => Err(format!("不能对 {} 类型使用数组访问", type_name(&other))),
            }
        }
        Expr::FieldAccess { base, field } => {
            infer_field_type(base, field, current_func, program, scoped_imports, locals)
        }
        Expr::MethodCall {
            receiver, method, ..
        } => {
            let receiver_type =
                infer_type_from_expr(receiver, current_func, program, scoped_imports, locals)?;
            let Type::Struct(object_name) = receiver_type else {
                return Err(format!(
                    "不能对 {} 类型调用对象方法",
                    type_name(&receiver_type)
                ));
            };
            find_object_method(program, &object_name, method)
                .map(|method_def| method_def.function.return_type.clone())
                .ok_or_else(|| format!("对象 `{}` 没有方法 `{}`", object_name, method))
        }
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => {
                infer_type_from_expr(expr, current_func, program, scoped_imports, locals)
            }
            UnaryOp::Not => Ok(Type::Bool),
        },
        Expr::Binary { left, op, .. } => match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                infer_type_from_expr(left, current_func, program, scoped_imports, locals)
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

fn infer_field_type<'ctx>(
    base: &Expr,
    field: &str,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
) -> Result<Type, String> {
    let base_type = infer_type_from_expr(base, current_func, program, scoped_imports, locals)?;
    let Type::Struct(struct_name) = base_type else {
        return Err(format!("不能对 {} 类型使用字段访问", type_name(&base_type)));
    };
    struct_field_info(program, &struct_name, field).map(|(_, ty)| ty)
}

fn infer_format_placeholder_type<'ctx>(
    name: &str,
    program: &Program,
    locals: &HashMap<String, LocalValue<'ctx>>,
) -> Result<Type, String> {
    if let Some((base, field)) = name.split_once("->") {
        let base_ty = locals
            .get(base.trim())
            .map(local_type)
            .ok_or_else(|| format!("未定义的格式化变量: {}", base.trim()))?;
        let Type::Struct(struct_name) = base_ty else {
            return Err(format!("不能对 {} 类型使用字段访问", type_name(&base_ty)));
        };
        return struct_field_info(program, &struct_name, field.trim()).map(|(_, ty)| ty);
    }
    locals
        .get(name)
        .map(local_type)
        .ok_or_else(|| format!("未定义的格式化变量: {}", name))
}

fn struct_field_info(
    program: &Program,
    struct_name: &str,
    field: &str,
) -> Result<(usize, Type), String> {
    aggregate_fields(program, struct_name)?
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.name == field)
        .map(|(index, field_def)| (index, field_def.field_type.clone()))
        .ok_or_else(|| format!("结构或对象 `{}` 没有字段 `{}`", struct_name, field))
}

fn resolve_var_type<'ctx>(
    var: &VarDecl,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
) -> Result<Type, String> {
    match (&var.var_type, &var.init) {
        (Some(declared), Some(init)) => {
            let inferred =
                infer_type_from_expr(init, current_func, program, scoped_imports, locals)?;
            resolve_declared_type(declared, &inferred).ok_or_else(|| {
                format!(
                    "变量初始化类型不匹配: `{}`\n  = 声明类型: {}\n  = 表达式类型: {}",
                    var.name,
                    type_name(declared),
                    type_name(&inferred)
                )
            })
        }
        (Some(declared), None) => {
            if is_unsized_array(declared) {
                Err(format!("数组预定义缺少长度: {}", var.name))
            } else {
                Ok(declared.clone())
            }
        }
        (None, Some(init)) => {
            infer_type_from_expr(init, current_func, program, scoped_imports, locals)
        }
        (None, None) => Err(format!("预定义变量缺少类型: {}", var.name)),
    }
}

fn resolve_declared_type(declared: &Type, inferred: &Type) -> Option<Type> {
    match (declared, inferred) {
        (
            Type::Array {
                element_type: declared_element,
                length: None,
            },
            Type::Array {
                element_type: inferred_element,
                length: Some(length),
            },
        ) if declared_element == inferred_element => Some(Type::Array {
            element_type: declared_element.clone(),
            length: Some(*length),
        }),
        _ if declared == inferred => Some(declared.clone()),
        _ => None,
    }
}

fn is_unsized_array(ty: &Type) -> bool {
    matches!(ty, Type::Array { length: None, .. })
}

fn compile_array_literal<'ctx>(
    elements: &[Expr],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    if elements.is_empty() {
        return Err("数组字面量不能为空".to_string());
    }
    let array_type = context.i32_type().array_type(elements.len() as u32);
    let mut array_value = array_type.const_zero();
    for (index, element) in elements.iter().enumerate() {
        let value = compile_expr(
            element,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        array_value = builder
            .build_insert_value(array_value, value, index as u32, "array.insert")
            .map_err(|e| format!("build_insert_value failed: {:?}", e))?
            .into_array_value();
    }
    Ok(array_value.into())
}

fn compile_struct_literal<'ctx>(
    name: &str,
    fields: &[(String, Expr)],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let struct_type =
        as_llvm_type(&Type::Struct(name.to_string()), context, program)?.into_struct_type();
    let struct_def = program
        .structs
        .iter()
        .find(|candidate| candidate.name == name)
        .ok_or_else(|| format!("未定义的结构: {}", name))?;
    let mut struct_value = struct_type.const_zero();
    for (index, field) in struct_def.fields.iter().enumerate() {
        let (_, expr) = fields
            .iter()
            .find(|(field_name, _)| field_name == &field.name)
            .ok_or_else(|| format!("构造 `{}` 缺少字段 `{}`", name, field.name))?;
        let value = compile_expr(
            expr,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        struct_value = builder
            .build_insert_value(struct_value, value, index as u32, "struct.insert")
            .map_err(|e| format!("build_insert_value failed: {:?}", e))?
            .into_struct_value();
    }
    Ok(struct_value.into())
}

fn compile_object_create<'ctx>(
    name: &str,
    args: &[Expr],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let object = program
        .objects
        .iter()
        .find(|candidate| candidate.name == name)
        .ok_or_else(|| format!("未定义的对象: {}", name))?;
    let object_type = aggregate_struct_type(name, context, program)?;
    let ptr = builder
        .build_malloc(object_type, "object.create")
        .map_err(|e| format!("build_malloc failed: {:?}", e))?;
    builder
        .build_store(ptr, object_type.const_zero())
        .map_err(|e| format!("build_store object zero failed: {:?}", e))?;

    if let Some(constructor) = &object.constructor {
        if args.len() != constructor.params.len() {
            return Err(format!(
                "创建 `{}` 的参数数量不匹配: 期望 {}，实际 {}",
                name,
                constructor.params.len(),
                args.len()
            ));
        }
        let mut constructor_locals = locals.clone();
        let current_alloca = builder
            .build_alloca(
                as_llvm_type(&Type::Struct(name.to_string()), context, program)?,
                "current.object",
            )
            .map_err(|e| format!("build_alloca failed: {:?}", e))?;
        builder
            .build_store(current_alloca, ptr)
            .map_err(|e| format!("build_store failed: {:?}", e))?;
        constructor_locals.insert(
            "当前".to_string(),
            LocalValue::Pointer(current_alloca, Type::Struct(name.to_string()), false, false),
        );

        for (arg, param) in args.iter().zip(&constructor.params) {
            let value = compile_expr(
                arg,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            )?;
            let alloca = builder
                .build_alloca(
                    as_llvm_type(&param.param_type, context, program)?,
                    &sanitize_llvm_name(&param.name),
                )
                .map_err(|e| format!("build_alloca failed: {:?}", e))?;
            builder
                .build_store(alloca, value)
                .map_err(|e| format!("build_store failed: {:?}", e))?;
            constructor_locals.insert(
                param.name.clone(),
                LocalValue::Pointer(alloca, param.param_type.clone(), false, false),
            );
        }

        let constructor_func = FunctionDef {
            name: "构造方法".to_string(),
            module_path: Some(crate::parser::object_module_path(name)),
            params: constructor.params.clone(),
            return_type: Type::Void,
            is_entry: false,
            is_external: false,
            external_symbol: None,
            body: constructor.body.clone(),
        };
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| "Missing current function for constructor".to_string())?;
        let mut constructor_imports = scoped_imports.to_vec();
        for stmt in &constructor.body {
            compile_stmt(
                stmt,
                &constructor_func,
                program,
                &mut constructor_imports,
                &mut constructor_locals,
                &function,
                builder,
                context,
                module,
            )?;
        }
    } else if !args.is_empty() {
        return Err(format!("对象 `{}` 没有构造方法，创建时不能传入参数", name));
    }

    Ok(ptr.into())
}

fn compile_field_access<'ctx>(
    base: &Expr,
    field: &str,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let base_type = infer_type_from_expr(base, current_func, program, scoped_imports, locals)?;
    let Type::Struct(struct_name) = &base_type else {
        return Err(format!("不能对 {} 类型使用字段访问", type_name(&base_type)));
    };
    if is_object_name(program, struct_name) {
        let (field_ptr, field_type) = compile_field_ptr(
            base,
            field,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        return builder
            .build_load(
                as_llvm_type(&field_type, context, program)?,
                field_ptr,
                "object.field",
            )
            .map_err(|e| format!("build_load failed: {:?}", e));
    }
    let (field_index, _field_type) = struct_field_info(program, struct_name, field)?;

    if let Expr::Ident(name) = base {
        let LocalValue::Pointer(ptr, _, _, _) = locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("未定义的变量: {}", name))?;
        let struct_type = as_llvm_type(&base_type, context, program)?.into_struct_type();
        let field_ptr = builder
            .build_struct_gep(struct_type, ptr, field_index as u32, "struct.field.ptr")
            .map_err(|e| format!("build_struct_gep failed: {:?}", e))?;
        return builder
            .build_load(
                as_llvm_type(&_field_type, context, program)?,
                field_ptr,
                "struct.field",
            )
            .map_err(|e| format!("build_load failed: {:?}", e));
    }

    let struct_value = compile_expr(
        base,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_extract_value(
            struct_value.into_struct_value(),
            field_index as u32,
            "struct.field",
        )
        .map_err(|e| format!("build_extract_value failed: {:?}", e))
}

fn compile_field_ptr<'ctx>(
    base: &Expr,
    field: &str,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(PointerValue<'ctx>, Type), String> {
    let base_type = infer_type_from_expr(base, current_func, program, scoped_imports, locals)?;
    let Type::Struct(type_name_value) = &base_type else {
        return Err(format!("不能对 {} 类型使用字段访问", type_name(&base_type)));
    };
    let (field_index, field_type) = struct_field_info(program, type_name_value, field)?;
    let aggregate_type = aggregate_struct_type(type_name_value, context, program)?;

    let aggregate_ptr = if is_object_name(program, type_name_value) {
        compile_expr(
            base,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?
        .into_pointer_value()
    } else if let Expr::Ident(name) = base {
        let LocalValue::Pointer(ptr, _, _, _) = locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("未定义的变量: {}", name))?;
        ptr
    } else {
        return Err("当前只支持给变量上的结构字段赋值".to_string());
    };

    let field_ptr = builder
        .build_struct_gep(
            aggregate_type,
            aggregate_ptr,
            field_index as u32,
            "aggregate.field.ptr",
        )
        .map_err(|e| format!("build_struct_gep failed: {:?}", e))?;
    Ok((field_ptr, field_type))
}

fn compile_array_index<'ctx>(
    array: &Expr,
    index: &Expr,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let Expr::Ident(name) = array else {
        return Err("当前只支持通过变量名访问数组，例如 `arr[n]`。".to_string());
    };
    let (element_ptr, element_type) = compile_array_element_ptr(
        name,
        index,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?;
    builder
        .build_load(
            as_llvm_type(&element_type, context, program)?,
            element_ptr,
            "array.elem",
        )
        .map_err(|e| format!("build_load failed: {:?}", e))
}

fn compile_array_element_ptr<'ctx>(
    name: &str,
    index: &Expr,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(PointerValue<'ctx>, Type), String> {
    let local = locals
        .get(name)
        .ok_or_else(|| format!("未定义的变量: {}", name))?;
    let LocalValue::Pointer(array_ptr, array_type, _, _) = local.clone();
    let Type::Array {
        element_type,
        length,
    } = array_type
    else {
        return Err(format!("不能对非数组变量使用下标访问: {}", name));
    };
    let length = length.ok_or_else(|| format!("数组缺少长度: {}", name))?;
    let llvm_array_type = as_llvm_type(
        &Type::Array {
            element_type: element_type.clone(),
            length: Some(length),
        },
        context,
        program,
    )?;
    let index_value = compile_expr(
        index,
        current_func,
        program,
        scoped_imports,
        locals,
        builder,
        context,
        module,
    )?
    .into_int_value();
    let zero = context.i32_type().const_zero();
    let element_ptr = unsafe {
        builder
            .build_in_bounds_gep(
                llvm_array_type,
                array_ptr,
                &[zero, index_value],
                "array.elem.ptr",
            )
            .map_err(|e| format!("build_in_bounds_gep failed: {:?}", e))?
    };
    Ok((element_ptr, *element_type))
}

fn compile_format_placeholder<'ctx>(
    name: &str,
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalValue<'ctx>>,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(Type, BasicValueEnum<'ctx>), String> {
    if let Some((base, field)) = name.split_once("->") {
        let expr = Expr::FieldAccess {
            base: Box::new(Expr::Ident(base.trim().to_string())),
            field: field.trim().to_string(),
        };
        let ty = infer_type_from_expr(&expr, current_func, program, scoped_imports, locals)?;
        let value = compile_expr(
            &expr,
            current_func,
            program,
            scoped_imports,
            locals,
            builder,
            context,
            module,
        )?;
        return Ok((ty, value));
    }

    let local = locals
        .get(name)
        .ok_or_else(|| format!("未定义的格式化变量: {}", name))?;
    Ok((
        local_type(local),
        load_local_value(local.clone(), name, builder, context, program)?,
    ))
}

fn compile_formatted_string<'ctx>(
    parts: &[FormatPart],
    current_func: &FunctionDef,
    program: &Program,
    scoped_imports: &[ImportDecl],
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
                let (ty, value) = compile_format_placeholder(
                    name,
                    current_func,
                    program,
                    scoped_imports,
                    locals,
                    builder,
                    context,
                    module,
                )?;
                match ty {
                    Type::Int => format.push_str("%d"),
                    Type::Double => format.push_str("%g"),
                    Type::String => format.push_str("%s"),
                    other => {
                        return Err(format!(
                            "暂不支持在格式化字符串中使用 {:?}: {}",
                            other, name
                        ));
                    }
                }
                args.push(value);
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
        LocalValue::Pointer(_, ty, _, _) => ty.clone(),
    }
}

fn free_owned_objects<'ctx>(
    locals: &HashMap<String, LocalValue<'ctx>>,
    program: &Program,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
) -> Result<(), String> {
    for local in locals.values() {
        let LocalValue::Pointer(ptr, ty, _, owns_heap) = local;
        let Type::Struct(name) = ty else {
            continue;
        };
        if !*owns_heap || !is_object_name(program, name) {
            continue;
        }
        let object_ptr = builder
            .build_load(as_llvm_type(ty, context, program)?, *ptr, "object.free.ptr")
            .map_err(|e| format!("build_load failed: {:?}", e))?
            .into_pointer_value();
        builder
            .build_free(object_ptr)
            .map_err(|e| format!("build_free failed: {:?}", e))?;
    }
    Ok(())
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Void => "无".to_string(),
        Type::Int => "整数".to_string(),
        Type::Double => "小数".to_string(),
        Type::Float => "浮点".to_string(),
        Type::Bool => "布尔".to_string(),
        Type::Char => "字符".to_string(),
        Type::String => "字符串".to_string(),
        Type::Struct(name) => format!("结构{}", name),
        Type::Array {
            element_type,
            length,
        } => match length {
            Some(length) => format!("{}数组[{}]", type_name(element_type), length),
            None => format!("{}数组", type_name(element_type)),
        },
    }
}

fn load_local_value<'ctx>(
    local: LocalValue<'ctx>,
    name: &str,
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    program: &Program,
) -> Result<BasicValueEnum<'ctx>, String> {
    match local {
        LocalValue::Pointer(ptr, ty, _, _) => builder
            .build_load(
                as_llvm_type(&ty, context, program)?,
                ptr,
                &format!("load.{}", sanitize_llvm_name(name)),
            )
            .map_err(|e| format!("build_load failed: {:?}", e)),
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

fn declare_strcmp_function<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
) -> FunctionValue<'ctx> {
    if let Some(function) = module.get_function("strcmp") {
        return function;
    }

    let ptr_type = context.ptr_type(AddressSpace::from(0u16));
    let fn_type = context
        .i32_type()
        .fn_type(&[ptr_type.into(), ptr_type.into()], false);
    module.add_function("strcmp", fn_type, None)
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
        .map(|arg| {
            compile_expr(
                arg,
                current_func,
                program,
                scoped_imports,
                locals,
                builder,
                context,
                module,
            )
            .map(Into::into)
        })
        .collect::<Result<_, _>>()?;

    builder
        .build_call(function, &args, "call")
        .map_err(|e| format!("build_call failed: {:?}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use inkwell::context::Context;

    fn generate_ir(source: &str) -> String {
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("parse failed");
        let context = Context::create();
        let module = context.create_module("test");
        generate(&program, &context, &module).expect("codegen failed");
        module.verify().expect("module verification failed");
        module.print_to_string().to_string()
    }

    #[test]
    fn test_external_symbol_is_used_in_ir() {
        let ir = generate_ir(
            "#模块 Rust扩展
@声明 外部(\"wen_add\")
定义 方法 相加（左：整数，右：整数）返回 整数
@声明 入口
定义 方法 主（）返回 无：
设 结果 = 相加（1，2）
。。",
        );

        assert!(ir.contains("declare i32 @wen_add(i32, i32)"));
        assert!(ir.contains("call i32 @wen_add"));
    }

    #[test]
    fn test_external_void_function_uses_void_abi() {
        let ir = generate_ir(
            "#模块 Rust扩展
@声明 外部(\"wen_print\")
定义 方法 打印（内容：字符串）返回 无
@声明 入口
定义 方法 主（）返回 无：
执行 打印：\"你好\"
。。",
        );

        assert!(ir.contains("declare void @wen_print(ptr)"));
        assert!(ir.contains("call void @wen_print"));
    }

    #[test]
    fn test_struct_literal_allocates_and_loads_field() {
        let ir = generate_ir(
            "定义结构坐标：x：小数，y：小数，z：小数。。
定义 方法 求x（）返回 小数：
设 原点=构造坐标：x：0.0，y：1.0，z：2.0。。
返回 原点->x
。。",
        );

        assert!(ir.contains("alloca { double, double, double }"));
        assert!(ir.contains("getelementptr inbounds"));
        assert!(ir.contains("load double"));
    }

    #[test]
    fn test_object_create_method_call_and_raii_free() {
        let ir = generate_ir(
            "定义对象向量：
结构：
    x：小数，
    y：小数
构造方法（x：小数，y：小数）：
    令当前->x=x
    令当前->y=y
公共成员：
    定义方法相乘（另一个向量：向量）返回 小数：
        返回 当前->x*另一个向量->x+当前->y*另一个向量->y
    。。
。。
定义 方法 求积（）返回 小数：
设 向量1=创建向量（10.0，15.0）
设 向量2=创建向量（10.0，10.0）
返回 向量1->相乘（向量2）
。。",
        );

        assert!(ir.contains("malloc"));
        assert!(ir.contains("free"));
        assert!(ir.contains("call double"));
        assert!(ir.contains("getelementptr inbounds"));
    }
}
