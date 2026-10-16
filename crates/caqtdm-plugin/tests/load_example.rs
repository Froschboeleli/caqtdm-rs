use std::path::PathBuf;

use caqtdm_plugin::LoadedPlugin;

fn example_library() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("debug");
    let file = if cfg!(target_os = "macos") {
        "libcaqtdm_plugin_example.dylib"
    } else if cfg!(target_os = "windows") {
        "caqtdm_plugin_example.dll"
    } else {
        "libcaqtdm_plugin_example.so"
    };
    path.push(file);
    path
}

#[test]
fn example_plugin_adds_one() {
    let path = example_library();
    assert!(path.is_file(), "missing {}", path.display());
    let plugin = unsafe { LoadedPlugin::load(&path).unwrap() };
    assert_eq!(plugin.name(), "add-one");
    let value = plugin.eval("3").unwrap();
    assert!((value - 4.0).abs() < 1e-9);
    assert!(plugin.eval("nope").is_err());
}
