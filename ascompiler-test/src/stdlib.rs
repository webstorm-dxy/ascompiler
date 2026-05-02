use ascompiler::{semantic, stdlib};

use stdlib::{STD_IO_INPUT_PATH, STD_IO_OUTPUT_PATH, parse_standard_library};

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
