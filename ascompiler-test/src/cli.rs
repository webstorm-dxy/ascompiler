use ascompiler::cli::parse_args;
use std::path::PathBuf;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn test_parse_repeated_ffi_link_args() {
    let options = parse_args(&args(&[
        "asc",
        "demo/ffi.as",
        "-o",
        "demo/ffi",
        "--ffi-lib",
        "target/debug/libdemo.a",
        "--ffi-lib",
        "target/debug/libdemo.dylib",
        "--ffi-search",
        "target/debug",
        "--ffi-rpath",
        "target/debug",
    ]))
    .expect("parse args failed");

    assert_eq!(options.input_path, PathBuf::from("demo/ffi.as"));
    assert_eq!(options.output_path, PathBuf::from("demo/ffi"));
    assert!(!options.ir_mode);
    assert_eq!(
        options.ffi.libs,
        vec![
            PathBuf::from("target/debug/libdemo.a"),
            PathBuf::from("target/debug/libdemo.dylib")
        ]
    );
    assert_eq!(
        options.ffi.search_paths,
        vec![PathBuf::from("target/debug")]
    );
    assert_eq!(options.ffi.rpaths, vec![PathBuf::from("target/debug")]);
}

#[test]
fn test_parse_ffi_arg_requires_value() {
    let err = parse_args(&args(&["asc", "demo/ffi.as", "--ffi-lib"]))
        .expect_err("expected missing value error");
    assert!(err.contains("--ffi-lib"));
}
