//! Headless rasterizer. Tests and `caqtdm paint` use it so a display can be
//! checked without a GPU. The egui viewer draws the same [`Visual`] list.

use caqtdm_core::{Color, Rect};
use caqtdm_widgets::{ShapeForm, Visual, VisualKind};

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

    pub fn text(&mut self, x: i32, y: i32, text: &str, color: Color) {
        let mut cursor = x;
        for ch in text.chars().take(80) {
            blit_glyph(self, cursor, y, ch, color);
            cursor += 6;
        }
    }

    pub fn to_ppm(&self) -> Vec<u8> {
        let header = format!("P6\n{} {}\n255\n", self.width, self.height);
        let mut out = header.into_bytes();
        for px in self.rgba.chunks(4) {
            out.extend_from_slice(&px[..3]);
        }
        out
    }
}

/// Paint every visible widget. The image is large enough to hold them.
pub fn paint(visuals: &[Visual]) -> Image {
    let (width, height) = bounds(visuals);
    let mut image = Image::new(width, height, Color::rgb(40, 44, 52));
    for visual in visuals.iter().filter(|v| v.visible) {
        paint_one(&mut image, visual);
    }
    image
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
    image.stroke_rect(rect, visual.foreground);
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
        VisualKind::Thermo { fraction }
        | VisualKind::Gauge {
            fraction,
            circular: false,
        } => {
            let h = ((rect.height as f64) * fraction) as i32;
            let filled = Rect::new(rect.x, rect.y + rect.height - h, rect.width, h);
            image.fill_rect(filled, visual.foreground);
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
            let color = if *on {
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
        VisualKind::Shape { form, filled } => paint_shape(image, rect, *form, *filled, visual),
        VisualKind::Polyline { points, closed } => {
            if points.len() >= 2 {
                for pair in points.windows(2) {
                    image.line(
                        rect.x + pair[0].x,
                        rect.y + pair[0].y,
                        rect.x + pair[1].x,
                        rect.y + pair[1].y,
                        visual.foreground,
                    );
                }
                if *closed {
                    let last = points.len() - 1;
                    image.line(
                        rect.x + points[last].x,
                        rect.y + points[last].y,
                        rect.x + points[0].x,
                        rect.y + points[0].y,
                        visual.foreground,
                    );
                }
            }
        }
        VisualKind::Plot {
            series,
            title,
            title_x,
            title_y,
            y_min,
            y_max,
            x_ticks,
            cursors,
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
                    y_min: *y_min,
                    y_max: *y_max,
                    x_ticks: *x_ticks,
                    cursors,
                },
            );
        }
        VisualKind::Clock { analog: true } => {
            let cx = rect.x + rect.width / 2;
            let cy = rect.y + rect.height / 2;
            let radius = (rect.width.min(rect.height) / 2 - 2).max(2);
            image.stroke_circle(cx, cy, radius, visual.foreground);
        }
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
                image.text(rect.x + 2, y, item, color);
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
                    image.text(cell.x + 1, cell.y + 1, name, Color::rgb(255, 255, 255));
                }
            }
        }
        VisualKind::Image {
            rgba,
            width,
            height,
            ..
        } => blit_rgba(image, rect, rgba, *width, *height),
        VisualKind::Frame => {}
        _ => {}
    }
    if !visual.text.is_empty()
        && !matches!(
            visual.kind,
            VisualKind::Frame | VisualKind::Plot { .. } | VisualKind::Tabs { .. }
        )
        && visual.class_name != "caChoice"
    {
        if visual.rotation % 180 == 90 {
            let mut y = rect.y + 2;
            for ch in visual.text.chars().take(24) {
                image.text(rect.x + 2, y, &ch.to_string(), visual.foreground);
                y += visual.font_px.max(8);
            }
        } else if visual.text.contains('\n') {
            let mut y = rect.y + 2;
            for line in visual.text.lines().take(8) {
                image.text(rect.x + 2, y, line, visual.foreground);
                y += visual.font_px.max(8);
            }
        } else {
            image.text(rect.x + 2, rect.y + 2, &visual.text, visual.foreground);
        }
    }
}

struct PlotPaint<'a> {
    series: &'a [Vec<f64>],
    title: &'a str,
    title_x: &'a str,
    title_y: &'a str,
    y_min: f64,
    y_max: f64,
    x_ticks: u32,
    cursors: &'a [f64],
}

fn paint_plot(image: &mut Image, rect: Rect, visual: &Visual, plot_paint: &PlotPaint<'_>) {
    if !plot_paint.title.is_empty() {
        image.text(rect.x + 2, rect.y + 1, plot_paint.title, visual.foreground);
    }
    if !plot_paint.title_y.is_empty() {
        image.text(
            rect.x + 2,
            rect.y + 10,
            plot_paint.title_y,
            visual.foreground,
        );
    }
    if !plot_paint.title_x.is_empty() {
        image.text(
            rect.x + 2,
            rect.y + rect.height - 8,
            plot_paint.title_x,
            visual.foreground,
        );
    }
    let plot = Rect::new(
        rect.x + 4,
        rect.y + 18,
        rect.width.saturating_sub(8).max(1),
        rect.height.saturating_sub(28).max(1),
    );
    image.stroke_rect(plot, visual.foreground);
    let ticks = plot_paint.x_ticks.clamp(1, 12);
    for tick in 0..ticks {
        let x = plot.x + (plot.width * tick as i32) / ticks as i32;
        image.line(
            x,
            plot.y + plot.height - 1,
            x,
            plot.y + plot.height - 4,
            visual.foreground,
        );
    }
    for (index, samples) in plot_paint.series.iter().enumerate() {
        paint_series_scaled(
            image,
            plot,
            samples,
            series_color(index, visual.foreground),
            plot_paint.y_min,
            plot_paint.y_max,
        );
    }
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

fn paint_tabs(image: &mut Image, rect: Rect, visual: &Visual, labels: &[String], current: usize) {
    let mut x = rect.x + 2;
    for (index, label) in labels.iter().enumerate() {
        let width = (label.len() as i32 * 6 + 8).max(16);
        let tab = Rect::new(x, rect.y + 1, width, 14);
        if index == current {
            image.fill_rect(tab, visual.foreground);
            image.text(x + 2, rect.y + 4, label, visual.background);
        } else {
            image.stroke_rect(tab, visual.foreground);
            image.text(x + 2, rect.y + 4, label, visual.foreground);
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

fn paint_shape(image: &mut Image, rect: Rect, form: ShapeForm, filled: bool, visual: &Visual) {
    match form {
        ShapeForm::Circle => {
            let r = rect.width.min(rect.height) / 2;
            if filled {
                image.fill_circle(
                    rect.x + rect.width / 2,
                    rect.y + rect.height / 2,
                    r,
                    visual.foreground,
                );
            } else {
                image.stroke_rect(rect, visual.foreground);
            }
        }
        ShapeForm::Line => image.line(
            rect.x,
            rect.y,
            rect.x + rect.width,
            rect.y + rect.height,
            visual.foreground,
        ),
        _ => {
            if filled {
                image.fill_rect(rect, visual.foreground);
            }
        }
    }
}

fn paint_series_scaled(
    image: &mut Image,
    rect: Rect,
    samples: &[f64],
    color: Color,
    y_min: f64,
    y_max: f64,
) {
    if samples.len() < 2 || rect.width < 2 || rect.height < 2 {
        return;
    }
    let (min, max) = if y_max > y_min {
        (y_min, y_max)
    } else {
        (
            samples.iter().copied().fold(f64::INFINITY, f64::min),
            samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let span = if max > min { max - min } else { 1.0 };
    let mut prev = None;
    for (i, sample) in samples.iter().enumerate() {
        let x = rect.x + ((i as f64) / (samples.len() as f64) * f64::from(rect.width - 1)) as i32;
        let y = rect.y + rect.height
            - 1
            - (((sample - min) / span) * f64::from(rect.height - 1)) as i32;
        if let Some((px, py)) = prev {
            image.line(px, py, x, y, color);
        }
        prev = Some((x, y));
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

fn blit_glyph(image: &mut Image, x: i32, y: i32, ch: char, color: Color) {
    let rows = glyph(ch);
    for (row, bits) in rows.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                image.put(x + col, y + row as i32, color);
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    let ch = ch.to_ascii_uppercase();
    match ch {
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x0E, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '.' => [0, 0, 0, 0, 0, 0x04, 0x04],
        ':' => [0, 0x04, 0x04, 0, 0x04, 0x04, 0],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        '+' => [0, 0x04, 0x04, 0x1F, 0x04, 0x04, 0],
        '/' => [0x01, 0x02, 0x04, 0x08, 0x10, 0, 0],
        '%' => [0x19, 0x1A, 0x02, 0x04, 0x0B, 0x13, 0x13],
        '(' | ')' => [0x04, 0x08, 0x08, 0x08, 0x08, 0x08, 0x04],
        _ => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caqtdm_core::Severity;
    use caqtdm_widgets::VisualKind;

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
            },
        ]);
        assert_eq!(image.pixel(5, 4).unwrap().g, 205);
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
                y_min: 0.0,
                y_max: 10.0,
                x_ticks: 2,
                cursors: vec![0.0],
            },
            "",
            Color::rgb(0, 180, 0),
        );
        plot.rect = Rect::new(0, 0, 80, 60);
        plot.class_name = "caCartesianPlot".into();
        let mut clock = visual(
            VisualKind::Clock { analog: true },
            "",
            Color::rgb(0, 180, 0),
        );
        clock.rect = Rect::new(0, 70, 20, 20);
        clock.class_name = "caClock".into();
        let image = paint(&[plot, clock]);
        assert_eq!(image.pixel(4, 30).unwrap().r, 220);
        assert!(image.pixel(10, 72).unwrap().g > 100);
    }
}
