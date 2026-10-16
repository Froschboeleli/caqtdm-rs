//! Turns a parsed display plus a [`Hub`](caqtdm_datasource::Hub) into visuals.
//!
//! Channel values, alarm color, visibility, calc, and writes live here so the
//! egui viewer and the software rasterizer share one behavior.

use std::collections::{BTreeMap, VecDeque};

use caqtdm_core::{
    alarm_color, apply_macros, eval_calc, eval_visibility, format_number, CalcInputs, ChannelData,
    Color, FormatType, MacroMap, PropValue, PutValue, PvValue, Severity,
};
use caqtdm_datasource::Hub;
use caqtdm_ui::WidgetNode;

use crate::visual::{Action, Effect, ShapeForm, Visual, VisualKind};

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
    zooms: BTreeMap<String, (f64, f64)>,
    cursors: BTreeMap<String, Vec<f64>>,
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
            | Action::PlaceCursor { id, .. } => *id,
        };
        let Some(node) = self.nodes.get(id as usize).cloned() else {
            return Vec::new();
        };
        match action {
            Action::Press { .. } => self.press(&node),
            Action::SetNumber { value, .. } => {
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
            Action::Zoom { y_min, y_max, .. } => {
                self.zooms.insert(node.name.clone(), (y_min, y_max));
                self.log.push(format!("zoom {} {y_min} {y_max}", node.name));
                Vec::new()
            }
            Action::PlaceCursor { at, .. } => {
                self.cursors.entry(node.name.clone()).or_default().push(at);
                self.log.push(format!("cursor {} {at}", node.name));
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
        if node.class_name == "caMenu"
            || node.class_name == "caChoice"
            || node.class_name == "caRowColMenu"
            || node.class_name == "QComboBox"
        {
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

    fn visual(&self, id: u32, node: &WidgetNode) -> Visual {
        let data = self.primary(node);
        let (foreground, background) = colors(node, data);
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
        }
    }

    fn kind(&self, node: &WidgetNode, data: Option<&ChannelData>) -> VisualKind {
        let value = data.and_then(|d| d.value.as_f64()).unwrap_or(0.0);
        let (min, max) = limits_of(node, data);
        let fraction = fraction(value, min, max);
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
                let items = split_list(node.prop_str("labels").or_else(|| node.prop_str("items")));
                if items.is_empty() {
                    VisualKind::Button
                } else {
                    VisualKind::Menu { items, selected: 0 }
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
            "caThermo" | "QProgressBar" => VisualKind::Thermo { fraction },
            "caMeter" => VisualKind::Meter { fraction },
            "caLed" => VisualKind::Led {
                on: led_on(node, value),
            },
            "caGraphics" => VisualKind::Shape {
                form: shape_form(node),
                filled: node
                    .enum_tail("fillstyle")
                    .is_none_or(|s| !s.eq_ignore_ascii_case("Outline")),
            },
            "caPolyLine" => VisualKind::Polyline {
                points: polyline_points(node),
                closed: node
                    .enum_tail("polystyle")
                    .is_some_and(|s| s.eq_ignore_ascii_case("Polygon")),
            },
            "caCartesianPlot" | "caStripPlot" => {
                let (y_min, y_max) = self.zooms.get(&node.name).copied().unwrap_or((
                    node.prop_f64("YaxisLimitsMin_1").unwrap_or(f64::NAN),
                    node.prop_f64("YaxisLimitsMax_1").unwrap_or(f64::NAN),
                ));
                VisualKind::Plot {
                    series: self.plot_series(node),
                    title: node.prop_str("Title").unwrap_or("").to_string(),
                    title_x: node.prop_str("TitleX").unwrap_or("").to_string(),
                    title_y: node.prop_str("TitleY").unwrap_or("").to_string(),
                    y_min,
                    y_max,
                    x_ticks: node.prop_f64("numberOfXticks").unwrap_or(4.0) as u32,
                    cursors: self.cursors.get(&node.name).cloned().unwrap_or_default(),
                }
            }
            "caScan2D" => scan_kind(
                node,
                data,
                self.axis_count(node, "channelWidth"),
                self.axis_count(node, "channelHeight"),
            ),
            "caWaterfallPlot" => VisualKind::Waterfall {
                rows: self.waterfall_rows(node),
            },
            "caByte" | "caByteController" | "caBitnames" => VisualKind::Bits {
                value: value as u32,
                count: bit_count(node),
                names: bit_names(node),
            },
            "caTable" | "caWaveTable" => VisualKind::Table {
                cells: table_cells(node, data),
            },
            "caMenu" | "caChoice" | "QComboBox" => VisualKind::Menu {
                items: menu_items(node, data),
                selected: value.max(0.0) as usize,
            },
            "caRelatedDisplay" | "caMimeDisplay" => VisualKind::Related {
                entries: related_entries(node),
            },
            "caShellCommand" | "caScriptButton" => VisualKind::Command {
                line: shell_line(node),
            },
            "caClock" => VisualKind::Clock {
                analog: node
                    .enum_tail("timeType")
                    .is_some_and(|t| t.eq_ignore_ascii_case("Analog")),
            },
            "caDoubleTabWidget" | "QTabWidget" => VisualKind::Tabs {
                labels: tab_labels(node),
                current: self.tabs.get(&node.name).copied().unwrap_or_else(|| {
                    node.prop_f64("currentIndex").unwrap_or(0.0).max(0.0) as usize
                }),
            },
            "caFrame" | "QFrame" | "QGroupBox" | "QWidget" | "QMainWindow" | "QDialog" => {
                VisualKind::Frame
            }
            "caImage" | "caCamera" => image_kind(
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
            "caAlarmTree" => VisualKind::Table {
                cells: self.alarm_tree_cells(node),
            },
            "caAlarmLog" => VisualKind::Table {
                cells: self.alarm_rows(data),
            },
            "caLineDraw" => VisualKind::Polyline {
                points: self
                    .history
                    .get(node.prop_str("channel").unwrap_or(""))
                    .map(|s| {
                        s.iter()
                            .enumerate()
                            .map(|(i, v)| caqtdm_core::Point {
                                x: i as i32,
                                y: *v as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                closed: false,
            },
            _ => VisualKind::Unknown,
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
                    return format_number(value, format, precision_of(node, Some(data)));
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
        node_channels(node)
            .first()
            .and_then(|raw| self.waterfall.get(raw))
            .map(|rows| rows.iter().cloned().collect())
            .unwrap_or_default()
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

    fn alarm_tree_cells(&self, node: &WidgetNode) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        fn walk(
            node: &WidgetNode,
            cache: &BTreeMap<String, ChannelData>,
            rows: &mut Vec<Vec<String>>,
        ) {
            if node.class_name != "caAlarmTree" && node.class_name != "caFrame" {
                let channel = node.prop_str("channel").unwrap_or("").to_string();
                let severity = cache
                    .get(&channel)
                    .map(|data| data.severity.name().to_string())
                    .unwrap_or_else(|| "n/a".into());
                rows.push(vec![
                    node.name.clone(),
                    channel,
                    node.prop_str("mask").unwrap_or("").to_string(),
                    severity,
                ]);
            }
            for child in &node.children {
                walk(child, cache, rows);
            }
        }
        walk(node, &self.cache, &mut rows);
        if rows.is_empty() {
            rows.push(vec!["empty".into()]);
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
    ] {
        if let Some(prop) = node.prop(key) {
            if key == "channels" {
                out.extend(prop.as_string_list());
            } else if let Some(text) = prop.as_str() {
                if !text.is_empty() {
                    out.push(text.to_string());
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

fn led_on(node: &WidgetNode, value: f64) -> bool {
    let bit = node.prop_f64("bitNr").unwrap_or(-1.0);
    if bit >= 0.0 {
        let mask = 1u32 << (bit as u32);
        (value as u32) & mask != 0
    } else {
        value != 0.0
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

fn scan_kind(node: &WidgetNode, data: Option<&ChannelData>, width: u32, height: u32) -> VisualKind {
    if let Some(data) = data {
        if width > 0 && height > 0 {
            let samples = data.value.as_f64_slice();
            if samples.len() >= (width * height) as usize {
                return VisualKind::Image {
                    note: "scan2d".into(),
                    width,
                    height,
                    rgba: samples_to_rgba(&samples, width, height),
                };
            }
        }
    }
    VisualKind::Plot {
        series: vec![data.map(|d| d.value.as_f64_slice()).unwrap_or_default()],
        title: node.prop_str("Title").unwrap_or("").to_string(),
        title_x: String::new(),
        title_y: String::new(),
        y_min: f64::NAN,
        y_max: f64::NAN,
        x_ticks: 2,
        cursors: Vec::new(),
    }
}

fn image_kind(
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
                return VisualKind::Image {
                    note,
                    width,
                    height,
                    rgba,
                };
            }
        }
    }
    VisualKind::Image {
        note,
        width: 0,
        height: 0,
        rgba: Vec::new(),
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

fn table_cells(node: &WidgetNode, data: Option<&ChannelData>) -> Vec<Vec<String>> {
    let _ = node;
    let Some(data) = data else {
        return vec![vec!["---".into()]];
    };
    match &data.value {
        PvValue::Doubles(v) => v.iter().map(|n| vec![n.to_string()]).collect(),
        PvValue::Strings(v) => v.iter().map(|s| vec![s.clone()]).collect(),
        other => vec![vec![other.as_text()]],
    }
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

fn format_clock(now_ms: u64) -> String {
    let secs = now_ms / 1000;
    let tod = secs % 86_400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    format!("{hour:02}:{min:02}:{sec:02}")
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
