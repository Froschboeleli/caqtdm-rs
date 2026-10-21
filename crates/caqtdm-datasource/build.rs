use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_libzmq)");
    println!("cargo:rerun-if-changed=build.rs");
    // The probe looks at host library paths; a cross build (for example
    // x86_64-pc-windows-gnu) must not link the host's libzmq.
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if host != target {
        return;
    }
    let candidates = [
        "/usr/lib/x86_64-linux-gnu/libzmq.so.5",
        "/usr/lib64/libzmq.so.5",
        "/usr/lib/libzmq.so.5",
        "/usr/local/lib/libzmq.so.5",
    ];
    let Some(library) = candidates.iter().find(|path| Path::new(path).exists()) else {
        return;
    };
    let out = std::env::var("OUT_DIR").unwrap_or_default();
    let link = Path::new(&out).join("libzmq.so");
    let _ = std::fs::remove_file(&link);
    if std::os::unix::fs::symlink(library, &link).is_err() {
        return;
    }
    println!("cargo:rustc-cfg=has_libzmq");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=dylib=zmq");
}
