//! In-memory OPC UA node map.
//!
//! caQtDM's opcua plugin speaks the binary protocol. This source keeps the
//! node-id channel names (`ns=2;s=Temperature`) and read/write behavior so a
//! display can be developed without a server. It does not implement OPC UA
//! SecureChannel, sessions, or the binary type system.

use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

pub struct OpcuaSource {
    nodes: BTreeMap<String, ChannelData>,
    monitors: BTreeMap<u64, String>,
    next: u64,
}

impl OpcuaSource {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            monitors: BTreeMap::new(),
            next: 1,
        }
    }

    pub fn insert_double(&mut self, node_id: &str, value: f64) {
        self.nodes
            .insert(node_id.to_string(), ChannelData::scalar(value));
    }
}

impl Default for OpcuaSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for OpcuaSource {
    fn name(&self) -> &str {
        "opcua"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        self.nodes
            .entry(spec.name.clone())
            .or_insert_with(|| ChannelData::disconnected(&spec.name));
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown opcua monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let mut data = self
            .nodes
            .get(&spec.name)
            .cloned()
            .unwrap_or_else(|| ChannelData::scalar(0.0));
        data.connected = true;
        data.value = match value {
            PutValue::Double(v) => PvValue::Double(*v),
            PutValue::Int(v) => PvValue::Int(*v),
            PutValue::String(s) => PvValue::String(s.clone()),
            PutValue::Doubles(v) => PvValue::Doubles(v.clone()),
        };
        self.nodes.insert(spec.name.clone(), data);
        Ok(())
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        self.monitors
            .iter()
            .filter_map(|(id, name)| {
                self.nodes.get(name).map(|data| Update {
                    id: *id,
                    data: data.clone(),
                })
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        format!("opcua node {} (in-memory, no binary session)", spec.name)
    }
}
