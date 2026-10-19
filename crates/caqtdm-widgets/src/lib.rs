//! Runtime behavior for caQtDM widgets.
//!
//! A [`Session`] subscribes the channels named by the display, applies alarm
//! color and visibility, and turns each widget into a [`Visual`]. The egui
//! viewer and the PPM rasterizer both consume that list. Writes go back
//! through the data-source hub. Shell commands stay dry-run unless
//! [`Session::allow_shell`] is set.

mod curves;
mod designer;
mod image;
mod mda;
mod runtime;
mod style;
mod visual;

pub use designer::{designer_plugins, install_designer_plugin, DesignerKind, DesignerPlugin};
pub use image::{decode_image, decode_ppm, load_image_notes};
pub use mda::write_rank2;
pub use runtime::{supported_classes, toggled_bit_value, Session};
pub use visual::{
    cell_at, cell_rect, colorize_rgba, qt_global_color, sample_color, tab_at, tab_rects, Action,
    ColorMap, CurveStyle, CurveSymbol, Effect, GridCell, RoiMark, ShapeForm, ThermoBand, TreeRow,
    Visual, VisualKind,
};
