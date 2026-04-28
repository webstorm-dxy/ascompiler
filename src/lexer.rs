/// Lexer for the 问源 programming language.
///
/// Handles both Chinese and English punctuation.
/// Chinese keywords (1-2 chars) are recognized even without whitespace separation.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Annotations
    At,       // @
    Declare,  // 声明
    Entry,    // 入口
    External, // 外部

    // Definitions
    Define, // 定义
    Method, // 方法
    Module, // 模块

    // Control flow
    ReturnKw,  // 返回
    If,        // 判断
    ElseIf,    // 若
    Else,      // 否则
    Loop,      // 循环
    Count,     // 计数
    Condition, // 条件

    // Types
    VoidKw,    // 无
    IntKw,     // 整数
    DoubleKw,  // 小数
    FloatKw,   // 浮点
    BoolKw,    // 布尔
    CharKw,    // 字符
    StringKw,  // 字符串
    IntTypeKw, // 整型 (integer type, used in variable declarations)

    // Variable definition
    Variable, // 变量
    Let,      // 设 (simplified variable definition)

    // Modules and calls
    Hash,    // #
    Import,  // 引用
    AsKw,    // 为
    Execute, // 执行

    // Symbols (bilingual: Chinese and English)
    LParen,    // ( or （
    RParen,    // ) or ）
    Colon,     // : or ：
    ScopeEnd,  // .. or 。。
    Comma,     // , or ，
    Equals,    // =
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    EqEq,      // ==
    NotEq,     // !=
    Less,      // <
    LessEq,    // <=
    Greater,   // >
    GreaterEq, // >=
    AndAnd,    // &&
    OrOr,      // ||
    Bang,      // !

    // Values
    Ident(String),
    IntLiteral(i64),
    StringLiteral(String),
    FormattedStringLiteral(String),

    // Special
    Eof,
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
        }
    }

    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current() {
            if ch.is_whitespace() {
                self.advance();
            } else if ch == '/' && self.peek(1) == Some('/') {
                while let Some(ch) = self.current() {
                    self.advance();
                    if ch == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Check if a character is a CJK unified ideograph (not punctuation).
    fn is_cjk_ideograph(ch: char) -> bool {
        ('\u{4e00}'..='\u{9fff}').contains(&ch)   // CJK Unified Ideographs
            || ('\u{3400}'..='\u{4dbf}').contains(&ch) // Extension A
            || ('\u{f900}'..='\u{faff}').contains(&ch) // Compatibility
    }

    /// Try to match a keyword at the current position.
    /// Returns Some(Token) if matched, advances the cursor.
    /// Returns None if no keyword matches (cursor unchanged).
    fn match_keyword(&mut self) -> Option<Token> {
        let ch1 = self.current()?;
        if !Self::is_cjk_ideograph(ch1) {
            return None;
        }

        // Check longer keywords first.
        if let (Some(ch2), Some(ch3)) = (self.peek(1), self.peek(2)) {
            if Self::is_cjk_ideograph(ch2) && Self::is_cjk_ideograph(ch3) {
                let triple = [ch1, ch2, ch3];
                if triple == ['字', '符', '串'] {
                    self.advance();
                    self.advance();
                    self.advance();
                    return Some(Token::StringKw);
                }
            }
        }

        // Check 2-char keywords.
        if let Some(ch2) = self.peek(1) {
            if Self::is_cjk_ideograph(ch2) {
                let pair = [ch1, ch2];
                let token_opt = match pair {
                    ['声', '明'] => Some(Token::Declare),
                    ['入', '口'] => Some(Token::Entry),
                    ['外', '部'] => Some(Token::External),
                    ['定', '义'] => Some(Token::Define),
                    ['方', '法'] => Some(Token::Method),
                    ['模', '块'] => Some(Token::Module),
                    ['返', '回'] => Some(Token::ReturnKw),
                    ['判', '断'] => Some(Token::If),
                    ['否', '则'] => Some(Token::Else),
                    ['循', '环'] => Some(Token::Loop),
                    ['计', '数'] => Some(Token::Count),
                    ['条', '件'] => Some(Token::Condition),
                    ['引', '用'] => Some(Token::Import),
                    ['执', '行'] => Some(Token::Execute),
                    ['整', '数'] => Some(Token::IntKw),
                    ['小', '数'] => Some(Token::DoubleKw),
                    ['浮', '点'] => Some(Token::FloatKw),
                    ['布', '尔'] => Some(Token::BoolKw),
                    ['字', '符'] => Some(Token::CharKw),
                    ['变', '量'] => Some(Token::Variable),
                    ['整', '型'] => Some(Token::IntTypeKw),
                    _ => None,
                };

                if let Some(token) = token_opt {
                    self.advance();
                    self.advance();
                    return Some(token);
                }
                // Not a 2-char keyword, fall through to 1-char check
            }
        }

        // Check 1-char keyword
        if ch1 == '无' {
            self.advance();
            return Some(Token::VoidKw);
        }
        if ch1 == '设' {
            self.advance();
            return Some(Token::Let);
        }
        if ch1 == '为' {
            self.advance();
            return Some(Token::AsKw);
        }
        if ch1 == '若' {
            self.advance();
            return Some(Token::ElseIf);
        }
        if ch1 == '且' {
            self.advance();
            return Some(Token::AndAnd);
        }
        if ch1 == '或' {
            self.advance();
            return Some(Token::OrOr);
        }

        None
    }

    /// Check if the current position starts a keyword (lookahead, no consume).
    fn would_match_keyword(&self) -> bool {
        let ch1 = match self.current() {
            Some(c) => c,
            None => return false,
        };
        if !Self::is_cjk_ideograph(ch1) {
            return false;
        }

        // Check longer keywords first.
        if let (Some(ch2), Some(ch3)) = (self.peek(1), self.peek(2)) {
            if Self::is_cjk_ideograph(ch2)
                && Self::is_cjk_ideograph(ch3)
                && [ch1, ch2, ch3] == ['字', '符', '串']
            {
                return true;
            }
        }

        // Check 2-char keywords
        if let Some(ch2) = self.peek(1) {
            if Self::is_cjk_ideograph(ch2) {
                let pair = [ch1, ch2];
                if matches!(
                    pair,
                    ['声', '明']
                        | ['入', '口']
                        | ['外', '部']
                        | ['定', '义']
                        | ['方', '法']
                        | ['模', '块']
                        | ['返', '回']
                        | ['判', '断']
                        | ['否', '则']
                        | ['循', '环']
                        | ['计', '数']
                        | ['条', '件']
                        | ['引', '用']
                        | ['执', '行']
                        | ['整', '数']
                        | ['小', '数']
                        | ['浮', '点']
                        | ['布', '尔']
                        | ['字', '符']
                        | ['变', '量']
                        | ['整', '型']
                ) {
                    return true;
                }
            }
        }

        // Check 1-char keyword
        ch1 == '无' || ch1 == '设' || ch1 == '为' || ch1 == '若' || ch1 == '且' || ch1 == '或'
    }

    /// Read an integer literal starting at the current position.
    fn read_number(&mut self) -> Token {
        let mut num_str = String::new();
        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let val: i64 = num_str.parse().unwrap_or(0);
        Token::IntLiteral(val)
    }

    /// Read a string literal. Supports English and Chinese quote pairs.
    fn read_string(&mut self, closing: char) -> Result<Token, String> {
        self.advance();
        let mut value = String::new();
        while let Some(ch) = self.current() {
            if ch == closing {
                self.advance();
                return Ok(Token::StringLiteral(value));
            }
            value.push(ch);
            self.advance();
        }
        Err("Unterminated string literal".to_string())
    }

    /// Read an f-string literal after the leading `f`.
    fn read_formatted_string(&mut self, quote: char, closing: char) -> Result<Token, String> {
        self.advance();
        if self.current() != Some(quote) {
            return Err("Expected quote after f-string prefix".to_string());
        }
        match self.read_string(closing)? {
            Token::StringLiteral(value) => Ok(Token::FormattedStringLiteral(value)),
            _ => unreachable!(),
        }
    }

    /// Read an identifier starting at the current position.
    /// Stops at whitespace, symbols, or when a new keyword begins.
    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.current() {
            if ch.is_whitespace() {
                break;
            }
            // Stop at any symbol character
            if matches!(
                ch,
                '@' | '('
                    | ')'
                    | '（'
                    | '）'
                    | ':'
                    | '：'
                    | ','
                    | '，'
                    | '.'
                    | '。'
                    | '='
                    | '+'
                    | '*'
                    | '/'
                    | '%'
                    | '!'
                    | '<'
                    | '>'
                    | '&'
                    | '|'
            ) {
                break;
            }
            if ch == '-' {
                let prev_is_cjk = ident
                    .chars()
                    .last()
                    .map(Self::is_cjk_ideograph)
                    .unwrap_or(false);
                let next_is_cjk = self.peek(1).map(Self::is_cjk_ideograph).unwrap_or(false);
                if !prev_is_cjk || !next_is_cjk {
                    break;
                }
            }
            // If a new identifier would start with a keyword, stop. Once an
            // identifier has started, keyword text may be part of the name.
            if ident.is_empty() && Self::is_cjk_ideograph(ch) && self.would_match_keyword() {
                break;
            }
            ident.push(ch);
            self.advance();
        }
        ident
    }

    /// Get the next token from the source.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let ch = match self.current() {
            Some(c) => c,
            None => return Token::Eof,
        };

        match ch {
            'f' if self.peek(1) == Some('"') => match self.read_formatted_string('"', '"') {
                Ok(token) => token,
                Err(msg) => {
                    eprintln!("Warning: {}", msg);
                    Token::Eof
                }
            },
            'f' if self.peek(1) == Some('“') => match self.read_formatted_string('“', '”') {
                Ok(token) => token,
                Err(msg) => {
                    eprintln!("Warning: {}", msg);
                    Token::Eof
                }
            },
            '#' => {
                self.advance();
                Token::Hash
            }
            '@' => {
                self.advance();
                Token::At
            }
            '"' => match self.read_string('"') {
                Ok(token) => token,
                Err(msg) => {
                    eprintln!("Warning: {}", msg);
                    Token::Eof
                }
            },
            '“' => match self.read_string('”') {
                Ok(token) => token,
                Err(msg) => {
                    eprintln!("Warning: {}", msg);
                    Token::Eof
                }
            },
            '(' | '（' => {
                self.advance();
                Token::LParen
            }
            ')' | '）' => {
                self.advance();
                Token::RParen
            }
            ':' | '：' => {
                self.advance();
                Token::Colon
            }
            ',' | '，' => {
                self.advance();
                Token::Comma
            }
            '.' => {
                // English scope end: ..
                self.advance();
                if self.current() == Some('.') {
                    self.advance();
                }
                Token::ScopeEnd
            }
            '。' => {
                // Chinese scope end: 。。
                self.advance();
                if self.current() == Some('。') {
                    self.advance();
                }
                Token::ScopeEnd
            }
            '=' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Equals
                }
            }
            '+' => {
                self.advance();
                Token::Plus
            }
            '-' => {
                self.advance();
                Token::Minus
            }
            '*' => {
                self.advance();
                Token::Star
            }
            '/' => {
                self.advance();
                Token::Slash
            }
            '%' => {
                self.advance();
                Token::Percent
            }
            '!' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::NotEq
                } else {
                    Token::Bang
                }
            }
            '<' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::LessEq
                } else {
                    Token::Less
                }
            }
            '>' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::GreaterEq
                } else {
                    Token::Greater
                }
            }
            '&' => {
                self.advance();
                if self.current() == Some('&') {
                    self.advance();
                    Token::AndAnd
                } else {
                    eprintln!("Warning: unrecognized character: '&'");
                    self.next_token()
                }
            }
            '|' => {
                self.advance();
                if self.current() == Some('|') {
                    self.advance();
                    Token::OrOr
                } else {
                    eprintln!("Warning: unrecognized character: '|'");
                    self.next_token()
                }
            }
            _ => {
                // Number literals
                if ch.is_ascii_digit() {
                    return self.read_number();
                }
                // Try keyword first, then identifier
                if let Some(token) = self.match_keyword() {
                    token
                } else {
                    let ident = self.read_identifier();
                    if ident.is_empty() {
                        // Should not happen but fallback
                        eprintln!("Warning: unrecognized character: '{}'", ch);
                        self.advance();
                        self.next_token()
                    } else {
                        Token::Ident(ident)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_loop_keywords_without_spaces() {
        let source = "循环计数i<10：循环条件i<5：。。。。";
        let mut lexer = Lexer::new(source);

        assert_eq!(lexer.next_token(), Token::Loop);
        assert_eq!(lexer.next_token(), Token::Count);
        assert_eq!(lexer.next_token(), Token::Ident("i".to_string()));
        assert_eq!(lexer.next_token(), Token::Less);
        assert_eq!(lexer.next_token(), Token::IntLiteral(10));
        assert_eq!(lexer.next_token(), Token::Colon);
        assert_eq!(lexer.next_token(), Token::Loop);
        assert_eq!(lexer.next_token(), Token::Condition);
        assert_eq!(lexer.next_token(), Token::Ident("i".to_string()));
        assert_eq!(lexer.next_token(), Token::Less);
        assert_eq!(lexer.next_token(), Token::IntLiteral(5));
        assert_eq!(lexer.next_token(), Token::Colon);
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
}
