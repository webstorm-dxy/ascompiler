use crate::lexer::Lexer;
use crate::parser::{Parser, Program};

pub const STD_IO_OUTPUT_PATH: &str = "标准库-输入输出-输出";
pub const STD_IO_INPUT_PATH: &str = "标准库-输入输出-获取输入";

const STANDARD_LIBRARY_SOURCES: &[&str] = &[include_str!("../../std/输入输出.as")];

pub fn parse_standard_library() -> Result<Program, String> {
    let mut std_program = empty_program();

    for source in STANDARD_LIBRARY_SOURCES {
        let parsed = Parser::new(Lexer::new(source)).parse_program()?;
        merge_programs(&mut std_program, parsed);
    }

    Ok(std_program)
}

pub fn merge_with_standard_library(mut program: Program) -> Result<Program, String> {
    let std_program = parse_standard_library()?;
    prepend_program(&mut program, std_program);
    Ok(program)
}

pub fn external_symbol_for(path: &str) -> Option<String> {
    match path {
        STD_IO_OUTPUT_PATH => Some("as_std_io_output".to_string()),
        STD_IO_INPUT_PATH => Some("as_std_io_input_int".to_string()),
        _ => None,
    }
}

fn empty_program() -> Program {
    Program {
        has_entry: false,
        modules: Vec::new(),
        imports: Vec::new(),
        functions: Vec::new(),
    }
}

fn merge_programs(target: &mut Program, source: Program) {
    target.has_entry |= source.has_entry;
    target.modules.extend(source.modules);
    target.imports.extend(source.imports);
    target.functions.extend(source.functions);
}

fn prepend_program(program: &mut Program, mut prefix: Program) {
    prefix.has_entry |= program.has_entry;
    prefix.modules.append(&mut program.modules);
    prefix.imports.append(&mut program.imports);
    prefix.functions.append(&mut program.functions);
    *program = prefix;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic;

    #[test]
    fn test_standard_library_exports_output() {
        let program = parse_standard_library().expect("std parse failed");
        assert!(program.modules.iter().any(|m| m.name == "标准库-输入输出"));
        assert!(
            program
                .functions
                .iter()
                .any(|f| semantic::function_path(f) == STD_IO_OUTPUT_PATH && f.is_external)
        );
        assert!(
            program
                .functions
                .iter()
                .any(|f| semantic::function_path(f) == STD_IO_INPUT_PATH && f.is_external)
        );
    }
}
