use caqtdm_core::{Color, Point, Rect, Severity};

/// One painted widget. Coordinates are absolute, in the display's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    pub id: u32,
    pub class_name: String,
    pub name: String,
    pub rect: Rect,
    pub foreground: Color,
    pub background: Color,
    pub text: String,
    pub kind: VisualKind,
    pub visible: bool,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VisualKind {
    Label,
    Entry {
        writable: bool,
    },
    Button,
    Toggle {
        on: bool,
    },
    Slider {
        fraction: f64,
        min: f64,
        max: f64,
    },
    Gauge {
        fraction: f64,
        circular: bool,
    },
    Thermo {
        fraction: f64,
    },
    Meter {
        fraction: f64,
    },
    Led {
        on: bool,
    },
    Shape {
        form: ShapeForm,
        filled: bool,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
    },
    Plot {
        series: Vec<Vec<f64>>,
    },
    Waterfall {
        rows: Vec<Vec<f64>>,
    },
    Bits {
        value: u32,
        count: u32,
    },
    Table {
        cells: Vec<Vec<String>>,
    },
    Menu {
        items: Vec<String>,
        selected: usize,
    },
    Related {
        entries: Vec<(String, String)>,
    },
    Command {
        line: String,
    },
    Clock,
    Frame,
    Image {
        note: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeForm {
    Rectangle,
    Circle,
    Arc,
    Line,
    Triangle,
    Arrow,
}

/// Something the runtime asks the viewer to do after a click or a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Open {
        file: String,
        macros: String,
    },
    Message(String),
    Shell {
        command: String,
        ran: bool,
    },
    /// `caReplaceMacro` asked the viewer to change a macro and reload.
    SetMacro {
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Press { id: u32 },
    SetNumber { id: u32, value: f64 },
    SetText { id: u32, text: String },
    Choose { id: u32, index: usize },
}
