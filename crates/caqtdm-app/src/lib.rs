//! Command line for the display manager.
//!
//! `caqtdm run vacuum.ui` opens the egui viewer. `dump`, `validate`, `convert`,
//! `paint`, and `widgets` stay headless so they can run in tests.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use caqtdm_core::parse_macro_list;
use caqtdm_core::PropValue;
use caqtdm_datasource::{register_builtin, BsreadSource, DataSource, Hub};
use caqtdm_epics_ca::CaSource;
use caqtdm_epics_pva::PvaSource;
use caqtdm_paint::paint_in;
use caqtdm_ui::{expand_includes, load_path, SearchPath, WidgetNode};
use caqtdm_widgets::{load_image_notes, supported_classes, Session};

mod config;
mod net;
mod rfb;

pub use config::ConfigFile;
pub use net::{
    attach_to, http_get, serve_attach, serve_attach_at, serve_http, serve_http_at,
    websocket_accept, websocket_roundtrip,
};
pub use rfb::{
    novnc_version, rfb_framebuffer, rfb_pointer, serve_novnc, serve_rfb, RfbEvent, RfbFrame,
};

#[derive(Debug, Clone)]
struct Options {
    command: String,
    file: Option<PathBuf>,
    macros: String,
    control_system: String,
    output: Option<PathBuf>,
    execute_shell: bool,
    ca: Option<SocketAddr>,
    pva: Option<SocketAddr>,
    macro_file: Option<PathBuf>,
    attach: Option<SocketAddr>,
    no_msg: bool,
    no_resize: bool,
    execute_only: bool,
    http_config: bool,
    empty_cache: bool,
    web_interaction: bool,
    allow_insecure_shell: bool,
    server: bool,
    style: String,
    style_file: Option<PathBuf>,
    display_font: String,
    geometry: Option<(u32, u32)>,
    url: String,
    host: String,
    server_port: u16,
    web_port: u16,
    web_port_set: bool,
    web_timeout: u64,
    web_limit: usize,
    launcher_root: String,
    novnc: bool,
    novnc_readonly: bool,
    slave_server: bool,
    options: Vec<(String, String)>,
}

pub fn execute(args: &[String]) -> Result<String, String> {
    let mut options = parse_args(args)?;
    options.macros = merge_macros(&options)?;
    match options.command.as_str() {
        "help" => Ok(help_text()),
        "widgets" => Ok(supported_classes().join("\n")),
        "validate" => {
            let path = require_file(&options, "validate")?;
            let _ = load_display(&path, &options)?;
            Ok(format!("ok {}", path.display()))
        }
        "dump" => {
            let path = require_file(&options, "dump")?;
            let display = load_display(&path, &options)?;
            let mut out = String::new();
            dump_node(&display.root, 0, &mut out);
            Ok(out)
        }
        "convert" => {
            let path = require_file(&options, "convert")?;
            let display = load_display(&path, &options)?;
            let mut lines = vec![format!("source {:?}", display.source)];
            display.root.walk(&mut |node| {
                let channel = node
                    .prop_str("channel")
                    .filter(|s| !s.is_empty())
                    .or_else(|| node.prop_str("files"))
                    .unwrap_or("");
                lines.push(format!("{} {} {}", node.class_name, node.name, channel));
            });
            Ok(lines.join("\n"))
        }
        "paint" | "print" | "snapshot" => paint_display(&options),
        "config" => Ok(config_text(&options)),
        "serve" => serve_flags(&options),
        "run" => {
            let path = require_file(&options, "run")?;
            if let Some(addr) = options.attach {
                return attach_to(addr, &path.display().to_string()).map_err(|err| err.to_string());
            }
            let display = load_display(&path, &options)?;
            let macros = parse_macro_list(&options.macros);
            let mut session = Session::open(display.root, standard_hub(&options), &macros);
            session.allow_shell = options.execute_shell;
            let search = display_search(&path, &options);
            let opts = options.clone();
            let factory = Arc::new(move || standard_hub(&opts));
            caqtdm_egui::launch(session, path, search, macros, factory)
                .map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        other => Err(format!("unknown command {other}\n{}", help_text())),
    }
}

fn require_file(options: &Options, command: &str) -> Result<PathBuf, String> {
    let requested = options
        .file
        .clone()
        .ok_or_else(|| format!("{command} needs a display file"))?;
    resolve_display(&requested, options)
}

fn resolve_display(requested: &Path, options: &Options) -> Result<PathBuf, String> {
    let mut search = SearchPath::from_env();
    if let Some(dir) = url_directory(&options.url) {
        search.push(dir);
    }
    search.find(&requested.to_string_lossy()).ok_or_else(|| {
        format!(
            "{}: No such file or directory (os error 2)",
            requested.display()
        )
    })
}

fn display_search(path: &Path, options: &Options) -> SearchPath {
    let mut search = SearchPath::from_file(path);
    if let Some(dir) = url_directory(&options.url) {
        search.push(dir);
    }
    search
}

fn load_display(path: &Path, options: &Options) -> Result<caqtdm_ui::DisplayFile, String> {
    let mut display = load_path(path).map_err(|e| e.to_string())?;
    let search = display_search(path, options);
    let macros = parse_macro_list(&options.macros);
    for warning in
        expand_includes(&mut display.root, &search, &macros).map_err(|e| e.to_string())?
    {
        eprintln!("caQtDM -- {warning}");
    }
    if let Some(style) = &options.style_file {
        let css = std::fs::read_to_string(style)
            .map_err(|err| format!("style file {}: {err}", style.display()))?;
        apply_stylesheet(&mut display.root, &css);
    }
    Ok(display)
}

fn paint_display(options: &Options) -> Result<String, String> {
    let path = require_file(options, "paint")?;
    let display = load_display(&path, options)?;
    let mut session = Session::open(
        display.root,
        standard_hub(options),
        &parse_macro_list(&options.macros),
    );
    session.allow_shell = options.execute_shell;
    session.tick(0);
    install_font(options)?;
    let mut visuals = session.visuals();
    let search = display_search(&path, options);
    load_image_notes(&mut visuals, |note| {
        search.find(note).and_then(|file| std::fs::read(file).ok())
    });
    // The window is the root widget's size. Children past its edge are
    // clipped, the way Qt shows them.
    let root = session.root.rect;
    let (width, height) = options
        .geometry
        .unwrap_or((root.width.max(1) as u32, root.height.max(1) as u32));
    let caption = if options.command == "snapshot" {
        let short = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("display");
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dur| dur.as_secs() as i64)
            .unwrap_or(0);
        Some(snapshot_caption(short, unix))
    } else {
        None
    };
    let image = caqtdm_paint::grab(&visuals, width, height, caption.as_deref());
    if options.command == "print" {
        let short = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("display");
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dur| dur.as_secs() as i64)
            .unwrap_or(0);
        let bytes = image.to_print_pdf(&snapshot_caption(short, unix), &path.display().to_string());
        let dest = options
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from("caQtDM.ps"));
        std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
        let via = spool_pdf(&dest)?;
        let note = via
            .map(|program| format!(" via {program}"))
            .unwrap_or_default();
        return Ok(format!(
            "wrote {} {}x{}{note}",
            dest.display(),
            image.width,
            image.height
        ));
    }
    let (bytes, dest) = match options.command.as_str() {
        "snapshot" => {
            let name =
                std::env::var("CAQTDM_SCREENSHOT_NAME").unwrap_or_else(|_| "caQtDM".to_string());
            (
                image.to_png(),
                options
                    .output
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(format!("{name}.png"))),
            )
        }
        _ => (
            image.to_ppm(),
            options
                .output
                .clone()
                .unwrap_or_else(|| path.with_extension("ppm")),
        ),
    };
    std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
    Ok(format!(
        "wrote {} {}x{}",
        dest.display(),
        image.width,
        image.height
    ))
}

fn apply_stylesheet(node: &mut WidgetNode, css: &str) {
    if node.prop_str("styleSheet").is_none() {
        node.set_prop("styleSheet", PropValue::String(css.to_string()));
    }
    for child in &mut node.children {
        apply_stylesheet(child, css);
    }
}

fn url_directory(url: &str) -> Option<&Path> {
    let path = Path::new(url);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

fn standard_hub(options: &Options) -> Hub {
    let mut hub = Hub::new(&options.control_system);
    register_builtin(&mut hub);
    let modulo = option_u64(options, "bsmodulo");
    let offset = option_u64(options, "bsoffset");
    if modulo > 1 || offset > 0 {
        hub.register(Box::new(BsreadSource::with_sampling(modulo, offset)));
    }
    match options.ca {
        Some(addr) => match CaSource::connect(addr) {
            Ok(source) => hub.register(Box::new(source)),
            Err(err) => {
                eprintln!("CA connect {addr} failed: {err}");
                hub.register(Box::new(CaSource::offline()));
            }
        },
        None => hub.register(Box::new(CaSource::offline())),
    }
    match options.pva {
        Some(addr) => match PvaSource::connect(addr) {
            Ok(source) => hub.register(Box::new(source)),
            Err(err) => {
                eprintln!("PVA connect {addr} failed: {err}");
                hub.register(Box::new(PvaSource::offline()));
            }
        },
        None => hub.register(Box::new(PvaSource::offline())),
    }
    hub
}

fn dump_node(node: &WidgetNode, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let channel = node.prop_str("channel").unwrap_or("");
    out.push_str(&format!(
        "{pad}{} {} {}x{} {}\n",
        node.class_name, node.name, node.rect.width, node.rect.height, channel
    ));
    for child in &node.children {
        dump_node(child, depth + 1, out);
    }
}

fn default_options() -> Options {
    Options {
        command: String::new(),
        file: None,
        macros: String::new(),
        control_system: "epics3".into(),
        output: None,
        execute_shell: false,
        ca: None,
        pva: None,
        macro_file: None,
        attach: None,
        no_msg: false,
        no_resize: false,
        execute_only: false,
        http_config: false,
        empty_cache: false,
        web_interaction: false,
        allow_insecure_shell: false,
        server: false,
        style: String::new(),
        style_file: None,
        display_font: String::new(),
        geometry: None,
        url: String::new(),
        host: "127.0.0.1".into(),
        server_port: 0,
        web_port: 0,
        web_port_set: false,
        web_timeout: 0,
        web_limit: 1000,
        launcher_root: String::new(),
        novnc: false,
        novnc_readonly: false,
        slave_server: false,
        options: Vec::new(),
    }
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = default_options();
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" | "-help" | "-?" => options.command = "help".into(),
            "--execute-shell" => options.execute_shell = true,
            "-noMsg" => options.no_msg = true,
            "-noResize" => options.no_resize = true,
            "-savetoimage" => options.command = "snapshot".into(),
            "-x" => options.execute_only = true,
            "-httpconfig" => options.http_config = true,
            "-emptycache" => options.empty_cache = true,
            "-web_interaction_timeout" => options.web_interaction = true,
            "-web_allow_insecure_cashell_commands" => options.allow_insecure_shell = true,
            "-novnc" => options.novnc = true,
            "-novnc_readonly" => options.novnc_readonly = true,
            "-slave_server" => options.slave_server = true,
            "-server" => options.server = true,
            "-print" => options.command = "print".into(),
            "-macro"
            | "--macro"
            | "-macrodefs"
            | "-cs"
            | "--cs"
            | "-o"
            | "--output"
            | "-display"
            | "--ca"
            | "--pva"
            | "-config"
            | "-attach"
            | "-style"
            | "-stylefile"
            | "-displayFont"
            | "-dg"
            | "-displayGeometry"
            | "-url"
            | "-option"
            | "-server_port"
            | "-web_server_port"
            | "-web_timeout"
            | "-web_instance_limit"
            | "-host"
            | "-web_launcher_root_file" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| format!("{arg} needs a value"))?
                    .clone();
                match arg {
                    "-macro" | "--macro" => options.macros = value,
                    "-macrodefs" => options.macro_file = Some(PathBuf::from(value)),
                    "-cs" | "--cs" => options.control_system = value,
                    "-o" | "--output" => options.output = Some(PathBuf::from(&value)),
                    "-display" | "-displayFont" => {
                        let _ = value;
                    }
                    "--ca" => options.ca = Some(parse_addr(&value)?),
                    "--pva" => options.pva = Some(parse_addr(&value)?),
                    "-config" => apply_config(&mut options, Path::new(&value))?,
                    "-attach" => options.attach = Some(parse_addr(&value)?),
                    "-style" => options.style = value,
                    "-stylefile" => options.style_file = Some(PathBuf::from(value)),
                    "-dg" | "-displayGeometry" => options.geometry = Some(parse_geometry(&value)?),
                    "-url" => options.url = value,
                    "-option" => options.options = parse_option_list(&value)?,
                    "-server_port" => options.server_port = parse_port(&value)?,
                    "-web_server_port" => {
                        options.web_port = parse_port(&value)?;
                        options.web_port_set = true;
                    }
                    "-web_timeout" => options.web_timeout = parse_timeout(&value)?,
                    "-web_instance_limit" => options.web_limit = parse_limit(&value)?,
                    "-host" => {
                        if value.parse::<std::net::IpAddr>().is_ok() {
                            options.host = value;
                        }
                    }
                    "-web_launcher_root_file" => options.launcher_root = value,
                    _ => {}
                }
                i += 1;
            }
            "dump" | "validate" | "paint" | "print" | "snapshot" | "widgets" | "convert"
            | "run" | "help" | "config" | "serve"
                if options.command.is_empty() =>
            {
                options.command = arg.to_string();
            }
            other if !other.starts_with('-') && options.file.is_none() => {
                options.file = Some(PathBuf::from(other));
            }
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    if options.http_config && (options.command.is_empty() || options.command == "run") {
        options.command = "config".into();
    }
    if options.allow_insecure_shell {
        options.execute_shell = true;
    }
    if options.novnc_readonly && !options.novnc {
        return Err("novnc-readonly can only be used together with -novnc".into());
    }
    if options.command.is_empty() {
        options.command = if options.novnc || (options.server && options.file.is_none()) {
            "serve".into()
        } else if options.file.is_some() {
            "run".into()
        } else {
            "help".into()
        };
    }
    let removed = clear_cache(options.empty_cache)?;
    if options.empty_cache {
        options
            .options
            .push(("emptycache".into(), removed.to_string()));
    }
    Ok(options)
}

fn parse_geometry(text: &str) -> Result<(u32, u32), String> {
    let size = text.split('+').next().unwrap_or(text);
    let Some((w, h)) = size.split_once('x') else {
        return Err(format!("bad geometry {text}"));
    };
    let width: u32 = w.parse().map_err(|_| format!("bad geometry {text}"))?;
    let height: u32 = h.parse().map_err(|_| format!("bad geometry {text}"))?;
    if width == 0 || height == 0 {
        return Err(format!("bad geometry {text}"));
    }
    Ok((width, height))
}

fn parse_port(text: &str) -> Result<u16, String> {
    text.parse().map_err(|_| format!("bad port {text}"))
}

fn parse_timeout(text: &str) -> Result<u64, String> {
    let seconds: u64 = text
        .parse()
        .map_err(|_| format!("bad web timeout {text}"))?;
    Ok(if seconds < 76 { 0 } else { seconds })
}

fn parse_limit(text: &str) -> Result<usize, String> {
    let limit: usize = text
        .parse()
        .map_err(|_| format!("bad web instance limit {text}"))?;
    if limit == 0 {
        return Err("web instance limit must be at least 1".into());
    }
    Ok(limit.min(5000))
}

fn parse_option_list(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!("bad option {part}"));
        };
        out.push((key.trim().to_string(), value.trim().to_string()));
    }
    Ok(out)
}

fn option_u64(options: &Options, key: &str) -> u64 {
    options
        .options
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0)
}

fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("caqtdm-display-cache")
}

fn clear_cache(enabled: bool) -> Result<usize, String> {
    if !enabled {
        return Ok(0);
    }
    let dir = cache_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        std::fs::remove_file(entry.path()).map_err(|err| err.to_string())?;
        count += 1;
    }
    Ok(count)
}

fn serve_flags(options: &Options) -> Result<String, String> {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let stop = Arc::new(AtomicBool::new(false));
    let host: std::net::IpAddr = options
        .host
        .parse()
        .map_err(|_| format!("bad host {}", options.host))?;
    let attach = serve_attach_at(
        std::net::SocketAddr::new(host, options.server_port),
        Arc::clone(&stop),
    )
    .map_err(|err| err.to_string())?;
    let web_port = effective_web_port(options);
    let status = server_status(options);
    let http = serve_http_at(
        std::net::SocketAddr::new(host, web_port),
        Arc::clone(&stop),
        status,
        options.web_limit,
    )
    .map_err(|err| err.to_string())?;
    let mut text = format!("http {http}\nattach {attach}");
    if options.novnc {
        install_font(options)?;
        let frame = framebuffer(options)?;
        let vnc = serve_rfb(
            std::net::SocketAddr::new(host, 0),
            Arc::clone(&stop),
            frame.clone(),
        )
        .map_err(|err| err.to_string())?;
        let novnc = serve_novnc(std::net::SocketAddr::new(host, 0), stop, frame)
            .map_err(|err| err.to_string())?;
        text.push_str(&format!(
            "\nvnc {}\nnovnc {}\nreadonly {}",
            vnc.0, novnc.0, options.novnc_readonly
        ));
    }
    Ok(text)
}

fn install_font(options: &Options) -> Result<(), String> {
    let name = if options.display_font.is_empty() {
        "Arimo"
    } else {
        options.display_font.as_str()
    };
    caqtdm_paint::install_display_font(name)
}

fn framebuffer(options: &Options) -> Result<RfbFrame, String> {
    if options.file.is_some() {
        let path = require_file(options, "novnc")?;
        let display = load_display(&path, options)?;
        let mut session = Session::open(
            display.root,
            standard_hub(options),
            &parse_macro_list(&options.macros),
        );
        session.tick(0);
        let root = session.root.rect;
        let image = paint_in(
            &session.visuals(),
            root.width.max(1) as u32,
            root.height.max(1) as u32,
        );
        return Ok(RfbFrame {
            width: image.width.min(u16::MAX as u32) as u16,
            height: image.height.min(u16::MAX as u32) as u16,
            rgba: image.rgba,
            readonly: options.novnc_readonly,
        });
    }
    let mut rgba = vec![0u8; 8 * 8 * 4];
    rgba[0] = 255;
    rgba[3] = 255;
    Ok(RfbFrame {
        width: 8,
        height: 8,
        rgba,
        readonly: options.novnc_readonly,
    })
}

fn effective_web_port(options: &Options) -> u16 {
    if options.web_port_set {
        options.web_port
    } else if options.server_port == u16::MAX {
        options.server_port - 1
    } else if options.server_port > 0 {
        options.server_port.saturating_add(1)
    } else {
        0
    }
}

fn server_status(options: &Options) -> String {
    format!(
        "caQtDM\nstyle={}\nnoMsg={}\nnoResize={}\nexecute_only={}\nshell={}\nweb_timeout={}\nweb_limit={}\ninteraction={}\nfont={}\nroot={}\nurl={}\n",
        options.style,
        options.no_msg,
        options.no_resize,
        options.execute_only,
        options.execute_shell,
        options.web_timeout,
        options.web_limit,
        options.web_interaction,
        options.display_font,
        options.launcher_root,
        options.url,
    )
}

/// Pulse id the bsread plugin would publish for these arguments.
pub fn sampled_pulse(args: &[String], now_ms: u64) -> Result<i64, String> {
    let options = parse_args(args)?;
    let modulo = option_u64(&options, "bsmodulo");
    let offset = option_u64(&options, "bsoffset");
    let mut source = BsreadSource::with_sampling(modulo, offset);
    let spec = caqtdm_core::parse_channel("pulse_id", "bsread");
    source.add_monitor(&spec).map_err(|err| err.to_string())?;
    let updates = source.poll(now_ms);
    Ok(updates
        .first()
        .and_then(|update| update.data.value.as_i64())
        .unwrap_or(0))
}

fn merge_macros(options: &Options) -> Result<String, String> {
    let mut parts = Vec::new();
    if let Some(path) = &options.macro_file {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("macro file {}: {err}", path.display()))?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            parts.push(line.to_string());
        }
    }
    if !options.macros.is_empty() {
        parts.push(options.macros.clone());
    }
    Ok(parts.join(","))
}

fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    text.parse().map_err(|_| format!("bad address {text}"))
}

fn apply_config(options: &mut Options, path: &Path) -> Result<(), String> {
    let config = ConfigFile::load(path)?;
    if options.control_system == "epics3" {
        options.control_system = config.control_system;
    }
    if options.macros.is_empty() {
        options.macros = config.macros;
    }
    if options.ca.is_none() && !config.ca.is_empty() {
        options.ca = Some(parse_addr(&config.ca)?);
    }
    if options.pva.is_none() && !config.pva.is_empty() {
        options.pva = Some(parse_addr(&config.pva)?);
    }
    if config.execute_shell {
        options.execute_shell = true;
    }
    Ok(())
}

fn config_text(options: &Options) -> String {
    let mut text = ConfigFile {
        control_system: options.control_system.clone(),
        ca: options.ca.map(|addr| addr.to_string()).unwrap_or_default(),
        pva: options.pva.map(|addr| addr.to_string()).unwrap_or_default(),
        macros: options.macros.clone(),
        execute_shell: options.execute_shell,
    }
    .to_text();
    let geometry = options
        .geometry
        .map(|(w, h)| format!("{w}x{h}"))
        .unwrap_or_default();
    let style_file = options
        .style_file
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    text.push_str(&format!(
        "style={}\nstylefile={}\nfont={}\ngeometry={}\nurl={}\nhost={}\nserver_port={}\nweb_port={}\nweb_timeout={}\nweb_limit={}\nnoMsg={}\nnoResize={}\nexecute_only={}\nhttpconfig={}\ninteraction={}\nroot={}\nslave_server={}\n",
        options.style,
        style_file,
        options.display_font,
        geometry,
        options.url,
        options.host,
        options.server_port,
        effective_web_port(options),
        options.web_timeout,
        options.web_limit,
        options.no_msg,
        options.no_resize,
        options.execute_only,
        options.http_config,
        options.web_interaction,
        options.launcher_root,
        options.slave_server,
    ));
    if let Some((_, count)) = options
        .options
        .iter()
        .rev()
        .find(|(key, _)| key == "emptycache")
    {
        text.push_str(&format!("emptycache={count}\n"));
    }
    for (key, value) in &options.options {
        if key != "emptycache" {
            text.push_str(&format!("option.{key}={value}\n"));
        }
    }
    text
}

/// Program and arguments for the native print path. Linux uses CUPS `lp`,
/// macOS uses `lpr`, and Windows uses the shell print verb. `CAQTDM_PRINT_SPOOLER`
/// replaces the program so a test can stand in for the spooler.
pub fn spool_command(pdf: &std::path::Path) -> (String, Vec<String>) {
    if let Ok(custom) = std::env::var("CAQTDM_PRINT_SPOOLER") {
        if !custom.is_empty() {
            return (custom, portrait_spool_args(pdf));
        }
    }
    if cfg!(windows) {
        (
            "powershell.exe".into(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                format!("Start-Process -FilePath '{}' -Verb Print", pdf.display()),
            ],
        )
    } else if cfg!(target_os = "macos") {
        ("lpr".into(), portrait_spool_args(pdf))
    } else {
        ("lp".into(), portrait_spool_args(pdf))
    }
}

fn portrait_spool_args(pdf: &std::path::Path) -> Vec<String> {
    vec![
        "-o".into(),
        "media=A4".into(),
        "-o".into(),
        "orientation-requested=3".into(),
        pdf.display().to_string(),
    ]
}

fn spool_pdf(pdf: &std::path::Path) -> Result<Option<String>, String> {
    let (program, args) = spool_command(pdf);
    let requested = std::env::var("CAQTDM_PRINT_SPOOLER")
        .ok()
        .is_some_and(|text| !text.is_empty());
    if !requested && !program_on_path(&program) {
        return Ok(None);
    }
    let status = std::process::Command::new(&program)
        .args(&args)
        .status()
        .map_err(|err| format!("print spooler {program}: {err}"))?;
    if !status.success() {
        return Err(format!("print spooler {program} exited {status}"));
    }
    Ok(Some(program))
}

fn program_on_path(program: &str) -> bool {
    let path = std::path::Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(program).is_file())
}

/// `yyyy-MM-dd HH:MM:SS, file`, the stamp `save_graphics` paints on a snapshot.
/// `QDate::currentDate` and `QTime::currentTime` are the process local time.
pub fn snapshot_caption(file: &str, unix_secs: i64) -> String {
    let (year, month, day, hour, minute, second) = caqtdm_paint::local_civil(unix_secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}, {file}")
}

fn help_text() -> String {
    "\
caqtdm [command] [file] [options]

Commands:
  run        open the egui viewer (default when a file is given)
  dump       print the widget tree
  validate   parse a display and expand includes
  convert    list widgets and channels
  paint      write a PPM rendering
  print      write a PDF page and hand it to lp, lpr, or the Windows print verb
  snapshot   write a PNG (caQtDM.png, or $CAQTDM_SCREENSHOT_NAME.png)
  widgets    list widget classes the runtime draws
  config     print the operator settings
  serve      status page and the attach port
  help

Options:
  -display FILE       accepted and ignored, matching upstream
  -macro KEY=VALUE,KEY2=VALUE2
  -macrodefs FILE     one KEY=VALUE per line, then -macro overrides
  -config FILE        operator settings (control system, macros, CA, PVA)
  -cs PLUGIN          default control system (epics3)
  --ca HOST:PORT      Channel Access server
  --pva HOST:PORT     PV Access server (scalar subset)
  -attach HOST:PORT   send the display path to a running instance
  -o FILE             output path for paint, print, and snapshot
  -savetoimage        write a PNG and exit, matching upstream (no filename argument)
  --execute-shell     run caShellCommand instead of recording it
  -x                  accepted and ignored, matching upstream (MEDM execute-only has no effect)
  -noMsg              quiet the status page
  -noResize           report that the window should not be resized
  -style NAME         style name reported on the status page
  -stylefile FILE     color, background-color, and font-size for widgets
  -displayFont NAME   accepted and ignored, matching upstream
  -dg WxH             paint canvas size (also -displayGeometry, +X+Y ignored)
  -url DIR            extra directory for included displays
                      CAQTDM_DISPLAY_PATH is also searched (: on Unix, ; on Windows)
  -option k=v,k2=v2   plugin options (bsmodulo, bsoffset)
  -httpconfig         print the operator settings
  -emptycache         delete files in the display cache
  -server             status page and attach port
  -host ADDR          bind address (default 127.0.0.1). A non-address is ignored
  -slave_server       mark this process as a web child, matching upstream
  -server_port N      attach port (0 picks a free port)
  -web_server_port N  status-page port (default is the attach port plus one)
  -web_timeout SECS   ignored when under 76
  -web_instance_limit N   HTTP answers before 503 (1..=5000)
  -web_interaction_timeout
  -web_launcher_root_file FILE
  -web_allow_insecure_cashell_commands
  -novnc              RFB 3.8 server and the same bytes over a WebSocket
  -novnc_readonly     drop pointer and key events (requires -novnc)
  -print              write a portrait A4 PDF and exit (caQtDM.ps unless -o is set)
"
    .to_string()
}
