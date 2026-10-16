//! OPC UA binary TCP helpers: Hello/Acknowledge and a Read service body.
//!
//! SecurityPolicy#None only. The secure-channel token and chunk padding are
//! not implemented. NodeId, DataValue, and the Read service ids follow the
//! binary mapping in OPC UA part 6.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::error::DsError;

pub const READ_REQUEST: u32 = 631;
pub const READ_RESPONSE: u32 = 634;
pub const VARIANT_DOUBLE: u8 = 11;
pub const STATUS_GOOD: u32 = 0;
pub const ATTRIBUTE_VALUE: u32 = 13;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeId {
    Numeric { ns: u16, id: u32 },
    String { ns: u16, text: String },
}

pub fn encode_node_id(node: &NodeId) -> Vec<u8> {
    match node {
        NodeId::Numeric { ns, id } if *ns < 256 && *id < 256 => {
            vec![0x00, *ns as u8, *id as u8]
        }
        NodeId::Numeric { ns, id } if *ns < 256 && *id < 65536 => {
            let mut out = vec![0x01, *ns as u8];
            out.extend_from_slice(&(*id as u16).to_le_bytes());
            out
        }
        NodeId::Numeric { ns, id } => {
            let mut out = vec![0x02];
            out.extend_from_slice(&ns.to_le_bytes());
            out.extend_from_slice(&id.to_le_bytes());
            out
        }
        NodeId::String { ns, text } => {
            let mut out = vec![0x03];
            out.extend_from_slice(&ns.to_le_bytes());
            out.extend_from_slice(&(text.len() as i32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
            out
        }
    }
}

pub fn decode_node_id(bytes: &[u8]) -> Result<(NodeId, usize), DsError> {
    let kind = *bytes.first().ok_or_else(|| DsError::new("empty node id"))?;
    match kind {
        0x00 => Ok((
            NodeId::Numeric {
                ns: u16::from(bytes.get(1).copied().unwrap_or(0)),
                id: u32::from(bytes.get(2).copied().unwrap_or(0)),
            },
            3,
        )),
        0x01 => {
            let id = u16::from_le_bytes([
                bytes.get(2).copied().unwrap_or(0),
                bytes.get(3).copied().unwrap_or(0),
            ]);
            Ok((
                NodeId::Numeric {
                    ns: u16::from(bytes.get(1).copied().unwrap_or(0)),
                    id: u32::from(id),
                },
                4,
            ))
        }
        0x02 => {
            if bytes.len() < 7 {
                return Err(DsError::new("numeric node id truncated"));
            }
            Ok((
                NodeId::Numeric {
                    ns: u16::from_le_bytes([bytes[1], bytes[2]]),
                    id: u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]),
                },
                7,
            ))
        }
        0x03 => {
            if bytes.len() < 7 {
                return Err(DsError::new("string node id truncated"));
            }
            let ns = u16::from_le_bytes([bytes[1], bytes[2]]);
            let len = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
            if len < 0 || bytes.len() < 7 + len as usize {
                return Err(DsError::new("string node id length"));
            }
            let text = String::from_utf8_lossy(&bytes[7..7 + len as usize]).into_owned();
            Ok((NodeId::String { ns, text }, 7 + len as usize))
        }
        other => Err(DsError::new(format!(
            "unsupported node id encoding {other}"
        ))),
    }
}

fn tcp_header(kind: &[u8; 3], chunk: u8, body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(kind);
    out.push(chunk);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(body);
    out
}

pub fn encode_hello(endpoint: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0u32.to_le_bytes());
    for value in [65_536u32, 65_536, 0, 0] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body.extend_from_slice(&(endpoint.len() as i32).to_le_bytes());
    body.extend_from_slice(endpoint.as_bytes());
    tcp_header(b"HEL", b'F', &body)
}

pub fn encode_ack() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0u32.to_le_bytes());
    for value in [65_536u32, 65_536, 0, 0] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    tcp_header(b"ACK", b'F', &body)
}

pub fn decode_hello(bytes: &[u8]) -> Result<String, DsError> {
    if bytes.len() < 8 + 20 + 4 || &bytes[0..3] != b"HEL" {
        return Err(DsError::new("not an OPC UA Hello"));
    }
    let len = i32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
    if len < 0 || bytes.len() < 32 + len as usize {
        return Err(DsError::new("Hello endpoint truncated"));
    }
    Ok(String::from_utf8_lossy(&bytes[32..32 + len as usize]).into_owned())
}

pub fn decode_ack(bytes: &[u8]) -> Result<(), DsError> {
    if bytes.len() < 8 || &bytes[0..3] != b"ACK" {
        Err(DsError::new("not an OPC UA Acknowledge"))
    } else {
        Ok(())
    }
}

pub fn encode_read_request(node: &NodeId, request_handle: u32) -> Vec<u8> {
    let mut body = encode_node_id(&NodeId::Numeric {
        ns: 0,
        id: READ_REQUEST,
    });
    body.extend_from_slice(&request_handle.to_le_bytes());
    body.extend_from_slice(&1i32.to_le_bytes());
    body.extend_from_slice(&encode_node_id(node));
    body.extend_from_slice(&ATTRIBUTE_VALUE.to_le_bytes());
    tcp_header(b"MSG", b'F', &body)
}

pub fn encode_read_response(value: f64) -> Vec<u8> {
    let mut body = encode_node_id(&NodeId::Numeric {
        ns: 0,
        id: READ_RESPONSE,
    });
    body.extend_from_slice(&STATUS_GOOD.to_le_bytes());
    body.extend_from_slice(&1i32.to_le_bytes());
    body.push(0x03);
    body.push(VARIANT_DOUBLE);
    body.extend_from_slice(&value.to_le_bytes());
    body.extend_from_slice(&STATUS_GOOD.to_le_bytes());
    tcp_header(b"MSG", b'F', &body)
}

pub fn decode_read_response(bytes: &[u8]) -> Result<f64, DsError> {
    if bytes.len() < 8 || &bytes[0..3] != b"MSG" {
        return Err(DsError::new("not an OPC UA message"));
    }
    let (service, n) = decode_node_id(&bytes[8..])?;
    match service {
        NodeId::Numeric {
            id: READ_RESPONSE, ..
        } => {}
        other => return Err(DsError::new(format!("unexpected service {other:?}"))),
    }
    let rest = &bytes[8 + n..];
    if rest.len() < 4 + 4 + 1 + 1 + 8 + 4 {
        return Err(DsError::new("ReadResponse truncated"));
    }
    let status = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
    if status != STATUS_GOOD {
        return Err(DsError::new(format!("ReadResponse status {status}")));
    }
    if rest[8] & 0x01 == 0 || rest[9] != VARIANT_DOUBLE {
        return Err(DsError::new("ReadResponse is not a double DataValue"));
    }
    let raw = [
        rest[10], rest[11], rest[12], rest[13], rest[14], rest[15], rest[16], rest[17],
    ];
    let value_status = u32::from_le_bytes([rest[18], rest[19], rest[20], rest[21]]);
    if value_status != STATUS_GOOD {
        return Err(DsError::new(format!("DataValue status {value_status}")));
    }
    Ok(f64::from_le_bytes(raw))
}

pub fn decode_read_request(bytes: &[u8]) -> Result<(NodeId, u32), DsError> {
    if bytes.len() < 8 || &bytes[0..3] != b"MSG" {
        return Err(DsError::new("not an OPC UA message"));
    }
    let (service, n) = decode_node_id(&bytes[8..])?;
    match service {
        NodeId::Numeric {
            id: READ_REQUEST, ..
        } => {}
        other => return Err(DsError::new(format!("unexpected service {other:?}"))),
    }
    let rest = &bytes[8 + n..];
    if rest.len() < 8 {
        return Err(DsError::new("ReadRequest truncated"));
    }
    let handle = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
    let (node, _) = decode_node_id(&rest[8..])?;
    Ok((node, handle))
}

/// Accept Hello, answer Acknowledge, then answer one Read with `value`.
pub fn serve_read(value: f64, stop: Arc<AtomicBool>) -> std::io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = serve_one(stream, value);
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    Ok(addr)
}

fn serve_one(mut stream: TcpStream, value: f64) -> Result<(), DsError> {
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .ok();
    let hello = read_frame(&mut stream)?;
    decode_hello(&hello)?;
    stream
        .write_all(&encode_ack())
        .map_err(|err| DsError::new(err.to_string()))?;
    let request = read_frame(&mut stream)?;
    decode_read_request(&request)?;
    stream
        .write_all(&encode_read_response(value))
        .map_err(|err| DsError::new(err.to_string()))?;
    Ok(())
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, DsError> {
    let mut header = [0u8; 8];
    stream
        .read_exact(&mut header)
        .map_err(|err| DsError::new(err.to_string()))?;
    let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if !(8..=1_000_000).contains(&size) {
        return Err(DsError::new("opcua frame size"));
    }
    let mut body = vec![0u8; size - 8];
    stream
        .read_exact(&mut body)
        .map_err(|err| DsError::new(err.to_string()))?;
    let mut frame = header.to_vec();
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn read_node(addr: std::net::SocketAddr, node: &NodeId) -> Result<f64, DsError> {
    let mut stream =
        TcpStream::connect(addr).map_err(|err| DsError::new(format!("opcua connect: {err}")))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();
    stream
        .write_all(&encode_hello("opc.tcp://127.0.0.1"))
        .map_err(|err| DsError::new(err.to_string()))?;
    let ack = read_frame(&mut stream)?;
    decode_ack(&ack)?;
    stream
        .write_all(&encode_read_request(node, 1))
        .map_err(|err| DsError::new(err.to_string()))?;
    let response = read_frame(&mut stream)?;
    decode_read_response(&response)
}
