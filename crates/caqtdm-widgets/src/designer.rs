//! Designer-equivalent widget plugins.
//!
//! Qt Designer plugins are Qt binaries. This registry is the same job for the
//! runtime: a display author (or another crate) names a class, the properties
//! Designer would have shown, and which built-in visual the class paints as.
//! The `.ui` reader already keeps every property. A registered class is no
//! longer an unknown widget.

use std::sync::Mutex;

use crate::visual::VisualKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignerKind {
    Label,
    Led,
    Frame,
    Gauge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignerPlugin {
    pub class_name: String,
    pub group: String,
    pub properties: Vec<String>,
    pub kind: DesignerKind,
}

static PLUGINS: Mutex<Vec<DesignerPlugin>> = Mutex::new(Vec::new());

/// Register a custom widget class. A later registration with the same name replaces it.
pub fn install_designer_plugin(plugin: DesignerPlugin) {
    if let Ok(mut guard) = PLUGINS.lock() {
        if let Some(slot) = guard
            .iter_mut()
            .find(|item| item.class_name == plugin.class_name)
        {
            *slot = plugin;
        } else {
            guard.push(plugin);
        }
    }
}

pub fn designer_plugins() -> Vec<DesignerPlugin> {
    PLUGINS
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

pub fn designer_kind(class_name: &str) -> Option<VisualKind> {
    let guard = PLUGINS.lock().ok()?;
    let plugin = guard.iter().find(|item| item.class_name == class_name)?;
    Some(match plugin.kind {
        DesignerKind::Label => VisualKind::Label,
        DesignerKind::Led => VisualKind::Led { on: true },
        DesignerKind::Frame => VisualKind::Frame,
        DesignerKind::Gauge => VisualKind::Gauge {
            fraction: 0.5,
            circular: false,
        },
    })
}
