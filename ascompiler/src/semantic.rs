use crate::lexer::Span;
use crate::parser::{
    ArrayAssignStmt, AssignStmt, BinaryOp, Expr, FieldAssignStmt, FormatPart, FunctionDef,
    ImportDecl, ObjectDef, Program, ReturnStmt, Stmt, StructDef, Type, UnaryOp, VarDecl,
    object_module_path,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct LocalInfo {
    ty: Type,
    is_mutable: bool,
    declaration_span: Option<Span>,
}

#[derive(Clone, Copy)]
struct SourceContext<'a> {
    source: Option<&'a str>,
    source_name: Option<&'a str>,
}

#[derive(Debug)]
struct ModuleRegistry {
    modules: HashSet<String>,
    callables: HashSet<String>,
}

impl ModuleRegistry {
    fn from_program(program: &Program) -> Self {
        let mut modules = HashSet::new();
        let mut callables = HashSet::new();

        for module in &program.modules {
            modules.insert(module.name.clone());
        }

        for func in &program.functions {
            callables.insert(function_path(func));
        }

        ModuleRegistry { modules, callables }
    }

    fn import_exists(&self, path: &str) -> bool {
        self.modules.contains(path) || self.callables.contains(path)
    }
}

#[allow(dead_code)]
pub fn analyze(program: &Program) -> Result<(), String> {
    analyze_with_source(program, None, None)
}

pub fn analyze_with_source(
    program: &Program,
    source: Option<&str>,
    source_name: Option<&str>,
) -> Result<(), String> {
    let source_context = SourceContext {
        source,
        source_name,
    };
    let registry = ModuleRegistry::from_program(program);

    for import in &program.imports {
        validate_import(import, &registry)?;
    }

    validate_structs(&program.structs, &program.objects)?;
    validate_objects(program)?;

    for func in &program.functions {
        for param in &func.params {
            validate_declared_type(&param.param_type, program)?;
        }
        validate_declared_type(&func.return_type, program)?;

        let mut scoped_imports = program.imports.clone();
        let mut locals: HashMap<String, LocalInfo> = func
            .params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    LocalInfo {
                        ty: param.param_type.clone(),
                        is_mutable: false,
                        declaration_span: None,
                    },
                )
            })
            .collect();

        for stmt in &func.body {
            if let Err(err) = validate_stmt(
                stmt,
                program,
                func,
                &registry,
                &mut scoped_imports,
                &mut locals,
                source_context,
            ) {
                if err.starts_with("error[") || err.starts_with("错误[") {
                    return Err(err);
                }
                return Err(format!(
                    "{}\n --> 方法 `{}`\n  = 帮助: 语义错误通常不是标点问题，而是名字解析、类型或可变性不满足要求；请从上面指出的实体开始检查。",
                    err,
                    function_path(func)
                ));
            }
        }
    }

    Ok(())
}

fn validate_stmt(
    stmt: &Stmt,
    program: &Program,
    func: &FunctionDef,
    registry: &ModuleRegistry,
    scoped_imports: &mut Vec<ImportDecl>,
    locals: &mut HashMap<String, LocalInfo>,
    source_context: SourceContext<'_>,
) -> Result<(), String> {
    match stmt {
        Stmt::Import(import) => {
            validate_import(import, registry)?;
            scoped_imports.push(import.clone());
        }
        Stmt::Execute(exec) => {
            resolve_execute_target(program, func, scoped_imports, &exec.target)
                .ok_or_else(|| format!("未找到模块或方法: {}", exec.target))?;
            for arg in &exec.args {
                validate_expr(arg, program, func, scoped_imports, locals)?;
            }
        }
        Stmt::VarDecl(var) => {
            validate_var_decl(var, program, func, scoped_imports, locals)?;
        }
        Stmt::Assign(assign) => {
            validate_assign_stmt(
                assign,
                program,
                func,
                scoped_imports,
                locals,
                source_context,
            )?;
        }
        Stmt::ArrayAssign(assign) => {
            validate_array_assign_stmt(
                assign,
                program,
                func,
                scoped_imports,
                locals,
                source_context,
            )?;
        }
        Stmt::FieldAssign(assign) => {
            validate_field_assign_stmt(assign, program, func, scoped_imports, locals)?;
        }
        Stmt::Return(ret) => {
            validate_return_stmt(
                ret,
                &func.return_type,
                program,
                func,
                scoped_imports,
                locals,
            )?;
        }
        Stmt::If(if_stmt) => {
            for branch in &if_stmt.branches {
                validate_condition(&branch.condition, program, func, scoped_imports, locals)?;
                let mut branch_locals = locals.clone();
                let mut branch_imports = scoped_imports.clone();
                for stmt in &branch.body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut branch_imports,
                        &mut branch_locals,
                        source_context,
                    )?;
                }
            }
            if let Some(body) = &if_stmt.else_body {
                let mut branch_locals = locals.clone();
                let mut branch_imports = scoped_imports.clone();
                for stmt in body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut branch_imports,
                        &mut branch_locals,
                        source_context,
                    )?;
                }
            }
        }
        Stmt::Select(select_stmt) => {
            let target_type = locals
                .get(&select_stmt.target)
                .map(|local| local.ty.clone())
                .ok_or_else(|| {
                    format!(
                        "未定义的变量 `{}`\n  = 帮助: `当前` 后面只能接当前作用域内已声明的变量。",
                        select_stmt.target
                    )
                })?;

            for case in &select_stmt.cases {
                let value_type = validate_expr(&case.value, program, func, scoped_imports, locals)?;
                if value_type != target_type {
                    return Err(format!(
                        "选择分支类型不匹配\n  = 当前变量 `{}`: {}\n  = 取值: {}",
                        select_stmt.target,
                        type_name(&target_type),
                        type_name(&value_type)
                    ));
                }

                let mut branch_locals = locals.clone();
                let mut branch_imports = scoped_imports.clone();
                for stmt in &case.body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut branch_imports,
                        &mut branch_locals,
                        source_context,
                    )?;
                }
            }

            if let Some(body) = &select_stmt.default_body {
                let mut branch_locals = locals.clone();
                let mut branch_imports = scoped_imports.clone();
                for stmt in body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut branch_imports,
                        &mut branch_locals,
                        source_context,
                    )?;
                }
            }
        }
        Stmt::Loop(loop_stmt) => match loop_stmt {
            crate::parser::LoopStmt::Count {
                var_name,
                end,
                body,
            } => {
                let end_type = validate_expr(end, program, func, scoped_imports, locals)?;
                if end_type != Type::Int {
                    return Err(format!(
                        "计数循环上限类型不匹配\n  = 期望: 整数\n  = 实际: {}",
                        type_name(&end_type)
                    ));
                }

                let mut loop_locals = locals.clone();
                loop_locals.insert(
                    var_name.clone(),
                    LocalInfo {
                        ty: Type::Int,
                        is_mutable: false,
                        declaration_span: None,
                    },
                );
                let mut loop_imports = scoped_imports.clone();
                for stmt in body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut loop_imports,
                        &mut loop_locals,
                        source_context,
                    )?;
                }
            }
            crate::parser::LoopStmt::Iterate {
                var_name,
                start,
                end,
                body,
            } => {
                let start_type = validate_expr(start, program, func, scoped_imports, locals)?;
                if start_type != Type::Int {
                    return Err(format!(
                        "迭代循环起始值类型不匹配\n  = 期望: 整数\n  = 实际: {}",
                        type_name(&start_type)
                    ));
                }
                let end_type = validate_expr(end, program, func, scoped_imports, locals)?;
                if end_type != Type::Int {
                    return Err(format!(
                        "迭代循环结束值类型不匹配\n  = 期望: 整数\n  = 实际: {}",
                        type_name(&end_type)
                    ));
                }

                let mut loop_locals = locals.clone();
                loop_locals.insert(
                    var_name.clone(),
                    LocalInfo {
                        ty: Type::Int,
                        is_mutable: false,
                        declaration_span: None,
                    },
                );
                let mut loop_imports = scoped_imports.clone();
                for stmt in body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut loop_imports,
                        &mut loop_locals,
                        source_context,
                    )?;
                }
            }
            crate::parser::LoopStmt::Condition { condition, body } => {
                validate_condition(condition, program, func, scoped_imports, locals)?;
                let mut loop_locals = locals.clone();
                let mut loop_imports = scoped_imports.clone();
                for stmt in body {
                    validate_stmt(
                        stmt,
                        program,
                        func,
                        registry,
                        &mut loop_imports,
                        &mut loop_locals,
                        source_context,
                    )?;
                }
            }
        },
    }
    Ok(())
}

fn validate_var_decl(
    var: &VarDecl,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &mut HashMap<String, LocalInfo>,
) -> Result<(), String> {
    let var_type = match (&var.var_type, &var.init) {
        (Some(var_type), Some(init)) => {
            let inferred_type = validate_expr(init, program, func, scoped_imports, locals)?;
            if let Some(resolved_type) = resolve_declared_type(var_type, &inferred_type) {
                resolved_type
            } else {
                return Err(format!(
                    "变量初始化类型不匹配: `{}`\n  = 声明类型: {}\n  = 表达式类型: {}\n  = 帮助: 请让初始化表达式返回相同类型，或调整变量的声明类型。",
                    var.name,
                    type_name(var_type),
                    type_name(&inferred_type)
                ));
            }
        }
        (Some(var_type), None) => {
            if is_unsized_array(var_type) {
                return Err(format!(
                    "数组预定义 `{}` 缺少长度\n  = 帮助: 请写成 `定义 变量：数组 {}[10]`，或使用数组字面量初始化。",
                    var.name, var.name
                ));
            }
            var_type.clone()
        }
        (None, Some(init)) => validate_expr(init, program, func, scoped_imports, locals)?,
        (None, None) => {
            return Err(format!(
                "预定义变量 `{}` 缺少类型\n  = 帮助: 预定义变量需要写出类型，例如 `定义 变量：整数{}`。",
                var.name, var.name
            ));
        }
    };
    locals.insert(
        var.name.clone(),
        LocalInfo {
            ty: var_type,
            is_mutable: var.is_mutable,
            declaration_span: Some(var.name_span),
        },
    );
    Ok(())
}

fn validate_assign_stmt(
    assign: &AssignStmt,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
    source_context: SourceContext<'_>,
) -> Result<(), String> {
    let target = locals.get(&assign.name).ok_or_else(|| {
        format!(
            "未定义的变量 `{}`\n  = 帮助: 请确认变量已在当前方法、判断分支或循环作用域中声明。",
            assign.name
        )
    })?;
    if !target.is_mutable {
        if let (Some(source), Some(declaration_span)) =
            (source_context.source, target.declaration_span)
        {
            return Err(render_immutable_assignment(
                source_context.source_name,
                source,
                &assign.name,
                declaration_span,
                assign.span,
            ));
        }
        return Err(format!(
            "不可变变量不能重新赋值: `{}`\n  = 原因: `定义 变量` 默认不可变\n  = 帮助: 如果需要重新赋值，请声明为 `定义 可变 变量：{} = ...`，或使用 `设 {}`。",
            assign.name, assign.name, assign.name
        ));
    }
    let value_type = validate_expr(&assign.value, program, func, scoped_imports, locals)?;
    if value_type == target.ty {
        Ok(())
    } else {
        Err(format!(
            "赋值类型不匹配: `{}`\n  = 变量类型: {}\n  = 表达式类型: {}\n  = 帮助: 请让右侧表达式返回相同类型，或调整变量的声明类型。",
            assign.name,
            type_name(&target.ty),
            type_name(&value_type)
        ))
    }
}

fn validate_array_assign_stmt(
    assign: &ArrayAssignStmt,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
    source_context: SourceContext<'_>,
) -> Result<(), String> {
    let target = locals.get(&assign.name).ok_or_else(|| {
        format!(
            "未定义的变量 `{}`\n  = 帮助: 请确认数组已在当前作用域中声明。",
            assign.name
        )
    })?;
    if !target.is_mutable {
        if let (Some(source), Some(declaration_span)) =
            (source_context.source, target.declaration_span)
        {
            return Err(render_immutable_assignment(
                source_context.source_name,
                source,
                &assign.name,
                declaration_span,
                assign.span,
            ));
        }
        return Err(format!("不可变数组不能修改元素: `{}`", assign.name));
    }
    let Type::Array {
        element_type,
        length,
    } = &target.ty
    else {
        return Err(format!(
            "不能对 {} 类型设置数组元素\n  = 帮助: 元素设置形如 `设arr[1]=10`，目标必须是数组。",
            type_name(&target.ty)
        ));
    };
    let index_type = validate_expr(&assign.index, program, func, scoped_imports, locals)?;
    if index_type != Type::Int {
        return Err(format!(
            "数组下标类型不匹配\n  = 期望: 整数\n  = 实际: {}",
            type_name(&index_type)
        ));
    }
    if let (Some(length), Expr::IntLiteral(index_value)) = (length, &assign.index) {
        if *index_value < 0 || *index_value as usize >= *length {
            return Err(format!(
                "数组下标越界\n  = 数组长度: {}\n  = 下标: {}\n  = 帮助: 数组下标从 0 开始。",
                length, index_value
            ));
        }
    }
    let value_type = validate_expr(&assign.value, program, func, scoped_imports, locals)?;
    if value_type == **element_type {
        Ok(())
    } else {
        Err(format!(
            "数组元素赋值类型不匹配: `{}`\n  = 元素类型: {}\n  = 表达式类型: {}",
            assign.name,
            type_name(element_type),
            type_name(&value_type)
        ))
    }
}

fn validate_field_assign_stmt(
    assign: &FieldAssignStmt,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<(), String> {
    let base_type = validate_expr(&assign.base, program, func, scoped_imports, locals)?;
    let Type::Struct(type_name_value) = base_type else {
        return Err(format!(
            "不能对 {} 类型设置字段\n  = 帮助: 字段赋值形如 `令当前->x = 1`。",
            type_name(&base_type)
        ));
    };
    ensure_object_field_access_allowed(program, func, &type_name_value, &assign.field)?;
    let (_, field_type) = aggregate_field_info(program, &type_name_value, &assign.field)?;
    let value_type = validate_expr(&assign.value, program, func, scoped_imports, locals)?;
    if value_type == field_type {
        Ok(())
    } else {
        Err(format!(
            "字段 `{}` 赋值类型不匹配\n  = 字段类型: {}\n  = 表达式类型: {}",
            assign.field,
            type_name(&field_type),
            type_name(&value_type)
        ))
    }
}

fn validate_return_stmt(
    ret: &ReturnStmt,
    expected_type: &Type,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<(), String> {
    match (&ret.value, expected_type) {
        (None, Type::Void) => Ok(()),
        (None, other) => Err(format!(
            "返回语句缺少返回值\n  = 期望返回类型: {}\n  = 帮助: 写成 `返回 <表达式>`，或把方法返回类型改为 `无`。",
            type_name(other)
        )),
        (Some(_), Type::Void) => Err(
            "无返回值方法不能返回表达式\n  = 期望: 只写 `返回` 或省略返回语句\n  = 帮助: 如果确实需要返回值，请修改方法签名中的 `返回 无`。"
                .to_string(),
        ),
        (Some(expr), expected) => {
            let actual = validate_expr(expr, program, func, scoped_imports, locals)?;
            if &actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "返回类型不匹配\n  = 期望: {}\n  = 实际: {}\n  = 帮助: 返回表达式的类型必须和方法签名里的返回类型一致。",
                    type_name(expected),
                    type_name(&actual)
                ))
            }
        }
    }
}

fn validate_condition(
    expr: &Expr,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<(), String> {
    match validate_expr(expr, program, func, scoped_imports, locals)? {
        Type::Int | Type::Bool => Ok(()),
        other => Err(format!(
            "条件表达式类型不匹配\n  = 期望: 整数 或 布尔\n  = 实际: {}",
            type_name(&other)
        )),
    }
}

fn validate_structs(structs: &[StructDef], objects: &[ObjectDef]) -> Result<(), String> {
    let mut names = HashSet::new();
    for struct_def in structs {
        if !names.insert(struct_def.name.clone()) {
            return Err(format!("结构 `{}` 重复定义", struct_def.name));
        }
        if objects.iter().any(|object| object.name == struct_def.name) {
            return Err(format!("类型 `{}` 同时定义为结构和对象", struct_def.name));
        }
        if struct_def.fields.is_empty() {
            return Err(format!(
                "结构 `{}` 至少需要一个字段\n  = 帮助: 写成 `定义结构{}：字段：整数。。`。",
                struct_def.name, struct_def.name
            ));
        }
        let mut field_names = HashSet::new();
        for field in &struct_def.fields {
            if !field_names.insert(field.name.clone()) {
                return Err(format!(
                    "结构 `{}` 中字段 `{}` 重复定义",
                    struct_def.name, field.name
                ));
            }
        }
    }
    for struct_def in structs {
        for field in &struct_def.fields {
            validate_declared_type_in_structs(&field.field_type, structs, objects)?;
        }
    }
    Ok(())
}

fn validate_objects(program: &Program) -> Result<(), String> {
    let mut names = HashSet::new();
    for object in &program.objects {
        if !names.insert(object.name.clone()) {
            return Err(format!("对象 `{}` 重复定义", object.name));
        }
        if object.fields.is_empty() {
            return Err(format!(
                "对象 `{}` 至少需要一个结构字段\n  = 帮助: 在对象内写 `结构：字段：类型`。",
                object.name
            ));
        }
        let mut field_names = HashSet::new();
        for field in &object.fields {
            if !field_names.insert(field.name.clone()) {
                return Err(format!(
                    "对象 `{}` 中字段 `{}` 重复定义",
                    object.name, field.name
                ));
            }
            validate_declared_type(&field.field_type, program)?;
        }
        if let Some(constructor) = &object.constructor {
            let mut locals: HashMap<String, LocalInfo> = constructor
                .params
                .iter()
                .map(|param| {
                    (
                        param.name.clone(),
                        LocalInfo {
                            ty: param.param_type.clone(),
                            is_mutable: false,
                            declaration_span: None,
                        },
                    )
                })
                .collect();
            locals.insert(
                "当前".to_string(),
                LocalInfo {
                    ty: Type::Struct(object.name.clone()),
                    is_mutable: false,
                    declaration_span: None,
                },
            );
            let constructor_func = FunctionDef {
                name: "构造方法".to_string(),
                module_path: Some(object_module_path(&object.name)),
                params: constructor.params.clone(),
                return_type: Type::Void,
                is_entry: false,
                is_external: false,
                external_symbol: None,
                body: constructor.body.clone(),
            };
            let registry = ModuleRegistry::from_program(program);
            let mut scoped_imports = program.imports.clone();
            for stmt in &constructor.body {
                validate_stmt(
                    stmt,
                    program,
                    &constructor_func,
                    &registry,
                    &mut scoped_imports,
                    &mut locals,
                    SourceContext {
                        source: None,
                        source_name: None,
                    },
                )?;
            }
        }
    }
    Ok(())
}

fn validate_declared_type(ty: &Type, program: &Program) -> Result<(), String> {
    validate_declared_type_in_structs(ty, &program.structs, &program.objects)
}

fn validate_declared_type_in_structs(
    ty: &Type,
    structs: &[StructDef],
    objects: &[ObjectDef],
) -> Result<(), String> {
    match ty {
        Type::Struct(name) => {
            if structs.iter().any(|struct_def| struct_def.name == *name)
                || objects.iter().any(|object| object.name == *name)
            {
                Ok(())
            } else {
                Err(format!("未定义的结构或对象 `{}`", name))
            }
        }
        Type::Array { element_type, .. } => {
            validate_declared_type_in_structs(element_type, structs, objects)
        }
        _ => Ok(()),
    }
}

fn find_struct<'a>(program: &'a Program, name: &str) -> Option<&'a StructDef> {
    program
        .structs
        .iter()
        .find(|struct_def| struct_def.name == name)
}

fn find_object<'a>(program: &'a Program, name: &str) -> Option<&'a ObjectDef> {
    program.objects.iter().find(|object| object.name == name)
}

fn find_object_method<'a>(
    program: &'a Program,
    object_name: &str,
    method: &str,
) -> Option<&'a crate::parser::ObjectMethod> {
    find_object(program, object_name)?
        .methods
        .iter()
        .find(|candidate| candidate.function.name == method)
}

fn current_object_name(func: &FunctionDef) -> Option<String> {
    func.module_path
        .as_ref()
        .and_then(|path| path.strip_prefix("对象-"))
        .map(ToString::to_string)
}

fn ensure_object_field_access_allowed(
    program: &Program,
    func: &FunctionDef,
    type_name_value: &str,
    field: &str,
) -> Result<(), String> {
    if find_object(program, type_name_value).is_some()
        && current_object_name(func).as_deref() != Some(type_name_value)
    {
        return Err(format!(
            "对象 `{}` 的结构字段 `{}` 默认私有，不能在对象外访问",
            type_name_value, field
        ));
    }
    Ok(())
}

fn aggregate_field_info(
    program: &Program,
    type_name_value: &str,
    field: &str,
) -> Result<(usize, Type), String> {
    if let Some(struct_def) = find_struct(program, type_name_value) {
        return struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.name == field)
            .map(|(index, field_def)| (index, field_def.field_type.clone()))
            .ok_or_else(|| format!("结构 `{}` 没有字段 `{}`", type_name_value, field));
    }
    if let Some(object) = find_object(program, type_name_value) {
        return object
            .fields
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.name == field)
            .map(|(index, field_def)| (index, field_def.field_type.clone()))
            .ok_or_else(|| format!("对象 `{}` 没有字段 `{}`", type_name_value, field));
    }
    Err(format!("未定义的结构或对象 `{}`", type_name_value))
}

fn validate_struct_literal(
    name: &str,
    fields: &[(String, Expr)],
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    let struct_def = find_struct(program, name).ok_or_else(|| {
        format!(
            "未定义的结构 `{}`\n  = 帮助: 请先在顶层写 `定义结构{}：...。。`。",
            name, name
        )
    })?;
    let mut seen = HashSet::new();
    for (field_name, value) in fields {
        if !seen.insert(field_name.clone()) {
            return Err(format!("构造 `{}` 时字段 `{}` 重复赋值", name, field_name));
        }
        let field_def = struct_def
            .fields
            .iter()
            .find(|field| field.name == *field_name)
            .ok_or_else(|| format!("结构 `{}` 没有字段 `{}`", name, field_name))?;
        let value_type = validate_expr(value, program, func, scoped_imports, locals)?;
        if value_type != field_def.field_type {
            return Err(format!(
                "构造 `{}` 的字段 `{}` 类型不匹配\n  = 字段类型: {}\n  = 表达式类型: {}",
                name,
                field_name,
                type_name(&field_def.field_type),
                type_name(&value_type)
            ));
        }
    }
    for field in &struct_def.fields {
        if !seen.contains(&field.name) {
            return Err(format!("构造 `{}` 缺少字段 `{}`", name, field.name));
        }
    }
    Ok(Type::Struct(name.to_string()))
}

fn validate_object_create(
    name: &str,
    args: &[Expr],
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    let object = find_object(program, name).ok_or_else(|| {
        format!(
            "未定义的对象 `{}`\n  = 帮助: 请先在顶层写 `定义对象{}：...。。`。",
            name, name
        )
    })?;
    let params: &[crate::parser::Param] = object
        .constructor
        .as_ref()
        .map(|constructor| constructor.params.as_slice())
        .unwrap_or(&[]);
    if args.len() != params.len() {
        return Err(format!(
            "创建 `{}` 的参数数量不匹配\n  = 期望: {} 个\n  = 实际: {} 个",
            name,
            params.len(),
            args.len()
        ));
    }
    for (arg, param) in args.iter().zip(params) {
        let arg_type = validate_expr(arg, program, func, scoped_imports, locals)?;
        if arg_type != param.param_type {
            return Err(format!(
                "创建 `{}` 的参数 `{}` 类型不匹配\n  = 期望: {}\n  = 实际: {}",
                name,
                param.name,
                type_name(&param.param_type),
                type_name(&arg_type)
            ));
        }
    }
    Ok(Type::Struct(name.to_string()))
}

fn validate_field_access(
    base: &Expr,
    field: &str,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    let base_type = validate_expr(base, program, func, scoped_imports, locals)?;
    let Type::Struct(struct_name) = base_type else {
        return Err(format!(
            "不能对 {} 类型使用字段访问\n  = 帮助: 字段访问形如 `结构变量->字段名`。",
            type_name(&base_type)
        ));
    };
    ensure_object_field_access_allowed(program, func, &struct_name, field)?;
    aggregate_field_info(program, &struct_name, field).map(|(_, ty)| ty)
}

fn validate_method_call(
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    let receiver_type = validate_expr(receiver, program, func, scoped_imports, locals)?;
    let Type::Struct(object_name) = receiver_type else {
        return Err(format!(
            "不能对 {} 类型调用对象方法\n  = 帮助: 方法调用形如 `对象变量->方法（参数）`。",
            type_name(&receiver_type)
        ));
    };
    let method_def = find_object_method(program, &object_name, method)
        .ok_or_else(|| format!("对象 `{}` 没有方法 `{}`", object_name, method))?;
    if method_def.access == crate::parser::MemberAccess::Private
        && current_object_name(func).as_deref() != Some(object_name.as_str())
    {
        return Err(format!(
            "对象 `{}` 的私有方法 `{}` 不能在这里调用",
            object_name, method
        ));
    }
    let params = &method_def.function.params[1..];
    if args.len() != params.len() {
        return Err(format!(
            "方法 `{}` 的参数数量不匹配\n  = 期望: {} 个\n  = 实际: {} 个",
            method,
            params.len(),
            args.len()
        ));
    }
    for (arg, param) in args.iter().zip(params) {
        let arg_type = validate_expr(arg, program, func, scoped_imports, locals)?;
        if arg_type != param.param_type {
            return Err(format!(
                "方法 `{}` 的参数 `{}` 类型不匹配\n  = 期望: {}\n  = 实际: {}",
                method,
                param.name,
                type_name(&param.param_type),
                type_name(&arg_type)
            ));
        }
    }
    Ok(method_def.function.return_type.clone())
}

fn validate_format_placeholder(
    name: &str,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    if let Some((base, field)) = name.split_once("->") {
        return validate_field_access(
            &Expr::Ident(base.trim().to_string()),
            field.trim(),
            program,
            func,
            scoped_imports,
            locals,
        );
    }
    locals
        .get(name)
        .map(|local| local.ty.clone())
        .ok_or_else(|| format!("未定义的格式化变量 `{}`\n  = 帮助: 格式化字符串 `{{...}}` 中只能引用当前作用域内已声明的变量。", name))
}

fn validate_expr(
    expr: &Expr,
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    match expr {
        Expr::IntLiteral(_) => Ok(Type::Int),
        Expr::DoubleLiteral(_) => Ok(Type::Double),
        Expr::Ident(name) => locals
            .get(name)
            .map(|local| local.ty.clone())
            .ok_or_else(|| {
                format!(
                    "未定义的变量 `{}`\n  = 帮助: 请确认名字拼写一致，并且声明出现在使用之前。",
                    name
                )
            }),
        Expr::Call {
            target,
            type_arg,
            args,
        } => {
            let resolved = resolve_execute_target(program, func, scoped_imports, target)
                .ok_or_else(|| format!("未找到模块或方法 `{}`\n  = 帮助: 如果这是外部模块中的方法，请先写 `引用 模块：路径`，或使用引用别名。", target))?;
            let called_func = program
                .functions
                .iter()
                .find(|candidate| function_path(candidate) == resolved)
                .ok_or_else(|| format!("未找到方法: {}", resolved))?;
            if called_func.return_type == Type::Void {
                return Err(format!(
                    "方法 `{}` 没有返回值\n  = 实际返回类型: 无\n  = 帮助: `无` 返回值的方法只能用 `执行` 调用，不能放在表达式里。",
                    target
                ));
            }
            if let Some(type_arg) = type_arg {
                if type_arg != &called_func.return_type {
                    return Err(format!(
                        "方法 `{}` 的泛型类型不匹配\n  = `->` 指定: {}\n  = 实际返回: {}",
                        target,
                        type_name(type_arg),
                        type_name(&called_func.return_type)
                    ));
                }
            }
            if args.len() != called_func.params.len() {
                return Err(format!(
                    "方法 `{}` 的参数数量不匹配\n  = 期望: {} 个\n  = 实际: {} 个",
                    target,
                    called_func.params.len(),
                    args.len()
                ));
            }
            for (arg, param) in args.iter().zip(&called_func.params) {
                let arg_type = validate_expr(arg, program, func, scoped_imports, locals)?;
                if arg_type != param.param_type {
                    return Err(format!(
                        "方法 `{}` 的参数 `{}` 类型不匹配\n  = 期望: {}\n  = 实际: {}",
                        target,
                        param.name,
                        type_name(&param.param_type),
                        type_name(&arg_type)
                    ));
                }
            }
            Ok(called_func.return_type.clone())
        }
        Expr::StringLiteral(_) => Ok(Type::String),
        Expr::FormattedString(parts) => {
            for part in parts {
                if let FormatPart::Placeholder(name) = part {
                    validate_format_placeholder(name, program, func, scoped_imports, locals)?;
                }
            }
            Ok(Type::String)
        }
        Expr::ArrayLiteral(elements) => {
            validate_array_literal(elements, program, func, scoped_imports, locals)
        }
        Expr::StructLiteral { name, fields } => {
            validate_struct_literal(name, fields, program, func, scoped_imports, locals)
        }
        Expr::ObjectCreate { name, args } => {
            validate_object_create(name, args, program, func, scoped_imports, locals)
        }
        Expr::Index { array, index } => {
            if !matches!(array.as_ref(), Expr::Ident(_)) {
                return Err("当前只支持通过变量名访问数组\n  = 帮助: 请写成 `arr[n]`。".to_string());
            }
            let array_type = validate_expr(array, program, func, scoped_imports, locals)?;
            let index_type = validate_expr(index, program, func, scoped_imports, locals)?;
            if index_type != Type::Int {
                return Err(format!(
                    "数组下标类型不匹配\n  = 期望: 整数\n  = 实际: {}",
                    type_name(&index_type)
                ));
            }
            match array_type {
                Type::Array {
                    element_type,
                    length,
                } => {
                    if let (Some(length), Expr::IntLiteral(index_value)) = (length, index.as_ref())
                    {
                        if *index_value < 0 || *index_value as usize >= length {
                            return Err(format!(
                                "数组下标越界\n  = 数组长度: {}\n  = 下标: {}\n  = 帮助: 数组下标从 0 开始。",
                                length, index_value
                            ));
                        }
                    }
                    Ok(*element_type)
                }
                other => Err(format!(
                    "不能对 {} 类型使用数组访问\n  = 帮助: 数组访问形如 `arr[n]`，左侧必须是数组。",
                    type_name(&other)
                )),
            }
        }
        Expr::FieldAccess { base, field } => {
            validate_field_access(base, field, program, func, scoped_imports, locals)
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => validate_method_call(
            receiver,
            method,
            args,
            program,
            func,
            scoped_imports,
            locals,
        ),
        Expr::Unary { op, expr } => {
            let ty = validate_expr(expr, program, func, scoped_imports, locals)?;
            match (op, ty) {
                (UnaryOp::Neg, Type::Int) => Ok(Type::Int),
                (UnaryOp::Neg, Type::Double) => Ok(Type::Double),
                (UnaryOp::Not, Type::Int | Type::Bool) => Ok(Type::Bool),
                (UnaryOp::Neg, other) => Err(format!("一元 `-` 不支持 {} 类型", type_name(&other))),
                (UnaryOp::Not, other) => Err(format!("一元 `!` 不支持 {} 类型", type_name(&other))),
            }
        }
        Expr::Binary { left, op, right } => {
            let left_ty = validate_expr(left, program, func, scoped_imports, locals)?;
            let right_ty = validate_expr(right, program, func, scoped_imports, locals)?;
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                    if left_ty == Type::Int && right_ty == Type::Int {
                        Ok(Type::Int)
                    } else if left_ty == Type::Double && right_ty == Type::Double {
                        Ok(Type::Double)
                    } else {
                        Err(format!(
                            "算术运算符 `{}` 需要同类型数字操作数\n  = 左侧: {}\n  = 右侧: {}",
                            binary_op_name(op),
                            type_name(&left_ty),
                            type_name(&right_ty)
                        ))
                    }
                }
                BinaryOp::Eq | BinaryOp::NotEq => {
                    if left_ty == right_ty {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "相等运算符 `{}` 两侧类型必须相同\n  = 左侧: {}\n  = 右侧: {}",
                            binary_op_name(op),
                            type_name(&left_ty),
                            type_name(&right_ty)
                        ))
                    }
                }
                BinaryOp::Less | BinaryOp::LessEq | BinaryOp::Greater | BinaryOp::GreaterEq => {
                    if (left_ty == Type::Int && right_ty == Type::Int)
                        || (left_ty == Type::Double && right_ty == Type::Double)
                    {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "比较运算符 `{}` 需要同类型数字操作数\n  = 左侧: {}\n  = 右侧: {}",
                            binary_op_name(op),
                            type_name(&left_ty),
                            type_name(&right_ty)
                        ))
                    }
                }
                BinaryOp::And | BinaryOp::Or => {
                    if matches!(left_ty, Type::Int | Type::Bool)
                        && matches!(right_ty, Type::Int | Type::Bool)
                    {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "逻辑运算符 `{}` 需要整数或布尔操作数\n  = 左侧: {}\n  = 右侧: {}",
                            binary_op_name(op),
                            type_name(&left_ty),
                            type_name(&right_ty)
                        ))
                    }
                }
            }
        }
    }
}

fn validate_array_literal(
    elements: &[Expr],
    program: &Program,
    func: &FunctionDef,
    scoped_imports: &[ImportDecl],
    locals: &HashMap<String, LocalInfo>,
) -> Result<Type, String> {
    if elements.is_empty() {
        return Err(
            "数组字面量不能为空\n  = 帮助: 请至少提供一个元素，例如 `【1，2，3】`。".to_string(),
        );
    }
    for element in elements {
        let element_type = validate_expr(element, program, func, scoped_imports, locals)?;
        if element_type != Type::Int {
            return Err(format!(
                "数组元素类型不匹配\n  = 期望: 整数\n  = 实际: {}\n  = 帮助: 当前数组先支持整数元素。",
                type_name(&element_type)
            ));
        }
    }
    Ok(Type::Array {
        element_type: Box::new(Type::Int),
        length: Some(elements.len()),
    })
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

fn validate_import(import: &ImportDecl, registry: &ModuleRegistry) -> Result<(), String> {
    if registry.import_exists(&import.path) {
        Ok(())
    } else {
        Err(format!(
            "未找到模块或方法 `{}`\n  = 帮助: 请确认 `#模块` 路径、方法名以及 `引用 模块：...` 中的路径完全一致。",
            import.path
        ))
    }
}

fn render_immutable_assignment(
    source_name: Option<&str>,
    source: &str,
    name: &str,
    declaration_span: Span,
    assignment_span: Span,
) -> String {
    let (assign_line, assign_col, _) = line_col(source, assignment_span.start);
    let location = match source_name {
        Some(source_name) => format!("{}:{}:{}", source_name, assign_line, assign_col),
        None => format!("第 {} 行，第 {} 列", assign_line, assign_col),
    };
    let line_number_width = assign_line
        .max(line_col(source, declaration_span.start).0)
        .to_string()
        .len();

    let mut out = String::new();
    out.push_str(&format!(
        "错误[E0384]: 不能给不可变变量 `{}` 重复赋值\n",
        name
    ));
    out.push_str(&format!(" --> {}\n", location));
    out.push_str(&format!("{:>width$} |\n", "", width = line_number_width));
    push_span_note(
        &mut out,
        source,
        declaration_span,
        &format!("第一次赋值给 `{}`", name),
        line_number_width,
        true,
    );
    push_span_note(
        &mut out,
        source,
        assignment_span,
        "不能给不可变变量重复赋值",
        line_number_width,
        false,
    );
    out.push_str(&format!("{:>width$} |\n", "", width = line_number_width));
    out.push_str("帮助: 可以把这个绑定声明为可变\n");
    if let Some((line_no, suggested, plus_col, plus_width)) =
        mutable_suggestion(source, declaration_span)
    {
        out.push_str(&format!("{:>width$} |\n", "", width = line_number_width));
        out.push_str(&format!(
            "{:>width$} | {}\n",
            line_no,
            suggested,
            width = line_number_width
        ));
        out.push_str(&format!(
            "{:>width$} | {}{}\n",
            "",
            " ".repeat(plus_col.saturating_sub(1)),
            "+".repeat(plus_width),
            width = line_number_width
        ));
    } else {
        out.push_str("  = 帮助: 将声明改为 `定义 可变 变量：...`，或使用 `设` 声明可变变量。\n");
    }

    out
}

fn push_span_note(
    out: &mut String,
    source: &str,
    span: Span,
    label: &str,
    line_number_width: usize,
    is_secondary: bool,
) {
    let (line_no, col_no, _) = line_col(source, span.start);
    let line_text = source.lines().nth(line_no.saturating_sub(1)).unwrap_or("");
    let col_width = col_no.saturating_sub(1);
    let caret_width = span
        .end
        .saturating_sub(span.start)
        .min(line_text.chars().count().saturating_sub(col_width))
        .max(1);
    let marker = if is_secondary { "-" } else { "^" };

    out.push_str(&format!(
        "{:>width$} | {}\n",
        line_no,
        line_text,
        width = line_number_width
    ));
    out.push_str(&format!(
        "{:>width$} | {}{} {}\n",
        "",
        " ".repeat(col_width),
        marker.repeat(caret_width),
        label,
        width = line_number_width
    ));
}

fn mutable_suggestion(
    source: &str,
    declaration_span: Span,
) -> Option<(usize, String, usize, usize)> {
    let (line_no, _, _) = line_col(source, declaration_span.start);
    let line_text = source.lines().nth(line_no.saturating_sub(1))?;
    let keyword_byte = line_text.find("变量")?;
    let mut suggested = String::new();
    suggested.push_str(&line_text[..keyword_byte]);
    suggested.push_str("可变 ");
    suggested.push_str(&line_text[keyword_byte..]);

    let plus_col = line_text[..keyword_byte].chars().count() + 1;
    Some((line_no, suggested, plus_col, "可变 ".chars().count()))
}

fn line_col(source: &str, pos: usize) -> (usize, usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut line_start = 0;
    for (idx, ch) in source.chars().enumerate() {
        if idx == pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
            line_start = idx + 1;
        } else {
            col += 1;
        }
    }
    (line, col, line_start)
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

fn binary_op_name(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEq => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEq => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

pub fn function_path(func: &FunctionDef) -> String {
    match &func.module_path {
        Some(module_path) => format!("{}-{}", module_path, func.name),
        None => func.name.clone(),
    }
}

pub fn resolve_execute_target(
    program: &Program,
    current_func: &FunctionDef,
    imports: &[ImportDecl],
    target: &str,
) -> Option<String> {
    let registry = ModuleRegistry::from_program(program);

    if let Some(resolved) = resolve_imported_target(imports, target, &registry) {
        return Some(resolved);
    }

    if !target.contains('-') && registry.callables.contains(target) {
        return Some(target.to_string());
    }

    if let Some(module_path) = &current_func.module_path {
        let local_path = format!("{}-{}", module_path, target);
        if !target.contains('-') && registry.callables.contains(&local_path) {
            return Some(local_path);
        }
    }

    None
}

fn resolve_imported_target(
    imports: &[ImportDecl],
    target: &str,
    registry: &ModuleRegistry,
) -> Option<String> {
    for import in imports {
        if let Some(alias) = &import.alias {
            if alias == target && registry.callables.contains(&import.path) {
                return Some(import.path.clone());
            }

            if let Some(suffix) = target.strip_prefix(&format!("{}-", alias)) {
                let resolved = format!("{}-{}", import.path, suffix);
                if registry.modules.contains(&import.path) && registry.callables.contains(&resolved)
                {
                    return Some(resolved);
                }
            }
        }

        if import.path == target && registry.callables.contains(target) {
            return Some(target.to_string());
        }

        if let Some(suffix) = target.strip_prefix(&format!("{}-", import.path)) {
            let resolved = format!("{}-{}", import.path, suffix);
            if registry.modules.contains(&import.path) && registry.callables.contains(&resolved) {
                return Some(resolved);
            }
        }
    }

    None
}
