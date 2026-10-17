//! Image decoders for `caImage` filenames.
//!
//! PPM is decoded here. PNG and GIF go through the `image` crate. A small SVG
//! subset (rect, circle, line) is painted directly so displays do not need a
//! full SVG engine.

use crate::VisualKind;

/// Decode PPM, PNG, GIF, or a small SVG (rect, circle, line) into straight RGBA.
pub fn decode_image(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.starts_with(b"P3") || bytes.starts_with(b"P6") {
        return decode_ppm(bytes);
    }
    if bytes.starts_with(b"<svg") || bytes.windows(4).any(|w| w == b"<svg") {
        return decode_svg(bytes);
    }
    let image = image::load_from_memory(bytes).ok()?;
    let rgba = image.to_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Decode a binary (P6) or ASCII (P3) PPM into straight RGBA.
pub fn decode_ppm(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut cursor = Cursor { data: bytes, i: 0 };
    let magic = cursor.token()?;
    let binary = match magic {
        b"P6" => true,
        b"P3" => false,
        _ => return None,
    };
    let width: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
    let height: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
    let maxval: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
    if width == 0 || height == 0 || maxval == 0 || maxval > 255 {
        return None;
    }
    let pixels = (width as usize).checked_mul(height as usize)?;
    let mut rgba = Vec::with_capacity(pixels * 4);
    if binary {
        if cursor.i < cursor.data.len() && cursor.data[cursor.i].is_ascii_whitespace() {
            cursor.i += 1;
        }
        let need = pixels.checked_mul(3)?;
        let rest = cursor.data.get(cursor.i..cursor.i + need)?;
        for chunk in rest.chunks(3) {
            rgba.push(scale(chunk[0] as u32, maxval));
            rgba.push(scale(chunk[1] as u32, maxval));
            rgba.push(scale(chunk[2] as u32, maxval));
            rgba.push(255);
        }
    } else {
        for _ in 0..pixels {
            let r: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
            let g: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
            let b: u32 = std::str::from_utf8(cursor.token()?).ok()?.parse().ok()?;
            rgba.push(scale(r, maxval));
            rgba.push(scale(g, maxval));
            rgba.push(scale(b, maxval));
            rgba.push(255);
        }
    }
    Some((width, height, rgba))
}

/// Fill `VisualKind::Image` rasters by asking `read` for the file bytes.
pub fn load_image_notes(
    visuals: &mut [crate::Visual],
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
) {
    for visual in visuals {
        if let VisualKind::Image {
            note,
            width,
            height,
            rgba,
            ..
        } = &mut visual.kind
        {
            if !rgba.is_empty() || note.is_empty() {
                continue;
            }
            let Some(bytes) = read(note) else {
                continue;
            };
            if let Some((w, h, pixels)) = decode_image(&bytes) {
                *width = w;
                *height = h;
                *rgba = pixels;
            }
        }
    }
}

fn decode_svg(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let width = svg_attr(text, "svg", "width").unwrap_or(16).clamp(1, 256);
    let height = svg_attr(text, "svg", "height").unwrap_or(16).clamp(1, 256);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for tag in ["rect", "circle", "line"] {
        let mut rest = text;
        while let Some(start) = rest.find(&format!("<{tag}")) {
            let slice = &rest[start..];
            let end = slice.find('>').unwrap_or(slice.len());
            let element = &slice[..end];
            paint_svg(&mut rgba, width, height, tag, element);
            rest = &slice[end..];
        }
    }
    Some((width as u32, height as u32, rgba))
}

fn paint_svg(rgba: &mut [u8], width: i32, height: i32, tag: &str, element: &str) {
    let color = svg_fill(element);
    match tag {
        "rect" => {
            let x = svg_attr(element, "", "x").unwrap_or(0);
            let y = svg_attr(element, "", "y").unwrap_or(0);
            let w = svg_attr(element, "", "width").unwrap_or(1);
            let h = svg_attr(element, "", "height").unwrap_or(1);
            for py in y..(y + h) {
                for px in x..(x + w) {
                    put_svg(rgba, width, height, px, py, color);
                }
            }
        }
        "circle" => {
            let cx = svg_attr(element, "", "cx").unwrap_or(0);
            let cy = svg_attr(element, "", "cy").unwrap_or(0);
            let r = svg_attr(element, "", "r").unwrap_or(1);
            for py in (cy - r)..=(cy + r) {
                for px in (cx - r)..=(cx + r) {
                    let dx = px - cx;
                    let dy = py - cy;
                    if dx * dx + dy * dy <= r * r {
                        put_svg(rgba, width, height, px, py, color);
                    }
                }
            }
        }
        "line" => {
            let x1 = svg_attr(element, "", "x1").unwrap_or(0);
            let y1 = svg_attr(element, "", "y1").unwrap_or(0);
            let x2 = svg_attr(element, "", "x2").unwrap_or(0);
            let y2 = svg_attr(element, "", "y2").unwrap_or(0);
            let steps = (x2 - x1).abs().max((y2 - y1).abs()).max(1);
            for step in 0..=steps {
                let x = x1 + (x2 - x1) * step / steps;
                let y = y1 + (y2 - y1) * step / steps;
                put_svg(rgba, width, height, x, y, color);
            }
        }
        _ => {}
    }
}

fn put_svg(rgba: &mut [u8], width: i32, height: i32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let i = ((y * width + x) * 4) as usize;
    rgba[i..i + 4].copy_from_slice(&color);
}

fn svg_fill(element: &str) -> [u8; 4] {
    let Some(raw) = attr_value(element, "fill").or_else(|| attr_value(element, "stroke")) else {
        return [0, 0, 0, 255];
    };
    let color = caqtdm_core::Color::parse(raw).unwrap_or(caqtdm_core::Color::rgb(0, 0, 0));
    [color.r, color.g, color.b, 255]
}

fn svg_attr(text: &str, tag: &str, name: &str) -> Option<i32> {
    let scope = if tag.is_empty() {
        text
    } else {
        let start = text.find(&format!("<{tag}"))?;
        let slice = &text[start..];
        let end = slice.find('>').unwrap_or(slice.len());
        &slice[..end]
    };
    attr_value(scope, name)?.trim_end_matches("px").parse().ok()
}

fn attr_value<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = element.find(&key)? + key.len();
    let rest = &element[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn scale(sample: u32, maxval: u32) -> u8 {
    if maxval == 255 {
        sample.min(255) as u8
    } else {
        (sample.min(maxval) * 255 / maxval) as u8
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn token(&mut self) -> Option<&'a [u8]> {
        self.skip();
        let start = self.i;
        while self.i < self.data.len() {
            let byte = self.data[self.i];
            if byte.is_ascii_whitespace() || byte == b'#' {
                break;
            }
            self.i += 1;
        }
        if self.i == start {
            None
        } else {
            Some(&self.data[start..self.i])
        }
    }

    fn skip(&mut self) {
        loop {
            while self.i < self.data.len() && self.data[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.i < self.data.len() && self.data[self.i] == b'#' {
                while self.i < self.data.len() && self.data[self.i] != b'\n' {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }
}
