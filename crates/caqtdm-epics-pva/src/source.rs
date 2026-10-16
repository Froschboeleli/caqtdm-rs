use std::collections::BTreeMap;
use std::net::SocketAddr;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue};
use caqtdm_datasource::{DataSource, DsError, Update};

use crate::client::PvaClient;
use crate::proto::PvaError;

/// Data-source plugin `epics4`. `pva://` and `ca://` both select this plugin
/// in caQtDM. This adapter speaks the scalar subset over TCP.
pub struct PvaSource {
    client: Option<PvaClient>,
    monitors: BTreeMap<u64, String>,
    latest: BTreeMap<String, ChannelData>,
    next: u64,
}

impl PvaSource {
    pub fn offline() -> Self {
        Self {
            client: None,
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
        }
    }

    pub fn connect(addr: SocketAddr) -> Result<Self, PvaError> {
        Ok(Self {
            client: Some(PvaClient::connect(addr)?),
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
        })
    }
}

impl DataSource for PvaSource {
    fn name(&self) -> &str {
        "epics4"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        if let Some(client) = self.client.as_mut() {
            client
                .create(&spec.name)
                .map_err(|e| DsError::new(e.to_string()))?;
            if let Ok(value) = client.get(&spec.name) {
                self.latest
                    .insert(spec.name.clone(), ChannelData::scalar(value));
            }
            client
                .subscribe(&spec.name)
                .map_err(|e| DsError::new(e.to_string()))?;
        }
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown PVA monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let Some(client) = self.client.as_mut() else {
            return Err(DsError::new("PVA client is offline"));
        };
        let number = value
            .as_f64()
            .ok_or_else(|| DsError::new("PVA put needs a number"))?;
        client
            .create(&spec.name)
            .map_err(|e| DsError::new(e.to_string()))?;
        client
            .put(&spec.name, number)
            .map_err(|e| DsError::new(e.to_string()))
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        if let Some(client) = self.client.as_mut() {
            if let Ok(events) = client.poll() {
                for event in events {
                    self.latest
                        .insert(event.name, ChannelData::scalar(event.value));
                }
            }
        }
        self.monitors
            .iter()
            .map(|(id, name)| Update {
                id: *id,
                data: self
                    .latest
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| ChannelData::disconnected(name)),
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        format!(
            "epics4 {} flavor {} (scalar subset, not pvxs NTScalar)",
            spec.name, spec.flavor
        )
    }
}
