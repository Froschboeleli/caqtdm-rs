//! Command line for the display manager.
//!
//! `caqtdm run vacuum.ui` opens the egui viewer. `dump`, `validate`, `convert`,
//! `paint`, and `widgets` stay headless so they can run in tests.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use caqtdm_core::parse_macro_list;
use caqtdm_datasource::{register_builtin, Hub};
use caqtdm_epics_ca::CaSource;
use caqtdm_epics_pva::PvaSource;
use caqtdm_paint::paint;
use caqtdm_ui::{expand_includes, load_path, SearchPath, WidgetNode};
use caqtdm_widgets::{supported_classes, Session};

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
}

pub fn execute(args: &[String]) -> Result<String, String> {
    let options = parse_args(args)?;
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
                let channel = node.prop_str("channel").unwrap_or("");
                lines.push(format!("{} {} {}", node.class_name, node.name, channel));
            });
            Ok(lines.join("\n"))
        }
        "paint" => {
            let path = require_file(&options, "paint")?;
            let display = load_display(&path, &options)?;
            let mut session = Session::open(
                display.root,
                standard_hub(&options),
                &parse_macro_list(&options.macros),
            );
            session.allow_shell = options.execute_shell;
            session.tick(0);
            let image = paint(&session.visuals());
            let dest = options
                .output
                .clone()
                .unwrap_or_else(|| path.with_extension("ppm"));
            std::fs::write(&dest, image.to_ppm()).map_err(|e| e.to_string())?;
            Ok(format!("wrote {}", dest.display()))
        }
        "run" => {
            let path = require_file(&options, "run")?;
            let display = load_display(&path, &options)?;
            let macros = parse_macro_list(&options.macros);
            let mut session = Session::open(display.root, standard_hub(&options), &macros);
            session.allow_shell = options.execute_shell;
            let mut search = SearchPath::from_file(&path);
            if let Some(parent) = path.parent() {
                search.push(parent);
            }
            let opts = options.clone();
            let factory = Arc::new(move || standard_hub(&opts));
            caqtdm_egui::launch(session, search, macros, factory).map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        other => Err(format!("unknown command {other}\n{}", help_text())),
    }
}

fn require_file(options: &Options, command: &str) -> Result<PathBuf, String> {
    options
        .file
        .clone()
        .ok_or_else(|| format!("{command} needs a display file"))
}

fn load_display(path: &Path, options: &Options) -> Result<caqtdm_ui::DisplayFile, String> {
    let mut display = load_path(path).map_err(|e| e.to_string())?;
    let mut search = SearchPath::from_file(path);
    if let Some(parent) = path.parent() {
        search.push(parent);
    }
    let macros = parse_macro_list(&options.macros);
    expand_includes(&mut display.root, &search, &macros).map_err(|e| e.to_string())?;
    Ok(display)
}

fn standard_hub(options: &Options) -> Hub {
    let mut hub = Hub::new(&options.control_system);
    register_builtin(&mut hub);
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

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        command: String::new(),
        file: None,
        macros: String::new(),
        control_system: "epics3".into(),
        output: None,
        execute_shell: false,
        ca: None,
        pva: None,
    };
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" | "-help" => options.command = "help".into(),
            "--execute-shell" => options.execute_shell = true,
            "-macro" | "--macro" | "-cs" | "--cs" | "-o" | "--output" | "-display" | "--ca"
            | "--pva" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| format!("{arg} needs a value"))?;
                match arg {
                    "-macro" | "--macro" => options.macros = value.clone(),
                    "-cs" | "--cs" => options.control_system = value.clone(),
                    "-o" | "--output" => options.output = Some(PathBuf::from(value)),
                    "-display" => options.file = Some(PathBuf::from(value)),
                    "--ca" => options.ca = Some(parse_addr(value)?),
                    "--pva" => options.pva = Some(parse_addr(value)?),
                    _ => {}
                }
                i += 1;
            }
            "dump" | "validate" | "paint" | "widgets" | "convert" | "run" | "help"
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
    if options.command.is_empty() {
        options.command = if options.file.is_some() {
            "run".into()
        } else {
            "help".into()
        };
    }
    Ok(options)
}

fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    text.parse().map_err(|_| format!("bad address {text}"))
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
  widgets    list widget classes the runtime draws
  help

Options:
  -display FILE
  -macro KEY=VALUE,KEY2=VALUE2
  -cs PLUGIN          default control system (epics3)
  --ca HOST:PORT      Channel Access server
  --pva HOST:PORT     PV Access server (scalar subset)
  -o FILE             PPM output for paint
  --execute-shell     run caShellCommand instead of recording it
"
    .to_string()
}
