//! Headless rasterizer. Tests and `caqtdm paint` use it so a display can be
//! checked without a GPU. The egui viewer draws the same [`Visual`] list.

use caqtdm_core::{Color, Point, Rect};
use caqtdm_widgets::{
    cell_rect, sample_color, CurveStyle, CurveSymbol, ShapeForm, Visual, VisualKind,
};

mod font;
mod print;

pub use font::{glyph_metrics, install_display_font, measure_text};
pub use print::{print_ps_layout, render_print_pdf, PrintLayout, PRINT_DPI};

/// Pixels kept free on the right of a plot that has a second Y scale.
const Y2_GUTTER: i32 = 22;

/// RGBA image, top-left origin, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, background: Color) -> Self {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            rgba.extend_from_slice(&[background.r, background.g, background.b, background.a]);
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<Color> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return None;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        Some(Color::rgba(
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ))
    }

    pub fn put(&mut self, x: i32, y: i32, color: Color) {
        if color.a == 0 || x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        self.rgba[i] = color.r;
        self.rgba[i + 1] = color.g;
        self.rgba[i + 2] = color.b;
        self.rgba[i + 3] = color.a;
    }

    pub fn cover(&mut self, x: i32, y: i32, color: Color, coverage: u8) {
        if coverage == 0 {
            return;
        }
        if coverage == 255 {
            self.put(x, y, color);
            return;
        }
        if let Some(dst) = self.pixel(x, y) {
            let c = u32::from(coverage);
            let inv = 255 - c;
            self.put(
                x,
                y,
                Color::rgb(
                    ((u32::from(color.r) * c + u32::from(dst.r) * inv) / 255) as u8,
                    ((u32::from(color.g) * c + u32::from(dst.g) * inv) / 255) as u8,
                    ((u32::from(color.b) * c + u32::from(dst.b) * inv) / 255) as u8,
                ),
            );
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                self.put(x, y, color);
            }
        }
    }

    pub fn stroke_rect(&mut self, rect: Rect, color: Color) {
        if rect.width <= 0 || rect.height <= 0 {
            return;
        }
        for x in rect.x..rect.x + rect.width {
            self.put(x, rect.y, color);
            self.put(x, rect.y + rect.height - 1, color);
        }
        for y in rect.y..rect.y + rect.height {
            self.put(rect.x, y, color);
            self.put(rect.x + rect.width - 1, y, color);
        }
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let r2 = radius * radius;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= r2 {
                    self.put(x, y, color);
                }
            }
        }
    }

    pub fn stroke_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let r2 = radius * radius;
        let inner = (radius - 1).max(0) * (radius - 1).max(0);
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
                if d <= r2 && d >= inner {
                    self.put(x, y, color);
                }
            }
        }
    }

    pub fn line_width(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, width: i32, color: Color) {
        let radius = (width.max(1) - 1) / 2;
        if radius == 0 {
            self.line(x0, y0, x1, y1, color);
            return;
        }
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    self.line(x0 + dx, y0 + dy, x1 + dx, y1 + dy, color);
                }
            }
        }
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, color: Color, px: i32) {
        font::draw(self, x, y, text, color, px);
    }

    pub fn to_ppm(&self) -> Vec<u8> {
        let header = format!("P6\n{} {}\n255\n", self.width, self.height);
        let mut out = header.into_bytes();
        for px in self.rgba.chunks(4) {
            out.extend_from_slice(&px[..3]);
        }
        out
    }

    /// PNG snapshot. Upstream's `-savetoimage` writes this format.
    pub fn to_png(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity((self.width * self.height * 3) as usize);
        for px in self.rgba.chunks(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        let Some(image) = image::RgbImage::from_raw(self.width, self.height, rgb) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let _ = image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
        out
    }

    /// Decode a PNG written by [`Self::to_png`]. The grab path uses this to
    /// check the file against the widget-tree raster.
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgb8();
        let width = image.width();
        let height = image.height();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for pixel in image.pixels() {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        Some(Self {
            width,
            height,
            rgba,
        })
    }

    /// PDF page for `printPS`. Qt 5 uses `QPrinter::NativeFormat`, which writes
    /// PDF when the destination is a file.
    pub fn to_print_pdf(&self, caption: &str, full_path: &str) -> Vec<u8> {
        render_print_pdf(self.width, self.height, &self.rgba, caption, full_path)
    }
}

/// Paint every visible widget. The image is large enough to hold them.
pub fn paint(visuals: &[Visual]) -> Image {
    paint_in(visuals, 0, 0)
}

/// Widget-tree raster plus an optional caption. Snapshot PNG is this image.
pub fn grab(visuals: &[Visual], width: u32, height: u32, caption: Option<&str>) -> Image {
    let mut image = paint_in(visuals, width, height);
    if let Some(caption) = caption {
        image.text(0, 0, caption, Color::rgb(255, 255, 255), 8);
    }
    image
}

/// Paint into a canvas of `width` by `height`. A zero size uses the widget bounds.
pub fn paint_in(visuals: &[Visual], width: u32, height: u32) -> Image {
    let (width, height) = if width > 0 && height > 0 {
        (width, height)
    } else {
        bounds(visuals)
    };
    let mut image = Image::new(width, height, Color::rgb(40, 44, 52));
    for visual in visuals.iter().filter(|v| v.visible) {
        paint_one(&mut image, visual);
    }
    image
}

fn stroke_border(image: &mut Image, rect: Rect, color: Color, width: i32) {
    let width = width.max(1);
    for inset in 0..width {
        let band = Rect::new(
            rect.x + inset,
            rect.y + inset,
            rect.width - 2 * inset,
            rect.height - 2 * inset,
        );
        if band.width > 0 && band.height > 0 {
            image.stroke_rect(band, color);
        }
    }
}

fn text_origin_x(visual: &Visual, rect: caqtdm_core::Rect, text: &str) -> i32 {
    if visual.align == 0 {
        return rect.x + 2;
    }
    let advance = font::advance_current(text, visual.font_px);
    let room = rect.width - advance;
    if visual.align == 2 {
        rect.x + room.max(2) - 2
    } else {
        rect.x + room.max(0) / 2
    }
}

fn bounds(visuals: &[Visual]) -> (u32, u32) {
    let mut w = 320;
    let mut h = 200;
    for visual in visuals {
        w = w.max(visual.rect.x + visual.rect.width);
        h = h.max(visual.rect.y + visual.rect.height);
    }
    (w.max(1) as u32, h.max(1) as u32)
}

fn paint_one(image: &mut Image, visual: &Visual) {
    let rect = visual.rect;
    image.fill_rect(rect, visual.background);
    stroke_border(
        image,
        rect,
        visual.border.unwrap_or(visual.foreground),
        visual.border_px,
    );
    match &visual.kind {
        VisualKind::Slider { fraction, .. } => {
            let filled = Rect::new(
                rect.x,
                rect.y,
                ((rect.width as f64) * fraction) as i32,
                rect.height,
            );
            image.fill_rect(filled, visual.foreground);
        }
        VisualKind::Gauge {
            fraction,
            circular: false,
        } => {
            let w = ((rect.width as f64) * fraction) as i32;
            image.fill_rect(Rect::new(rect.x, rect.y, w, rect.height), visual.foreground);
        }
        VisualKind::Thermo { fraction, bands } => {
            let h = ((rect.height as f64) * fraction) as i32;
            let filled = Rect::new(rect.x, rect.y + rect.height - h, rect.width, h);
            image.fill_rect(filled, visual.foreground);
            for band in bands {
                let y = rect.y + rect.height
                    - 1
                    - (band.at.clamp(0.0, 1.0) * f64::from(rect.height - 1)) as i32;
                let color = if band.major {
                    Color::rgb(255, 0, 0)
                } else {
                    Color::rgb(255, 220, 0)
                };
                for x in rect.x + 1..rect.x + rect.width - 1 {
                    image.put(x, y, color);
                }
            }
        }
        VisualKind::Gauge {
            fraction,
            circular: true,
        }
        | VisualKind::Meter { fraction } => {
            let cx = rect.x + rect.width / 2;
            let cy = rect.y + rect.height / 2;
            let radius = rect.width.min(rect.height) / 2 - 2;
            image.fill_circle(cx, cy, radius, visual.background);
            let angle = std::f64::consts::PI * (1.0 - fraction);
            let x1 = cx + (angle.cos() * f64::from(radius)) as i32;
            let y1 = cy - (angle.sin() * f64::from(radius)) as i32;
            image.line(cx, cy, x1, y1, visual.foreground);
        }
        VisualKind::Led { on } => {
            let color = if *on || visual.class_name == "caLed" {
                visual.foreground
            } else {
                Color::rgb(70, 70, 70)
            };
            let r = rect.width.min(rect.height) / 2;
            image.fill_circle(
                rect.x + rect.width / 2,
                rect.y + rect.height / 2,
                r.max(1),
                color,
            );
        }
        VisualKind::Shape {
            form,
            filled,
            start_angle,
            span_angle,
        } => paint_shape(
            image,
            rect,
            *form,
            *filled,
            *start_angle,
            *span_angle,
            visual,
        ),
        VisualKind::Polyline {
            points,
            closed,
            width,
        } => paint_polyline(image, rect, points, *closed, *width, visual.foreground),
        VisualKind::Plot {
            series,
            title,
            title_x,
            title_y,
            x_min,
            x_max,
            y_min,
            y_max,
            y2_min,
            y2_max,
            log_y,
            x_ticks,
            cursors,
            legend,
            colors,
            axes,
            styles,
            grid,
            show_x,
            show_y,
            symbols,
            symbol_px,
            x_limits,
            log_x,
            time_x,
            grid_color,
            scale_color,
            xs,
            align_px,
            time_at,
        } => {
            paint_plot(
                image,
                rect,
                visual,
                &PlotPaint {
                    series,
                    title,
                    title_x,
                    title_y,
                    x_min: *x_min,
                    x_max: *x_max,
                    y_min: *y_min,
                    y_max: *y_max,
                    y2_min: *y2_min,
                    y2_max: *y2_max,
                    log_y: *log_y,
                    x_ticks: *x_ticks,
                    cursors,
                    legend,
                    colors,
                    axes,
                    styles,
                    grid: *grid,
                    show_x: *show_x,
                    show_y: *show_y,
                    symbols,
                    symbol_px: *symbol_px,
                    x_limits: *x_limits,
                    log_x: *log_x,
                    time_x: *time_x,
                    grid_color: *grid_color,
                    scale_color: *scale_color,
                    xs,
                    align_px: *align_px,
                    time_at: *time_at,
                },
            );
        }
        VisualKind::Waterfall {
            rows,
            min,
            max,
            map,
        } => {
            paint_waterfall(image, rect, rows, *min, *max, map);
        }
        VisualKind::Choice {
            items,
            selected,
            columns,
            inverse,
        } => paint_buttons(
            image,
            rect,
            visual,
            items,
            (*columns, *inverse, Some(*selected)),
        ),
        VisualKind::Grid { columns, cells } => {
            let labels: Vec<String> = cells.iter().map(|cell| cell.label.clone()).collect();
            paint_buttons(image, rect, visual, &labels, (*columns, false, None));
        }
        VisualKind::Clock {
            analog: true,
            hour,
            minute,
            second,
        } => paint_clock(image, rect, *hour, *minute, *second, visual.foreground),
        VisualKind::Tabs { labels, current } => {
            paint_tabs(image, rect, visual, labels, *current);
        }
        VisualKind::Menu { items, selected } if visual.class_name == "caChoice" => {
            for (index, item) in items.iter().enumerate() {
                let y = rect.y + 2 + index as i32 * 14;
                let color = if index == *selected {
                    visual.foreground
                } else {
                    Color::rgb(40, 40, 40)
                };
                image.text(rect.x + 2, y, item, color, visual.font_px);
            }
        }
        VisualKind::Bits {
            value,
            count,
            names,
        } => {
            let n = (*count).clamp(1, 32) as i32;
            let w = (rect.width / n).max(2);
            for bit in 0..n {
                let on = value & (1 << bit) != 0;
                let cell = Rect::new(rect.x + bit * w, rect.y + 2, w - 1, rect.height - 4);
                image.fill_rect(
                    cell,
                    if on {
                        visual.foreground
                    } else {
                        Color::rgb(60, 60, 60)
                    },
                );
                if let Some(name) = names.get(bit as usize) {
                    image.text(
                        cell.x + 1,
                        cell.y + 1,
                        name,
                        Color::rgb(255, 255, 255),
                        visual.font_px,
                    );
                }
            }
        }
        VisualKind::Image {
            rgba,
            width,
            height,
            roi,
            ..
        } => {
            blit_rgba(image, rect, rgba, *width, *height);
            if let Some(mark) = roi {
                paint_roi(image, rect, mark, *width, *height);
            }
        }
        VisualKind::Frame => {}
        _ => {}
    }
    if !visual.text.is_empty()
        && !matches!(
            visual.kind,
            VisualKind::Frame
                | VisualKind::Plot { .. }
                | VisualKind::Tabs { .. }
                | VisualKind::Choice { .. }
                | VisualKind::Grid { .. }
                | VisualKind::Waterfall { .. }
                | VisualKind::Clock { analog: true, .. }
        )
    {
        if visual.rotation % 180 == 90 {
            let mut y = rect.y + 2;
            for ch in visual.text.chars().take(24) {
                image.text(
                    rect.x + 2,
                    y,
                    &ch.to_string(),
                    visual.foreground,
                    visual.font_px,
                );
                y += visual.font_px.max(8);
            }
        } else if visual.text.contains('\n') {
            let mut y = rect.y + 2;
            for line in visual.text.lines().take(8) {
                image.text(rect.x + 2, y, line, visual.foreground, visual.font_px);
                y += visual.font_px.max(8);
            }
        } else {
            image.text(
                text_origin_x(visual, rect, &visual.text),
                rect.y + 2,
                &visual.text,
                visual.foreground,
                visual.font_px,
            );
        }
    }
}

struct PlotPaint<'a> {
    series: &'a [Vec<f64>],
    title: &'a str,
    title_x: &'a str,
    title_y: &'a str,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    y2_min: f64,
    y2_max: f64,
    log_y: bool,
    x_ticks: u32,
    cursors: &'a [f64],
    legend: &'a [String],
    colors: &'a [Option<Color>],
    axes: &'a [(f64, f64)],
    styles: &'a [CurveStyle],
    grid: bool,
    show_x: bool,
    show_y: bool,
    symbols: &'a [CurveSymbol],
    symbol_px: i32,
    x_limits: Option<(f64, f64)>,
    log_x: bool,
    time_x: bool,
    grid_color: Color,
    scale_color: Option<Color>,
    xs: &'a [Vec<f64>],
    align_px: i32,
    time_at: f64,
}

fn paint_plot(image: &mut Image, rect: Rect, visual: &Visual, plot_paint: &PlotPaint<'_>) {
    if !plot_paint.title.is_empty() {
        image.text(
            rect.x + 2,
            rect.y + 1,
            plot_paint.title,
            visual.foreground,
            visual.font_px,
        );
    }
    if !plot_paint.title_y.is_empty() {
        image.text(
            rect.x + 2,
            rect.y + 10,
            plot_paint.title_y,
            visual.foreground,
            visual.font_px,
        );
    }
    if !plot_paint.title_x.is_empty() {
        image.text(
            rect.x + 2,
            rect.y + rect.height - 8,
            plot_paint.title_x,
            visual.foreground,
            visual.font_px,
        );
    }
    let y2 = plot_paint.y2_max > plot_paint.y2_min
        && plot_paint.y2_min.is_finite()
        && plot_paint.y2_max.is_finite();
    let gutter = if y2 { Y2_GUTTER } else { 0 };
    let align = plot_paint.align_px.max(0);
    let plot = Rect::new(
        rect.x + 4 + align,
        rect.y + 18,
        rect.width.saturating_sub(8 + gutter + align).max(1),
        rect.height.saturating_sub(28).max(1),
    );
    // A black canvas is the plot constructor color. Scale widgets sit on the
    // foreground palette, so black ticks stay visible beside the canvas. The
    // top band is left alone: legend text uses the series color, and the first
    // constructor series is white.
    let black_canvas = visual.background == Color::rgb(0, 0, 0);
    if black_canvas {
        let left_w = plot.x - rect.x;
        if left_w > 0 {
            image.fill_rect(
                Rect::new(rect.x, plot.y, left_w, plot.height),
                visual.foreground,
            );
        }
        let right = plot.x + plot.width;
        let right_w = rect.x + rect.width - right;
        if right_w > 0 {
            image.fill_rect(
                Rect::new(right, plot.y, right_w, plot.height),
                visual.foreground,
            );
        }
        let bottom = plot.y + plot.height;
        let bottom_h = rect.y + rect.height - bottom;
        if bottom_h > 0 {
            image.fill_rect(
                Rect::new(rect.x, bottom, rect.width, bottom_h),
                visual.foreground,
            );
        }
    }
    image.stroke_rect(plot, visual.foreground);
    let scale = plot_paint.scale_color.unwrap_or(visual.foreground);
    if plot_paint.grid {
        for frac in [0.15_f64, 0.85] {
            let y = plot.y + (frac * f64::from(plot.height - 1)) as i32;
            dash_hline(
                image,
                plot.x + 1,
                plot.x + plot.width - 2,
                y,
                plot_paint.grid_color,
            );
            let x = plot.x + (frac * f64::from(plot.width - 1)) as i32;
            dash_vline(
                image,
                x,
                plot.y + 1,
                plot.y + plot.height - 2,
                plot_paint.grid_color,
            );
        }
    }
    if plot_paint.show_y {
        for frac in [0.0_f64, 0.5, 1.0] {
            let y = plot.y + plot.height - 1 - (frac * f64::from((plot.height - 1).max(1))) as i32;
            image.line(plot.x - 4, y, plot.x - 1, y, scale);
        }
    }
    if y2 {
        paint_y2_axis(
            image,
            plot,
            plot_paint.y2_min,
            plot_paint.y2_max,
            plot_paint.log_y,
            scale,
            visual.font_px,
        );
    }
    if plot_paint.show_x {
        let ticks = plot_paint.x_ticks.clamp(1, 12);
        for tick in 0..ticks {
            let x = plot.x + (plot.width * tick as i32) / ticks as i32;
            if black_canvas {
                let y0 = plot.y + plot.height;
                image.line(x, y0, x, y0 + 3, scale);
            } else {
                image.line(
                    x,
                    plot.y + plot.height - 1,
                    x,
                    plot.y + plot.height - 4,
                    scale,
                );
            }
        }
    }
    for (index, samples) in plot_paint.series.iter().enumerate() {
        let (lo, hi) = series_window(index, plot_paint);
        let color = plot_paint
            .colors
            .get(index)
            .and_then(|color| *color)
            .unwrap_or_else(|| series_color(index, visual.foreground));
        let style = plot_paint
            .styles
            .get(index)
            .copied()
            .unwrap_or(CurveStyle::Lines);
        let symbol = plot_paint
            .symbols
            .get(index)
            .copied()
            .unwrap_or(CurveSymbol::NoSymbol);
        let xs = plot_paint.xs.get(index).map(Vec::as_slice).unwrap_or(&[]);
        paint_series_scaled(
            image,
            plot,
            samples,
            &SeriesDraw {
                color,
                x_min: plot_paint.x_min,
                x_max: plot_paint.x_max,
                y_min: lo,
                y_max: hi,
                log_y: plot_paint.log_y,
                log_x: plot_paint.log_x,
                style,
                symbol,
                symbol_px: plot_paint.symbol_px,
                x_limits: plot_paint.x_limits,
                xs,
            },
        );
    }
    if plot_paint.time_x && plot_paint.title_x.is_empty() {
        let (lo, hi) = plot_paint.x_limits.unwrap_or((0.0, 1.0));
        let (left_ms, right_ms) = if plot_paint.time_at > 0.0 {
            (plot_paint.time_at - 60_000.0, plot_paint.time_at)
        } else {
            (lo, hi)
        };
        let offset = local_offset_secs((right_ms / 1000.0) as i64);
        let px = visual.font_px.clamp(6, 8);
        paint_time_labels(image, rect, left_ms, offset, scale, px, true);
        paint_time_labels(image, rect, right_ms, offset, scale, px, false);
    }
    paint_legend(image, rect, visual, plot_paint);
    for cursor in plot_paint.cursors {
        let x = plot.x + (cursor.clamp(0.0, 1.0) * f64::from(plot.width - 1)) as i32;
        image.line(
            x,
            plot.y,
            x,
            plot.y + plot.height - 1,
            Color::rgb(220, 40, 40),
        );
    }
}

fn series_window(index: usize, plot_paint: &PlotPaint<'_>) -> (f64, f64) {
    if index > 0 {
        if let Some((lo, hi)) = plot_paint.axes.get(index) {
            if hi > lo && lo.is_finite() && hi.is_finite() {
                return (*lo, *hi);
            }
        }
    }
    if index > 0 && plot_paint.y2_max > plot_paint.y2_min {
        (plot_paint.y2_min, plot_paint.y2_max)
    } else {
        (plot_paint.y_min, plot_paint.y_max)
    }
}

fn paint_legend(image: &mut Image, rect: Rect, visual: &Visual, plot_paint: &PlotPaint<'_>) {
    let px = visual.font_px.clamp(6, 10);
    let mut y = rect.y + 1;
    for (index, label) in plot_paint.legend.iter().enumerate() {
        if label.is_empty() || y + px > rect.y + 16 {
            break;
        }
        let color = plot_paint
            .colors
            .get(index)
            .and_then(|color| *color)
            .unwrap_or(visual.foreground);
        let shown: String = label.chars().take(8).collect();
        let width = font::advance_current(&shown, px);
        let x = rect.x + rect.width - width - 2;
        image.text(x, y, &shown, color, px);
        y += px;
    }
}

fn paint_roi(
    image: &mut Image,
    rect: Rect,
    mark: &caqtdm_widgets::RoiMark,
    width: u32,
    height: u32,
) {
    if width == 0 || height == 0 || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let mapx = |value: f64| rect.x + (value / f64::from(width) * f64::from(rect.width)) as i32;
    let mapy = |value: f64| rect.y + (value / f64::from(height) * f64::from(rect.height)) as i32;
    let color = Color::rgb(220, 20, 20);
    if mark.kind == 1 {
        let x = mapx(mark.x0);
        let y = mapy(mark.y0);
        image.line(x, rect.y, x, rect.y + rect.height - 1, color);
        image.line(rect.x, y, rect.x + rect.width - 1, y, color);
        return;
    }
    let x0 = mapx(mark.x0);
    let y0 = mapy(mark.y0);
    let x1 = mapx(mark.x1);
    let y1 = mapy(mark.y1);
    match mark.marker {
        2 => image.line(x0, y0, x1, y1, color),
        3 => {
            image.line(x0, y0, x1, y1, color);
            image.line(x1, y1, x1 - 4, y1 - 3, color);
            image.line(x1, y1, x1 - 4, y1 + 3, color);
        }
        1 => {
            let cx = (x0 + x1) / 2;
            let cy = (y0 + y1) / 2;
            image.line(cx, rect.y, cx, rect.y + rect.height - 1, color);
            image.line(rect.x, cy, rect.x + rect.width - 1, cy, color);
            stroke_box(image, x0, y0, x1, y1, color);
        }
        _ => stroke_box(image, x0, y0, x1, y1, color),
    }
}

fn stroke_box(image: &mut Image, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
    let (left, right) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (top, bottom) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    image.line(left, top, right, top, color);
    image.line(right, top, right, bottom, color);
    image.line(right, bottom, left, bottom, color);
    image.line(left, bottom, left, top, color);
}

fn paint_tabs(image: &mut Image, rect: Rect, visual: &Visual, labels: &[String], current: usize) {
    let mut x = rect.x + 2;
    for (index, label) in labels.iter().enumerate() {
        let width = (font::advance_current(label, visual.font_px) + 8).max(16);
        let tab = Rect::new(x, rect.y + 1, width, 14);
        if index == current {
            image.fill_rect(tab, visual.foreground);
            image.text(x + 2, rect.y + 4, label, visual.background, visual.font_px);
        } else {
            image.stroke_rect(tab, visual.foreground);
            image.text(x + 2, rect.y + 4, label, visual.foreground, visual.font_px);
        }
        x += width + 2;
    }
}

fn blit_rgba(image: &mut Image, rect: Rect, rgba: &[u8], width: u32, height: u32) {
    if width == 0 || height == 0 || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < need {
        return;
    }
    for y in 0..rect.height {
        for x in 0..rect.width {
            let sx = (x as u32 * width) / rect.width as u32;
            let sy = (y as u32 * height) / rect.height as u32;
            let i = ((sy * width + sx) * 4) as usize;
            image.put(
                rect.x + x,
                rect.y + y,
                Color::rgba(rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]),
            );
        }
    }
}

fn paint_shape(
    image: &mut Image,
    rect: Rect,
    form: ShapeForm,
    filled: bool,
    start_angle: i32,
    span_angle: i32,
    visual: &Visual,
) {
    let cx = rect.x + rect.width / 2;
    let cy = rect.y + rect.height / 2;
    let color = visual.foreground;
    match form {
        ShapeForm::Circle => {
            let r = rect.width.min(rect.height) / 2;
            if filled {
                image.fill_circle(cx, cy, r, color);
            } else {
                image.stroke_circle(cx, cy, r.max(1), color);
            }
        }
        ShapeForm::Line => image.line(
            rect.x,
            rect.y,
            rect.x + rect.width - 1,
            rect.y + rect.height - 1,
            color,
        ),
        ShapeForm::Triangle => {
            fill_triangle(
                image,
                [
                    (cx, rect.y + 2),
                    (rect.x + 2, rect.y + rect.height - 3),
                    (rect.x + rect.width - 3, rect.y + rect.height - 3),
                ],
                color,
            );
        }
        ShapeForm::Arrow => {
            let y = cy;
            let x0 = rect.x + 2;
            let x1 = rect.x + rect.width - 3;
            image.line(x0, y, x1, y, color);
            image.line(x1, y, x1 - 8, y - 6, color);
            image.line(x1, y, x1 - 8, y + 6, color);
            if filled {
                fill_triangle(image, [(x1, y), (x1 - 8, y - 6), (x1 - 8, y + 6)], color);
            }
        }
        ShapeForm::Arc => {
            let radius = (rect.width.min(rect.height) / 2 - 2).max(2);
            paint_arc(
                image,
                cx,
                cy,
                radius,
                (start_angle, span_angle, filled),
                color,
            );
        }
        ShapeForm::Rectangle => {
            if filled {
                image.fill_rect(rect, color);
            }
        }
    }
}

fn paint_polyline(
    image: &mut Image,
    rect: Rect,
    points: &[Point],
    closed: bool,
    width: i32,
    color: Color,
) {
    if points.len() < 2 {
        return;
    }
    for pair in points.windows(2) {
        image.line_width(
            rect.x + pair[0].x,
            rect.y + pair[0].y,
            rect.x + pair[1].x,
            rect.y + pair[1].y,
            width,
            color,
        );
    }
    if closed {
        let last = points.len() - 1;
        image.line_width(
            rect.x + points[last].x,
            rect.y + points[last].y,
            rect.x + points[0].x,
            rect.y + points[0].y,
            width,
            color,
        );
    }
}

fn paint_clock(image: &mut Image, rect: Rect, hour: u32, minute: u32, second: u32, color: Color) {
    let cx = rect.x + rect.width / 2;
    let cy = rect.y + rect.height / 2;
    let radius = (rect.width.min(rect.height) / 2 - 2).max(4);
    image.stroke_circle(cx, cy, radius, color);
    let hour_angle = f64::from(hour % 12) * 30.0 + f64::from(minute) * 0.5;
    hand(image, cx, cy, f64::from(radius) * 0.5, hour_angle, color);
    hand(
        image,
        cx,
        cy,
        f64::from(radius) * 0.75,
        f64::from(minute) * 6.0,
        color,
    );
    hand(
        image,
        cx,
        cy,
        f64::from(radius) * 0.85,
        f64::from(second) * 6.0,
        color,
    );
}

fn hand(image: &mut Image, cx: i32, cy: i32, length: f64, degrees_from_12: f64, color: Color) {
    let rad = degrees_from_12.to_radians();
    let x1 = cx + (rad.sin() * length) as i32;
    let y1 = cy - (rad.cos() * length) as i32;
    image.line(cx, cy, x1, y1, color);
}

fn paint_arc(
    image: &mut Image,
    cx: i32,
    cy: i32,
    radius: i32,
    angles: (i32, i32, bool),
    color: Color,
) {
    let (start, span, filled) = angles;
    let span = if span == 0 { 90 } else { span };
    let steps = span.abs().max(1);
    if filled {
        let r2 = radius * radius;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let deg = (dx as f64).atan2(-(dy as f64)).to_degrees();
                if angle_inside(deg, f64::from(start), f64::from(span)) {
                    image.put(x, y, color);
                }
            }
        }
    }
    for step in 0..=steps {
        let deg = f64::from(start) + f64::from(span) * (f64::from(step) / f64::from(steps));
        let rad = deg.to_radians();
        let x = cx + (rad.cos() * f64::from(radius)) as i32;
        let y = cy - (rad.sin() * f64::from(radius)) as i32;
        image.put(x, y, color);
    }
}

fn angle_inside(deg: f64, start: f64, span: f64) -> bool {
    let norm = |mut angle: f64| {
        while angle < 0.0 {
            angle += 360.0;
        }
        while angle >= 360.0 {
            angle -= 360.0;
        }
        angle
    };
    let from = norm(start);
    let delta = norm(deg - from);
    if span >= 0.0 {
        delta <= span
    } else {
        delta >= norm(span)
    }
}

fn fill_triangle(image: &mut Image, points: [(i32, i32); 3], color: Color) {
    let (ax, ay) = points[0];
    let (bx, by) = points[1];
    let (cx, cy) = points[2];
    let min_x = ax.min(bx).min(cx);
    let max_x = ax.max(bx).max(cx);
    let min_y = ay.min(by).min(cy);
    let max_y = ay.max(by).max(cy);
    let area = edge(ax, ay, bx, by, cx, cy);
    if area == 0 {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let w0 = edge(bx, by, cx, cy, x, y);
            let w1 = edge(cx, cy, ax, ay, x, y);
            let w2 = edge(ax, ay, bx, by, x, y);
            let inside = if area > 0 {
                w0 >= 0 && w1 >= 0 && w2 >= 0
            } else {
                w0 <= 0 && w1 <= 0 && w2 <= 0
            };
            if inside {
                image.put(x, y, color);
            }
        }
    }
}

fn edge(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i32 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

fn paint_buttons(
    image: &mut Image,
    rect: Rect,
    visual: &Visual,
    labels: &[String],
    layout: (u32, bool, Option<usize>),
) {
    let (columns, inverse, selected) = layout;
    let count = labels.len();
    for index in 0..count {
        let cell = cell_rect(rect, index, count, columns, inverse);
        let on = selected == Some(index) || selected.is_none();
        let inset = Rect::new(
            cell.x + 1,
            cell.y + 1,
            (cell.width - 2).max(1),
            (cell.height - 2).max(1),
        );
        if on {
            image.fill_rect(inset, visual.foreground);
        } else {
            image.stroke_rect(inset, visual.foreground);
        }
        if let Some(label) = labels.get(index) {
            let ink = if on {
                visual.background
            } else {
                visual.foreground
            };
            image.text(inset.x + 2, inset.y + 2, label, ink, visual.font_px);
        }
    }
}

fn paint_waterfall(
    image: &mut Image,
    rect: Rect,
    rows: &[Vec<f64>],
    min: f64,
    max: f64,
    map: &caqtdm_widgets::ColorMap,
) {
    if rows.is_empty() || rect.width < 2 || rect.height < 2 {
        return;
    }
    let span = if max > min { max - min } else { 1.0 };
    let row_h = (rect.height / rows.len() as i32).max(1);
    for (row_index, row) in rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        let y0 = rect.y + row_index as i32 * row_h;
        let col_w = (rect.width / row.len() as i32).max(1);
        for (col, sample) in row.iter().enumerate() {
            let t = ((sample - min) / span).clamp(0.0, 1.0);
            let color = sample_color(map, t);
            let x0 = rect.x + col as i32 * col_w;
            image.fill_rect(Rect::new(x0, y0, col_w, row_h), color);
        }
    }
}

struct SeriesDraw<'a> {
    color: Color,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    log_y: bool,
    log_x: bool,
    style: CurveStyle,
    symbol: CurveSymbol,
    symbol_px: i32,
    x_limits: Option<(f64, f64)>,
    xs: &'a [f64],
}

fn paint_series_scaled(image: &mut Image, rect: Rect, samples: &[f64], draw: &SeriesDraw<'_>) {
    let bare = draw.style == CurveStyle::NoCurve && draw.symbol == CurveSymbol::NoSymbol;
    if bare || samples.is_empty() || rect.width < 2 || rect.height < 2 {
        return;
    }
    if samples.len() < 2
        && draw.symbol == CurveSymbol::NoSymbol
        && !matches!(
            draw.style,
            CurveStyle::Dots | CurveStyle::FatDots | CurveStyle::Sticks | CurveStyle::HorSticks
        )
    {
        return;
    }
    let last = samples.len() - 1;
    let (start, end) = if draw.x_limits.is_some() || draw.log_x {
        (0, last)
    } else {
        let mut start = (draw.x_min.clamp(0.0, 1.0) * last as f64).round() as usize;
        let mut end = (draw.x_max.clamp(0.0, 1.0) * last as f64).round() as usize;
        if end <= start {
            start = 0;
            end = last;
        }
        (start, end.min(last))
    };
    let window = &samples[start..=end];
    let (min, max) = if draw.y_max > draw.y_min {
        (draw.y_min, draw.y_max)
    } else {
        (
            window.iter().copied().fold(f64::INFINITY, f64::min),
            window.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let span = if max > min { max - min } else { 1.0 };
    let log_min = if draw.log_y && min <= 0.0 { 1e-6 } else { min };
    let log_max = if draw.log_y && max <= log_min {
        log_min * 10.0
    } else {
        max
    };
    let use_xs = draw.xs.len() == samples.len();
    let (axis_lo, axis_hi) = if use_xs && draw.x_limits.is_none() && !draw.log_x {
        xs_axis(draw, &draw.xs[start..=end])
    } else {
        data_axis(draw, last)
    };
    let mut prev: Option<(i32, i32)> = None;
    let denom = (window.len() - 1).max(1) as f64;
    let bottom = rect.y + rect.height - 1;
    for (i, sample) in window.iter().enumerate() {
        let x = if use_xs {
            let value = draw.xs[start + i];
            let frac = (value - axis_lo) / (axis_hi - axis_lo);
            rect.x + (frac.clamp(0.0, 1.0) * f64::from(rect.width - 1)) as i32
        } else if draw.x_limits.is_some() || draw.log_x {
            let value = (start + i) as f64;
            let frac = if draw.log_x {
                log_frac(value, axis_lo, axis_hi)
            } else {
                (value - axis_lo) / (axis_hi - axis_lo)
            };
            rect.x + (frac.clamp(0.0, 1.0) * f64::from(rect.width - 1)) as i32
        } else {
            rect.x + ((i as f64) / denom * f64::from(rect.width - 1)) as i32
        };
        let frac = if draw.log_y {
            let sample = if *sample <= 0.0 { log_min } else { *sample };
            (sample.log10() - log_min.log10()) / (log_max.log10() - log_min.log10())
        } else {
            (sample - min) / span
        };
        let y = bottom - (frac.clamp(0.0, 1.0) * f64::from(rect.height - 1)) as i32;
        match draw.style {
            CurveStyle::NoCurve => {}
            CurveStyle::Dots => image.put(x, y, draw.color),
            CurveStyle::FatDots => {
                image.fill_rect(Rect::new(x - 1, y - 1, 3, 3), draw.color);
            }
            CurveStyle::Sticks => image.line(x, bottom, x, y, draw.color),
            CurveStyle::HorSticks => image.line(rect.x, y, x, y, draw.color),
            CurveStyle::Steps => {
                if let Some((px, py)) = prev {
                    image.line(px, py, x, py, draw.color);
                    image.line(x, py, x, y, draw.color);
                }
            }
            CurveStyle::FillUnder => {
                if let Some((px, py)) = prev {
                    fill_under(image, px, py, x, y, bottom, draw.color);
                }
            }
            CurveStyle::ThinLines => {
                if let Some((px, py)) = prev {
                    image.line(px, py, x, y, draw.color);
                }
            }
            CurveStyle::Lines => {
                if let Some((px, py)) = prev {
                    image.line_width(px, py, x, y, 3, draw.color);
                }
            }
        }
        paint_symbol(image, x, y, draw.symbol, draw.symbol_px, draw.color);
        prev = Some((x, y));
    }
}

fn data_axis(draw: &SeriesDraw<'_>, last: usize) -> (f64, f64) {
    let (lo, hi) = draw.x_limits.unwrap_or((0.0, last as f64));
    let (lo, hi) = if hi > lo { (lo, hi) } else { (0.0, 10.0) };
    let span = hi - lo;
    let zoom_lo = lo + draw.x_min.clamp(0.0, 1.0) * span;
    let zoom_hi = lo + draw.x_max.clamp(0.0, 1.0) * span;
    if zoom_hi > zoom_lo {
        (zoom_lo, zoom_hi)
    } else {
        (lo, hi)
    }
}

fn xs_axis(draw: &SeriesDraw<'_>, xs: &[f64]) -> (f64, f64) {
    let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
    let span = hi - lo;
    let zoom_lo = lo + draw.x_min.clamp(0.0, 1.0) * span;
    let zoom_hi = lo + draw.x_max.clamp(0.0, 1.0) * span;
    if zoom_hi > zoom_lo {
        (zoom_lo, zoom_hi)
    } else {
        (lo, hi)
    }
}

fn log_frac(value: f64, lo: f64, hi: f64) -> f64 {
    let lo = if lo <= 0.0 { 1e-6 } else { lo };
    let hi = if hi <= lo { lo * 10.0 } else { hi };
    let value = if value <= 0.0 { lo } else { value };
    (value.log10() - lo.log10()) / (hi.log10() - lo.log10())
}

/// `hh:mm:ss` and `dd-MM-yy` for `QwtDateScaleDraw` with `Qt::LocalTime`.
/// `offset_secs` is the zone offset from UTC, the same value `tm_gmtoff` carries.
pub fn axis_time_lines(unix_secs: i64, offset_secs: i32) -> (String, String) {
    let local = unix_secs.saturating_add(i64::from(offset_secs));
    let secs = local.rem_euclid(86_400) as u32;
    let z = local.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = (if month <= 2 { y + 1 } else { y }) as i32;
    let hms = format!(
        "{:02}:{:02}:{:02}",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    );
    let date = format!("{day:02}-{month:02}-{:02}", year.rem_euclid(100));
    (hms, date)
}

#[repr(C)]
struct LibcTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const i8,
}

fn local_tm(unix_secs: i64) -> Option<LibcTm> {
    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut LibcTm) -> *mut LibcTm;
    }
    unsafe {
        let mut tm = std::mem::zeroed::<LibcTm>();
        if localtime_r(&unix_secs, &mut tm).is_null() {
            None
        } else {
            Some(tm)
        }
    }
}

/// Zone offset Qt uses for `Qt::LocalTime` on this process.
pub fn local_offset_secs(unix_secs: i64) -> i32 {
    local_tm(unix_secs)
        .map(|tm| tm.tm_gmtoff as i32)
        .unwrap_or(0)
}

/// `QDate::currentDate` and `QTime::currentTime`: year, month, day, hour, minute, second.
pub fn local_civil(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    if let Some(tm) = local_tm(unix_secs) {
        (
            tm.tm_year + 1900,
            tm.tm_mon as u32 + 1,
            tm.tm_mday as u32,
            tm.tm_hour as u32,
            tm.tm_min as u32,
            tm.tm_sec as u32,
        )
    } else {
        let (hms, date) = axis_time_lines(unix_secs, 0);
        let mut clock = hms.split(':');
        let hour = clock.next().unwrap_or("0").parse().unwrap_or(0);
        let minute = clock.next().unwrap_or("0").parse().unwrap_or(0);
        let second = clock.next().unwrap_or("0").parse().unwrap_or(0);
        let mut day = date.split('-');
        let d: u32 = day.next().unwrap_or("1").parse().unwrap_or(1);
        let month: u32 = day.next().unwrap_or("1").parse().unwrap_or(1);
        let yy: i32 = day.next().unwrap_or("70").parse().unwrap_or(70);
        (1900 + yy, month, d, hour, minute, second)
    }
}

fn paint_time_labels(
    image: &mut Image,
    rect: Rect,
    ms: f64,
    offset: i32,
    color: Color,
    px: i32,
    left: bool,
) {
    let (hms, date) = axis_time_lines((ms / 1000.0) as i64, offset);
    let date_y = rect.y + rect.height - px;
    let time_y = date_y - px;
    for (line, y) in [(&hms, time_y), (&date, date_y)] {
        let width = font::advance_current(line, px);
        let x = if left {
            rect.x + 2
        } else {
            rect.x + rect.width - width - 2
        };
        image.text(x, y, line, color, px);
    }
}

/// `Qt::DashLine` at pen width 1 is 4 pixels on, 2 off.
fn dash_on(offset: i32) -> bool {
    offset.rem_euclid(6) < 4
}

fn dash_hline(image: &mut Image, x0: i32, x1: i32, y: i32, color: Color) {
    let (left, right) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    for x in left..=right {
        if dash_on(x - left) {
            image.put(x, y, color);
        }
    }
}

fn dash_vline(image: &mut Image, x: i32, y0: i32, y1: i32, color: Color) {
    let (top, bot) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    for y in top..=bot {
        if dash_on(y - top) {
            image.put(x, y, color);
        }
    }
}

fn paint_symbol(image: &mut Image, x: i32, y: i32, symbol: CurveSymbol, px: i32, color: Color) {
    if symbol == CurveSymbol::NoSymbol {
        return;
    }
    let r = (px / 2).max(1);
    let span = r + 1;
    for dy in -span..=span {
        for dx in -span..=span {
            let coverage = symbol_coverage(symbol, dx, dy, r);
            image.cover(x + dx, y + dy, color, coverage);
        }
    }
}

/// Coverage of one pixel of a `QwtSymbol`. The interior is opaque. The edge
/// fades across one pixel, which is the antialiased pixmap Qwt builds.
fn symbol_coverage(symbol: CurveSymbol, dx: i32, dy: i32, r: i32) -> u8 {
    let rf = f64::from(r);
    match symbol {
        CurveSymbol::NoSymbol => 0,
        CurveSymbol::Ellipse => disk_coverage(dx, dy, rf),
        CurveSymbol::Diamond => filled_coverage(dx.abs() + dy.abs(), r),
        CurveSymbol::Triangle | CurveSymbol::UTriangle => triangle_coverage(dx, dy, r, 0, -1),
        CurveSymbol::DTriangle => triangle_coverage(dx, dy, r, 0, 1),
        CurveSymbol::LTriangle => triangle_coverage(dx, dy, r, -1, 0),
        CurveSymbol::RTriangle => triangle_coverage(dx, dy, r, 1, 0),
        CurveSymbol::Cross | CurveSymbol::Star1 => {
            let arm = stroke_coverage(dx, dy, r) | stroke_coverage(dy, dx, r);
            if symbol == CurveSymbol::Star1 {
                arm | diag_coverage(dx, dy, r) | diag_coverage(dx, -dy, r)
            } else {
                arm
            }
        }
        CurveSymbol::XCross | CurveSymbol::Star2 => {
            let arm = diag_coverage(dx, dy, r) | diag_coverage(dx, -dy, r);
            if symbol == CurveSymbol::Star2 {
                arm | stroke_coverage(dx, dy, r)
            } else {
                arm
            }
        }
        CurveSymbol::HLine => stroke_coverage(dx, dy, r),
        CurveSymbol::VLine => stroke_coverage(dy, dx, r),
        CurveSymbol::Hexagon => {
            let limit = if dy.abs() * 2 <= r {
                r
            } else {
                r - (dy.abs() - r / 2)
            };
            filled_coverage(dx.abs(), limit.max(0))
        }
    }
}

fn disk_coverage(dx: i32, dy: i32, r: f64) -> u8 {
    let d = f64::from(dx * dx + dy * dy).sqrt();
    let edge = (r + 0.5) - d;
    if edge >= 0.5 {
        255
    } else if edge <= -0.5 {
        0
    } else {
        ((edge + 0.5) * 255.0) as u8
    }
}

fn filled_coverage(distance: i32, limit: i32) -> u8 {
    if distance < limit {
        255
    } else if distance == limit {
        180
    } else if distance == limit + 1 {
        70
    } else {
        0
    }
}

fn stroke_coverage(along: i32, across: i32, r: i32) -> u8 {
    if along.abs() > r + 1 {
        return 0;
    }
    match across.abs() {
        0 if along.abs() <= r => 255,
        0 => 80,
        1 if along.abs() <= r => 70,
        _ => 0,
    }
}

fn diag_coverage(dx: i32, dy: i32, r: i32) -> u8 {
    if dx.abs() > r + 1 || dy.abs() > r + 1 {
        return 0;
    }
    match (dx - dy).abs() {
        0 if dx.abs() <= r => 255,
        1 if dx.abs() <= r => 70,
        _ => 0,
    }
}

fn triangle_coverage(dx: i32, dy: i32, r: i32, dir_x: i32, dir_y: i32) -> u8 {
    let (along, across) = if dir_y != 0 {
        (dir_y * dy, dx.abs())
    } else {
        (dir_x * dx, dy.abs())
    };
    let step = along + r / 2;
    if !(0..=r).contains(&step) {
        return if step == -1 || step == r + 1 { 70 } else { 0 };
    }
    let reach = r - step;
    filled_coverage(across, reach)
}

fn fill_under(image: &mut Image, x0: i32, y0: i32, x1: i32, y1: i32, bottom: i32, color: Color) {
    let (left, right, y_left, y_right) = if x0 <= x1 {
        (x0, x1, y0, y1)
    } else {
        (x1, x0, y1, y0)
    };
    let span = (right - left).max(1);
    for x in left..=right {
        let t = f64::from(x - left) / f64::from(span);
        let y = y_left + ((f64::from(y_right - y_left)) * t) as i32;
        let (top, bot) = if y <= bottom {
            (y, bottom)
        } else {
            (bottom, y)
        };
        image.line(x, top, x, bot, color);
    }
}

fn paint_y2_axis(
    image: &mut Image,
    plot: Rect,
    min: f64,
    max: f64,
    log_y: bool,
    color: Color,
    px: i32,
) {
    let right = plot.x + plot.width - 1;
    let span = (plot.height - 1).max(1);
    for frac in [0.0_f64, 0.5, 1.0] {
        let y = plot.y + plot.height - 1 - (frac * f64::from(span)) as i32;
        image.line(right, y, right + 5, y, color);
        let value = if log_y {
            let lo = if min <= 0.0 { 1e-6 } else { min };
            let hi = if max <= lo { lo * 10.0 } else { max };
            10f64.powf(lo.log10() + frac * (hi.log10() - lo.log10()))
        } else {
            min + frac * (max - min)
        };
        image.text(right + 7, y - px.max(6) / 2, &tick_label(value), color, px);
    }
}

fn tick_label(value: f64) -> String {
    if value.fract().abs() < 1e-6 && value.abs() < 1_000_000.0 {
        format!("{value:.0}")
    } else if value.abs() >= 1000.0 || (value != 0.0 && value.abs() < 0.01) {
        format!("{value:.2e}")
    } else {
        format!("{value:.2}")
    }
}

fn series_color(index: usize, first: Color) -> Color {
    const PALETTE: [Color; 4] = [
        Color::rgb(0x4f, 0xc3, 0xf7),
        Color::rgb(0xff, 0xb7, 0x4d),
        Color::rgb(0xce, 0x93, 0xd8),
        Color::rgb(0xa5, 0xd6, 0xa7),
    ];
    if index == 0 {
        first
    } else {
        PALETTE[index % PALETTE.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caqtdm_core::{Point, Severity};
    use caqtdm_widgets::{ShapeForm, VisualKind};

    fn visual(kind: VisualKind, text: &str, fg: Color) -> Visual {
        Visual {
            id: 0,
            class_name: "caLabel".into(),
            name: "v".into(),
            rect: Rect::new(2, 2, 80, 24),
            foreground: fg,
            background: Color::rgb(10, 10, 10),
            text: text.into(),
            kind,
            visible: true,
            severity: Severity::NoAlarm,
            font_px: 10,
            rotation: 0,
            align: 0,
            border: None,
            border_px: 1,
        }
    }

    #[test]
    fn label_text_and_led_are_pixels() {
        let green = Color::rgb(0, 205, 0);
        let image = paint(&[
            visual(VisualKind::Label, "A", green),
            Visual {
                id: 1,
                class_name: "caLed".into(),
                name: "led".into(),
                rect: Rect::new(90, 4, 16, 16),
                foreground: green,
                background: Color::rgb(0, 0, 0),
                text: String::new(),
                kind: VisualKind::Led { on: true },
                visible: true,
                severity: Severity::NoAlarm,
                font_px: 10,
                rotation: 0,
                align: 0,
                border: None,
                border_px: 1,
            },
        ]);
        let mut ink = false;
        for y in 2..24 {
            for x in 2..40 {
                if image.pixel(x, y).unwrap().g == 205 {
                    ink = true;
                }
            }
        }
        assert!(ink, "A was not painted");
        assert!(image.pixel(96, 10).unwrap().g > 100);
        assert!(image.to_ppm().starts_with(b"P6\n"));
    }

    #[test]
    fn ppm_image_is_stretched_into_the_widget() {
        let mut picture = visual(
            VisualKind::Image {
                note: "mark.ppm".into(),
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
                roi: None,
            },
            "",
            Color::rgb(0, 0, 0),
        );
        picture.rect = Rect::new(0, 0, 2, 1);
        picture.class_name = "caImage".into();
        let image = paint(&[picture]);
        assert_eq!(image.pixel(0, 0).unwrap().r, 255);
        assert_eq!(image.pixel(1, 0).unwrap().b, 255);
    }

    #[test]
    fn plot_cursor_and_analog_clock_are_drawn() {
        let mut plot = visual(
            VisualKind::Plot {
                series: vec![vec![0.0, 10.0]],
                title: "t".into(),
                title_x: String::new(),
                title_y: String::new(),
                x_min: 0.0,
                x_max: 1.0,
                y_min: 0.0,
                y_max: 10.0,
                y2_min: f64::NAN,
                y2_max: f64::NAN,
                log_y: false,
                x_ticks: 2,
                cursors: vec![0.0],
                legend: Vec::new(),
                colors: Vec::new(),
                axes: Vec::new(),
                styles: Vec::new(),
                grid: true,
                show_x: true,
                show_y: true,
                symbols: Vec::new(),
                symbol_px: 2,
                x_limits: None,
                log_x: false,
                time_x: false,
                grid_color: Color::rgb(160, 160, 160),
                scale_color: None,
                xs: Vec::new(),
                align_px: 0,
                time_at: 0.0,
            },
            "",
            Color::rgb(0, 180, 0),
        );
        plot.rect = Rect::new(0, 0, 80, 60);
        plot.class_name = "caCartesianPlot".into();
        let mut clock = visual(
            VisualKind::Clock {
                analog: true,
                hour: 0,
                minute: 0,
                second: 0,
            },
            "",
            Color::rgb(0, 180, 0),
        );
        clock.rect = Rect::new(0, 70, 20, 20);
        clock.class_name = "caClock".into();
        let image = paint(&[plot, clock]);
        assert_eq!(image.pixel(4, 30).unwrap().r, 220);
        assert!(image.pixel(10, 72).unwrap().g > 100);
    }

    #[test]
    fn plot_zoom_windows_both_axes() {
        // Plot interior is (4, 18, 72, 32). With y from 0 to 10, sample 2 lands
        // on y=43 and sample 8 lands on y=25, both inside the frame.
        let mut wide = visual(
            VisualKind::Plot {
                series: vec![vec![2.0, 2.0, 2.0, 8.0, 8.0]],
                title: String::new(),
                title_x: String::new(),
                title_y: String::new(),
                x_min: 0.0,
                x_max: 1.0,
                y_min: 0.0,
                y_max: 10.0,
                y2_min: f64::NAN,
                y2_max: f64::NAN,
                log_y: false,
                x_ticks: 1,
                cursors: Vec::new(),
                legend: Vec::new(),
                colors: Vec::new(),
                axes: Vec::new(),
                styles: Vec::new(),
                grid: true,
                show_x: true,
                show_y: true,
                symbols: Vec::new(),
                symbol_px: 2,
                x_limits: None,
                log_x: false,
                time_x: false,
                grid_color: Color::rgb(160, 160, 160),
                scale_color: None,
                xs: Vec::new(),
                align_px: 0,
                time_at: 0.0,
            },
            "",
            Color::rgb(0, 180, 220),
        );
        wide.rect = Rect::new(0, 0, 80, 60);
        wide.class_name = "caCartesianPlot".into();
        let full = paint(&[wide.clone()]);
        assert!(full.pixel(8, 43).unwrap().b > 150);
        assert!(full.pixel(8, 25).unwrap().b < 100);

        if let VisualKind::Plot { x_min, x_max, .. } = &mut wide.kind {
            *x_min = 0.75;
            *x_max = 1.0;
        }
        let x_zoomed = paint(&[wide.clone()]);
        assert!(x_zoomed.pixel(8, 25).unwrap().b > 150, "x window");
        assert!(x_zoomed.pixel(8, 43).unwrap().b < 100, "x window");

        if let VisualKind::Plot {
            x_min,
            x_max,
            y_min,
            y_max,
            ..
        } = &mut wide.kind
        {
            *x_min = 0.0;
            *x_max = 1.0;
            *y_min = 6.0;
            *y_max = 10.0;
        }
        let y_zoomed = paint(&[wide]);
        // Sample 8 moves from y=25 to y=34 when the y window is 6..10.
        assert!(y_zoomed.pixel(60, 34).unwrap().b > 150, "y window");
        assert!(y_zoomed.pixel(60, 25).unwrap().b < 100, "y window");
    }

    fn plot_visual(kind: VisualKind, class_name: &str) -> Visual {
        let mut drawn = visual(kind, "", Color::rgb(0, 180, 220));
        drawn.rect = Rect::new(0, 0, 80, 60);
        drawn.class_name = class_name.into();
        drawn
    }

    fn series_plot(series: Vec<Vec<f64>>, y: (f64, f64), y2: (f64, f64), log_y: bool) -> Visual {
        plot_visual(
            VisualKind::Plot {
                series,
                title: String::new(),
                title_x: String::new(),
                title_y: String::new(),
                x_min: 0.0,
                x_max: 1.0,
                y_min: y.0,
                y_max: y.1,
                y2_min: y2.0,
                y2_max: y2.1,
                log_y,
                x_ticks: 1,
                cursors: Vec::new(),
                legend: Vec::new(),
                colors: Vec::new(),
                axes: Vec::new(),
                styles: Vec::new(),
                grid: true,
                show_x: true,
                show_y: true,
                symbols: Vec::new(),
                symbol_px: 2,
                x_limits: None,
                log_x: false,
                time_x: false,
                grid_color: Color::rgb(160, 160, 160),
                scale_color: None,
                xs: Vec::new(),
                align_px: 0,
                time_at: 0.0,
            },
            "caCartesianPlot",
        )
    }

    #[test]
    fn log_scale_lifts_the_middle_decade() {
        // Interior is (4, 18, 72, 32). Value 10 of 1..100 is near the bottom on a
        // linear axis (y=46) and at mid scale on log10 (y=33).
        let linear = paint(&[series_plot(
            vec![vec![1.0, 10.0, 100.0]],
            (1.0, 100.0),
            (f64::NAN, f64::NAN),
            false,
        )]);
        let logged = paint(&[series_plot(
            vec![vec![1.0, 10.0, 100.0]],
            (1.0, 100.0),
            (f64::NAN, f64::NAN),
            true,
        )]);
        assert!(linear.pixel(39, 47).unwrap().b > 150, "linear decade");
        assert!(linear.pixel(39, 34).unwrap().b < 100, "linear decade");
        assert!(logged.pixel(39, 34).unwrap().b > 150, "log decade");
        assert!(logged.pixel(39, 47).unwrap().b < 100, "log decade");
    }

    #[test]
    fn second_y_axis_places_the_second_curve() {
        let image = paint(&[series_plot(
            vec![vec![2.0, 2.0], vec![500.0, 500.0]],
            (0.0, 10.0),
            (0.0, 1000.0),
            false,
        )]);
        let left = image.pixel(20, 43).unwrap();
        assert!(left.b > 150 && left.r < 80, "first axis");
        let right = image.pixel(20, 34).unwrap();
        assert!(right.r > 200 && right.b < 100, "second axis {right:?}");
        // Right-hand ruler: plot is (4, 18, 50, 32), ticks stick out past x=53.
        let tick = image.pixel(56, 18).unwrap();
        assert!(tick.b > 150, "top tick of the second scale");
        let gap = image.pixel(56, 26).unwrap();
        assert!(gap.b < 40, "gap between right-hand ticks");
        let mut labeled = false;
        for y in 8..28 {
            for x in 58..80 {
                if let Some(pixel) = image.pixel(x, y) {
                    if pixel.b > 150 {
                        labeled = true;
                    }
                }
            }
        }
        assert!(labeled, "second scale label");
    }

    #[test]
    fn strip_window_paints_only_the_kept_samples() {
        // Last two samples are 3 and 4. The left edge sits on 3 (y=40), not on an
        // earlier 1 (y=46).
        let drawn = paint(&[series_plot(
            vec![vec![3.0, 4.0]],
            (0.0, 10.0),
            (f64::NAN, f64::NAN),
            false,
        )]);
        assert!(drawn.pixel(8, 40).unwrap().b > 150, "recent sample");
        assert!(
            drawn.pixel(8, 46).unwrap().b < 100,
            "older sample is outside the span"
        );
    }

    #[test]
    fn shapes_clock_gauges_and_buttons_are_distinct_pixels() {
        let fg = Color::rgb(0, 80, 220);
        let mut triangle = visual(
            VisualKind::Shape {
                form: ShapeForm::Triangle,
                filled: true,
                start_angle: 0,
                span_angle: 90,
            },
            "",
            fg,
        );
        triangle.rect = Rect::new(0, 0, 40, 40);
        let mut arrow = visual(
            VisualKind::Shape {
                form: ShapeForm::Arrow,
                filled: true,
                start_angle: 0,
                span_angle: 90,
            },
            "",
            fg,
        );
        arrow.rect = Rect::new(50, 0, 60, 30);
        let mut arc = visual(
            VisualKind::Shape {
                form: ShapeForm::Arc,
                filled: false,
                start_angle: 0,
                span_angle: 90,
            },
            "",
            fg,
        );
        arc.rect = Rect::new(0, 50, 40, 40);
        let mut clock = visual(
            VisualKind::Clock {
                analog: true,
                hour: 3,
                minute: 0,
                second: 0,
            },
            "",
            fg,
        );
        clock.rect = Rect::new(120, 0, 80, 80);
        let mut gauge = visual(
            VisualKind::Gauge {
                fraction: 0.5,
                circular: false,
            },
            "",
            fg,
        );
        gauge.rect = Rect::new(0, 100, 80, 20);
        let mut dial = visual(
            VisualKind::Gauge {
                fraction: 0.0,
                circular: true,
            },
            "",
            fg,
        );
        dial.rect = Rect::new(90, 100, 80, 80);
        let mut meter = visual(VisualKind::Meter { fraction: 0.0 }, "", fg);
        meter.rect = Rect::new(180, 100, 80, 80);
        let mut thermo = visual(
            VisualKind::Thermo {
                fraction: 0.25,
                bands: vec![caqtdm_widgets::ThermoBand {
                    at: 0.8,
                    major: true,
                }],
            },
            "",
            fg,
        );
        thermo.rect = Rect::new(270, 0, 20, 80);
        let mut choice = visual(
            VisualKind::Choice {
                items: vec!["Off".into(), "On".into()],
                selected: 0,
                columns: 1,
                inverse: false,
            },
            "",
            fg,
        );
        choice.rect = Rect::new(0, 190, 40, 40);
        choice.class_name = "caChoice".into();
        let mut menu = visual(
            VisualKind::Grid {
                columns: 2,
                cells: vec![
                    caqtdm_widgets::GridCell {
                        label: "A".into(),
                        file: "a.ui".into(),
                        args: String::new(),
                    },
                    caqtdm_widgets::GridCell {
                        label: "B".into(),
                        file: "b.ui".into(),
                        args: String::new(),
                    },
                ],
            },
            "",
            fg,
        );
        menu.rect = Rect::new(50, 190, 40, 20);
        menu.class_name = "caRowColMenu".into();
        let mut fall = visual(
            VisualKind::Waterfall {
                rows: vec![vec![0.0, 100.0]],
                min: 0.0,
                max: 100.0,
                map: caqtdm_widgets::ColorMap::Heat,
            },
            "",
            fg,
        );
        fall.rect = Rect::new(100, 190, 40, 20);
        let mut pen = visual(
            VisualKind::Polyline {
                points: vec![Point { x: 2, y: 10 }, Point { x: 36, y: 10 }],
                closed: false,
                width: 3,
            },
            "",
            Color::rgb(255, 0, 0),
        );
        pen.rect = Rect::new(160, 190, 40, 24);
        pen.background = Color::rgb(10, 10, 10);

        let image = paint(&[
            triangle, arrow, arc, clock, gauge, dial, meter, thermo, choice, menu, fall, pen,
        ]);
        assert!(image.pixel(20, 24).unwrap().b > 150, "triangle center");
        assert!(image.pixel(8, 8).unwrap().b < 40, "triangle corner");
        assert!(image.pixel(107, 15).unwrap().b > 150, "arrow tip");
        assert!(image.pixel(70, 4).unwrap().b < 40, "above the shaft");
        assert!(image.pixel(38, 70).unwrap().b > 150, "arc at 3 o'clock");
        assert!(image.pixel(20, 70).unwrap().b < 40, "arc center");
        assert!(image.pixel(172, 40).unwrap().b > 150, "hour hand at 3");
        assert!(image.pixel(160, 24).unwrap().b > 150, "minute hand at 12");
        assert!(image.pixel(148, 40).unwrap().b < 40, "no hand at 9");
        assert!(image.pixel(10, 110).unwrap().b > 150, "linear bar");
        assert!(image.pixel(60, 110).unwrap().b < 40, "empty linear bar");
        assert!(image.pixel(111, 140).unwrap().b > 150, "circular needle");
        assert!(image.pixel(130, 120).unwrap().b < 40, "above the needle");
        assert!(image.pixel(201, 140).unwrap().b > 150, "meter needle");
        assert_eq!(image.pixel(280, 70).unwrap().b, 220, "thermo bar");
        assert_eq!(
            (
                image.pixel(280, 16).unwrap().r,
                image.pixel(280, 16).unwrap().b
            ),
            (255, 0),
            "alarm band"
        );
        assert!(image.pixel(10, 200).unwrap().b > 150, "selected choice");
        assert!(image.pixel(10, 220).unwrap().b < 40, "other choice");
        assert!(image.pixel(55, 200).unwrap().b > 150, "left menu cell");
        assert!(
            image.pixel(70, 200).unwrap().b < 40,
            "gap between menu cells"
        );
        assert!(image.pixel(85, 200).unwrap().b > 150, "right menu cell");
        let cold = image.pixel(110, 200).unwrap();
        let hot = image.pixel(130, 200).unwrap();
        assert!(cold.b > 200 && cold.r < 20, "low intensity");
        assert!(hot.r > 200 && hot.b < 20, "high intensity");
        assert_eq!(image.pixel(180, 200).unwrap().r, 255, "pen");
        assert_eq!(image.pixel(180, 201).unwrap().r, 255, "pen width");
    }

    #[test]
    fn lowercase_is_a_different_glyph_from_uppercase() {
        let (wide, h_ymin) = glyph_metrics("Arimo", 16.0, 'H').unwrap();
        let (narrow, _) = glyph_metrics("Arimo", 16.0, 'i').unwrap();
        let (_, g_ymin) = glyph_metrics("Arimo", 16.0, 'g').unwrap();
        assert!(wide > narrow + 2.0, "proportional advances");
        assert!(h_ymin >= 0, "H sits on the baseline");
        assert!(g_ymin < 0, "g descends below the baseline");
        let (mono_i, _) = glyph_metrics("Cousine", 16.0, 'i').unwrap();
        let (mono_w, _) = glyph_metrics("Cousine", 16.0, 'W').unwrap();
        assert!((mono_i - mono_w).abs() < 0.1, "Cousine is monospace");
        let arimo_i = rightmost_ink("iH", "Arimo");
        let arimo_w = rightmost_ink("WH", "Arimo");
        assert!(
            arimo_w > arimo_i + 4,
            "Arimo paints W further right than i ({arimo_w} vs {arimo_i})"
        );
        let cousine_i = rightmost_ink("iH", "Cousine");
        let cousine_w = rightmost_ink("WH", "Cousine");
        assert!(
            (cousine_i - cousine_w).abs() <= 1,
            "Cousine paints the next glyph at the same advance ({cousine_i} vs {cousine_w})"
        );
        install_display_font("Arimo").unwrap();
    }

    #[test]
    fn unicode_fallback_and_gray_edges() {
        let (advance, _) = glyph_metrics("Arimo", 18.0, '你').unwrap();
        assert!(advance > 1.0, "CJK falls back to a face that has the glyph");
        let (hangul, _) = glyph_metrics("Arimo", 18.0, '한').unwrap();
        assert!(hangul > 1.0, "Hangul falls back to WenQuanYi");
        let (jamo, _) = glyph_metrics("Arimo", 18.0, '\u{1100}').unwrap();
        assert!(jamo > 1.0, "Hangul jamo is in the fallback face");
        let mut label = visual(VisualKind::Label, "你H", Color::rgb(0, 0, 0));
        label.font_px = 20;
        label.rect = Rect::new(2, 2, 160, 36);
        label.background = Color::rgb(255, 255, 255);
        let image = paint(&[label]);
        let mut solid = 0;
        let mut blended = 0;
        for y in 3..37 {
            for x in 3..150 {
                let pixel = image.pixel(x, y).unwrap();
                if pixel.r == 0 && pixel.g == 0 && pixel.b == 0 {
                    solid += 1;
                } else if pixel.r > 8 && pixel.r < 247 && pixel.g == pixel.r && pixel.b == pixel.r {
                    blended += 1;
                }
            }
        }
        assert!(solid > 8, "stem pixels stay the widget color");
        assert!(blended > 4, "partial coverage is composited, not dropped");
        install_display_font("Arimo").unwrap();
    }

    #[test]
    fn snapshot_png_is_the_widget_tree_plus_caption() {
        let label = visual(VisualKind::Label, "A", Color::rgb(0, 180, 0));
        let plain = paint_in(std::slice::from_ref(&label), 120, 40);
        let shot = grab(&[label], 120, 40, Some("1970-01-01 00:00:00, panel.ui"));
        let decoded = Image::from_png(&shot.to_png()).expect("png");
        assert_eq!(decoded.rgba, shot.rgba);
        let mut caption = false;
        for y in 0..12 {
            for x in 0..120 {
                if plain.pixel(x, y) != shot.pixel(x, y) {
                    caption = true;
                }
            }
        }
        assert!(caption, "the caption is drawn on the grab");
        for y in 16..40 {
            for x in 0..120 {
                assert_eq!(
                    plain.pixel(x, y),
                    shot.pixel(x, y),
                    "body pixel {x},{y} left the widget raster"
                );
            }
        }
    }

    fn rightmost_ink(text: &str, face: &str) -> i32 {
        install_display_font(face).unwrap();
        let mut label = visual(VisualKind::Label, text, Color::rgb(0, 220, 0));
        label.font_px = 16;
        label.rect = Rect::new(2, 2, 160, 28);
        label.background = Color::rgb(240, 240, 240);
        let image = paint(&[label]);
        let mut right = 0;
        for y in 3..29 {
            for x in 3..161 {
                let pixel = image.pixel(x, y).unwrap();
                if pixel.g > 180 && pixel.r < 40 {
                    right = right.max(x);
                }
            }
        }
        assert!(right > 4, "{face} {text} painted no glyph");
        right
    }
}
