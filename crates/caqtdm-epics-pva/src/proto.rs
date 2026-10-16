//! PV Access frame helpers for the subset this crate speaks.
//!
//! Search messages follow the pvxs layout: magic `0xCA`, version 2, an 8-byte
//! header, size-prefixed strings, and a channel list. GET, PUT, and MONITOR
//! use a smaller scalar body (status byte, type code, IEEE-754 value) shared
//! by this client and this server. That body is not a pvxs `FieldDesc`
//! introspection, so a full pvxs peer will not accept it as NTScalar.

use std::fmt;

pub const MAGIC: u8 = 0xCA;
pub const VERSION: u8 = 2;
pub const FLAG_SERVER: u8 = 0x40;
pub const FLAG_BIG_ENDIAN: u8 = 0x80;
pub const FLAG_CONTROL: u8 = 0x01;
pub const HEADER_LEN: usize = 8;

pub const CMD_BEACON: u8 = 0;
pub const CMD_CONNECTION_VALIDATION: u8 = 1;
pub const CMD_ECHO: u8 = 2;
pub const CMD_SEARCH: u8 = 3;
pub const CMD_SEARCH_RESPONSE: u8 = 4;
pub const CMD_CREATE_CHANNEL: u8 = 7;
pub const CMD_GET: u8 = 10;
pub const CMD_PUT: u8 = 11;
pub const CMD_MONITOR: u8 = 13;

pub const STATUS_OK: u8 = 0xFF;
pub const TYPE_DOUBLE: u8 = 0x43;
pub const TYPE_STRING: u8 = 0x60;
pub const SUB_GET: u8 = 0x40;
pub const SUB_INIT: u8 = 0x08;
pub const SUB_DATA: u8 = 0x01;

pub const SEARCH_UNICAST: u8 = 0x80;
pub const SEARCH_MUST_REPLY: u8 = 0x01;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PvaError {
    pub message: String,
}

impl PvaError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PvaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PvaError {}

impl From<std::io::Error> for PvaError {
    fn from(err: std::io::Error) -> Self {
        Self::new(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub flags: u8,
    pub command: u8,
    pub payload_len: u32,
}

impl Header {
    pub fn new(server: bool, command: u8, payload_len: u32) -> Self {
        Self {
            flags: if server { FLAG_SERVER } else { 0 },
            command,
            payload_len,
        }
    }

    pub fn big_endian(self) -> bool {
        self.flags & FLAG_BIG_ENDIAN != 0
    }

    pub fn encode(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0] = MAGIC;
        out[1] = VERSION;
        out[2] = self.flags;
        out[3] = self.command;
        let len = if self.big_endian() {
            self.payload_len.to_be_bytes()
        } else {
            self.payload_len.to_le_bytes()
        };
        out[4..8].copy_from_slice(&len);
        out
    }
}

pub fn decode_header(buf: &[u8]) -> Result<Header, PvaError> {
    if buf.len() < HEADER_LEN || buf[0] != MAGIC {
        return Err(PvaError::new("not a PVA header"));
    }
    let flags = buf[2];
    let len_bytes: [u8; 4] = buf[4..8].try_into().unwrap_or([0; 4]);
    let payload_len = if flags & FLAG_BIG_ENDIAN != 0 {
        u32::from_be_bytes(len_bytes)
    } else {
        u32::from_le_bytes(len_bytes)
    };
    Ok(Header {
        flags,
        command: buf[3],
        payload_len,
    })
}

pub fn frame(server: bool, command: u8, payload: &[u8]) -> Vec<u8> {
    let header = Header::new(server, command, payload.len() as u32);
    let mut out = header.encode().to_vec();
    out.extend_from_slice(payload);
    out
}

pub fn encode_size(value: u32, big_endian: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if value < 254 {
        out.push(value as u8);
    } else {
        out.push(0xFE);
        let bytes = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

pub fn decode_size(buf: &[u8], big_endian: bool) -> Result<(Option<u32>, usize), PvaError> {
    let first = *buf
        .first()
        .ok_or_else(|| PvaError::new("missing PVA size"))?;
    match first {
        0xFF => Ok((None, 1)),
        0xFE => {
            if buf.len() < 5 {
                return Err(PvaError::new("truncated PVA size"));
            }
            let raw: [u8; 4] = buf[1..5].try_into().unwrap_or([0; 4]);
            let value = if big_endian {
                u32::from_be_bytes(raw)
            } else {
                u32::from_le_bytes(raw)
            };
            Ok((Some(value), 5))
        }
        other => Ok((Some(u32::from(other)), 1)),
    }
}

pub fn encode_string(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = encode_size(text.len() as u32, big_endian);
    out.extend_from_slice(text.as_bytes());
    out
}

pub fn decode_string(buf: &[u8], big_endian: bool) -> Result<(String, usize), PvaError> {
    let (len, used) = decode_size(buf, big_endian)?;
    let len = len.ok_or_else(|| PvaError::new("null PVA string"))? as usize;
    let end = used + len;
    if buf.len() < end {
        return Err(PvaError::new("truncated PVA string"));
    }
    let text = String::from_utf8_lossy(&buf[used..end]).into_owned();
    Ok((text, end))
}

fn put_u16(buf: &mut Vec<u8>, value: u16, big_endian: bool) {
    let bytes = if big_endian {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    };
    buf.extend_from_slice(&bytes);
}

fn put_u32(buf: &mut Vec<u8>, value: u32, big_endian: bool) {
    let bytes = if big_endian {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    };
    buf.extend_from_slice(&bytes);
}

fn read_u16(buf: &[u8], big_endian: bool) -> Result<(u16, usize), PvaError> {
    if buf.len() < 2 {
        return Err(PvaError::new("truncated u16"));
    }
    let raw = [buf[0], buf[1]];
    let value = if big_endian {
        u16::from_be_bytes(raw)
    } else {
        u16::from_le_bytes(raw)
    };
    Ok((value, 2))
}

fn read_u32(buf: &[u8], big_endian: bool) -> Result<(u32, usize), PvaError> {
    if buf.len() < 4 {
        return Err(PvaError::new("truncated u32"));
    }
    let raw: [u8; 4] = buf[..4].try_into().unwrap_or([0; 4]);
    let value = if big_endian {
        u32::from_be_bytes(raw)
    } else {
        u32::from_le_bytes(raw)
    };
    Ok((value, 4))
}

/// One SEARCH datagram. A zero response address is 16 raw zero bytes, which
/// is what pvxs writes for `IN6ADDR_ANY_INIT` (not an IPv4-mapped address).
pub fn search_frame(seq: u32, channels: &[(u32, &str)], port: u16, unicast: bool) -> Vec<u8> {
    let be = false;
    let mut payload = Vec::new();
    put_u32(&mut payload, seq, be);
    payload.push(if unicast { SEARCH_UNICAST } else { 0 });
    payload.extend_from_slice(&[0, 0, 0]);
    payload.extend_from_slice(&[0u8; 16]);
    put_u16(&mut payload, port, be);
    payload.extend_from_slice(&encode_size(1, be));
    payload.extend_from_slice(&encode_string("tcp", be));
    put_u16(&mut payload, channels.len() as u16, be);
    for (cid, name) in channels {
        put_u32(&mut payload, *cid, be);
        payload.extend_from_slice(&encode_string(name, be));
    }
    frame(false, CMD_SEARCH, &payload)
}

pub fn discover_frame(seq: u32, port: u16) -> Vec<u8> {
    let be = false;
    let mut payload = Vec::new();
    put_u32(&mut payload, seq, be);
    payload.push(SEARCH_MUST_REPLY);
    payload.extend_from_slice(&[0, 0, 0]);
    payload.extend_from_slice(&[0u8; 16]);
    put_u16(&mut payload, port, be);
    payload.extend_from_slice(&encode_size(0, be));
    put_u16(&mut payload, 0, be);
    frame(false, CMD_SEARCH, &payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub sequence: u32,
    pub flags: u8,
    pub port: u16,
    pub protocols: Vec<String>,
    pub channels: Vec<(u32, String)>,
}

pub fn decode_search_payload(payload: &[u8], big_endian: bool) -> Result<SearchRequest, PvaError> {
    let (sequence, mut at) = read_u32(payload, big_endian)?;
    let flags = *payload
        .get(at)
        .ok_or_else(|| PvaError::new("search flags"))?;
    at += 4; // flags + 3 reserved
    at += 16; // address
    let (port, n) = read_u16(payload.get(at..).unwrap_or(&[]), big_endian)?;
    at += n;
    let (count, n) = decode_size(payload.get(at..).unwrap_or(&[]), big_endian)?;
    at += n;
    let mut protocols = Vec::new();
    for _ in 0..count.unwrap_or(0) {
        let (text, n) = decode_string(payload.get(at..).unwrap_or(&[]), big_endian)?;
        at += n;
        protocols.push(text);
    }
    let (nchan, n) = read_u16(payload.get(at..).unwrap_or(&[]), big_endian)?;
    at += n;
    let mut channels = Vec::new();
    for _ in 0..nchan {
        let (cid, n) = read_u32(payload.get(at..).unwrap_or(&[]), big_endian)?;
        at += n;
        let (name, n) = decode_string(payload.get(at..).unwrap_or(&[]), big_endian)?;
        at += n;
        channels.push((cid, name));
    }
    let _ = at;
    Ok(SearchRequest {
        sequence,
        flags,
        port,
        protocols,
        channels,
    })
}

pub fn search_response(seq: u32, port: u16, found: bool, cids: &[u32]) -> Vec<u8> {
    let be = false;
    let mut payload = vec![0u8; 12];
    payload[0] = 0xC1;
    put_u32(&mut payload, seq, be);
    payload.extend_from_slice(&[0u8; 16]);
    put_u16(&mut payload, port, be);
    payload.extend_from_slice(&encode_string("tcp", be));
    payload.push(u8::from(found));
    put_u16(&mut payload, cids.len() as u16, be);
    for cid in cids {
        put_u32(&mut payload, *cid, be);
    }
    frame(true, CMD_SEARCH_RESPONSE, &payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResponse {
    pub sequence: u32,
    pub port: u16,
    pub protocol: String,
    pub found: bool,
    pub cids: Vec<u32>,
}

pub fn decode_search_response(payload: &[u8]) -> Result<SearchResponse, PvaError> {
    if payload.len() < 12 + 4 + 16 + 2 {
        return Err(PvaError::new("short search response"));
    }
    let (sequence, _) = read_u32(&payload[12..], false)?;
    let at = 12 + 4 + 16;
    let (port, n) = read_u16(&payload[at..], false)?;
    let mut at = at + n;
    let (protocol, n) = decode_string(&payload[at..], false)?;
    at += n;
    let found = payload.get(at).copied().unwrap_or(0) != 0;
    at += 1;
    let (count, n) = read_u16(&payload[at..], false)?;
    at += n;
    let mut cids = Vec::new();
    for _ in 0..count {
        let (cid, n) = read_u32(&payload[at..], false)?;
        at += n;
        cids.push(cid);
    }
    Ok(SearchResponse {
        sequence,
        port,
        protocol,
        found,
        cids,
    })
}

pub fn create_channel_request(cid: u32, name: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    put_u16(&mut payload, 1, false);
    put_u32(&mut payload, cid, false);
    payload.extend_from_slice(&encode_string(name, false));
    frame(false, CMD_CREATE_CHANNEL, &payload)
}

pub fn decode_create_request(payload: &[u8]) -> Result<(u32, String), PvaError> {
    let (_count, n) = read_u16(payload, false)?;
    let (cid, n2) = read_u32(&payload[n..], false)?;
    let (name, _) = decode_string(&payload[n + n2..], false)?;
    Ok((cid, name))
}

pub fn create_channel_response(cid: u32, sid: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    put_u32(&mut payload, cid, false);
    put_u32(&mut payload, sid, false);
    payload.push(STATUS_OK);
    frame(true, CMD_CREATE_CHANNEL, &payload)
}

pub fn op_request(command: u8, sid: u32, ioid: u32, subcmd: u8, extra: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    put_u32(&mut payload, sid, false);
    put_u32(&mut payload, ioid, false);
    payload.push(subcmd);
    payload.extend_from_slice(extra);
    frame(false, command, &payload)
}

pub fn scalar_body(value: f64) -> Vec<u8> {
    let mut body = vec![STATUS_OK, TYPE_DOUBLE];
    body.extend_from_slice(&value.to_le_bytes());
    body
}

pub fn decode_scalar_body(bytes: &[u8]) -> Result<f64, PvaError> {
    if bytes.len() < 10 || bytes[0] != STATUS_OK || bytes[1] != TYPE_DOUBLE {
        return Err(PvaError::new("expected a scalar double body"));
    }
    let raw: [u8; 8] = bytes[2..10].try_into().unwrap_or([0; 8]);
    Ok(f64::from_le_bytes(raw))
}

pub fn split_frame(buf: &[u8]) -> Result<Option<(Header, usize)>, PvaError> {
    if buf.len() < HEADER_LEN {
        return Ok(None);
    }
    let header = decode_header(buf)?;
    if header.flags & FLAG_CONTROL != 0 {
        return Ok(Some((header, HEADER_LEN)));
    }
    let total = HEADER_LEN + header.payload_len as usize;
    if buf.len() < total {
        return Ok(None);
    }
    Ok(Some((header, total)))
}
