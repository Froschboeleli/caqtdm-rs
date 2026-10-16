//! Netpbm P3/P6 decoder for `caImage` filenames.
//!
//! caQtDM displays often point `filename` at a small picture. PNG and GIF stay
//! out of this crate; the rasterizer and the viewer both understand PPM, which
//! is enough to test image widgets without an image library.

use crate::VisualKind;

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
        } = &mut visual.kind
        {
            if !rgba.is_empty() || note.is_empty() {
                continue;
            }
            let Some(bytes) = read(note) else {
                continue;
            };
            if let Some((w, h, pixels)) = decode_ppm(&bytes) {
                *width = w;
                *height = h;
                *rgba = pixels;
            }
        }
    }
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
