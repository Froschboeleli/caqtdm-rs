use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

/// Simulated GPS feed driven by the caller's clock.
///
/// Channels: `time` (unix seconds), `lat`, `lon`, `alt`, `sats`, `utc`.
/// Coordinates default to a fixed point and can be overwritten. This is not
/// a receiver driver. caQtDM's gps plugin talks to hardware; this crate
/// keeps the channel names and the time base so displays still run.
pub struct GpsSource {
    monitors: BTreeMap<u64, String>,
    next: u64,
    lat: f64,
    lon: f64,
    alt: f64,
    sats: i64,
}

impl GpsSource {
    pub fn new() -> Self {
        Self {
            monitors: BTreeMap::new(),
            next: 1,
            lat: 47.3769,
            lon: 8.5417,
            alt: 408.0,
            sats: 8,
        }
    }
}

impl Default for GpsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for GpsSource {
    fn name(&self) -> &str {
        "gps"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        self.monitors.insert(id, spec.name.to_ascii_lowercase());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown gps monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let number = value.as_f64().unwrap_or(0.0);
        match spec.name.to_ascii_lowercase().as_str() {
            "lat" => self.lat = number,
            "lon" => self.lon = number,
            "alt" => self.alt = number,
            "sats" => self.sats = number as i64,
            other => {
                return Err(DsError::new(format!("gps channel {other} is read-only")));
            }
        }
        Ok(())
    }

    fn poll(&mut self, now_ms: u64) -> Vec<Update> {
        let secs = now_ms / 1000;
        self.monitors
            .iter()
            .map(|(id, name)| {
                let mut data = ChannelData::scalar(0.0);
                data.description = format!("simulated gps {name}");
                data.value = match name.as_str() {
                    "time" => PvValue::Double(secs as f64),
                    "lat" => PvValue::Double(self.lat),
                    "lon" => PvValue::Double(self.lon),
                    "alt" => PvValue::Double(self.alt),
                    "sats" => PvValue::Int(self.sats),
                    "utc" => PvValue::String(format_utc(secs)),
                    _ => {
                        data.connected = false;
                        PvValue::String(String::new())
                    }
                };
                if !data.connected {
                    data = ChannelData::disconnected(name);
                }
                Update { id: *id, data }
            })
            .collect()
    }
}

fn format_utc(secs: u64) -> String {
    // Civil date from Unix time. Enough for a clock widget, not a timezone database.
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02}:{sec:02}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}
