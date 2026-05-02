use ascompiler::lexer::{Lexer, Token};

#[test]
fn test_simple_main_function() {
    let source = "@声明 入口\n定义 方法 测试（）返回 无：\n。。";
    let mut lexer = Lexer::new(source);

    let expected = vec![
        Token::At,
        Token::Declare,
        Token::Entry,
        Token::Define,
        Token::Method,
        Token::Ident("测试".to_string()),
        Token::LParen,
        Token::RParen,
        Token::ReturnKw,
        Token::VoidKw,
        Token::Colon,
        Token::ScopeEnd,
        Token::Eof,
    ];

    for exp in expected {
        let tok = lexer.next_token();
        assert_eq!(tok, exp, "Token mismatch");
    }
}

#[test]
fn test_struct_definition_tokens() {
    let source = "定义结构坐标：x：小数，y：小数。。";
    let mut lexer = Lexer::new(source);

    let expected = vec![
        Token::Define,
        Token::StructKw,
        Token::Ident("坐标".to_string()),
        Token::Colon,
        Token::Ident("x".to_string()),
        Token::Colon,
        Token::DoubleKw,
        Token::Comma,
        Token::Ident("y".to_string()),
        Token::Colon,
        Token::DoubleKw,
        Token::ScopeEnd,
        Token::Eof,
    ];

    for exp in expected {
        let tok = lexer.next_token();
        assert_eq!(tok, exp, "Token mismatch");
    }
}

#[test]
fn test_object_definition_and_create_tokens() {
    let source = "定义对象向量：结构：x：小数 构造方法（x：小数）：令当前->x=x 公共成员：。。创建向量（1.0）";
    let mut lexer = Lexer::new(source);

    let expected = vec![
        Token::Define,
        Token::ObjectKw,
        Token::Ident("向量".to_string()),
        Token::Colon,
        Token::StructKw,
        Token::Colon,
        Token::Ident("x".to_string()),
        Token::Colon,
        Token::DoubleKw,
        Token::Construct,
        Token::Method,
        Token::LParen,
        Token::Ident("x".to_string()),
        Token::Colon,
        Token::DoubleKw,
        Token::RParen,
        Token::Colon,
        Token::Let,
        Token::Current,
        Token::Arrow,
        Token::Ident("x".to_string()),
        Token::Equals,
        Token::Ident("x".to_string()),
        Token::Ident("公共成员".to_string()),
        Token::Colon,
        Token::ScopeEnd,
        Token::Create,
        Token::Ident("向量".to_string()),
        Token::LParen,
        Token::DoubleLiteral(1.0),
        Token::RParen,
        Token::Eof,
    ];

    for exp in expected {
        let tok = lexer.next_token();
        assert_eq!(tok, exp, "Token mismatch");
    }
}

#[test]
fn test_construct_and_double_literal_tokens() {
    let source = "设 原点=构造坐标：x：0.0。。 原点->x";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::Let);
    assert_eq!(lexer.next_token(), Token::Ident("原点".to_string()));
    assert_eq!(lexer.next_token(), Token::Equals);
    assert_eq!(lexer.next_token(), Token::Construct);
    assert_eq!(lexer.next_token(), Token::Ident("坐标".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Ident("x".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::DoubleLiteral(0.0));
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
    assert_eq!(lexer.next_token(), Token::Ident("原点".to_string()));
    assert_eq!(lexer.next_token(), Token::Arrow);
    assert_eq!(lexer.next_token(), Token::Ident("x".to_string()));
}

#[test]
fn test_no_spaces_between_keywords() {
    let source = "定义方法测试（）返回无：。。";
    let mut lexer = Lexer::new(source);
    assert_eq!(lexer.next_token(), Token::Define);
    assert_eq!(lexer.next_token(), Token::Method);
    assert_eq!(lexer.next_token(), Token::Ident("测试".to_string()));
}

#[test]
fn test_chinese_parentheses() {
    let source = "定义 方法 测试（参数1：整数）返回 无：。。";
    let mut lexer = Lexer::new(source);
    assert_eq!(lexer.next_token(), Token::Define);
    assert_eq!(lexer.next_token(), Token::Method);
    assert_eq!(lexer.next_token(), Token::Ident("测试".to_string()));
    assert_eq!(lexer.next_token(), Token::LParen); // （
    assert_eq!(lexer.next_token(), Token::Ident("参数1".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::IntKw);
    assert_eq!(lexer.next_token(), Token::RParen); // ）
}

#[test]
fn test_array_keyword_and_brackets() {
    let source = "定义变量：数组 arr【10】 设 b=[1,2]";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::Define);
    assert_eq!(lexer.next_token(), Token::Variable);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::ArrayKw);
    assert_eq!(lexer.next_token(), Token::Ident("arr".to_string()));
    assert_eq!(lexer.next_token(), Token::LBracket);
    assert_eq!(lexer.next_token(), Token::IntLiteral(10));
    assert_eq!(lexer.next_token(), Token::RBracket);
    assert_eq!(lexer.next_token(), Token::Let);
    assert_eq!(lexer.next_token(), Token::Ident("b".to_string()));
    assert_eq!(lexer.next_token(), Token::Equals);
    assert_eq!(lexer.next_token(), Token::LBracket);
    assert_eq!(lexer.next_token(), Token::IntLiteral(1));
    assert_eq!(lexer.next_token(), Token::Comma);
    assert_eq!(lexer.next_token(), Token::IntLiteral(2));
    assert_eq!(lexer.next_token(), Token::RBracket);
}

#[test]
fn test_english_scope_end() {
    let source = "定义 方法 f()返回 无：..";
    let mut lexer = Lexer::new(source);
    // skip to scope end
    while lexer.next_token() != Token::Colon {}
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
}

#[test]
fn test_chinese_scope_end() {
    let source = "定义 方法 f()返回 无：。。";
    let mut lexer = Lexer::new(source);
    while lexer.next_token() != Token::Colon {}
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
}

#[test]
fn test_module_import_execute_and_string() {
    let source = "#模块 第一个模块\n引用 模块：标准库-输入输出-输出 为 输出\n执行 输出：“你好”";
    let mut lexer = Lexer::new(source);
    assert_eq!(lexer.next_token(), Token::Hash);
    assert_eq!(lexer.next_token(), Token::Module);
    assert_eq!(lexer.next_token(), Token::Ident("第一个模块".to_string()));
    assert_eq!(lexer.next_token(), Token::Import);
    assert_eq!(lexer.next_token(), Token::Module);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(
        lexer.next_token(),
        Token::Ident("标准库-输入输出-输出".to_string())
    );
    assert_eq!(lexer.next_token(), Token::AsKw);
    assert_eq!(lexer.next_token(), Token::Ident("输出".to_string()));
    assert_eq!(lexer.next_token(), Token::Execute);
    assert_eq!(lexer.next_token(), Token::Ident("输出".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::StringLiteral("你好".to_string()));
}

#[test]
fn test_formatted_string_literal() {
    let source = "执行 输出：f“你好，{名字}”";
    let mut lexer = Lexer::new(source);
    assert_eq!(lexer.next_token(), Token::Execute);
    assert_eq!(lexer.next_token(), Token::Ident("输出".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(
        lexer.next_token(),
        Token::FormattedStringLiteral("你好，{名字}".to_string())
    );
}

#[test]
fn test_take_value_generic_input_tokens() {
    let source = "取值 获取输入->整数：“输入提示词”";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::TakeValue);
    assert_eq!(lexer.next_token(), Token::Ident("获取输入".to_string()));
    assert_eq!(lexer.next_token(), Token::Arrow);
    assert_eq!(lexer.next_token(), Token::IntKw);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(
        lexer.next_token(),
        Token::StringLiteral("输入提示词".to_string())
    );
}

#[test]
fn test_chinese_assignment_operator_without_spaces() {
    let source = "设输入内容为取值 获取输入->整数：“请输入一个数”";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::Let);
    assert_eq!(lexer.next_token(), Token::Ident("输入内容".to_string()));
    assert_eq!(lexer.next_token(), Token::AsKw);
    assert_eq!(lexer.next_token(), Token::TakeValue);
    assert_eq!(lexer.next_token(), Token::Ident("获取输入".to_string()));
    assert_eq!(lexer.next_token(), Token::Arrow);
    assert_eq!(lexer.next_token(), Token::IntKw);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(
        lexer.next_token(),
        Token::StringLiteral("请输入一个数".to_string())
    );
}

#[test]
fn test_condition_keywords_and_cpp_operators() {
    let source = "判断x>=10&&x!=20：若 y<3||!z：判断 a 且 b 或 c：否则：";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::If);
    assert_eq!(lexer.next_token(), Token::Ident("x".to_string()));
    assert_eq!(lexer.next_token(), Token::GreaterEq);
    assert_eq!(lexer.next_token(), Token::IntLiteral(10));
    assert_eq!(lexer.next_token(), Token::AndAnd);
    assert_eq!(lexer.next_token(), Token::Ident("x".to_string()));
    assert_eq!(lexer.next_token(), Token::NotEq);
    assert_eq!(lexer.next_token(), Token::IntLiteral(20));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::ElseIf);
    assert_eq!(lexer.next_token(), Token::Ident("y".to_string()));
    assert_eq!(lexer.next_token(), Token::Less);
    assert_eq!(lexer.next_token(), Token::IntLiteral(3));
    assert_eq!(lexer.next_token(), Token::OrOr);
    assert_eq!(lexer.next_token(), Token::Bang);
    assert_eq!(lexer.next_token(), Token::Ident("z".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::If);
    assert_eq!(lexer.next_token(), Token::Ident("a".to_string()));
    assert_eq!(lexer.next_token(), Token::AndAnd);
    assert_eq!(lexer.next_token(), Token::Ident("b".to_string()));
    assert_eq!(lexer.next_token(), Token::OrOr);
    assert_eq!(lexer.next_token(), Token::Ident("c".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Else);
    assert_eq!(lexer.next_token(), Token::Colon);
}

#[test]
fn test_select_keywords_without_spaces() {
    let source = "当前x：取1：此外：。。";
    let mut lexer = Lexer::new(source);
    assert_eq!(lexer.next_token(), Token::Current);
    assert_eq!(lexer.next_token(), Token::Ident("x".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Case);
    assert_eq!(lexer.next_token(), Token::IntLiteral(1));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Otherwise);
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
}

#[test]
fn test_loop_keywords_without_spaces() {
    let source = "循环计数i<10：循环迭代j<1..5：循环条件i<5：。。。。。。";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::Loop);
    assert_eq!(lexer.next_token(), Token::Count);
    assert_eq!(lexer.next_token(), Token::Ident("i".to_string()));
    assert_eq!(lexer.next_token(), Token::Less);
    assert_eq!(lexer.next_token(), Token::IntLiteral(10));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Loop);
    assert_eq!(lexer.next_token(), Token::Iterate);
    assert_eq!(lexer.next_token(), Token::Ident("j".to_string()));
    assert_eq!(lexer.next_token(), Token::Less);
    assert_eq!(lexer.next_token(), Token::IntLiteral(1));
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
    assert_eq!(lexer.next_token(), Token::IntLiteral(5));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::Loop);
    assert_eq!(lexer.next_token(), Token::Condition);
    assert_eq!(lexer.next_token(), Token::Ident("i".to_string()));
    assert_eq!(lexer.next_token(), Token::Less);
    assert_eq!(lexer.next_token(), Token::IntLiteral(5));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
    assert_eq!(lexer.next_token(), Token::ScopeEnd);
}

#[test]
fn test_external_declaration_and_string_type() {
    let source = "@声明 外部\n定义 方法 输出（内容：字符串）返回 无";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::At);
    assert_eq!(lexer.next_token(), Token::Declare);
    assert_eq!(lexer.next_token(), Token::External);
    assert_eq!(lexer.next_token(), Token::Define);
    assert_eq!(lexer.next_token(), Token::Method);
    assert_eq!(lexer.next_token(), Token::Ident("输出".to_string()));
    assert_eq!(lexer.next_token(), Token::LParen);
    assert_eq!(lexer.next_token(), Token::Ident("内容".to_string()));
    assert_eq!(lexer.next_token(), Token::Colon);
    assert_eq!(lexer.next_token(), Token::StringKw);
    assert_eq!(lexer.next_token(), Token::RParen);
    assert_eq!(lexer.next_token(), Token::ReturnKw);
    assert_eq!(lexer.next_token(), Token::VoidKw);
    assert_eq!(lexer.next_token(), Token::Eof);
}

#[test]
fn test_external_declaration_with_symbol_uses_existing_tokens() {
    let source = "@声明 外部（\"wen_add\"）";
    let mut lexer = Lexer::new(source);

    assert_eq!(lexer.next_token(), Token::At);
    assert_eq!(lexer.next_token(), Token::Declare);
    assert_eq!(lexer.next_token(), Token::External);
    assert_eq!(lexer.next_token(), Token::LParen);
    assert_eq!(
        lexer.next_token(),
        Token::StringLiteral("wen_add".to_string())
    );
    assert_eq!(lexer.next_token(), Token::RParen);
    assert_eq!(lexer.next_token(), Token::Eof);
}
