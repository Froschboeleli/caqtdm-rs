//! C ABI for out-of-process display logic.
//!
//! A plugin is a cdylib that exports `caqtdm_plugin_abi`, `caqtdm_plugin_name`,
//! and `caqtdm_plugin_eval`. The host can pass a lookup callback so the plugin
//! reads channel values without linking the rest of the workspace. Shell and
//! Python from `caScriptButton` are not this interface: those stay dry-run.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::Path;

pub const ABI_V1: u32 = 1;

#[repr(C)]
pub struct HostV1 {
    pub abi: u32,
    pub user: *mut c_void,
    pub lookup: Option<extern "C" fn(*mut c_void, *const c_char, *mut f64) -> i32>,
}

pub struct LoadedPlugin {
    _lib: libloading::Library,
    name: String,
    eval: unsafe extern "C" fn(*const c_char, *mut f64) -> i32,
}

impl LoadedPlugin {
    /// Load a cdylib. The file must stay valid for the life of this value.
    ///
    /// # Safety
    /// The library's exported functions must match the signatures above.
    /// A mismatched plugin can crash the process.
    pub unsafe fn load(path: &Path) -> Result<Self, String> {
        let lib = libloading::Library::new(path).map_err(|e| e.to_string())?;
        let abi: libloading::Symbol<extern "C" fn() -> u32> =
            lib.get(b"caqtdm_plugin_abi").map_err(|e| e.to_string())?;
        if abi() != ABI_V1 {
            return Err(format!("plugin {} has a different ABI", path.display()));
        }
        let name_fn: libloading::Symbol<extern "C" fn() -> *const c_char> =
            lib.get(b"caqtdm_plugin_name").map_err(|e| e.to_string())?;
        let name = CStr::from_ptr(name_fn()).to_string_lossy().into_owned();
        let eval_fn = {
            let eval: libloading::Symbol<unsafe extern "C" fn(*const c_char, *mut f64) -> i32> =
                lib.get(b"caqtdm_plugin_eval").map_err(|e| e.to_string())?;
            *eval
        };
        Ok(Self {
            _lib: lib,
            name,
            eval: eval_fn,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn eval(&self, expr: &str) -> Result<f64, String> {
        let c = CString::new(expr).map_err(|e| e.to_string())?;
        let mut out = 0.0;
        let rc = unsafe { (self.eval)(c.as_ptr(), &mut out) };
        if rc == 0 {
            Ok(out)
        } else {
            Err(format!("plugin {} rejected {expr}", self.name))
        }
    }
}
