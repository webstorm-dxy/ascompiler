use crate::parser::{
    AssignStmt, BinaryOp, Expr, FormatPart, FunctionDef, ImportDecl, Program, ReturnStmt, Stmt,
    Type, UnaryOp, VarDecl,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct LocalInfo {
    ty: Type,
    is_mutable: bool,
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

pub fn analyze(program: &Program) -> Result<(), String> {
    let registry = ModuleRegistry::from_program(program);

    for import in &program.imports {
        validate_import(import, &registry)?;
    }

    for func in &program.functions {
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
                    },
                )
            })
            .collect();

        for stmt in &func.body {
            validate_stmt(
                stmt,
                program,
                func,
                &registry,
                &mut scoped_imports,
                &mut locals,
            )?;
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
            validate_assign_stmt(assign, program, func, scoped_imports, locals)?;
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
                    return Err(format!("计数循环上限必须是整数，找到 {:?}", end_type));
                }

                let mut loop_locals = locals.clone();
                loop_locals.insert(
                    var_name.clone(),
                    LocalInfo {
                        ty: Type::Int,
                        is_mutable: false,
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
    let inferred_type = validate_expr(&var.init, program, func, scoped_imports, locals)?;
    let var_type = var.var_type.clone().unwrap_or(inferred_type);
    locals.insert(
        var.name.clone(),
        LocalInfo {
            ty: var_type,
            is_mutable: var.is_mutable,
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
) -> Result<(), String> {
    let target = locals
        .get(&assign.name)
        .ok_or_else(|| format!("未定义的变量: {}", assign.name))?;
    if !target.is_mutable {
        return Err(format!("不可变变量不能重新赋值: {}", assign.name));
    }
    let value_type = validate_expr(&assign.value, program, func, scoped_imports, locals)?;
    if value_type == target.ty {
        Ok(())
    } else {
        Err(format!(
            "赋值类型不匹配: {} 是 {:?}, 但表达式是 {:?}",
            assign.name, target.ty, value_type
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
        (None, other) => Err(format!("返回语句缺少 {:?} 类型的值", other)),
        (Some(_), Type::Void) => Err("无返回值方法不能返回表达式".to_string()),
        (Some(expr), expected) => {
            let actual = validate_expr(expr, program, func, scoped_imports, locals)?;
            if &actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "返回类型不匹配: 期望 {:?}, 找到 {:?}",
                    expected, actual
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
        other => Err(format!("条件表达式必须是整数或布尔，找到 {:?}", other)),
    }
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
        Expr::Ident(name) => locals
            .get(name)
            .map(|local| local.ty.clone())
            .ok_or_else(|| format!("未定义的变量: {}", name)),
        Expr::Call { target, args } => {
            let resolved = resolve_execute_target(program, func, scoped_imports, target)
                .ok_or_else(|| format!("未找到模块或方法: {}", target))?;
            let called_func = program
                .functions
                .iter()
                .find(|candidate| function_path(candidate) == resolved)
                .ok_or_else(|| format!("未找到方法: {}", resolved))?;
            if called_func.return_type == Type::Void {
                return Err(format!("方法没有返回值: {}", target));
            }
            if args.len() != called_func.params.len() {
                return Err(format!(
                    "方法 {} 需要 {} 个参数，找到 {} 个",
                    target,
                    called_func.params.len(),
                    args.len()
                ));
            }
            for (arg, param) in args.iter().zip(&called_func.params) {
                let arg_type = validate_expr(arg, program, func, scoped_imports, locals)?;
                if arg_type != param.param_type {
                    return Err(format!(
                        "方法 {} 的参数 {} 类型不匹配: 期望 {:?}, 找到 {:?}",
                        target, param.name, param.param_type, arg_type
                    ));
                }
            }
            Ok(called_func.return_type.clone())
        }
        Expr::StringLiteral(_) => Ok(Type::String),
        Expr::FormattedString(parts) => {
            for part in parts {
                if let FormatPart::Placeholder(name) = part {
                    locals
                        .get(name)
                        .ok_or_else(|| format!("未定义的格式化变量: {}", name))?;
                }
            }
            Ok(Type::String)
        }
        Expr::Unary { op, expr } => {
            let ty = validate_expr(expr, program, func, scoped_imports, locals)?;
            match (op, ty) {
                (UnaryOp::Neg, Type::Int) => Ok(Type::Int),
                (UnaryOp::Not, Type::Int | Type::Bool) => Ok(Type::Bool),
                (UnaryOp::Neg, other) => Err(format!("一元 - 不支持 {:?} 类型", other)),
                (UnaryOp::Not, other) => Err(format!("一元 ! 不支持 {:?} 类型", other)),
            }
        }
        Expr::Binary { left, op, right } => {
            let left_ty = validate_expr(left, program, func, scoped_imports, locals)?;
            let right_ty = validate_expr(right, program, func, scoped_imports, locals)?;
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                    if left_ty == Type::Int && right_ty == Type::Int {
                        Ok(Type::Int)
                    } else {
                        Err(format!(
                            "运算符 {:?} 需要整数操作数，找到 {:?} 和 {:?}",
                            op, left_ty, right_ty
                        ))
                    }
                }
                BinaryOp::Eq | BinaryOp::NotEq => {
                    if left_ty == right_ty {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "运算符 {:?} 两侧类型必须相同，找到 {:?} 和 {:?}",
                            op, left_ty, right_ty
                        ))
                    }
                }
                BinaryOp::Less | BinaryOp::LessEq | BinaryOp::Greater | BinaryOp::GreaterEq => {
                    if left_ty == Type::Int && right_ty == Type::Int {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "比较运算符 {:?} 需要整数操作数，找到 {:?} 和 {:?}",
                            op, left_ty, right_ty
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
                            "逻辑运算符 {:?} 需要整数或布尔操作数，找到 {:?} 和 {:?}",
                            op, left_ty, right_ty
                        ))
                    }
                }
            }
        }
    }
}

fn validate_import(import: &ImportDecl, registry: &ModuleRegistry) -> Result<(), String> {
    if registry.import_exists(&import.path) {
        Ok(())
    } else {
        Err(format!("未找到模块或方法: {}", import.path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::stdlib;

    #[test]
    fn test_alias_resolves_standard_output() {
        let source = "引用 模块：标准库-输入输出-输出 为 输出\n定义 方法 测试（）返回 无：执行 输出：“你好”。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");
        let program = stdlib::merge_with_standard_library(program).expect("std merge failed");

        analyze(&program).expect("Semantic analysis failed");
        let func = program.functions.iter().find(|f| f.name == "测试").unwrap();
        let resolved = resolve_execute_target(&program, func, &program.imports, "输出")
            .expect("Resolve failed");
        assert_eq!(resolved, stdlib::STD_IO_OUTPUT_PATH);
    }

    #[test]
    fn test_missing_import_reports_error() {
        let source = "引用 模块：不存在\n定义 方法 测试（）返回 无：。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        let err = analyze(&program).expect_err("Expected semantic error");
        assert!(err.contains("未找到模块或方法"));
    }

    #[test]
    fn test_assignment_and_return_in_count_loop() {
        let source = "定义 方法 从零求和（结束值：整数）返回 整数：设 cnt = 0 循环计数i<结束值：cnt=cnt+1。。返回 cnt。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        analyze(&program).expect("Semantic analysis failed");
    }

    #[test]
    fn test_define_variable_is_immutable_by_default() {
        let source = "定义 方法 测试（）返回 无：定义 变量：cnt = 0 cnt = 1。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        let err = analyze(&program).expect_err("Expected immutable assignment error");
        assert!(err.contains("不可变变量不能重新赋值"));
    }

    #[test]
    fn test_define_mutable_variable_allows_assignment() {
        let source = "定义 方法 测试（）返回 无：定义 可变 变量：cnt = 0 cnt = 1。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        analyze(&program).expect("Semantic analysis failed");
    }

    #[test]
    fn test_function_call_expression_uses_return_type() {
        let source = "定义 方法 从零求和（结束值：整数）返回 整数：返回 结束值。。定义 方法 测试（）返回 无：设 s = 从零求和（10）。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        analyze(&program).expect("Semantic analysis failed");
    }
}
