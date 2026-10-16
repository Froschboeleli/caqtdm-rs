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
        /// Alarm and warning marks as fractions of the scale, bottom to top.
        bands: Vec<ThermoBand>,
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
        /// Degrees. 0 is 3 o'clock and positive span runs counter-clockwise.
        start_angle: i32,
        span_angle: i32,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
        width: i32,
    },
    Plot {
        series: Vec<Vec<f64>>,
        title: String,
        title_x: String,
        title_y: String,
        /// Sample-index window as fractions of the series, 0 to 1.
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        /// Second curve scale. NaN means that curve shares `y_min`/`y_max`.
        y2_min: f64,
        y2_max: f64,
        /// `YAxisType::log10`. Placement is log10 of the value over the axis span.
        log_y: bool,
        x_ticks: u32,
        /// Horizontal cursor positions as fractions of the plot width, 0 to 1.
        cursors: Vec<f64>,
    },
    Waterfall {
        rows: Vec<Vec<f64>>,
        min: f64,
        max: f64,
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
    /// `caChoice`: one push button per item.
    Choice {
        items: Vec<String>,
        selected: usize,
        /// How many buttons sit on one row. 1 is a vertical stack.
        columns: u32,
        /// `RowInverse` draws item 0 at the bottom.
        inverse: bool,
    },
    /// `caRowColMenu`: a grid of buttons. Each cell opens a display.
    Grid {
        columns: u32,
        cells: Vec<GridCell>,
    },
    Related {
        entries: Vec<(String, String)>,
    },
    Command {
        line: String,
    },
    Clock {
        analog: bool,
        hour: u32,
        minute: u32,
        second: u32,
    },
    /// `caDoubleTabWidget` / `QTabWidget`. `current` selects the visible page.
    Tabs {
        labels: Vec<String>,
        current: usize,
    },
    Frame,
    /// `caAlarmTree`. A collapsed group omits its children.
    Tree {
        rows: Vec<TreeRow>,
    },
    Image {
        note: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Unknown,
}

/// One mark on a thermometer scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermoBand {
    pub at: f64,
    /// Major alarm marks are red. Warning marks are yellow.
    pub major: bool,
}

/// One button in a row/column menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    pub label: String,
    pub file: String,
    pub args: String,
}

/// One row of an alarm tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub name: String,
    pub depth: u32,
    pub group: bool,
    pub open: bool,
    pub cells: Vec<String>,
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
    /// Rubber-band zoom. `x_min`/`x_max` are fractions of the sample index.
    /// `y_min`/`y_max` are data values. Both replace the plot window.
    Zoom {
        id: u32,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    },
    /// Drop a vertical cursor. `at` is a fraction of the plot width.
    PlaceCursor {
        id: u32,
        at: f64,
    },
    /// Expand or collapse one alarm-tree group.
    ToggleGroup {
        id: u32,
        index: usize,
    },
}

/// Button rectangle for choice buttons and row/column menus.
///
/// `columns` is how many buttons share a row. A single column fills downward.
/// `RowColumn` stacking fills downward and then across. `inverse` puts index 0
/// on the last row of a one-column stack.
pub fn cell_rect(outer: Rect, index: usize, count: usize, columns: u32, inverse: bool) -> Rect {
    let count = count.max(1);
    let columns = (columns.max(1) as usize).min(count);
    let rows = count.div_ceil(columns);
    let (col, row) = if inverse && columns == 1 {
        (0, count.saturating_sub(1).saturating_sub(index))
    } else if rows > 1 && columns > 1 {
        (index / rows, index % rows)
    } else {
        (index % columns, index / columns)
    };
    let cw = (outer.width / columns as i32).max(1);
    let rh = (outer.height / rows as i32).max(1);
    Rect::new(outer.x + col as i32 * cw, outer.y + row as i32 * rh, cw, rh)
}

/// Index of the button under a point. The last button wins when the point
/// misses every cell.
pub fn cell_at(outer: Rect, x: f32, y: f32, count: usize, columns: u32, inverse: bool) -> usize {
    if count == 0 {
        return 0;
    }
    for index in 0..count {
        if cell_rect(outer, index, count, columns, inverse).contains(x, y) {
            return index;
        }
    }
    count - 1
}
