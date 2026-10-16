use std::collections::BTreeMap;
use std::path::PathBuf;

use caqtdm_core::{PropValue, Rect};

/// Which reader produced a display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Ui,
    Adl,
    Edl,
    Alh,
    Prc,
}

/// One widget, or a plain Qt container, after parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetNode {
    pub class_name: String,
    pub name: String,
    /// File coordinates, relative to the parent. [`assign_geometry`] also
    /// fills [`Self::rect`] with the absolute position used for painting.
    pub properties: BTreeMap<String, PropValue>,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<WidgetNode>,
    pub layout: Option<LayoutNode>,
    pub rect: Rect,
}

impl WidgetNode {
    pub fn new(class_name: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            name: name.into(),
            properties: BTreeMap::new(),
            attributes: BTreeMap::new(),
            children: Vec::new(),
            layout: None,
            rect: Rect::default(),
        }
    }

    pub fn prop(&self, name: &str) -> Option<&PropValue> {
        self.properties.get(name)
    }

    pub fn prop_str(&self, name: &str) -> Option<&str> {
        self.prop(name).and_then(PropValue::as_str)
    }

    pub fn prop_f64(&self, name: &str) -> Option<f64> {
        self.prop(name).and_then(PropValue::as_f64)
    }

    pub fn prop_bool(&self, name: &str) -> Option<bool> {
        self.prop(name).and_then(PropValue::as_bool)
    }

    pub fn enum_tail(&self, name: &str) -> Option<&str> {
        self.prop(name).and_then(PropValue::enum_tail)
    }

    pub fn set_prop(&mut self, name: impl Into<String>, value: PropValue) {
        self.properties.insert(name.into(), value);
    }

    pub fn geometry_prop(&self) -> Option<Rect> {
        match self.prop("geometry") {
            Some(PropValue::Rect(r)) => Some(*r),
            _ => None,
        }
    }

    /// Walk every widget, including widgets nested in layouts.
    pub fn walk<'a>(&'a self, visit: &mut dyn FnMut(&'a WidgetNode)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
        if let Some(layout) = &self.layout {
            layout.walk(visit);
        }
    }

    pub fn count_class(&self, class_name: &str) -> usize {
        let mut n = 0;
        self.walk(&mut |w| {
            if w.class_name == class_name {
                n += 1;
            }
        });
        n
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub class_name: String,
    pub name: String,
    pub properties: BTreeMap<String, PropValue>,
    pub items: Vec<LayoutItem>,
}

impl LayoutNode {
    fn walk<'a>(&'a self, visit: &mut dyn FnMut(&'a WidgetNode)) {
        for item in &self.items {
            if let Some(widget) = &item.widget {
                widget.walk(visit);
            }
            if let Some(layout) = &item.layout {
                layout.walk(visit);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutItem {
    pub row: Option<i32>,
    pub column: Option<i32>,
    pub row_span: Option<i32>,
    pub col_span: Option<i32>,
    pub widget: Option<WidgetNode>,
    pub layout: Option<LayoutNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomWidget {
    pub class_name: String,
    pub extends: String,
    pub header: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayFile {
    pub version: String,
    pub class_name: String,
    pub root: WidgetNode,
    pub custom_widgets: Vec<CustomWidget>,
    pub source: SourceFormat,
    pub path: Option<PathBuf>,
}

impl DisplayFile {
    pub fn widget_count(&self) -> usize {
        let mut n = 0;
        self.root.walk(&mut |_| n += 1);
        n
    }
}

/// Fill [`WidgetNode::rect`] with absolute coordinates.
pub fn assign_geometry(root: &mut WidgetNode) {
    let (w, h) = root_size(root);
    root.rect = Rect::new(0, 0, w, h);
    place_children(root);
}

fn root_size(root: &WidgetNode) -> (i32, i32) {
    if let Some(g) = root.geometry_prop() {
        return (g.width.max(1), g.height.max(1));
    }
    if let Some(PropValue::Size(s)) = root.prop("size") {
        return (s.width.max(1), s.height.max(1));
    }
    (800, 600)
}

fn place_children(parent: &mut WidgetNode) {
    let parent_rect = parent.rect;
    let is_window = parent.class_name == "QMainWindow" || parent.class_name == "QDialog";
    let menu_h = if is_window && parent.children.iter().any(|c| c.class_name == "QMenuBar") {
        22
    } else {
        0
    };
    let status_h = if is_window && parent.children.iter().any(|c| c.class_name == "QStatusBar") {
        18
    } else {
        0
    };
    let client = Rect::new(
        parent_rect.x,
        parent_rect.y + menu_h,
        parent_rect.width,
        (parent_rect.height - menu_h - status_h).max(0),
    );

    if let Some(layout) = parent.layout.as_mut() {
        layout_into(layout, content_box(parent_rect, layout));
    }

    for child in &mut parent.children {
        if child.class_name == "QMenuBar" {
            child.rect = Rect::new(parent_rect.x, parent_rect.y, parent_rect.width, menu_h);
        } else if child.class_name == "QStatusBar" {
            child.rect = Rect::new(
                parent_rect.x,
                parent_rect.bottom() - status_h,
                parent_rect.width,
                status_h,
            );
        } else if is_window
            && (child.name == "centralwidget" || child.class_name == "QWidget")
            && child.geometry_prop().is_none()
        {
            child.rect = client;
        } else if let Some(g) = child.geometry_prop() {
            child.rect = Rect::new(parent.rect.x + g.x, parent.rect.y + g.y, g.width, g.height);
        } else if child.rect.is_empty() {
            child.rect = parent.rect;
        }
        place_children(child);
    }
}

fn content_box(rect: Rect, layout: &LayoutNode) -> Rect {
    let margin = layout_i32(layout, "margin").unwrap_or(0);
    let left = layout_i32(layout, "leftMargin").unwrap_or(margin);
    let top = layout_i32(layout, "topMargin").unwrap_or(margin);
    let right = layout_i32(layout, "rightMargin").unwrap_or(margin);
    let bottom = layout_i32(layout, "bottomMargin").unwrap_or(margin);
    Rect::new(
        rect.x + left,
        rect.y + top,
        (rect.width - left - right).max(0),
        (rect.height - top - bottom).max(0),
    )
}

fn layout_i32(layout: &LayoutNode, name: &str) -> Option<i32> {
    layout
        .properties
        .get(name)
        .and_then(PropValue::as_f64)
        .map(|v| v as i32)
}

fn layout_into(layout: &mut LayoutNode, area: Rect) {
    let spacing = layout_i32(layout, "spacing").unwrap_or(0);
    let class = layout.class_name.as_str();
    if class.contains("Grid") || class.contains("Form") {
        layout_grid(layout, area, spacing);
    } else if class.contains("HBox") {
        layout_box(layout, area, spacing, true);
    } else {
        layout_box(layout, area, spacing, false);
    }
}

fn layout_box(layout: &mut LayoutNode, area: Rect, spacing: i32, horizontal: bool) {
    let n = layout.items.len().max(1) as i32;
    let gaps = spacing * (n - 1).max(0);
    let span = if horizontal { area.width } else { area.height };
    let each = ((span - gaps) / n).max(0);
    let mut cursor = if horizontal { area.x } else { area.y };
    for item in &mut layout.items {
        let rect = if horizontal {
            Rect::new(cursor, area.y, each, area.height)
        } else {
            Rect::new(area.x, cursor, area.width, each)
        };
        place_item(item, rect);
        cursor += each + spacing;
    }
}

fn layout_grid(layout: &mut LayoutNode, area: Rect, spacing: i32) {
    let mut rows = 1;
    let mut cols = 1;
    for item in &layout.items {
        let r = item.row.unwrap_or(0) + item.row_span.unwrap_or(1);
        let c = item.column.unwrap_or(0) + item.col_span.unwrap_or(1);
        rows = rows.max(r);
        cols = cols.max(c);
    }
    if layout.class_name.contains("Form") {
        cols = cols.max(2);
    }
    let cell_w = ((area.width - spacing * (cols - 1).max(0)) / cols).max(0);
    let cell_h = ((area.height - spacing * (rows - 1).max(0)) / rows).max(0);
    for item in &mut layout.items {
        let r = item.row.unwrap_or(0);
        let c = item.column.unwrap_or(0);
        let rs = item.row_span.unwrap_or(1).max(1);
        let cs = item.col_span.unwrap_or(1).max(1);
        let x = area.x + c * (cell_w + spacing);
        let y = area.y + r * (cell_h + spacing);
        let w = cell_w * cs + spacing * (cs - 1);
        let h = cell_h * rs + spacing * (rs - 1);
        place_item(item, Rect::new(x, y, w, h));
    }
}

fn place_item(item: &mut LayoutItem, rect: Rect) {
    if let Some(widget) = item.widget.as_mut() {
        widget.rect = rect;
        if widget.layout.is_some() || !widget.children.is_empty() {
            place_children(widget);
        }
    }
    if let Some(layout) = item.layout.as_mut() {
        layout_into(layout, rect);
    }
}
