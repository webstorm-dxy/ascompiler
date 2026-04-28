use crate::parser::{
    BinaryOp, Expr, FormatPart, FunctionDef, ImportDecl, Program, Stmt, Type, UnaryOp, VarDecl,
};
use std::collections::{HashMap, HashSet};

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
        let mut locals: HashMap<String, Type> = func
            .params
            .iter()
            .map(|param| (param.name.clone(), param.param_type.clone()))
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
    locals: &mut HashMap<String, Type>,
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
                validate_expr(arg, locals)?;
            }
        }
        Stmt::VarDecl(var) => {
            validate_var_decl(var, locals)?;
        }
        Stmt::If(if_stmt) => {
            for branch in &if_stmt.branches {
                validate_condition(&branch.condition, locals)?;
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
    }
    Ok(())
}

fn validate_var_decl(var: &VarDecl, locals: &mut HashMap<String, Type>) -> Result<(), String> {
    let inferred_type = validate_expr(&var.init, locals)?;
    let var_type = var.var_type.clone().unwrap_or(inferred_type);
    locals.insert(var.name.clone(), var_type);
    Ok(())
}

fn validate_condition(expr: &Expr, locals: &HashMap<String, Type>) -> Result<(), String> {
    match validate_expr(expr, locals)? {
        Type::Int | Type::Bool => Ok(()),
        other => Err(format!("条件表达式必须是整数或布尔，找到 {:?}", other)),
    }
}

fn validate_expr(expr: &Expr, locals: &HashMap<String, Type>) -> Result<Type, String> {
    match expr {
        Expr::IntLiteral(_) => Ok(Type::Int),
        Expr::Ident(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("未定义的变量: {}", name)),
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
            let ty = validate_expr(expr, locals)?;
            match (op, ty) {
                (UnaryOp::Neg, Type::Int) => Ok(Type::Int),
                (UnaryOp::Not, Type::Int | Type::Bool) => Ok(Type::Bool),
                (UnaryOp::Neg, other) => Err(format!("一元 - 不支持 {:?} 类型", other)),
                (UnaryOp::Not, other) => Err(format!("一元 ! 不支持 {:?} 类型", other)),
            }
        }
        Expr::Binary { left, op, right } => {
            let left_ty = validate_expr(left, locals)?;
            let right_ty = validate_expr(right, locals)?;
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
}
