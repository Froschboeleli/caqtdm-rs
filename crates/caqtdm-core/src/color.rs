use crate::alarm::Severity;

/// sRGB color with an alpha channel. Components are 0..=255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn transparent() -> Self {
        Self::rgba(0, 0, 0, 0)
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Parse `#rgb`, `#rrggbb`, `#rrggbbaa`, or a CSS/Qt color name.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if let Some(hex) = t.strip_prefix('#') {
            return parse_hex(hex);
        }
        named(t)
    }

    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
        };
        Self::rgba(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let h = hex.trim();
    match h.len() {
        3 | 4 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
            let r = d(0)?;
            let g = d(1)?;
            let b = d(2)?;
            let a = if h.len() == 4 { d(3)? } else { 255 };
            Some(Color::rgba(r, g, b, a))
        }
        6 | 8 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
            let r = d(0)?;
            let g = d(2)?;
            let b = d(4)?;
            let a = if h.len() == 8 { d(6)? } else { 255 };
            Some(Color::rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn named(name: &str) -> Option<Color> {
    let c = match name.to_ascii_lowercase().as_str() {
        "black" => Color::rgb(0, 0, 0),
        "white" => Color::rgb(255, 255, 255),
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 205, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "cyan" => Color::rgb(0, 255, 255),
        "magenta" => Color::rgb(255, 0, 255),
        "gray" | "grey" => Color::rgb(0x88, 0x88, 0x88),
        "lightgray" | "lightgrey" => Color::rgb(0xd3, 0xd3, 0xd3),
        "darkgray" | "darkgrey" => Color::rgb(0x44, 0x44, 0x44),
        "orange" => Color::rgb(255, 165, 0),
        "transparent" => Color::transparent(),
        _ => return None,
    };
    Some(c)
}

/// Alarm colors from caQtDM `alarmdefs.h`.
pub fn alarm_color(severity: Severity) -> Color {
    match severity {
        Severity::NoAlarm => Color::rgb(0x00, 0xcd, 0x00),
        Severity::Minor => Color::rgb(0xff, 0xff, 0x00),
        Severity::Major => Color::rgb(0xff, 0x00, 0x00),
        Severity::Invalid | Severity::NotConnected => Color::rgb(0xff, 0xff, 0xff),
    }
}

/// Static ADL/MEDM color index. Index 0 is white and index 14 is black,
/// followed by green, cyan, blue, magenta, red, and yellow ramps. This is
/// the layout caQtDM inherits from MEDM when it translates `.adl` files.
pub fn medm_color(index: i32) -> Color {
    const TABLE: &[(u8, u8, u8)] = &[
        (255, 255, 255),
        (236, 236, 236),
        (218, 218, 218),
        (200, 200, 200),
        (187, 187, 187),
        (174, 174, 174),
        (158, 158, 158),
        (145, 145, 145),
        (133, 133, 133),
        (120, 120, 120),
        (105, 105, 105),
        (90, 90, 90),
        (70, 70, 70),
        (45, 45, 45),
        (0, 0, 0),
        (0, 216, 0),
        (30, 187, 0),
        (51, 153, 0),
        (45, 127, 0),
        (33, 108, 0),
        (15, 84, 0),
        (0, 70, 0),
        (0, 45, 0),
        (0, 0, 0),
        (0, 255, 255),
        (0, 224, 224),
        (0, 192, 192),
        (0, 160, 160),
        (0, 128, 128),
        (0, 96, 96),
        (0, 64, 64),
        (0, 32, 32),
        (0, 0, 255),
        (0, 0, 224),
        (0, 0, 192),
        (0, 0, 160),
        (0, 0, 128),
        (0, 0, 96),
        (0, 0, 64),
        (0, 0, 32),
        (255, 0, 255),
        (224, 0, 224),
        (192, 0, 192),
        (160, 0, 160),
        (128, 0, 128),
        (96, 0, 96),
        (64, 0, 64),
        (32, 0, 32),
        (255, 0, 0),
        (224, 0, 0),
        (192, 0, 0),
        (160, 0, 0),
        (128, 0, 0),
        (96, 0, 0),
        (64, 0, 0),
        (32, 0, 0),
        (255, 255, 0),
        (224, 224, 0),
        (192, 192, 0),
        (160, 160, 0),
        (128, 128, 0),
        (96, 96, 0),
        (64, 64, 0),
        (32, 32, 0),
        (255, 128, 0),
    ];
    let i = if index < 0 { 0 } else { index as usize };
    let (r, g, b) = TABLE.get(i).copied().unwrap_or(TABLE[0]);
    Color::rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_names() {
        assert_eq!(Color::parse("#0f0").unwrap(), Color::rgb(0, 255, 0));
        assert_eq!(Color::parse("#00cd00").unwrap(), Color::rgb(0, 205, 0));
        assert_eq!(Color::parse("yellow").unwrap().g, 255);
        assert!(Color::parse("not-a-color").is_none());
    }

    #[test]
    fn alarm_palette_matches_caqtdm() {
        assert_eq!(alarm_color(Severity::NoAlarm), Color::rgb(0x00, 0xcd, 0x00));
        assert_eq!(alarm_color(Severity::Minor), Color::rgb(255, 255, 0));
        assert_eq!(alarm_color(Severity::Major), Color::rgb(255, 0, 0));
        assert_eq!(alarm_color(Severity::Invalid), Color::rgb(255, 255, 255));
    }

    #[test]
    fn medm_endpoints() {
        assert_eq!(medm_color(0), Color::rgb(255, 255, 255));
        assert_eq!(medm_color(14), Color::rgb(0, 0, 0));
        assert_eq!(medm_color(64), Color::rgb(255, 128, 0));
    }
}
