/// Parser for the 问源 programming language.
///
/// Converts a token stream into an AST. Handles `@声明 入口` annotations
/// that mark the following function as the program entry point.
use crate::lexer::{Lexer, Token};

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
    /// Function body statements.
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl(VarDecl),
    Import(ImportDecl),
    Execute(ExecuteStmt),
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
    /// Optional explicit type (None = type inferred from initializer).
    pub var_type: Option<Type>,
    /// Initializer expression.
    pub init: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
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
    /// When true, the next function definition is the entry point.
    next_is_entry: bool,
    /// Whether this program has an entry point defined.
    has_entry: bool,
    /// Current module path for following top-level function definitions.
    current_module: Option<String>,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        let mut lexer = lexer;
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            next_is_entry: false,
            has_entry: false,
            current_module: None,
        }
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
    }

    /// Expect `expected`, advance if matched, error otherwise.
    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if &self.current == expected {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected {:?} but found {:?}",
                expected, self.current
            ))
        }
    }

    /// Expect an identifier, return its value.
    fn expect_ident(&mut self) -> Result<String, String> {
        match &self.current {
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            other => Err(format!("Expected identifier but found {:?}", other)),
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
                other => {
                    return Err(format!("Unexpected token {:?} at top level", other));
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
            other => Err(format!("Expected 入口 after @声明, found {:?}", other)),
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
            other => {
                return Err(format!("Expected 方法 after 定义, found {:?}", other));
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

        // Scope start: : or ：
        self.expect(&Token::Colon)?;

        // Parse function body
        let body = self.parse_body()?;

        // Determine if this is the entry point
        let is_entry = self.next_is_entry;
        if is_entry {
            self.has_entry = true;
            self.next_is_entry = false;
        }

        Ok(FunctionDef {
            name,
            module_path: self.current_module.clone(),
            params,
            return_type,
            is_entry,
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
            None => Err(format!("Expected type keyword, found {:?}", self.current)),
        }
    }

    /// Parse function body: statements until scope end.
    fn parse_body(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while self.current != Token::ScopeEnd && self.current != Token::Eof {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }
        self.expect(&Token::ScopeEnd)?;
        Ok(stmts)
    }

    /// Parse a single statement.
    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match &self.current {
            Token::Define => self.parse_var_decl().map(Stmt::VarDecl),
            Token::Let => self.parse_let_stmt().map(Stmt::VarDecl),
            Token::Import => self.parse_import_decl().map(Stmt::Import),
            Token::Execute => self.parse_execute_stmt().map(Stmt::Execute),
            other => Err(format!("Unexpected token {:?} in function body", other)),
        }
    }

    /// Parse a variable declaration: `定义 变量： [type] name = expr`
    fn parse_var_decl(&mut self) -> Result<VarDecl, String> {
        self.expect(&Token::Define)?;
        self.expect(&Token::Variable)?;
        self.expect(&Token::Colon)?;

        // Try to parse an explicit type
        let var_type = Type::from_token(&self.current);
        if var_type.is_some() {
            self.advance();
        }

        let name = self.expect_ident()?;
        self.expect(&Token::Equals)?;
        let init = self.parse_expr()?;

        Ok(VarDecl {
            name,
            var_type,
            init,
        })
    }

    /// Parse a simplified variable declaration: `设 name = expr`
    fn parse_let_stmt(&mut self) -> Result<VarDecl, String> {
        self.expect(&Token::Let)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Equals)?;
        let init = self.parse_expr()?;

        Ok(VarDecl {
            name,
            var_type: None,
            init,
        })
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

    /// Parse an expression (currently only integer literals).
    fn parse_expr(&mut self) -> Result<Expr, String> {
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
            other => Err(format!("Expected expression, found {:?}", other)),
        }
    }
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
                assert_eq!(v.init, Expr::IntLiteral(10));
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
}
