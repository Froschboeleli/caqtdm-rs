//! In-process PV Access peer for search, scalar GET/PUT/MONITOR, and NTScalar.
//!
//! The search reply is the pvxs SEARCH_RESPONSE shape. A GET or PUT whose
//! subcommand is `SUB_GET` (or a short scalar body) still uses the scalar
//! encoding. A GET with `SUB_INIT`, and a PUT whose body starts with the
//! `epics:nt/NTScalar:1.0` field description, carry NTScalar on the wire.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ntscalar::{
    decode_values, encode_full_bitset, encode_introspection, encode_values, NtScalar,
};
use crate::proto::{
    create_channel_response, decode_create_request, decode_scalar_body, decode_search_payload,
    frame, scalar_body, search_response, split_frame, PvaError, CMD_CREATE_CHANNEL, CMD_ECHO,
    CMD_GET, CMD_MONITOR, CMD_PUT, CMD_SEARCH, SUB_DATA, SUB_INIT,
};

struct Record {
    value: f64,
    severity: i32,
    status: i32,
    message: String,
    seconds: i64,
    nanoseconds: i32,
    user_tag: i32,
    generation: u64,
}

struct Database {
    records: BTreeMap<String, Record>,
    by_sid: BTreeMap<u32, String>,
    next_sid: u32,
}

#[derive(Clone)]
struct Sub {
    sub_id: u32,
    sid: u32,
    seen: u64,
}

pub struct PvaServer {
    stop: Arc<AtomicBool>,
    pub tcp: std::net::SocketAddr,
    pub udp: std::net::SocketAddr,
}

impl PvaServer {
    pub fn start(values: Vec<(&str, f64)>) -> Result<Self, PvaError> {
        let mut db = Database {
            records: BTreeMap::new(),
            by_sid: BTreeMap::new(),
            next_sid: 1,
        };
        for (name, value) in values {
            db.records.insert(
                name.to_string(),
                Record {
                    value,
                    severity: 0,
                    status: 0,
                    message: String::new(),
                    seconds: 0,
                    nanoseconds: 0,
                    user_tag: 0,
                    generation: 1,
                },
            );
        }
        let db = Arc::new(Mutex::new(db));
        let stop = Arc::new(AtomicBool::new(false));
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let tcp = listener.local_addr()?;
        let udp = UdpSocket::bind("127.0.0.1:0")?;
        udp.set_nonblocking(true)?;
        let udp_addr = udp.local_addr()?;
        let stop_tcp = Arc::clone(&stop);
        let db_tcp = Arc::clone(&db);
        std::thread::spawn(move || tcp_loop(listener, db_tcp, stop_tcp));
        let stop_udp = Arc::clone(&stop);
        std::thread::spawn(move || udp_loop(udp, tcp.port(), stop_udp));
        Ok(Self {
            stop,
            tcp,
            udp: udp_addr,
        })
    }
}

impl Drop for PvaServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn tcp_loop(listener: TcpListener, db: Arc<Mutex<Database>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let db = Arc::clone(&db);
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let _ = serve(stream, db, stop);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

fn udp_loop(socket: UdpSocket, tcp_port: u16, stop: Arc<AtomicBool>) {
    let mut buf = [0u8; 2048];
    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Some(reply) = reply_search(&buf[..n], tcp_port) {
                    let _ = socket.send_to(&reply, src);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

fn reply_search(datagram: &[u8], tcp_port: u16) -> Option<Vec<u8>> {
    let header = crate::proto::decode_header(datagram).ok()?;
    if header.command != CMD_SEARCH || datagram.len() < 8 + header.payload_len as usize {
        return None;
    }
    let payload = &datagram[8..8 + header.payload_len as usize];
    let req = decode_search_payload(payload, header.big_endian()).ok()?;
    let cids: Vec<u32> = req.channels.iter().map(|(cid, _)| *cid).collect();
    Some(search_response(
        req.sequence,
        tcp_port,
        !cids.is_empty(),
        &cids,
    ))
}

fn serve(
    mut stream: TcpStream,
    db: Arc<Mutex<Database>>,
    stop: Arc<AtomicBool>,
) -> Result<(), PvaError> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(40)));
    let mut buf = Vec::new();
    let mut subs = Vec::new();
    let mut tmp = [0u8; 4096];
    while !stop.load(Ordering::Relaxed) {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
        while let Some((header, total)) = split_frame(&buf)? {
            let payload = buf[8..total].to_vec();
            buf.drain(..total);
            if let Some(reply) = handle(&db, &mut subs, header.command, &payload)? {
                stream.write_all(&reply)?;
            }
        }
        if let Some(reply) = push_monitors(&db, &mut subs)? {
            stream.write_all(&reply)?;
        }
    }
    Ok(())
}

fn handle(
    db: &Mutex<Database>,
    subs: &mut Vec<Sub>,
    command: u8,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, PvaError> {
    match command {
        CMD_ECHO => Ok(Some(frame(true, CMD_ECHO, payload))),
        CMD_CREATE_CHANNEL => {
            let (cid, name) = decode_create_request(payload)?;
            let mut db = db.lock().expect("pva db");
            if !db.records.contains_key(&name) {
                return Ok(None);
            }
            let sid = db.next_sid;
            db.next_sid += 1;
            db.by_sid.insert(sid, name);
            Ok(Some(create_channel_response(cid, sid)))
        }
        CMD_GET => {
            let (sid, ioid, sub, _) = split_op(payload)?;
            let body = if sub & SUB_INIT != 0 {
                nt_body(&nt_of(db, sid))
            } else {
                scalar_body(value_of(db, sid).unwrap_or(0.0))
            };
            Ok(Some(op_reply(CMD_GET, sid, ioid, sub, &body)))
        }
        CMD_PUT => {
            let (sid, ioid, sub, extra) = split_op(payload)?;
            if let Some(nt) = nt_from_extra(extra) {
                let mut db = db.lock().expect("pva db");
                if let Some(name) = db.by_sid.get(&sid).cloned() {
                    if let Some(record) = db.records.get_mut(&name) {
                        apply_nt(record, &nt);
                    }
                }
            } else if let Ok(value) = decode_scalar_body(extra) {
                let mut db = db.lock().expect("pva db");
                if let Some(name) = db.by_sid.get(&sid).cloned() {
                    if let Some(record) = db.records.get_mut(&name) {
                        record.value = value;
                        record.generation = record.generation.wrapping_add(1);
                    }
                }
            }
            for sub in subs.iter_mut() {
                if sub.sid == sid {
                    sub.seen = 0;
                }
            }
            Ok(Some(op_reply(
                CMD_PUT,
                sid,
                ioid,
                sub,
                &[crate::proto::STATUS_OK],
            )))
        }
        CMD_MONITOR => {
            let (sid, ioid, _sub, _) = split_op(payload)?;
            subs.push(Sub {
                sub_id: ioid,
                sid,
                seen: 0,
            });
            push_monitors(db, subs)
        }
        _ => Ok(None),
    }
}

fn apply_nt(record: &mut Record, nt: &NtScalar) {
    record.value = nt.value;
    record.severity = nt.severity;
    record.status = nt.status;
    record.message = nt.message.clone();
    record.seconds = nt.seconds;
    record.nanoseconds = nt.nanoseconds;
    record.user_tag = nt.user_tag;
    record.generation = record.generation.wrapping_add(1);
}

fn nt_of(db: &Mutex<Database>, sid: u32) -> NtScalar {
    let db = db.lock().expect("pva db");
    let record = db.by_sid.get(&sid).and_then(|name| db.records.get(name));
    match record {
        Some(record) => NtScalar {
            value: record.value,
            severity: record.severity,
            status: record.status,
            message: record.message.clone(),
            seconds: record.seconds,
            nanoseconds: record.nanoseconds,
            user_tag: record.user_tag,
        },
        None => NtScalar {
            value: 0.0,
            severity: 3,
            status: 0,
            message: "missing".into(),
            seconds: 0,
            nanoseconds: 0,
            user_tag: 0,
        },
    }
}

fn nt_body(value: &NtScalar) -> Vec<u8> {
    let mut body = encode_introspection();
    body.extend_from_slice(&encode_full_bitset());
    body.extend_from_slice(&encode_values(value));
    body
}

fn nt_from_extra(extra: &[u8]) -> Option<NtScalar> {
    let intro = encode_introspection();
    if extra.len() <= intro.len() || extra.get(..intro.len()) != Some(intro.as_slice()) {
        return None;
    }
    let rest = &extra[intro.len()..];
    let size = *rest.first()? as usize;
    decode_values(rest.get(1 + size..)?).ok()
}

fn value_of(db: &Mutex<Database>, sid: u32) -> Option<f64> {
    let db = db.lock().expect("pva db");
    let name = db.by_sid.get(&sid)?;
    db.records.get(name).map(|r| r.value)
}

fn push_monitors(db: &Mutex<Database>, subs: &mut [Sub]) -> Result<Option<Vec<u8>>, PvaError> {
    let db = db.lock().expect("pva db");
    let mut out = Vec::new();
    for sub in subs.iter_mut() {
        let Some(name) = db.by_sid.get(&sub.sid) else {
            continue;
        };
        let Some(record) = db.records.get(name) else {
            continue;
        };
        if record.generation == sub.seen {
            continue;
        }
        sub.seen = record.generation;
        out.extend_from_slice(&op_reply(
            CMD_MONITOR,
            sub.sid,
            sub.sub_id,
            SUB_DATA,
            &scalar_body(record.value),
        ));
    }
    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

fn split_op(payload: &[u8]) -> Result<(u32, u32, u8, &[u8]), PvaError> {
    if payload.len() < 9 {
        return Err(PvaError::new("short PVA operation"));
    }
    let sid = u32::from_le_bytes(payload[0..4].try_into().unwrap_or([0; 4]));
    let ioid = u32::from_le_bytes(payload[4..8].try_into().unwrap_or([0; 4]));
    Ok((sid, ioid, payload[8], &payload[9..]))
}

fn op_reply(command: u8, sid: u32, ioid: u32, sub: u8, extra: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&sid.to_le_bytes());
    payload.extend_from_slice(&ioid.to_le_bytes());
    payload.push(sub);
    payload.extend_from_slice(extra);
    frame(true, command, &payload)
}
