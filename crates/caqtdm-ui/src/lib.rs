//! Display-file readers. Every format lands in the same [`DisplayFile`] tree
//! so the widget runtime does not care whether the screen was drawn in Qt
//! Designer or translated from MEDM.

mod adl;
mod alh;
mod edit;
mod edl;
mod error;
mod expand;
mod model;
mod prc;
mod search;
mod ui_xml;
mod write_ui;

pub use edit::DisplayEditor;
pub use error::UiError;
pub use expand::expand_includes;
pub use model::{
    assign_geometry, CustomWidget, DisplayFile, LayoutItem, LayoutNode, SourceFormat, WidgetNode,
};
pub use search::{fetch_http, fetch_https, SearchPath};
pub use write_ui::write_ui;

use std::path::Path;

/// Load a display, detecting the format from the extension or the contents.
pub fn load_path(path: &Path) -> Result<DisplayFile, UiError> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| UiError::io(path.display().to_string(), err))?;
    let mut display = load_str(&text, path)?;
    display.path = Some(path.to_path_buf());
    Ok(display)
}

/// Parse `text`. `path` is only used for the extension and for error messages.
pub fn load_str(text: &str, path: &Path) -> Result<DisplayFile, UiError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut display = match ext.as_str() {
        "ui" => ui_xml::parse_ui(text)?,
        "adl" => adl::parse_adl(text)?,
        "edl" => edl::parse_edl(text)?,
        "alh" => alh::parse_alh(text)?,
        "prc" => prc::parse_prc(text)?,
        _ => sniff(text)?,
    };
    display.path = Some(path.to_path_buf());
    assign_geometry(&mut display.root);
    Ok(display)
}

fn sniff(text: &str) -> Result<DisplayFile, UiError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("<?xml") || trimmed.starts_with("<ui") {
        return ui_xml::parse_ui(text);
    }
    if trimmed.contains("beginScreenProperties") || trimmed.contains("beginObjectProperties") {
        return edl::parse_edl(text);
    }
    if trimmed.starts_with("#!") || trimmed.starts_with("#channel") {
        return prc::parse_prc(text);
    }
    if trimmed.contains("GROUP ") || trimmed.starts_with("CHANNEL ") {
        return alh::parse_alh(text);
    }
    adl::parse_adl(text)
}
