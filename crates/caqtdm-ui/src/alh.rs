//! Alarm-handler configuration. caQtDM shows these files in `caAlarmTree`.
//! Group nesting follows leading indentation: a deeper line belongs to the
//! group above it.

use caqtdm_core::{PropValue, Rect};

use crate::model::{DisplayFile, SourceFormat, WidgetNode};

pub fn parse_alh(text: &str) -> Result<DisplayFile, crate::error::UiError> {
    let mut root = WidgetNode::new("caAlarmTree", "alarmTree");
    root.set_prop("geometry", PropValue::Rect(Rect::new(0, 0, 640, 480)));
    // Stack of (indent, index path into the tree). The root is indent -1.
    let mut stack: Vec<(i32, Vec<usize>)> = vec![(-1, Vec::new())];
    let mut pending_section: Option<(Vec<usize>, String, String)> = None;

    for raw in text.lines() {
        let indent = raw.chars().take_while(|c| *c == ' ' || *c == '\t').count() as i32;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('$') {
            if name == "END" {
                if let Some((path, key, buf)) = pending_section.take() {
                    if let Some(node) = node_at(&mut root, &path) {
                        node.set_prop(key, PropValue::String(buf.trim().to_string()));
                    }
                }
            } else if pending_section.is_none() {
                let path = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
                let key = match name {
                    "GUIDANCE" => "guidance",
                    "COMMAND" => "command",
                    "SEVRCOMMAND" => "sevrCommand",
                    _ => "note",
                };
                pending_section = Some((path, key.to_string(), String::new()));
            }
            continue;
        }
        if let Some((_, _, buf)) = pending_section.as_mut() {
            buf.push_str(line);
            buf.push('\n');
            continue;
        }

        let mut parts = line.split_whitespace();
        let keyword = parts.next().unwrap_or("");
        match keyword {
            "GROUP" => {
                while stack.last().is_some_and(|(d, _)| *d >= indent) {
                    stack.pop();
                }
                let mask = parts.next().unwrap_or("NULL");
                let name = parts.next().unwrap_or("group");
                let mut node = WidgetNode::new("caFrame", name);
                node.set_prop("text", PropValue::String(name.to_string()));
                node.set_prop("mask", PropValue::String(mask.to_string()));
                node.set_prop(
                    "geometry",
                    PropValue::Rect(Rect::new(indent * 8, 0, 240, 24)),
                );
                let parent_path = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
                let index = push_child(&mut root, &parent_path, node);
                let mut path = parent_path;
                path.push(index);
                stack.push((indent, path));
            }
            "CHANNEL" => {
                while stack.last().is_some_and(|(d, _)| *d > indent) {
                    stack.pop();
                }
                let name = parts.next().unwrap_or("channel");
                let pv = parts.next().unwrap_or("");
                let mask = parts.next().unwrap_or("");
                let mut node = WidgetNode::new("caLineEdit", name);
                node.set_prop("channel", PropValue::String(pv.to_string()));
                node.set_prop("text", PropValue::String(name.to_string()));
                node.set_prop("mask", PropValue::String(mask.to_string()));
                node.set_prop("colorMode", PropValue::Enum("Alarm".into()));
                node.set_prop(
                    "geometry",
                    PropValue::Rect(Rect::new(indent * 8, 0, 220, 22)),
                );
                let parent_path = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
                push_child(&mut root, &parent_path, node);
            }
            _ => {}
        }
    }
    Ok(DisplayFile {
        version: "alh".to_string(),
        class_name: "AlarmHandler".to_string(),
        root,
        custom_widgets: Vec::new(),
        source: SourceFormat::Alh,
        path: None,
    })
}

fn push_child(root: &mut WidgetNode, path: &[usize], node: WidgetNode) -> usize {
    let parent = node_at(root, path).expect("parent path");
    parent.children.push(node);
    parent.children.len() - 1
}

fn node_at<'a>(root: &'a mut WidgetNode, path: &[usize]) -> Option<&'a mut WidgetNode> {
    let mut current = root;
    for index in path {
        current = current.children.get_mut(*index)?;
    }
    Some(current)
}
