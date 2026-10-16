//! MEDM `.adl` reader. The file is a brace tree: a quoted or bare name,
//! then either `= value` or `{ ... }`.

use caqtdm_core::{medm_color, PropValue, Rect};

use crate::error::UiError;
use crate::model::{DisplayFile, SourceFormat, WidgetNode};

#[derive(Debug)]
struct Node {
    name: String,
    fields: Vec<Field>,
}

#[derive(Debug)]
struct Field {
    name: String,
    value: Value,
}

#[derive(Debug)]
enum Value {
    Scalar(String),
    Group(Node),
}

pub fn parse_adl(text: &str) -> Result<DisplayFile, UiError> {
    let tokens = tokenize(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let mut objects = Vec::new();
    while parser.pos < parser.tokens.len() {
        objects.push(parser.parse_node()?);
    }
    let mut root = WidgetNode::new("QWidget", "display");
    root.set_prop("geometry", PropValue::Rect(Rect::new(0, 0, 400, 300)));
    for object in objects {
        if object.name == "display" || object.name == "file" {
            apply_display(&mut root, &object);
        } else if let Some(widget) = widget_from_adl(&object) {
            root.children.push(widget);
        }
    }
    Ok(DisplayFile {
        version: "adl".to_string(),
        class_name: "Display".to_string(),
        root,
        custom_widgets: Vec::new(),
        source: SourceFormat::Adl,
        path: None,
    })
}

fn apply_display(root: &mut WidgetNode, object: &Node) {
    if let Some(group) = group(object, "object") {
        let rect = rect_from(group);
        root.set_prop("geometry", PropValue::Rect(rect));
    }
    if let Some(clr) = scalar_int(object, "clr") {
        root.set_prop("foreground", PropValue::Color(medm_color(clr)));
    }
    if let Some(bclr) = scalar_int(object, "bclr") {
        root.set_prop("background", PropValue::Color(medm_color(bclr)));
    }
}

fn widget_from_adl(object: &Node) -> Option<WidgetNode> {
    let kind = object.name.to_ascii_lowercase();
    let (class_name, form) = adl_class(&kind)?;
    let mut node = WidgetNode::new(class_name, unique_name(&object.name));
    if let Some(group) = group(object, "object") {
        node.set_prop("geometry", PropValue::Rect(rect_from(group)));
    }
    apply_basic(&mut node, object);
    apply_dynamic(&mut node, object);
    if let Some(form) = form {
        node.set_prop(
            "form",
            PropValue::Enum(
                match form {
                    "oval" | "circle" => "caGraphics::Form::Circle",
                    "arc" => "caGraphics::Form::Arc",
                    "line" => "caGraphics::Form::Line",
                    "triangle" => "caGraphics::Form::Triangle",
                    "arrow" => "caGraphics::Form::Arrow",
                    _ => "caGraphics::Form::Rectangle",
                }
                .to_string(),
            ),
        );
    }
    if let Some(text) = scalar(object, "textix").or_else(|| scalar(object, "label")) {
        node.set_prop("text", PropValue::String(text.to_string()));
    }
    if let Some(chan) = scalar(object, "chan").or_else(|| scalar_in_controls(object, "chan")) {
        node.set_prop("channel", PropValue::String(chan.to_string()));
    }
    if kind == "composite" {
        for field in &object.fields {
            if let Value::Group(child) = &field.value {
                if group(child, "object").is_some() && child.name != "object" {
                    if let Some(child) = widget_from_adl(child) {
                        node.children.push(child);
                    }
                }
            }
        }
    }
    if kind == "related display" {
        let mut labels = Vec::new();
        let mut files = Vec::new();
        for index in 0..16 {
            let label = scalar(object, &format!("label{index}"))
                .or_else(|| scalar(object, &format!("label[{index}]")));
            let file = scalar(object, &format!("name{index}"))
                .or_else(|| scalar(object, &format!("display[{index}].name")));
            if label.is_none() && file.is_none() {
                continue;
            }
            labels.push(label.unwrap_or("").to_string());
            files.push(file.unwrap_or("").to_string());
        }
        // Also accept a "display" group list.
        if files.is_empty() {
            for field in &object.fields {
                if field.name == "display" {
                    if let Value::Group(group) = &field.value {
                        labels.push(scalar(group, "label").unwrap_or("").to_string());
                        files.push(scalar(group, "name").unwrap_or("").to_string());
                    }
                }
            }
        }
        if !labels.is_empty() {
            node.set_prop("labels", PropValue::String(labels.join(";")));
            node.set_prop("files", PropValue::String(files.join(";")));
        }
    }
    if let Some(min) = scalar_in_controls(object, "lopr").and_then(|s| s.parse::<f64>().ok()) {
        node.set_prop("minValue", PropValue::Double(min));
        node.set_prop("limitsMode", PropValue::Enum("User".into()));
    }
    if let Some(max) = scalar_in_controls(object, "hopr").and_then(|s| s.parse::<f64>().ok()) {
        node.set_prop("maxValue", PropValue::Double(max));
        node.set_prop("limitsMode", PropValue::Enum("User".into()));
    }
    if let Some(end) = scalar_in_controls(object, "endBit").and_then(|s| s.parse::<f64>().ok()) {
        node.set_prop("endBit", PropValue::Double(end));
    }
    if let Some(points) = scalar(object, "points") {
        node.set_prop(
            "xyPairs",
            PropValue::String(points.replace(['(', ')'], "").replace(' ', ";")),
        );
    }
    if class_name == "caImage" {
        if let Some(file) = scalar(object, "image name").or_else(|| scalar(object, "name")) {
            node.set_prop("filename", PropValue::String(file.to_string()));
        }
    }
    for field in &object.fields {
        if let Value::Group(child) = &field.value {
            if is_adl_widget(&child.name) {
                if let Some(child) = widget_from_adl(child) {
                    node.children.push(child);
                }
            }
        }
    }
    Some(node)
}

/// MEDM objects from `parseFuncTable`, plus the composite children caQtDM
/// still draws. A name that is not in that set is skipped. Upstream's
/// `getNextElement` returns null for it and does not emit a widget.
fn adl_class(kind: &str) -> Option<(&'static str, Option<&str>)> {
    Some(match kind {
        "text" => ("caLabel", None),
        "rectangle" | "oval" | "circle" | "arc" | "line" | "triangle" | "arrow" => {
            ("caGraphics", Some(kind))
        }
        "polyline" | "polygon" | "falling line" | "rising line" => ("caPolyLine", None),
        "related display" => ("caRelatedDisplay", None),
        "shell command" => ("caShellCommand", None),
        "message button" => ("caMessageButton", None),
        "menu" => ("caMenu", None),
        "choice button" | "button" => ("caChoice", None),
        "text entry" => ("caTextEntry", None),
        "text update" => ("caLineEdit", None),
        "valuator" | "wheel switch" => ("caSlider", None),
        "meter" => ("caMeter", None),
        "bar" | "indicator" | "bar monitor" => ("caThermo", None),
        "strip chart" => ("caStripPlot", None),
        "cartesian plot" => ("caCartesianPlot", None),
        "byte" => ("caByte", None),
        "image" => ("caImage", None),
        "composite" => ("caFrame", None),
        "scale" => ("caLinearGauge", None),
        "dynamic symbol" => ("caGraphics", None),
        _ => return None,
    })
}

fn is_adl_widget(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "text"
            | "rectangle"
            | "oval"
            | "circle"
            | "arc"
            | "line"
            | "triangle"
            | "arrow"
            | "polyline"
            | "polygon"
            | "related display"
            | "shell command"
            | "message button"
            | "menu"
            | "choice button"
            | "text entry"
            | "text update"
            | "valuator"
            | "wheel switch"
            | "meter"
            | "bar"
            | "indicator"
            | "strip chart"
            | "cartesian plot"
            | "byte"
            | "image"
            | "composite"
            | "scale"
            | "dynamic symbol"
            | "bar monitor"
            | "falling line"
            | "rising line"
            | "button"
    )
}

fn apply_basic(node: &mut WidgetNode, object: &Node) {
    let basic = group(object, "basic attribute").unwrap_or(object);
    if let Some(clr) = scalar_int(basic, "clr") {
        node.set_prop("foreground", PropValue::Color(medm_color(clr)));
    }
    if let Some(bclr) = scalar_int(basic, "bclr") {
        node.set_prop("background", PropValue::Color(medm_color(bclr)));
    }
    if scalar(basic, "fill").is_some_and(|v| v == "outline" || v == "0") {
        node.set_prop(
            "fillstyle",
            PropValue::Enum("caGraphics::FillStyle::Outline".into()),
        );
    }
}

fn apply_dynamic(node: &mut WidgetNode, object: &Node) {
    let Some(dyn_attr) = group(object, "dynamic attribute") else {
        return;
    };
    if let Some(chan) = scalar(dyn_attr, "chan") {
        node.set_prop("channel", PropValue::String(chan.to_string()));
    }
    if let Some(chan) = scalar(dyn_attr, "chanb") {
        node.set_prop("channelB", PropValue::String(chan.to_string()));
    }
    if let Some(vis) = scalar(dyn_attr, "vis") {
        let mapped = match vis {
            "static" => "StaticV",
            "if not zero" => "IfNotZero",
            "if zero" => "IfZero",
            "calc" => "Calc",
            other => other,
        };
        node.set_prop("visibility", PropValue::Enum(mapped.to_string()));
    }
    if let Some(calc) = scalar(dyn_attr, "calc") {
        node.set_prop("visibilityCalc", PropValue::String(calc.to_string()));
    }
    if scalar(dyn_attr, "clr").is_some_and(|c| c == "alarm") {
        node.set_prop("colorMode", PropValue::Enum("Alarm".into()));
    } else {
        node.set_prop("colorMode", PropValue::Enum("Static".into()));
    }
}

fn rect_from(object: &Node) -> Rect {
    Rect::new(
        scalar_int(object, "x").unwrap_or(0),
        scalar_int(object, "y").unwrap_or(0),
        scalar_int(object, "width").unwrap_or(scalar_int(object, "w").unwrap_or(10)),
        scalar_int(object, "height").unwrap_or(scalar_int(object, "h").unwrap_or(10)),
    )
}

fn group<'a>(object: &'a Node, name: &str) -> Option<&'a Node> {
    object.fields.iter().find_map(|f| {
        if f.name.eq_ignore_ascii_case(name) {
            if let Value::Group(node) = &f.value {
                return Some(node);
            }
        }
        None
    })
}

fn scalar_in_controls<'a>(object: &'a Node, name: &str) -> Option<&'a str> {
    if let Some(value) = scalar(object, name) {
        return Some(value);
    }
    for field in &object.fields {
        if field.name.eq_ignore_ascii_case("object") {
            continue;
        }
        if let Value::Group(group) = &field.value {
            if let Some(value) = scalar(group, name) {
                return Some(value);
            }
        }
    }
    None
}

fn scalar<'a>(object: &'a Node, name: &str) -> Option<&'a str> {
    object.fields.iter().find_map(|f| {
        if f.name.eq_ignore_ascii_case(name) {
            if let Value::Scalar(v) = &f.value {
                return Some(v.as_str());
            }
        }
        None
    })
}

fn scalar_int(object: &Node, name: &str) -> Option<i32> {
    scalar(object, name).and_then(|s| s.parse().ok())
}

fn unique_name(class_name: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let slug: String = class_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("adl_{slug}_{n}")
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_node(&mut self) -> Result<Node, UiError> {
        let name = self.expect_name()?;
        self.expect_lbrace()?;
        let mut fields = Vec::new();
        while !self.at(Tok::RBrace) {
            if self.pos >= self.tokens.len() {
                return Err(UiError::msg("unclosed '{' in adl file"));
            }
            fields.push(self.parse_field()?);
        }
        self.bump();
        Ok(Node { name, fields })
    }

    fn parse_field(&mut self) -> Result<Field, UiError> {
        let name = self.expect_name()?;
        if self.at(Tok::Eq) {
            self.bump();
            let value = match self.bump().cloned() {
                Some(Tok::String(s) | Tok::Ident(s) | Tok::Number(s)) => Value::Scalar(s),
                other => {
                    return Err(UiError::msg(format!(
                        "expected value after {name}, got {other:?}"
                    )))
                }
            };
            return Ok(Field { name, value });
        }
        if self.at(Tok::LBrace) {
            self.bump(); // consume '{'
            let mut fields = Vec::new();
            while !self.at(Tok::RBrace) {
                if self.pos >= self.tokens.len() {
                    return Err(UiError::msg("unclosed group in adl file"));
                }
                fields.push(self.parse_field()?);
            }
            self.bump();
            return Ok(Field {
                name: name.clone(),
                value: Value::Group(Node { name, fields }),
            });
        }
        Err(UiError::msg(format!("expected '=' or '{{' after {name}")))
    }

    fn expect_name(&mut self) -> Result<String, UiError> {
        let mut name = match self.bump().cloned() {
            Some(Tok::String(s) | Tok::Ident(s)) => s,
            other => {
                return Err(UiError::msg(format!(
                    "expected a name at token {}, got {other:?}, context {:?}",
                    self.pos,
                    &self.tokens[self.pos.saturating_sub(4)..self.tokens.len().min(self.pos + 4)]
                )))
            }
        };
        // MEDM object names such as "falling line" and "related display" are
        // several words. A name ends at '=' or '{'.
        while let Some(Tok::Ident(extra)) = self.tokens.get(self.pos).cloned() {
            name.push(' ');
            name.push_str(&extra);
            self.pos += 1;
        }
        Ok(name)
    }

    fn expect_lbrace(&mut self) -> Result<(), UiError> {
        if self.at(Tok::LBrace) {
            self.bump();
            Ok(())
        } else {
            Err(UiError::msg("expected '{'"))
        }
    }

    fn at(&self, kind: Tok) -> bool {
        matches!(
            (self.tokens.get(self.pos), kind),
            (Some(Tok::LBrace), Tok::LBrace)
                | (Some(Tok::RBrace), Tok::RBrace)
                | (Some(Tok::Eq), Tok::Eq)
        )
    }

    fn bump(&mut self) -> Option<&Tok> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    String(String),
    Number(String),
    LBrace,
    RBrace,
    Eq,
}

fn tokenize(text: &str) -> Result<Vec<Tok>, UiError> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() || c == b',' {
            i += 1;
            continue;
        }
        if c == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'{' {
            out.push(Tok::LBrace);
            i += 1;
            continue;
        }
        if c == b'}' {
            out.push(Tok::RBrace);
            i += 1;
            continue;
        }
        if c == b'=' {
            out.push(Tok::Eq);
            i += 1;
            continue;
        }
        if c == b'"' {
            i += 1;
            let start = i;
            let mut s = String::new();
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' && i + 1 < b.len() {
                    s.push(b[i + 1] as char);
                    i += 2;
                    continue;
                }
                s.push(b[i] as char);
                i += 1;
            }
            if i >= b.len() {
                return Err(UiError::msg(format!("unterminated string near {start}")));
            }
            i += 1;
            out.push(Tok::String(s));
            continue;
        }
        if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Number(text[start..i].to_string()));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Ident(text[start..i].to_string()));
            continue;
        }
        // Parentheses in point lists are skipped; the digits become numbers.
        if c == b'(' || c == b')' {
            i += 1;
            continue;
        }
        return Err(UiError::msg(format!(
            "unexpected character '{}' in adl",
            c as char
        )));
    }
    Ok(out)
}
