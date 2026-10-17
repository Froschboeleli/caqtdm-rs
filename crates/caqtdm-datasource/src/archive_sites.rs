//! Site archive retrievers from caQtDM's HIPA, PRO, and SF plugins.
//!
//! SF posts the JSON document built in `archiveSF_plugin.h` and reads the
//! binned `channel` / `data` / `globalSeconds` / `value.mean` array that
//! `sfRetrieval.cpp` walks.
//!
//! HIPA and PRO call `gmv()` in `libNewLogRPC.a` (`libNewLogRPC_64.a` on
//! 64-bit). That static library is not in the caQtDM tree, and the project's
//! build says it will skip the plugin when the archive is missing. What a
//! display sees is `GetLogShift`: the arguments in `hipaRetrieval.c` and
//! `proRetrieval.c`, `ExactTime`, the `Valerr` hold-last rule, and the
//! plugin's `-startHours` filter. Those steps are public and tested.
//! `encode_gmv_request` is only the loopback stand-in that carries the same
//! arguments. It is not the RPC framing inside `libNewLogRPC`.

#![cfg_attr(not(feature = "libgmv"), allow(dead_code))]

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

/// Pack a log timestamp the way `cvtms_c` is used.
///
/// `GetLogShift` calls `cvtms_c(yearday, 0, 0, 1, 0, &retr_time)`. The sample
/// path calls `cnvms_c` and then `ExactTime`. A day-of-year plus milliseconds
/// does not fit in the 32-bit `Timef` those functions share, and the retrieval
/// call passes milliseconds 0, so this packing is seconds from the start of
/// the year. `unpack_log_time` is the inverse `cnvms_c` shape `(day, hour,
/// minute, second, millis)`.
pub fn pack_log_time(day: i32, hour: i32, minute: i32, second: i32) -> i32 {
    day * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Inverse of [`pack_log_time`]. Milliseconds stay 0.
pub fn unpack_log_time(timef: i32) -> (i32, i32, i32, i32, i32) {
    let day = timef.div_euclid(86_400);
    let rest = timef.rem_euclid(86_400);
    let hour = rest / 3_600;
    let minute = (rest % 3_600) / 60;
    let second = rest % 60;
    (day, hour, minute, second, 0)
}

/// `ExactTime` in `hipaRetrieval.c`: day plus the fraction of the day.
pub fn exact_time(timef: i32) -> f64 {
    let (day, hour, minute, second, millis) = unpack_log_time(timef);
    let daytime = f64::from(hour)
        + f64::from(minute) / 60.0
        + f64::from(second) / 3600.0
        + f64::from(millis) / 3_600_000.0;
    f64::from(day) + daytime / 24.0
}

/// Hours relative to `now_day`, matching the loop that rewrites `Timer`.
pub fn hours_from_now(timef: i32, now_day: f64) -> f64 {
    (exact_time(timef) - now_day) * 24.0
}

/// Day of year from `cvdate_c(year, month, day, &yearday, ...)`. Weekday is unused.
pub fn day_of_year(year: i32, month: i32, day: i32) -> i32 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let mdays = [
        0,
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let month = month.clamp(1, 12) as usize;
    let mut doy = day;
    for days in mdays.iter().take(month).skip(1) {
        doy += *days;
    }
    doy
}

/// `annee` in `GetLogShift`: years from 2000, or from 1900 before that.
pub fn logging_year_code(year: i32) -> i16 {
    if year >= 2000 {
        (year - 2000) as i16
    } else {
        (year - 1900) as i16
    }
}

/// One `GetLogShift` call. `node` is the string passed to `gmv`. Both C files
/// pass `hipa-lgexp.psi.ch`. `logging_server` is the `LOGGINGSERVER` value the
/// plugin sets before the call.
#[derive(Debug, Clone, PartialEq)]
pub struct LogCall {
    pub node: &'static str,
    pub logging_server: &'static str,
    pub device: String,
    pub logid: &'static str,
    pub annee: i16,
    pub yearday: i32,
    pub retr_time: i32,
    pub numdev: i32,
    pub maxval: i32,
    pub tau: i32,
    pub now: f64,
    pub start_hours: i32,
}

/// Local clock `GetActualTime` fills before `cvdate_c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogClock {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

/// Build the `gmv` arguments `GetLogShift` computes from the clock and `startSeconds`.
pub fn prepare_log_call(hipa: bool, clock: LogClock, start_seconds: i32, device: &str) -> LogCall {
    let LogClock {
        year,
        month,
        day,
        hour,
        minute,
        second,
    } = clock;
    let start_hours = start_seconds / 3600;
    let shift = if hour - start_hours > 0 {
        0
    } else {
        -(start_hours / 24 + 1)
    };
    let mut yearday = day_of_year(year, month, day) + shift;
    if yearday < 1 {
        yearday = 1;
    }
    let maxval = (3600 / 5) * 24 * (-shift + 1);
    let now = f64::from(day_of_year(year, month, day))
        + (f64::from(hour) + f64::from(minute) / 60.0 + f64::from(second) / 3600.0) / 24.0;
    let mut name = device.as_bytes().to_vec();
    name.truncate(40);
    LogCall {
        node: HIPA_HOST,
        logging_server: if hipa { HIPA_HOST } else { PRO_HOST },
        device: String::from_utf8_lossy(&name).into_owned(),
        logid: "U",
        annee: logging_year_code(year),
        yearday,
        retr_time: pack_log_time(yearday, 0, 0, 1),
        numdev: 1,
        maxval,
        tau: -99,
        now,
        start_hours,
    }
}

/// `GetLogShift` after `gmv` returns. `numval` is the count `gmv` writes.
/// `Timef[Maxval - 1] - 2` is `nbVal`. A negative `Valerr` repeats the last
/// good value. The first `numval - 2` times become hours before `now`.
/// `numval < 5` is an empty archive, which is what the plugin plots.
pub fn reduce_log(
    numval: i32,
    maxval: i32,
    values: &[f32],
    times: &[i32],
    errs: &[i16],
    now: f64,
) -> Vec<(f64, f32)> {
    if numval < 5 || maxval <= 0 || times.len() < maxval as usize {
        return Vec::new();
    }
    let nb = times[maxval as usize - 1] - 2;
    if nb <= 0 {
        return Vec::new();
    }
    let count = (nb as usize)
        .min(values.len())
        .min(times.len())
        .min(errs.len());
    let mut last = 0.0f32;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let value = if errs[index] < 0 {
            last
        } else {
            last = values[index];
            values[index]
        };
        out.push((exact_time(times[index]), value));
    }
    let hours = (numval - 2).max(0) as usize;
    for sample in out.iter_mut().take(hours) {
        sample.0 = (sample.0 - now) * 24.0;
    }
    out
}

/// The plugin keeps samples whose time is at least `-startHours`.
pub fn filter_log_window(samples: &[(f64, f32)], start_hours: i32) -> Vec<(f64, f32)> {
    samples
        .iter()
        .copied()
        .filter(|(time, _)| *time >= -f64::from(start_hours))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_log_shift_matches_every_public_call_site() {
        assert_eq!(day_of_year(2026, 1, 1), 1);
        assert_eq!(day_of_year(2024, 3, 1), 61);
        assert_eq!(day_of_year(2025, 3, 1), 60);
        assert_eq!(logging_year_code(1998), 98);
        assert_eq!(logging_year_code(2026), 26);

        let hipa = prepare_log_call(
            true,
            LogClock {
                year: 2026,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
            },
            48 * 3600,
            &"D".repeat(50),
        );
        assert_eq!(hipa.node, HIPA_HOST);
        assert_eq!(hipa.logging_server, HIPA_HOST);
        assert_eq!(hipa.device.len(), 40);
        assert_eq!(hipa.logid, "U");
        assert_eq!(hipa.tau, -99);
        assert_eq!(hipa.numdev, 1);
        assert_eq!(hipa.yearday, 1, "a shift before day 1 clamps");
        assert_eq!(hipa.retr_time, pack_log_time(1, 0, 0, 1));
        assert!((exact_time(hipa.retr_time) - (1.0 + 1.0 / 86_400.0)).abs() < 1e-12);

        let pro = prepare_log_call(
            false,
            LogClock {
                year: 2026,
                month: 10,
                day: 17,
                hour: 12,
                minute: 0,
                second: 0,
            },
            3600,
            "MHP1",
        );
        assert_eq!(pro.node, HIPA_HOST, "proRetrieval.c passes the HIPA node");
        assert_eq!(pro.logging_server, PRO_HOST);
        assert_eq!(pro.yearday, day_of_year(2026, 10, 17));
        assert_eq!(pro.maxval, (3600 / 5) * 24);

        let early = prepare_log_call(
            true,
            LogClock {
                year: 2026,
                month: 6,
                day: 15,
                hour: 2,
                minute: 30,
                second: 0,
            },
            3 * 3600,
            "MHP1",
        );
        assert_eq!(early.yearday, day_of_year(2026, 6, 15) - 1);

        let times = [10, 20, 30, 40, 50, 8];
        let values = [1.5f32, 2.5, 3.5, 9.0, 4.5, 5.5];
        let errs = [0i16, 0, -1, 0, 1, 0];
        assert!(reduce_log(4, 6, &values, &times, &errs, 10.5).is_empty());
        let samples = reduce_log(8, 6, &values, &times, &errs, exact_time(10));
        assert_eq!(samples.len(), 6);
        assert_eq!(
            samples[2].1, 2.5,
            "a negative Valerr repeats the last good value"
        );
        assert!((samples[0].0).abs() < 1e-9);
        assert!((samples[1].0 - hours_from_now(20, exact_time(10))).abs() < 1e-9);
        let kept = filter_log_window(&[(-2.0, 1.0), (-1.0, 2.0), (0.5, 3.0)], 1);
        assert_eq!(kept, vec![(-1.0, 2.0), (0.5, 3.0)]);
    }
}
