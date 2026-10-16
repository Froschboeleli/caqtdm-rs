use std::collections::BTreeMap;

use caqtdm_core::{ChannelSpec, PutValue};

use crate::error::DsError;
use crate::pattern::{evaluate, name_seed, parse_pattern, Pattern};
use crate::source::{DataSource, Update};

/// Simulated channels. Writes are stored and replace the generated waveform
/// until the monitor is cleared.
pub struct SimSource {
    plugin: String,
    monitors: BTreeMap<u64, (String, Pattern)>,
    overrides: BTreeMap<String, caqtdm_core::ChannelData>,
    next: u64,
}

impl SimSource {
    pub fn new(plugin: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            monitors: BTreeMap::new(),
            overrides: BTreeMap::new(),
            next: 1,
        }
    }
}

impl DataSource for SimSource {
    fn name(&self) -> &str {
        &self.plugin
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        let pattern = parse_pattern(&spec.name);
        self.monitors.insert(id, (spec.name.clone(), pattern));
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown sim monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let mut data = caqtdm_core::ChannelData::scalar(value.as_f64().unwrap_or(0.0));
        data.value = match value {
            PutValue::Double(v) => caqtdm_core::PvValue::Double(*v),
            PutValue::Int(v) => caqtdm_core::PvValue::Int(*v),
            PutValue::String(s) => caqtdm_core::PvValue::String(s.clone()),
            PutValue::Doubles(v) => caqtdm_core::PvValue::Doubles(v.clone()),
        };
        data.access_write = true;
        self.overrides.insert(spec.name.clone(), data);
        Ok(())
    }

    fn poll(&mut self, now_ms: u64) -> Vec<Update> {
        self.monitors
            .iter()
            .map(|(id, (name, pattern))| {
                let data = if let Some(over) = self.overrides.get(name) {
                    over.clone()
                } else {
                    evaluate(pattern, now_ms, name_seed(name))
                };
                Update { id: *id, data }
            })
            .collect()
    }
}
