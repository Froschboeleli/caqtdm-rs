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
    let defs = std::env::temp_dir().join(format!("caqtdm-macros-{}", std::process::id()));
    std::fs::write(&defs, "# sector\nSYS=VAC\n").unwrap();
    let dumped = execute(&args(&["dump", ui, "-macrodefs", defs.to_str().unwrap()])).unwrap();
    let _ = std::fs::remove_file(&defs);
    assert!(dumped.contains("caLabel"));
    assert!(dumped.contains("intern://VAC:SETPOINT"));

    let converted = execute(&args(&["convert", ui, "-macro", "SYS=VAC"])).unwrap();
    assert!(converted.contains("caStripPlot"));

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/displays");
    let edl_path = format!("{root}/rf.edl");
    let edl = execute(&args(&["convert", &edl_path])).unwrap();
    assert!(edl.contains("caRelatedDisplay"));
    assert!(edl.contains("gauges.ui"));
    let alh_path = format!("{root}/vacuum.alh");
    let alh = execute(&args(&["dump", &alh_path])).unwrap();
    assert!(alh.contains("caAlarmTree"));
    assert!(alh.contains("sim://sine:1:5000:2"));
    let prc_path = format!("{root}/magnets.prc");
    let prc = execute(&args(&["convert", &prc_path])).unwrap();
    assert!(prc.contains("caTextEntry"));
    assert!(prc.contains("intern://VAC:SETPOINT"));

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
fn config_print_attach_and_http() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let ui = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/displays/vacuum.ui"
    );
    let dir = std::env::temp_dir().join(format!("caqtdm-config-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let cfg = dir.join("caqtdm.conf");
    std::fs::write(
        &cfg,
        "control_system=epics4\nmacros=SYS=RF\nexecute_shell=true\n",
    )
    .unwrap();
    let text = execute(&args(&["config", "-config", cfg.to_str().unwrap()])).unwrap();
    assert!(text.contains("control_system=epics4"));
    assert!(text.contains("macros=SYS=RF"));
    assert!(text.contains("execute_shell=true"));

    let ppm = dir.join("vacuum.ppm");
    let printed = execute(&args(&[
        "print",
        ui,
        "-macro",
        "SYS=VAC",
        "-o",
        ppm.to_str().unwrap(),
    ]))
    .unwrap();
    assert!(printed.contains("wrote"));
    assert!(std::fs::read(&ppm).unwrap().starts_with(b"P6\n"));

    let stop = Arc::new(AtomicBool::new(false));
    let addr = caqtdm_app::serve_attach(Arc::clone(&stop)).unwrap();
    let reply = execute(&args(&["run", ui, "-attach", &addr.to_string()])).unwrap();
    assert!(reply.contains("ok"));
    assert!(reply.contains("vacuum.ui"));
    stop.store(true, Ordering::Relaxed);

    let stop = Arc::new(AtomicBool::new(false));
    let addr = caqtdm_app::serve_http(Arc::clone(&stop)).unwrap();
    let page = caqtdm_app::http_get(addr).unwrap();
    assert!(page.contains("200"));
    assert!(page.contains("caQtDM"));
    let pong = caqtdm_app::websocket_roundtrip(addr, "ping").unwrap();
    assert_eq!(pong, "pong");
    stop.store(true, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn help_and_bad_file() {
    let help = execute(&args(&["help"])).unwrap();
    assert!(help.contains("-macro"));
    assert!(help.contains("-server"));
    assert!(help.contains("-web_instance_limit"));
    assert!(help.contains("-stylefile"));
    assert!(help.contains("-emptycache"));
    assert!(help.contains("-novnc"));
    let err = execute(&args(&["validate", "missing-display.ui"])).unwrap_err();
    assert!(!err.is_empty());
    let unknown = execute(&args(&["-notAFlag"])).unwrap_err();
    assert!(unknown.contains("unexpected argument -notAFlag"));
}

#[test]
fn server_flags_are_reported_and_clamped() {
    let limited = execute(&args(&[
        "config",
        "-web_instance_limit",
        "9000",
        "-web_timeout",
        "80",
        "-server_port",
        "14000",
        "-displayFont",
        "Helvetica",
        "-x",
        "-style",
        "fusion",
        "-noMsg",
        "-noResize",
        "-dg",
        "30x16+10+20",
        "-option",
        "bsmodulo=10,bsoffset=3",
        "-web_launcher_root_file",
        "root.ui",
        "-web_interaction_timeout",
        "-host",
        "127.0.0.1",
    ]))
    .unwrap();
    assert!(limited.contains("web_limit=5000"));
    assert!(limited.contains("web_timeout=80"));
    assert!(limited.contains("web_port=14001"));
    assert!(limited.contains("font=Helvetica"));
    assert!(limited.contains("execute_only=true"));
    assert!(limited.contains("style=fusion"));
    assert!(limited.contains("noMsg=true"));
    assert!(limited.contains("noResize=true"));
    assert!(limited.contains("geometry=30x16"));
    assert!(limited.contains("option.bsmodulo=10"));
    assert!(limited.contains("option.bsoffset=3"));
    assert!(limited.contains("root=root.ui"));
    assert!(limited.contains("interaction=true"));
    assert!(limited.contains("host=127.0.0.1"));

    let disabled = execute(&args(&["config", "-web_timeout", "30"])).unwrap();
    assert!(disabled.contains("web_timeout=0"));

    let zero = execute(&args(&["config", "-web_instance_limit", "0"])).unwrap_err();
    assert!(zero.contains("at least 1"));

    let http = execute(&args(&["-httpconfig"])).unwrap();
    assert!(http.contains("httpconfig=true"));
    assert!(http.contains("control_system="));
}

#[test]
fn stylefile_geometry_and_url_change_the_result() {
    let dir = std::env::temp_dir().join(format!("caqtdm-flags-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ui = dir.join("label.ui");
    std::fs::write(
        &ui,
        r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>20</height></rect></property>
 <widget class="caLabel" name="letter">
  <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>20</height></rect></property>
  <property name="text"><string>A</string></property>
 </widget>
</widget></ui>"#,
    )
    .unwrap();
    let css = dir.join("red.qss");
    std::fs::write(&css, "color: #ff0000;\n").unwrap();
    let ppm = dir.join("label.ppm");
    execute(&args(&[
        "paint",
        ui.to_str().unwrap(),
        "-stylefile",
        css.to_str().unwrap(),
        "-dg",
        "40x20",
        "-o",
        ppm.to_str().unwrap(),
    ]))
    .unwrap();
    let bytes = std::fs::read(&ppm).unwrap();
    assert!(bytes.starts_with(b"P6\n40 20\n"));
    assert!(
        ppm_has(&bytes, |r, g, b| r == 255 && g == 0 && b == 0),
        "stylesheet color was not painted"
    );
    let (r, g, b) = ppm_rgb(&bytes, 38, 18);
    assert_eq!((r, g, b), (236, 236, 236));

    let small = dir.join("small.ppm");
    let painted = execute(&args(&[
        ui.to_str().unwrap(),
        "-savetoimage",
        small.to_str().unwrap(),
        "-displayGeometry",
        "30x16",
    ]))
    .unwrap();
    assert!(painted.contains("30x16"));
    assert!(std::fs::read(&small).unwrap().starts_with(b"P6\n30 16\n"));

    let extra = dir.join("extra");
    std::fs::create_dir_all(&extra).unwrap();
    std::fs::write(
        extra.join("child.ui"),
        r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <widget class="caLabel" name="fromurl">
  <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>16</height></rect></property>
  <property name="text"><string>FROMURL</string></property>
 </widget>
</widget></ui>"#,
    )
    .unwrap();
    let parent = dir.join("parent.ui");
    std::fs::write(
        &parent,
        r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <widget class="caInclude" name="inc">
  <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>16</height></rect></property>
  <property name="filename"><string>child.ui</string></property>
 </widget>
</widget></ui>"#,
    )
    .unwrap();
    let missing = execute(&args(&["dump", parent.to_str().unwrap()])).unwrap_err();
    assert!(missing.contains("included display not found"));
    let dumped = execute(&args(&[
        "dump",
        parent.to_str().unwrap(),
        "-url",
        extra.to_str().unwrap(),
    ]))
    .unwrap();
    assert!(dumped.contains("caLabel"));
    assert!(dumped.contains("fromurl"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_limit_and_sampling_options() {
    use std::net::SocketAddr;

    let text = execute(&args(&[
        "-server",
        "-server_port",
        "0",
        "-web_server_port",
        "0",
        "-web_instance_limit",
        "1",
        "-noMsg",
        "-noResize",
        "-style",
        "fusion",
        "-web_allow_insecure_cashell_commands",
        "-web_timeout",
        "90",
    ]))
    .unwrap();
    let mut lines = text.lines();
    let http: SocketAddr = lines
        .next()
        .unwrap()
        .trim_start_matches("http ")
        .parse()
        .unwrap();
    let attach: SocketAddr = lines
        .next()
        .unwrap()
        .trim_start_matches("attach ")
        .parse()
        .unwrap();
    assert_ne!(http, attach);
    let page = http_retry(http);
    assert!(page.contains("200"));
    assert!(page.contains("caQtDM"));
    assert!(page.contains("style=fusion"));
    assert!(page.contains("noMsg=true"));
    assert!(page.contains("noResize=true"));
    assert!(page.contains("shell=true"));
    assert!(page.contains("web_limit=1"));
    assert!(page.contains("web_timeout=90"));
    let busy = http_retry(http);
    assert!(busy.contains("503"));
    assert!(busy.contains("busy"));
    let _ = attach;

    assert_eq!(
        caqtdm_app::sampled_pulse(&args(&["config"]), 1000).unwrap(),
        100
    );
    assert_eq!(
        caqtdm_app::sampled_pulse(
            &args(&["config", "-option", "bsmodulo=10,bsoffset=3"]),
            1000
        )
        .unwrap(),
        103
    );
}

#[test]
fn empty_cache_deletes_the_cached_file() {
    let dir = std::env::temp_dir().join("caqtdm-display-cache");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("stale-{}", std::process::id()));
    std::fs::write(&file, b"old").unwrap();
    let text = execute(&args(&["config", "-emptycache"])).unwrap();
    assert!(!file.exists());
    let line = text
        .lines()
        .find(|line| line.starts_with("emptycache="))
        .unwrap();
    let count: usize = line.trim_start_matches("emptycache=").parse().unwrap();
    assert!(count >= 1);
}

#[test]
fn novnc_serves_the_framebuffer_and_readonly_drops_input() {
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let err = execute(&args(&["-novnc_readonly"])).unwrap_err();
    assert!(err.contains("novnc"));

    let dir = std::env::temp_dir().join(format!("caqtdm-vnc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ui = dir.join("label.ui");
    std::fs::write(
        &ui,
        r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>20</height></rect></property>
 <widget class="caLabel" name="letter">
  <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>20</height></rect></property>
  <property name="text"><string>A</string></property>
 </widget>
</widget></ui>"#,
    )
    .unwrap();
    let css = dir.join("red.qss");
    std::fs::write(&css, "color: #ff0000;\n").unwrap();
    let missing = execute(&args(&[
        "paint",
        ui.to_str().unwrap(),
        "-displayFont",
        "no-such-face",
        "-o",
        dir.join("bad.ppm").to_str().unwrap(),
    ]))
    .unwrap_err();
    assert!(missing.contains("display font"));

    let text = execute(&args(&[
        "-server",
        "-novnc",
        "-novnc_readonly",
        "-server_port",
        "0",
        "-web_server_port",
        "0",
        "-displayFont",
        "Cousine",
        "-stylefile",
        css.to_str().unwrap(),
        ui.to_str().unwrap(),
    ]))
    .unwrap();
    assert!(text.contains("readonly true"));
    let vnc: SocketAddr = line_addr(&text, "vnc ");
    let novnc: SocketAddr = line_addr(&text, "novnc ");
    let (_w, _h, rgba) = caqtdm_app::rfb_framebuffer(vnc).unwrap();
    assert!(
        rgba.chunks(4)
            .any(|px| px[0] == 255 && px[1] == 0 && px[2] == 0),
        "rfb framebuffer missed the red glyph"
    );
    assert_eq!(caqtdm_app::novnc_version(novnc).unwrap(), "RFB 003.008\n");

    let stop = Arc::new(AtomicBool::new(false));
    let frame = caqtdm_app::RfbFrame {
        width: 2,
        height: 2,
        rgba: vec![255, 0, 0, 255, 0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255],
        readonly: false,
    };
    let (addr, events) = caqtdm_app::serve_rfb(
        "127.0.0.1:0".parse().unwrap(),
        Arc::clone(&stop),
        frame.clone(),
    )
    .unwrap();
    let (w, h, pixels) = caqtdm_app::rfb_framebuffer(addr).unwrap();
    assert_eq!((w, h), (2, 2));
    assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
    caqtdm_app::rfb_pointer(addr, 1, 1).unwrap();
    let seen = wait_events(&events);
    assert_eq!(seen.len(), 1);
    assert!(seen[0].pointer);
    assert_eq!((seen[0].x, seen[0].y, seen[0].buttons), (1, 1, 1));
    stop.store(true, Ordering::Relaxed);

    let stop = Arc::new(AtomicBool::new(false));
    let mut quiet = frame;
    quiet.readonly = true;
    let (addr, events) =
        caqtdm_app::serve_rfb("127.0.0.1:0".parse().unwrap(), stop, quiet).unwrap();
    caqtdm_app::rfb_pointer(addr, 1, 1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(events.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn line_addr(text: &str, prefix: &str) -> std::net::SocketAddr {
    text.lines()
        .find(|line| line.starts_with(prefix))
        .unwrap()
        .trim_start_matches(prefix)
        .parse()
        .unwrap()
}

fn wait_events(events: &std::sync::Mutex<Vec<caqtdm_app::RfbEvent>>) -> Vec<caqtdm_app::RfbEvent> {
    for _ in 0..40 {
        let guard = events.lock().unwrap();
        if !guard.is_empty() {
            return guard.clone();
        }
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Vec::new()
}

fn http_retry(addr: std::net::SocketAddr) -> String {
    let mut last = String::new();
    for _ in 0..40 {
        match caqtdm_app::http_get(addr) {
            Ok(page) => return page,
            Err(err) => {
                last = err.to_string();
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
    panic!("http {addr}: {last}");
}

fn ppm_has(bytes: &[u8], pred: impl Fn(u8, u8, u8) -> bool) -> bool {
    let data_at = bytes
        .windows(5)
        .position(|window| window == b"\n255\n")
        .unwrap()
        + 5;
    bytes[data_at..]
        .chunks(3)
        .any(|px| px.len() == 3 && pred(px[0], px[1], px[2]))
}

fn ppm_rgb(bytes: &[u8], x: u32, y: u32) -> (u8, u8, u8) {
    let data_at = bytes
        .windows(5)
        .position(|window| window == b"\n255\n")
        .unwrap()
        + 5;
    let header = std::str::from_utf8(&bytes[..data_at]).unwrap();
    let mut lines = header.split('\n');
    assert_eq!(lines.next(), Some("P6"));
    let size = lines.next().unwrap();
    let width: u32 = size.split_whitespace().next().unwrap().parse().unwrap();
    let i = ((y * width + x) * 3) as usize;
    let px = &bytes[data_at + i..data_at + i + 3];
    (px[0], px[1], px[2])
}
