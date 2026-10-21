//! TrueType text for the headless rasterizer.
//!
//! Arimo and Cousine are the Chrome metric-compatible faces (SIL Open Font
//! License, see `fonts/OFL.txt`). Noto Sans (same license), Droid Sans
//! Fallback, and a WenQuanYi Micro Hei subset (both Apache-2.0, see
//! `fonts/Apache-2.0.txt`) fill glyphs the selected face does not have,
//! including Greek, Cyrillic, CJK, and Hangul.
//!
//! fontdue supplies the advance, bearings, and a grayscale bitmap. A fully
//! covered pixel is the widget color. Partial coverage is composited onto the
//! pixel already there, which is the grayscale edge Qt paints instead of a
//! hard 50% cutoff.
//!
//! `install_display_font` selects Noto Sans (the default, which is what
//! `fc-match sans` gives Qt on a stock Linux desktop), Arimo, Cousine, or a
//! `.ttf` path. Noto Sans has a real bold face; the others get a synthesized
//! bold, drawn twice one pixel apart the way FreeType emboldens.
//! The viewer flag `-displayFont` is accepted and ignored, matching upstream.

use std::cell::RefCell;
use std::sync::OnceLock;

use fontdue::{Font, FontSettings, Metrics};

use crate::Image;
use caqtdm_core::{Color, Rect};

const ARIMO: &[u8] = include_bytes!("../fonts/Arimo-Regular.ttf");
const COUSINE: &[u8] = include_bytes!("../fonts/Cousine-Regular.ttf");
const NOTO: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
const NOTO_BOLD: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");
const DROID: &[u8] = include_bytes!("../fonts/DroidSansFallbackFull.ttf");
const HANGUL: &[u8] = include_bytes!("../fonts/WenQuanYiHangul.ttf");

/// The selected face and, when the family ships one, its bold companion.
struct Faces {
    regular: Font,
    bold: Option<Font>,
}

impl Faces {
    /// The face to rasterize with and whether bold must be synthesized.
    fn pick(&self, bold: bool) -> (&Font, bool) {
        match (&self.bold, bold) {
            (Some(face), true) => (face, false),
            (None, true) => (&self.regular, true),
            (_, false) => (&self.regular, false),
        }
    }
}

thread_local! {
    static FACE: RefCell<Faces> = RefCell::new(resolve("Noto Sans").expect("Noto Sans"));
}

/// Extra pixels a synthesized bold glyph is widened by.
fn embolden_px(px: f32) -> i32 {
    (px / 24.0).round().max(1.0) as i32
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
    let faces = resolve(name)?;
    Ok(advance(&faces.regular, px, text, 0))
}

/// `(advance, ymin)` of one character. A descender has a negative `ymin`.
pub fn glyph_metrics(name: &str, px: f32, ch: char) -> Result<(f32, i32), String> {
    let faces = resolve(name)?;
    let metrics = face_for(Some(&faces.regular), ch).metrics(ch, px.max(1.0));
    Ok((metrics.advance_width, metrics.ymin))
}

pub fn draw(image: &mut Image, x: i32, y: i32, text: &str, color: Color, px: i32) {
    draw_styled(image, x, y, text, color, px, false);
}

/// `draw` with the bold face (or a synthesized bold) when `bold` is set.
pub fn draw_styled(
    image: &mut Image,
    x: i32,
    y: i32,
    text: &str,
    color: Color,
    px: i32,
    bold: bool,
) {
    let px = (px.max(6) as f32).min(72.0);
    FACE.with(|face| {
        let faces = face.borrow();
        let (font, synth) = faces.pick(bold);
        let extra = if synth { embolden_px(px) } else { 0 };
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
            let chosen = face_for(Some(font), ch);
            let (metrics, bitmap) = chosen.rasterize(ch, px);
            for dx in 0..=extra {
                blit(
                    image,
                    cursor + dx as f32,
                    baseline,
                    &metrics,
                    &bitmap,
                    color,
                );
            }
            cursor += metrics.advance_width + extra as f32;
        }
    });
}

pub fn advance_current(text: &str, px: i32) -> i32 {
    advance_styled(text, px, false)
}

/// Advance of `text` in the current face, bold or regular.
pub fn advance_styled(text: &str, px: i32, bold: bool) -> i32 {
    let px = (px.max(6) as f32).min(72.0);
    FACE.with(|face| {
        let faces = face.borrow();
        let (font, synth) = faces.pick(bold);
        let extra = if synth { embolden_px(px) } else { 0 };
        advance(font, px, text, extra).round() as i32
    })
}

fn advance(font: &Font, px: f32, text: &str, extra: i32) -> f32 {
    let px = px.max(1.0);
    text.chars()
        .take(80)
        .map(|ch| face_for(Some(font), ch).metrics(ch, px).advance_width + extra as f32)
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
                load_bytes(HANGUL).expect("WenQuanYi Hangul"),
            ]
        })
        .as_slice()
}

fn resolve(name: &str) -> Result<Faces, String> {
    let trimmed = name.trim();
    let single = |font: Font| Faces {
        regular: font,
        bold: None,
    };
    if trimmed.is_empty() || is_noto(trimmed) {
        return Ok(Faces {
            regular: load_bytes(NOTO)?,
            bold: Some(load_bytes(NOTO_BOLD)?),
        });
    }
    if is_arimo(trimmed) {
        return load_bytes(ARIMO).map(single);
    }
    if is_cousine(trimmed) {
        return load_bytes(COUSINE).map(single);
    }
    let path = std::path::Path::new(trimmed);
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|err| format!("display font {trimmed}: {err}"))?;
        return load_bytes(&bytes)
            .map(single)
            .map_err(|err| format!("display font {trimmed}: {err}"));
    }
    Err(format!("display font '{trimmed}' was not found"))
}

fn is_arimo(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "arimo" | "arial" | "helvetica"
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
        "noto" | "noto sans" | "noto-sans" | "sans" | "sans-serif"
    )
}

/// Line spacing of the current face at `px`, the `QFontMetricsF::lineSpacing`
/// the fit loops compare against.
pub fn line_height(px: i32) -> i32 {
    let px = (px.max(6) as f32).min(72.0);
    FACE.with(|face| line_spacing(&face.borrow().regular, px).round() as i32)
}

fn line_spacing(font: &Font, px: f32) -> f32 {
    font.horizontal_line_metrics(px)
        .map(|line| line.new_line_size)
        .unwrap_or(px * 1.2)
}

/// `FontScalingWidget::calculateFontPointSizeF`. Mode 1 steps the point
/// size by 0.5 until the line spacing fits `height - 2`. Mode 2 then shrinks
/// it while the text is wider than `width - 4`. Mode 0 keeps `base_px`.
/// The result is in pixels at 96 dpi (one point is 4/3 pixel). `bold`
/// measures with the bold face.
pub fn fit_px_styled(text: &str, rect: Rect, mode: u8, base_px: i32, bold: bool) -> i32 {
    if mode == 0 || text.is_empty() {
        return base_px;
    }
    const MIN_PT: f32 = 4.0;
    let to_px = |pt: f32| pt * 4.0 / 3.0;
    let lines = text.lines().count().max(1) as f32;
    let border_h = (rect.height - 2) as f32;
    let border_w = (rect.width - 4) as f32;
    let longest = text
        .lines()
        .max_by_key(|line| line.chars().count())
        .unwrap_or(text);
    FACE.with(|face| {
        let faces = face.borrow();
        let (font, synth) = faces.pick(bold);
        let mut pt = (base_px as f32 * 0.75).max(MIN_PT);
        let height = |pt: f32| lines * line_spacing(font, to_px(pt));
        let mut guard = 0;
        while height(pt) > border_h && pt > MIN_PT && guard < 1000 {
            pt -= 0.5;
            guard += 1;
        }
        guard = 0;
        while height(pt) < border_h && guard < 1000 {
            pt += 0.5;
            guard += 1;
        }
        if mode == 2 {
            guard = 0;
            while advance(
                font,
                to_px(pt),
                longest,
                if synth { embolden_px(to_px(pt)) } else { 0 },
            ) > border_w
                && pt > MIN_PT
                && guard < 1000
            {
                pt -= 0.5;
                guard += 1;
            }
        }
        to_px(pt.max(MIN_PT)).round() as i32
    })
}

/// Top-left origin that centers `text` in `rect`.
pub fn centered_origin(text: &str, px: i32, rect: Rect) -> (i32, i32) {
    centered_origin_styled(text, px, rect, false)
}

/// `centered_origin` measured with the bold face when `bold` is set.
pub fn centered_origin_styled(text: &str, px: i32, rect: Rect, bold: bool) -> (i32, i32) {
    let width = advance_styled(text, px, bold);
    (
        rect.x + (rect.width - width) / 2,
        rect.y + (rect.height - line_height(px)) / 2,
    )
}

/// Text turned a quarter turn. `up` reads from the bottom of `rect` upward,
/// the `caLabelVertical::Up` direction; otherwise it reads downward. Each
/// glyph pixel is transposed into place with its coverage. `bold` uses the
/// bold face.
pub fn draw_rotated_styled(
    image: &mut Image,
    rect: Rect,
    text: &str,
    color: Color,
    px: i32,
    up: bool,
    bold: bool,
) {
    let px_f = (px.max(6) as f32).min(72.0);
    let height = line_height(px).max(1);
    // The rotated block is `height` wide, centered across the widget, and
    // starts at the end of `rect` that the reading direction begins from.
    let x0 = rect.x + (rect.width - height) / 2;
    FACE.with(|face| {
        let faces = face.borrow();
        let (font, synth) = faces.pick(bold);
        let extra = if synth { embolden_px(px_f) } else { 0 };
        let ascent = font
            .horizontal_line_metrics(px_f)
            .map(|line| line.ascent)
            .unwrap_or(px_f * 0.8)
            .round() as i32;
        let mut cursor = 0.0_f32;
        for ch in text.chars().take(80) {
            let chosen = face_for(Some(font), ch);
            let (metrics, bitmap) = chosen.rasterize(ch, px_f);
            let origin_x = cursor.round() as i32 + metrics.xmin;
            let origin_y = ascent - metrics.ymin - metrics.height as i32;
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 {
                        continue;
                    }
                    for dx in 0..=extra {
                        // Upright position inside the text line.
                        let sx = origin_x + col as i32 + dx;
                        let sy = origin_y + row as i32;
                        let (x, y) = if up {
                            (x0 + sy, rect.y + rect.height - 1 - sx)
                        } else {
                            (x0 + height - 1 - sy, rect.y + sx)
                        };
                        image.cover(x, y, color, coverage);
                    }
                }
            }
            cursor += metrics.advance_width + extra as f32;
        }
    });
}
