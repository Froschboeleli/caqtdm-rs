use std::collections::BTreeMap;

use caqtdm_core::{parse_channel, ChannelData, PutValue};

use crate::error::DsError;
use crate::source::DataSource;

struct Route {
    plugin: String,
    local_id: u64,
}

/// Routes `plugin://name` monitors to the registered [`DataSource`].
pub struct Hub {
    sources: BTreeMap<String, Box<dyn DataSource>>,
    routes: BTreeMap<u64, Route>,
    next_id: u64,
    default_plugin: String,
}

impl Hub {
    pub fn new(default_plugin: impl Into<String>) -> Self {
        Self {
            sources: BTreeMap::new(),
            routes: BTreeMap::new(),
            next_id: 1,
            default_plugin: default_plugin.into(),
        }
    }

    pub fn default_plugin(&self) -> &str {
        &self.default_plugin
    }

    pub fn register(&mut self, source: Box<dyn DataSource>) {
        let name = source.name().to_ascii_lowercase();
        self.sources.insert(name, source);
    }

    pub fn plugins(&self) -> Vec<String> {
        self.sources.keys().cloned().collect()
    }

    pub fn monitor(&mut self, raw: &str) -> Result<u64, DsError> {
        let spec = parse_channel(raw, &self.default_plugin);
        if spec.name.is_empty() {
            return Err(DsError::new("empty channel name"));
        }
        let plugin = spec.plugin.to_ascii_lowercase();
        let source = self
            .sources
            .get_mut(&plugin)
            .ok_or_else(|| DsError::new(format!("no control plugin '{plugin}'")))?;
        let local_id = source.add_monitor(&spec)?;
        let id = self.next_id;
        self.next_id += 1;
        self.routes.insert(id, Route { plugin, local_id });
        Ok(id)
    }

    pub fn clear(&mut self, id: u64) -> Result<(), DsError> {
        let Some(route) = self.routes.remove(&id) else {
            return Err(DsError::new(format!("unknown monitor {id}")));
        };
        if let Some(source) = self.sources.get_mut(&route.plugin) {
            source.clear_monitor(route.local_id)?;
        }
        Ok(())
    }

    pub fn put(&mut self, raw: &str, value: &PutValue) -> Result<(), DsError> {
        let spec = parse_channel(raw, &self.default_plugin);
        let plugin = spec.plugin.to_ascii_lowercase();
        let source = self
            .sources
            .get_mut(&plugin)
            .ok_or_else(|| DsError::new(format!("no control plugin '{plugin}'")))?;
        source.set_value(&spec, value)
    }

    pub fn poll(&mut self, now_ms: u64) -> Vec<(u64, ChannelData)> {
        let mut local: BTreeMap<(String, u64), ChannelData> = BTreeMap::new();
        for (name, source) in &mut self.sources {
            for update in source.poll(now_ms) {
                local.insert((name.clone(), update.id), update.data);
            }
        }
        let mut out = Vec::new();
        for (id, route) in &self.routes {
            if let Some(data) = local.get(&(route.plugin.clone(), route.local_id)) {
                out.push((*id, data.clone()));
            }
        }
        out
    }

    pub fn describe(&self, raw: &str) -> String {
        let spec = parse_channel(raw, &self.default_plugin);
        let plugin = spec.plugin.to_ascii_lowercase();
        self.sources
            .get(&plugin)
            .map(|s| s.describe(&spec))
            .unwrap_or_else(|| format!("unregistered plugin {plugin}"))
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new("epics3")
    }
}
