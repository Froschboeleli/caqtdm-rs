//! A practical EDM `.edl` reader. Screens are `beginX` / `endX` blocks with
//! `key value` lines. The objects caQtDM's edl2ui cares about are mapped onto
//! the same widget classes as a `.ui` file.

use caqtdm_core::{Color, PropValue, Rect};

use crate::error::UiError;
use crate::model::{DisplayFile, SourceFormat, WidgetNode};

pub fn parse_edl(text: &str) -> Result<DisplayFile, UiError> {
    let blocks = split_blocks(text);
    let mut root = WidgetNode::new("QWidget", "edm");
    root.set_prop("geometry", PropValue::Rect(Rect::new(0, 0, 400, 300)));
    for block in &blocks {
        if block.kind.contains("ScreenProperties") {
            apply_screen(&mut root, block);
        } else if block.kind.starts_with("object ") || block.header.starts_with("object ") {
            root.children.push(widget_from_edl(block));
        }
    }
    Ok(DisplayFile {
        version: "edl".to_string(),
        class_name: "Display".to_string(),
        root,
        custom_widgets: Vec::new(),
        source: SourceFormat::Edl,
        path: None,
    })
}

struct Block {
    header: String,
    kind: String,
    lines: Vec<(String, String)>,
    raw: Vec<String>,
}

fn split_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("begin") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(Block {
                header: String::new(),
                kind: trimmed.trim_start_matches("begin").trim().to_string(),
                lines: Vec::new(),
                raw: Vec::new(),
            });
            continue;
        }
        if trimmed.starts_with("end") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            continue;
        }
        if trimmed.starts_with("object ") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(Block {
                header: trimmed.to_string(),
                kind: trimmed.to_string(),
                lines: Vec::new(),
                raw: Vec::new(),
            });
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.raw.push(trimmed.to_string());
            if let Some((k, v)) = trimmed.split_once(char::is_whitespace) {
                block
                    .lines
                    .push((k.to_string(), v.trim().trim_matches('"').to_string()));
            }
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn apply_screen(root: &mut WidgetNode, block: &Block) {
    let w = int_field(block, "w").unwrap_or(400);
    let h = int_field(block, "h").unwrap_or(300);
    root.set_prop("geometry", PropValue::Rect(Rect::new(0, 0, w, h)));
    if let Some(title) = field(block, "title") {
        root.set_prop("windowTitle", PropValue::String(title.to_string()));
    }
    if let Some(color) = color_field(block, "bgColor") {
        root.set_prop("background", PropValue::Color(color));
    }
}

fn widget_from_edl(block: &Block) -> WidgetNode {
    let class_token = block
        .header
        .strip_prefix("object ")
        .unwrap_or(block.kind.as_str())
        .trim();
    let (class_name, form) = match class_token {
        "activeXTextClass" | "activeXTextDspClass" | "activeXRegTextClass" => ("caLineEdit", None),
        "activeMessageButtonClass" => ("caMessageButton", None),
        "activeButtonClass" | "activeMenuButtonClass" => ("caMenu", None),
        "activeChoiceButtonClass" => ("caChoice", None),
        "activeMotifSliderClass" | "activeSliderClass" => ("caSlider", None),
        "activeMeterClass" => ("caMeter", None),
        "activeBarClass" => ("caThermo", None),
        "activeCircleClass" => ("caGraphics", Some("Circle")),
        "activeRectangleClass" => ("caGraphics", Some("Rectangle")),
        "activeLineClass" => ("caGraphics", Some("Line")),
        "activeArcClass" => ("caGraphics", Some("Arc")),
        "activePipClass" | "activeRelatedDisplayClass" => ("caRelatedDisplay", None),
        "xyGraphClass" | "activeXyPlotClass" => ("caCartesianPlot", None),
        "activeStripChartClass" => ("caStripPlot", None),
        "activeGroupClass" => ("caFrame", None),
        "Text" | "text" => ("caLabel", None),
        _ => ("caLabel", None),
    };
    let mut node = WidgetNode::new(
        class_name,
        format!("edl_{}", class_token.trim_end_matches("Class")),
    );
    let x = int_field(block, "x").unwrap_or(0);
    let y = int_field(block, "y").unwrap_or(0);
    let w = int_field(block, "w").unwrap_or(80);
    let h = int_field(block, "h").unwrap_or(20);
    node.set_prop("geometry", PropValue::Rect(Rect::new(x, y, w, h)));
    if let Some(pv) = field(block, "controlPv")
        .or_else(|| field(block, "indicatorPv"))
        .or_else(|| field(block, "pv"))
    {
        node.set_prop("channel", PropValue::String(pv.to_string()));
    } else if let Some(pv) = first_quoted_pv(block) {
        node.set_prop("channel", PropValue::String(pv));
    }
    if let Some(text) = field(block, "value").or_else(|| field(block, "fontAlign")) {
        if class_name == "caLabel" {
            node.set_prop("text", PropValue::String(text.to_string()));
        }
    }
    if let Some(label) = field(block, "label") {
        node.set_prop("label", PropValue::String(label.to_string()));
        if class_name == "caLabel" || class_name == "caMessageButton" {
            node.set_prop("text", PropValue::String(label.to_string()));
        }
    }
    if let Some(color) = color_field(block, "fgColor") {
        node.set_prop("foreground", PropValue::Color(color));
    }
    if let Some(color) = color_field(block, "bgColor") {
        node.set_prop("background", PropValue::Color(color));
    }
    if field(block, "fgAlarm").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        node.set_prop("colorMode", PropValue::Enum("Alarm".into()));
    }
    if let Some(form) = form {
        node.set_prop("form", PropValue::Enum(format!("caGraphics::Form::{form}")));
    }
    node
}

fn field<'a>(block: &'a Block, key: &str) -> Option<&'a str> {
    block
        .lines
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn int_field(block: &Block, key: &str) -> Option<i32> {
    field(block, key).and_then(|s| s.parse().ok())
}

fn color_field(block: &Block, key: &str) -> Option<Color> {
    let value = field(block, key)?;
    if let Some(index) = value
        .strip_prefix("index")
        .map(str::trim)
        .and_then(|s| s.parse().ok())
    {
        return Some(caqtdm_core::medm_color(index));
    }
    Color::parse(value)
}

fn first_quoted_pv(block: &Block) -> Option<String> {
    for line in &block.raw {
        if line.starts_with('"') || line.contains(':') && !line.contains(' ') {
            let cleaned = line
                .trim_matches(|c| c == '"' || c == '{' || c == '}')
                .trim();
            if cleaned.contains(':') {
                return Some(cleaned.to_string());
            }
        }
    }
    None
}
