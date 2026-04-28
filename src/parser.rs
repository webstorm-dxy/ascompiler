/// Parser for the 问源 programming language.
///
/// Converts a token stream into an AST. Handles `@声明 入口` annotations
/// that mark the following function as the program entry point.
use crate::lexer::{Lexer, Span, Token};

// ---------------------------------------------------------------------------
// AST node definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub has_entry: bool,
    pub modules: Vec<ModuleDef>,
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    /// Original function name from source.
    pub name: String,
    /// Module path containing this function, if any.
    pub module_path: Option<String>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type.
    pub return_type: Type,
    /// True if this function is the program entry point (main).
    pub is_entry: bool,
    /// True if this function is implemented outside 问源 source.
    pub is_external: bool,
    /// Function body statements.
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl(VarDecl),
    Assign(AssignStmt),
    Return(ReturnStmt),
    Import(ImportDecl),
    Execute(ExecuteStmt),
    If(IfStmt),
    Loop(LoopStmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignStmt {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecuteStmt {
    pub target: String,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    /// Variable name.
    pub name: String,
    /// Source span for the variable name.
    pub name_span: Span,
    /// Source span for the whole declaration statement.
    pub span: Span,
    /// Optional explicit type (None = type inferred from initializer).
    pub var_type: Option<Type>,
    /// True when the variable can be assigned after declaration.
    pub is_mutable: bool,
    /// Initializer expression.
    pub init: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub branches: Vec<IfBranch>,
    pub else_body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfBranch {
    pub condition: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoopStmt {
    Count {
        var_name: String,
        end: Expr,
        body: Vec<Stmt>,
    },
    Condition {
        condition: Expr,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
    FormattedString(Vec<FormatPart>),
    Ident(String),
    Call {
        target: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormatPart {
    Text(String),
    Placeholder(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub param_type: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Void,
    Int,
    Double,
    Float,
    Bool,
    Char,
    String,
}

impl Type {
    pub fn from_token(tok: &Token) -> Option<Type> {
        match tok {
            Token::VoidKw => Some(Type::Void),
            Token::IntKw => Some(Type::Int),
            Token::DoubleKw => Some(Type::Double),
            Token::FloatKw => Some(Type::Float),
            Token::BoolKw => Some(Type::Bool),
            Token::CharKw => Some(Type::Char),
            Token::StringKw => Some(Type::String),
            Token::IntTypeKw => Some(Type::Int),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub struct Parser {
    lexer: Lexer,
    current: Token,
    current_span: Span,
    /// When true, the next function definition is the entry point.
    next_is_entry: bool,
    /// Whether this program has an entry point defined.
    has_entry: bool,
    /// When true, the next function definition is an external declaration.
    next_is_external: bool,
    /// Current module path for following top-level function definitions.
    current_module: Option<String>,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        let mut lexer = lexer;
        let current = lexer.next_token();
        let current_span = lexer.last_span();
        Parser {
            lexer,
            current,
            current_span,
            next_is_entry: false,
            has_entry: false,
            next_is_external: false,
            current_module: None,
        }
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
        self.current_span = self.lexer.last_span();
    }

    fn error(&self, title: impl Into<String>, help: impl Into<String>) -> String {
        render_diagnostic(
            "解析错误",
            title.into(),
            self.lexer.source_name(),
            self.lexer.source_text(),
            self.current_span,
            Some(help.into()),
        )
    }

    fn lexical_error(&self, message: &str) -> String {
        render_diagnostic(
            "词法错误",
            message.to_string(),
            self.lexer.source_name(),
            self.lexer.source_text(),
            self.current_span,
            Some("编译器在这里无法继续稳定地切分 token；请先修正这个字符或字面量。".to_string()),
        )
    }

    /// Expect `expected`, advance if matched, error otherwise.
    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if &self.current == expected {
            self.advance();
            Ok(())
        } else if let Token::Error(message) = &self.current {
            Err(self.lexical_error(message))
        } else {
            Err(self.error(
                format!(
                    "期望 `{}`，但找到了 `{}`",
                    token_name(expected),
                    token_name(&self.current)
                ),
                format!(
                    "在这里补上 `{}`，或检查前一行是否少写了分隔符。",
                    token_name(expected)
                ),
            ))
        }
    }

    /// Expect an identifier, return its value.
    fn expect_ident(&mut self) -> Result<String, String> {
        self.expect_ident_span().map(|(name, _)| name)
    }

    /// Expect an identifier, return its value and source span.
    fn expect_ident_span(&mut self) -> Result<(String, Span), String> {
        match &self.current {
            Token::Ident(name) => {
                let name = name.clone();
                let span = self.current_span;
                self.advance();
                Ok((name, span))
            }
            Token::Error(message) => Err(self.lexical_error(message)),
            other => Err(self.error(
                format!("期望标识符，但找到了 `{}`", token_name(other)),
                "标识符通常是方法名、变量名或模块路径，例如 `测试`、`结果`、`标准库-输入输出`。",
            )),
        }
    }

    /// Parse the entire program.
    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut modules = Vec::new();
        let mut imports = Vec::new();
        let mut functions = Vec::new();

        while self.current != Token::Eof {
            match &self.current {
                Token::Hash => {
                    let module = self.parse_module_def()?;
                    self.current_module = Some(module.name.clone());
                    modules.push(module);
                }
                Token::Import => {
                    imports.push(self.parse_import_decl()?);
                }
                Token::At => {
                    self.parse_declaration()?;
                }
                Token::Define => {
                    let func = self.parse_function_def()?;
                    functions.push(func);
                }
                Token::Error(message) => {
                    return Err(self.lexical_error(message));
                }
                other => {
                    return Err(self.error(
                        format!("顶层不允许出现 `{}`", token_name(other)),
                        "顶层只能写模块声明 `#模块 ...`、引用声明 `引用 模块：...`、`@声明 ...` 或 `定义 方法 ...`。",
                    ));
                }
            }
        }

        Ok(Program {
            has_entry: self.has_entry,
            modules,
            imports,
            functions,
        })
    }

    /// Parse `#模块 module-path`
    fn parse_module_def(&mut self) -> Result<ModuleDef, String> {
        self.expect(&Token::Hash)?;
        self.expect(&Token::Module)?;
        let name = self.expect_ident()?;
        Ok(ModuleDef { name })
    }

    /// Parse `引用 模块：path [为 alias]`
    fn parse_import_decl(&mut self) -> Result<ImportDecl, String> {
        self.expect(&Token::Import)?;
        self.expect(&Token::Module)?;
        self.expect(&Token::Colon)?;
        let path = self.expect_ident()?;
        let alias = if self.current == Token::AsKw {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(ImportDecl { path, alias })
    }

    /// Parse `@声明 入口`
    fn parse_declaration(&mut self) -> Result<(), String> {
        self.expect(&Token::At)?;
        self.expect(&Token::Declare)?;

        // Check what follows 声明
        match &self.current {
            Token::Entry => {
                self.advance();
                self.next_is_entry = true;
                Ok(())
            }
            Token::External => {
                self.advance();
                self.next_is_external = true;
                Ok(())
            }
            Token::Error(message) => Err(self.lexical_error(message)),
            other => Err(self.error(
                format!(
                    "@声明 后只能写 `入口` 或 `外部`，但找到了 `{}`",
                    token_name(other)
                ),
                "如果要声明入口点，请写 `@声明 入口`；如果要声明外部函数，请写 `@声明 外部`。",
            )),
        }
    }

    /// Parse a function definition:
    /// `定义 方法 name (params) 返回 return_type : ... scope_end`
    /// or with Chinese punctuation variants.
    fn parse_function_def(&mut self) -> Result<FunctionDef, String> {
        self.expect(&Token::Define)?;

        // Expect definition type keyword: currently only 方法 is supported
        match &self.current {
            Token::Method => {
                self.advance();
            }
            Token::Error(message) => return Err(self.lexical_error(message)),
            other => {
                return Err(self.error(
                    format!("`定义` 后期望 `方法`，但找到了 `{}`", token_name(other)),
                    "方法定义形如 `定义 方法 名称（参数：类型）返回 类型：...。。`；变量定义只能写在方法体里。",
                ));
            }
        }

        // Function name
        let name = self.expect_ident()?;

        // Parameters: ( ... ) or （ ... ）
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        // Return type: 返回 <type>
        self.expect(&Token::ReturnKw)?;
        let return_type = self.parse_type()?;

        // Determine if this is the entry point
        let is_entry = self.next_is_entry;
        if is_entry {
            self.has_entry = true;
            self.next_is_entry = false;
        }

        let is_external = self.next_is_external;
        if is_external {
            self.next_is_external = false;
        }

        let body = if is_external {
            Vec::new()
        } else {
            // Scope start: : or ：
            self.expect(&Token::Colon)?;
            // Parse function body
            self.parse_body()?
        };

        Ok(FunctionDef {
            name,
            module_path: self.current_module.clone(),
            params,
            return_type,
            is_entry,
            is_external,
            body,
        })
    }

    /// Parse comma-separated parameters: param (, param)*
    fn parse_params(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();

        // Check if there are any params (next token is RParen means empty)
        if self.current == Token::RParen {
            return Ok(params);
        }

        loop {
            let param = self.parse_param()?;
            params.push(param);

            // Check for comma separator (Chinese or English)
            if self.current == Token::Comma {
                self.advance();
                continue;
            }
            break;
        }

        Ok(params)
    }

    /// Parse a single parameter: name : type or name ： type
    fn parse_param(&mut self) -> Result<Param, String> {
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let param_type = self.parse_type()?;
        Ok(Param { name, param_type })
    }

    /// Parse a type keyword.
    fn parse_type(&mut self) -> Result<Type, String> {
        match Type::from_token(&self.current) {
            Some(t) => {
                self.advance();
                Ok(t)
            }
            None => {
                if let Token::Error(message) = &self.current {
                    Err(self.lexical_error(message))
                } else {
                    Err(self.error(
                        format!("期望类型关键字，但找到了 `{}`", token_name(&self.current)),
                        "可用类型包括 `无`、`整数`、`小数`、`浮点`、`布尔`、`字符`、`字符串`。",
                    ))
                }
            }
        }
    }

    /// Parse function body: statements until scope end.
    fn parse_body(&mut self) -> Result<Vec<Stmt>, String> {
        self.parse_statements_until_scope_end()
    }

    fn parse_statements_until_scope_end(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while self.current != Token::ScopeEnd && self.current != Token::Eof {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }
        self.expect(&Token::ScopeEnd)?;
        Ok(stmts)
    }

    fn parse_statements_until_if_boundary(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while !matches!(
            self.current,
            Token::ElseIf | Token::Else | Token::ScopeEnd | Token::Eof
        ) {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    /// Parse a single statement.
    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match &self.current {
            Token::Define => self.parse_var_decl().map(Stmt::VarDecl),
            Token::Let => self.parse_let_stmt().map(Stmt::VarDecl),
            Token::ReturnKw => self.parse_return_stmt().map(Stmt::Return),
            Token::Ident(_) => self.parse_assign_stmt().map(Stmt::Assign),
            Token::Import => self.parse_import_decl().map(Stmt::Import),
            Token::Execute => self.parse_execute_stmt().map(Stmt::Execute),
            Token::If => self.parse_if_stmt().map(Stmt::If),
            Token::Loop => self.parse_loop_stmt().map(Stmt::Loop),
            Token::Error(message) => Err(self.lexical_error(message)),
            other => Err(self.error(
                format!("方法体内不能以 `{}` 开始一条语句", token_name(other)),
                "这里可以写变量定义、赋值、返回、执行、判断、循环或局部引用语句。",
            )),
        }
    }

    /// Parse `判断 condition：... [若 condition：...] [否则：...]。。`
    fn parse_if_stmt(&mut self) -> Result<IfStmt, String> {
        self.expect(&Token::If)?;
        let condition = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        let body = self.parse_statements_until_if_boundary()?;
        let mut branches = vec![IfBranch { condition, body }];

        while self.current == Token::ElseIf {
            self.advance();
            let condition = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let body = self.parse_statements_until_if_boundary()?;
            branches.push(IfBranch { condition, body });
        }

        let else_body = if self.current == Token::Else {
            self.advance();
            self.expect(&Token::Colon)?;
            Some(self.parse_statements_until_if_boundary()?)
        } else {
            None
        };

        self.expect(&Token::ScopeEnd)?;
        Ok(IfStmt {
            branches,
            else_body,
        })
    }

    /// Parse `循环计数i<end：...。。` or `循环条件 condition：...。。`.
    fn parse_loop_stmt(&mut self) -> Result<LoopStmt, String> {
        self.expect(&Token::Loop)?;
        match &self.current {
            Token::Count => {
                self.advance();
                let var_name = self.expect_ident()?;
                self.expect(&Token::Less)?;
                let end = self.parse_expr()?;
                self.expect(&Token::Colon)?;
                let body = self.parse_statements_until_scope_end()?;
                Ok(LoopStmt::Count {
                    var_name,
                    end,
                    body,
                })
            }
            Token::Condition => {
                self.advance();
                let condition = self.parse_expr()?;
                self.expect(&Token::Colon)?;
                let body = self.parse_statements_until_scope_end()?;
                Ok(LoopStmt::Condition { condition, body })
            }
            Token::Error(message) => Err(self.lexical_error(message)),
            other => Err(self.error(
                format!(
                    "`循环` 后期望 `计数` 或 `条件`，但找到了 `{}`",
                    token_name(other)
                ),
                "计数循环形如 `循环计数i<10：...。。`；条件循环形如 `循环条件 x<10：...。。`。",
            )),
        }
    }

    /// Parse a variable declaration: `定义 [可变] 变量： [type] name = expr`
    fn parse_var_decl(&mut self) -> Result<VarDecl, String> {
        let start = self.current_span.start;
        self.expect(&Token::Define)?;
        let is_mutable = if self.current == Token::Mutable {
            self.advance();
            true
        } else {
            false
        };
        self.expect(&Token::Variable)?;
        self.expect(&Token::Colon)?;

        // Try to parse an explicit type
        let var_type = Type::from_token(&self.current);
        if var_type.is_some() {
            self.advance();
        }

        let (name, name_span) = self.expect_ident_span()?;
        self.expect(&Token::Equals)?;
        let init = self.parse_expr()?;
        let span = Span {
            start,
            end: self.current_span.start.max(name_span.end),
        };

        Ok(VarDecl {
            name,
            name_span,
            span,
            var_type,
            is_mutable,
            init,
        })
    }

    /// Parse a simplified variable declaration: `设 name = expr`.
    /// `设` variables are mutable by default.
    fn parse_let_stmt(&mut self) -> Result<VarDecl, String> {
        let start = self.current_span.start;
        self.expect(&Token::Let)?;
        let (name, name_span) = self.expect_ident_span()?;
        self.expect(&Token::Equals)?;
        let init = self.parse_expr()?;
        let span = Span {
            start,
            end: self.current_span.start.max(name_span.end),
        };

        Ok(VarDecl {
            name,
            name_span,
            span,
            var_type: None,
            is_mutable: true,
            init,
        })
    }

    /// Parse an assignment statement: `name = expr`
    fn parse_assign_stmt(&mut self) -> Result<AssignStmt, String> {
        let start = self.current_span.start;
        let (name, name_span) = self.expect_ident_span()?;
        self.expect(&Token::Equals)?;
        let value = self.parse_expr()?;
        let span = Span {
            start,
            end: self.current_span.start.max(name_span.end),
        };
        Ok(AssignStmt {
            name,
            name_span,
            span,
            value,
        })
    }

    /// Parse `返回` or `返回 expr`.
    fn parse_return_stmt(&mut self) -> Result<ReturnStmt, String> {
        self.expect(&Token::ReturnKw)?;
        let value = if matches!(
            self.current,
            Token::ScopeEnd | Token::ElseIf | Token::Else | Token::Eof
        ) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(ReturnStmt { value })
    }

    /// Parse `执行 target：arg1，arg2`. Whitespace between 执行 and target is optional.
    fn parse_execute_stmt(&mut self) -> Result<ExecuteStmt, String> {
        self.expect(&Token::Execute)?;
        let target = self.expect_ident()?;
        self.expect(&Token::Colon)?;

        let mut args = Vec::new();
        if matches!(self.current, Token::ScopeEnd | Token::Eof) {
            return Ok(ExecuteStmt { target, args });
        }

        loop {
            args.push(self.parse_expr()?);
            if self.current == Token::Comma {
                self.advance();
                continue;
            }
            break;
        }

        Ok(ExecuteStmt { target, args })
    }

    /// Parse a C++-style expression subset.
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_logical_and()?;
        while self.current == Token::OrOr {
            self.advance();
            let right = self.parse_logical_and()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while self.current == Token::AndAnd {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_comparison()?;
        loop {
            let op = match self.current {
                Token::EqEq => BinaryOp::Eq,
                Token::NotEq => BinaryOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_term()?;
        loop {
            let op = match self.current {
                Token::Less => BinaryOp::Less,
                Token::LessEq => BinaryOp::LessEq,
                Token::Greater => BinaryOp::Greater,
                Token::GreaterEq => BinaryOp::GreaterEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_term()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_factor()?;
        loop {
            let op = match self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_factor()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = match self.current {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Rem,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.current {
            Token::Minus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            Token::Bang => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match &self.current {
            Token::IntLiteral(val) => {
                let val = *val;
                self.advance();
                Ok(Expr::IntLiteral(val))
            }
            Token::StringLiteral(val) => {
                let val = val.clone();
                self.advance();
                Ok(Expr::StringLiteral(val))
            }
            Token::FormattedStringLiteral(val) => {
                let parts = parse_format_parts(val).map_err(|msg| {
                    self.error(
                        format!("格式化字符串无效：{}", msg),
                        "占位符需要写成 `{变量名}`；普通 `{` 和 `}` 请分别写成 `{{` 和 `}}`。",
                    )
                })?;
                self.advance();
                Ok(Expr::FormattedString(parts))
            }
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.current == Token::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    if self.current != Token::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.current == Token::Comma {
                                self.advance();
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Call { target: name, args })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Error(message) => Err(self.lexical_error(message)),
            other => Err(self.error(
                format!("期望表达式，但找到了 `{}`", token_name(other)),
                "表达式可以是整数、字符串、变量名、方法调用、括号表达式或一元/二元运算。",
            )),
        }
    }
}

fn token_name(token: &Token) -> String {
    match token {
        Token::At => "@".to_string(),
        Token::Declare => "声明".to_string(),
        Token::Entry => "入口".to_string(),
        Token::External => "外部".to_string(),
        Token::Define => "定义".to_string(),
        Token::Method => "方法".to_string(),
        Token::Module => "模块".to_string(),
        Token::ReturnKw => "返回".to_string(),
        Token::If => "判断".to_string(),
        Token::ElseIf => "若".to_string(),
        Token::Else => "否则".to_string(),
        Token::Loop => "循环".to_string(),
        Token::Count => "计数".to_string(),
        Token::Condition => "条件".to_string(),
        Token::VoidKw => "无".to_string(),
        Token::IntKw => "整数".to_string(),
        Token::DoubleKw => "小数".to_string(),
        Token::FloatKw => "浮点".to_string(),
        Token::BoolKw => "布尔".to_string(),
        Token::CharKw => "字符".to_string(),
        Token::StringKw => "字符串".to_string(),
        Token::IntTypeKw => "整型".to_string(),
        Token::Variable => "变量".to_string(),
        Token::Mutable => "可变".to_string(),
        Token::Let => "设".to_string(),
        Token::Hash => "#".to_string(),
        Token::Import => "引用".to_string(),
        Token::AsKw => "为".to_string(),
        Token::Execute => "执行".to_string(),
        Token::LParen => "(".to_string(),
        Token::RParen => ")".to_string(),
        Token::Colon => ":".to_string(),
        Token::ScopeEnd => "。。".to_string(),
        Token::Comma => ",".to_string(),
        Token::Equals => "=".to_string(),
        Token::Plus => "+".to_string(),
        Token::Minus => "-".to_string(),
        Token::Star => "*".to_string(),
        Token::Slash => "/".to_string(),
        Token::Percent => "%".to_string(),
        Token::EqEq => "==".to_string(),
        Token::NotEq => "!=".to_string(),
        Token::Less => "<".to_string(),
        Token::LessEq => "<=".to_string(),
        Token::Greater => ">".to_string(),
        Token::GreaterEq => ">=".to_string(),
        Token::AndAnd => "&&".to_string(),
        Token::OrOr => "||".to_string(),
        Token::Bang => "!".to_string(),
        Token::Ident(name) => format!("标识符 `{}`", name),
        Token::IntLiteral(value) => format!("整数 `{}`", value),
        Token::StringLiteral(_) => "字符串字面量".to_string(),
        Token::FormattedStringLiteral(_) => "格式化字符串".to_string(),
        Token::Error(message) => format!("错误 token（{}）", message),
        Token::Eof => "文件结束".to_string(),
    }
}

fn render_diagnostic(
    kind: &str,
    title: String,
    source_name: Option<&str>,
    source: &str,
    span: Span,
    help: Option<String>,
) -> String {
    let (line_no, col_no, line_start) = line_col(source, span.start);
    let line_text = source.lines().nth(line_no.saturating_sub(1)).unwrap_or("");
    let col_width = col_no.saturating_sub(1);
    let caret_width = span
        .end
        .saturating_sub(span.start)
        .min(line_text.chars().count().saturating_sub(col_width))
        .max(1);
    let line_number_width = line_no.to_string().len();
    let mut out = String::new();

    out.push_str(&format!("{}: {}\n", kind, title));
    if let Some(source_name) = source_name {
        out.push_str(&format!(" --> {}:{}:{}\n", source_name, line_no, col_no));
    } else {
        out.push_str(&format!(" --> 第 {} 行，第 {} 列\n", line_no, col_no));
    }
    out.push_str(&format!("{:>width$} |\n", "", width = line_number_width));
    out.push_str(&format!(
        "{:>width$} | {}\n",
        line_no,
        line_text,
        width = line_number_width
    ));
    out.push_str(&format!(
        "{:>width$} | {}{}\n",
        "",
        " ".repeat(col_width),
        "^".repeat(caret_width),
        width = line_number_width
    ));
    if let Some(help) = help {
        out.push_str(&format!("  = 帮助: {}\n", help));
    }
    out.push_str(&format!("  = 位置: 字符偏移 {}\n", line_start + col_width));
    out
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

fn parse_format_parts(source: &str) -> Result<Vec<FormatPart>, String> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    text.push('{');
                    continue;
                }

                if !text.is_empty() {
                    parts.push(FormatPart::Text(std::mem::take(&mut text)));
                }

                let mut name = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        closed = true;
                        break;
                    }
                    name.push(inner);
                }

                if !closed {
                    return Err("Unclosed placeholder in formatted string".to_string());
                }

                let name = name.trim();
                if name.is_empty() {
                    return Err("Empty placeholder in formatted string".to_string());
                }
                parts.push(FormatPart::Placeholder(name.to_string()));
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    text.push('}');
                } else {
                    return Err("Single '}' in formatted string".to_string());
                }
            }
            _ => text.push(ch),
        }
    }

    if !text.is_empty() {
        parts.push(FormatPart::Text(text));
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

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
                assert_eq!(v.init, Expr::IntLiteral(10));
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
                assert_eq!(v.init, Expr::IntLiteral(20));
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
                assert_eq!(v.init, Expr::IntLiteral(30));
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
                assert_eq!(v.init, Expr::IntLiteral(10));
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
                assert_eq!(v.init, Expr::IntLiteral(0));
            }
            other => panic!("Expected VarDecl, found {:?}", other),
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
                    Expr::Call {
                        target: "从零求和".to_string(),
                        args: vec![Expr::IntLiteral(10)],
                    }
                );
            }
            other => panic!("Expected VarDecl, found {:?}", other),
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
        assert!(func.body.is_empty());
        assert_eq!(func.params[0].param_type, Type::String);
    }
}
