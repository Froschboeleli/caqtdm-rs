use crate::color::Color;
use crate::geometry::{Point, Rect, Size};

/// A property value as it appears in a Qt `.ui` file or a converted ADL object.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    String(String),
    StringList(Vec<String>),
    Number(i64),
    Double(f64),
    Bool(bool),
    /// Qt enum stored as the full token (`caLineEdit::colMode::Alarm`) and
    /// also available through [`PropValue::enum_tail`].
    Enum(String),
    Set(String),
    Color(Color),
    Rect(Rect),
    Size(Size),
    Point(Point),
    Font(FontSpec),
}

/// The subset of `QFont` that displays actually use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSpec {
    pub family: String,
    pub point_size: i32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for FontSpec {
    fn default() -> Self {
        Self {
            family: "Sans".to_string(),
            point_size: 10,
            bold: false,
            italic: false,
        }
    }
}

impl PropValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) | Self::Enum(s) | Self::Set(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_string_list(&self) -> Vec<String> {
        match self {
            Self::StringList(list) => list.clone(),
            Self::String(s) => split_list(s),
            _ => Vec::new(),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(v) => Some(*v),
            Self::Number(v) => Some(*v as f64),
            Self::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Self::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            Self::Number(v) => Some(*v != 0),
            Self::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            Self::Color(c) => Some(*c),
            Self::String(s) => Color::parse(s),
            Self::Number(n) if (0..=65).contains(n) => Some(crate::color::medm_color(*n as i32)),
            _ => None,
        }
    }

    /// Last `::` component of a Qt enum, so `caLineEdit::colMode::Alarm`
    /// becomes `Alarm`.
    pub fn enum_tail(&self) -> Option<&str> {
        let raw = self.as_str()?;
        Some(raw.rsplit("::").next().unwrap_or(raw))
    }
}

fn split_list(s: &str) -> Vec<String> {
    s.split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}
