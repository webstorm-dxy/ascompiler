use std::ffi::CString;
use wenyuan_ffi::AsStr;

#[wenyuan_ffi::export(name = "test_wen_add")]
fn add(left: i32, right: i32) -> i32 {
    left + right
}

#[wenyuan_ffi::export(name = "test_wen_string_len")]
fn string_len(text: AsStr) -> i32 {
    text.to_str().map(|value| value.len() as i32).unwrap_or(0)
}

#[wenyuan_ffi::export(name = "test_wen_void")]
fn accept_void() {}

#[test]
fn test_exported_primitive_function_is_callable() {
    assert_eq!(add(20, 22), 42);
}

#[test]
fn test_exported_string_helper_reads_c_string() {
    let text = CString::new("wenyuan").expect("CString failed");
    let text = unsafe { AsStr::from_ptr(text.as_ptr()) };
    assert_eq!(string_len(text), 7);
}

#[test]
fn test_exported_void_function_is_callable() {
    accept_void();
}
