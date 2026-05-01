use wenyuan_ffi::AsStr;

#[wenyuan_ffi::export(name = "wen_add")]
fn add(left: i32, right: i32) -> i32 {
    left + right
}

#[wenyuan_ffi::export(name = "wen_text_len")]
fn text_len(text: AsStr) -> i32 {
    text.to_str()
        .map(|value| value.chars().count() as i32)
        .unwrap_or(0)
}

#[wenyuan_ffi::export(name = "wen_print_text")]
fn print_text(text: AsStr) {
    println!("{}", text.to_string_lossy());
}
