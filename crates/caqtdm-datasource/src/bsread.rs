//! Simulated SwissFEL bsread pulse stream.
//!
//! The real plugin subscribes to a 0MQ stream of structured pulse messages.
//! This source advances a pulse id from `now_ms` and synthesizes a scalar or
//! a short waveform so strip charts still move. It does not speak bsread.

use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

pub struct BsreadSource {
    monitors: BTreeMap<u64, String>,
    next: u64,
    /// Pulse spacing in milliseconds. 10 ms is a 100 Hz machine.
    period_ms: u64,
}

impl BsreadSource {
    pub fn new() -> Self {
        Self {
            monitors: BTreeMap::new(),
            next: 1,
            period_ms: 10,
        }
    }
}

impl Default for BsreadSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for BsreadSource {
    fn name(&self) -> &str {
        "bsread"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let id = self.next;
        self.next += 1;
        self.monitors.insert(id, spec.name.clone());
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown bsread monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, _value: &PutValue) -> Result<(), DsError> {
        Err(DsError::new(format!(
            "bsread channel {} is a pulse stream and cannot be written",
            spec.name
        )))
    }

    fn poll(&mut self, now_ms: u64) -> Vec<Update> {
        let pulse = now_ms / self.period_ms;
        self.monitors
            .iter()
            .map(|(id, name)| {
                let mut data = ChannelData::scalar(0.0);
                data.description = format!("bsread pulse {pulse}");
                data.value = if name == "pulse_id" {
                    PvValue::Int(pulse as i64)
                } else if name.ends_with(":wf") {
                    let samples: Vec<f64> = (0..16)
                        .map(|i| ((pulse as f64) * 0.05 + i as f64).sin())
                        .collect();
                    data.element_count = samples.len();
                    PvValue::Doubles(samples)
                } else {
                    PvValue::Double((pulse as f64 * 0.07).sin() * 10.0)
                };
                Update { id: *id, data }
            })
            .collect()
    }

    fn describe(&self, spec: &ChannelSpec) -> String {
        format!("bsread {} (simulated pulse, no 0MQ)", spec.name)
    }
}

/// One bsread value. The wire form is three length-prefixed parts: the main
/// header JSON, the data header JSON, and little-endian channel bytes.
/// This is the bsread message layout without a ZeroMQ transport.
#[derive(Debug, Clone, PartialEq)]
pub struct BsMessage {
    pub pulse_id: u64,
    pub channel: String,
    pub value: f64,
}

pub fn bsread_parts(message: &BsMessage) -> [Vec<u8>; 3] {
    let main = format!(
        "{{\"htype\":\"bsr_m-1.1\",\"pulse_id\":{}}}",
        message.pulse_id
    );
    let data_header = format!(
        "{{\"htype\":\"bsr_d-1.1\",\"channels\":[{{\"name\":\"{}\",\"type\":\"float64\"}}]}}",
        message.channel
    );
    [
        main.into_bytes(),
        data_header.into_bytes(),
        message.value.to_le_bytes().to_vec(),
    ]
}

pub fn decode_bsread_parts(parts: &[Vec<u8>]) -> Result<BsMessage, DsError> {
    if parts.len() < 3 {
        return Err(DsError::new("bsread multipart is short"));
    }
    let mut packed = Vec::new();
    for part in parts.iter().take(3) {
        push_part(&mut packed, part);
    }
    decode_bsread(&packed)
}

pub fn encode_bsread(message: &BsMessage) -> Vec<u8> {
    let main = format!(
        "{{\"htype\":\"bsr_m-1.1\",\"pulse_id\":{}}}",
        message.pulse_id
    );
    let data_header = format!(
        "{{\"htype\":\"bsr_d-1.1\",\"channels\":[{{\"name\":\"{}\",\"type\":\"float64\"}}]}}",
        message.channel
    );
    let mut out = Vec::new();
    push_part(&mut out, main.as_bytes());
    push_part(&mut out, data_header.as_bytes());
    push_part(&mut out, &message.value.to_le_bytes());
    out
}

pub fn decode_bsread(bytes: &[u8]) -> Result<BsMessage, DsError> {
    let (main, rest) = take_part(bytes)?;
    let (header, rest) = take_part(rest)?;
    let (data, _) = take_part(rest)?;
    let main = std::str::from_utf8(main).map_err(|_| DsError::new("bsread header is not utf-8"))?;
    let header =
        std::str::from_utf8(header).map_err(|_| DsError::new("bsread data header is not utf-8"))?;
    if !main.contains("bsr_m-1.1") || !header.contains("bsr_d-1.1") {
        return Err(DsError::new("bsread htype mismatch"));
    }
    let pulse_id = json_u64(main, "pulse_id").unwrap_or(0);
    let channel = json_string(header, "name").unwrap_or_default();
    if data.len() < 8 {
        return Err(DsError::new("bsread value is short"));
    }
    let value = f64::from_le_bytes(data[..8].try_into().unwrap_or([0; 8]));
    Ok(BsMessage {
        pulse_id,
        channel,
        value,
    })
}

fn push_part(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn take_part(bytes: &[u8]) -> Result<(&[u8], &[u8]), DsError> {
    if bytes.len() < 4 {
        return Err(DsError::new("bsread part header truncated"));
    }
    let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if bytes.len() < 4 + len {
        return Err(DsError::new("bsread part truncated"));
    }
    Ok((&bytes[4..4 + len], &bytes[4 + len..]))
}

fn json_u64(text: &str, key: &str) -> Option<u64> {
    let at = text.find(&format!("\"{key}\""))? + key.len() + 2;
    let tail = text[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    tail[..end].parse().ok()
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let token = format!("\"{key}\"");
    let at = text.find(&token)? + token.len();
    let tail = text[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let tail = tail.strip_prefix('"')?;
    let end = tail.find('"')?;
    Some(tail[..end].to_string())
}

pub fn serve_bsread(
    message: BsMessage,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::net::SocketAddr> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let bytes = encode_bsread(&message);
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(300)));
                    let _ = std::io::Write::write_all(&mut stream, &bytes);
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

pub fn fetch_bsread(addr: std::net::SocketAddr) -> Result<BsMessage, DsError> {
    let mut stream = std::net::TcpStream::connect(addr)
        .map_err(|err| DsError::new(format!("bsread connect: {err}")))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .ok();
    let mut buf = Vec::new();
    let _ = std::io::Read::read_to_end(&mut stream, &mut buf);
    decode_bsread(&buf)
}
