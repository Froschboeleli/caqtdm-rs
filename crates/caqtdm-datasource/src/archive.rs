//! Archive URL builder and a text fixture reader.
//!
//! caQtDM's archive plugin talks to a channel archiver over HTTP. This crate
//! builds the same style of request URL and plays back a local fixture so
//! displays and tests have waveforms without a network service.
//!
//! Fixture syntax, one sample per line after a `channel` header:
//!
//! ```text
//! channel SR:VAC:P
//! 0,1.2,0
//! 1000,1.5,1
//! ```
//!
//! Columns are time in milliseconds, value, and EPICS severity (0..3).

use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue, Severity};

use crate::error::DsError;
use crate::source::{DataSource, Update};

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveSample {
    pub time_ms: u64,
    pub value: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveRequest {
    pub channel: String,
    pub from_ms: u64,
    pub to_ms: u64,
}

pub fn build_archive_url(base: &str, channel: &str, from_ms: u64, to_ms: u64) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/data?channel={channel}&from={from_ms}&to={to_ms}")
}

pub fn parse_archive_request(name: &str) -> ArchiveRequest {
    let (channel, query) = name.split_once('?').unwrap_or((name, ""));
    let mut from_ms = 0;
    let mut to_ms = u64::MAX;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "from" => from_ms = v.parse().unwrap_or(0),
                "to" => to_ms = v.parse().unwrap_or(u64::MAX),
                _ => {}
            }
        }
    }
    ArchiveRequest {
        channel: channel.to_string(),
        from_ms,
        to_ms,
    }
}

pub fn parse_fixture(text: &str) -> Result<BTreeMap<String, Vec<ArchiveSample>>, DsError> {
    let mut out: BTreeMap<String, Vec<ArchiveSample>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("channel ") {
            current = Some(name.trim().to_string());
            out.entry(name.trim().to_string()).or_default();
            continue;
        }
        let Some(name) = current.clone() else {
            return Err(DsError::new(format!(
                "archive sample before a channel header: {line}"
            )));
        };
        let mut cols = line.split(',');
        let time_ms = cols
            .next()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| DsError::new(format!("bad archive time in {line}")))?;
        let value = cols
            .next()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| DsError::new(format!("bad archive value in {line}")))?;
        let sev_n: i16 = cols.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        out.entry(name).or_default().push(ArchiveSample {
            time_ms,
            value,
            severity: Severity::from_epics(sev_n),
        });
    }
    Ok(out)
}

pub struct ArchiveSource {
    series: BTreeMap<String, Vec<ArchiveSample>>,
    monitors: BTreeMap<u64, ArchiveRequest>,
    next: u64,
}

impl ArchiveSource {
    pub fn from_fixture(text: &str) -> Result<Self, DsError> {
        Ok(Self {
            series: parse_fixture(text)?,
            monitors: BTreeMap::new(),
            next: 1,
        })
    }

    pub fn empty() -> Self {
        Self {
            series: BTreeMap::new(),
            monitors: BTreeMap::new(),
            next: 1,
        }
    }
}

impl DataSource for ArchiveSource {
    fn name(&self) -> &str {
        "archive"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        self.monitors.insert(id, parse_archive_request(&spec.name));
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown archive monitor {id}")))
    }

    fn set_value(&mut self, _spec: &ChannelSpec, _value: &PutValue) -> Result<(), DsError> {
        Err(DsError::new("archive channels are read-only"))
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        self.monitors
            .iter()
            .map(|(id, req)| {
                let samples = self
                    .series
                    .get(&req.channel)
                    .map(|all| {
                        all.iter()
                            .filter(|s| s.time_ms >= req.from_ms && s.time_ms <= req.to_ms)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut data = if samples.is_empty() {
                    ChannelData::disconnected(&req.channel)
                } else {
                    let mut data =
                        ChannelData::scalar(samples.last().map(|s| s.value).unwrap_or(0.0));
                    data.value = PvValue::Doubles(samples.iter().map(|s| s.value).collect());
                    data.element_count = samples.len();
                    data.severity = samples
                        .last()
                        .map(|s| s.severity)
                        .unwrap_or(Severity::NoAlarm);
                    data
                };
                data.description = format!("archive {}", req.channel);
                Update { id: *id, data }
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        let req = parse_archive_request(&spec.name);
        build_archive_url("archive:/", &req.channel, req.from_ms, req.to_ms)
    }
}

/// Archiver Appliance `getData.json` document for one channel.
pub fn samples_to_json(channel: &str, samples: &[ArchiveSample]) -> String {
    let mut data = String::from("[");
    for (index, sample) in samples.iter().enumerate() {
        if index > 0 {
            data.push(',');
        }
        let secs = sample.time_ms / 1000;
        let nanos = (sample.time_ms % 1000) * 1_000_000;
        data.push_str(&format!(
            "{{\"secs\":{secs},\"nanos\":{nanos},\"val\":{},\"severity\":{}}}",
            sample.value,
            severity_code(sample.severity)
        ));
    }
    data.push(']');
    format!("[{{\"meta\":{{\"name\":\"{channel}\"}},\"data\":{data}}}]")
}

pub fn parse_archiver_json(text: &str) -> Result<Vec<ArchiveSample>, DsError> {
    let mut samples = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"secs\"") {
        let window = &rest[at..rest.len().min(at + 200)];
        let secs =
            json_field(window, "secs").ok_or_else(|| DsError::new("archive json missing secs"))?;
        let nanos = json_field(window, "nanos").unwrap_or(0.0);
        let value =
            json_field(window, "val").ok_or_else(|| DsError::new("archive json missing val"))?;
        let severity = json_field(window, "severity").unwrap_or(0.0);
        samples.push(ArchiveSample {
            time_ms: (secs * 1000.0 + nanos / 1_000_000.0) as u64,
            value,
            severity: Severity::from_epics(severity as i16),
        });
        rest = &rest[at + 6..];
    }
    if samples.is_empty() {
        Err(DsError::new("archive json contained no samples"))
    } else {
        Ok(samples)
    }
}

fn severity_code(severity: Severity) -> i16 {
    match severity {
        Severity::NoAlarm => 0,
        Severity::Minor => 1,
        Severity::Major => 2,
        Severity::Invalid => 3,
        Severity::NotConnected => 99,
    }
}

fn json_field(window: &str, name: &str) -> Option<f64> {
    let key = format!("\"{name}\"");
    let at = window.find(&key)? + key.len();
    let tail = window[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(tail.len());
    tail[..end].parse().ok()
}

/// Serve one JSON document for `GET /retrieval/data/getData.json`.
pub fn serve_archiver(
    body: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::net::SocketAddr> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(300)));
                    let mut buf = [0u8; 2048];
                    let _ = std::io::Read::read(&mut stream, &mut buf);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    Ok(addr)
}

pub fn fetch_archiver(
    addr: std::net::SocketAddr,
    channel: &str,
) -> Result<Vec<ArchiveSample>, DsError> {
    let mut stream = std::net::TcpStream::connect(addr)
        .map_err(|err| DsError::new(format!("archive connect: {err}")))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .ok();
    let request = format!(
        "GET /retrieval/data/getData.json?pv={channel} HTTP/1.1\r\nHost: archive\r\nConnection: close\r\n\r\n"
    );
    std::io::Write::write_all(&mut stream, request.as_bytes())
        .map_err(|err| DsError::new(format!("archive write: {err}")))?;
    let mut buf = Vec::new();
    let _ = std::io::Read::read_to_end(&mut stream, &mut buf);
    let text = String::from_utf8_lossy(&buf);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);
    parse_archiver_json(body)
}
