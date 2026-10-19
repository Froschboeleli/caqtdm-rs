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

    /// Parse `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, or a CSS/Qt color name.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if let Some(hex) = t.strip_prefix('#') {
            return parse_hex(hex);
        }
        if let Some(color) = parse_rgb_fn(t) {
            return Some(color);
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
        other => return css_name(other),
    };
    Some(c)
}

fn parse_rgb_fn(text: &str) -> Option<Color> {
    let lower = text.trim().to_ascii_lowercase();
    let inner = if let Some(rest) = lower.strip_prefix("rgba(") {
        rest
    } else {
        lower.strip_prefix("rgb(")?
    };
    let inner = inner.trim().strip_suffix(')')?.trim();
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parse_component(parts[0])?;
    let g = parse_component(parts[1])?;
    let b = parse_component(parts[2])?;
    let a = if parts.len() >= 4 {
        parse_component(parts[3])?
    } else {
        255
    };
    Some(Color::rgba(r, g, b, a))
}

fn parse_component(text: &str) -> Option<u8> {
    let v: f64 = text.parse().ok()?;
    Some(v.round().clamp(0.0, 255.0) as u8)
}

/// SVG/CSS names Qt accepts in a stylesheet. Names already matched above stay
/// as they are (`green` is the MEDM green, not CSS green).
fn css_name(name: &str) -> Option<Color> {
    const NAMES: &[(&str, u8, u8, u8)] = &[
        ("aliceblue", 240, 248, 255),
        ("antiquewhite", 250, 235, 215),
        ("aqua", 0, 255, 255),
        ("aquamarine", 127, 255, 212),
        ("azure", 240, 255, 255),
        ("beige", 245, 245, 220),
        ("bisque", 255, 228, 196),
        ("blanchedalmond", 255, 235, 205),
        ("blueviolet", 138, 43, 226),
        ("brown", 165, 42, 42),
        ("burlywood", 222, 184, 135),
        ("cadetblue", 95, 158, 160),
        ("chartreuse", 127, 255, 0),
        ("chocolate", 210, 105, 30),
        ("coral", 255, 127, 80),
        ("cornflowerblue", 100, 149, 237),
        ("cornsilk", 255, 248, 220),
        ("crimson", 220, 20, 60),
        ("darkblue", 0, 0, 139),
        ("darkcyan", 0, 139, 139),
        ("darkgoldenrod", 184, 134, 11),
        ("darkgreen", 0, 100, 0),
        ("darkkhaki", 189, 183, 107),
        ("darkmagenta", 139, 0, 139),
        ("darkolivegreen", 85, 107, 47),
        ("darkorange", 255, 140, 0),
        ("darkorchid", 153, 50, 204),
        ("darkred", 139, 0, 0),
        ("darksalmon", 233, 150, 122),
        ("darkseagreen", 143, 188, 143),
        ("darkslateblue", 72, 61, 139),
        ("darkslategray", 47, 79, 79),
        ("darkslategrey", 47, 79, 79),
        ("darkturquoise", 0, 206, 209),
        ("darkviolet", 148, 0, 211),
        ("deeppink", 255, 20, 147),
        ("deepskyblue", 0, 191, 255),
        ("dimgray", 105, 105, 105),
        ("dimgrey", 105, 105, 105),
        ("dodgerblue", 30, 144, 255),
        ("firebrick", 178, 34, 34),
        ("floralwhite", 255, 250, 240),
        ("forestgreen", 34, 139, 34),
        ("fuchsia", 255, 0, 255),
        ("gainsboro", 220, 220, 220),
        ("ghostwhite", 248, 248, 255),
        ("gold", 255, 215, 0),
        ("goldenrod", 218, 165, 32),
        ("greenyellow", 173, 255, 47),
        ("honeydew", 240, 255, 240),
        ("hotpink", 255, 105, 180),
        ("indianred", 205, 92, 92),
        ("indigo", 75, 0, 130),
        ("ivory", 255, 255, 240),
        ("khaki", 240, 230, 140),
        ("lavender", 230, 230, 250),
        ("lavenderblush", 255, 240, 245),
        ("lawngreen", 124, 252, 0),
        ("lemonchiffon", 255, 250, 205),
        ("lightblue", 173, 216, 230),
        ("lightcoral", 240, 128, 128),
        ("lightcyan", 224, 255, 255),
        ("lightgoldenrodyellow", 250, 250, 210),
        ("lightgreen", 144, 238, 144),
        ("lightpink", 255, 182, 193),
        ("lightsalmon", 255, 160, 122),
        ("lightseagreen", 32, 178, 170),
        ("lightskyblue", 135, 206, 250),
        ("lightslategray", 119, 136, 153),
        ("lightslategrey", 119, 136, 153),
        ("lightsteelblue", 176, 196, 222),
        ("lightyellow", 255, 255, 224),
        ("lime", 0, 255, 0),
        ("limegreen", 50, 205, 50),
        ("linen", 250, 240, 230),
        ("maroon", 128, 0, 0),
        ("mediumaquamarine", 102, 205, 170),
        ("mediumblue", 0, 0, 205),
        ("mediumorchid", 186, 85, 211),
        ("mediumpurple", 147, 112, 219),
        ("mediumseagreen", 60, 179, 113),
        ("mediumslateblue", 123, 104, 238),
        ("mediumspringgreen", 0, 250, 154),
        ("mediumturquoise", 72, 209, 204),
        ("mediumvioletred", 199, 21, 133),
        ("midnightblue", 25, 25, 112),
        ("mintcream", 245, 255, 250),
        ("mistyrose", 255, 228, 225),
        ("moccasin", 255, 228, 181),
        ("navajowhite", 255, 222, 173),
        ("navy", 0, 0, 128),
        ("oldlace", 253, 245, 230),
        ("olive", 128, 128, 0),
        ("olivedrab", 107, 142, 35),
        ("orangered", 255, 69, 0),
        ("orchid", 218, 112, 214),
        ("palegoldenrod", 238, 232, 170),
        ("palegreen", 152, 251, 152),
        ("paleturquoise", 175, 238, 238),
        ("palevioletred", 219, 112, 147),
        ("papayawhip", 255, 239, 213),
        ("peachpuff", 255, 218, 185),
        ("peru", 205, 133, 63),
        ("pink", 255, 192, 203),
        ("plum", 221, 160, 221),
        ("powderblue", 176, 224, 230),
        ("purple", 128, 0, 128),
        ("rosybrown", 188, 143, 143),
        ("royalblue", 65, 105, 225),
        ("saddlebrown", 139, 69, 19),
        ("salmon", 250, 128, 114),
        ("sandybrown", 244, 164, 96),
        ("seagreen", 46, 139, 87),
        ("seashell", 255, 245, 238),
        ("sienna", 160, 82, 45),
        ("silver", 192, 192, 192),
        ("skyblue", 135, 206, 235),
        ("slateblue", 106, 90, 205),
        ("slategray", 112, 128, 144),
        ("slategrey", 112, 128, 144),
        ("snow", 255, 250, 250),
        ("springgreen", 0, 255, 127),
        ("steelblue", 70, 130, 180),
        ("tan", 210, 180, 140),
        ("teal", 0, 128, 128),
        ("thistle", 216, 191, 216),
        ("tomato", 255, 99, 71),
        ("turquoise", 64, 224, 208),
        ("violet", 238, 130, 238),
        ("wheat", 245, 222, 179),
        ("whitesmoke", 245, 245, 245),
        ("yellowgreen", 154, 205, 50),
    ];
    NAMES
        .iter()
        .find(|(entry, _, _, _)| *entry == name)
        .map(|(_, r, g, b)| Color::rgb(*r, *g, *b))
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
