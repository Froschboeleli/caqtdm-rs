//! TrueType text for the headless rasterizer.
//!
//! Arimo and Cousine are the Chrome metric-compatible faces (SIL Open Font
//! License, see `fonts/OFL.txt`). Noto Sans (same license) and Droid Sans
//! Fallback (Apache-2.0, see `fonts/Apache-2.0.txt`) fill glyphs the selected
//! face does not have, including Greek, Cyrillic, and CJK.
//!
//! fontdue supplies the advance, bearings, and a grayscale bitmap. A fully
//! covered pixel is the widget color. Partial coverage is composited onto the
//! pixel already there, which is the grayscale edge Qt paints instead of a
//! hard 50% cutoff.
//!
//! `-displayFont` selects Arimo, Cousine, Noto, or a `.ttf` path.

use std::cell::RefCell;
use std::sync::OnceLock;

use fontdue::{Font, FontSettings, Metrics};

use crate::Image;
use caqtdm_core::Color;

const ARIMO: &[u8] = include_bytes!("../fonts/Arimo-Regular.ttf");
const COUSINE: &[u8] = include_bytes!("../fonts/Cousine-Regular.ttf");
const NOTO: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
const DROID: &[u8] = include_bytes!("../fonts/DroidSansFallbackFull.ttf");

thread_local! {
    static FACE: RefCell<Font> = RefCell::new(load_bytes(ARIMO).expect("Arimo"));
}

fn load_bytes(bytes: &[u8]) -> Result<Font, String> {
    Font::from_bytes(bytes, FontSettings::default()).map_err(|err| err.to_string())
}

/// Select the face used by later `paint` calls on this thread.
pub fn install_display_font(name: &str) -> Result<(), String> {
    let font = resolve(name)?;
    FACE.with(|face| *face.borrow_mut() = font);
    Ok(())
}

/// Advance width in pixels for `text` at `px`, using the named face.
pub fn measure_text(name: &str, px: f32, text: &str) -> Result<f32, String> {
    let font = resolve(name)?;
    Ok(advance(&font, px, text))
}

/// `(advance, ymin)` of one character. A descender has a negative `ymin`.
pub fn glyph_metrics(name: &str, px: f32, ch: char) -> Result<(f32, i32), String> {
    let font = resolve(name)?;
    let metrics = face_for(Some(&font), ch).metrics(ch, px.max(1.0));
    Ok((metrics.advance_width, metrics.ymin))
}

pub fn draw(image: &mut Image, x: i32, y: i32, text: &str, color: Color, px: i32) {
    let px = (px.max(6) as f32).min(72.0);
    FACE.with(|face| {
        let font = face.borrow();
        let line = font
            .horizontal_line_metrics(px)
            .unwrap_or(fontdue::LineMetrics {
                ascent: px * 0.8,
                descent: -px * 0.2,
                line_gap: 0.0,
                new_line_size: px,
            });
        let baseline = y + line.ascent.round() as i32;
        let mut cursor = x as f32;
        for ch in text.chars().take(80) {
            let chosen = face_for(Some(&font), ch);
            let (metrics, bitmap) = chosen.rasterize(ch, px);
            blit(image, cursor, baseline, &metrics, &bitmap, color);
            cursor += metrics.advance_width;
        }
    });
}

pub fn advance_current(text: &str, px: i32) -> i32 {
    let px = (px.max(6) as f32).min(72.0);
    FACE.with(|face| advance(&face.borrow(), px, text).round() as i32)
}

fn advance(font: &Font, px: f32, text: &str) -> f32 {
    let px = px.max(1.0);
    text.chars()
        .take(80)
        .map(|ch| face_for(Some(font), ch).metrics(ch, px).advance_width)
        .sum()
}

fn blit(
    image: &mut Image,
    cursor: f32,
    baseline: i32,
    metrics: &Metrics,
    bitmap: &[u8],
    color: Color,
) {
    let origin_x = cursor.round() as i32 + metrics.xmin;
    let origin_y = baseline - metrics.ymin - metrics.height as i32;
    for row in 0..metrics.height {
        for col in 0..metrics.width {
            let coverage = bitmap[row * metrics.width + col];
            let x = origin_x + col as i32;
            let y = origin_y + row as i32;
            if coverage == 255 {
                image.put(x, y, color);
            } else if coverage > 0 {
                if let Some(dst) = image.pixel(x, y) {
                    image.put(x, y, composite(dst, color, coverage));
                }
            }
        }
    }
}

fn composite(dst: Color, src: Color, coverage: u8) -> Color {
    let c = u32::from(coverage);
    let inv = 255 - c;
    Color::rgb(
        ((u32::from(src.r) * c + u32::from(dst.r) * inv) / 255) as u8,
        ((u32::from(src.g) * c + u32::from(dst.g) * inv) / 255) as u8,
        ((u32::from(src.b) * c + u32::from(dst.b) * inv) / 255) as u8,
    )
}

fn face_for(primary: Option<&Font>, ch: char) -> &Font {
    if let Some(font) = primary {
        if font.lookup_glyph_index(ch) != 0 {
            return font;
        }
    }
    for font in fallbacks() {
        if font.lookup_glyph_index(ch) != 0 {
            return font;
        }
    }
    primary.unwrap_or(&fallbacks()[0])
}

fn fallbacks() -> &'static [Font] {
    static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            vec![
                load_bytes(NOTO).expect("Noto Sans"),
                load_bytes(DROID).expect("Droid Sans Fallback"),
            ]
        })
        .as_slice()
}

fn resolve(name: &str) -> Result<Font, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || is_arimo(trimmed) {
        return load_bytes(ARIMO);
    }
    if is_cousine(trimmed) {
        return load_bytes(COUSINE);
    }
    if is_noto(trimmed) {
        return load_bytes(NOTO);
    }
    let path = std::path::Path::new(trimmed);
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|err| format!("display font {trimmed}: {err}"))?;
        return load_bytes(&bytes).map_err(|err| format!("display font {trimmed}: {err}"));
    }
    Err(format!("display font '{trimmed}' was not found"))
}

fn is_arimo(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "arimo" | "sans" | "arial" | "helvetica" | "sans-serif"
    )
}

fn is_cousine(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "cousine" | "mono" | "monospace" | "courier"
    )
}

fn is_noto(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "noto" | "noto sans" | "noto-sans"
    )
}
