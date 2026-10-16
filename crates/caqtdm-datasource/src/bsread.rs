//! Simulated SwissFEL bsread pulse stream.
//!
//! The real plugin subscribes to a 0MQ stream of structured pulse messages.
//! This source advances a pulse id from `now_ms` and synthesizes a scalar or
//! a short waveform so strip charts still move. It does not speak bsread.

use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

pub struct BsreadSource {
    monitors: BTreeMap<u64, String>,
    next: u64,
    /// Pulse spacing in milliseconds. 10 ms is a 100 Hz machine.
    period_ms: u64,
}

impl BsreadSource {
    pub fn new() -> Self {
        Self {
            monitors: BTreeMap::new(),
            next: 1,
            period_ms: 10,
        }
    }
}

impl Default for BsreadSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for BsreadSource {
    fn name(&self) -> &str {
        "bsread"
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
            .ok_or_else(|| DsError::new(format!("unknown bsread monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, _value: &PutValue) -> Result<(), DsError> {
        Err(DsError::new(format!(
            "bsread channel {} is a pulse stream and cannot be written",
            spec.name
        )))
    }

    fn poll(&mut self, now_ms: u64) -> Vec<Update> {
        let pulse = now_ms / self.period_ms;
        self.monitors
            .iter()
            .map(|(id, name)| {
                let mut data = ChannelData::scalar(0.0);
                data.description = format!("bsread pulse {pulse}");
                data.value = if name == "pulse_id" {
                    PvValue::Int(pulse as i64)
                } else if name.ends_with(":wf") {
                    let samples: Vec<f64> = (0..16)
                        .map(|i| ((pulse as f64) * 0.05 + i as f64).sin())
                        .collect();
                    data.element_count = samples.len();
                    PvValue::Doubles(samples)
                } else {
                    PvValue::Double((pulse as f64 * 0.07).sin() * 10.0)
                };
                Update { id: *id, data }
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        format!("bsread {} (simulated pulse, no 0MQ)", spec.name)
    }
}
