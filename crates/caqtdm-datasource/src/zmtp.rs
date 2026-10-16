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
    greeting_for(as_server, "NULL")
}

pub fn greeting_for(as_server: bool, mechanism: &str) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    bytes[..8].fill(0xFF);
    bytes[8] = 3;
    bytes[9] = 0;
    let raw = mechanism.as_bytes();
    let n = raw.len().min(20);
    bytes[10..10 + n].copy_from_slice(&raw[..n]);
    bytes[30] = u8::from(as_server);
    bytes
}

pub fn parse_greeting(bytes: &[u8]) -> Result<bool, DsError> {
    let _ = mechanism_of(bytes)?;
    Ok(bytes[30] != 0)
}

pub fn mechanism_of(bytes: &[u8]) -> Result<String, DsError> {
    if bytes.len() < 64 || bytes[..8] != [0xFF; 8] || bytes[8] != 3 {
        return Err(DsError::new("ZMTP greeting is not version 3"));
    }
    let end = bytes[10..30].iter().position(|b| *b == 0).unwrap_or(20);
    let name = std::str::from_utf8(&bytes[10..10 + end])
        .map_err(|_| DsError::new("ZMTP mechanism is not utf-8"))?;
    if name != "NULL" && name != "PLAIN" && name != "CURVE" {
        return Err(DsError::new(format!(
            "ZMTP mechanism {name} is not supported"
        )));
    }
    Ok(name.to_string())
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

fn exchange_greeting(
    stream: &mut TcpStream,
    as_server: bool,
    mechanism: &str,
) -> Result<(), DsError> {
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .ok();
    stream.set_nodelay(true).ok();
    stream
        .write_all(&greeting_for(as_server, mechanism))
        .map_err(|err| DsError::new(err.to_string()))?;
    let mut peer = [0u8; 64];
    stream
        .read_exact(&mut peer)
        .map_err(|err| DsError::new(format!("zmtp greeting: {err}")))?;
    let got = mechanism_of(&peer)?;
    if got != mechanism {
        return Err(DsError::new(format!("peer mechanism {got}")));
    }
    Ok(())
}

fn command_body(name: &str, rest: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(name.len() as u8);
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(rest);
    body
}

pub fn serve_plain(
    user: String,
    password: String,
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
                    if plain_server(&mut stream, &user, &password).is_ok() {
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

fn plain_server(stream: &mut TcpStream, user: &str, password: &str) -> Result<(), DsError> {
    exchange_greeting(stream, true, "PLAIN")?;
    let (_more, command, body) = read_frame(stream)?;
    if !command {
        return Err(DsError::new("PLAIN HELLO missing"));
    }
    let (got_user, got_pass) = parse_plain_hello(&body)?;
    if got_user != user || got_pass != password {
        return Err(DsError::new("PLAIN credentials rejected"));
    }
    let welcome = frame(&command_body("WELCOME", &[]), false, true);
    stream
        .write_all(&welcome)
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, body) = read_frame(stream)?;
    if !command || !body.windows(8).any(|window| window == b"INITIATE") {
        return Err(DsError::new("PLAIN INITIATE missing"));
    }
    stream
        .write_all(&ready("PULL"))
        .map_err(|err| DsError::new(err.to_string()))?;
    Ok(())
}

fn parse_plain_hello(body: &[u8]) -> Result<(String, String), DsError> {
    if body.len() < 6 || &body[1..6] != b"HELLO" {
        return Err(DsError::new("PLAIN HELLO"));
    }
    let mut at = 6;
    let (user, next) = take_short_string(body, at)?;
    at = next;
    let (password, _) = take_short_string(body, at)?;
    Ok((user, password))
}

fn take_short_string(body: &[u8], at: usize) -> Result<(String, usize), DsError> {
    let len = *body.get(at).ok_or_else(|| DsError::new("PLAIN string"))? as usize;
    let start = at + 1;
    let end = start + len;
    if body.len() < end {
        return Err(DsError::new("PLAIN string truncated"));
    }
    let text = std::str::from_utf8(&body[start..end])
        .map_err(|_| DsError::new("PLAIN string utf-8"))?
        .to_string();
    Ok((text, end))
}

pub fn send_bsread_plain(
    addr: std::net::SocketAddr,
    user: &str,
    password: &str,
    message: &BsMessage,
) -> Result<(), DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|err| DsError::new(err.to_string()))?;
    exchange_greeting(&mut stream, false, "PLAIN")?;
    let mut hello = command_body("HELLO", &[]);
    hello.push(user.len() as u8);
    hello.extend_from_slice(user.as_bytes());
    hello.push(password.len() as u8);
    hello.extend_from_slice(password.as_bytes());
    stream
        .write_all(&frame(&hello, false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, body) = read_frame(&mut stream)?;
    if !command || !body.windows(7).any(|window| window == b"WELCOME") {
        return Err(DsError::new("PLAIN WELCOME missing"));
    }
    let initiate = command_body("INITIATE", &metadata("PUSH"));
    stream
        .write_all(&frame(&initiate, false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, body) = read_frame(&mut stream)?;
    if !command || !body.windows(5).any(|window| window == b"READY") {
        return Err(DsError::new("PLAIN READY missing"));
    }
    write_parts(&mut stream, message)
}

fn metadata(socket_type: &str) -> Vec<u8> {
    let mut body = Vec::new();
    let name = b"Socket-Type";
    body.push(name.len() as u8);
    body.extend_from_slice(name);
    body.extend_from_slice(&(socket_type.len() as u32).to_be_bytes());
    body.extend_from_slice(socket_type.as_bytes());
    body
}

fn write_parts(stream: &mut TcpStream, message: &BsMessage) -> Result<(), DsError> {
    let parts = crate::bsread::bsread_parts(message);
    for (index, part) in parts.iter().enumerate() {
        let more = index + 1 != parts.len();
        stream
            .write_all(&frame(part, more, false))
            .map_err(|err| DsError::new(err.to_string()))?;
    }
    Ok(())
}

pub struct CurveIdentity {
    pub secret: crypto_box::SecretKey,
    pub public: crypto_box::PublicKey,
}

impl CurveIdentity {
    pub fn generate() -> Self {
        let secret = crypto_box::SecretKey::generate(&mut rand::rngs::OsRng);
        let public = secret.public_key();
        Self { secret, public }
    }
}

pub fn serve_curve(
    server: CurveIdentity,
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
                    if let Ok(mut session) = curve_server(&mut stream, &server) {
                        if let Ok(parts) = read_curve_messages(&mut stream, &mut session, 3) {
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

struct CurveSession {
    peer_transient: crypto_box::PublicKey,
    local_transient: CurveIdentity,
    peer_short: u64,
    local_short: u64,
}

pub fn send_bsread_curve(
    addr: std::net::SocketAddr,
    server_public: &crypto_box::PublicKey,
    client_long: &CurveIdentity,
    message: &BsMessage,
) -> Result<(), DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|err| DsError::new(err.to_string()))?;
    let mut session = curve_client(&mut stream, server_public, client_long)?;
    let parts = crate::bsread::bsread_parts(message);
    for part in &parts {
        write_curve_message(&mut stream, &mut session, part)?;
    }
    Ok(())
}

fn curve_client(
    stream: &mut TcpStream,
    server_public: &crypto_box::PublicKey,
    client_long: &CurveIdentity,
) -> Result<CurveSession, DsError> {
    exchange_greeting(stream, false, "CURVE")?;
    let transient = CurveIdentity::generate();
    let short = 1u64;
    let hello_box = nacl_box(
        &transient.secret,
        server_public,
        &nonce(b"CurveZMQHELLO---", &short.to_be_bytes()),
        &[0u8; 64],
    )?;
    let mut hello = Vec::new();
    hello.extend_from_slice(&[1, 0]);
    hello.extend(std::iter::repeat_n(0u8, 72));
    hello.extend_from_slice(transient.public.as_bytes());
    hello.extend_from_slice(&short.to_be_bytes());
    hello.extend_from_slice(&hello_box);
    let body = command_body("HELLO", &hello);
    if body.len() != 200 {
        return Err(DsError::new("CURVE HELLO size"));
    }
    stream
        .write_all(&frame(&body, false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, welcome) = read_frame(stream)?;
    if !command || welcome.len() != 168 {
        return Err(DsError::new("CURVE WELCOME"));
    }
    let welcome_nonce = &welcome[8..24];
    let opened = nacl_open(
        &transient.secret,
        server_public,
        &prefix_nonce(b"WELCOME-", welcome_nonce),
        &welcome[24..],
    )?;
    if opened.len() != 128 {
        return Err(DsError::new("CURVE WELCOME box"));
    }
    let server_transient = crypto_box::PublicKey::from(
        <[u8; 32]>::try_from(&opened[..32]).map_err(|_| DsError::new("CURVE server key"))?,
    );
    let cookie = opened[32..].to_vec();
    let vouch_nonce = rand_bytes::<16>();
    let mut vouch_plain = [0u8; 64];
    vouch_plain[..32].copy_from_slice(transient.public.as_bytes());
    vouch_plain[32..].copy_from_slice(server_public.as_bytes());
    let vouch_box = nacl_box(
        &client_long.secret,
        &server_transient,
        &prefix_nonce(b"VOUCH---", &vouch_nonce),
        &vouch_plain,
    )?;
    let mut vouch = Vec::new();
    vouch.extend_from_slice(&vouch_nonce);
    vouch.extend_from_slice(&vouch_box);
    let mut initiate_plain = Vec::new();
    initiate_plain.extend_from_slice(client_long.public.as_bytes());
    initiate_plain.extend_from_slice(&vouch);
    initiate_plain.extend_from_slice(&metadata("PUSH"));
    let initiate_short = 2u64;
    let initiate_box = nacl_box(
        &transient.secret,
        &server_transient,
        &nonce(b"CurveZMQINITIATE", &initiate_short.to_be_bytes()),
        &initiate_plain,
    )?;
    let mut initiate = Vec::new();
    initiate.extend_from_slice(&cookie);
    initiate.extend_from_slice(&initiate_short.to_be_bytes());
    initiate.extend_from_slice(&initiate_box);
    stream
        .write_all(&frame(&command_body("INITIATE", &initiate), false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, ready_body) = read_frame(stream)?;
    if !command {
        return Err(DsError::new("CURVE READY"));
    }
    let ready_short = u64::from_be_bytes(
        ready_body
            .get(6..14)
            .and_then(|b| b.try_into().ok())
            .unwrap_or([0; 8]),
    );
    let _ = nacl_open(
        &transient.secret,
        &server_transient,
        &nonce(b"CurveZMQREADY---", &ready_short.to_be_bytes()),
        ready_body.get(14..).unwrap_or(&[]),
    )?;
    Ok(CurveSession {
        peer_transient: server_transient,
        local_transient: transient,
        peer_short: ready_short,
        local_short: initiate_short,
    })
}

fn curve_server(stream: &mut TcpStream, server: &CurveIdentity) -> Result<CurveSession, DsError> {
    exchange_greeting(stream, true, "CURVE")?;
    let (_more, command, hello) = read_frame(stream)?;
    if !command || hello.len() != 200 {
        return Err(DsError::new("CURVE HELLO"));
    }
    let client_public = crypto_box::PublicKey::from(
        <[u8; 32]>::try_from(&hello[80..112]).map_err(|_| DsError::new("CURVE client key"))?,
    );
    let short = u64::from_be_bytes(hello[112..120].try_into().unwrap_or([0; 8]));
    let zeros = nacl_open(
        &server.secret,
        &client_public,
        &nonce(b"CurveZMQHELLO---", &short.to_be_bytes()),
        &hello[120..],
    )?;
    if zeros != [0u8; 64] {
        return Err(DsError::new("CURVE HELLO box"));
    }
    let transient = CurveIdentity::generate();
    let cookie_key = rand_bytes::<32>();
    let cookie_nonce = rand_bytes::<16>();
    let mut cookie_plain = [0u8; 64];
    cookie_plain[..32].copy_from_slice(client_public.as_bytes());
    let transient_secret = transient.secret.to_bytes();
    cookie_plain[32..].copy_from_slice(&transient_secret);
    let cookie_box = secret_box(
        &cookie_key,
        &prefix_nonce(b"COOKIE--", &cookie_nonce),
        &cookie_plain,
    )?;
    let mut cookie = Vec::new();
    cookie.extend_from_slice(&cookie_nonce);
    cookie.extend_from_slice(&cookie_box);
    let mut welcome_plain = Vec::new();
    welcome_plain.extend_from_slice(transient.public.as_bytes());
    welcome_plain.extend_from_slice(&cookie);
    let welcome_nonce = rand_bytes::<16>();
    let welcome_box = nacl_box(
        &server.secret,
        &client_public,
        &prefix_nonce(b"WELCOME-", &welcome_nonce),
        &welcome_plain,
    )?;
    let mut welcome = Vec::new();
    welcome.extend_from_slice(&welcome_nonce);
    welcome.extend_from_slice(&welcome_box);
    let body = command_body("WELCOME", &welcome);
    if body.len() != 168 {
        return Err(DsError::new("CURVE WELCOME size"));
    }
    stream
        .write_all(&frame(&body, false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    let (_more, command, initiate) = read_frame(stream)?;
    if !command || &initiate[1..9] != b"INITIATE" {
        return Err(DsError::new("CURVE INITIATE"));
    }
    let data = &initiate[9..];
    if data.len() < 96 + 8 {
        return Err(DsError::new("CURVE INITIATE short"));
    }
    let cookie_back = &data[..96];
    let opened_cookie = secret_open(
        &cookie_key,
        &prefix_nonce(b"COOKIE--", &cookie_back[..16]),
        &cookie_back[16..],
    )?;
    if &opened_cookie[..32] != client_public.as_bytes() {
        return Err(DsError::new("CURVE cookie client"));
    }
    let initiate_short = u64::from_be_bytes(data[96..104].try_into().unwrap_or([0; 8]));
    if initiate_short <= short {
        return Err(DsError::new("CURVE short nonce did not increase"));
    }
    let initiate_plain = nacl_open(
        &transient.secret,
        &client_public,
        &nonce(b"CurveZMQINITIATE", &initiate_short.to_be_bytes()),
        &data[104..],
    )?;
    if initiate_plain.len() < 32 + 96 {
        return Err(DsError::new("CURVE INITIATE box"));
    }
    let client_long = crypto_box::PublicKey::from(
        <[u8; 32]>::try_from(&initiate_plain[..32]).map_err(|_| DsError::new("CURVE long key"))?,
    );
    let vouch = &initiate_plain[32..128];
    let vouch_plain = nacl_open(
        &transient.secret,
        &client_long,
        &prefix_nonce(b"VOUCH---", &vouch[..16]),
        &vouch[16..],
    )?;
    let client_transient = *client_public.as_bytes();
    let server_public_bytes = *server.public.as_bytes();
    if vouch_plain.len() != 64
        || vouch_plain[..32] != client_transient
        || vouch_plain[32..] != server_public_bytes
    {
        return Err(DsError::new("CURVE vouch"));
    }
    let ready_short = 1u64;
    let ready_box = nacl_box(
        &transient.secret,
        &client_public,
        &nonce(b"CurveZMQREADY---", &ready_short.to_be_bytes()),
        &metadata("PULL"),
    )?;
    let mut ready_body = Vec::new();
    ready_body.extend_from_slice(&ready_short.to_be_bytes());
    ready_body.extend_from_slice(&ready_box);
    stream
        .write_all(&frame(&command_body("READY", &ready_body), false, true))
        .map_err(|err| DsError::new(err.to_string()))?;
    Ok(CurveSession {
        peer_transient: client_public,
        local_transient: transient,
        peer_short: initiate_short,
        local_short: ready_short,
    })
}

fn write_curve_message(
    stream: &mut TcpStream,
    session: &mut CurveSession,
    payload: &[u8],
) -> Result<(), DsError> {
    session.local_short += 1;
    let boxed = nacl_box(
        &session.local_transient.secret,
        &session.peer_transient,
        &nonce(b"CurveZMQMESSAGE-", &session.local_short.to_be_bytes()),
        payload,
    )?;
    let mut body = Vec::new();
    body.extend_from_slice(&session.local_short.to_be_bytes());
    body.extend_from_slice(&boxed);
    stream
        .write_all(&frame(&command_body("MESSAGE", &body), false, true))
        .map_err(|err| DsError::new(err.to_string()))
}

fn read_curve_messages(
    stream: &mut TcpStream,
    session: &mut CurveSession,
    count: usize,
) -> Result<Vec<Vec<u8>>, DsError> {
    let mut parts = Vec::new();
    while parts.len() < count {
        let (_more, command, body) = read_frame(stream)?;
        if !command || body.len() < 15 || &body[1..8] != b"MESSAGE" {
            return Err(DsError::new("CURVE MESSAGE"));
        }
        let short = u64::from_be_bytes(body[8..16].try_into().unwrap_or([0; 8]));
        if short <= session.peer_short {
            return Err(DsError::new("CURVE message nonce replay"));
        }
        session.peer_short = short;
        let plain = nacl_open(
            &session.local_transient.secret,
            &session.peer_transient,
            &nonce(b"CurveZMQMESSAGE-", &short.to_be_bytes()),
            &body[16..],
        )?;
        parts.push(plain);
    }
    Ok(parts)
}

fn nonce(prefix: &[u8; 16], short: &[u8; 8]) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[..16].copy_from_slice(prefix);
    out[16..].copy_from_slice(short);
    out
}

fn prefix_nonce(prefix: &[u8], long: &[u8]) -> [u8; 24] {
    let mut out = [0u8; 24];
    let n = prefix.len().min(8);
    out[..n].copy_from_slice(&prefix[..n]);
    let copy = long.len().min(16);
    out[8..8 + copy].copy_from_slice(&long[..copy]);
    out
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut out = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut out);
    out
}

fn nacl_box(
    sender: &crypto_box::SecretKey,
    receiver: &crypto_box::PublicKey,
    nonce: &[u8; 24],
    plain: &[u8],
) -> Result<Vec<u8>, DsError> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    let salsa = crypto_box::SalsaBox::new(receiver, sender);
    salsa
        .encrypt(GenericArray::from_slice(nonce), plain)
        .map_err(|_| DsError::new("curve box"))
}

fn nacl_open(
    receiver: &crypto_box::SecretKey,
    sender: &crypto_box::PublicKey,
    nonce: &[u8; 24],
    boxed: &[u8],
) -> Result<Vec<u8>, DsError> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    let salsa = crypto_box::SalsaBox::new(sender, receiver);
    salsa
        .decrypt(GenericArray::from_slice(nonce), boxed)
        .map_err(|_| DsError::new("curve box open"))
}

fn secret_box(key: &[u8; 32], nonce: &[u8; 24], plain: &[u8]) -> Result<Vec<u8>, DsError> {
    use crypto_secretbox::aead::{generic_array::GenericArray, Aead, KeyInit};
    let cipher = crypto_secretbox::XSalsa20Poly1305::new(GenericArray::from_slice(key));
    cipher
        .encrypt(GenericArray::from_slice(nonce), plain)
        .map_err(|_| DsError::new("curve cookie"))
}

fn secret_open(key: &[u8; 32], nonce: &[u8; 24], boxed: &[u8]) -> Result<Vec<u8>, DsError> {
    use crypto_secretbox::aead::{generic_array::GenericArray, Aead, KeyInit};
    let cipher = crypto_secretbox::XSalsa20Poly1305::new(GenericArray::from_slice(key));
    cipher
        .decrypt(GenericArray::from_slice(nonce), boxed)
        .map_err(|_| DsError::new("curve cookie open"))
}
