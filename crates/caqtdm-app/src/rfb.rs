//! RFB 3.8 framebuffer server, plus the same bytes over a WebSocket.
//!
//! `-novnc` is the WebSocket transport a browser client uses. `-novnc_readonly`
//! accepts the handshake and the framebuffer request, and drops pointer and
//! key events. Security type is None. The only encoding is raw.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::net::{encode_ws_binary, encode_ws_binary_masked, read_ws_payload, websocket_accept};

const VERSION: &[u8] = b"RFB 003.008\n";

#[derive(Debug, Clone)]
pub struct RfbFrame {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfbEvent {
    pub pointer: bool,
    pub x: u16,
    pub y: u16,
    pub buttons: u8,
    pub key: u32,
    pub down: bool,
}

pub fn serve_rfb(
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    frame: RfbFrame,
) -> std::io::Result<(SocketAddr, Arc<Mutex<Vec<RfbEvent>>>)> {
    serve(addr, stop, frame, false)
}

pub fn serve_novnc(
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    frame: RfbFrame,
) -> std::io::Result<(SocketAddr, Arc<Mutex<Vec<RfbEvent>>>)> {
    serve(addr, stop, frame, true)
}

fn serve(
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    frame: RfbFrame,
    websocket: bool,
) -> std::io::Result<(SocketAddr, Arc<Mutex<Vec<RfbEvent>>>)> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::clone(&events);
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
                    let frame = frame.clone();
                    let events = Arc::clone(&shared);
                    if websocket {
                        let _ = session_ws(stream, &frame, &events);
                    } else {
                        let _ = session_tcp(stream, &frame, &events);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    Ok((bound, events))
}

fn session_tcp(
    stream: TcpStream,
    frame: &RfbFrame,
    events: &Mutex<Vec<RfbEvent>>,
) -> std::io::Result<()> {
    let mut io = TcpIo { stream };
    run_session(&mut io, frame, events)
}

fn session_ws(
    mut stream: TcpStream,
    frame: &RfbFrame,
    events: &Mutex<Vec<RfbEvent>>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::Error::other("websocket closed"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(at) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        if buf.len() > 8192 {
            return Err(std::io::Error::other("websocket header"));
        }
    };
    let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let key = header_value(&header, "sec-websocket-key").unwrap_or_default();
    let accept = websocket_accept(&key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream.write_all(response.as_bytes())?;
    let mut io = WsIo {
        stream,
        pending: buf[header_end..].to_vec(),
    };
    run_session(&mut io, frame, events)
}

trait Io {
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()>;
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
}

struct TcpIo {
    stream: TcpStream,
}

impl Io for TcpIo {
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.stream.read_exact(buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(buf)
    }
}

struct WsIo {
    stream: TcpStream,
    pending: Vec<u8>,
}

impl Io for WsIo {
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            if self.pending.is_empty() {
                self.pending = read_ws_payload(&mut self.stream)?;
            }
            let n = (buf.len() - filled).min(self.pending.len());
            buf[filled..filled + n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            filled += n;
        }
        Ok(())
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(&encode_ws_binary(buf))
    }
}

fn run_session(
    io: &mut impl Io,
    frame: &RfbFrame,
    events: &Mutex<Vec<RfbEvent>>,
) -> std::io::Result<()> {
    io.write_all(VERSION)?;
    let mut version = [0u8; 12];
    io.read_exact(&mut version)?;
    if &version[..3] != b"RFB" {
        return Err(std::io::Error::other("rfb version"));
    }
    io.write_all(&[1, 1])?;
    let mut chosen = [0u8; 1];
    io.read_exact(&mut chosen)?;
    if chosen[0] != 1 {
        io.write_all(&1u32.to_be_bytes())?;
        return Err(std::io::Error::other("rfb security"));
    }
    io.write_all(&0u32.to_be_bytes())?;
    let mut shared = [0u8; 1];
    io.read_exact(&mut shared)?;
    io.write_all(&server_init(frame))?;
    loop {
        let mut kind = [0u8; 1];
        if io.read_exact(&mut kind).is_err() {
            break;
        }
        match kind[0] {
            0 => {
                let mut rest = [0u8; 19];
                io.read_exact(&mut rest)?;
            }
            2 => {
                let mut head = [0u8; 3];
                io.read_exact(&mut head)?;
                let count = u16::from_be_bytes([head[1], head[2]]) as usize;
                let mut enc = vec![0u8; count * 4];
                io.read_exact(&mut enc)?;
            }
            3 => {
                let mut rest = [0u8; 9];
                io.read_exact(&mut rest)?;
                let x = u16::from_be_bytes([rest[1], rest[2]]);
                let y = u16::from_be_bytes([rest[3], rest[4]]);
                let w = u16::from_be_bytes([rest[5], rest[6]]);
                let h = u16::from_be_bytes([rest[7], rest[8]]);
                io.write_all(&framebuffer_update(frame, x, y, w, h))?;
            }
            4 => {
                let mut rest = [0u8; 7];
                io.read_exact(&mut rest)?;
                if !frame.readonly {
                    let key = u32::from_be_bytes([rest[3], rest[4], rest[5], rest[6]]);
                    push(
                        events,
                        RfbEvent {
                            pointer: false,
                            x: 0,
                            y: 0,
                            buttons: 0,
                            key,
                            down: rest[0] != 0,
                        },
                    );
                }
            }
            5 => {
                let mut rest = [0u8; 5];
                io.read_exact(&mut rest)?;
                if !frame.readonly {
                    push(
                        events,
                        RfbEvent {
                            pointer: true,
                            x: u16::from_be_bytes([rest[1], rest[2]]),
                            y: u16::from_be_bytes([rest[3], rest[4]]),
                            buttons: rest[0],
                            key: 0,
                            down: false,
                        },
                    );
                }
            }
            6 => {
                let mut head = [0u8; 7];
                io.read_exact(&mut head)?;
                let len = u32::from_be_bytes([head[3], head[4], head[5], head[6]]) as usize;
                let mut text = vec![0u8; len];
                io.read_exact(&mut text)?;
            }
            _ => break,
        }
    }
    Ok(())
}

fn push(events: &Mutex<Vec<RfbEvent>>, event: RfbEvent) {
    if let Ok(mut guard) = events.lock() {
        guard.push(event);
    }
}

fn server_init(frame: &RfbFrame) -> Vec<u8> {
    let name = b"caQtDM";
    let mut out = Vec::new();
    out.extend_from_slice(&frame.width.to_be_bytes());
    out.extend_from_slice(&frame.height.to_be_bytes());
    out.push(32);
    out.push(24);
    out.push(0);
    out.push(1);
    out.extend_from_slice(&255u16.to_be_bytes());
    out.extend_from_slice(&255u16.to_be_bytes());
    out.extend_from_slice(&255u16.to_be_bytes());
    out.push(16);
    out.push(8);
    out.push(0);
    out.extend_from_slice(&[0, 0, 0]);
    out.extend_from_slice(&(name.len() as u32).to_be_bytes());
    out.extend_from_slice(name);
    out
}

fn framebuffer_update(frame: &RfbFrame, x: u16, y: u16, w: u16, h: u16) -> Vec<u8> {
    let x = x.min(frame.width.saturating_sub(1));
    let y = y.min(frame.height.saturating_sub(1));
    let w = w.min(frame.width.saturating_sub(x)).max(1);
    let h = h.min(frame.height.saturating_sub(y)).max(1);
    let mut out = vec![0, 0];
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&y.to_be_bytes());
    out.extend_from_slice(&w.to_be_bytes());
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&0i32.to_be_bytes());
    for row in y..y + h {
        for col in x..x + w {
            let i = ((row as usize * frame.width as usize) + col as usize) * 4;
            let (r, g, b) = if i + 2 < frame.rgba.len() {
                (frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2])
            } else {
                (0, 0, 0)
            };
            out.extend_from_slice(&[b, g, r, 0]);
        }
    }
    out
}

fn header_value(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(name) {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn connect_retry(addr: SocketAddr) -> std::io::Result<TcpStream> {
    let mut last = std::io::Error::other("connect");
    for _ in 0..40 {
        match TcpStream::connect(addr) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_millis(800)))?;
                stream.set_write_timeout(Some(Duration::from_millis(800)))?;
                return Ok(stream);
            }
            Err(err) => {
                last = err;
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
    Err(last)
}

/// Handshake and return the framebuffer as RGBA.
pub fn rfb_framebuffer(addr: SocketAddr) -> std::io::Result<(u16, u16, Vec<u8>)> {
    let stream = connect_retry(addr)?;
    let mut io = TcpIo { stream };
    let (w, h) = handshake(&mut io)?;
    request_full(&mut io, w, h)?;
    let rgba = read_update(&mut io, w, h)?;
    Ok((w, h, rgba))
}

/// Handshake, then a button-1 press. Readonly servers accept it and drop it.
pub fn rfb_pointer(addr: SocketAddr, x: u16, y: u16) -> std::io::Result<()> {
    let stream = connect_retry(addr)?;
    let mut io = TcpIo { stream };
    let (w, h) = handshake(&mut io)?;
    request_full(&mut io, w, h)?;
    let _ = read_update(&mut io, w, h)?;
    let mut msg = vec![5, 1];
    msg.extend_from_slice(&x.to_be_bytes());
    msg.extend_from_slice(&y.to_be_bytes());
    io.write_all(&msg)
}

/// WebSocket upgrade, then the RFB version string from the first binary frame.
pub fn novnc_version(addr: SocketAddr) -> std::io::Result<String> {
    let mut stream = connect_retry(addr)?;
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    stream.write_all(request.as_bytes())?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::Error::other("novnc closed"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(at) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let header = String::from_utf8_lossy(&buf[..header_end]);
    if !header.contains("101") {
        return Err(std::io::Error::other("novnc upgrade failed"));
    }
    let mut pending = buf[header_end..].to_vec();
    let mut version = Vec::new();
    while version.len() < 12 {
        let payload = pop_ws_frame(&mut stream, &mut pending)?;
        version.extend_from_slice(&payload);
    }
    stream.write_all(&encode_ws_binary_masked(VERSION))?;
    version.truncate(12);
    String::from_utf8(version).map_err(|_| std::io::Error::other("rfb version"))
}

fn pop_ws_frame(stream: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<Vec<u8>> {
    loop {
        if buf.len() >= 2 {
            let masked = buf[1] & 0x80 != 0;
            let mut len = (buf[1] & 0x7f) as usize;
            let mut header = 2;
            if len == 126 {
                if buf.len() < 4 {
                    read_more(stream, buf)?;
                    continue;
                }
                len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
                header = 4;
            } else if len == 127 {
                if buf.len() < 10 {
                    read_more(stream, buf)?;
                    continue;
                }
                let bytes: [u8; 8] = buf[2..10].try_into().unwrap_or([0; 8]);
                len = u64::from_be_bytes(bytes) as usize;
                header = 10;
            }
            let mask_len = if masked { 4 } else { 0 };
            let total = header + mask_len + len;
            if buf.len() < total {
                read_more(stream, buf)?;
                continue;
            }
            let mut payload = buf[header + mask_len..total].to_vec();
            if masked {
                let mask = buf[header..header + 4].to_vec();
                for (index, byte) in payload.iter_mut().enumerate() {
                    *byte ^= mask[index % 4];
                }
            }
            buf.drain(..total);
            return Ok(payload);
        }
        read_more(stream, buf)?;
    }
}

fn read_more(stream: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<()> {
    let mut tmp = [0u8; 2048];
    let n = stream.read(&mut tmp)?;
    if n == 0 {
        return Err(std::io::Error::other("novnc closed"));
    }
    buf.extend_from_slice(&tmp[..n]);
    Ok(())
}

fn handshake(io: &mut impl Io) -> std::io::Result<(u16, u16)> {
    let mut version = [0u8; 12];
    io.read_exact(&mut version)?;
    if version.as_slice() != VERSION {
        return Err(std::io::Error::other("rfb version"));
    }
    io.write_all(VERSION)?;
    let mut security = [0u8; 2];
    io.read_exact(&mut security)?;
    if security != [1, 1] {
        return Err(std::io::Error::other("rfb security types"));
    }
    io.write_all(&[1])?;
    let mut result = [0u8; 4];
    io.read_exact(&mut result)?;
    if result != [0, 0, 0, 0] {
        return Err(std::io::Error::other("rfb security result"));
    }
    io.write_all(&[1])?;
    let mut header = [0u8; 24];
    io.read_exact(&mut header)?;
    let width = u16::from_be_bytes([header[0], header[1]]);
    let height = u16::from_be_bytes([header[2], header[3]]);
    let name_len = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as usize;
    let mut name = vec![0u8; name_len];
    io.read_exact(&mut name)?;
    if &name != b"caQtDM" {
        return Err(std::io::Error::other("rfb name"));
    }
    Ok((width, height))
}

fn request_full(io: &mut impl Io, w: u16, h: u16) -> std::io::Result<()> {
    let mut msg = vec![3, 0];
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.extend_from_slice(&0u16.to_be_bytes());
    msg.extend_from_slice(&w.to_be_bytes());
    msg.extend_from_slice(&h.to_be_bytes());
    io.write_all(&msg)
}

fn read_update(io: &mut impl Io, w: u16, h: u16) -> std::io::Result<Vec<u8>> {
    let mut head = [0u8; 4];
    io.read_exact(&mut head)?;
    if head[0] != 0 {
        return Err(std::io::Error::other("rfb update"));
    }
    let nrects = u16::from_be_bytes([head[2], head[3]]);
    if nrects != 1 {
        return Err(std::io::Error::other("rfb rects"));
    }
    let mut rect = [0u8; 12];
    io.read_exact(&mut rect)?;
    let pixels = (w as usize) * (h as usize);
    let mut raw = vec![0u8; pixels * 4];
    io.read_exact(&mut raw)?;
    let mut rgba = Vec::with_capacity(pixels * 4);
    for px in raw.chunks(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    Ok(rgba)
}
