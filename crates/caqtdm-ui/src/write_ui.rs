//! Write a `.ui` document from the in-memory display.
//!
//! Qt Designer is not in the upstream tree, so this is not Designer's byte
//! layout. Property order is sorted and indentation is fixed. Parsing the
//! result yields the same widgets, layouts, and values as parsing the file
//! Designer saved.

use caqtdm_core::PropValue;

use crate::model::{CustomWidget, DisplayFile, LayoutItem, LayoutNode, WidgetNode};

pub fn write_ui(display: &DisplayFile) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let version = if display.version.is_empty() {
        "4.0"
    } else {
        display.version.as_str()
    };
    out.push_str(&format!("<ui version=\"{version}\">\n"));
    if !display.class_name.is_empty() {
        out.push_str(&format!(
            " <class>{}</class>\n",
            escape(&display.class_name)
        ));
    }
    write_widget(&mut out, &display.root, 1);
    if !display.custom_widgets.is_empty() {
        out.push_str(" <customwidgets>\n");
        for widget in &display.custom_widgets {
            write_custom(&mut out, widget);
        }
        out.push_str(" </customwidgets>\n");
    }
    out.push_str("</ui>\n");
    out
}

fn write_custom(out: &mut String, widget: &CustomWidget) {
    out.push_str("  <customwidget>\n");
    out.push_str(&format!(
        "   <class>{}</class>\n",
        escape(&widget.class_name)
    ));
    out.push_str(&format!(
        "   <extends>{}</extends>\n",
        escape(&widget.extends)
    ));
    out.push_str(&format!("   <header>{}</header>\n", escape(&widget.header)));
    out.push_str("  </customwidget>\n");
}

fn write_widget(out: &mut String, node: &WidgetNode, depth: usize) {
    let pad = " ".repeat(depth);
    out.push_str(&format!(
        "{pad}<widget class=\"{}\" name=\"{}\">\n",
        escape(&node.class_name),
        escape(&node.name)
    ));
    for (name, value) in &node.properties {
        write_property(out, depth + 1, name, value);
    }
    for (name, value) in &node.attributes {
        let inner = " ".repeat(depth + 1);
        out.push_str(&format!(
            "{inner}<attribute name=\"{}\">\n{inner} <string>{}</string>\n{inner}</attribute>\n",
            escape(name),
            escape(value)
        ));
    }
    for child in &node.children {
        write_widget(out, child, depth + 1);
    }
    if let Some(layout) = &node.layout {
        write_layout(out, layout, depth + 1);
    }
    out.push_str(&format!("{pad}</widget>\n"));
}

fn write_layout(out: &mut String, layout: &LayoutNode, depth: usize) {
    let pad = " ".repeat(depth);
    out.push_str(&format!(
        "{pad}<layout class=\"{}\" name=\"{}\">\n",
        escape(&layout.class_name),
        escape(&layout.name)
    ));
    for (name, value) in &layout.properties {
        write_property(out, depth + 1, name, value);
    }
    for item in &layout.items {
        write_item(out, item, depth + 1);
    }
    out.push_str(&format!("{pad}</layout>\n"));
}

fn write_item(out: &mut String, item: &LayoutItem, depth: usize) {
    let pad = " ".repeat(depth);
    let mut attrs = String::new();
    if let Some(row) = item.row {
        attrs.push_str(&format!(" row=\"{row}\""));
    }
    if let Some(column) = item.column {
        attrs.push_str(&format!(" column=\"{column}\""));
    }
    if let Some(span) = item.row_span {
        attrs.push_str(&format!(" rowspan=\"{span}\""));
    }
    if let Some(span) = item.col_span {
        attrs.push_str(&format!(" colspan=\"{span}\""));
    }
    out.push_str(&format!("{pad}<item{attrs}>\n"));
    if let Some(widget) = &item.widget {
        write_widget(out, widget, depth + 1);
    }
    if let Some(layout) = &item.layout {
        write_layout(out, layout, depth + 1);
    }
    out.push_str(&format!("{pad}</item>\n"));
}

fn write_property(out: &mut String, depth: usize, name: &str, value: &PropValue) {
    let pad = " ".repeat(depth);
    out.push_str(&format!("{pad}<property name=\"{}\">\n", escape(name)));
    write_value(out, depth + 1, value);
    out.push_str(&format!("{pad}</property>\n"));
}

fn write_value(out: &mut String, depth: usize, value: &PropValue) {
    let pad = " ".repeat(depth);
    match value {
        PropValue::String(text) => {
            out.push_str(&format!("{pad}<string>{}</string>\n", escape(text)));
        }
        PropValue::Enum(text) => {
            out.push_str(&format!("{pad}<enum>{}</enum>\n", escape(text)));
        }
        PropValue::Set(text) => {
            out.push_str(&format!("{pad}<set>{}</set>\n", escape(text)));
        }
        PropValue::Bool(flag) => {
            out.push_str(&format!(
                "{pad}<bool>{}</bool>\n",
                if *flag { "true" } else { "false" }
            ));
        }
        PropValue::Number(number) => {
            out.push_str(&format!("{pad}<number>{number}</number>\n"));
        }
        PropValue::Double(number) => {
            out.push_str(&format!(
                "{pad}<double>{}</double>\n",
                format_double(*number)
            ));
        }
        PropValue::StringList(items) => {
            out.push_str(&format!("{pad}<stringlist>\n"));
            for item in items {
                out.push_str(&format!("{pad} <string>{}</string>\n", escape(item)));
            }
            out.push_str(&format!("{pad}</stringlist>\n"));
        }
        PropValue::Color(color) => {
            out.push_str(&format!("{pad}<color>\n"));
            out.push_str(&format!("{pad} <red>{}</red>\n", color.r));
            out.push_str(&format!("{pad} <green>{}</green>\n", color.g));
            out.push_str(&format!("{pad} <blue>{}</blue>\n", color.b));
            out.push_str(&format!("{pad} <alpha>{}</alpha>\n", color.a));
            out.push_str(&format!("{pad}</color>\n"));
        }
        PropValue::Rect(rect) => {
            out.push_str(&format!("{pad}<rect>\n"));
            out.push_str(&format!("{pad} <x>{}</x>\n", rect.x));
            out.push_str(&format!("{pad} <y>{}</y>\n", rect.y));
            out.push_str(&format!("{pad} <width>{}</width>\n", rect.width));
            out.push_str(&format!("{pad} <height>{}</height>\n", rect.height));
            out.push_str(&format!("{pad}</rect>\n"));
        }
        PropValue::Size(size) => {
            out.push_str(&format!("{pad}<size>\n"));
            out.push_str(&format!("{pad} <width>{}</width>\n", size.width));
            out.push_str(&format!("{pad} <height>{}</height>\n", size.height));
            out.push_str(&format!("{pad}</size>\n"));
        }
        PropValue::Point(point) => {
            out.push_str(&format!("{pad}<point>\n"));
            out.push_str(&format!("{pad} <x>{}</x>\n", point.x));
            out.push_str(&format!("{pad} <y>{}</y>\n", point.y));
            out.push_str(&format!("{pad}</point>\n"));
        }
        PropValue::Font(font) => {
            out.push_str(&format!("{pad}<font>\n"));
            out.push_str(&format!(
                "{pad} <family>{}</family>\n",
                escape(&font.family)
            ));
            out.push_str(&format!(
                "{pad} <pointsize>{}</pointsize>\n",
                font.point_size
            ));
            out.push_str(&format!(
                "{pad} <bold>{}</bold>\n",
                if font.bold { "true" } else { "false" }
            ));
            out.push_str(&format!(
                "{pad} <italic>{}</italic>\n",
                if font.italic { "true" } else { "false" }
            ));
            out.push_str(&format!("{pad}</font>\n"));
        }
    }
}

fn format_double(number: f64) -> String {
    if number.is_finite() && number == number.round() && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_xml::parse_ui;
    use std::path::Path;

    #[test]
    fn upstream_ui_files_round_trip_through_the_model() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../upstream");
        let mut files = Vec::new();
        walk(&root, &mut files);
        assert!(files.len() > 10, "upstream ui files are present");
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|_| String::new());
            let Ok(original) = parse_ui(&text) else {
                continue;
            };
            let written = write_ui(&original);
            let again = parse_ui(&written).unwrap_or_else(|err| {
                panic!("{} did not parse after write: {err}", path.display())
            });
            assert_eq!(
                again,
                original,
                "{} normalized tree differs from the upstream file",
                path.display()
            );
            let twice = parse_ui(&write_ui(&again)).unwrap();
            assert_eq!(twice, again);
        }
    }

    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("ui") {
                out.push(path);
            }
        }
    }
}
