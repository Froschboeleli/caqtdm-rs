//! A small ZMTP 3.0 peer for bsread.
//!
//! The greeting is the 64-byte NULL-security signature from ZeroMQ RFC 23.
//! Command frames carry READY, then multipart messages use the MORE flag.
//! This is enough to move a three-part bsread pulse. It is not a full libzmq.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::bsread::{decode_bsread_parts, BsMessage};
use crate::error::DsError;

const MORE: u8 = 0x01;
const LONG: u8 = 0x02;
const COMMAND: u8 = 0x04;

pub fn greeting(as_server: bool) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    bytes[..8].fill(0xFF);
    bytes[8] = 3;
    bytes[9] = 0;
    let mechanism = b"NULL";
    bytes[10..10 + mechanism.len()].copy_from_slice(mechanism);
    bytes[30] = u8::from(as_server);
    bytes
}

pub fn parse_greeting(bytes: &[u8]) -> Result<bool, DsError> {
    if bytes.len() < 64 || bytes[..8] != [0xFF; 8] || bytes[8] != 3 {
        return Err(DsError::new("ZMTP greeting is not version 3"));
    }
    if &bytes[10..14] != b"NULL" {
        return Err(DsError::new("ZMTP mechanism is not NULL"));
    }
    Ok(bytes[30] != 0)
}

fn ready(socket_type: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(5);
    body.extend_from_slice(b"READY");
    let name = b"Socket-Type";
    body.push(name.len() as u8);
    body.extend_from_slice(name);
    body.extend_from_slice(&(socket_type.len() as u32).to_be_bytes());
    body.extend_from_slice(socket_type.as_bytes());
    frame(&body, false, true)
}

fn frame(body: &[u8], more: bool, command: bool) -> Vec<u8> {
    let mut flags = 0u8;
    if more {
        flags |= MORE;
    }
    if body.len() > 255 {
        flags |= LONG;
    }
    if command {
        flags |= COMMAND;
    }
    let mut out = vec![flags];
    if body.len() > 255 {
        out.extend_from_slice(&(body.len() as u64).to_be_bytes());
    } else {
        out.push(body.len() as u8);
    }
    out.extend_from_slice(body);
    out
}

fn read_frame(stream: &mut TcpStream) -> Result<(bool, bool, Vec<u8>), DsError> {
    let mut flags = [0u8; 1];
    stream
        .read_exact(&mut flags)
        .map_err(|err| DsError::new(format!("zmtp flags: {err}")))?;
    let long = flags[0] & LONG != 0;
    let size = if long {
        let mut raw = [0u8; 8];
        stream
            .read_exact(&mut raw)
            .map_err(|err| DsError::new(format!("zmtp size: {err}")))?;
        u64::from_be_bytes(raw) as usize
    } else {
        let mut raw = [0u8; 1];
        stream
            .read_exact(&mut raw)
            .map_err(|err| DsError::new(format!("zmtp size: {err}")))?;
        raw[0] as usize
    };
    if size > 1_000_000 {
        return Err(DsError::new("zmtp frame is too large"));
    }
    let mut body = vec![0u8; size];
    stream
        .read_exact(&mut body)
        .map_err(|err| DsError::new(format!("zmtp body: {err}")))?;
    Ok((flags[0] & MORE != 0, flags[0] & COMMAND != 0, body))
}

fn read_multipart(stream: &mut TcpStream) -> Result<Vec<Vec<u8>>, DsError> {
    let mut parts = Vec::new();
    loop {
        let (more, command, body) = read_frame(stream)?;
        if command {
            continue;
        }
        parts.push(body);
        if !more {
            break;
        }
    }
    Ok(parts)
}

fn handshake(stream: &mut TcpStream, as_server: bool, socket_type: &str) -> Result<(), DsError> {
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok();
    stream.set_nodelay(true).ok();
    stream
        .write_all(&greeting(as_server))
        .map_err(|err| DsError::new(err.to_string()))?;
    let mut peer = [0u8; 64];
    stream
        .read_exact(&mut peer)
        .map_err(|err| DsError::new(format!("zmtp greeting: {err}")))?;
    parse_greeting(&peer)?;
    stream
        .write_all(&ready(socket_type))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, body) = read_frame(stream)?;
    if !command || !body.windows(5).any(|window| window == b"READY") {
        return Err(DsError::new("peer did not send ZMTP READY"));
    }
    Ok(())
}

/// Accept one client, complete the NULL handshake, and return the multipart body.
pub fn serve_zmtp(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<(
    std::net::SocketAddr,
    std::sync::mpsc::Receiver<Vec<Vec<u8>>>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if handshake(&mut stream, true, "PULL").is_ok() {
                        if let Ok(parts) = read_multipart(&mut stream) {
                            let _ = tx.send(parts);
                        }
                    }
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

pub fn send_bsread(addr: std::net::SocketAddr, message: &BsMessage) -> Result<(), DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|err| DsError::new(err.to_string()))?;
    handshake(&mut stream, false, "PUSH")?;
    let parts = crate::bsread::bsread_parts(message);
    for (index, part) in parts.iter().enumerate() {
        let more = index + 1 != parts.len();
        stream
            .write_all(&frame(part, more, false))
            .map_err(|err| DsError::new(err.to_string()))?;
    }
    Ok(())
}

pub fn recv_bsread(rx: &std::sync::mpsc::Receiver<Vec<Vec<u8>>>) -> Result<BsMessage, DsError> {
    let parts = rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| DsError::new("zmtp bsread timed out"))?;
    decode_bsread_parts(&parts)
}
