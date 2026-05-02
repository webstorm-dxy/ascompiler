use ascompiler::lexer::Lexer;
use ascompiler::parser::*;

#[test]
fn test_parse_simple_entry_function() {
    let source = "@声明 入口\n定义 方法 测试（）返回 无：\n。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert!(program.has_entry);
    assert_eq!(program.functions.len(), 1);

    let func = &program.functions[0];
    assert_eq!(func.name, "测试");
    assert!(func.params.is_empty());
    assert_eq!(func.return_type, Type::Void);
    assert!(func.is_entry);
    assert!(func.body.is_empty());
}

#[test]
fn test_parse_function_with_params() {
    let source = "定义 方法 计算（参数1：整数，参数2：小数）返回 整数：。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert_eq!(program.functions.len(), 1);
    let func = &program.functions[0];
    assert_eq!(func.name, "计算");
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].name, "参数1");
    assert_eq!(func.params[0].param_type, Type::Int);
    assert_eq!(func.params[1].name, "参数2");
    assert_eq!(func.params[1].param_type, Type::Double);
    assert_eq!(func.return_type, Type::Int);
    assert!(!func.is_entry);
    assert!(func.body.is_empty());
}

#[test]
fn test_parse_english_scope_end() {
    let source = "定义 方法 f（）返回 无：..";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");
    assert_eq!(program.functions.len(), 1);
    assert!(!program.functions[0].is_entry);
    assert!(program.functions[0].body.is_empty());
}

#[test]
fn test_parse_var_decl_auto_type() {
    let source = "定义 方法 测试（）返回 无：定义 变量：x=10。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "x");
            assert_eq!(v.var_type, None);
            assert!(!v.is_mutable);
            assert_eq!(v.init, Some(Expr::IntLiteral(10)));
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_var_decl_explicit_type() {
    let source = "定义 方法 测试（）返回 无：定义 变量：整型 y=20。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "y");
            assert_eq!(v.var_type, Some(Type::Int));
            assert!(!v.is_mutable);
            assert_eq!(v.init, Some(Expr::IntLiteral(20)));
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_let_stmt() {
    let source = "定义 方法 测试（）返回 无：设 z=30。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "z");
            assert_eq!(v.var_type, None);
            assert!(v.is_mutable);
            assert_eq!(v.init, Some(Expr::IntLiteral(30)));
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_var_decl_no_space_between_define_variable() {
    let source = "定义 方法 测试（）返回 无：定义变量：x=10。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "x");
            assert_eq!(v.var_type, None);
            assert!(!v.is_mutable);
            assert_eq!(v.init, Some(Expr::IntLiteral(10)));
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_mutable_var_decl_no_space_between_keywords() {
    let source = "定义 方法 测试（）返回 无：定义可变变量：cnt=0。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "cnt");
            assert!(v.is_mutable);
            assert_eq!(v.init, Some(Expr::IntLiteral(0)));
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_predefined_var_decl_without_initializer() {
    let source = "定义 方法 测试（）返回 无：定义变量：整数x x=10 x=11。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 3);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "x");
            assert_eq!(v.var_type, Some(Type::Int));
            assert!(v.is_mutable);
            assert_eq!(v.init, None);
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
    assert!(matches!(&func.body[1], Stmt::Assign(_)));
    assert!(matches!(&func.body[2], Stmt::Assign(_)));
}

#[test]
fn test_parse_array_literal_with_chinese_brackets() {
    let source = "定义 方法 测试（）返回 无：定义可变变量：数组 arr = 【1，2，3，4】。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "arr");
            assert!(v.is_mutable);
            assert_eq!(
                v.var_type,
                Some(Type::Array {
                    element_type: Box::new(Type::Int),
                    length: None,
                })
            );
            assert_eq!(
                v.init,
                Some(Expr::ArrayLiteral(vec![
                    Expr::IntLiteral(1),
                    Expr::IntLiteral(2),
                    Expr::IntLiteral(3),
                    Expr::IntLiteral(4),
                ]))
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_predefined_array_with_english_brackets() {
    let source = "定义 方法 测试（）返回 无：定义变量：数组 arr[10]。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "arr");
            assert!(v.is_mutable);
            assert_eq!(
                v.var_type,
                Some(Type::Array {
                    element_type: Box::new(Type::Int),
                    length: Some(10),
                })
            );
            assert_eq!(v.init, None);
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_array_index_expression() {
    let source = "定义 方法 测试（）返回 整数：定义变量：数组 arr = [1,2,3] 返回 arr[1]。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[1] {
        Stmt::Return(ReturnStmt { value: Some(expr) }) => {
            assert_eq!(
                expr,
                &Expr::Index {
                    array: Box::new(Expr::Ident("arr".to_string())),
                    index: Box::new(Expr::IntLiteral(1)),
                }
            );
        }
        other => panic!("Expected return with index expression, found {:?}", other),
    }
}

#[test]
fn test_parse_array_element_assignment_with_let_prefix() {
    let source = "定义 方法 测试（）返回 无：设 arr = [1,2,3] 设arr【1】=10 设arr[2]为20。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[1] {
        Stmt::ArrayAssign(assign) => {
            assert_eq!(assign.name, "arr");
            assert_eq!(assign.index, Expr::IntLiteral(1));
            assert_eq!(assign.value, Expr::IntLiteral(10));
        }
        other => panic!("Expected ArrayAssign, found {:?}", other),
    }
    match &func.body[2] {
        Stmt::ArrayAssign(assign) => {
            assert_eq!(assign.name, "arr");
            assert_eq!(assign.index, Expr::IntLiteral(2));
            assert_eq!(assign.value, Expr::IntLiteral(20));
        }
        other => panic!("Expected ArrayAssign, found {:?}", other),
    }
}

#[test]
fn test_parse_chinese_variable_name() {
    let source = "定义 方法 测试（）返回 无：设 结果=100。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::VarDecl(v) => {
            assert_eq!(v.name, "结果");
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_module_import_and_execute() {
    let source = "#模块 第一个模块\n引用 模块：标准库-输入输出-输出 为 输出\n@声明 入口\n定义 方法 测试（）返回 无：执行输出：“你好，世界”。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert_eq!(program.modules.len(), 1);
    assert_eq!(program.modules[0].name, "第一个模块");
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.imports[0].path, "标准库-输入输出-输出");
    assert_eq!(program.imports[0].alias, Some("输出".to_string()));

    let func = &program.functions[0];
    assert_eq!(func.module_path, Some("第一个模块".to_string()));
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::Execute(exec) => {
            assert_eq!(exec.target, "输出");
            assert_eq!(
                exec.args,
                vec![Expr::StringLiteral("你好，世界".to_string())]
            );
        }
        other => panic!("Expected Execute, found {:?}", other),
    }
}

#[test]
fn test_parse_formatted_string() {
    let source = "定义 方法 测试（）返回 无：设 x=10 执行输出：f“hello,{x}”。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[1] {
        Stmt::Execute(exec) => {
            assert_eq!(
                exec.args,
                vec![Expr::FormattedString(vec![
                    FormatPart::Text("hello,".to_string()),
                    FormatPart::Placeholder("x".to_string()),
                ])]
            );
        }
        other => panic!("Expected Execute, found {:?}", other),
    }
}

#[test]
fn test_parse_struct_definition_and_literal() {
    let source = "定义结构坐标：x：小数，y：小数，z：小数。。定义 方法 测试（）返回 无：设 原点=构造坐标：x：0.0，y：0.0，z：0.0。。执行输出：f“x：{原点->x}”。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert_eq!(program.structs.len(), 1);
    assert_eq!(program.structs[0].name, "坐标");
    assert_eq!(program.structs[0].fields.len(), 3);
    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "原点");
            assert_eq!(
                var.init,
                Some(Expr::StructLiteral {
                    name: "坐标".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::DoubleLiteral(0.0)),
                        ("y".to_string(), Expr::DoubleLiteral(0.0)),
                        ("z".to_string(), Expr::DoubleLiteral(0.0)),
                    ],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
    match &func.body[1] {
        Stmt::Execute(exec) => assert_eq!(
            exec.args,
            vec![Expr::FormattedString(vec![
                FormatPart::Text("x：".to_string()),
                FormatPart::Placeholder("原点->x".to_string()),
            ])]
        ),
        other => panic!("Expected Execute, found {:?}", other),
    }
}

#[test]
fn test_parse_object_definition_create_and_method_call() {
    let source = "定义对象向量：结构：x：小数，y：小数 构造方法（x：小数，y：小数）：令当前->x=x 令当前->y=y 公共成员：定义方法相乘（另一个向量：向量）返回 小数：返回 当前->x*另一个向量->x+当前->y*另一个向量->y。。。。定义 方法 测试（）返回 无：设 向量1=创建向量（10.0，15.0）设 向量2=创建向量（10.0，10.0）设 结果=向量1->相乘（向量2）。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert_eq!(program.objects.len(), 1);
    assert_eq!(program.objects[0].name, "向量");
    assert_eq!(program.objects[0].fields.len(), 2);
    assert_eq!(
        program.objects[0]
            .constructor
            .as_ref()
            .unwrap()
            .params
            .len(),
        2
    );
    assert_eq!(program.objects[0].methods.len(), 1);
    assert_eq!(program.functions.len(), 2);

    let func = &program.functions[1];
    match &func.body[0] {
        Stmt::VarDecl(var) => assert_eq!(
            var.init,
            Some(Expr::ObjectCreate {
                name: "向量".to_string(),
                args: vec![Expr::DoubleLiteral(10.0), Expr::DoubleLiteral(15.0)],
            })
        ),
        other => panic!("Expected VarDecl, found {:?}", other),
    }
    match &func.body[2] {
        Stmt::VarDecl(var) => assert_eq!(
            var.init,
            Some(Expr::MethodCall {
                receiver: Box::new(Expr::Ident("向量1".to_string())),
                method: "相乘".to_string(),
                args: vec![Expr::Ident("向量2".to_string())],
            })
        ),
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_struct_field_access_expression() {
    let source = "定义 方法 测试（）返回 无：设 x=原点->x。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(var) => assert_eq!(
            var.init,
            Some(Expr::FieldAccess {
                base: Box::new(Expr::Ident("原点".to_string())),
                field: "x".to_string(),
            })
        ),
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_if_else_and_cpp_expression_precedence() {
    let source = "定义 方法 测试（）返回 无：设 x=5 判断x + 1 > 3 * 2：执行输出：“大”若x==5：执行输出：“等”否则：执行输出：“小”。。。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 2);
    match &func.body[1] {
        Stmt::If(if_stmt) => {
            assert_eq!(if_stmt.branches.len(), 2);
            assert!(if_stmt.else_body.is_some());
            assert_eq!(if_stmt.branches[0].body.len(), 1);
            assert_eq!(if_stmt.branches[1].body.len(), 1);
        }
        other => panic!("Expected If, found {:?}", other),
    }
}

#[test]
fn test_parse_select_with_int_and_string_cases() {
    let source = "定义 方法 测试（）返回 无：设 x=1 当前x：取1：执行输出：“一”取 2：执行输出：“二”此外：执行输出：“其他”。。设 名字=\"问源\" 当前 名字：取“问源”：执行输出：“是”此外：执行输出：“否”。。。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[1] {
        Stmt::Select(select_stmt) => {
            assert_eq!(select_stmt.target, "x");
            assert_eq!(select_stmt.cases.len(), 2);
            assert_eq!(select_stmt.cases[0].value, Expr::IntLiteral(1));
            assert!(select_stmt.default_body.is_some());
        }
        other => panic!("Expected Select, found {:?}", other),
    }
    match &func.body[3] {
        Stmt::Select(select_stmt) => {
            assert_eq!(select_stmt.target, "名字");
            assert_eq!(
                select_stmt.cases[0].value,
                Expr::StringLiteral("问源".to_string())
            );
        }
        other => panic!("Expected Select, found {:?}", other),
    }
}

#[test]
fn test_parse_count_loop_without_spaces() {
    let source = "定义 方法 测试（）返回 无：循环计数i<10：执行输出：f“{i}”。。。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::Loop(LoopStmt::Count {
            var_name,
            end,
            body,
        }) => {
            assert_eq!(var_name, "i");
            assert_eq!(end, &Expr::IntLiteral(10));
            assert_eq!(body.len(), 1);
        }
        other => panic!("Expected count loop, found {:?}", other),
    }
}

#[test]
fn test_parse_condition_loop_with_space() {
    let source = "定义 方法 测试（）返回 无：设 x=1 循环 条件 x<3：执行输出：“x”。。。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 2);
    match &func.body[1] {
        Stmt::Loop(LoopStmt::Condition { condition, body }) => {
            assert_eq!(
                condition,
                &Expr::Binary {
                    left: Box::new(Expr::Ident("x".to_string())),
                    op: BinaryOp::Less,
                    right: Box::new(Expr::IntLiteral(3)),
                }
            );
            assert_eq!(body.len(), 1);
        }
        other => panic!("Expected condition loop, found {:?}", other),
    }
}

#[test]
fn test_parse_iterate_loop_range_without_spaces() {
    let source = "定义 方法 测试（）返回 无：循环迭代i<1..5：执行输出：f“{i}”。。。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.body.len(), 1);
    match &func.body[0] {
        Stmt::Loop(LoopStmt::Iterate {
            var_name,
            start,
            end,
            body,
        }) => {
            assert_eq!(var_name, "i");
            assert_eq!(start, &Expr::IntLiteral(1));
            assert_eq!(end, &Expr::IntLiteral(5));
            assert_eq!(body.len(), 1);
        }
        other => panic!("Expected iterate loop, found {:?}", other),
    }
}

#[test]
fn test_parse_returning_count_loop_with_assignment() {
    let source = "定义 方法 从零求和（结束值：整数）返回 整数：设 cnt = 0 循环计数i<结束值：cnt=cnt+1。。返回 cnt。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.return_type, Type::Int);
    assert_eq!(func.params[0].name, "结束值");
    assert_eq!(func.body.len(), 3);
    match &func.body[1] {
        Stmt::Loop(LoopStmt::Count {
            var_name,
            end,
            body,
        }) => {
            assert_eq!(var_name, "i");
            assert_eq!(end, &Expr::Ident("结束值".to_string()));
            assert_eq!(body.len(), 1);
            match &body[0] {
                Stmt::Assign(assign) => assert_eq!(assign.name, "cnt"),
                other => panic!("Expected assignment, found {:?}", other),
            }
        }
        other => panic!("Expected count loop, found {:?}", other),
    }
    assert!(matches!(&func.body[2], Stmt::Return(_)));
}

#[test]
fn test_parse_call_expression_in_let() {
    let source = "定义 方法 从零求和（结束值：整数）返回 整数：返回 结束值。。定义 方法 测试（）返回 无：设 s = 从零求和（10）。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[1];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "s");
            assert_eq!(
                var.init,
                Some(Expr::Call {
                    target: "从零求和".to_string(),
                    type_arg: None,
                    args: vec![Expr::IntLiteral(10)],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_take_value_generic_call() {
    let source = "定义 方法 测试（）返回 无：设 s = 取值 获取输入->整数：“输入提示词”。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "s");
            assert_eq!(
                var.init,
                Some(Expr::Call {
                    target: "获取输入".to_string(),
                    type_arg: Some(Type::Int),
                    args: vec![Expr::StringLiteral("输入提示词".to_string())],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_take_value_generic_call_without_args() {
    let source = "定义 方法 测试（）返回 无：设 s = 取值 获取输入->整数。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "s");
            assert_eq!(
                var.init,
                Some(Expr::Call {
                    target: "获取输入".to_string(),
                    type_arg: Some(Type::Int),
                    args: vec![],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_take_value_call_without_generic_type() {
    let source = "定义 方法 fib（项数：整数）返回 整数：判断项数<=1：返回 项数 否则：返回 fib（项数-1）+fib（项数-2）。。。。定义 方法 测试（）返回 无：设 结果为取值fib：10。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[1];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "结果");
            assert_eq!(
                var.init,
                Some(Expr::Call {
                    target: "fib".to_string(),
                    type_arg: None,
                    args: vec![Expr::IntLiteral(10)],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_let_with_chinese_assignment_operator() {
    let source = "定义 方法 测试（）返回 无：设输入内容为取值 获取输入->整数：“请输入一个数”。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[0] {
        Stmt::VarDecl(var) => {
            assert_eq!(var.name, "输入内容");
            assert_eq!(
                var.init,
                Some(Expr::Call {
                    target: "获取输入".to_string(),
                    type_arg: Some(Type::Int),
                    args: vec![Expr::StringLiteral("请输入一个数".to_string())],
                })
            );
        }
        other => panic!("Expected VarDecl, found {:?}", other),
    }
}

#[test]
fn test_parse_assignment_with_chinese_assignment_operator() {
    let source = "定义 方法 测试（）返回 无：设 cnt = 0 cnt为cnt+1。。";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    match &func.body[1] {
        Stmt::Assign(assign) => {
            assert_eq!(assign.name, "cnt");
            assert_eq!(
                assign.value,
                Expr::Binary {
                    left: Box::new(Expr::Ident("cnt".to_string())),
                    op: BinaryOp::Add,
                    right: Box::new(Expr::IntLiteral(1)),
                }
            );
        }
        other => panic!("Expected Assign, found {:?}", other),
    }
}

#[test]
fn test_parse_external_function_declaration() {
    let source = "#模块 标准库-输入输出\n@声明 外部\n定义 方法 输出（内容：字符串）返回 无";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    assert_eq!(program.modules[0].name, "标准库-输入输出");
    assert_eq!(program.functions.len(), 1);

    let func = &program.functions[0];
    assert_eq!(func.module_path, Some("标准库-输入输出".to_string()));
    assert_eq!(func.name, "输出");
    assert!(func.is_external);
    assert!(!func.is_entry);
    assert_eq!(func.external_symbol, None);
    assert!(func.body.is_empty());
    assert_eq!(func.params[0].param_type, Type::String);
}

#[test]
fn test_parse_external_function_declaration_with_symbol() {
    let source =
        "#模块 Rust扩展\n@声明 外部(\"wen_add\")\n定义 方法 相加（左：整数，右：整数）返回 整数";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert_eq!(func.module_path, Some("Rust扩展".to_string()));
    assert_eq!(func.name, "相加");
    assert!(func.is_external);
    assert_eq!(func.external_symbol, Some("wen_add".to_string()));
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.return_type, Type::Int);
}

#[test]
fn test_parse_external_function_declaration_with_chinese_symbol_parens() {
    let source =
        "#模块 Rust扩展\n@声明 外部（\"wen_print\"）\n定义 方法 打印（内容：字符串）返回 无";
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let program = parser.parse_program().expect("Parse failed");

    let func = &program.functions[0];
    assert!(func.is_external);
    assert_eq!(func.external_symbol, Some("wen_print".to_string()));
    assert_eq!(func.return_type, Type::Void);
}
