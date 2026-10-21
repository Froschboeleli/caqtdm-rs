//! Non-blocking Channel Access client for live displays.
//!
//! It reads the libca environment (`EPICS_CA_ADDR_LIST`,
//! `EPICS_CA_AUTO_ADDR_LIST`, `EPICS_CA_NAME_SERVERS`,
//! `EPICS_CA_SERVER_PORT`, `EPICS_CA_MAX_ARRAY_BYTES`), searches over UDP and
//! over TCP name servers at the same time, opens one virtual circuit per
//! server, and subscribes with the channel's native `DBR_TIME_*` type after a
//! `DBR_CTRL_*` read for units, precision, limits and enum strings. Nothing in
//! `poll` blocks, so hundreds of channels connect in parallel and channels
//! that are not found never stall the display.

use std::collections::{BTreeMap, HashMap};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant, UNIX_EPOCH};

use caqtdm_core::ChannelData;

use crate::dbr::{decode_dbr, meta_size, value_size, DBR_DOUBLE, DBR_STRING};
use crate::protocol::{
    encode_message, pad_string, take_message, version_message, CaError, CaHeader, ACCESS_READ,
    ACCESS_WRITE, CA_MINOR, CMD_ACCESS_RIGHTS, CMD_CLIENT_NAME, CMD_CREATE_CHAN,
    CMD_CREATE_CH_FAIL, CMD_ECHO, CMD_ERROR, CMD_EVENT_ADD, CMD_HOST_NAME, CMD_READ_NOTIFY,
    CMD_SEARCH, CMD_SERVER_DISCONN, CMD_VERSION, CMD_WRITE, DBE_ALARM, DBE_VALUE, DONT_REPLY,
};

/// Seconds between the POSIX epoch and the EPICS epoch (1990-01-01).
const EPICS_EPOCH: u64 = 631_152_000;
const DBR_TIME_OFFSET: u16 = 14;
const DBR_CTRL_OFFSET: u16 = 28;
/// The PSI gateway (PCAS) postpones a burst of asynchronous reads and some
/// never complete. Keeping at most this many channels between create and
/// first data matches the staggered pace of TCP name search, which got every
/// channel of S_OP_overview through sf-cagw. 0 means no limit.
const DEFAULT_INFLIGHT: usize = 64;
/// A channel counts against the limit for at most this long.
const INFLIGHT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct CaConfig {
    pub addr_list: Vec<SocketAddr>,
    pub name_servers: Vec<SocketAddr>,
    pub max_array_bytes: usize,
    pub notes: Vec<String>,
}

impl CaConfig {
    /// Read the libca environment variables.
    pub fn from_env() -> Self {
        let get = |key: &str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
        let port: u16 = get("EPICS_CA_SERVER_PORT")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(5064);
        let mut notes = Vec::new();
        let mut addr_list = Vec::new();
        if let Some(list) = get("EPICS_CA_ADDR_LIST") {
            for entry in list.split_whitespace() {
                match resolve(entry, port) {
                    Ok(addr) => addr_list.push(addr),
                    Err(err) => notes.push(format!("EPICS_CA_ADDR_LIST {entry}: {err}")),
                }
            }
        }
        let auto = get("EPICS_CA_AUTO_ADDR_LIST")
            .map(|v| !v.trim().eq_ignore_ascii_case("no"))
            .unwrap_or(true);
        if auto {
            addr_list.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
        }
        let mut name_servers = Vec::new();
        if let Some(list) = get("EPICS_CA_NAME_SERVERS") {
            for entry in list.split_whitespace() {
                match resolve(entry, port) {
                    Ok(addr) => name_servers.push(addr),
                    Err(err) => notes.push(format!("EPICS_CA_NAME_SERVERS {entry}: {err}")),
                }
            }
        }
        let max_array_bytes = get("EPICS_CA_MAX_ARRAY_BYTES")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(16384);
        Self {
            addr_list,
            name_servers,
            max_array_bytes,
            notes,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.addr_list.is_empty() && self.name_servers.is_empty()
    }
}

fn resolve(entry: &str, default_port: u16) -> Result<SocketAddr, String> {
    let with_port = if entry.contains(':') {
        entry.to_string()
    } else {
        format!("{entry}:{default_port}")
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| "no IPv4 address".to_string())
}

struct Circuit {
    stream: TcpStream,
    rbuf: Vec<u8>,
    wbuf: Vec<u8>,
    /// Opened for TCP name search (EPICS_CA_NAME_SERVERS).
    name_server: bool,
    dead: bool,
}

impl Circuit {
    fn open(addr: SocketAddr, name_server: bool) -> Result<Self, CaError> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        let mut circuit = Self {
            stream,
            rbuf: Vec::new(),
            wbuf: Vec::new(),
            name_server,
            dead: false,
        };
        let mut hello = version_message();
        hello.extend(encode_message(
            CaHeader::new(CMD_CLIENT_NAME),
            &pad_string(&user_name()),
        ));
        hello.extend(encode_message(
            CaHeader::new(CMD_HOST_NAME),
            &pad_string(&host_name()),
        ));
        circuit.wbuf.extend(hello);
        Ok(circuit)
    }

    fn flush(&mut self) {
        while !self.wbuf.is_empty() && !self.dead {
            match self.stream.write(&self.wbuf) {
                Ok(0) => self.dead = true,
                Ok(n) => {
                    self.wbuf.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => self.dead = true,
            }
        }
    }

    fn fill(&mut self) {
        let mut tmp = vec![0u8; 256 * 1024];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => {
                    self.dead = true;
                    break;
                }
                Ok(n) => self.rbuf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    self.dead = true;
                    break;
                }
            }
        }
    }
}

fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "caqtdm-rs".into())
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".into())
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Searching { next: Instant, tries: u32 },
    Creating(SocketAddr),
    Connected { server: SocketAddr, sid: u32, native: u16, count: u32 },
}

struct Chan {
    name: String,
    state: State,
    access: u32,
    ctrl: Option<ChannelData>,
    latest: Option<ChannelData>,
    ever_connected: bool,
    /// Last ECA status of a monitor or read reply that carried no data.
    last_status: Option<u32>,
    events: u32,
    reads: u32,
    /// When CREATE_CHAN went out; `None` while the create waits in the queue.
    create_sent: Option<Instant>,
}

/// Counts for the console report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaStats {
    pub total: usize,
    pub connected: usize,
    pub with_value: usize,
}

pub struct LiveCa {
    config: CaConfig,
    udp: Option<UdpSocket>,
    circuits: HashMap<SocketAddr, Circuit>,
    /// cid is the index into `chans` plus one.
    chans: Vec<Chan>,
    by_name: BTreeMap<String, u32>,
    log: Vec<String>,
    started: Instant,
    /// Channels waiting for CREATE_CHAN, oldest first.
    create_queue: std::collections::VecDeque<u32>,
    /// Most channels between CREATE_CHAN and their first data reply.
    max_inflight: usize,
}

impl LiveCa {
    pub fn new(config: CaConfig) -> Self {
        let mut log: Vec<String> = config.notes.clone();
        let udp = match UdpSocket::bind("0.0.0.0:0") {
            Ok(sock) => {
                let _ = sock.set_broadcast(true);
                let _ = sock.set_nonblocking(true);
                Some(sock)
            }
            Err(err) => {
                log.push(format!("CA UDP socket: {err}"));
                None
            }
        };
        let mut live = Self {
            config,
            udp,
            circuits: HashMap::new(),
            chans: Vec::new(),
            by_name: BTreeMap::new(),
            log,
            started: Instant::now(),
            create_queue: std::collections::VecDeque::new(),
            max_inflight: std::env::var("CAQTDM_CA_INFLIGHT")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(DEFAULT_INFLIGHT),
        };
        for ns in live.config.name_servers.clone() {
            match Circuit::open(ns, true) {
                Ok(c) => {
                    live.log.push(format!("CA name server {ns} connected"));
                    live.circuits.insert(ns, c);
                }
                Err(err) => live.log.push(format!("CA name server {ns}: {err}")),
            }
        }
        live
    }

    /// Most channels between create and first data (0 = no limit).
    pub fn set_max_inflight(&mut self, limit: usize) {
        self.max_inflight = limit;
    }

    pub fn config(&self) -> &CaConfig {
        &self.config
    }

    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub fn add(&mut self, name: &str) {
        if self.by_name.contains_key(name) {
            return;
        }
        self.chans.push(Chan {
            name: name.to_string(),
            state: State::Searching {
                next: Instant::now(),
                tries: 0,
            },
            access: 0,
            ctrl: None,
            latest: None,
            ever_connected: false,
            last_status: None,
            events: 0,
            reads: 0,
            create_sent: None,
        });
        self.by_name.insert(name.to_string(), self.chans.len() as u32);
    }

    pub fn latest(&self, name: &str) -> Option<&ChannelData> {
        let cid = *self.by_name.get(name)?;
        let chan = &self.chans[cid as usize - 1];
        match chan.state {
            State::Connected { .. } => chan.latest.as_ref(),
            _ => None,
        }
    }

    pub fn stats(&self) -> CaStats {
        let mut stats = CaStats {
            total: self.chans.len(),
            ..CaStats::default()
        };
        for chan in &self.chans {
            if matches!(chan.state, State::Connected { .. }) {
                stats.connected += 1;
                if chan.latest.is_some() {
                    stats.with_value += 1;
                }
            }
        }
        stats
    }

    /// One line per connected channel that has no value yet: native type,
    /// element count, access rights, replies seen, and the last ECA status.
    pub fn valueless(&self) -> Vec<String> {
        self.chans
            .iter()
            .filter_map(|c| match c.state {
                State::Connected { native, count, server, .. } if c.latest.is_none() => Some(format!(
                    "{} native {} count {} access {} reads {} events {} status {:?} via {}",
                    c.name, native, count, c.access, c.reads, c.events, c.last_status, server
                )),
                _ => None,
            })
            .collect()
    }

    pub fn unconnected(&self) -> Vec<String> {
        self.chans
            .iter()
            .filter(|c| !matches!(c.state, State::Connected { .. }))
            .map(|c| c.name.clone())
            .collect()
    }

    pub fn put(&mut self, name: &str, text: Option<&str>, values: &[f64]) -> Result<(), CaError> {
        let cid = *self
            .by_name
            .get(name)
            .ok_or_else(|| CaError::new(format!("{name} is not monitored")))?;
        let chan = &self.chans[cid as usize - 1];
        let State::Connected { server, sid, native, .. } = chan.state else {
            return Err(CaError::new(format!("{name} is not connected")));
        };
        if std::env::var("CAQTDM_CA_WRITE").map(|v| v != "1").unwrap_or(true) {
            return Err(CaError::new(
                "Channel Access writes are off; set CAQTDM_CA_WRITE=1 to allow them",
            ));
        }
        if chan.access & ACCESS_WRITE == 0 {
            return Err(CaError::new(format!("{name} has no write access")));
        }
        let (dbr, count, payload) = if let Some(text) = text.filter(|_| native == DBR_STRING || values.is_empty()) {
            let mut raw = [0u8; 40];
            let n = text.len().min(39);
            raw[..n].copy_from_slice(&text.as_bytes()[..n]);
            (DBR_STRING, 1u32, raw.to_vec())
        } else {
            let mut payload = Vec::new();
            for v in values {
                payload.extend_from_slice(&v.to_be_bytes());
            }
            (DBR_DOUBLE, values.len().max(1) as u32, payload)
        };
        let mut payload = payload;
        payload.resize(crate::protocol::align8(payload.len()), 0);
        let mut header = CaHeader::new(CMD_WRITE);
        header.data_type = dbr;
        header.count = count;
        header.cid = sid;
        header.available = cid;
        let circuit = self
            .circuits
            .get_mut(&server)
            .ok_or_else(|| CaError::new("circuit gone"))?;
        circuit.wbuf.extend(encode_message(header, &payload));
        circuit.flush();
        Ok(())
    }

    /// Drive searches, connections and subscriptions. Never blocks.
    pub fn poll(&mut self) {
        self.send_searches();
        self.read_udp();
        self.send_creates();
        let addrs: Vec<SocketAddr> = self.circuits.keys().copied().collect();
        for addr in addrs {
            let mut messages = Vec::new();
            if let Some(c) = self.circuits.get_mut(&addr) {
                c.flush();
                c.fill();
                loop {
                    match take_message(&mut c.rbuf) {
                        Ok(Some(m)) => messages.push(m),
                        Ok(None) => break,
                        Err(err) => {
                            self.log.push(format!("CA {addr}: {err}"));
                            c.dead = true;
                            break;
                        }
                    }
                }
            }
            for (header, payload) in messages {
                self.handle(addr, header, &payload);
            }
            if let Some(c) = self.circuits.get_mut(&addr) {
                c.flush();
            }
        }
        let dead: Vec<SocketAddr> = self
            .circuits
            .iter()
            .filter(|(_, c)| c.dead)
            .map(|(a, _)| *a)
            .collect();
        for addr in dead {
            self.log.push(format!("CA circuit {addr} closed"));
            self.circuits.remove(&addr);
            for chan in &mut self.chans {
                let on = match chan.state {
                    State::Creating(a) => a == addr,
                    State::Connected { server, .. } => server == addr,
                    _ => false,
                };
                if on {
                    chan.state = State::Searching {
                        next: Instant::now() + Duration::from_secs(1),
                        tries: 0,
                    };
                }
            }
        }
    }

    fn send_searches(&mut self) {
        let now = Instant::now();
        let mut due = Vec::new();
        for (index, chan) in self.chans.iter_mut().enumerate() {
            if let State::Searching { next, tries } = chan.state {
                if next <= now {
                    let backoff = Duration::from_millis((50u64 << tries.min(7)).min(5000));
                    chan.state = State::Searching {
                        next: now + backoff,
                        tries: tries + 1,
                    };
                    due.push((index as u32 + 1, chan.name.clone()));
                }
            }
        }
        if due.is_empty() {
            return;
        }
        // UDP: pack searches into datagrams below 1450 bytes.
        if let Some(udp) = &self.udp {
            let mut datagram = version_message();
            let mut flush = |datagram: &mut Vec<u8>| {
                if datagram.len() > 16 {
                    for dest in &self.config.addr_list {
                        if let Err(err) = udp.send_to(datagram, dest) {
                            if self.log.len() < 200 {
                                self.log.push(format!("CA search to {dest}: {err}"));
                            }
                        }
                    }
                }
                *datagram = version_message();
            };
            for (cid, name) in &due {
                let msg = search(*cid, name);
                if datagram.len() + msg.len() > 1450 {
                    flush(&mut datagram);
                }
                datagram.extend(msg);
            }
            flush(&mut datagram);
        }
        // TCP name servers get the same search on their circuit.
        for circuit in self.circuits.values_mut().filter(|c| c.name_server) {
            for (cid, name) in &due {
                circuit.wbuf.extend(search(*cid, name));
            }
            circuit.flush();
        }
    }

    fn read_udp(&mut self) {
        let Some(udp) = &self.udp else { return };
        let mut replies = Vec::new();
        let mut buf = vec![0u8; 65536];
        loop {
            match udp.recv_from(&mut buf) {
                Ok((n, src)) => {
                    let mut bytes = buf[..n].to_vec();
                    while let Ok(Some((header, _))) = take_message(&mut bytes) {
                        if header.command == CMD_SEARCH {
                            replies.push((header, src));
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Windows reports ICMP port unreachable as ConnectionReset.
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            }
        }
        for (header, src) in replies {
            self.found(header, src);
        }
    }

    /// A search reply: `data_type` is the server port, `cid` the server IP
    /// (or all ones for "the sender"), `available` our channel id.
    fn found(&mut self, header: CaHeader, src: SocketAddr) {
        let cid = header.available;
        let Some(chan) = self.chans.get((cid as usize).wrapping_sub(1)) else {
            return;
        };
        if !matches!(chan.state, State::Searching { .. }) {
            return;
        }
        let ip = if header.cid == 0 || header.cid == 0xFFFF_FFFF {
            src.ip()
        } else {
            IpAddr::V4(Ipv4Addr::from(header.cid))
        };
        let server = SocketAddr::new(ip, header.data_type);
        if !self.circuits.contains_key(&server) {
            match Circuit::open(server, false) {
                Ok(c) => {
                    self.log.push(format!("CA circuit {server} opened"));
                    self.circuits.insert(server, c);
                }
                Err(err) => {
                    self.log.push(format!("CA circuit {server}: {err}"));
                    return;
                }
            }
        }
        self.queue_create(cid, server);
    }

    fn queue_create(&mut self, cid: u32, server: SocketAddr) {
        let chan = &mut self.chans[cid as usize - 1];
        chan.state = State::Creating(server);
        chan.create_sent = None;
        self.create_queue.push_back(cid);
    }

    fn inflight(&self) -> usize {
        let now = Instant::now();
        self.chans
            .iter()
            .filter(|c| {
                let waiting = match c.state {
                    State::Creating(_) => c.create_sent.is_some(),
                    State::Connected { .. } => {
                        c.latest.is_none() && c.last_status.is_none()
                    }
                    State::Searching { .. } => false,
                };
                waiting
                    && c.create_sent
                        .map(|at| now.duration_since(at) < INFLIGHT_TIMEOUT)
                        .unwrap_or(false)
            })
            .count()
    }

    fn send_creates(&mut self) {
        if self.create_queue.is_empty() {
            return;
        }
        let mut budget = if self.max_inflight == 0 {
            usize::MAX
        } else {
            self.max_inflight.saturating_sub(self.inflight())
        };
        while budget > 0 {
            let Some(cid) = self.create_queue.pop_front() else { break };
            let chan = &self.chans[cid as usize - 1];
            let State::Creating(server) = chan.state else { continue };
            if chan.create_sent.is_some() {
                continue;
            }
            let name = chan.name.clone();
            let mut header = CaHeader::new(CMD_CREATE_CHAN);
            header.cid = cid;
            header.available = u32::from(CA_MINOR);
            if let Some(c) = self.circuits.get_mut(&server) {
                c.wbuf.extend(encode_message(header, &pad_string(&name)));
                self.chans[cid as usize - 1].create_sent = Some(Instant::now());
                budget -= 1;
            } else {
                self.chans[cid as usize - 1].state = State::Searching {
                    next: Instant::now(),
                    tries: 0,
                };
            }
        }
    }

    /// A search reply over TCP (name server). libca ignores the address in
    /// it and creates the channel on the same circuit; the PSI gateway
    /// answers with port 0.
    fn found_on_circuit(&mut self, header: CaHeader, circuit: SocketAddr) {
        let cid = header.available;
        let Some(chan) = self.chans.get((cid as usize).wrapping_sub(1)) else {
            return;
        };
        if !matches!(chan.state, State::Searching { .. }) {
            return;
        }
        if self.circuits.contains_key(&circuit) {
            self.queue_create(cid, circuit);
        }
    }

    fn handle(&mut self, addr: SocketAddr, header: CaHeader, payload: &[u8]) {
        match header.command {
            CMD_VERSION => {}
            CMD_SEARCH => self.found_on_circuit(header, addr),
            CMD_ECHO => {
                if let Some(c) = self.circuits.get_mut(&addr) {
                    c.wbuf.extend(CaHeader::new(CMD_ECHO).encode());
                }
            }
            CMD_ACCESS_RIGHTS => {
                if let Some(chan) = self.chan_mut(header.cid) {
                    chan.access = header.available;
                }
            }
            CMD_CREATE_CHAN => self.created(addr, header),
            CMD_CREATE_CH_FAIL => {
                if let Some(chan) = self.chan_mut(header.cid) {
                    chan.state = State::Searching {
                        next: Instant::now() + Duration::from_secs(5),
                        tries: 6,
                    };
                }
            }
            CMD_SERVER_DISCONN => {
                if let Some(chan) = self.chan_mut(header.cid) {
                    chan.state = State::Searching {
                        next: Instant::now() + Duration::from_millis(200),
                        tries: 0,
                    };
                }
            }
            CMD_READ_NOTIFY => {
                if header.available == 0 {
                    return;
                }
                if header.cid != 1 || payload.is_empty() {
                    if let Some(chan) = self.chan_mut(header.available) {
                        chan.last_status = Some(header.cid);
                    }
                    return;
                }
                match decode_dbr(trim(payload, header), header.data_type) {
                    Ok(data) => {
                        if let Some(chan) = self.chan_mut(header.available) {
                            chan.reads += 1;
                            let mut data = data;
                            data.description = chan.name.clone();
                            data.access_read = chan.access & ACCESS_READ != 0;
                            data.access_write = chan.access & ACCESS_WRITE != 0;
                            match chan.latest.take() {
                                Some(latest) => {
                                    chan.latest = Some(merge(Some(&data), latest));
                                }
                                // The CTRL read carries the value too; show it
                                // until the first monitor event arrives.
                                None => chan.latest = Some(data.clone()),
                            }
                            chan.ctrl = Some(data);
                        }
                    }
                    Err(err) => self.note_decode(header.available, header.data_type, &err),
                }
            }
            CMD_EVENT_ADD => {
                if header.available == 0 {
                    return;
                }
                if header.cid != 1 || payload.is_empty() {
                    if let Some(chan) = self.chan_mut(header.available) {
                        chan.last_status = Some(header.cid);
                    }
                    return;
                }
                match decode_dbr(trim(payload, header), header.data_type) {
                    Ok(mut data) => {
                        if (DBR_TIME_OFFSET..DBR_TIME_OFFSET + 7).contains(&header.data_type)
                            && payload.len() >= 12
                        {
                            let secs = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                            let nsec = u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]);
                            data.timestamp = Some(
                                UNIX_EPOCH
                                    + Duration::from_secs(u64::from(secs) + EPICS_EPOCH)
                                    + Duration::from_nanos(u64::from(nsec.min(999_999_999))),
                            );
                        }
                        if let Some(chan) = self.chan_mut(header.available) {
                            chan.events += 1;
                            data.description = chan.name.clone();
                            data.access_read = chan.access & ACCESS_READ != 0;
                            data.access_write = chan.access & ACCESS_WRITE != 0;
                            let merged = merge(chan.ctrl.as_ref(), data);
                            chan.latest = Some(merged);
                        }
                    }
                    Err(err) => self.note_decode(header.available, header.data_type, &err),
                }
            }
            CMD_ERROR => {
                let original = crate::protocol::decode_header(payload).ok();
                let text_at = original.map(|(_, n)| n).unwrap_or(16);
                let text = payload
                    .get(text_at..)
                    .map(crate::protocol::unpad_string)
                    .unwrap_or_default();
                let what = original
                    .map(|(h, _)| format!("cmd {} cid {}", h.command, h.cid))
                    .unwrap_or_default();
                if self.log.len() < 500 {
                    self.log.push(format!(
                        "CA error from {addr}: status {} {what} {text}",
                        header.available
                    ));
                }
            }
            _ => {}
        }
    }

    fn note_decode(&mut self, cid: u32, dbr: u16, err: &CaError) {
        let name = self
            .chans
            .get((cid as usize).wrapping_sub(1))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        if self.log.len() < 500 {
            self.log.push(format!("CA decode {name} dbr {dbr}: {err}"));
        }
    }

    fn created(&mut self, addr: SocketAddr, header: CaHeader) {
        let cid = header.cid;
        let max_bytes = self.config.max_array_bytes.max(16384);
        let Some(chan) = self.chan_mut(cid) else { return };
        let native = header.data_type.min(6);
        let size = value_size(native).max(1);
        let count = header.count.max(1).min((max_bytes / size).max(1) as u32);
        chan.state = State::Connected {
            server: addr,
            sid: header.available,
            native,
            count,
        };
        chan.ever_connected = true;
        let sid = header.available;
        let mut out = Vec::new();
        let mut read = CaHeader::new(CMD_READ_NOTIFY);
        read.data_type = DBR_CTRL_OFFSET + native;
        read.count = count;
        read.cid = sid;
        read.available = cid;
        out.extend(encode_message(read, &[]));
        let mut sub = CaHeader::new(CMD_EVENT_ADD);
        sub.data_type = DBR_TIME_OFFSET + native;
        // count 0 asks for the current element count (CA 4.13); keep the
        // fixed count for older servers.
        sub.count = count;
        sub.cid = sid;
        sub.available = cid;
        out.extend(encode_message(
            sub,
            &crate::protocol::event_add_body(DBE_VALUE | DBE_ALARM),
        ));
        if let Some(c) = self.circuits.get_mut(&addr) {
            c.wbuf.extend(out);
        }
    }

    fn chan_mut(&mut self, cid: u32) -> Option<&mut Chan> {
        self.chans.get_mut((cid as usize).wrapping_sub(1))
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }
}

fn search(cid: u32, name: &str) -> Vec<u8> {
    let mut header = CaHeader::new(CMD_SEARCH);
    header.data_type = DONT_REPLY;
    header.count = u32::from(CA_MINOR);
    header.cid = cid;
    header.available = cid;
    encode_message(header, &pad_string(name))
}

/// Copy units, precision, limits and enum labels from the CTRL read into a
/// TIME monitor update.
fn merge(ctrl: Option<&ChannelData>, mut data: ChannelData) -> ChannelData {
    if let Some(ctrl) = ctrl {
        data.precision = ctrl.precision;
        data.units = ctrl.units.clone();
        data.limits = ctrl.limits;
        if data.enum_strings.is_empty() {
            data.enum_strings = ctrl.enum_strings.clone();
        }
    }
    data
}


/// Payloads are padded to 8 bytes; cut them to `count` elements so a padded
/// scalar does not decode as a two-element array.
fn trim(payload: &[u8], header: CaHeader) -> &[u8] {
    let count = header.count.max(1) as usize;
    let want = meta_size(header.data_type)
        + count * value_size(crate::dbr::native_of(header.data_type)).max(1);
    &payload[..want.min(payload.len())]
}
