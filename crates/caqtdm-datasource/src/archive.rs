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

/// One archiveHTTP retrieval, matching `UrlHandlerHttp::assembleUrl`.
///
/// The live PSI host is not required. [`serve_archive_http`] answers the same
/// path on the loopback, and [`values_in_window`] applies the request to a fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveHttpQuery {
    pub host: String,
    pub https: bool,
    pub backend: String,
    pub channel: String,
    pub begin_secs: i64,
    pub end_secs: i64,
    /// `nrOfBins`. Zero or negative selects `/api/4/events`. A positive count
    /// selects `/api/4/binned` and adds `binCount`.
    pub bins: i32,
    pub raw_path: String,
    pub binned_path: String,
}

impl ArchiveHttpQuery {
    pub fn defaults() -> Self {
        let mut host = std::env::var("CAQTDM_ARCHIVEHTTP_URL")
            .unwrap_or_else(|_| "data-api.psi.ch".to_string());
        let mut https = false;
        if let Some(rest) = host.strip_prefix("https://") {
            https = true;
            host = rest.to_string();
        } else if let Some(rest) = host.strip_prefix("http://") {
            host = rest.to_string();
        }
        host = host.trim_end_matches('/').to_string();
        Self {
            host,
            https,
            backend: std::env::var("CAQTDM_ARCHIVEHTTP_OVERRIDE_BACKEND")
                .unwrap_or_else(|_| "sf-archiver".to_string()),
            channel: String::new(),
            begin_secs: 0,
            end_secs: 0,
            bins: 0,
            raw_path: std::env::var("CAQTDM_ARCHIVEHTTP_APIPATH_RAW")
                .unwrap_or_else(|_| "/api/4/events".to_string()),
            binned_path: std::env::var("CAQTDM_ARCHIVEHTTP_APIPATH_BINNED")
                .unwrap_or_else(|_| "/api/4/binned".to_string()),
        }
    }

    /// `endSeconds = now`, `startSeconds = end - secondsPast`, as `WorkerHTTP::getFromArchive`.
    pub fn from_window(channel: &str, end_ms: u64, seconds_past: u64, bins: i32) -> Self {
        let end_secs = (end_ms / 1000) as i64;
        let begin_secs = end_secs.saturating_sub(seconds_past as i64);
        Self {
            channel: channel.to_string(),
            begin_secs,
            end_secs,
            bins,
            ..Self::defaults()
        }
    }

    pub fn url(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        let path = if self.bins > 0 {
            &self.binned_path
        } else {
            &self.raw_path
        };
        let begin = iso_utc(self.begin_secs);
        let end = iso_utc(self.end_secs);
        let mut query = format!(
            "backend={}&channelName={}&begDate={}&endDate={}",
            pct(&self.backend),
            pct(&self.channel),
            pct(&begin),
            pct(&end)
        );
        if self.bins > 0 {
            query.push_str(&format!("&binCount={}", self.bins));
        }
        query.push_str("&allowLargeResult=true");
        format!(
            "{scheme}://{host}{path}?{query}",
            host = self.host,
            path = path
        )
    }
}

/// Samples inside `[begin, end]`, averaged into `bins` buckets when `bins > 0`.
pub fn values_in_window(
    samples: &[ArchiveSample],
    begin_ms: u64,
    end_ms: u64,
    bins: i32,
) -> Vec<f64> {
    let window: Vec<&ArchiveSample> = samples
        .iter()
        .filter(|sample| sample.time_ms >= begin_ms && sample.time_ms <= end_ms)
        .collect();
    if bins <= 0 || window.is_empty() {
        return window.iter().map(|sample| sample.value).collect();
    }
    let span = end_ms.saturating_sub(begin_ms).max(1);
    let mut sums = vec![0.0; bins as usize];
    let mut counts = vec![0u32; bins as usize];
    for sample in window {
        let mut index =
            ((sample.time_ms - begin_ms) as u128 * bins as u128 / span as u128) as usize;
        if index >= bins as usize {
            index = bins as usize - 1;
        }
        sums[index] += sample.value;
        counts[index] += 1;
    }
    sums.iter()
        .zip(counts)
        .filter(|(_, count)| *count > 0)
        .map(|(sum, count)| sum / f64::from(count))
        .collect()
}

/// Loopback stand-in for `data-api.psi.ch`. The request must use the archiveHTTP path.
pub fn serve_archive_http(
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
                    let mut buf = [0u8; 4096];
                    let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
                    let text = String::from_utf8_lossy(&buf[..n]);
                    let ok = text.contains("/api/4/events")
                        || text.contains("/api/4/binned")
                        || text.contains("/api/4/backend/list");
                    let payload = if ok {
                        body.as_str()
                    } else {
                        "missing archiveHTTP path"
                    };
                    let status = if ok { "200 OK" } else { "404 Not Found" };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
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

pub fn fetch_archive_http(
    addr: std::net::SocketAddr,
    query: &ArchiveHttpQuery,
) -> Result<String, DsError> {
    let mut stream = std::net::TcpStream::connect(addr)
        .map_err(|err| DsError::new(format!("archiveHTTP connect: {err}")))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .ok();
    let url = query.url();
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.find('/').map(|i| &rest[i..]))
        .unwrap_or("/");
    let request = format!("GET {path} HTTP/1.1\r\nHost: archive\r\nConnection: close\r\n\r\n");
    std::io::Write::write_all(&mut stream, request.as_bytes())
        .map_err(|err| DsError::new(format!("archiveHTTP write: {err}")))?;
    let mut buf = Vec::new();
    let _ = std::io::Read::read_to_end(&mut stream, &mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    if !text.contains("200 OK") {
        return Err(DsError::new(format!(
            "archiveHTTP request was not accepted: {text}"
        )));
    }
    Ok(text.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

fn iso_utc(unix_secs: i64) -> String {
    let secs = unix_secs.rem_euclid(86_400) as u32;
    let z = unix_secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = (if month <= 2 { y + 1 } else { y }) as i32;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

fn pct(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn archive_http_url_matches_the_plugin_query() {
        let query = ArchiveHttpQuery {
            host: "data-api.psi.ch".into(),
            https: false,
            backend: "sf-archiver".into(),
            channel: "SR:VAC:P".into(),
            begin_secs: 0,
            end_secs: 60,
            bins: 0,
            raw_path: "/api/4/events".into(),
            binned_path: "/api/4/binned".into(),
        };
        let url = query.url();
        assert!(url.starts_with("http://data-api.psi.ch/api/4/events?"));
        assert!(url.contains("backend=sf-archiver"));
        assert!(url.contains("channelName=SR%3AVAC%3AP"));
        assert!(url.contains("begDate=1970-01-01T00%3A00%3A00Z"));
        assert!(url.contains("endDate=1970-01-01T00%3A01%3A00Z"));
        assert!(url.ends_with("&allowLargeResult=true"));

        let binned = ArchiveHttpQuery { bins: 4, ..query };
        assert!(binned.url().contains("/api/4/binned?"));
        assert!(binned.url().contains("binCount=4"));
    }

    #[test]
    fn mock_bins_and_the_loopback_path() {
        let samples = vec![
            ArchiveSample {
                time_ms: 0,
                value: 1.0,
                severity: Severity::NoAlarm,
            },
            ArchiveSample {
                time_ms: 500,
                value: 3.0,
                severity: Severity::NoAlarm,
            },
            ArchiveSample {
                time_ms: 1_500,
                value: 10.0,
                severity: Severity::NoAlarm,
            },
        ];
        let raw = values_in_window(&samples, 0, 2_000, 0);
        assert_eq!(raw, vec![1.0, 3.0, 10.0]);
        let binned = values_in_window(&samples, 0, 2_000, 2);
        assert_eq!(binned, vec![2.0, 10.0]);

        let stop = Arc::new(AtomicBool::new(false));
        let addr = serve_archive_http("[1,2]".into(), Arc::clone(&stop)).unwrap();
        let query = ArchiveHttpQuery {
            host: addr.ip().to_string(),
            https: false,
            backend: "sf-archiver".into(),
            channel: "VAC".into(),
            begin_secs: 0,
            end_secs: 60,
            bins: 0,
            raw_path: "/api/4/events".into(),
            binned_path: "/api/4/binned".into(),
        };
        let body = fetch_archive_http(addr, &query).unwrap();
        assert_eq!(body, "[1,2]");
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
