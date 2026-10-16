//! In-process Channel Access server.
//!
//! It answers UDP search, then TCP version, create-channel, access rights,
//! read notify, write notify, and event add. Each record advertises its
//! native DBR type. Reads, writes, and monitors use the type and count the
//! client asked for, including waveforms.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::dbr::{decode_puts, encode_dbr, native_of, Record, DBR_STRING, DBR_TIME_DOUBLE};
use crate::protocol::{
    search_reply, take_message, unpad_string, version_message, CaError, CaHeader, ACCESS_READ,
    ACCESS_WRITE, CMD_ACCESS_RIGHTS, CMD_CLEAR_CHANNEL, CMD_CLIENT_NAME, CMD_CREATE_CHAN,
    CMD_CREATE_CH_FAIL, CMD_ECHO, CMD_EVENT_ADD, CMD_HOST_NAME, CMD_READ_NOTIFY, CMD_SEARCH,
    CMD_VERSION, CMD_WRITE_NOTIFY, DONT_REPLY,
};

struct Database {
    records: BTreeMap<String, Record>,
    by_sid: BTreeMap<u32, String>,
    next_sid: u32,
}

#[derive(Clone)]
struct Sub {
    sub_id: u32,
    sid: u32,
    dbr: u16,
    count: u32,
    seen: u64,
}

pub struct SoftIoc {
    stop: Arc<AtomicBool>,
    pub tcp: std::net::SocketAddr,
    pub udp: std::net::SocketAddr,
    db: Arc<Mutex<Database>>,
}

impl SoftIoc {
    pub fn start(records: Vec<Record>) -> Result<Self, CaError> {
        let mut db = Database {
            records: BTreeMap::new(),
            by_sid: BTreeMap::new(),
            next_sid: 1,
        };
        for record in records {
            db.records.insert(record.name.clone(), record);
        }
        let db = Arc::new(Mutex::new(db));
        let stop = Arc::new(AtomicBool::new(false));
        let tcp_listener = TcpListener::bind("127.0.0.1:0")?;
        tcp_listener.set_nonblocking(true)?;
        let tcp = tcp_listener.local_addr()?;
        let udp_socket = UdpSocket::bind("127.0.0.1:0")?;
        udp_socket.set_nonblocking(true)?;
        let udp = udp_socket.local_addr()?;

        let stop_tcp = Arc::clone(&stop);
        let db_tcp = Arc::clone(&db);
        std::thread::spawn(move || tcp_loop(tcp_listener, db_tcp, stop_tcp));

        let stop_udp = Arc::clone(&stop);
        std::thread::spawn(move || udp_loop(udp_socket, tcp.port(), stop_udp));

        Ok(Self { stop, tcp, udp, db })
    }

    pub fn set(&self, name: &str, value: f64) {
        if let Some(record) = self.db.lock().expect("ca db").records.get_mut(name) {
            record.value = value;
            record.generation = record.generation.wrapping_add(1);
        }
    }
}

impl Drop for SoftIoc {
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
                    let _ = serve_client(stream, db, stop);
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
    let mut buf = datagram.to_vec();
    let mut out = version_message();
    let mut any = false;
    while let Ok(Some((header, _))) = take_message(&mut buf) {
        if header.command == CMD_SEARCH && header.data_type == DONT_REPLY {
            out.extend_from_slice(&search_reply(tcp_port, header.cid));
            any = true;
        }
    }
    any.then_some(out)
}

fn serve_client(
    mut stream: TcpStream,
    db: Arc<Mutex<Database>>,
    stop: Arc<AtomicBool>,
) -> Result<(), CaError> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let _ = stream.set_nodelay(true);
    let mut buf = Vec::new();
    let mut subs: Vec<Sub> = Vec::new();
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
        while let Some((header, payload)) = take_message(&mut buf)? {
            if let Some(frame) = handle(&db, &mut subs, header, &payload)? {
                stream.write_all(&frame)?;
            }
        }
        if let Some(frame) = push_events(&db, &mut subs)? {
            stream.write_all(&frame)?;
        }
    }
    Ok(())
}

fn handle(
    db: &Mutex<Database>,
    subs: &mut Vec<Sub>,
    header: CaHeader,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, CaError> {
    match header.command {
        CMD_VERSION => Ok(Some(version_message())),
        CMD_CLIENT_NAME | CMD_HOST_NAME | CMD_CLEAR_CHANNEL => Ok(None),
        CMD_ECHO => {
            let mut echo = header;
            echo.payload_size = 0;
            Ok(Some(echo.encode()))
        }
        CMD_CREATE_CHAN => Ok(Some(create_channel(db, header.cid, payload))),
        CMD_READ_NOTIFY => read_notify(db, header),
        CMD_WRITE_NOTIFY => write_notify(db, subs, header, payload),
        CMD_EVENT_ADD => event_add(db, subs, header),
        _ => Ok(None),
    }
}

fn create_channel(db: &Mutex<Database>, cid: u32, payload: &[u8]) -> Vec<u8> {
    let name = unpad_string(payload);
    let mut db = db.lock().expect("ca db");
    let (data_type, count) = match db.records.get(&name) {
        Some(record) => (record.native, record.count()),
        None => {
            let mut header = CaHeader::new(CMD_CREATE_CH_FAIL);
            header.cid = cid;
            return header.encode();
        }
    };
    let sid = db.next_sid;
    db.next_sid += 1;
    db.by_sid.insert(sid, name);
    let mut header = CaHeader::new(CMD_CREATE_CHAN);
    header.data_type = data_type;
    header.count = count;
    header.cid = cid;
    header.available = sid;
    let mut out = header.encode();
    let mut rights = CaHeader::new(CMD_ACCESS_RIGHTS);
    rights.cid = cid;
    rights.available = ACCESS_READ | ACCESS_WRITE;
    out.extend_from_slice(&rights.encode());
    out
}

fn read_notify(db: &Mutex<Database>, header: CaHeader) -> Result<Option<Vec<u8>>, CaError> {
    let db = db.lock().expect("ca db");
    let Some(name) = db.by_sid.get(&header.cid).cloned() else {
        return Ok(None);
    };
    let Some(record) = db.records.get(&name) else {
        return Ok(None);
    };
    let dbr = if header.data_type == 0 {
        record.native
    } else {
        header.data_type
    };
    let payload = encode_dbr(record, dbr, header.count.max(1))?;
    let mut reply = CaHeader::new(CMD_READ_NOTIFY);
    reply.data_type = dbr;
    reply.count = header.count.max(1);
    reply.cid = header.cid;
    reply.available = header.available;
    Ok(Some(crate::protocol::encode_message(reply, &payload)))
}

fn write_notify(
    db: &Mutex<Database>,
    subs: &mut [Sub],
    header: CaHeader,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, CaError> {
    let dbr = header.data_type;
    let native = native_of(dbr);
    let count = header.count.max(1);
    {
        let mut db = db.lock().expect("ca db");
        if let Some(name) = db.by_sid.get(&header.cid).cloned() {
            if let Some(record) = db.records.get_mut(&name) {
                if native == DBR_STRING {
                    record.text = unpad_string(payload);
                    if let Ok(value) = record.text.parse::<f64>() {
                        record.value = value;
                    }
                } else {
                    let values = decode_puts(payload, native, count)?;
                    if let Some(first) = values.first().copied() {
                        record.value = first;
                    }
                    if values.len() > 1 || !record.waveform.is_empty() {
                        if record.waveform.len() < values.len() {
                            record.waveform.resize(values.len(), 0.0);
                        }
                        for (index, value) in values.iter().enumerate() {
                            if let Some(slot) = record.waveform.get_mut(index) {
                                *slot = *value;
                            }
                        }
                    }
                }
                record.generation = record.generation.wrapping_add(1);
            }
        }
    }
    for sub in subs.iter_mut() {
        if sub.sid == header.cid {
            sub.seen = 0;
        }
    }
    let mut reply = CaHeader::new(CMD_WRITE_NOTIFY);
    reply.data_type = crate::protocol::ECA_NORMAL;
    reply.count = 1;
    reply.cid = header.cid;
    reply.available = header.available;
    Ok(Some(reply.encode()))
}

fn event_add(
    db: &Mutex<Database>,
    subs: &mut Vec<Sub>,
    header: CaHeader,
) -> Result<Option<Vec<u8>>, CaError> {
    subs.push(Sub {
        sub_id: header.available,
        sid: header.cid,
        dbr: if header.data_type == 0 {
            let db = db.lock().expect("ca db");
            db.by_sid
                .get(&header.cid)
                .and_then(|name| db.records.get(name))
                .map(|record| record.native.saturating_add(14))
                .unwrap_or(DBR_TIME_DOUBLE)
        } else {
            header.data_type
        },
        count: header.count.max(1),
        seen: 0,
    });
    push_events(db, subs)
}

fn push_events(db: &Mutex<Database>, subs: &mut [Sub]) -> Result<Option<Vec<u8>>, CaError> {
    let db = db.lock().expect("ca db");
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
        let payload = encode_dbr(record, sub.dbr, sub.count)?;
        let mut header = CaHeader::new(CMD_EVENT_ADD);
        header.data_type = sub.dbr;
        header.count = sub.count;
        header.cid = sub.sid;
        header.available = sub.sub_id;
        out.extend_from_slice(&crate::protocol::encode_message(header, &payload));
    }
    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}
