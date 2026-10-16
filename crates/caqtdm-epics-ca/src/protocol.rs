//! Channel Access message headers.
//!
//! The layout matches `caProto.h`: a 16-byte big-endian header, and an 8-byte
//! extension when `payload_size` is `0xFFFF` (CA minor 4.9 and later). Payload
//! bytes are 8-byte aligned. Strings are null-terminated and then padded.

use std::fmt;

pub const CA_MAJOR: u16 = 4;
pub const CA_MINOR: u16 = 13;
pub const HEADER_SIZE: usize = 16;
pub const EXTENDED_MARK: u16 = 0xFFFF;

pub const CMD_VERSION: u16 = 0;
pub const CMD_EVENT_ADD: u16 = 1;
pub const CMD_EVENT_CANCEL: u16 = 2;
pub const CMD_WRITE: u16 = 4;
pub const CMD_SEARCH: u16 = 6;
pub const CMD_EVENTS_OFF: u16 = 8;
pub const CMD_EVENTS_ON: u16 = 9;
pub const CMD_ERROR: u16 = 11;
pub const CMD_CLEAR_CHANNEL: u16 = 12;
pub const CMD_NOT_FOUND: u16 = 14;
pub const CMD_READ_NOTIFY: u16 = 15;
pub const CMD_CREATE_CHAN: u16 = 18;
pub const CMD_WRITE_NOTIFY: u16 = 19;
pub const CMD_CLIENT_NAME: u16 = 20;
pub const CMD_HOST_NAME: u16 = 21;
pub const CMD_ACCESS_RIGHTS: u16 = 22;
pub const CMD_ECHO: u16 = 23;
pub const CMD_CREATE_CH_FAIL: u16 = 26;
pub const CMD_SERVER_DISCONN: u16 = 27;

pub const DONT_REPLY: u16 = 5;
pub const DO_REPLY: u16 = 10;
pub const ECA_NORMAL: u16 = 1;

pub const ACCESS_READ: u32 = 1;
pub const ACCESS_WRITE: u32 = 2;

pub const DBE_VALUE: u16 = 0x01;
pub const DBE_LOG: u16 = 0x02;
pub const DBE_ALARM: u16 = 0x04;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaError {
    pub message: String,
}

impl CaError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CaError {}

impl From<std::io::Error> for CaError {
    fn from(err: std::io::Error) -> Self {
        Self::new(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaHeader {
    pub command: u16,
    pub data_type: u16,
    pub count: u32,
    pub cid: u32,
    pub available: u32,
    pub payload_size: u32,
}

impl CaHeader {
    pub fn new(command: u16) -> Self {
        Self {
            command,
            data_type: 0,
            count: 0,
            cid: 0,
            available: 0,
            payload_size: 0,
        }
    }

    pub fn encode(self) -> Vec<u8> {
        let extended =
            self.payload_size > u32::from(u16::MAX) - 1 || self.count > u32::from(u16::MAX);
        let post = if extended {
            EXTENDED_MARK
        } else {
            self.payload_size as u16
        };
        let count = if extended { 0 } else { self.count as u16 };
        let mut buf = Vec::with_capacity(if extended { 24 } else { 16 });
        buf.extend_from_slice(&self.command.to_be_bytes());
        buf.extend_from_slice(&post.to_be_bytes());
        buf.extend_from_slice(&self.data_type.to_be_bytes());
        buf.extend_from_slice(&count.to_be_bytes());
        buf.extend_from_slice(&self.cid.to_be_bytes());
        buf.extend_from_slice(&self.available.to_be_bytes());
        if extended {
            buf.extend_from_slice(&self.payload_size.to_be_bytes());
            buf.extend_from_slice(&self.count.to_be_bytes());
        }
        buf
    }
}

/// Decode a header. Returns the header and the number of header bytes consumed.
pub fn decode_header(buf: &[u8]) -> Result<(CaHeader, usize), CaError> {
    if buf.len() < HEADER_SIZE {
        return Err(CaError::new("CA header shorter than 16 bytes"));
    }
    let command = u16::from_be_bytes([buf[0], buf[1]]);
    let post = u16::from_be_bytes([buf[2], buf[3]]);
    let data_type = u16::from_be_bytes([buf[4], buf[5]]);
    let count16 = u16::from_be_bytes([buf[6], buf[7]]);
    let cid = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let available = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
    if post == EXTENDED_MARK {
        if buf.len() < 24 {
            return Err(CaError::new("extended CA header truncated"));
        }
        let payload_size = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let count = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        return Ok((
            CaHeader {
                command,
                data_type,
                count,
                cid,
                available,
                payload_size,
            },
            24,
        ));
    }
    Ok((
        CaHeader {
            command,
            data_type,
            count: u32::from(count16),
            cid,
            available,
            payload_size: u32::from(post),
        },
        16,
    ))
}

pub fn align8(size: usize) -> usize {
    size.saturating_add(7) & !7
}

/// Null-terminated, 8-byte aligned payload.
pub fn pad_string(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes.resize(align8(bytes.len()), 0);
    bytes
}

pub fn unpad_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn encode_message(header: CaHeader, payload: &[u8]) -> Vec<u8> {
    let mut header = header;
    header.payload_size = payload.len() as u32;
    let mut out = header.encode();
    out.extend_from_slice(payload);
    out
}

/// Pull one message off the front of `buf` if a full frame is present.
pub fn take_message(buf: &mut Vec<u8>) -> Result<Option<(CaHeader, Vec<u8>)>, CaError> {
    if buf.len() < HEADER_SIZE {
        return Ok(None);
    }
    let (header, header_len) = match decode_header(buf) {
        Ok(v) => v,
        Err(err) if err.message.contains("truncated") => return Ok(None),
        Err(err) => return Err(err),
    };
    let total = header_len + header.payload_size as usize;
    if buf.len() < total {
        return Ok(None);
    }
    let payload = buf[header_len..total].to_vec();
    buf.drain(..total);
    Ok(Some((header, payload)))
}

pub fn version_message() -> Vec<u8> {
    let mut header = CaHeader::new(CMD_VERSION);
    header.count = u32::from(CA_MINOR);
    header.encode()
}

pub fn search_message(cid: u32, name: &str) -> Vec<u8> {
    let payload = pad_string(name);
    let mut header = CaHeader::new(CMD_SEARCH);
    header.data_type = DONT_REPLY;
    header.count = u32::from(CA_MINOR);
    header.cid = cid;
    header.available = cid;
    encode_message(header, &payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchReply {
    pub server_port: u16,
    pub server_ip: u32,
    pub cid: u32,
}

pub fn decode_search_reply(datagram: &[u8]) -> Result<SearchReply, CaError> {
    let mut buf = datagram.to_vec();
    let mut found = None;
    while let Some((header, _)) = take_message(&mut buf)? {
        if header.command == CMD_SEARCH {
            found = Some(SearchReply {
                server_port: header.data_type,
                server_ip: header.cid,
                cid: header.available,
            });
        }
    }
    found.ok_or_else(|| CaError::new("search reply missing"))
}

pub fn search_reply(port: u16, cid: u32) -> Vec<u8> {
    let mut header = CaHeader::new(CMD_SEARCH);
    header.data_type = port;
    header.count = u32::from(CA_MINOR);
    header.cid = 0xFFFF_FFFF;
    header.available = cid;
    header.encode()
}

/// 16-byte monitor subscription body: three f32 deadbands, a mask, and pad.
pub fn event_add_body(mask: u16) -> [u8; 16] {
    let mut body = [0u8; 16];
    body[12..14].copy_from_slice(&mask.to_be_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_numbers_match_ca_proto() {
        assert_eq!(CMD_VERSION, 0);
        assert_eq!(CMD_SERVER_DISCONN, 27);
        assert_eq!(CA_MAJOR, 4);
        assert_eq!(DO_REPLY, 10);
        assert_eq!(DBE_LOG, 2);
        assert_eq!(CMD_EVENT_CANCEL, 2);
        assert_eq!(CMD_WRITE, 4);
        assert_eq!(CMD_EVENTS_OFF, 8);
        assert_eq!(CMD_EVENTS_ON, 9);
        assert_eq!(CMD_ERROR, 11);
        assert_eq!(CMD_NOT_FOUND, 14);
    }
}
