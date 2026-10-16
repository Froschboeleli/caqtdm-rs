//! [`DataSource`](caqtdm_datasource::DataSource) adapter over [`crate::CaClient`].

use std::collections::BTreeMap;
use std::net::SocketAddr;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue};
use caqtdm_datasource::{DataSource, DsError, Update};

use crate::client::CaClient;
use crate::protocol::CaError;

pub struct CaSource {
    client: Option<CaClient>,
    monitors: BTreeMap<u64, String>,
    latest: BTreeMap<String, ChannelData>,
    next: u64,
    fault: Option<String>,
}

impl CaSource {
    pub fn offline() -> Self {
        Self {
            client: None,
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
            fault: Some("no Channel Access server configured".to_string()),
        }
    }

    pub fn connect(addr: SocketAddr) -> Result<Self, CaError> {
        Ok(Self {
            client: Some(CaClient::connect(addr)?),
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
            fault: None,
        })
    }
}

impl DataSource for CaSource {
    fn name(&self) -> &str {
        "epics3"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        if let Some(client) = self.client.as_mut() {
            client
                .create(&spec.name)
                .map_err(|e| DsError::new(e.to_string()))?;
            client
                .subscribe(&spec.name)
                .map_err(|e| DsError::new(e.to_string()))?;
            if let Ok(data) = client.read(&spec.name) {
                self.latest.insert(spec.name.clone(), data);
            }
        }
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown CA monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let Some(client) = self.client.as_mut() else {
            return Err(DsError::new("Channel Access client is offline"));
        };
        let number = value
            .as_f64()
            .ok_or_else(|| DsError::new("CA put needs a number"))?;
        client
            .create(&spec.name)
            .map_err(|e| DsError::new(e.to_string()))?;
        client
            .write_double(&spec.name, number)
            .map_err(|e| DsError::new(e.to_string()))
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        if let Some(client) = self.client.as_mut() {
            if let Ok(events) = client.poll() {
                for event in events {
                    self.latest.insert(event.name, event.data);
                }
            }
        }
        self.monitors
            .iter()
            .map(|(id, name)| {
                let data = self
                    .latest
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| ChannelData::disconnected(name));
                Update { id: *id, data }
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        match &self.fault {
            Some(msg) => format!("epics3 {} ({msg})", spec.name),
            None => format!("epics3 {}", spec.name),
        }
    }
}
