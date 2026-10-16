//! Core types shared by the parser, data sources, widgets, and viewer.
//!
//! The model follows caQtDM's runtime picture: a display is a tree of widgets,
//! each widget reads process variables through a named control-system plugin,
//! and alarm severity, precision, and limits travel with the value.

pub mod alarm;
pub mod calc;
pub mod channel;
pub mod color;
pub mod format;
pub mod geometry;
pub mod macros;
pub mod property;
pub mod value;

pub use alarm::Severity;
pub use calc::{eval_calc, eval_visibility, CalcError, CalcInputs};
pub use channel::{parse_channel, ChannelSpec};
pub use color::{alarm_color, medm_color, Color};
pub use format::{format_number, FormatType};
pub use geometry::{Point, Rect, Size};
pub use macros::{apply_macros, parse_macro_list, MacroMap};
pub use property::PropValue;
pub use value::{ChannelData, Limits, PutValue, PvValue};
