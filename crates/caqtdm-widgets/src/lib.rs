//! Runtime behavior for caQtDM widgets.
//!
//! A [`Session`] subscribes the channels named by the display, applies alarm
//! color and visibility, and turns each widget into a [`Visual`]. The egui
//! viewer and the PPM rasterizer both consume that list. Writes go back
//! through the data-source hub. Shell commands stay dry-run unless
//! [`Session::allow_shell`] is set.

mod image;
mod runtime;
mod visual;

pub use image::{decode_ppm, load_image_notes};
pub use runtime::{supported_classes, toggled_bit_value, Session};
pub use visual::{Action, Effect, ShapeForm, Visual, VisualKind};
