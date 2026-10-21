//! [`DataSource`](caqtdm_datasource::DataSource) adapter over [`crate::CaClient`].

use std::collections::BTreeMap;
use std::net::SocketAddr;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue};
use caqtdm_datasource::{DataSource, DsError, Update};

use crate::client::CaClient;
use crate::live::{CaConfig, CaStats, LiveCa};
use crate::protocol::CaError;

pub struct CaSource {
    client: Option<CaClient>,
    live: Option<LiveCa>,
    last_report: Option<(std::time::Instant, CaStats)>,
    monitors: BTreeMap<u64, String>,
    latest: BTreeMap<String, ChannelData>,
    next: u64,
    fault: Option<String>,
}

impl CaSource {
    pub fn offline() -> Self {
        Self {
            client: None,
            live: None,
            last_report: None,
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
            fault: Some("no Channel Access server configured".to_string()),
        }
    }

    pub fn connect(addr: SocketAddr) -> Result<Self, CaError> {
        Ok(Self {
            client: Some(CaClient::connect(addr)?),
            live: None,
            last_report: None,
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
            fault: None,
        })
    }
}

impl CaSource {
    /// Live client from the libca environment, or `None` when neither
    /// `EPICS_CA_ADDR_LIST` nor `EPICS_CA_NAME_SERVERS` is set and auto
    /// addressing is off.
    pub fn from_env() -> Option<Self> {
        let config = CaConfig::from_env();
        if config.is_empty() {
            return None;
        }
        let quiet = std::env::var("CAQTDM_CA_QUIET").is_ok();
        if !quiet {
            eprintln!(
                "CA: addr list {:?}, name servers {:?}, max array bytes {}",
                config.addr_list, config.name_servers, config.max_array_bytes
            );
        }
        Some(Self {
            client: None,
            live: Some(LiveCa::new(config)),
            last_report: None,
            monitors: BTreeMap::new(),
            latest: BTreeMap::new(),
            next: 1,
            fault: None,
        })
    }

    pub fn stats(&self) -> Option<CaStats> {
        self.live.as_ref().map(|l| l.stats())
    }

    pub fn unconnected(&self) -> Vec<String> {
        self.live.as_ref().map(|l| l.unconnected()).unwrap_or_default()
    }

    fn report(&mut self) {
        let Some(live) = self.live.as_mut() else { return };
        if std::env::var("CAQTDM_CA_QUIET").is_ok() {
            live.take_log();
            return;
        }
        for line in live.take_log() {
            eprintln!("{line}");
        }
        let stats = live.stats();
        let now = std::time::Instant::now();
        let due = match self.last_report {
            None => true,
            Some((at, last)) => last != stats && now.duration_since(at).as_millis() >= 1000,
        };
        if due {
            eprintln!(
                "CA {:.1}s: {}/{} channels connected, {} with values",
                live.uptime().as_secs_f64(),
                stats.connected,
                stats.total,
                stats.with_value
            );
            self.last_report = Some((now, stats));
        }
    }
}

impl Drop for CaSource {
    fn drop(&mut self) {
        let Some(live) = self.live.as_ref() else { return };
        if std::env::var("CAQTDM_CA_QUIET").is_ok() {
            return;
        }
        let stats = live.stats();
        eprintln!(
            "CA final: {}/{} channels connected, {} with values",
            stats.connected, stats.total, stats.with_value
        );
        for name in live.unconnected() {
            eprintln!("CA not connected: {name}");
        }
        for line in live.valueless() {
            eprintln!("CA no value: {line}");
        }
    }
}

impl DataSource for CaSource {
    fn name(&self) -> &str {
        "epics3"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        if let Some(live) = self.live.as_mut() {
            live.add(&spec.name);
        } else if let Some(client) = self.client.as_mut() {
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
        if let Some(live) = self.live.as_mut() {
            let (text, values) = match value {
                PutValue::String(s) => (Some(s.as_str()), s.trim().parse::<f64>().ok().into_iter().collect()),
                PutValue::Doubles(v) => (None, v.clone()),
                other => (None, other.as_f64().into_iter().collect::<Vec<_>>()),
            };
            return live
                .put(&spec.name, text, &values)
                .map_err(|e| DsError::new(e.to_string()));
        }
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
        if self.live.is_some() {
            if let Some(live) = self.live.as_mut() {
                live.poll();
            }
            self.report();
            let live = self.live.as_ref().expect("live");
            return self
                .monitors
                .iter()
                .map(|(id, name)| Update {
                    id: *id,
                    data: live
                        .latest(name)
                        .cloned()
                        .unwrap_or_else(|| ChannelData::disconnected(name)),
                })
                .collect();
        }
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
