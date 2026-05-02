use ascompiler::codegen::generate;
use ascompiler::lexer::Lexer;
use ascompiler::parser::Parser;
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
