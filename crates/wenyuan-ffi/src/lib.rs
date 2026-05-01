//! Helpers for writing Rust functions callable from 问源 FFI declarations.
//!
//! 问源 v1 FFI uses the C ABI. Primitive mappings are direct: `整数` is
//! `i32`, `小数` is `f64`, `浮点` is `f32`, `布尔` is `bool`, `字符` is `u8`,
//! and `无` is `()`.

use std::ffi::CStr;
use std::os::raw::c_char;

pub use wenyuan_ffi_macros::export;

/// Borrowed UTF-8 string pointer passed by 问源.
///
/// This is a transparent wrapper around `*const c_char`, so it can be used in
/// exported `extern "C"` function signatures.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct AsStr {
    ptr: *const c_char,
}

impl AsStr {
    /// Build an `AsStr` from a raw C string pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be null or point to a valid NUL-terminated C string for the
    /// duration of any borrow returned by this value.
    pub const unsafe fn from_ptr(ptr: *const c_char) -> Self {
        Self { ptr }
    }

    pub const fn as_ptr(self) -> *const c_char {
        self.ptr
    }

    pub fn as_c_str(&self) -> Option<&CStr> {
        if self.ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(self.ptr) })
        }
    }

    pub fn to_str(&self) -> Option<&str> {
        self.as_c_str()?.to_str().ok()
    }

    pub fn to_string_lossy(&self) -> String {
        self.as_c_str()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
