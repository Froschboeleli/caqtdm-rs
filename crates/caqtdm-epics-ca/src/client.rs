//! Blocking Channel Access client.
//!
//! Search is one UDP round trip. The TCP session creates the channel, then
//! reads, writes, and subscribes with the DBR type the caller asks for.
//! The convenience methods still use `DBR_CTRL_DOUBLE`, native double, and
//! `DBR_TIME_DOUBLE`.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use caqtdm_core::ChannelData;

use crate::dbr::{
    decode_dbr, encode_dbr, native_of, Record, DBR_CTRL_DOUBLE, DBR_DOUBLE, DBR_STRING,
    DBR_TIME_DOUBLE,
};
use crate::protocol::{
    decode_search_reply, search_message, take_message, version_message, CaError, CaHeader,
    ACCESS_READ, CMD_ACCESS_RIGHTS, CMD_CREATE_CHAN, CMD_CREATE_CH_FAIL, CMD_EVENT_ADD,
    CMD_READ_NOTIFY, CMD_VERSION, CMD_WRITE_NOTIFY, DBE_ALARM, DBE_VALUE,
};

pub struct CaClient {
    stream: TcpStream,
    buf: Vec<u8>,
    inbox: VecDeque<(CaHeader, Vec<u8>)>,
    next_cid: u32,
    next_ioid: u32,
    channels: BTreeMap<String, Chan>,
    /// Subscription id to channel name; monitor replies carry the ECA
    /// status in the cid field, not the server id.
    subs: BTreeMap<u32, String>,
}

#[derive(Clone)]
struct Chan {
    sid: u32,
    access: u32,
}

#[derive(Debug, Clone)]
pub struct MonitorEvent {
    pub name: String,
    pub sub_id: u32,
    pub data: ChannelData,
}

impl CaClient {
    pub fn connect(addr: SocketAddr) -> Result<Self, CaError> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(&version_message())?;
        let mut client = Self {
            stream,
            buf: Vec::new(),
            inbox: VecDeque::new(),
            next_cid: 1,
            next_ioid: 1,
            channels: BTreeMap::new(),
            subs: BTreeMap::new(),
        };
        let _ = client.read_matching(|h| h.command == CMD_VERSION)?;
        client.write_msg(
            CaHeader::new(crate::protocol::CMD_CLIENT_NAME),
            &crate::protocol::pad_string("caqtdm-rs"),
        )?;
        client.write_msg(
            CaHeader::new(crate::protocol::CMD_HOST_NAME),
            &crate::protocol::pad_string("localhost"),
        )?;
        Ok(client)
    }

    pub fn search(udp: SocketAddr, name: &str) -> Result<SocketAddr, CaError> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut datagram = version_message();
        datagram.extend_from_slice(&search_message(1, name));
        socket.send_to(&datagram, udp)?;
        let mut buf = [0u8; 2048];
        let (n, src) = socket.recv_from(&mut buf)?;
        let reply = decode_search_reply(&buf[..n])?;
        let ip = if reply.server_ip == 0 || reply.server_ip == 0xFFFF_FFFF {
            match src.ip() {
                std::net::IpAddr::V4(v4) => v4,
                other => return Err(CaError::new(format!("search reply from {other}"))),
            }
        } else {
            std::net::Ipv4Addr::from(reply.server_ip)
        };
        Ok(SocketAddr::new(std::net::IpAddr::V4(ip), reply.server_port))
    }

    pub fn create(&mut self, name: &str) -> Result<(), CaError> {
        if self.channels.contains_key(name) {
            return Ok(());
        }
        let cid = self.next_cid;
        self.next_cid += 1;
        let mut header = CaHeader::new(CMD_CREATE_CHAN);
        header.cid = cid;
        header.available = u32::from(crate::protocol::CA_MINOR);
        self.write_msg(header, &crate::protocol::pad_string(name))?;
        let mut sid = None;
        let mut access = 0;
        for _ in 0..8 {
            let (header, _) = self.read_matching(|h| {
                h.command == CMD_CREATE_CHAN
                    || h.command == CMD_CREATE_CH_FAIL
                    || h.command == CMD_ACCESS_RIGHTS
            })?;
            match header.command {
                CMD_CREATE_CH_FAIL => {
                    return Err(CaError::new(format!("channel {name} not found")))
                }
                CMD_CREATE_CHAN => sid = Some(header.available),
                CMD_ACCESS_RIGHTS => access = header.available,
                _ => {}
            }
            if sid.is_some() && access != 0 {
                break;
            }
        }
        let sid = sid.ok_or_else(|| CaError::new(format!("no create reply for {name}")))?;
        self.channels.insert(name.to_string(), Chan { sid, access });
        Ok(())
    }

    pub fn read(&mut self, name: &str) -> Result<ChannelData, CaError> {
        self.read_dbr(name, DBR_CTRL_DOUBLE, 1)
    }

    pub fn read_dbr(&mut self, name: &str, dbr: u16, count: u32) -> Result<ChannelData, CaError> {
        let sid = self.sid(name)?;
        let ioid = self.alloc_ioid();
        let mut header = CaHeader::new(CMD_READ_NOTIFY);
        header.data_type = dbr;
        header.count = count.max(1);
        header.cid = sid;
        header.available = ioid;
        self.write_msg(header, &[])?;
        let (header, payload) =
            self.read_matching(|h| h.command == CMD_READ_NOTIFY && h.available == ioid)?;
        let mut data = decode_dbr(&payload, header.data_type)?;
        data.description = name.to_string();
        if let Some(chan) = self.channels.get(name) {
            data.access_read = chan.access & ACCESS_READ != 0;
            data.access_write = chan.access & crate::protocol::ACCESS_WRITE != 0;
        }
        Ok(data)
    }

    pub fn write_double(&mut self, name: &str, value: f64) -> Result<(), CaError> {
        self.write_values(name, DBR_DOUBLE, &[value])
    }

    pub fn write_values(&mut self, name: &str, dbr: u16, values: &[f64]) -> Result<(), CaError> {
        let native = native_of(dbr);
        let mut payload = Vec::new();
        for value in values {
            let record = Record::new("", *value);
            payload.extend(encode_dbr(&record, native, 1)?);
        }
        self.write_payload(name, native, values.len().max(1) as u32, &payload)
    }

    pub fn write_text(&mut self, name: &str, text: &str) -> Result<(), CaError> {
        let mut raw = [0u8; 40];
        let bytes = text.as_bytes();
        let n = bytes.len().min(39);
        raw[..n].copy_from_slice(&bytes[..n]);
        self.write_payload(name, DBR_STRING, 1, &raw)
    }

    fn write_payload(
        &mut self,
        name: &str,
        dbr: u16,
        count: u32,
        payload: &[u8],
    ) -> Result<(), CaError> {
        let sid = self.sid(name)?;
        let ioid = self.alloc_ioid();
        let mut header = CaHeader::new(CMD_WRITE_NOTIFY);
        header.data_type = dbr;
        header.count = count;
        header.cid = sid;
        header.available = ioid;
        self.write_msg(header, payload)?;
        let _ = self.read_matching(|h| h.command == CMD_WRITE_NOTIFY && h.available == ioid)?;
        Ok(())
    }

    pub fn subscribe(&mut self, name: &str) -> Result<u32, CaError> {
        self.subscribe_dbr(name, DBR_TIME_DOUBLE, 1)
    }

    pub fn subscribe_dbr(&mut self, name: &str, dbr: u16, count: u32) -> Result<u32, CaError> {
        let sid = self.sid(name)?;
        let sub_id = self.alloc_ioid();
        self.subs.insert(sub_id, name.to_string());
        let mut header = CaHeader::new(CMD_EVENT_ADD);
        header.data_type = dbr;
        header.count = count.max(1);
        header.cid = sid;
        header.available = sub_id;
        self.write_msg(
            header,
            &crate::protocol::event_add_body(DBE_VALUE | DBE_ALARM),
        )?;
        Ok(sub_id)
    }

    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>, CaError> {
        self.pull(Duration::from_millis(20))?;
        let mut events = Vec::new();
        let mut keep = VecDeque::new();
        while let Some((header, payload)) = self.inbox.pop_front() {
            if header.command != CMD_EVENT_ADD {
                keep.push_back((header, payload));
                continue;
            }
            let data = decode_dbr(&payload, header.data_type)
                .unwrap_or_else(|_| ChannelData::disconnected(""));
            let name = self.subs.get(&header.available).cloned().unwrap_or_default();
            events.push(MonitorEvent {
                name,
                sub_id: header.available,
                data,
            });
        }
        self.inbox = keep;
        Ok(events)
    }

    fn sid(&self, name: &str) -> Result<u32, CaError> {
        self.channels
            .get(name)
            .map(|c| c.sid)
            .ok_or_else(|| CaError::new(format!("channel {name} is not created")))
    }

    fn alloc_ioid(&mut self) -> u32 {
        let id = self.next_ioid;
        self.next_ioid += 1;
        id
    }

    fn write_msg(&mut self, header: CaHeader, payload: &[u8]) -> Result<(), CaError> {
        let frame = crate::protocol::encode_message(header, payload);
        self.stream.write_all(&frame)?;
        Ok(())
    }

    fn read_matching(
        &mut self,
        pred: impl Fn(&CaHeader) -> bool,
    ) -> Result<(CaHeader, Vec<u8>), CaError> {
        for _ in 0..40 {
            if let Some(pos) = self.inbox.iter().position(|(header, _)| pred(header)) {
                return self
                    .inbox
                    .remove(pos)
                    .ok_or_else(|| CaError::new("CA inbox entry disappeared"));
            }
            self.pull(Duration::from_secs(2))?;
        }
        Err(CaError::new("timed out waiting for a CA reply"))
    }

    fn pull(&mut self, timeout: Duration) -> Result<(), CaError> {
        while let Some(msg) = take_message(&mut self.buf)? {
            self.inbox.push_back(msg);
        }
        self.stream.set_read_timeout(Some(timeout))?;
        let mut tmp = [0u8; 8192];
        match self.stream.read(&mut tmp) {
            Ok(0) => return Err(CaError::new("CA server closed the connection")),
            Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        }
        while let Some(msg) = take_message(&mut self.buf)? {
            self.inbox.push_back(msg);
        }
        Ok(())
    }
}
