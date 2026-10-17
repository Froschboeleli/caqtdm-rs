use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

/// GPS feed driven by the caller's clock, an NMEA sentence, or a gpsd JSON line.
///
/// Channels: `time` (unix seconds), `lat`, `lon`, `alt`, `sats`, `utc`.
/// With no receiver the coordinates stay at a fixed site and can be overwritten.
/// `ingest_nmea` reads GGA/RMC. `ingest_gpsd` reads a gpsd TPV or SKY object.
/// `exchange_gpsd` speaks the gpsd watch protocol on a TCP socket.
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

    /// Apply one receiver fix. Later polls publish these coordinates.
    pub fn apply_fix(&mut self, fix: &GpsFix) {
        self.lat = fix.lat;
        self.lon = fix.lon;
        self.alt = fix.alt;
        self.sats = fix.sats;
    }

    /// Parse every NMEA sentence in `text`. Returns true when a fix was applied.
    pub fn ingest_nmea(&mut self, text: &str) -> bool {
        let mut applied = false;
        for line in text.lines() {
            if let Some(fix) = parse_nmea(line) {
                self.apply_fix(&fix);
                applied = true;
            }
        }
        applied
    }

    /// Parse one gpsd JSON line (`TPV` or `SKY`).
    pub fn ingest_gpsd(&mut self, line: &str) -> bool {
        if let Some(fix) = parse_gpsd(line) {
            if line.contains("\"class\":\"TPV\"") || line.contains("\"class\": \"TPV\"") {
                self.lat = fix.lat;
                self.lon = fix.lon;
                self.alt = fix.alt;
            }
            if fix.sats >= 0 {
                self.sats = fix.sats;
            }
            return true;
        }
        false
    }
}

/// One position from a receiver.
#[derive(Debug, Clone, PartialEq)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub sats: i64,
    pub utc: String,
}

/// The line a gpsd client sends to start the JSON watch stream.
pub fn gpsd_watch_request() -> &'static str {
    "?WATCH={\"enable\":true,\"json\":true};\n"
}

/// `$GPGGA` / `$GPRMC` (and the `GN` talker). Lat/lon are `ddmm.mmm`.
pub fn parse_nmea(sentence: &str) -> Option<GpsFix> {
    let sentence = sentence.trim();
    let body = sentence.split('*').next().unwrap_or(sentence);
    let mut fields = body.split(',');
    let kind = fields.next().unwrap_or("");
    let kind = kind.trim_start_matches('$');
    if kind.ends_with("GGA") {
        let time = fields.next().unwrap_or("");
        let lat = nmea_coord(fields.next()?, fields.next()?)?;
        let lon = nmea_coord(fields.next()?, fields.next()?)?;
        let _quality = fields.next();
        let sats: i64 = fields.next().unwrap_or("0").parse().unwrap_or(0);
        let _hdop = fields.next();
        let alt: f64 = fields.next().unwrap_or("0").parse().unwrap_or(0.0);
        return Some(GpsFix {
            lat,
            lon,
            alt,
            sats,
            utc: time.to_string(),
        });
    }
    if kind.ends_with("RMC") {
        let time = fields.next().unwrap_or("");
        let status = fields.next().unwrap_or("V");
        if status != "A" {
            return None;
        }
        let lat = nmea_coord(fields.next()?, fields.next()?)?;
        let lon = nmea_coord(fields.next()?, fields.next()?)?;
        return Some(GpsFix {
            lat,
            lon,
            alt: 0.0,
            sats: -1,
            utc: time.to_string(),
        });
    }
    None
}

fn nmea_coord(raw: &str, hemi: &str) -> Option<f64> {
    if raw.is_empty() {
        return None;
    }
    let value: f64 = raw.parse().ok()?;
    let degrees = (value / 100.0).floor();
    let minutes = value - degrees * 100.0;
    let mut signed = degrees + minutes / 60.0;
    if hemi.eq_ignore_ascii_case("S") || hemi.eq_ignore_ascii_case("W") {
        signed = -signed;
    }
    Some(signed)
}

/// gpsd `TPV` carries lat/lon/alt. `SKY` carries `uSat` or a `satellites` array.
pub fn parse_gpsd(line: &str) -> Option<GpsFix> {
    let line = line.trim();
    if !line.contains("\"class\"") {
        return None;
    }
    if line.contains("TPV") {
        let lat = json_f64(line, "lat")?;
        let lon = json_f64(line, "lon")?;
        let alt = json_f64(line, "alt").unwrap_or(0.0);
        let utc = json_string(line, "time").unwrap_or_default();
        return Some(GpsFix {
            lat,
            lon,
            alt,
            sats: -1,
            utc,
        });
    }
    if line.contains("SKY") {
        let sats = json_f64(line, "uSat")
            .map(|n| n as i64)
            .unwrap_or_else(|| used_satellites(line));
        return Some(GpsFix {
            lat: 0.0,
            lon: 0.0,
            alt: 0.0,
            sats,
            utc: String::new(),
        });
    }
    None
}

fn used_satellites(line: &str) -> i64 {
    line.matches("\"used\":true").count() as i64 + line.matches("\"used\": true").count() as i64
}

fn json_f64(text: &str, key: &str) -> Option<f64> {
    let pattern = format!("\"{key}\"");
    let at = text.find(&pattern)? + pattern.len();
    let rest = text[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .unwrap_or(rest.len());
    rest[..end].trim_matches('"').parse().ok()
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let at = text.find(&pattern)? + pattern.len();
    let rest = text[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Connect to gpsd, enable the JSON watch, and return the first TPV fix.
pub fn exchange_gpsd(addr: std::net::SocketAddr) -> Result<GpsFix, DsError> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))
        .map_err(|err| DsError::new(format!("gpsd connect: {err}")))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok();
    stream
        .write_all(gpsd_watch_request().as_bytes())
        .map_err(|err| DsError::new(format!("gpsd watch: {err}")))?;
    let reader = BufReader::new(stream);
    for line in reader.lines().take(8) {
        let line = line.map_err(|err| DsError::new(format!("gpsd read: {err}")))?;
        if let Some(fix) = parse_gpsd(&line) {
            if line.contains("TPV") {
                return Ok(fix);
            }
        }
    }
    Err(DsError::new("gpsd sent no TPV"))
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
