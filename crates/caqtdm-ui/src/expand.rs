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
    let items = instance_items(node);
    // `HandleWidget` loops `qMax(macroList.count(), getItemCount())` times.
    let count = node
        .prop_f64("numberOfItems")
        .unwrap_or(1.0)
        .max(1.0)
        .max(items.len() as f64) as usize;
    let layout = IncludeLayout::from_node(node);
    let base = node.geometry_prop().unwrap_or(Rect::new(0, 0, 100, 40));
    let item_size = root_size(&loaded.root);

    let mut frame = WidgetNode::new("caFrame", node.name.clone());
    frame.properties = node.properties.clone();
    frame.rect = node.rect;

    let mut nested = search.clone();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            nested.origin = Some(parent.to_path_buf());
        }
    }
    let mut extent = (1, 1);
    let mut cells = (0, 0);
    for index in 0..count {
        let item = items.get(index).cloned().unwrap_or_default();
        let mut inst_macros = macros.clone();
        inst_macros.extend_from(&parse_macro_list(&item.macros));
        let (x, y) = layout.place(index, item_size, &item);
        extent.0 = extent.0.max(x + item_size.0);
        extent.1 = extent.1.max(y + item_size.1);
        cells = layout.cells(index + 1);
        // The loaded window keeps its own class, name, and stylesheet. Its
        // central widget fills it, so a `QWidget#centralWidget` rule applies.
        let mut copy = loaded.root.clone();
        copy.set_prop(
            "geometry",
            PropValue::Rect(Rect::new(x, y, item_size.0, item_size.1)),
        );
        if let Err(err) = expand_node(&mut copy, &nested, &inst_macros, stack, warnings) {
            stack.pop();
            return Err(err);
        }
        frame.children.push(copy);
    }
    stack.pop();
    if layout.adjust {
        // `includeWidget->resize(...)` after the loop.
        let margin = layout.margin;
        let (width, height) = if layout.positions {
            (extent.0 + margin, extent.1 + margin)
        } else {
            let (columns, rows) = (cells.0.max(1), cells.1.max(1));
            (
                columns * item_size.0 + (columns - 1) * layout.spacing_h + margin,
                rows * item_size.1 + (rows - 1) * layout.spacing_v + margin,
            )
        };
        frame.set_prop(
            "geometry",
            PropValue::Rect(Rect::new(base.x, base.y, width, height)),
        );
    }
    Ok(frame)
}

/// Size of an included file's top-level widget: its `geometry` or `size`.
fn root_size(root: &WidgetNode) -> (i32, i32) {
    if let Some(g) = root.geometry_prop() {
        return (g.width.max(1), g.height.max(1));
    }
    if let Some(PropValue::Size(s)) = root.prop("size") {
        return (s.width.max(1), s.height.max(1));
    }
    (100, 40)
}

/// Stacking rules of `caInclude`, as `HandleWidget` fills the grid layout.
///
/// `Row` adds item `j` at grid row `j` (a vertical stack). `Column` adds it at
/// column `j` (a horizontal run). `RowColumn` fills `maximumLines` rows per
/// column, `ColumnRow` fills `maximumColumns` columns per row. `Positions`
/// moves each copy to the `[x,y]` pair carried in its macro, scaled by the
/// correction factors, offset by half the margin.
struct IncludeLayout {
    stacking: String,
    positions: bool,
    adjust: bool,
    max_lines: i32,
    max_columns: i32,
    spacing_h: i32,
    spacing_v: i32,
    margin: i32,
    x_factor: f64,
    y_factor: f64,
}

impl IncludeLayout {
    fn from_node(node: &WidgetNode) -> Self {
        let stacking = node
            .enum_tail("stacking")
            .unwrap_or("Row")
            .to_ascii_lowercase();
        Self {
            positions: stacking == "positions",
            stacking,
            adjust: node.prop_bool("adjustSizeToContents").unwrap_or(true),
            max_lines: node.prop_f64("maximumLines").unwrap_or(1.0).max(1.0) as i32,
            max_columns: node.prop_f64("maximumColumns").unwrap_or(1.0).max(1.0) as i32,
            spacing_h: node.prop_f64("horizontalSpacing").unwrap_or(0.0).max(0.0) as i32,
            spacing_v: node.prop_f64("verticalSpacing").unwrap_or(0.0).max(0.0) as i32,
            margin: node.prop_f64("margin").unwrap_or(0.0).max(0.0) as i32,
            x_factor: node.prop_f64("xCorrectionFactor").unwrap_or(1.0),
            y_factor: node.prop_f64("yCorrectionFactor").unwrap_or(1.0),
        }
    }

    /// Grid cell (column, row) of item `index`.
    fn cell(&self, index: usize) -> (i32, i32) {
        let i = index as i32;
        match self.stacking.as_str() {
            "column" => (i, 0),
            "rowcolumn" => (i / self.max_lines, i % self.max_lines),
            "columnrow" => (i % self.max_columns, i / self.max_columns),
            _ => (0, i),
        }
    }

    /// Columns and rows used by the first `count` items.
    fn cells(&self, count: usize) -> (i32, i32) {
        let mut columns = 0;
        let mut rows = 0;
        for index in 0..count {
            let (column, row) = self.cell(index);
            columns = columns.max(column + 1);
            rows = rows.max(row + 1);
        }
        (columns, rows)
    }

    fn place(&self, index: usize, size: (i32, i32), item: &IncludeItem) -> (i32, i32) {
        if self.positions {
            let x = (f64::from(item.x) * self.x_factor).round() as i32;
            let y = (f64::from(item.y) * self.y_factor).round() as i32;
            return (x + self.margin / 2, y + self.margin / 2);
        }
        let (column, row) = self.cell(index);
        (
            column * (size.0 + self.spacing_h),
            row * (size.1 + self.spacing_v),
        )
    }
}

/// One entry of the `macro` list: its macro text and, for `Positions`
/// stacking, the first `[x,y]` pair. A pair that names a channel instead of
/// a number starts at 0; `caInclude::setMacro` keeps only the first pair.
#[derive(Debug, Clone, Default)]
struct IncludeItem {
    macros: String,
    x: i32,
    y: i32,
}

fn instance_items(node: &WidgetNode) -> Vec<IncludeItem> {
    let raw = node
        .prop_str("macro")
        .or_else(|| node.prop_str("macrolist"))
        .unwrap_or("");
    raw.split(';')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(parse_include_item)
        .collect()
}

fn parse_include_item(piece: &str) -> IncludeItem {
    let mut item = IncludeItem {
        macros: piece.to_string(),
        x: 0,
        y: 0,
    };
    let Some(open) = piece.find('[') else {
        return item;
    };
    let Some(close) = piece[open..].find(']').map(|i| open + i) else {
        return item;
    };
    let inner = &piece[open + 1..close];
    let mut parts = inner.split(',').map(str::trim);
    let x = parts.next().unwrap_or("");
    let y = parts.next().unwrap_or("");
    item.x = x.parse().unwrap_or(0);
    item.y = y.parse().unwrap_or(0);
    let mut rest = String::new();
    rest.push_str(piece[..open].trim_end_matches([',', ' ']));
    let tail = piece[close + 1..].trim_start_matches([',', ' ']);
    if !tail.is_empty() {
        if !rest.is_empty() {
            rest.push(',');
        }
        rest.push_str(tail);
    }
    item.macros = rest;
    item
}

fn empty_include(node: &WidgetNode) -> WidgetNode {
    let mut frame = WidgetNode::new("caFrame", node.name.clone());
    frame.properties = node.properties.clone();
    frame.rect = node.rect;
    frame
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
