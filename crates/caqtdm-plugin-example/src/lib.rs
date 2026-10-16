//! Example plugin: `caqtdm_plugin_eval("3")` returns 4.

use std::ffi::CStr;
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn caqtdm_plugin_abi() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn caqtdm_plugin_name() -> *const c_char {
    static NAME: &CStr = c"add-one";
    NAME.as_ptr()
}

/// # Safety
/// `expr` must be a valid C string and `out` must point at an `f64`.
#[no_mangle]
pub unsafe extern "C" fn caqtdm_plugin_eval(expr: *const c_char, out: *mut f64) -> i32 {
    if expr.is_null() || out.is_null() {
        return 1;
    }
    let text = CStr::from_ptr(expr).to_string_lossy();
    match text.trim().parse::<f64>() {
        Ok(value) => {
            *out = value + 1.0;
            0
        }
        Err(_) => 2,
    }
}
