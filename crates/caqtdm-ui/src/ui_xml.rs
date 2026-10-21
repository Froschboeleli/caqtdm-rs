use std::collections::BTreeMap;

use caqtdm_core::{Color, FontSpec, Point, PropValue, Rect, Size};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::error::UiError;
use crate::model::{CustomWidget, DisplayFile, LayoutItem, LayoutNode, SourceFormat, WidgetNode};

pub fn parse_ui(text: &str) -> Result<DisplayFile, UiError> {
    let mut reader = Reader::from_str(text);
    // Qt keeps the whitespace inside `<string>` values: " Title " pads the
    // text on purpose. Whitespace-only nodes between tags are skipped below.
    reader.config_mut().trim_text(false);
    let mut version = String::new();
    let mut class_name = String::new();
    let mut root: Option<WidgetNode> = None;
    let mut custom = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(&e);
                match name.as_str() {
                    "ui" => {
                        if let Some(v) = attr(&e, "version") {
                            version = v;
                        }
                    }
                    "class" => class_name = read_text(&mut reader)?.trim().to_string(),
                    "widget" => {
                        root = Some(read_widget(&mut reader, &e)?);
                    }
                    "customwidgets" => custom = read_custom_widgets(&mut reader)?,
                    _ => skip_rest(&mut reader)?,
                }
            }
            Ok(Event::Empty(e)) => {
                if local_name(&e) == "class" {
                    class_name = attr(&e, "name").unwrap_or_default();
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }

    let root = root.ok_or_else(|| UiError::msg("ui file has no root widget"))?;
    Ok(DisplayFile {
        version,
        class_name,
        root,
        custom_widgets: custom,
        source: SourceFormat::Ui,
        path: None,
    })
}

fn read_widget(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<WidgetNode, UiError> {
    let mut node = WidgetNode::new(
        attr(start, "class").unwrap_or_else(|| "QWidget".to_string()),
        attr(start, "name").unwrap_or_else(|| "widget".to_string()),
    );
    let mut zorder: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(&e).as_str() {
                "property" => {
                    let pname = attr(&e, "name").unwrap_or_default();
                    if let Some(value) = read_property_value(reader)? {
                        node.properties.insert(pname, value);
                    }
                }
                "attribute" => {
                    let aname = attr(&e, "name").unwrap_or_default();
                    let value = read_text_deep(reader)?;
                    node.attributes.insert(aname, value);
                }
                "widget" => node.children.push(read_widget(reader, &e)?),
                "layout" => node.layout = Some(read_layout(reader, &e)?),
                "item" => skip_rest(reader)?,
                "zorder" => zorder.push(read_text_deep(reader)?.trim().to_string()),
                _ => skip_rest(reader)?,
            },
            Ok(Event::Empty(e)) => {
                if local_name(&e) == "widget" {
                    node.children.push(WidgetNode::new(
                        attr(&e, "class").unwrap_or_else(|| "QWidget".to_string()),
                        attr(&e, "name").unwrap_or_default(),
                    ));
                }
            }
            Ok(Event::End(e)) if local_name_end(&e) == "widget" => {
                apply_zorder(&mut node, &zorder);
                return Ok(node);
            }
            Ok(Event::Eof) => return Err(UiError::msg("unclosed widget")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_layout(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<LayoutNode, UiError> {
    let mut layout = LayoutNode {
        class_name: attr(start, "class").unwrap_or_else(|| "QVBoxLayout".to_string()),
        name: attr(start, "name").unwrap_or_default(),
        properties: BTreeMap::new(),
        items: Vec::new(),
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(&e).as_str() {
                "property" => {
                    let pname = attr(&e, "name").unwrap_or_default();
                    if let Some(value) = read_property_value(reader)? {
                        layout.properties.insert(pname, value);
                    }
                }
                "item" => layout.items.push(read_layout_item(reader, &e)?),
                _ => skip_rest(reader)?,
            },
            Ok(Event::End(e)) if local_name_end(&e) == "layout" => return Ok(layout),
            Ok(Event::Eof) => return Err(UiError::msg("unclosed layout")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_layout_item(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<LayoutItem, UiError> {
    let mut item = LayoutItem {
        row: attr(start, "row").and_then(|s| s.parse().ok()),
        column: attr(start, "column").and_then(|s| s.parse().ok()),
        row_span: attr(start, "rowspan").and_then(|s| s.parse().ok()),
        col_span: attr(start, "colspan").and_then(|s| s.parse().ok()),
        widget: None,
        layout: None,
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(&e).as_str() {
                "widget" => item.widget = Some(read_widget(reader, &e)?),
                "layout" => item.layout = Some(read_layout(reader, &e)?),
                _ => skip_rest(reader)?,
            },
            Ok(Event::End(e)) if local_name_end(&e) == "item" => return Ok(item),
            Ok(Event::Eof) => return Err(UiError::msg("unclosed layout item")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_property_value(reader: &mut Reader<&[u8]>) -> Result<Option<PropValue>, UiError> {
    let mut value = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                value = Some(read_typed(reader, &local_name(&e), &e)?);
            }
            Ok(Event::Empty(e)) => {
                value = Some(empty_typed(&local_name(&e)));
            }
            Ok(Event::End(e)) if local_name_end(&e) == "property" => return Ok(value),
            Ok(Event::Eof) => return Err(UiError::msg("unclosed property")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn empty_typed(kind: &str) -> PropValue {
    match kind {
        "string" | "cstring" | "enum" | "set" | "pixmap" | "url" | "cursorShape" => {
            PropValue::String(String::new())
        }
        "stringlist" => PropValue::StringList(Vec::new()),
        "bool" => PropValue::Bool(false),
        "number" | "int" | "uint" | "longlong" | "uLongLong" => PropValue::Number(0),
        "double" | "float" => PropValue::Double(0.0),
        _ => PropValue::String(String::new()),
    }
}

fn read_typed(
    reader: &mut Reader<&[u8]>,
    kind: &str,
    start: &BytesStart<'_>,
) -> Result<PropValue, UiError> {
    match kind {
        "string" | "cstring" | "pixmap" | "url" => Ok(PropValue::String(read_text(reader)?)),
        "enum" => Ok(PropValue::Enum(read_text(reader)?.trim().to_string())),
        "set" => Ok(PropValue::Set(read_text(reader)?.trim().to_string())),
        "bool" => {
            let t = read_text(reader)?;
            Ok(PropValue::Bool(
                t.trim().eq_ignore_ascii_case("true") || t.trim() == "1",
            ))
        }
        "number" | "int" | "uint" | "longlong" | "uLongLong" => {
            let t = read_text(reader)?;
            let n = t.trim().parse::<i64>().unwrap_or(0);
            Ok(PropValue::Number(n))
        }
        "double" | "float" => {
            let t = read_text(reader)?;
            let n = t.trim().parse::<f64>().unwrap_or(0.0);
            Ok(PropValue::Double(n))
        }
        "color" => {
            let map = read_leaf_map(reader, "color")?;
            let r = num(&map, "red");
            let g = num(&map, "green");
            let b = num(&map, "blue");
            // uic writes `<color alpha="0">`; the attribute is the alpha.
            let a = attr(start, "alpha")
                .or_else(|| map.get("alpha").cloned())
                .map(|s| s.parse().unwrap_or(255))
                .unwrap_or(255);
            Ok(PropValue::Color(Color::rgba(r, g, b, a)))
        }
        "rect" => {
            let map = read_leaf_map(reader, "rect")?;
            Ok(PropValue::Rect(Rect::new(
                num_i(&map, "x"),
                num_i(&map, "y"),
                num_i(&map, "width"),
                num_i(&map, "height"),
            )))
        }
        "size" => {
            let map = read_leaf_map(reader, "size")?;
            Ok(PropValue::Size(Size {
                width: num_i(&map, "width"),
                height: num_i(&map, "height"),
            }))
        }
        "point" => {
            let map = read_leaf_map(reader, "point")?;
            Ok(PropValue::Point(Point {
                x: num_i(&map, "x"),
                y: num_i(&map, "y"),
            }))
        }
        "font" => {
            let map = read_leaf_map(reader, "font")?;
            Ok(PropValue::Font(FontSpec {
                family: map
                    .get("family")
                    .cloned()
                    .unwrap_or_else(|| "Sans".to_string()),
                point_size: map
                    .get("pointsize")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                // `QFont::bold()` is true above `QFont::Medium` (57), which
                // is how uic reads `<weight>75</weight>` without `<bold>`.
                bold: map.get("bold").is_some_and(|s| s == "true" || s == "1")
                    || map
                        .get("weight")
                        .and_then(|s| s.parse::<i32>().ok())
                        .is_some_and(|weight| weight > 57),
                italic: map.get("italic").is_some_and(|s| s == "true" || s == "1"),
            }))
        }
        "stringlist" => {
            let mut items = Vec::new();
            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) if local_name(&e) == "string" => {
                        items.push(read_text(reader)?);
                    }
                    Ok(Event::Empty(e)) if local_name(&e) == "string" => items.push(String::new()),
                    Ok(Event::End(e)) if local_name_end(&e) == "stringlist" => {
                        return Ok(PropValue::StringList(items));
                    }
                    Ok(Event::Eof) => return Err(UiError::msg("unclosed stringlist")),
                    Err(err) => return Err(UiError::Xml(err.to_string())),
                    _ => {}
                }
            }
        }
        other => {
            let text = read_text_deep_named(reader, other)?;
            Ok(PropValue::String(text))
        }
    }
}

fn read_leaf_map(
    reader: &mut Reader<&[u8]>,
    end_tag: &str,
) -> Result<BTreeMap<String, String>, UiError> {
    let mut map = BTreeMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(&e);
                let text = read_text(reader)?;
                map.insert(name, text.trim().to_string());
            }
            Ok(Event::Empty(e)) => {
                map.insert(local_name(&e), String::new());
            }
            Ok(Event::End(e)) if local_name_end(&e) == end_tag => return Ok(map),
            Ok(Event::Eof) => return Err(UiError::msg(format!("unclosed {end_tag}"))),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_custom_widgets(reader: &mut Reader<&[u8]>) -> Result<Vec<CustomWidget>, UiError> {
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if local_name(&e) == "customwidget" => {
                out.push(read_one_custom(reader)?);
            }
            Ok(Event::End(e)) if local_name_end(&e) == "customwidgets" => return Ok(out),
            Ok(Event::Eof) => return Err(UiError::msg("unclosed customwidgets")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_one_custom(reader: &mut Reader<&[u8]>) -> Result<CustomWidget, UiError> {
    let mut class_name = String::new();
    let mut extends = String::new();
    let mut header = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(&e).as_str() {
                "class" => class_name = read_text(reader)?.trim().to_string(),
                "extends" => extends = read_text(reader)?.trim().to_string(),
                "header" => header = read_text(reader)?.trim().to_string(),
                _ => skip_rest(reader)?,
            },
            Ok(Event::End(e)) if local_name_end(&e) == "customwidget" => {
                return Ok(CustomWidget {
                    class_name,
                    extends,
                    header,
                });
            }
            Ok(Event::Eof) => return Err(UiError::msg("unclosed customwidget")),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_text(reader: &mut Reader<&[u8]>) -> Result<String, UiError> {
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(t)) => {
                text.push_str(&t.unescape().map_err(|e| UiError::Xml(e.to_string()))?);
            }
            Ok(Event::CData(t)) => {
                text.push_str(&String::from_utf8_lossy(&t));
            }
            Ok(Event::End(_)) => return Ok(text),
            Ok(Event::Start(_)) => skip_rest(reader)?,
            Ok(Event::Eof) => return Ok(text),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_text_deep(reader: &mut Reader<&[u8]>) -> Result<String, UiError> {
    let mut text = String::new();
    let mut depth = 1;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(text);
                }
            }
            Ok(Event::Text(t)) => {
                let piece = t.unescape().unwrap_or_default();
                if !piece.trim().is_empty() {
                    text.push_str(&piece);
                }
            }
            Ok(Event::Eof) => return Ok(text),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn read_text_deep_named(reader: &mut Reader<&[u8]>, _end: &str) -> Result<String, UiError> {
    // The start tag was already consumed, so the matching end is depth 1.
    read_text(reader)
}

fn skip_rest(reader: &mut Reader<&[u8]>) -> Result<(), UiError> {
    let mut depth = 1;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            Ok(Event::Eof) => return Ok(()),
            Err(err) => return Err(UiError::Xml(err.to_string())),
            _ => {}
        }
    }
}

fn local_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn local_name_end(e: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn attr(e: &BytesStart<'_>, key: &str) -> Option<String> {
    e.try_get_attribute(key)
        .ok()
        .flatten()
        .and_then(|a| a.unescape_value().ok().map(|c| c.into_owned()))
}

fn num(map: &BTreeMap<String, String>, key: &str) -> u8 {
    map.get(key).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn num_i(map: &BTreeMap<String, String>, key: &str) -> i32 {
    map.get(key).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// `uic` raises each child named in a `<zorder>` element in turn, so the
/// last name ends on top. Children that are not named keep their file order
/// underneath.
fn apply_zorder(node: &mut WidgetNode, zorder: &[String]) {
    if zorder.is_empty() {
        return;
    }
    let mut children = std::mem::take(&mut node.children);
    let mut raised = Vec::new();
    for name in zorder {
        if let Some(pos) = children.iter().position(|child| &child.name == name) {
            raised.push(children.remove(pos));
        } else if let Some(pos) = raised.iter().position(|child| &child.name == name) {
            let child = raised.remove(pos);
            raised.push(child);
        }
    }
    children.extend(raised);
    node.children = children;
}
