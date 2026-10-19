use caqtdm_core::{apply_macros, parse_macro_list, MacroMap, PropValue, Rect};

use crate::error::UiError;
use crate::load_path;
use crate::model::{assign_geometry, WidgetNode};
use crate::search::SearchPath;

/// `CAQTDM_MAX_INCLUDE_LEVEL` is 50 and a load is refused once `level` reaches 49.
const MAX_INCLUDE_DEPTH: usize = 49;

/// Substitute macros and inline `caInclude` widgets.
///
/// A missing include is a warning, not a failure. Upstream posts
/// `sorry, could not load include file` and still opens the parent.
pub fn expand_includes(
    root: &mut WidgetNode,
    search: &SearchPath,
    macros: &MacroMap,
) -> Result<Vec<String>, UiError> {
    let mut warnings = Vec::new();
    let mut stack = Vec::new();
    expand_node(root, search, macros, &mut stack, &mut warnings)?;
    assign_geometry(root);
    Ok(warnings)
}

fn expand_node(
    node: &mut WidgetNode,
    search: &SearchPath,
    macros: &MacroMap,
    stack: &mut Vec<std::path::PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), UiError> {
    substitute_node(node, macros);
    let mut local = macros.clone();
    if let Some(extra) = node.prop_str("macro").or_else(|| node.prop_str("macros")) {
        local.extend_from(&parse_macro_list(extra));
    }

    if node.class_name == "caInclude" {
        *node = expand_include(node, search, &local, stack, warnings)?;
        return Ok(());
    }

    for child in &mut node.children {
        expand_node(child, search, &local, stack, warnings)?;
    }
    if let Some(layout) = node.layout.as_mut() {
        for item in &mut layout.items {
            if let Some(widget) = item.widget.as_mut() {
                expand_node(widget, search, &local, stack, warnings)?;
            }
        }
    }
    Ok(())
}

fn expand_include(
    node: &WidgetNode,
    search: &SearchPath,
    macros: &MacroMap,
    stack: &mut Vec<std::path::PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<WidgetNode, UiError> {
    let filename = node.prop_str("filename").unwrap_or("").trim().to_string();
    let Some(path) = search.resolve_include(&filename) else {
        warnings.push(format!("sorry, could not load include file {filename}"));
        return Ok(empty_include(node));
    };
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
    if stack.len() >= MAX_INCLUDE_DEPTH || stack.iter().any(|seen| seen == &canonical) {
        warnings.push(format!("sorry, could not load include file {filename}"));
        return Ok(empty_include(node));
    }
    stack.push(canonical);
    let loaded = match load_path(&path) {
        Ok(file) => file,
        Err(err) => {
            stack.pop();
            return Err(err);
        }
    };
    let count = node.prop_f64("numberOfItems").unwrap_or(1.0).max(1.0) as usize;
    let instance_macros = instance_macro_list(node, count);
    let stacking = node.enum_tail("stacking").unwrap_or("Row").to_string();
    let max_lines = node
        .prop_f64("maximumLines")
        .unwrap_or(count as f64)
        .max(1.0) as i32;
    let base = node.geometry_prop().unwrap_or(Rect::new(0, 0, 100, 40));

    let mut frame = WidgetNode::new("caFrame", node.name.clone());
    frame.properties = node.properties.clone();
    frame.rect = node.rect;

    let inner = loaded.root;
    // Use the central widget's children when the included file is a main window.
    let source_children = if inner.class_name == "QMainWindow" {
        inner
            .children
            .iter()
            .find(|c| c.name == "centralwidget")
            .map(|c| c.children.clone())
            .unwrap_or(inner.children.clone())
    } else {
        vec![inner.clone()]
    };

    let mut nested = search.clone();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            nested.origin = Some(parent.to_path_buf());
        }
    }
    for index in 0..count {
        let mut inst_macros = macros.clone();
        if let Some(extra) = instance_macros.get(index) {
            inst_macros.extend_from(extra);
        }
        let (dx, dy) = stack_offset(index, count, &stacking, max_lines, base);
        for child in &source_children {
            let mut copy = child.clone();
            if let Err(err) = expand_node(&mut copy, &nested, &inst_macros, stack, warnings) {
                stack.pop();
                return Err(err);
            }
            shift_geometry(&mut copy, dx, dy);
            frame.children.push(copy);
        }
    }
    stack.pop();
    Ok(frame)
}

fn empty_include(node: &WidgetNode) -> WidgetNode {
    let mut frame = WidgetNode::new("caFrame", node.name.clone());
    frame.properties = node.properties.clone();
    frame.rect = node.rect;
    frame
}

fn instance_macro_list(node: &WidgetNode, count: usize) -> Vec<MacroMap> {
    let raw = node
        .prop_str("macro")
        .or_else(|| node.prop_str("macrolist"))
        .unwrap_or("");
    let pieces: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if pieces.len() <= 1 && !raw.contains(';') {
        return vec![parse_macro_list(raw); count];
    }
    pieces.into_iter().map(parse_macro_list).collect()
}

fn stack_offset(
    index: usize,
    count: usize,
    stacking: &str,
    max_lines: i32,
    base: Rect,
) -> (i32, i32) {
    let i = index as i32;
    let step_x = base.width.max(1);
    let step_y = base.height.max(1);
    match stacking.to_ascii_lowercase().as_str() {
        "column" | "vertical" => (0, i * step_y),
        "rowcolumn" | "rowcolumnwrap" => {
            let lines = max_lines.max(1);
            let col = i / lines;
            let row = i % lines;
            (col * step_x, row * step_y)
        }
        "positions" => (0, 0),
        _ => {
            let _ = count;
            (i * step_x, 0)
        }
    }
}

fn shift_geometry(node: &mut WidgetNode, dx: i32, dy: i32) {
    if let Some(PropValue::Rect(rect)) = node.properties.get_mut("geometry") {
        rect.x += dx;
        rect.y += dy;
    }
    for child in &mut node.children {
        shift_geometry(child, dx, dy);
    }
}

fn substitute_node(node: &mut WidgetNode, macros: &MacroMap) {
    for value in node.properties.values_mut() {
        substitute_value(value, macros);
    }
    for child in &mut node.children {
        substitute_node(child, macros);
    }
    if let Some(layout) = node.layout.as_mut() {
        for item in &mut layout.items {
            if let Some(widget) = item.widget.as_mut() {
                substitute_node(widget, macros);
            }
        }
    }
}

fn substitute_value(value: &mut PropValue, macros: &MacroMap) {
    match value {
        PropValue::String(s) | PropValue::Enum(s) | PropValue::Set(s) => {
            *s = apply_macros(s, macros);
        }
        PropValue::StringList(list) => {
            for item in list {
                *item = apply_macros(item, macros);
            }
        }
        _ => {}
    }
}
