use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

/// In-memory soft process variables, matching caQtDM's `intern` plugin.
///
/// Several widgets can share one store. A write is visible to every monitor
/// of that name on the next poll.
#[derive(Clone)]
pub struct InternSource {
    store: Arc<Mutex<BTreeMap<String, ChannelData>>>,
    monitors: BTreeMap<u64, String>,
    next: u64,
}

impl InternSource {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(BTreeMap::new())),
            monitors: BTreeMap::new(),
            next: 1,
        }
    }

    pub fn insert(&self, name: &str, data: ChannelData) {
        self.store
            .lock()
            .expect("intern store")
            .insert(name.to_string(), data);
    }
}

impl Default for InternSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for InternSource {
    fn name(&self) -> &str {
        "intern"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        let mut store = self.store.lock().expect("intern store");
        store.entry(spec.name.clone()).or_insert_with(|| {
            let mut data = ChannelData::scalar(0.0);
            data.description = spec.name.clone();
            data
        });
        drop(store);
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown intern monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let mut store = self.store.lock().expect("intern store");
        let entry = store
            .entry(spec.name.clone())
            .or_insert_with(|| ChannelData::scalar(0.0));
        entry.connected = true;
        entry.value = match value {
            PutValue::Double(v) => PvValue::Double(*v),
            PutValue::Int(v) => PvValue::Int(*v),
            PutValue::String(s) => PvValue::String(s.clone()),
            PutValue::Doubles(v) => PvValue::Doubles(v.clone()),
        };
        entry.access_write = true;
        entry.access_read = true;
        Ok(())
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        let store = self.store.lock().expect("intern store");
        self.monitors
            .iter()
            .filter_map(|(id, name)| {
                store.get(name).map(|data| Update {
                    id: *id,
                    data: data.clone(),
                })
            })
            .collect()
    }
}
