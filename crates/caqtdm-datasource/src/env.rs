use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

/// Reads process environment variables. Writes are rejected.
///
/// The channel name is the variable name (`env://USER`). This is the same
/// idea as caQtDM's environment plugin.
pub struct EnvSource {
    monitors: BTreeMap<u64, String>,
    next: u64,
    /// Optional overlay so tests do not depend on the real environment.
    overlay: BTreeMap<String, String>,
}

impl EnvSource {
    pub fn new() -> Self {
        Self {
            monitors: BTreeMap::new(),
            next: 1,
            overlay: BTreeMap::new(),
        }
    }

    pub fn set_overlay(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.overlay.insert(name.into(), value.into());
    }

    fn lookup(&self, name: &str) -> Option<String> {
        if let Some(v) = self.overlay.get(name) {
            return Some(v.clone());
        }
        std::env::var(name).ok()
    }
}

impl Default for EnvSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for EnvSource {
    fn name(&self) -> &str {
        "env"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown env monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, _value: &PutValue) -> Result<(), DsError> {
        Err(DsError::new(format!(
            "environment variable {} is read-only",
            spec.name
        )))
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        self.monitors
            .iter()
            .map(|(id, name)| {
                let mut data = match self.lookup(name) {
                    Some(text) => {
                        let mut data = ChannelData::scalar(text.parse().unwrap_or(0.0));
                        data.value = PvValue::String(text);
                        data
                    }
                    None => ChannelData::disconnected(name),
                };
                data.description = name.clone();
                data.access_write = false;
                Update { id: *id, data }
            })
            .collect()
    }
}
