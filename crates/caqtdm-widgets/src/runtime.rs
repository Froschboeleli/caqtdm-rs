//! Turns a parsed display plus a [`Hub`](caqtdm_datasource::Hub) into visuals.
//!
//! Channel values, alarm color, visibility, calc, and writes live here so the
//! egui viewer and the software rasterizer share one behavior.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use caqtdm_core::{
    alarm_color, apply_macros, eval_calc, eval_visibility, format_number, CalcInputs, ChannelData,
    Color, FormatType, MacroMap, PropValue, PutValue, PvValue, Severity,
};
use caqtdm_datasource::Hub;
use caqtdm_ui::WidgetNode;

use crate::visual::{
    qt_global_color, Action, ColorMap, CurveStyle, CurveSymbol, Effect, GridCell, ShapeForm,
    ThermoBand, TreeRow, Visual, VisualKind,
};

const HISTORY: usize = 240;

pub struct Session {
    pub root: WidgetNode,
    hub: Hub,
    /// Monitor id to the channel string the widget asked for.
    subs: BTreeMap<u64, String>,
    cache: BTreeMap<String, ChannelData>,
    history: BTreeMap<String, VecDeque<f64>>,
    waterfall: BTreeMap<String, VecDeque<Vec<f64>>>,
    pending: BTreeMap<String, f64>,
    toggles: BTreeMap<String, bool>,
    tabs: BTreeMap<String, usize>,
    /// x fraction lo/hi, then y data lo/hi.
    zooms: BTreeMap<String, (f64, f64, f64, f64)>,
    cursors: BTreeMap<String, Vec<f64>>,
    /// Alarm-tree group names that are folded shut.
    collapsed: BTreeSet<String>,
    alarm_log: VecDeque<Vec<String>>,
    last_severity: BTreeMap<String, Severity>,
    pub log: Vec<String>,
    pub allow_shell: bool,
    now_ms: u64,
    /// Widgets in the same order as [`WidgetNode::walk`].
    nodes: Vec<WidgetNode>,
}

impl Session {
    pub fn open(mut root: WidgetNode, hub: Hub, macros: &MacroMap) -> Self {
        substitute(&mut root, macros);
        let mut session = Self {
            root,
            hub,
            subs: BTreeMap::new(),
            cache: BTreeMap::new(),
            history: BTreeMap::new(),
            waterfall: BTreeMap::new(),
            pending: BTreeMap::new(),
            toggles: BTreeMap::new(),
            tabs: BTreeMap::new(),
            zooms: BTreeMap::new(),
            cursors: BTreeMap::new(),
            collapsed: BTreeSet::new(),
            alarm_log: VecDeque::new(),
            last_severity: BTreeMap::new(),
            log: Vec::new(),
            allow_shell: false,
            now_ms: 0,
            nodes: Vec::new(),
        };
        session.reindex();
        let channels = session.channels();
        for raw in channels {
            session.watch(&raw);
        }
        session
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        for (id, data) in self.hub.poll(now_ms) {
            if let Some(name) = self.subs.get(&id).cloned() {
                if let Some(sample) = data.value.as_f64() {
                    let series = self.history.entry(name.clone()).or_default();
                    series.push_back(sample);
                    while series.len() > HISTORY {
                        series.pop_front();
                    }
                }
                if matches!(data.value, PvValue::Doubles(_)) {
                    let row = data.value.as_f64_slice();
                    let rows = self.waterfall.entry(name.clone()).or_default();
                    rows.push_back(row);
                    while rows.len() > 64 {
                        rows.pop_front();
                    }
                }
                self.note_alarm(&name, &data);
                self.cache.insert(name, data);
            }
        }
        self.propagate_rescale();
    }

    fn note_alarm(&mut self, name: &str, data: &ChannelData) {
        let previous = self.last_severity.insert(name.to_string(), data.severity);
        if previous.is_some_and(|old| old != data.severity) {
            self.alarm_log.push_back(vec![
                name.to_string(),
                data.severity.name().to_string(),
                data.value.as_text(),
            ]);
            while self.alarm_log.len() > 32 {
                self.alarm_log.pop_front();
            }
        }
    }

    pub fn visuals(&self) -> Vec<Visual> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(id, node)| self.visual(id as u32, node))
            .collect()
    }

    pub fn act(&mut self, action: Action) -> Vec<Effect> {
        let id = match &action {
            Action::Press { id }
            | Action::SetNumber { id, .. }
            | Action::SetText { id, .. }
            | Action::Choose { id, .. }
            | Action::Zoom { id, .. }
            | Action::PlaceCursor { id, .. }
            | Action::ToggleGroup { id, .. }
            | Action::Roi { id, .. } => *id,
        };
        let Some(node) = self.nodes.get(id as usize).cloned() else {
            return Vec::new();
        };
        match action {
            Action::Press { .. } => self.press(&node),
            Action::SetNumber { value, .. } => {
                let value = clamp_entry(&node, value);
                if node.class_name == "caApplyNumeric" {
                    self.pending.insert(node.name.clone(), value);
                    self.log.push(format!("pending {} = {value}", node.name));
                } else {
                    self.write_number(&node, value);
                }
                Vec::new()
            }
            Action::SetText { text, .. } => {
                self.write_text(&node, &text);
                Vec::new()
            }
            Action::Choose { index, .. } => self.choose(&node, index),
            Action::Zoom {
                x_min,
                x_max,
                y_min,
                y_max,
                ..
            } => {
                self.zooms
                    .insert(node.name.clone(), (x_min, x_max, y_min, y_max));
                self.log.push(format!(
                    "zoom {} x {x_min} {x_max} y {y_min} {y_max}",
                    node.name
                ));
                Vec::new()
            }
            Action::PlaceCursor { at, .. } => {
                self.cursors.entry(node.name.clone()).or_default().push(at);
                self.log.push(format!("cursor {} {at}", node.name));
                Vec::new()
            }
            Action::ToggleGroup { index, .. } => {
                self.toggle_group(&node, index);
                Vec::new()
            }
            Action::Roi { x0, y0, x1, y1, .. } => {
                self.write_roi(&node, x0, y0, x1, y1);
                Vec::new()
            }
        }
    }

    fn press(&mut self, node: &WidgetNode) -> Vec<Effect> {
        match node.class_name.as_str() {
            "caMessageButton" | "QPushButton" | "QToolButton" => {
                if let Some(text) = node
                    .prop_str("pressMessage")
                    .or_else(|| node.prop_str("label"))
                {
                    if node.prop_str("channel").is_some() {
                        self.write_text(node, text);
                    }
                } else if node.prop_str("channel").is_some() {
                    let value = node.prop_f64("pressValue").unwrap_or(1.0);
                    self.write_number(node, value);
                }
                let label = node
                    .prop_str("label")
                    .or_else(|| node.prop_str("text"))
                    .unwrap_or(&node.name);
                vec![Effect::Message(format!("pressed {label}"))]
            }
            "caToggleButton" | "QCheckBox" | "QRadioButton" => {
                let on = !self.toggles.get(&node.name).copied().unwrap_or(false);
                self.toggles.insert(node.name.clone(), on);
                self.write_number(node, if on { 1.0 } else { 0.0 });
                Vec::new()
            }
            "caRelatedDisplay" | "caMimeDisplay" => self.choose(node, 0),
            "caShellCommand" | "caScriptButton" => {
                let command = shell_line(node);
                let ran =
                    self.allow_shell && node.class_name == "caShellCommand" && !command.is_empty();
                if ran {
                    let _ = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&command)
                        .spawn();
                }
                self.log.push(format!("shell {command}"));
                vec![Effect::Shell { command, ran }]
            }
            "caSpinbox" | "QSpinBox" | "QDoubleSpinBox" => {
                let current = self
                    .primary(node)
                    .and_then(|d| d.value.as_f64())
                    .unwrap_or(0.0);
                let step = node.prop_f64("incrementValue").unwrap_or(1.0);
                self.write_number(node, current + step);
                Vec::new()
            }
            "caHMIConfig" => {
                let expr = node.prop_str("valueOrCalc").unwrap_or("0");
                let value = if node
                    .enum_tail("calculationType")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("Calc"))
                {
                    eval_calc(expr, &self.calc_inputs(node)).unwrap_or(0.0)
                } else {
                    expr.parse().unwrap_or(0.0)
                };
                if let Some(out) = node.prop_str("outputA").filter(|s| !s.is_empty()) {
                    let _ = self.hub.put(out, &PutValue::Double(value));
                    self.log.push(format!("put {out} = {value}"));
                }
                Vec::new()
            }
            "caApplyNumeric" => {
                if let Some(value) = self.pending.get(&node.name).copied() {
                    self.write_number(node, value);
                    self.pending.remove(&node.name);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn choose(&mut self, node: &WidgetNode, index: usize) -> Vec<Effect> {
        if node.class_name == "caChoice" {
            let start = node.prop_f64("startBit").unwrap_or(0.0).max(0.0) as usize;
            self.write_number(node, (start + index) as f64);
            return Vec::new();
        }
        if node.class_name == "caRowColMenu" {
            let cells = grid_cells(node);
            if let Some(cell) = cells.get(index) {
                self.log.push(format!("open {} {}", cell.file, cell.args));
                return vec![Effect::Open {
                    file: cell.file.clone(),
                    macros: cell.args.clone(),
                }];
            }
            return Vec::new();
        }
        if node.class_name == "caMenu" || node.class_name == "QComboBox" {
            self.write_number(node, index as f64);
            return Vec::new();
        }
        if node.class_name == "caDoubleTabWidget" || node.class_name == "QTabWidget" {
            self.tabs.insert(node.name.clone(), index);
            self.log.push(format!("tab {} {index}", node.name));
            return Vec::new();
        }
        if node.class_name == "caReplaceMacro" {
            let values = macro_values(node);
            let key = node.prop_str("macroKey").unwrap_or("").to_string();
            let value = values.get(index).cloned().unwrap_or_default();
            self.log.push(format!("macro {key}={value}"));
            return vec![Effect::SetMacro { key, value }];
        }
        let entries = related_entries(node);
        if let Some((_, file)) = entries.get(index) {
            let macros = node
                .prop_str("args")
                .or_else(|| node.prop_str("macros"))
                .unwrap_or("")
                .to_string();
            self.log.push(format!("open {file}"));
            return vec![Effect::Open {
                file: file.clone(),
                macros,
            }];
        }
        Vec::new()
    }

    fn write_number(&mut self, node: &WidgetNode, value: f64) {
        let Some(raw) = node.prop_str("channel").or_else(|| node.prop_str("pv")) else {
            return;
        };
        match self.hub.put(raw, &PutValue::Double(value)) {
            Ok(()) => self.log.push(format!("put {raw} = {value}")),
            Err(err) => self.log.push(err.to_string()),
        }
    }

    fn write_text(&mut self, node: &WidgetNode, text: &str) {
        let Some(raw) = node.prop_str("channel").or_else(|| node.prop_str("pv")) else {
            return;
        };
        let value = if let Ok(number) = text.trim().parse::<f64>() {
            PutValue::Double(number)
        } else {
            PutValue::String(text.to_string())
        };
        match self.hub.put(raw, &value) {
            Ok(()) => self.log.push(format!("put {raw} = {text}")),
            Err(err) => self.log.push(err.to_string()),
        }
    }

    fn write_roi(&mut self, node: &WidgetNode, x0: f64, y0: f64, x1: f64, y1: f64) {
        let Some(values) = roi_write_values(roi_code(node, "ROI_writeType"), x0, y0, x1, y1) else {
            return;
        };
        let channels = split_prop(node, "ROI_writeChannels");
        if channels.len() != values.len() {
            self.log.push(format!("roi {} channel count", node.name));
            return;
        }
        for (raw, value) in channels.iter().zip(values) {
            if raw.is_empty() {
                continue;
            }
            match self.hub.put(raw, &PutValue::Double(value)) {
                Ok(()) => self.log.push(format!("put {raw} = {value}")),
                Err(err) => self.log.push(err.to_string()),
            }
        }
    }

    fn visual(&self, id: u32, node: &WidgetNode) -> Visual {
        let data = self.primary(node);
        let (mut foreground, background) = colors(node, data);
        if node.class_name == "caLed" {
            foreground = led_lamp(node, data, foreground);
        }
        if node.class_name == "caPolyLine" {
            if let Some(color) = prop_color(node, "lineColor") {
                foreground = color;
            }
        }
        let visible = self.is_visible(node);
        let text = self.caption(node, data);
        let kind = self.kind(node, data);
        Visual {
            id,
            class_name: node.class_name.clone(),
            name: node.name.clone(),
            rect: node.rect,
            foreground,
            background,
            text,
            kind,
            visible,
            severity: data.map(|d| d.severity).unwrap_or(Severity::NotConnected),
            font_px: font_px_of(node),
            rotation: rotation_of(node),
            align: align_of(node),
            border: prop_color(node, "borderColor").or_else(|| prop_color(node, "frameColor")),
            border_px: node
                .prop_f64("lineWidth")
                .or_else(|| node.prop_f64("borderWidth"))
                .or_else(|| node.prop_f64("frameLineWidth"))
                .unwrap_or(1.0)
                .max(1.0) as i32,
        }
    }

    fn kind(&self, node: &WidgetNode, data: Option<&ChannelData>) -> VisualKind {
        let value = data.and_then(|d| d.value.as_f64()).unwrap_or(0.0);
        let (min, max) = limits_of(node, data);
        let fraction = scaled_fraction(node, value, min, max);
        match node.class_name.as_str() {
            "caLabel" | "caLabelVertical" | "QLabel" | "caMultiLineString" | "QTextEdit"
            | "QPlainTextEdit" => VisualKind::Label,
            "caLineEdit" | "caTextEntry" | "QLineEdit" | "QLCDNumber" => VisualKind::Entry {
                writable: node.class_name != "caLineEdit" && node.class_name != "QLCDNumber",
            },
            "caNumeric" | "caSpinbox" | "QSpinBox" | "QDoubleSpinBox" => {
                VisualKind::Entry { writable: true }
            }
            "caApplyNumeric" => VisualKind::Entry { writable: true },
            "caMessageButton" | "QPushButton" | "QToolButton" => VisualKind::Button,
            "caRowColMenu" => {
                let cells = grid_cells(node);
                if cells.is_empty() {
                    VisualKind::Button
                } else {
                    VisualKind::Grid {
                        columns: rowcol_columns(node, cells.len()),
                        cells,
                    }
                }
            }
            "caToggleButton" | "QCheckBox" | "QRadioButton" => VisualKind::Toggle {
                on: self
                    .toggles
                    .get(&node.name)
                    .copied()
                    .unwrap_or(value != 0.0),
            },
            "caSlider" | "QSlider" | "QScrollBar" => VisualKind::Slider {
                fraction: slider_fraction(node, fraction),
                min,
                max,
            },
            "caLinearGauge" => VisualKind::Gauge {
                fraction,
                circular: false,
            },
            "caCircularGauge" | "QDial" => VisualKind::Gauge {
                fraction,
                circular: true,
            },
            "caThermo" | "QProgressBar" => VisualKind::Thermo {
                fraction,
                bands: thermo_bands(node, data, min, max),
            },
            "caMeter" => VisualKind::Meter { fraction },
            "caLed" => VisualKind::Led {
                on: led_on(
                    node,
                    value,
                    data.map(|item| item.value.as_text())
                        .as_deref()
                        .unwrap_or(""),
                ),
            },
            "caGraphics" => VisualKind::Shape {
                form: shape_form(node),
                filled: node
                    .enum_tail("fillstyle")
                    .is_none_or(|s| !s.eq_ignore_ascii_case("Outline")),
                start_angle: node.prop_f64("startAngle").unwrap_or(0.0) as i32,
                span_angle: node.prop_f64("spanAngle").unwrap_or(90.0) as i32,
            },
            "caPolyLine" => VisualKind::Polyline {
                points: polyline_points(node),
                closed: node
                    .enum_tail("polystyle")
                    .is_some_and(|s| s.eq_ignore_ascii_case("Polygon")),
                width: node.prop_f64("lineSize").unwrap_or(1.0).max(1.0) as i32,
            },
            "caCartesianPlot" | "caStripPlot" => self.plot_kind(node),
            "caScan2D" => self.scan_kind(
                node,
                data,
                self.axis_count(node, "channelWidth"),
                self.axis_count(node, "channelHeight"),
            ),
            "caWaterfallPlot" => {
                let rows = self.waterfall_rows(node);
                let (min, max) = waterfall_limits(node, &rows);
                VisualKind::Waterfall {
                    rows,
                    min,
                    max,
                    map: color_map(node),
                }
            }
            "caByte" | "caByteController" | "caBitnames" => VisualKind::Bits {
                value: value as u32,
                count: bit_count(node),
                names: bit_names(node),
            },
            "caTable" | "caWaveTable" => VisualKind::Table {
                cells: self.table_cells(node),
            },
            "caMenu" | "QComboBox" => VisualKind::Menu {
                items: menu_items(node, data),
                selected: value.max(0.0) as usize,
            },
            "caChoice" => {
                let (items, selected) = choice_items(node, data, value);
                let (columns, inverse) = choice_layout(node, items.len());
                VisualKind::Choice {
                    items,
                    selected,
                    columns,
                    inverse,
                }
            }
            "caRelatedDisplay" | "caMimeDisplay" => VisualKind::Related {
                entries: related_entries(node),
            },
            "caShellCommand" | "caScriptButton" => VisualKind::Command {
                line: shell_line(node),
            },
            "caClock" => {
                let (hour, minute, second) = clock_parts(self.now_ms);
                VisualKind::Clock {
                    analog: node
                        .enum_tail("timeType")
                        .is_some_and(|t| t.eq_ignore_ascii_case("Analog")),
                    hour,
                    minute,
                    second,
                }
            }
            "caDoubleTabWidget" | "QTabWidget" => VisualKind::Tabs {
                labels: tab_labels(node),
                current: self.tabs.get(&node.name).copied().unwrap_or_else(|| {
                    node.prop_f64("currentIndex").unwrap_or(0.0).max(0.0) as usize
                }),
            },
            "caFrame" | "QFrame" | "QGroupBox" | "QWidget" | "QMainWindow" | "QDialog" => {
                VisualKind::Frame
            }
            "caImage" | "caCamera" => self.image_kind(
                node,
                data,
                self.axis_count(node, "channelWidth"),
                self.axis_count(node, "channelHeight"),
            ),
            "caReplaceMacro" => VisualKind::Menu {
                items: macro_values(node),
                selected: 0,
            },
            "caCalc" => VisualKind::Label,
            "caInclude" => VisualKind::Frame,
            "caAlarmTree" => VisualKind::Tree {
                rows: self.alarm_tree_rows(node),
            },
            "caAlarmLog" => VisualKind::Table {
                cells: self.alarm_rows(data),
            },
            "caLineDraw" => VisualKind::Polyline {
                points: self.line_draw_points(node),
                closed: false,
                width: node.prop_f64("frameLineWidth").unwrap_or(1.0).max(1.0) as i32,
            },
            other => crate::designer::designer_kind(other).unwrap_or(VisualKind::Unknown),
        }
    }

    fn caption(&self, node: &WidgetNode, data: Option<&ChannelData>) -> String {
        if node.class_name == "caCalc" {
            let expr = node
                .prop_str("calc")
                .or_else(|| node.prop_str("visibilityCalc"))
                .unwrap_or("0");
            return match eval_calc(expr, &self.calc_inputs(node)) {
                Ok(v) => format_number(v, FormatType::Decimal, precision_of(node, data)),
                Err(err) => err.to_string(),
            };
        }
        if node.class_name == "caClock" {
            return format_clock(self.now_ms);
        }
        if let Some(data) = data {
            if !data.connected && is_monitor(&node.class_name) {
                return "---".to_string();
            }
            if is_numeric_text(&node.class_name) {
                let format = FormatType::parse(node.enum_tail("formatType").unwrap_or("decimal"));
                let textual = matches!(data.value, PvValue::String(_) | PvValue::Enum { .. });
                if textual && format != FormatType::Enumeric {
                    return data.value.as_text();
                }
                if let Some(value) = data.value.as_f64() {
                    return with_units(
                        node,
                        data,
                        format_number(value, format, precision_of(node, Some(data))),
                    );
                }
                return data.value.as_text();
            }
        }
        node.prop_str("text")
            .or_else(|| node.prop_str("label"))
            .or_else(|| node.prop_str("Title"))
            .unwrap_or("")
            .to_string()
    }

    fn is_visible(&self, node: &WidgetNode) -> bool {
        let mode = node.enum_tail("visibility").unwrap_or("StaticV");
        let value = self
            .primary(node)
            .and_then(|d| d.value.as_f64())
            .unwrap_or(0.0);
        match mode.to_ascii_lowercase().as_str() {
            "ifnotzero" => value != 0.0,
            "ifzero" => value == 0.0,
            "calc" => {
                let expr = node.prop_str("visibilityCalc").unwrap_or("1");
                eval_visibility(expr, &self.calc_inputs(node)).unwrap_or(true)
            }
            _ => true,
        }
    }

    fn calc_inputs(&self, node: &WidgetNode) -> CalcInputs {
        let mut inputs = CalcInputs::default();
        let channels = node_channels(node);
        for (index, raw) in channels.iter().enumerate().take(12) {
            if let Some(data) = self.cache.get(raw) {
                inputs.set(index, data.value.as_f64().unwrap_or(0.0));
                inputs.set_string(index, data.value.as_text());
            }
        }
        inputs
    }

    /// Width or height from a number property, or from the channel named by that property.
    fn axis_count(&self, node: &WidgetNode, key: &str) -> u32 {
        if let Some(number) = node.prop_f64(key) {
            if number > 0.0 {
                return number as u32;
            }
        }
        node.prop_str(key)
            .filter(|name| name.contains("://") || name.chars().any(|c| c.is_ascii_alphabetic()))
            .and_then(|name| self.cache.get(name))
            .and_then(|data| data.value.as_f64())
            .unwrap_or(0.0)
            .max(0.0) as u32
    }

    fn primary<'a>(&'a self, node: &WidgetNode) -> Option<&'a ChannelData> {
        node_channels(node)
            .first()
            .and_then(|raw| self.cache.get(raw))
    }

    fn plot_kind(&self, node: &WidgetNode) -> VisualKind {
        let series = self.plot_series(node);
        let series_len = series.len().clamp(1, 7);
        let series_axes: Vec<(f64, f64)> = (0..series_len)
            .map(|index| axis_pair(node, index as u32))
            .collect();
        let (axis_min, axis_max) = series_axes.first().copied().unwrap_or((f64::NAN, f64::NAN));
        let (y2_min, y2_max) = series_axes.get(1).copied().unwrap_or((f64::NAN, f64::NAN));
        let (x_min, x_max, y_min, y_max) = self
            .zooms
            .get(&node.name)
            .copied()
            .unwrap_or((0.0, 1.0, axis_min, axis_max));
        VisualKind::Plot {
            series,
            title: node.prop_str("Title").unwrap_or("").to_string(),
            title_x: node.prop_str("TitleX").unwrap_or("").to_string(),
            title_y: node.prop_str("TitleY").unwrap_or("").to_string(),
            x_min,
            x_max,
            y_min,
            y_max,
            y2_min,
            y2_max,
            log_y: node
                .enum_tail("YAxisType")
                .is_some_and(|mode| mode.eq_ignore_ascii_case("log10")),
            x_ticks: node.prop_f64("numberOfXticks").unwrap_or(5.0) as u32,
            cursors: self.cursors.get(&node.name).cloned().unwrap_or_default(),
            legend: plot_legend(node, &series_axes),
            colors: plot_colors(node, series_len),
            axes: series_axes,
            styles: (0..series_len)
                .map(|index| curve_style(node, index))
                .collect(),
            grid: node.prop_bool("grid").unwrap_or(true),
            show_x: node.prop_bool("XaxisEnabled").unwrap_or(true),
            show_y: node.prop_bool("YaxisEnabled").unwrap_or(true),
            symbols: (0..series_len)
                .map(|index| curve_symbol(node, index))
                .collect(),
            symbol_px: symbol_px(node),
            x_limits: x_axis_limits(node),
            log_x: axis_named(node, "XaxisType", "log10"),
            time_x: axis_named(node, "XaxisType", "time"),
            grid_color: prop_color(node, "gridColor").unwrap_or(Color::rgb(160, 160, 160)),
            scale_color: prop_color(node, "scaleColor"),
        }
    }

    fn plot_series(&self, node: &WidgetNode) -> Vec<Vec<f64>> {
        let span = if node.class_name == "caStripPlot" {
            node.prop_f64("timeSpan")
                .map(|n| n as usize)
                .filter(|n| *n > 0)
        } else {
            None
        };
        node_channels(node)
            .into_iter()
            .map(|raw| {
                let mut samples: Vec<f64> = self
                    .history
                    .get(&raw)
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default();
                if let Some(span) = span {
                    if samples.len() > span {
                        samples = samples[samples.len() - span..].to_vec();
                    }
                }
                samples
            })
            .collect()
    }

    fn waterfall_rows(&self, node: &WidgetNode) -> Vec<Vec<f64>> {
        let mut rows: Vec<Vec<f64>> = node_channels(node)
            .first()
            .and_then(|raw| self.waterfall.get(raw))
            .map(|rows| rows.iter().cloned().collect())
            .unwrap_or_default();
        if let Some(limit) = node.prop_f64("NumberOfTraces") {
            let limit = limit.max(0.0) as usize;
            if limit > 0 && rows.len() > limit {
                rows = rows.split_off(rows.len() - limit);
            }
        }
        rows
    }

    fn line_draw_points(&self, node: &WidgetNode) -> Vec<caqtdm_core::Point> {
        let raw = node.prop_str("channel").unwrap_or("");
        let samples: Vec<f64> = self
            .history
            .get(raw)
            .map(|series| series.iter().copied().collect())
            .unwrap_or_default();
        if samples.is_empty() {
            return Vec::new();
        }
        let (min, max) = limits_of(node, self.cache.get(raw));
        let width = node.rect.width.max(1);
        let height = node.rect.height.max(1);
        let last = samples.len().saturating_sub(1).max(1) as f64;
        samples
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let x = if samples.len() == 1 {
                    0
                } else {
                    ((index as f64) / last * f64::from(width - 1)) as i32
                };
                let y = height - 1 - (fraction(*sample, min, max) * f64::from(height - 1)) as i32;
                caqtdm_core::Point { x, y }
            })
            .collect()
    }

    fn table_cells(&self, node: &WidgetNode) -> Vec<Vec<String>> {
        let channels = node_channels(node);
        if node.class_name == "caWaveTable" {
            let data = channels.first().and_then(|raw| self.cache.get(raw));
            return wave_table(node, data);
        }
        let columns: Vec<Vec<String>> = channels
            .iter()
            .map(|raw| column_text(node, self.cache.get(raw)))
            .collect();
        if columns.is_empty() {
            return vec![vec!["---".into()]];
        }
        let rows = columns.iter().map(Vec::len).max().unwrap_or(0).max(1);
        (0..rows)
            .map(|row| {
                columns
                    .iter()
                    .map(|column| column.get(row).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    }

    fn toggle_group(&mut self, node: &WidgetNode, index: usize) {
        let rows = self.alarm_tree_rows(node);
        let Some(row) = rows.get(index) else {
            return;
        };
        if !row.group {
            return;
        }
        if !self.collapsed.remove(&row.name) {
            self.collapsed.insert(row.name.clone());
        }
        self.log.push(format!(
            "tree {} {}",
            row.name,
            self.collapsed.contains(&row.name)
        ));
    }

    fn propagate_rescale(&mut self) {
        let jobs: Vec<(String, f64)> = self
            .nodes
            .iter()
            .filter(|node| node.class_name == "wmSignalRescale")
            .flat_map(|node| {
                let mut out = Vec::new();
                if let Some(name) = node.prop_str("softChannelA").filter(|s| !s.is_empty()) {
                    out.push((name.to_string(), f64::from(node.rect.width)));
                }
                if let Some(name) = node.prop_str("softChannelB").filter(|s| !s.is_empty()) {
                    out.push((name.to_string(), f64::from(node.rect.height)));
                }
                out
            })
            .collect();
        for (name, value) in jobs {
            if self.hub.put(&name, &PutValue::Double(value)).is_ok() {
                self.log.push(format!("rescale {name} = {value}"));
            }
        }
    }

    fn channels(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.root.walk(&mut |node| {
            for raw in node_channels(node) {
                if !raw.is_empty() && !out.contains(&raw) {
                    out.push(raw);
                }
            }
        });
        out
    }

    fn watch(&mut self, raw: &str) {
        match self.hub.monitor(raw) {
            Ok(id) => {
                self.subs.insert(id, raw.to_string());
                self.cache
                    .insert(raw.to_string(), ChannelData::disconnected(raw));
            }
            Err(err) => self.log.push(err.to_string()),
        }
    }

    fn reindex(&mut self) {
        self.nodes.clear();
        collect(&self.root, &mut self.nodes);
    }

    fn alarm_tree_rows(&self, node: &WidgetNode) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        fn walk(
            node: &WidgetNode,
            depth: u32,
            cache: &BTreeMap<String, ChannelData>,
            collapsed: &BTreeSet<String>,
            rows: &mut Vec<TreeRow>,
        ) {
            let group = node.class_name != "caAlarmTree" && !node.children.is_empty();
            let open = !collapsed.contains(&node.name);
            if node.class_name != "caAlarmTree" {
                let channel = node.prop_str("channel").unwrap_or("").to_string();
                let severity = cache
                    .get(&channel)
                    .map(|data| data.severity.name().to_string())
                    .unwrap_or_else(|| "n/a".into());
                rows.push(TreeRow {
                    name: node.name.clone(),
                    depth,
                    group,
                    open,
                    cells: vec![
                        node.name.clone(),
                        channel,
                        node.prop_str("mask").unwrap_or("").to_string(),
                        severity,
                    ],
                });
            }
            if node.class_name == "caAlarmTree" || open {
                let next = if node.class_name == "caAlarmTree" {
                    depth
                } else {
                    depth + 1
                };
                for child in &node.children {
                    walk(child, next, cache, collapsed, rows);
                }
            }
        }
        walk(node, 0, &self.cache, &self.collapsed, &mut rows);
        if rows.is_empty() {
            rows.push(TreeRow {
                name: "empty".into(),
                depth: 0,
                group: false,
                open: true,
                cells: vec!["empty".into()],
            });
        }
        rows
    }

    fn alarm_rows(&self, data: Option<&ChannelData>) -> Vec<Vec<String>> {
        if !self.alarm_log.is_empty() {
            return self.alarm_log.iter().cloned().collect();
        }
        let Some(data) = data else {
            return vec![vec!["disconnected".into()]];
        };
        vec![vec![
            data.description.clone(),
            data.severity.name().to_string(),
            data.value.as_text(),
        ]]
    }

    fn scan_kind(
        &self,
        node: &WidgetNode,
        data: Option<&ChannelData>,
        width: u32,
        height: u32,
    ) -> VisualKind {
        if width > 0 && height > 0 {
            if let Some(samples) = self.savedata_samples(node, width, height) {
                return image_visual(
                    "mda".into(),
                    width,
                    height,
                    samples_to_rgba(&samples, width, height),
                    self.roi_mark(node),
                );
            }
        }
        let mut kind = scan_waveform(node, data, width, height);
        if let VisualKind::Image { roi, rgba, .. } = &mut kind {
            *roi = self.roi_mark(node);
            if let Some(map) = explicit_color_map(node) {
                crate::colorize_rgba(rgba, &map);
            }
        }
        kind
    }

    fn image_kind(
        &self,
        node: &WidgetNode,
        data: Option<&ChannelData>,
        width: u32,
        height: u32,
    ) -> VisualKind {
        let mut kind = image_waveform(node, data, width, height);
        if let VisualKind::Image { roi, rgba, .. } = &mut kind {
            *roi = self.roi_mark(node);
            if let Some(map) = explicit_color_map(node) {
                crate::colorize_rgba(rgba, &map);
            }
        }
        kind
    }

    fn savedata_samples(&self, node: &WidgetNode, width: u32, height: u32) -> Option<Vec<f64>> {
        let path = self.savedata_text(node, "channelSAVEDATA_PATH")?;
        let sub = self.savedata_text(node, "channelSAVEDATA_SUBDIR")?;
        let file = self.savedata_text(node, "channelSAVEDATA_FILENAME")?;
        let joined = format!("{path}/{sub}/{file}");
        let filename = crate::mda::mda_filename(&joined);
        let bytes = std::fs::read(&filename).ok()?;
        let pv = node.prop_str("channelData").unwrap_or("");
        let image = crate::mda::detector_image(&bytes, pv, width as usize, height as usize).ok()?;
        Some(image.into_iter().map(f64::from).collect())
    }

    fn savedata_text(&self, node: &WidgetNode, key: &str) -> Option<String> {
        let name = node.prop_str(key)?.trim().to_string();
        if name.is_empty() {
            return None;
        }
        self.cache.get(&name).and_then(|data| match &data.value {
            PvValue::String(text) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
    }

    fn roi_mark(&self, node: &WidgetNode) -> Option<crate::RoiMark> {
        let kind = roi_code(node, "ROI_readType");
        if kind == 0 {
            return None;
        }
        let channels = split_prop(node, "ROI_readChannels");
        let values: Vec<f64> = channels
            .iter()
            .filter_map(|name| self.cache.get(name).and_then(|data| data.value.as_f64()))
            .collect();
        let need = if kind == 1 { 2 } else { 4 };
        if values.len() < need {
            return None;
        }
        let marker = roi_marker(node);
        let (x0, y0, x1, y1) = match kind {
            1 => (values[0], values[1], values[0], values[1]),
            4 => {
                let (cx, cy, w, h) = (values[0], values[1], values[2], values[3]);
                (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0)
            }
            _ => (values[0], values[1], values[2], values[3]),
        };
        Some(crate::RoiMark {
            x0,
            y0,
            x1,
            y1,
            kind,
            marker,
        })
    }
}

fn collect(node: &WidgetNode, out: &mut Vec<WidgetNode>) {
    out.push(node.clone());
    for child in &node.children {
        collect(child, out);
    }
    if let Some(layout) = &node.layout {
        collect_layout(layout, out);
    }
}

fn collect_layout(layout: &caqtdm_ui::LayoutNode, out: &mut Vec<WidgetNode>) {
    for item in &layout.items {
        if let Some(widget) = &item.widget {
            collect(widget, out);
        }
        if let Some(nested) = &item.layout {
            collect_layout(nested, out);
        }
    }
}

fn substitute(node: &mut WidgetNode, macros: &MacroMap) {
    for value in node.properties.values_mut() {
        match value {
            PropValue::String(text) | PropValue::Enum(text) | PropValue::Set(text) => {
                *text = apply_macros(text, macros);
            }
            PropValue::StringList(items) => {
                for item in items {
                    *item = apply_macros(item, macros);
                }
            }
            _ => {}
        }
    }
    for child in &mut node.children {
        substitute(child, macros);
    }
}

fn node_channels(node: &WidgetNode) -> Vec<String> {
    let mut out = Vec::new();
    for key in [
        "channel",
        "channelData",
        "channelWidth",
        "channelHeight",
        "channelB",
        "channelC",
        "channelD",
        "pv",
        "enumChannel",
        "channelEnum",
        "channelValue",
        "softChannelA",
        "softChannelB",
        "outputA",
        "outputB",
        "channels",
        "channels_1",
        "channels_2",
        "channels_3",
        "channels_4",
        "channels_5",
        "channels_6",
        "ROI_readChannels",
        "ROI_writeChannels",
        "channelSAVEDATA_PATH",
        "channelSAVEDATA_SUBDIR",
        "channelSAVEDATA_FILENAME",
        "channelXCPT",
        "channelYCPT",
        "channelXNEWDATA",
        "channelYNEWDATA",
    ] {
        if let Some(prop) = node.prop(key) {
            if key == "channels" || key.starts_with("channels_") {
                for item in prop.as_string_list() {
                    if !item.is_empty() && !out.contains(&item) {
                        out.push(item);
                    }
                }
            } else if let Some(text) = prop.as_str() {
                for item in text.split(';') {
                    let item = item.trim();
                    if !item.is_empty() && !out.contains(&item.to_string()) {
                        out.push(item.to_string());
                    }
                }
            }
        }
    }
    out
}

fn colors(node: &WidgetNode, data: Option<&ChannelData>) -> (Color, Color) {
    let mut fg = node
        .prop("foreground")
        .and_then(PropValue::as_color)
        .unwrap_or_else(|| Color::rgb(20, 20, 20));
    let mut bg = node
        .prop("background")
        .and_then(PropValue::as_color)
        .unwrap_or_else(|| Color::rgb(236, 236, 236));
    let mode = node.enum_tail("colorMode").unwrap_or("Static");
    if mode.eq_ignore_ascii_case("Alarm")
        || mode.eq_ignore_ascii_case("Alarm_Default")
        || mode.eq_ignore_ascii_case("Alarm_Static")
    {
        let Some(data) = data else {
            return (fg, bg);
        };
        if !data.connected {
            return (Color::rgb(90, 90, 90), Color::rgb(255, 255, 255));
        }
        return (alarm_color(data.severity), bg);
    }
    if let Some(sheet) = node.prop_str("styleSheet") {
        if let Some(color) = crate::style::css_color(sheet, "color") {
            fg = color;
        }
        if let Some(color) = crate::style::css_color(sheet, "background-color") {
            bg = color;
        }
    }
    (fg, bg)
}

fn limits_of(node: &WidgetNode, data: Option<&ChannelData>) -> (f64, f64) {
    let mode = node.enum_tail("limitsMode").unwrap_or("Channel");
    if !mode.eq_ignore_ascii_case("User") {
        if let Some(span) = data.and_then(|d| d.limits.display_span()) {
            return span;
        }
    }
    let min = node.prop_f64("minValue").unwrap_or(0.0);
    let max = node.prop_f64("maxValue").unwrap_or(100.0);
    (min, max)
}

fn fraction(value: f64, min: f64, max: f64) -> f64 {
    if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn precision_of(node: &WidgetNode, data: Option<&ChannelData>) -> i32 {
    let mode = node.enum_tail("precisionMode").unwrap_or("Channel");
    if mode.eq_ignore_ascii_case("User") {
        node.prop_f64("precision").unwrap_or(3.0) as i32
    } else {
        data.map(|d| d.precision)
            .unwrap_or_else(|| node.prop_f64("precision").unwrap_or(3.0) as i32)
    }
}

fn led_on(node: &WidgetNode, value: f64, text: &str) -> bool {
    if let Some(token) = node.prop_str("trueValue").filter(|text| !text.is_empty()) {
        if let Ok(number) = token.parse::<f64>() {
            return (value - number).abs() < 1e-9;
        }
        return text == token;
    }
    if let Some(token) = node.prop_str("falseValue").filter(|text| !text.is_empty()) {
        if let Ok(number) = token.parse::<f64>() {
            if (value - number).abs() < 1e-9 {
                return false;
            }
        } else if text == token {
            return false;
        }
    }
    let bit = node.prop_f64("bitNr").unwrap_or(-1.0);
    if bit >= 0.0 {
        let mask = 1u32 << (bit as u32);
        (value as u32) & mask != 0
    } else {
        value != 0.0
    }
}

/// Lamp color for `caLed`. On uses `trueColor`, off uses `falseColor`, and a
/// disconnected channel uses `undefinedColor` when that property is set.
fn led_lamp(node: &WidgetNode, data: Option<&ChannelData>, fallback: Color) -> Color {
    let value = data.and_then(|item| item.value.as_f64()).unwrap_or(0.0);
    let text = data.map(|item| item.value.as_text()).unwrap_or_default();
    if data.is_some_and(|item| !item.connected) {
        if let Some(color) = prop_color(node, "undefinedColor") {
            return color;
        }
    }
    if led_on(node, value, &text) {
        prop_color(node, "trueColor").unwrap_or(fallback)
    } else {
        prop_color(node, "falseColor").unwrap_or(Color::rgb(70, 70, 70))
    }
}

fn prop_color(node: &WidgetNode, key: &str) -> Option<Color> {
    node.prop(key).and_then(PropValue::as_color)
}

fn with_units(node: &WidgetNode, data: &ChannelData, text: String) -> String {
    if node.prop("unitsEnabled").and_then(PropValue::as_bool) != Some(true) {
        return text;
    }
    let units = node
        .prop_str("units")
        .filter(|item| !item.is_empty())
        .unwrap_or(data.units.as_str());
    if units.is_empty() {
        text
    } else {
        format!("{text} {units}")
    }
}

fn scaled_fraction(node: &WidgetNode, value: f64, min: f64, max: f64) -> f64 {
    let log = node
        .prop("logarithmicScale")
        .and_then(PropValue::as_bool)
        .unwrap_or(false);
    if log && min > 0.0 && max > min && value > 0.0 {
        let start = min.log10();
        let end = max.log10();
        return fraction(value.log10(), start, end);
    }
    fraction(value, min, max)
}

fn align_of(node: &WidgetNode) -> i8 {
    let raw = node
        .enum_tail("alignment")
        .unwrap_or("")
        .to_ascii_lowercase();
    if raw.contains("right") {
        2
    } else if raw.contains("center") {
        1
    } else {
        0
    }
}

fn shape_form(node: &WidgetNode) -> ShapeForm {
    match node.enum_tail("form").unwrap_or("Rectangle") {
        "Circle" | "Ellipse" => ShapeForm::Circle,
        "Arc" => ShapeForm::Arc,
        "Line" => ShapeForm::Line,
        "Triangle" => ShapeForm::Triangle,
        "Arrow" => ShapeForm::Arrow,
        _ => ShapeForm::Rectangle,
    }
}

fn polyline_points(node: &WidgetNode) -> Vec<caqtdm_core::Point> {
    let raw = node.prop_str("xyPairs").unwrap_or("");
    let nums: Vec<f64> = raw
        .split(|c: char| c == ';' || c == ',' || c.is_whitespace())
        .filter_map(|s| s.parse().ok())
        .collect();
    nums.chunks(2)
        .filter_map(|pair| {
            let y = pair.get(1).copied()?;
            Some(caqtdm_core::Point {
                x: pair[0] as i32,
                y: y as i32,
            })
        })
        .collect()
}

fn font_px_of(node: &WidgetNode) -> i32 {
    if let Some(sheet) = node.prop_str("styleSheet") {
        if let Some(px) = crate::style::css_px(sheet, "font-size") {
            return px;
        }
    }
    let mode = node.enum_tail("fontScaleMode").unwrap_or("None");
    let scaled = match mode.to_ascii_lowercase().as_str() {
        "height" | "heightandwidth" => (f64::from(node.rect.height.max(1)) * 0.7) as i32,
        "width" => (f64::from(node.rect.width.max(1)) * 0.2) as i32,
        _ => 0,
    };
    if scaled > 0 {
        return scaled.clamp(6, 64);
    }
    node.prop("font")
        .and_then(|value| match value {
            PropValue::Font(font) => Some(font.point_size),
            _ => None,
        })
        .unwrap_or(10)
}

fn rotation_of(node: &WidgetNode) -> i32 {
    if node.class_name == "caLabelVertical" {
        return 90;
    }
    let angle = node.prop_f64("tiltAngle").unwrap_or(0.0) as i32;
    if angle != 0 {
        return angle;
    }
    match node.enum_tail("direction").unwrap_or("") {
        "Up" | "Down" => 90,
        _ => 0,
    }
}

fn bit_count(node: &WidgetNode) -> u32 {
    let start = node.prop_f64("startBit").unwrap_or(0.0).max(0.0) as u32;
    let end = node.prop_f64("endBit").unwrap_or(15.0).max(0.0) as u32;
    end.saturating_sub(start).saturating_add(1).clamp(1, 32)
}

fn bit_names(node: &WidgetNode) -> Vec<String> {
    let raw = node
        .prop_str("trueStrings")
        .or_else(|| node.prop_str("names"))
        .unwrap_or("");
    raw.split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn tab_labels(node: &WidgetNode) -> Vec<String> {
    let mut labels = Vec::new();
    for key in ["itemsHorizontal", "itemsVertical", "items"] {
        if let Some(text) = node.prop_str(key) {
            labels.extend(
                text.split([';', ','])
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string),
            );
        }
    }
    if labels.is_empty() {
        labels.push("Page".into());
    }
    labels
}

fn scan_waveform(
    node: &WidgetNode,
    data: Option<&ChannelData>,
    width: u32,
    height: u32,
) -> VisualKind {
    if let Some(data) = data {
        if width > 0 && height > 0 {
            let samples = data.value.as_f64_slice();
            if samples.len() >= (width * height) as usize {
                return image_visual(
                    "scan2d".into(),
                    width,
                    height,
                    samples_to_rgba(&samples, width, height),
                    None,
                );
            }
        }
    }
    VisualKind::Plot {
        series: vec![data.map(|d| d.value.as_f64_slice()).unwrap_or_default()],
        title: node.prop_str("Title").unwrap_or("").to_string(),
        title_x: String::new(),
        title_y: String::new(),
        x_min: 0.0,
        x_max: 1.0,
        y_min: f64::NAN,
        y_max: f64::NAN,
        y2_min: f64::NAN,
        y2_max: f64::NAN,
        log_y: false,
        x_ticks: 2,
        cursors: Vec::new(),
        legend: Vec::new(),
        colors: Vec::new(),
        axes: Vec::new(),
        styles: Vec::new(),
        grid: node.prop_bool("grid").unwrap_or(true),
        show_x: node.prop_bool("XaxisEnabled").unwrap_or(true),
        show_y: node.prop_bool("YaxisEnabled").unwrap_or(true),
        symbols: Vec::new(),
        symbol_px: symbol_px(node),
        x_limits: x_axis_limits(node),
        log_x: axis_named(node, "XaxisType", "log10"),
        time_x: axis_named(node, "XaxisType", "time"),
        grid_color: prop_color(node, "gridColor").unwrap_or(Color::rgb(160, 160, 160)),
        scale_color: prop_color(node, "scaleColor"),
    }
}

fn image_waveform(
    node: &WidgetNode,
    data: Option<&ChannelData>,
    width: u32,
    height: u32,
) -> VisualKind {
    let note = node
        .prop_str("filename")
        .or_else(|| node.prop_str("channel"))
        .or_else(|| node.prop_str("channelData"))
        .unwrap_or("image")
        .to_string();
    if let Some(PvValue::Bytes(bytes)) = data.map(|d| &d.value) {
        if width > 0 && height > 0 {
            let pack = node
                .prop_str("dataType")
                .or_else(|| node.prop_str("pack"))
                .unwrap_or("");
            let rgba = camera_rgba(bytes, width, height, pack);
            if !rgba.is_empty() {
                return image_visual(note, width, height, rgba, None);
            }
        }
    }
    image_visual(note, 0, 0, Vec::new(), None)
}

fn image_visual(
    note: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    roi: Option<crate::RoiMark>,
) -> VisualKind {
    VisualKind::Image {
        note,
        width,
        height,
        rgba,
        roi,
    }
}

fn curve_symbol(node: &WidgetNode, index: usize) -> CurveSymbol {
    match node.enum_tail(&format!("symbol_{}", index + 1)) {
        Some(name) if name.eq_ignore_ascii_case("Ellipse") || name.eq_ignore_ascii_case("Rect") => {
            CurveSymbol::Ellipse
        }
        Some(name) if name.eq_ignore_ascii_case("Diamond") => CurveSymbol::Diamond,
        Some(name) if name.eq_ignore_ascii_case("Triangle") => CurveSymbol::Triangle,
        Some(name) if name.eq_ignore_ascii_case("DTriangle") => CurveSymbol::DTriangle,
        Some(name) if name.eq_ignore_ascii_case("UTriangle") => CurveSymbol::UTriangle,
        Some(name) if name.eq_ignore_ascii_case("LTriangle") => CurveSymbol::LTriangle,
        Some(name) if name.eq_ignore_ascii_case("RTriangle") => CurveSymbol::RTriangle,
        Some(name) if name.eq_ignore_ascii_case("Cross") => CurveSymbol::Cross,
        Some(name) if name.eq_ignore_ascii_case("XCross") => CurveSymbol::XCross,
        Some(name) if name.eq_ignore_ascii_case("HLine") => CurveSymbol::HLine,
        Some(name) if name.eq_ignore_ascii_case("VLine") => CurveSymbol::VLine,
        Some(name) if name.eq_ignore_ascii_case("Star1") => CurveSymbol::Star1,
        Some(name) if name.eq_ignore_ascii_case("Star2") => CurveSymbol::Star2,
        Some(name) if name.eq_ignore_ascii_case("Hexagon") => CurveSymbol::Hexagon,
        _ => CurveSymbol::NoSymbol,
    }
}

fn symbol_px(node: &WidgetNode) -> i32 {
    let factor = node.prop_f64("symbolResizeFactor").unwrap_or(1.0);
    ((f64::from(node.rect.height.max(1)) * factor / 50.0).round() as i32).max(2)
}

fn axis_named(node: &WidgetNode, key: &str, name: &str) -> bool {
    node.enum_tail(key)
        .is_some_and(|mode| mode.eq_ignore_ascii_case(name))
}

/// `setXaxisLimits` applies the pair only when scaling is not `Auto`.
/// Equal ends become 0..10, matching `setScaleX`.
fn x_axis_limits(node: &WidgetNode) -> Option<(f64, f64)> {
    if node
        .enum_tail("XaxisScaling")
        .is_none_or(|mode| mode.eq_ignore_ascii_case("Auto"))
    {
        return None;
    }
    let text = node.prop_str("XaxisLimits").unwrap_or("0;1");
    let mut parts = text
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let lo: f64 = parts.next()?.parse().ok()?;
    let hi: f64 = parts.next()?.parse().ok()?;
    if lo == hi {
        Some((0.0, 10.0))
    } else {
        Some((lo, hi))
    }
}

fn curve_style(node: &WidgetNode, index: usize) -> CurveStyle {
    match node.enum_tail(&format!("Style_{}", index + 1)) {
        Some(name) if name.eq_ignore_ascii_case("NoCurve") => CurveStyle::NoCurve,
        Some(name) if name.eq_ignore_ascii_case("Sticks") => CurveStyle::Sticks,
        Some(name) if name.eq_ignore_ascii_case("Steps") => CurveStyle::Steps,
        Some(name) if name.eq_ignore_ascii_case("Dots") => CurveStyle::Dots,
        Some(name) if name.eq_ignore_ascii_case("FillUnder") => CurveStyle::FillUnder,
        Some(name) if name.eq_ignore_ascii_case("ThinLines") => CurveStyle::ThinLines,
        Some(name) if name.eq_ignore_ascii_case("HorSticks") => CurveStyle::HorSticks,
        Some(name) if name.eq_ignore_ascii_case("FatDots") => CurveStyle::FatDots,
        _ => CurveStyle::Lines,
    }
}

fn color_map(node: &WidgetNode) -> ColorMap {
    explicit_color_map(node).unwrap_or(ColorMap::Wavelength)
}

fn explicit_color_map(node: &WidgetNode) -> Option<ColorMap> {
    let name = node.enum_tail("ColorMap")?;
    Some(match name {
        name if name.eq_ignore_ascii_case("grey") => ColorMap::Grey,
        name if name.eq_ignore_ascii_case("spectrum_hot") => ColorMap::Hot,
        name if name.eq_ignore_ascii_case("spectrum_heat") => ColorMap::Heat,
        name if name.eq_ignore_ascii_case("spectrum_jet") => ColorMap::Jet,
        name if name.eq_ignore_ascii_case("spectrum_custom") => custom_color_map(node),
        _ => ColorMap::Wavelength,
    })
}

fn custom_color_map(node: &WidgetNode) -> ColorMap {
    let stops: Vec<Color> = node
        .prop_str("customColorMap")
        .unwrap_or("")
        .split([';', ',', ' '])
        .filter_map(|part| part.trim().parse::<i32>().ok())
        .map(qt_global_color)
        .collect();
    if stops.len() < 2 {
        ColorMap::Wavelength
    } else {
        ColorMap::Custom(stops)
    }
}

fn plot_colors(node: &WidgetNode, count: usize) -> Vec<Option<Color>> {
    // `caCartesianPlot` constructor: white, red, yellow, blue, green, magenta.
    const DEFAULTS: [Color; 6] = [
        Color::rgb(255, 255, 255),
        Color::rgb(255, 0, 0),
        Color::rgb(255, 255, 0),
        Color::rgb(0, 0, 255),
        Color::rgb(0, 255, 0),
        Color::rgb(255, 0, 255),
    ];
    (1..=count)
        .map(|index| {
            Some(
                prop_color(node, &format!("color_{index}")).unwrap_or_else(|| {
                    DEFAULTS
                        .get(index - 1)
                        .copied()
                        .unwrap_or(Color::rgb(255, 255, 255))
                }),
            )
        })
        .collect()
}

fn plot_legend(node: &WidgetNode, axes: &[(f64, f64)]) -> Vec<String> {
    if node.prop("LegendEnabled").and_then(PropValue::as_bool) == Some(false) {
        return Vec::new();
    }
    let fixed = node
        .enum_tail("YAxisScaling")
        .is_some_and(|mode| mode.eq_ignore_ascii_case("fixedScale"));
    node_channels(node)
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            if fixed {
                if let Some((lo, hi)) = axes.get(index) {
                    if lo.is_finite() && hi.is_finite() {
                        return format!("{name} [{},{}]", axis_token(*lo), axis_token(*hi));
                    }
                }
            }
            name
        })
        .collect()
}

fn axis_token(value: f64) -> String {
    format!("{value:.1}").trim_end_matches('0').to_string()
}

fn split_prop(node: &WidgetNode, key: &str) -> Vec<String> {
    node.prop_str(key)
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn roi_code(node: &WidgetNode, key: &str) -> u8 {
    match node.enum_tail(key).unwrap_or("") {
        "xy_only" => 1,
        "xy1_xy2" => 2,
        "xyUpleft_xyLowright" => 3,
        "xycenter_width_height" => 4,
        _ => 0,
    }
}

fn roi_marker(node: &WidgetNode) -> u8 {
    match node.enum_tail("ROI_readmarkerType").unwrap_or("") {
        "box_crosshairs" => 1,
        "line" => 2,
        "arrow" => 3,
        _ => 0,
    }
}

/// Two or four values, matching `Callback_WriteDetectedValues`.
fn roi_write_values(kind: u8, x0: f64, y0: f64, x1: f64, y1: f64) -> Option<Vec<f64>> {
    let deltax = -(x0 - x1);
    let deltay = -(y0 - y1);
    match kind {
        1 => Some(vec![x0, y0]),
        2 => Some(vec![x0, y0, x1, y1]),
        3 => {
            let values = if deltax < 0.0 && deltay > 0.0 {
                [x1, y0, x0, y1]
            } else if deltax < 0.0 && deltay < 0.0 {
                [x1, y1, x0, y0]
            } else if deltax > 0.0 && deltay < 0.0 {
                [x0, y1, x1, y0]
            } else {
                [x0, y0, x1, y1]
            };
            Some(values.to_vec())
        }
        4 => {
            let mut x = x0;
            let mut y = y0;
            let mut w = x1 - x0;
            let mut h = y1 - y0;
            if w < 0.0 {
                x += w;
                w = -w;
            }
            if h < 0.0 {
                y += h;
                h = -h;
            }
            Some(vec![x + w / 2.0, y + h / 2.0, w, h])
        }
        _ => None,
    }
}

fn camera_rgba(bytes: &[u8], width: u32, height: u32, pack: &str) -> Vec<u8> {
    let pixels = (width as usize).saturating_mul(height as usize);
    if pixels == 0 {
        return Vec::new();
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    let pack = pack.to_ascii_lowercase();
    if pack == "mono16" && bytes.len() >= pixels * 2 {
        for chunk in bytes.chunks(2).take(pixels) {
            let level = chunk[0];
            rgba.extend_from_slice(&[level, level, level, 255]);
        }
    } else if pack == "rgba" && bytes.len() >= pixels * 4 {
        rgba.extend_from_slice(&bytes[..pixels * 4]);
    } else if pack == "mono8" && bytes.len() >= pixels {
        for byte in bytes.iter().take(pixels) {
            rgba.extend_from_slice(&[*byte, *byte, *byte, 255]);
        }
    } else if pack == "rgb888" || (pack.is_empty() && bytes.len() >= pixels * 3) {
        for chunk in bytes.chunks(3).take(pixels) {
            if chunk.len() < 3 {
                break;
            }
            rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
    } else if bytes.len() >= pixels {
        for byte in bytes.iter().take(pixels) {
            rgba.extend_from_slice(&[*byte, *byte, *byte, 255]);
        }
    }
    rgba
}

fn samples_to_rgba(samples: &[f64], width: u32, height: u32) -> Vec<u8> {
    let pixels = (width as usize).saturating_mul(height as usize);
    let mut rgba = Vec::with_capacity(pixels * 4);
    let max = samples
        .iter()
        .take(pixels)
        .copied()
        .fold(1.0_f64, f64::max)
        .max(1.0);
    for sample in samples.iter().take(pixels) {
        let level = ((*sample / max).clamp(0.0, 1.0) * 255.0) as u8;
        rgba.extend_from_slice(&[level, level, level, 255]);
    }
    rgba
}

fn macro_values(node: &WidgetNode) -> Vec<String> {
    let raw = node.prop_str("macroValues").unwrap_or("");
    raw.split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Left-to-right cells, least significant bit on the left, matching the rasterizer.
pub fn toggled_bit_value(value: u32, count: u32, x: f32, left: i32, width: i32) -> f64 {
    let count = count.clamp(1, 32);
    let span = f64::from(width.max(1));
    let mut index = ((f64::from(x) - f64::from(left)) / span * f64::from(count)).floor() as i32;
    if index < 0 {
        index = 0;
    }
    if index as u32 >= count {
        index = (count - 1) as i32;
    }
    let next = value ^ (1_u32 << index);
    f64::from(next)
}

fn menu_items(node: &WidgetNode, data: Option<&ChannelData>) -> Vec<String> {
    if let Some(data) = data {
        if !data.enum_strings.is_empty() {
            return data.enum_strings.clone();
        }
    }
    let from_prop = node
        .prop("items")
        .map(PropValue::as_string_list)
        .unwrap_or_default();
    if from_prop.is_empty() {
        vec!["0".into(), "1".into()]
    } else {
        from_prop
    }
}

fn related_entries(node: &WidgetNode) -> Vec<(String, String)> {
    let labels = node
        .prop("labels")
        .map(PropValue::as_string_list)
        .unwrap_or_default();
    let files = node
        .prop("files")
        .map(PropValue::as_string_list)
        .unwrap_or_default();
    let n = labels.len().max(files.len());
    (0..n)
        .map(|i| {
            (
                labels.get(i).cloned().unwrap_or_default(),
                files.get(i).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn shell_line(node: &WidgetNode) -> String {
    let command = node
        .prop_str("command")
        .or_else(|| node.prop_str("scriptCommand"))
        .unwrap_or("");
    let args = node.prop_str("scriptParameter").unwrap_or("");
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

fn column_text(node: &WidgetNode, data: Option<&ChannelData>) -> Vec<String> {
    let Some(data) = data else {
        return vec!["---".into()];
    };
    let prec = precision_of(node, Some(data));
    match &data.value {
        PvValue::Doubles(values) => values
            .iter()
            .map(|n| format_number(*n, FormatType::Decimal, prec))
            .collect(),
        PvValue::Strings(values) => values.clone(),
        other => vec![other.as_text()],
    }
}

fn wave_table(node: &WidgetNode, data: Option<&ChannelData>) -> Vec<Vec<String>> {
    let values = column_text(node, data);
    let columns = node.prop_f64("numberOfColumns").unwrap_or(1.0).max(1.0) as usize;
    let rows = node.prop_f64("numberOfRows").map(|n| n.max(1.0) as usize);
    let rows = rows.unwrap_or_else(|| values.len().div_ceil(columns).max(1));
    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    values
                        .get(row * columns + column)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

fn slider_fraction(node: &WidgetNode, fraction: f64) -> f64 {
    let inverted = node
        .prop_str("orientation")
        .is_some_and(|text| text.contains("Down") || text.contains("Left"))
        || node
            .prop_str("invertedAppearance")
            .is_some_and(|text| text == "true" || text == "1");
    if inverted {
        1.0 - fraction
    } else {
        fraction
    }
}

fn split_list(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_numeric_text(class_name: &str) -> bool {
    matches!(
        class_name,
        "caLineEdit" | "caTextEntry" | "caNumeric" | "caApplyNumeric" | "caSpinbox" | "QLCDNumber"
    )
}

fn is_monitor(class_name: &str) -> bool {
    matches!(
        class_name,
        "caLineEdit"
            | "caNumeric"
            | "caApplyNumeric"
            | "caSpinbox"
            | "caSlider"
            | "caLed"
            | "caThermo"
            | "caMeter"
            | "caLinearGauge"
            | "caCircularGauge"
            | "caByte"
            | "caMenu"
            | "caChoice"
            | "caTextEntry"
            | "QLCDNumber"
    )
}

fn clock_parts(now_ms: u64) -> (u32, u32, u32) {
    let secs = now_ms / 1000;
    let tod = secs % 86_400;
    let hour = (tod / 3600) as u32;
    let minute = ((tod % 3600) / 60) as u32;
    let second = (tod % 60) as u32;
    (hour, minute, second)
}

fn format_clock(now_ms: u64) -> String {
    let (hour, minute, second) = clock_parts(now_ms);
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn clamp_entry(node: &WidgetNode, value: f64) -> f64 {
    if node.class_name != "caNumeric" && node.class_name != "caApplyNumeric" {
        return value;
    }
    if node
        .enum_tail("limitsMode")
        .is_some_and(|mode| mode.eq_ignore_ascii_case("User"))
    {
        let min = node.prop_f64("minValue").unwrap_or(f64::NEG_INFINITY);
        let max = node.prop_f64("maxValue").unwrap_or(f64::INFINITY);
        return value.clamp(min, max);
    }
    value
}

fn thermo_bands(
    _node: &WidgetNode,
    data: Option<&ChannelData>,
    min: f64,
    max: f64,
) -> Vec<ThermoBand> {
    if max <= min {
        return Vec::new();
    }
    let Some(data) = data else {
        return Vec::new();
    };
    let mut bands = Vec::new();
    for (at, major) in [
        (data.limits.lower_warning, false),
        (data.limits.upper_warning, false),
        (data.limits.lower_alarm, true),
        (data.limits.upper_alarm, true),
    ] {
        if at.is_finite() {
            bands.push(ThermoBand {
                at: fraction(at, min, max),
                major,
            });
        }
    }
    bands
}

fn axis_pair(node: &WidgetNode, index: u32) -> (f64, f64) {
    let n = index + 1;
    let min = node.prop_f64(&format!("YaxisLimitsMin_{n}"));
    let max = node.prop_f64(&format!("YaxisLimitsMax_{n}"));
    if min.is_some() || max.is_some() {
        return (min.unwrap_or(f64::NAN), max.unwrap_or(f64::NAN));
    }
    if let Some(text) = node.prop_str("YaxisLimits") {
        let nums: Vec<f64> = text
            .split([';', ','])
            .filter_map(|part| part.trim().parse().ok())
            .collect();
        let slot = index as usize * 2;
        if nums.len() >= slot + 2 {
            return (nums[slot], nums[slot + 1]);
        }
    }
    (f64::NAN, f64::NAN)
}

fn waterfall_limits(node: &WidgetNode, rows: &[Vec<f64>]) -> (f64, f64) {
    let min = node.prop_f64("IntensityMin");
    let max = node.prop_f64("IntensityMax");
    if let (Some(min), Some(max)) = (min, max) {
        if max > min {
            return (min, max);
        }
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for row in rows {
        for sample in row {
            lo = lo.min(*sample);
            hi = hi.max(*sample);
        }
    }
    if hi > lo {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

fn choice_items(node: &WidgetNode, data: Option<&ChannelData>, value: f64) -> (Vec<String>, usize) {
    let all = menu_items(node, data);
    let start = node.prop_f64("startBit").unwrap_or(0.0).max(0.0) as usize;
    let end = node
        .prop_f64("endBit")
        .map(|bit| bit.max(0.0) as usize)
        .unwrap_or(all.len().saturating_sub(1));
    let items = if all.is_empty() {
        vec!["0".into(), "1".into()]
    } else if start < all.len() {
        let end = end.min(all.len() - 1);
        if start <= end {
            all[start..=end].to_vec()
        } else {
            all
        }
    } else {
        all
    };
    let selected = value.max(0.0) as usize;
    let selected = selected
        .saturating_sub(start)
        .min(items.len().saturating_sub(1));
    (items, selected)
}

fn choice_layout(node: &WidgetNode, count: usize) -> (u32, bool) {
    let count = count.max(1);
    match node.enum_tail("stackingMode").unwrap_or("Row") {
        "Column" => (count as u32, false),
        "RowInverse" => (1, true),
        "RowColumn" => {
            let lines = (count as f64).sqrt().ceil().max(2.0) as usize;
            (count.div_ceil(lines).max(1) as u32, false)
        }
        _ => (1, false),
    }
}

fn grid_cells(node: &WidgetNode) -> Vec<GridCell> {
    let labels = split_list(node.prop_str("labels"));
    let files = split_list(node.prop_str("files"));
    let args = split_list(node.prop_str("args"));
    let count = if !labels.is_empty() && !files.is_empty() && !args.is_empty() {
        labels.len().min(files.len()).min(args.len())
    } else {
        labels.len().max(files.len())
    };
    (0..count)
        .map(|index| GridCell {
            label: labels.get(index).cloned().unwrap_or_default(),
            file: files.get(index).cloned().unwrap_or_default(),
            args: args.get(index).cloned().unwrap_or_default(),
        })
        .collect()
}

fn rowcol_columns(node: &WidgetNode, count: usize) -> u32 {
    let count = count.max(1);
    match node.enum_tail("stacking").unwrap_or("Row") {
        "Column" => count as u32,
        "RowColumn" => {
            let lines = (count as f64).sqrt().ceil().max(2.0) as usize;
            count.div_ceil(lines).max(1) as u32
        }
        _ => 1,
    }
}

/// Classes the runtime knows how to draw. The list is the contract the CLI
/// prints and the parity notes refer to.
pub fn supported_classes() -> &'static [&'static str] {
    &[
        "caLabel",
        "caLabelVertical",
        "caLineEdit",
        "caTextEntry",
        "caMultiLineString",
        "caNumeric",
        "caApplyNumeric",
        "caSpinbox",
        "caSlider",
        "caMenu",
        "caChoice",
        "caMessageButton",
        "caToggleButton",
        "caRelatedDisplay",
        "caMimeDisplay",
        "caShellCommand",
        "caScriptButton",
        "caGraphics",
        "caPolyLine",
        "caFrame",
        "caInclude",
        "caLed",
        "caLinearGauge",
        "caCircularGauge",
        "caThermo",
        "caMeter",
        "caByte",
        "caByteController",
        "caBitnames",
        "caCartesianPlot",
        "caStripPlot",
        "caWaterfallPlot",
        "caScan2D",
        "caTable",
        "caWaveTable",
        "caCalc",
        "caClock",
        "caImage",
        "caCamera",
        "caLineDraw",
        "caAlarmTree",
        "caAlarmLog",
        "caDoubleTabWidget",
        "caRowColMenu",
        "caReplaceMacro",
        "caHMIConfig",
        "wmSignalRescale",
        "QLabel",
        "QPushButton",
        "QLineEdit",
        "QSlider",
        "QComboBox",
        "QCheckBox",
        "QFrame",
        "QGroupBox",
        "QProgressBar",
        "QDial",
        "QLCDNumber",
        "QSpinBox",
        "QTextEdit",
    ]
}
