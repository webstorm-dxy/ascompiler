use ascompiler::lexer::Lexer;
use ascompiler::parser::Parser;
use ascompiler::{semantic, stdlib};

use semantic::{analyze, resolve_execute_target};

#[test]
fn test_alias_resolves_standard_output() {
    let source =
        "引用 模块：标准库-输入输出-输出 为 输出\n定义 方法 测试（）返回 无：执行 输出：“你好”。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");
    let program = stdlib::merge_with_standard_library(program).expect("std merge failed");

    analyze(&program).expect("Semantic analysis failed");
    let func = program.functions.iter().find(|f| f.name == "测试").unwrap();
    let resolved =
        resolve_execute_target(&program, func, &program.imports, "输出").expect("Resolve failed");
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
fn test_select_cases_must_match_target_type() {
    let source = "定义 方法 测试（）返回 无：设 x=1 当前x：取“不是整数”：返回。。。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected semantic error");
    assert!(err.contains("选择分支类型不匹配"));
}

#[test]
fn test_select_accepts_string_cases() {
    let source =
        "定义 方法 测试（）返回 无：设 名字=\"问源\" 当前 名字：取“问源”：返回 此外：返回。。。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_assignment_and_return_in_iterate_loop() {
    let source =
        "定义 方法 求和（）返回 整数：设 cnt = 0 循环迭代i<1..5：cnt=cnt+i。。返回 cnt。。";
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
fn test_predefined_variable_is_mutable_by_default() {
    let source = "定义 方法 测试（）返回 无：定义变量：整数x x=10 x=11。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_predefined_variable_requires_explicit_type() {
    let source = "定义 方法 测试（）返回 无：定义变量：x x=10。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected missing type error");
    assert!(err.contains("预定义变量 `x` 缺少类型"));
}

#[test]
fn test_array_literal_and_predefined_array_are_valid() {
    let source = "定义 方法 测试（）返回 无：定义可变变量：数组 arr = 【1，2，3，4】 定义变量：数组 empty[10]。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_predefined_array_requires_length() {
    let source = "定义 方法 测试（）返回 无：定义变量：数组 arr。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected missing array length error");
    assert!(err.contains("数组预定义 `arr` 缺少长度"));
}

#[test]
fn test_array_index_returns_element_type() {
    let source = "定义 方法 测试（）返回 整数：定义变量：数组 arr = [1,2,3] 返回 arr[1]。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_array_index_must_be_in_bounds_when_literal() {
    let source = "定义 方法 测试（）返回 整数：定义变量：数组 arr = [1,2,3] 返回 arr[3]。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected array index error");
    assert!(err.contains("数组下标越界"));
}

#[test]
fn test_array_element_assignment_is_valid() {
    let source = "定义 方法 测试（）返回 整数：设 arr = [1,2,3] 设arr【1】=10 设arr[2]为20 返回 arr[1]+arr[2]。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_struct_literal_and_field_access() {
    let source = "定义结构坐标：x：小数，y：小数，z：小数。。定义 方法 测试（）返回 小数：设 原点=构造坐标：x：0.0，y：1.0，z：2.0。。返回 原点->x。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_struct_literal_rejects_wrong_field_type() {
    let source =
        "定义结构坐标：x：小数。。定义 方法 测试（）返回 无：设 原点=构造坐标：x：1。。。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Semantic analysis should fail");
    assert!(err.contains("字段 `x` 类型不匹配"));
}

#[test]
fn test_object_create_and_public_method_call() {
    let source = "定义对象向量：结构：x：小数，y：小数 构造方法（x：小数，y：小数）：令当前->x=x 令当前->y=y 公共成员：定义方法相乘（另一个向量：向量）返回 小数：返回 当前->x*另一个向量->x+当前->y*另一个向量->y。。。。定义 方法 测试（）返回 小数：设 向量1=创建向量（10.0，15.0）设 向量2=创建向量（10.0，10.0）返回 向量1->相乘（向量2）。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_object_fields_are_private_outside_object() {
    let source = "定义对象向量：结构：x：小数 公共成员：定义方法读取x（）返回 小数：返回 当前->x。。。。定义 方法 测试（）返回 小数：设 向量1=创建向量（）返回 向量1->x。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected private field error");
    assert!(err.contains("默认私有"));
}

#[test]
fn test_immutable_array_element_assignment_is_rejected() {
    let source = "定义 方法 测试（）返回 无：定义变量：数组 arr = [1,2,3] 设arr[1]=10。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    let err = analyze(&program).expect_err("Expected immutable array assignment error");
    assert!(err.contains("不可变"));
}

#[test]
fn test_function_call_expression_uses_return_type() {
    let source = "定义 方法 从零求和（结束值：整数）返回 整数：返回 结束值。。定义 方法 测试（）返回 无：设 s = 从零求和（10）。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_take_value_call_without_generic_type_uses_return_type() {
    let source = "定义 方法 fib（项数：整数）返回 整数：判断项数<=1：返回 项数 否则：返回 fib（项数-1）+fib（项数-2）。。。。定义 方法 测试（）返回 无：设 结果为取值fib：10。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_take_value_input_int_uses_std_return_type() {
    let source = "引用 模块：标准库-输入输出-获取输入 为 获取输入\n定义 方法 测试（）返回 无：设 s = 取值 获取输入->整数。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");
    let program = stdlib::merge_with_standard_library(program).expect("std merge failed");

    analyze(&program).expect("Semantic analysis failed");
}

#[test]
fn test_take_value_generic_type_must_match_return_type() {
    let source = "引用 模块：标准库-输入输出-获取输入 为 获取输入\n定义 方法 测试（）返回 无：设 s = 取值 获取输入->字符串：“输入提示词”。。";
    let program = Parser::new(Lexer::new(source))
        .parse_program()
        .expect("Parse failed");
    let program = stdlib::merge_with_standard_library(program).expect("std merge failed");

    let err = analyze(&program).expect_err("Expected generic mismatch");
    assert!(err.contains("泛型类型不匹配"));
}
