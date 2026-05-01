use proc_macro::TokenStream;

/// Export a Rust function through the C ABI for 问源.
///
/// Use `#[wenyuan_ffi::export(name = "native_symbol")]`.
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_text = attr.to_string();
    let symbol = match parse_symbol_name(&attr_text) {
        Ok(symbol) => symbol,
        Err(err) => return compile_error(&err),
    };

    let mut item_text = item.to_string();
    if item_text.trim_start().starts_with("fn ") {
        let leading = item_text.len() - item_text.trim_start().len();
        item_text.insert_str(leading, "pub ");
    }

    let Some(fn_pos) = item_text.find("fn ") else {
        return compile_error("#[wenyuan_ffi::export] 只能用于函数");
    };
    item_text.insert_str(fn_pos, "extern \"C\" ");

    let output = format!("#[unsafe(export_name = \"{}\")]\n{}", symbol, item_text);
    output.parse().unwrap_or_else(|_| {
        compile_error("#[wenyuan_ffi::export] 无法生成有效的 Rust 代码；请检查函数签名")
    })
}

fn parse_symbol_name(attr: &str) -> Result<String, String> {
    let attr = attr.trim();
    let Some(rest) = attr.strip_prefix("name") else {
        return Err("请写成 #[wenyuan_ffi::export(name = \"symbol\")]".to_string());
    };
    let Some(rest) = rest.trim_start().strip_prefix('=') else {
        return Err("`name` 后需要 `= \"symbol\"`".to_string());
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix('"') else {
        return Err("导出符号名必须是字符串字面量".to_string());
    };
    let Some(end) = rest.find('"') else {
        return Err("导出符号名缺少结束引号".to_string());
    };
    let symbol = &rest[..end];
    if symbol.is_empty() {
        return Err("导出符号名不能为空".to_string());
    }
    Ok(symbol.to_string())
}

fn compile_error(message: &str) -> TokenStream {
    format!("compile_error!({:?});", message)
        .parse()
        .expect("compile_error token stream must parse")
}
