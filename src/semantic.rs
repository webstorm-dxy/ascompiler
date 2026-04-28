use crate::lexer::Span;
use crate::parser::{
    AssignStmt, BinaryOp, Expr, FormatPart, FunctionDef, ImportDecl, Program, ReturnStmt, Stmt,
    Type, UnaryOp, VarDecl,
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
    let inferred_type = validate_expr(&var.init, program, func, scoped_imports, locals)?;
    let var_type = var.var_type.clone().unwrap_or(inferred_type);
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
            .ok_or_else(|| {
                format!(
                    "未定义的变量 `{}`\n  = 帮助: 请确认名字拼写一致，并且声明出现在使用之前。",
                    name
                )
            }),
        Expr::Call { target, args } => {
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
                    locals
                        .get(name)
                        .ok_or_else(|| format!("未定义的格式化变量 `{}`\n  = 帮助: 格式化字符串 `{{...}}` 中只能引用当前作用域内已声明的变量。", name))?;
                }
            }
            Ok(Type::String)
        }
        Expr::Unary { op, expr } => {
            let ty = validate_expr(expr, program, func, scoped_imports, locals)?;
            match (op, ty) {
                (UnaryOp::Neg, Type::Int) => Ok(Type::Int),
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
                    } else {
                        Err(format!(
                            "算术运算符 `{}` 需要整数操作数\n  = 左侧: {}\n  = 右侧: {}",
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
                    if left_ty == Type::Int && right_ty == Type::Int {
                        Ok(Type::Bool)
                    } else {
                        Err(format!(
                            "比较运算符 `{}` 需要整数操作数\n  = 左侧: {}\n  = 右侧: {}",
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

fn type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Void => "无",
        Type::Int => "整数",
        Type::Double => "小数",
        Type::Float => "浮点",
        Type::Bool => "布尔",
        Type::Char => "字符",
        Type::String => "字符串",
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
