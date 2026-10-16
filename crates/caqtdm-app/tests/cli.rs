use caqtdm_app::execute;

fn args(parts: &[&str]) -> Vec<String> {
    std::iter::once("caqtdm".to_string())
        .chain(parts.iter().map(|s| (*s).to_string()))
        .collect()
}

#[test]
fn widgets_and_sample_display() {
    let text = execute(&args(&["widgets"])).unwrap();
    assert!(text.contains("caLineEdit"));
    assert!(text.contains("caCartesianPlot"));

    let ui = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/displays/vacuum.ui"
    );
    let dumped = execute(&args(&["dump", ui, "-macro", "SYS=VAC"])).unwrap();
    assert!(dumped.contains("caLabel"));
    assert!(dumped.contains("intern://VAC:SETPOINT"));

    let converted = execute(&args(&["convert", ui, "-macro", "SYS=VAC"])).unwrap();
    assert!(converted.contains("caStripPlot"));

    let dir = std::env::temp_dir().join(format!("caqtdm-paint-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let ppm = dir.join("vacuum.ppm");
    let painted = execute(&args(&[
        "paint",
        ui,
        "-macro",
        "SYS=VAC",
        "-o",
        ppm.to_str().unwrap(),
    ]))
    .unwrap();
    assert!(painted.contains("wrote"));
    let bytes = std::fs::read(&ppm).unwrap();
    assert!(bytes.starts_with(b"P6\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn help_and_bad_file() {
    let help = execute(&args(&["help"])).unwrap();
    assert!(help.contains("-macro"));
    let err = execute(&args(&["validate", "missing-display.ui"])).unwrap_err();
    assert!(!err.is_empty());
}
