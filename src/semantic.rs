use crate::parser::{FunctionDef, ImportDecl, Program, Stmt};
use std::collections::HashSet;

pub const STD_IO_MODULE: &str = "标准库-输入输出";
pub const STD_OUTPUT_FUNCTION: &str = "标准库-输入输出-输出";

#[derive(Debug)]
struct ModuleRegistry {
    modules: HashSet<String>,
    callables: HashSet<String>,
}

impl ModuleRegistry {
    fn from_program(program: &Program) -> Self {
        let mut modules = HashSet::from([STD_IO_MODULE.to_string()]);
        let mut callables = HashSet::from([STD_OUTPUT_FUNCTION.to_string()]);

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
        for stmt in &func.body {
            match stmt {
                Stmt::Import(import) => {
                    validate_import(import, &registry)?;
                    scoped_imports.push(import.clone());
                }
                Stmt::Execute(exec) => {
                    resolve_execute_target(program, func, &scoped_imports, &exec.target)
                        .ok_or_else(|| format!("未找到模块或方法: {}", exec.target))?;
                }
                Stmt::VarDecl(_) => {}
            }
        }
    }

    Ok(())
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

    #[test]
    fn test_alias_resolves_standard_output() {
        let source = "引用 模块：标准库-输入输出-输出 为 输出\n定义 方法 测试（）返回 无：执行 输出：“你好”。。";
        let program = Parser::new(Lexer::new(source))
            .parse_program()
            .expect("Parse failed");

        analyze(&program).expect("Semantic analysis failed");
        let func = &program.functions[0];
        let resolved = resolve_execute_target(&program, func, &program.imports, "输出")
            .expect("Resolve failed");
        assert_eq!(resolved, STD_OUTPUT_FUNCTION);
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
