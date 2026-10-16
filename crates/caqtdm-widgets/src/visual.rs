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
    /// Pixel size used by the rasterizer. Comes from the font or from font scaling.
    pub font_px: i32,
    /// Degrees clockwise. `caLabelVertical` uses 90.
    pub rotation: i32,
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
        title: String,
        title_x: String,
        title_y: String,
        y_min: f64,
        y_max: f64,
        x_ticks: u32,
        /// Horizontal cursor positions as fractions of the plot width, 0 to 1.
        cursors: Vec<f64>,
    },
    Waterfall {
        rows: Vec<Vec<f64>>,
    },
    Bits {
        value: u32,
        count: u32,
        names: Vec<String>,
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
    Clock {
        analog: bool,
    },
    /// `caDoubleTabWidget` / `QTabWidget`. `current` selects the visible page.
    Tabs {
        labels: Vec<String>,
        current: usize,
    },
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
    Press {
        id: u32,
    },
    SetNumber {
        id: u32,
        value: f64,
    },
    SetText {
        id: u32,
        text: String,
    },
    Choose {
        id: u32,
        index: usize,
    },
    /// Rubber-band zoom. Replaces the plot's Y window until the next zoom.
    Zoom {
        id: u32,
        y_min: f64,
        y_max: f64,
    },
    /// Drop a vertical cursor. `at` is a fraction of the plot width.
    PlaceCursor {
        id: u32,
        at: f64,
    },
}
