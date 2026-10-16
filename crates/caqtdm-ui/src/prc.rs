//! PSI `.prc` grids. A small column-oriented table that caQtDM turns into
//! monitors and controls. `formRead` becomes a line monitor, `formWrite` a
//! text entry, `comment` a label.

use caqtdm_core::{Color, PropValue, Rect};

use crate::model::{DisplayFile, SourceFormat, WidgetNode};

pub fn parse_prc(text: &str) -> Result<DisplayFile, crate::error::UiError> {
    let mut columns = 2i32;
    let mut root = WidgetNode::new("QWidget", "prc");
    let mut row = 0i32;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#!grid") {
            columns = rest.trim().parse().unwrap_or(2).max(1);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let mut parts = split_prc(line);
        if parts.is_empty() {
            continue;
        }
        let name = parts.remove(0);
        let kind = parts
            .first()
            .cloned()
            .unwrap_or_else(|| "formRead".to_string());
        if kind == "comment" || name == "comment" {
            let text = parts.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
            let mut label = WidgetNode::new("caLabel", format!("comment_{row}"));
            label.set_prop(
                "text",
                PropValue::String(if text.is_empty() { name } else { text }),
            );
            label.set_prop(
                "geometry",
                PropValue::Rect(Rect::new(8, 8 + row * 28, 360, 24)),
            );
            if let Some(color) = flag_value(&parts, "-fg") {
                if let Some(c) = Color::parse(&color) {
                    label.set_prop("foreground", PropValue::Color(c));
                }
            }
            root.children.push(label);
        } else {
            let class_name = match kind.as_str() {
                "formWrite" | "textWrite" => "caTextEntry",
                "formButton" | "messageButton" => "caMessageButton",
                "menu" | "formMenu" => "caMenu",
                "formLed" | "led" => "caLed",
                "formSlider" | "slider" => "caSlider",
                "formGauge" | "gauge" => "caMeter",
                _ => "caLineEdit",
            };
            let mut node = WidgetNode::new(class_name, format!("prc_{row}"));
            node.set_prop("channel", PropValue::String(name.clone()));
            node.set_prop("text", PropValue::String(name));
            let col = row % columns;
            let grid_row = row / columns;
            node.set_prop(
                "geometry",
                PropValue::Rect(Rect::new(8 + col * 180, 8 + grid_row * 28, 160, 24)),
            );
            node.set_prop("colorMode", PropValue::Enum("Alarm".into()));
            if class_name == "caMenu" {
                let items = parts.iter().skip(1).cloned().collect::<Vec<_>>().join(",");
                if !items.is_empty() {
                    node.set_prop("items", PropValue::String(items));
                }
            }
            root.children.push(node);
        }
        row += 1;
    }
    let height = 16 + row * 28;
    root.set_prop(
        "geometry",
        PropValue::Rect(Rect::new(0, 0, 16 + columns * 180, height.max(40))),
    );
    Ok(DisplayFile {
        version: "prc".to_string(),
        class_name: "PrcGrid".to_string(),
        root,
        custom_widgets: Vec::new(),
        source: SourceFormat::Prc,
        path: None,
    })
}

fn split_prc(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = false;
    for ch in line.chars() {
        match ch {
            '"' => quote = !quote,
            c if c.is_whitespace() && !quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn flag_value(parts: &[String], flag: &str) -> Option<String> {
    parts
        .iter()
        .position(|p| p == flag)
        .and_then(|i| parts.get(i + 1).cloned())
}
