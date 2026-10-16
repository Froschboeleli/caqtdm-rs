//! Site archive retrievers from caQtDM's HIPA, PRO, and SF plugins.
//!
//! SF posts the JSON document built in `archiveSF_plugin.h` and reads the
//! binned `channel` / `data` / `globalSeconds` / `value.mean` array that
//! `sfRetrieval.cpp` walks.
//!
//! HIPA and PRO call the site `gmv()` logger. That library is not in the
//! caQtDM tree, so the bytes on the wire are the arguments `GetLogShift`
//! passes: host, 40-byte device, log id `"U"`, year offset, retrieval time,
//! and the float/int/short sample buffers `gmv` fills. The host comes from
//! the plugin (`hipa-lgexp.psi.ch`, `proscan-lgexp.psi.ch`). Sample cleanup
//! matches the loop in `hipaRetrieval.c`: fewer than five points is empty,
//! and a negative `Valerr` repeats the last good value.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::error::DsError;

pub const HIPA_HOST: &str = "hipa-lgexp.psi.ch";
pub const PRO_HOST: &str = "proscan-lgexp.psi.ch";

#[derive(Debug, Clone, PartialEq)]
pub struct GmvQuery {
    pub host: String,
    pub device: String,
    pub logid: String,
    pub annee: i16,
    pub retr_time: i32,
    pub numdev: i32,
    pub maxval: i32,
    pub tau: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GmvSample {
    pub time: i32,
    pub value: f32,
}

/// Day offset and buffer length from `GetLogShift`.
pub fn gmv_window(start_seconds: i32, hour: i32) -> (i32, i32) {
    let start_hours = start_seconds / 3600;
    let day = if hour - start_hours > 0 {
        0
    } else {
        -(start_hours / 24 + 1)
    };
    let maxval = (3600 / 5) * 24 * (-day + 1);
    (day, maxval)
}

pub fn hipa_query(device: &str, annee: i16, retr_time: i32, maxval: i32) -> GmvQuery {
    gmv_query(HIPA_HOST, device, annee, retr_time, maxval)
}

pub fn pro_query(device: &str, annee: i16, retr_time: i32, maxval: i32) -> GmvQuery {
    gmv_query(PRO_HOST, device, annee, retr_time, maxval)
}

fn gmv_query(host: &str, device: &str, annee: i16, retr_time: i32, maxval: i32) -> GmvQuery {
    GmvQuery {
        host: host.to_string(),
        device: device.to_string(),
        logid: "U".to_string(),
        annee,
        retr_time,
        numdev: 1,
        maxval,
        tau: -99,
    }
}

pub fn encode_gmv_request(query: &GmvQuery) -> Vec<u8> {
    let mut out = b"GMV1".to_vec();
    push_str(&mut out, &query.host);
    let mut device = [0u8; 40];
    let bytes = query.device.as_bytes();
    device[..bytes.len().min(40)].copy_from_slice(&bytes[..bytes.len().min(40)]);
    out.extend_from_slice(&device);
    let mut logid = [0u8; 6];
    let id = query.logid.as_bytes();
    logid[..id.len().min(6)].copy_from_slice(&id[..id.len().min(6)]);
    out.extend_from_slice(&logid);
    out.extend_from_slice(&query.annee.to_le_bytes());
    out.extend_from_slice(&query.retr_time.to_le_bytes());
    out.extend_from_slice(&query.numdev.to_le_bytes());
    out.extend_from_slice(&query.maxval.to_le_bytes());
    out.extend_from_slice(&query.tau.to_le_bytes());
    out
}

pub fn decode_gmv_request(bytes: &[u8]) -> Result<GmvQuery, DsError> {
    if bytes.len() < 4 || &bytes[..4] != b"GMV1" {
        return Err(DsError::new("gmv request magic"));
    }
    let (host, rest) = take_str(&bytes[4..])?;
    if rest.len() < 40 + 6 + 2 + 16 {
        return Err(DsError::new("gmv request is short"));
    }
    let device = unpad(&rest[..40]);
    let logid = unpad(&rest[40..46]);
    let annee = i16::from_le_bytes(rest[46..48].try_into().unwrap_or([0; 2]));
    let retr_time = i32::from_le_bytes(rest[48..52].try_into().unwrap_or([0; 4]));
    let numdev = i32::from_le_bytes(rest[52..56].try_into().unwrap_or([0; 4]));
    let maxval = i32::from_le_bytes(rest[56..60].try_into().unwrap_or([0; 4]));
    let tau = i32::from_le_bytes(rest[60..64].try_into().unwrap_or([0; 4]));
    Ok(GmvQuery {
        host,
        device,
        logid,
        annee,
        retr_time,
        numdev,
        maxval,
        tau,
    })
}

/// `values`, `times`, and `errs` are the buffers `gmv` writes, length `maxval`.
pub fn apply_gmv(values: &[f32], times: &[i32], errs: &[i16]) -> Vec<GmvSample> {
    let maxval = values.len().min(times.len()).min(errs.len());
    if maxval == 0 {
        return Vec::new();
    }
    let numval = times[maxval - 1] - 2;
    if numval < 5 {
        return Vec::new();
    }
    let count = (numval as usize).min(maxval);
    let mut last = 0.0f32;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let value = if errs[index] < 0 {
            last
        } else {
            last = values[index];
            values[index]
        };
        out.push(GmvSample {
            time: times[index],
            value,
        });
    }
    out
}

pub fn encode_gmv_reply(numval: i32, values: &[f32], times: &[i32], errs: &[i16]) -> Vec<u8> {
    let mut out = b"GMV1".to_vec();
    out.extend_from_slice(&numval.to_le_bytes());
    let n = values.len().min(times.len()).min(errs.len());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for index in 0..n {
        out.extend_from_slice(&values[index].to_le_bytes());
        out.extend_from_slice(&times[index].to_le_bytes());
        out.extend_from_slice(&errs[index].to_le_bytes());
    }
    out
}

pub fn decode_gmv_reply(bytes: &[u8]) -> Result<Vec<GmvSample>, DsError> {
    if bytes.len() < 12 || &bytes[..4] != b"GMV1" {
        return Err(DsError::new("gmv reply magic"));
    }
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap_or([0; 4])) as usize;
    let mut values = Vec::with_capacity(n);
    let mut times = Vec::with_capacity(n);
    let mut errs = Vec::with_capacity(n);
    let mut rest = &bytes[12..];
    for _ in 0..n {
        if rest.len() < 10 {
            return Err(DsError::new("gmv sample truncated"));
        }
        values.push(f32::from_le_bytes(rest[..4].try_into().unwrap_or([0; 4])));
        times.push(i32::from_le_bytes(rest[4..8].try_into().unwrap_or([0; 4])));
        errs.push(i16::from_le_bytes(rest[8..10].try_into().unwrap_or([0; 2])));
        rest = &rest[10..];
    }
    Ok(apply_gmv(&values, &times, &errs))
}

pub fn serve_gmv(
    reply: Vec<u8>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<(std::net::SocketAddr, std::sync::mpsc::Receiver<GmvQuery>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                    let mut buf = vec![0u8; 512];
                    if let Ok(n) = stream.read(&mut buf) {
                        if let Ok(query) = decode_gmv_request(&buf[..n]) {
                            let _ = tx.send(query);
                        }
                    }
                    let _ = stream.write_all(&reply);
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    Ok((addr, rx))
}

pub fn fetch_gmv(addr: std::net::SocketAddr, query: &GmvQuery) -> Result<Vec<GmvSample>, DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|err| DsError::new(err.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok();
    stream
        .write_all(&encode_gmv_request(query))
        .map_err(|err| DsError::new(err.to_string()))?;
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    decode_gmv_reply(&buf)
}

/// JSON body from `ArchiveSF_Plugin::getFromArchive`.
pub fn sf_query(
    channel: &str,
    start_seconds: f64,
    end_seconds: f64,
    backend: Option<&str>,
    bins: Option<i32>,
) -> String {
    let channels = match backend {
        Some(backend) if !backend.is_empty() => {
            format!(r#""channels": [ {{"name":"{channel}", "backend" : "{backend}" }}]"#)
        }
        _ => format!(r#""channels": [ {{"name":"{channel}" }}]"#),
    };
    let agg = match bins {
        Some(bins) => format!(
            r#", "aggregation": {{"aggregationType":"value", "aggregations":["min","mean","max"], "nrOfBins" : {bins}}}"#
        ),
        None => {
            r#", "aggregation": {"aggregationType":"value", "aggregations":["min","mean","max"], "durationPerBin" : "PT1S"}"#
                .to_string()
        }
    };
    format!(
        r#"{{"response":{{"format":"json"}},"range": {{ "startSeconds" : "{start}", "endSeconds" : "{end}"}},{channels},"fields":["channel","globalSeconds","value"]{agg}}}"#,
        start = format_g(start_seconds),
        end = format_g(end_seconds),
    )
}

fn format_g(value: f64) -> String {
    let text = format!("{value:.10}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfSample {
    pub x: f64,
    pub y: f64,
}

/// Walk the JSON array the way `sfRetrieval::downloadFinished` does for binned data.
pub fn parse_sf_response(
    text: &str,
    now_seconds: f64,
    seconds_past: f64,
    time_axis: bool,
) -> Result<Vec<SfSample>, DsError> {
    let mut samples = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"globalSeconds\"") {
        let window = &rest[at..rest.len().min(at + 240)];
        let Some(time_text) = json_string(window, "globalSeconds") else {
            rest = &rest[at + 16..];
            continue;
        };
        let Ok(archive_time) = time_text.parse::<f64>() else {
            return Err(DsError::new("sf globalSeconds is not a number"));
        };
        let mean = json_number(window, "mean")
            .or_else(|| json_number(window, "value"))
            .ok_or_else(|| DsError::new("sf sample missing mean"))?;
        if now_seconds - archive_time < seconds_past {
            let x = if time_axis {
                archive_time * 1000.0
            } else {
                -(now_seconds - archive_time) / 3600.0
            };
            samples.push(SfSample { x, y: mean });
        }
        rest = &rest[at + 16..];
    }
    if samples.is_empty() {
        Err(DsError::new("sf archive returned no samples"))
    } else {
        Ok(samples)
    }
}

pub fn serve_sf(
    body: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<(std::net::SocketAddr, std::sync::mpsc::Receiver<String>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                    let mut buf = vec![0u8; 4096];
                    if let Ok(n) = stream.read(&mut buf) {
                        let text = String::from_utf8_lossy(&buf[..n]);
                        if let Some(json) = text.split("\r\n\r\n").nth(1) {
                            let _ = tx.send(json.to_string());
                        }
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    Ok((addr, rx))
}

pub fn post_sf(addr: std::net::SocketAddr, json: &str) -> Result<Vec<SfSample>, DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|err| DsError::new(err.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok();
    let request = format!(
        "POST /archive HTTP/1.1\r\nHost: sf\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
        json.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| DsError::new(err.to_string()))?;
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or(&text);
    parse_sf_response(body, 1_700_000_100.0, 10_000.0, false)
}

fn push_str(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u16).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn take_str(bytes: &[u8]) -> Result<(String, &[u8]), DsError> {
    if bytes.len() < 2 {
        return Err(DsError::new("gmv string length"));
    }
    let len = u16::from_le_bytes(bytes[..2].try_into().unwrap_or([0; 2])) as usize;
    if bytes.len() < 2 + len {
        return Err(DsError::new("gmv string truncated"));
    }
    let text = std::str::from_utf8(&bytes[2..2 + len])
        .map_err(|_| DsError::new("gmv string utf-8"))?
        .to_string();
    Ok((text, &bytes[2 + len..]))
}

fn unpad(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn json_string(window: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let at = window.find(&key)? + key.len();
    let tail = window[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let tail = tail.strip_prefix('"')?;
    let end = tail.find('"')?;
    Some(tail[..end].to_string())
}

fn json_number(window: &str, name: &str) -> Option<f64> {
    let key = format!("\"{name}\"");
    let at = window.find(&key)? + key.len();
    let tail = window[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(tail.len());
    tail[..end].parse().ok()
}
