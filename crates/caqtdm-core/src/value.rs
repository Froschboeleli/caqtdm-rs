use std::time::SystemTime;

use crate::alarm::Severity;

/// Numeric limits that travel with a channel, matching the EPICS DBR_CTRL fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub lower_display: f64,
    pub upper_display: f64,
    pub lower_alarm: f64,
    pub lower_warning: f64,
    pub upper_warning: f64,
    pub upper_alarm: f64,
    pub lower_ctrl: f64,
    pub upper_ctrl: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            lower_display: 0.0,
            upper_display: 0.0,
            lower_alarm: f64::NAN,
            lower_warning: f64::NAN,
            upper_warning: f64::NAN,
            upper_alarm: f64::NAN,
            lower_ctrl: 0.0,
            upper_ctrl: 0.0,
        }
    }
}

impl Limits {
    pub fn display_span(self) -> Option<(f64, f64)> {
        if self.upper_display > self.lower_display {
            Some((self.lower_display, self.upper_display))
        } else if self.upper_ctrl > self.lower_ctrl {
            Some((self.lower_ctrl, self.upper_ctrl))
        } else {
            None
        }
    }
}

/// A process-variable value. Arrays stay arrays so plots and tables can use them.
#[derive(Debug, Clone, PartialEq)]
pub enum PvValue {
    Double(f64),
    Int(i64),
    String(String),
    Enum { index: i32, label: String },
    Doubles(Vec<f64>),
    Ints(Vec<i64>),
    Strings(Vec<String>),
    Bytes(Vec<u8>),
}

impl PvValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            Self::Enum { index, .. } => Some(f64::from(*index)),
            Self::String(s) => s.trim().parse().ok(),
            Self::Doubles(v) => v.first().copied(),
            Self::Ints(v) => v.first().map(|n| *n as f64),
            Self::Bytes(v) => v.first().map(|n| f64::from(*n)),
            Self::Strings(v) => v.first().and_then(|s| s.trim().parse().ok()),
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|v| v as i64)
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Double(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    format!("{}", *v as i64)
                } else {
                    format!("{v}")
                }
            }
            Self::Int(v) => v.to_string(),
            Self::String(s) => s.clone(),
            Self::Enum { label, index } => {
                if label.is_empty() {
                    index.to_string()
                } else {
                    label.clone()
                }
            }
            Self::Doubles(v) => v
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            Self::Ints(v) => v
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            Self::Strings(v) => v.join("\n"),
            Self::Bytes(v) => format!("<{} bytes>", v.len()),
        }
    }

    pub fn as_f64_slice(&self) -> Vec<f64> {
        match self {
            Self::Doubles(v) => v.clone(),
            Self::Ints(v) => v.iter().map(|n| *n as f64).collect(),
            Self::Bytes(v) => v.iter().map(|n| f64::from(*n)).collect(),
            other => other.as_f64().into_iter().collect(),
        }
    }
}

/// Snapshot of one monitored channel, including the metadata widgets need.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelData {
    pub connected: bool,
    pub severity: Severity,
    pub status: i16,
    pub precision: i32,
    pub units: String,
    pub limits: Limits,
    pub enum_strings: Vec<String>,
    pub value: PvValue,
    pub timestamp: Option<SystemTime>,
    pub access_read: bool,
    pub access_write: bool,
    pub description: String,
    pub element_count: usize,
}

impl ChannelData {
    pub fn disconnected(name: &str) -> Self {
        Self {
            connected: false,
            severity: Severity::NotConnected,
            status: 0,
            precision: 0,
            units: String::new(),
            limits: Limits::default(),
            enum_strings: Vec::new(),
            value: PvValue::String(String::new()),
            timestamp: None,
            access_read: false,
            access_write: false,
            description: name.to_string(),
            element_count: 1,
        }
    }

    pub fn bytes(bytes: Vec<u8>) -> Self {
        let count = bytes.len();
        let mut data = Self::scalar(0.0);
        data.value = PvValue::Bytes(bytes);
        data.element_count = count.max(1);
        data
    }

    pub fn scalar(value: f64) -> Self {
        Self {
            connected: true,
            severity: Severity::NoAlarm,
            status: 0,
            precision: 3,
            units: String::new(),
            limits: Limits {
                lower_display: 0.0,
                upper_display: 100.0,
                lower_ctrl: 0.0,
                upper_ctrl: 100.0,
                ..Limits::default()
            },
            enum_strings: Vec::new(),
            value: PvValue::Double(value),
            timestamp: None,
            access_read: true,
            access_write: true,
            description: String::new(),
            element_count: 1,
        }
    }
}

/// Value a widget writes back to a channel.
#[derive(Debug, Clone, PartialEq)]
pub enum PutValue {
    Double(f64),
    Int(i64),
    String(String),
    Doubles(Vec<f64>),
}

impl PutValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            Self::String(s) => s.trim().parse().ok(),
            Self::Doubles(v) => v.first().copied(),
        }
    }
}
