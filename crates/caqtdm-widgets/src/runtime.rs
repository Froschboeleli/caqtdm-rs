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
                self.cache.insert(name, data);
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
            | Action::Choose { id, .. } => *id,
        };
        let Some(node) = self.nodes.get(id as usize).cloned() else {
            return Vec::new();
        };
        match action {
            Action::Press { .. } => self.press(&node),
            Action::SetNumber { value, .. } => {
                self.write_number(&node, value);
                Vec::new()
            }
            Action::SetText { text, .. } => {
                self.write_text(&node, &text);
                Vec::new()
            }
            Action::Choose { index, .. } => self.choose(&node, index),
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
            || node.class_name == "QComboBox"
        {
            self.write_number(node, index as f64);
            return Vec::new();
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
        if let Err(err) = self.hub.put(raw, &value) {
            self.log.push(err.to_string());
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
            "caMessageButton" | "QPushButton" | "QToolButton" | "caRowColMenu" => {
                VisualKind::Button
            }
            "caToggleButton" | "QCheckBox" | "QRadioButton" => VisualKind::Toggle {
                on: self
                    .toggles
                    .get(&node.name)
                    .copied()
                    .unwrap_or(value != 0.0),
            },
            "caSlider" | "QSlider" | "QScrollBar" => VisualKind::Slider { fraction, min, max },
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
            "caCartesianPlot" | "caStripPlot" | "caScan2D" => VisualKind::Plot {
                series: self.plot_series(node),
            },
            "caWaterfallPlot" => VisualKind::Waterfall {
                rows: self.waterfall_rows(node),
            },
            "caByte" | "caByteController" | "caBitnames" => VisualKind::Bits {
                value: value as u32,
                count: node.prop_f64("endBit").unwrap_or(15.0) as u32 + 1,
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
            "caClock" => VisualKind::Clock,
            "caFrame" | "QFrame" | "QGroupBox" | "QWidget" | "QMainWindow" | "QDialog"
            | "QTabWidget" | "caDoubleTabWidget" => VisualKind::Frame,
            "caImage" | "caCamera" => VisualKind::Image {
                note: node
                    .prop_str("filename")
                    .or_else(|| node.prop_str("channel"))
                    .unwrap_or("image")
                    .to_string(),
            },
            "caCalc" => VisualKind::Label,
            "caInclude" => VisualKind::Frame,
            "caAlarmTree" | "caAlarmLog" => VisualKind::Table {
                cells: alarm_rows(data),
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

    fn primary<'a>(&'a self, node: &WidgetNode) -> Option<&'a ChannelData> {
        node_channels(node)
            .first()
            .and_then(|raw| self.cache.get(raw))
    }

    fn plot_series(&self, node: &WidgetNode) -> Vec<Vec<f64>> {
        node_channels(node)
            .into_iter()
            .map(|raw| {
                self.history
                    .get(&raw)
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default()
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
    for key in ["channel", "channelB", "pv", "enumChannel", "channels"] {
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
    let fg = node
        .prop("foreground")
        .and_then(PropValue::as_color)
        .unwrap_or_else(|| Color::rgb(20, 20, 20));
    let bg = node
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

fn alarm_rows(data: Option<&ChannelData>) -> Vec<Vec<String>> {
    let Some(data) = data else {
        return vec![vec!["disconnected".into()]];
    };
    vec![vec![
        data.description.clone(),
        data.severity.name().to_string(),
        data.value.as_text(),
    ]]
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
