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
    /// 0 left, 1 center, 2 right. From the widget `alignment` property.
    pub align: i8,
    /// `borderColor` or `frameColor`. Absent keeps the foreground outline.
    pub border: Option<Color>,
    /// `lineWidth`, `borderWidth`, or `frameLineWidth`. 1 is a single pixel.
    pub border_px: i32,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
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
        /// One label per series. Empty draws no legend. A fixed scale appends
        /// `[min,max]` the way `caStripPlot::legendText` does.
        legend: Vec<String>,
        /// `color_1` .. `color_7`. `None` keeps the default series color.
        colors: Vec<Option<Color>>,
        /// Per-series `(min, max)` from `YaxisLimitsMin_N` / `YaxisLimitsMax_N`.
        /// A non-finite pair leaves that series on the primary or second window.
        axes: Vec<(f64, f64)>,
        /// `Style_1` .. `Style_7`. Empty means every series is `Lines`.
        styles: Vec<CurveStyle>,
        /// `grid`. Upstream turns this on in the cartesian constructor.
        grid: bool,
        /// `XaxisEnabled`. False hides the bottom tick marks.
        show_x: bool,
        /// `YaxisEnabled`. False hides the left tick marks.
        show_y: bool,
        /// `symbol_1` .. `symbol_6`. Empty means `NoSymbol`. Upstream maps `Rect` to an ellipse.
        symbols: Vec<CurveSymbol>,
        /// Box size `max(2, round(height * symbolResizeFactor / 50))`.
        symbol_px: i32,
        /// User or Channel `XaxisLimits`. `None` keeps index autoscaling.
        x_limits: Option<(f64, f64)>,
        /// `XaxisType::log10` places the sample index on a log10 axis.
        log_x: bool,
        /// `XaxisType::time` stamps UTC `hh:mm:ss` under the axis.
        time_x: bool,
        /// `gridColor`. Absent is Qt gray.
        grid_color: Color,
        /// `scaleColor` for the tick marks. Absent uses the foreground.
        scale_color: Option<Color>,
    },
    Waterfall {
        rows: Vec<Vec<f64>>,
        min: f64,
        max: f64,
        /// `ColorMap`. Upstream's constructor selects wavelength.
        map: ColorMap,
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
        /// Read-back marker from `ROI_readChannels`. Coordinates are image pixels.
        roi: Option<RoiMark>,
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

/// Marker drawn on a camera or scan image.
///
/// `kind` is `imagewidget.h`'s `ROI_type`: 1 `xy_only`, 2 `xy1_xy2`,
/// 3 `xyUpleft_xyLowright`, 4 `xycenter_width_height`. `marker` is
/// `ROI_markertype`: 0 box, 1 box with crosshairs, 2 line, 3 arrow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoiMark {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub kind: u8,
    pub marker: u8,
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

/// `caCartesianPlot::curvSymbol`. These are geometric stand-ins for `QwtSymbol`.
/// `Rect` is drawn as `Ellipse` because `myMarker` maps it that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveSymbol {
    NoSymbol,
    Ellipse,
    Diamond,
    Triangle,
    DTriangle,
    UTriangle,
    LTriangle,
    RTriangle,
    Cross,
    XCross,
    HLine,
    VLine,
    Star1,
    Star2,
    Hexagon,
}

/// `caCartesianPlot::curvStyle`. `Lines` is the constructor default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveStyle {
    NoCurve,
    Lines,
    Sticks,
    Steps,
    Dots,
    FillUnder,
    ThinLines,
    HorSticks,
    FatDots,
}

/// Spectrogram and camera maps from `colormaps.h`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorMap {
    Grey,
    Wavelength,
    Hot,
    Heat,
    Jet,
    /// Qt global-color indexes from `customColorMap`. Fewer than two stops
    /// falls back to wavelength, matching `setColormap`.
    Custom(Vec<Color>),
}

/// Color at fraction `t` (0..1) of a caQtDM colormap.
pub fn sample_color(map: &ColorMap, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    match map {
        ColorMap::Grey => {
            let v = (t * 255.0).round() as u8;
            Color::rgb(v, v, v)
        }
        ColorMap::Wavelength => wavelength(380.0 + t * 400.0),
        ColorMap::Hot => hot(t),
        ColorMap::Heat => heat(t),
        ColorMap::Jet => jet(t),
        ColorMap::Custom(stops) => {
            if stops.len() < 2 {
                wavelength(380.0 + t * 400.0)
            } else {
                let scaled = t * (stops.len() - 1) as f64;
                let index = (scaled.floor() as usize).min(stops.len() - 2);
                let frac = scaled - index as f64;
                lerp(stops[index], stops[index + 1], frac)
            }
        }
    }
}

/// Recolor a gray image by the red channel. RGB images are left alone.
pub fn colorize_rgba(rgba: &mut [u8], map: &ColorMap) {
    let gray = rgba
        .chunks(4)
        .all(|px| px.len() == 4 && px[0] == px[1] && px[1] == px[2]);
    if !gray {
        return;
    }
    for px in rgba.chunks_mut(4) {
        let color = sample_color(map, f64::from(px[0]) / 255.0);
        px[0] = color.r;
        px[1] = color.g;
        px[2] = color.b;
    }
}

fn lerp(a: Color, b: Color, t: f64) -> Color {
    let mix = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Color::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

fn heat(t: f64) -> Color {
    let b = 255.0 * (4.0 * (0.75 - t)).clamp(0.0, 1.0);
    let r = 255.0 * (4.0 * (t - 0.25)).clamp(0.0, 1.0);
    let g = 255.0 * (4.0 * (t - 0.5).abs() - 1.0).clamp(0.0, 1.0);
    Color::rgb(r as u8, g as u8, b as u8)
}

fn wavelength(wave: f64) -> Color {
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    if (380.0..440.0).contains(&wave) {
        r = -(wave - 440.0) / (440.0 - 380.0);
        b = 1.0;
    } else if (440.0..490.0).contains(&wave) {
        g = (wave - 440.0) / (490.0 - 440.0);
        b = 1.0;
    } else if (490.0..510.0).contains(&wave) {
        g = 1.0;
        b = -(wave - 510.0) / (510.0 - 490.0);
    } else if (510.0..580.0).contains(&wave) {
        r = (wave - 510.0) / (580.0 - 510.0);
        g = 1.0;
    } else if (580.0..645.0).contains(&wave) {
        r = 1.0;
        g = -(wave - 645.0) / (645.0 - 580.0);
    } else if (645.0..=780.0).contains(&wave) {
        r = 1.0;
    }
    let s = if wave > 700.0 {
        0.3 + 0.7 * (780.0 - wave) / (780.0 - 700.0)
    } else if wave < 420.0 {
        0.3 + 0.7 * (wave - 380.0) / (420.0 - 380.0)
    } else {
        1.0
    };
    let chan = |v: f64| (v * s).powf(0.8) * 255.0;
    Color::rgb(chan(r) as u8, chan(g) as u8, chan(b) as u8)
}

fn jet(t: f64) -> Color {
    const STOPS: [(f64, u8, u8, u8); 13] = [
        (0.0, 0, 0, 189),
        (1.0 / 13.0, 0, 0, 255),
        (2.0 / 13.0, 0, 66, 255),
        (3.0 / 13.0, 0, 132, 255),
        (4.0 / 13.0, 0, 189, 255),
        (5.0 / 13.0, 0, 255, 255),
        (6.0 / 13.0, 66, 255, 189),
        (7.0 / 13.0, 132, 255, 132),
        (8.0 / 13.0, 189, 255, 66),
        (9.0 / 13.0, 255, 255, 0),
        (10.0 / 13.0, 255, 189, 0),
        (12.0 / 13.0, 255, 66, 0),
        (1.0, 189, 0, 0),
    ];
    let mut index = 0;
    while index + 1 < STOPS.len() && t > STOPS[index + 1].0 {
        index += 1;
    }
    let (t0, r0, g0, b0) = STOPS[index];
    let (t1, r1, g1, b1) = STOPS[(index + 1).min(STOPS.len() - 1)];
    let frac = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    lerp(Color::rgb(r0, g0, b0), Color::rgb(r1, g1, b1), frac)
}

#[allow(clippy::excessive_precision)]
fn hot(t: f64) -> Color {
    // 64-wide tables from `ColorMap_Hot`. Index `j = (i + 0.5) / 4`.
    const R: [f64; 64] = [
        0.0,
        0.03968253968253968,
        0.07936507936507936,
        0.119047619047619,
        0.1587301587301587,
        0.1984126984126984,
        0.2380952380952381,
        0.2777777777777778,
        0.3174603174603174,
        0.3571428571428571,
        0.3968253968253968,
        0.4365079365079365,
        0.4761904761904762,
        0.5158730158730158,
        0.5555555555555556,
        0.5952380952380952,
        0.6349206349206349,
        0.6746031746031745,
        0.7142857142857142,
        0.753968253968254,
        0.7936507936507936,
        0.8333333333333333,
        0.873015873015873,
        0.9126984126984127,
        0.9523809523809523,
        0.992063492063492,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
    ];
    const G: [f64; 64] = [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.03174603174603163,
        0.0714285714285714,
        0.1111111111111112,
        0.1507936507936507,
        0.1904761904761905,
        0.23015873015873,
        0.2698412698412698,
        0.3095238095238093,
        0.3492063492063491,
        0.3888888888888888,
        0.4285714285714284,
        0.4682539682539679,
        0.5079365079365079,
        0.5476190476190477,
        0.5873015873015872,
        0.6269841269841268,
        0.6666666666666665,
        0.7063492063492065,
        0.746031746031746,
        0.7857142857142856,
        0.8253968253968254,
        0.8650793650793651,
        0.9047619047619047,
        0.9444444444444442,
        0.984126984126984,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
        1.0,
    ];
    const B: [f64; 64] = [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.04761904761904745,
        0.1269841269841265,
        0.2063492063492056,
        0.2857142857142856,
        0.3650793650793656,
        0.4444444444444446,
        0.5238095238095237,
        0.6031746031746028,
        0.6825396825396828,
        0.7619047619047619,
        0.8412698412698409,
        0.92063492063492,
        1.0,
    ];
    let i = (t * 255.0).round() as usize;
    let j = (((i as f64) + 0.5) / 4.0) as usize;
    let j = j.min(63);
    Color::rgb(
        (255.0 * R[j]) as u8,
        (255.0 * G[j]) as u8,
        (255.0 * B[j]) as u8,
    )
}

/// Qt `GlobalColor` indexes 2..18, the range `setColormap` keeps.
pub fn qt_global_color(index: i32) -> Color {
    match index.clamp(2, 18) {
        3 => Color::rgb(255, 255, 255),
        4 => Color::rgb(64, 64, 64),
        5 => Color::rgb(160, 160, 164),
        6 => Color::rgb(192, 192, 192),
        7 => Color::rgb(255, 0, 0),
        8 => Color::rgb(0, 255, 0),
        9 => Color::rgb(0, 0, 255),
        10 => Color::rgb(0, 255, 255),
        11 => Color::rgb(255, 0, 255),
        12 => Color::rgb(255, 255, 0),
        13 => Color::rgb(128, 0, 0),
        14 => Color::rgb(0, 128, 0),
        15 => Color::rgb(0, 0, 128),
        16 => Color::rgb(0, 128, 128),
        17 => Color::rgb(128, 0, 128),
        18 => Color::rgb(128, 128, 0),
        _ => Color::rgb(0, 0, 0),
    }
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
    /// Rubber-band on `caCamera` / `caScan2D`. The corners are the two points
    /// `Callback_WriteDetectedValues` turns into 2 or 4 channel writes.
    Roi {
        id: u32,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
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
