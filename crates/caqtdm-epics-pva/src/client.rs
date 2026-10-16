//! Blocking client for the scalar PV Access subset.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use crate::proto::{
    create_channel_request, decode_header, decode_scalar_body, decode_search_response, op_request,
    scalar_body, search_frame, split_frame, PvaError, CMD_CREATE_CHANNEL, CMD_GET, CMD_MONITOR,
    CMD_PUT, SUB_GET,
};

pub struct PvaClient {
    stream: TcpStream,
    buf: Vec<u8>,
    inbox: VecDeque<(u8, Vec<u8>)>,
    next_cid: u32,
    next_ioid: u32,
    channels: BTreeMap<String, u32>,
}

#[derive(Debug, Clone)]
pub struct MonitorEvent {
    pub name: String,
    pub sub_id: u32,
    pub value: f64,
}

impl PvaClient {
    pub fn search(udp: SocketAddr, name: &str) -> Result<SocketAddr, PvaError> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        let datagram = search_frame(1, &[(7, name)], socket.local_addr()?.port(), true);
        socket.send_to(&datagram, udp)?;
        let mut buf = [0u8; 2048];
        let (n, src) = socket.recv_from(&mut buf)?;
        let header = decode_header(&buf[..n])?;
        let payload = &buf[8..8 + header.payload_len as usize];
        let reply = decode_search_response(payload)?;
        if !reply.found || !reply.cids.contains(&7) {
            return Err(PvaError::new(format!("PVA search missed {name}")));
        }
        let ip = match src.ip() {
            std::net::IpAddr::V4(v4) => v4,
            other => return Err(PvaError::new(format!("search from {other}"))),
        };
        Ok(SocketAddr::new(std::net::IpAddr::V4(ip), reply.port))
    }

    pub fn connect(addr: SocketAddr) -> Result<Self, PvaError> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        Ok(Self {
            stream,
            buf: Vec::new(),
            inbox: VecDeque::new(),
            next_cid: 1,
            next_ioid: 1,
            channels: BTreeMap::new(),
        })
    }

    pub fn create(&mut self, name: &str) -> Result<u32, PvaError> {
        if let Some(sid) = self.channels.get(name) {
            return Ok(*sid);
        }
        let cid = self.next_cid;
        self.next_cid += 1;
        self.stream.write_all(&create_channel_request(cid, name))?;
        let (_cmd, payload) = self.read_matching(|cmd, payload| {
            cmd == CMD_CREATE_CHANNEL
                && payload.len() >= 4
                && u32::from_le_bytes(payload[..4].try_into().unwrap_or([0; 4])) == cid
        })?;
        if payload.len() < 9 {
            return Err(PvaError::new("short create reply"));
        }
        let sid = u32::from_le_bytes(payload[4..8].try_into().unwrap_or([0; 4]));
        self.channels.insert(name.to_string(), sid);
        Ok(sid)
    }

    pub fn get(&mut self, name: &str) -> Result<f64, PvaError> {
        let sid = self.sid(name)?;
        let ioid = self.alloc();
        self.stream
            .write_all(&op_request(CMD_GET, sid, ioid, SUB_GET, &[]))?;
        let (_cmd, payload) =
            self.read_matching(|cmd, payload| cmd == CMD_GET && ioid_of(payload) == Some(ioid))?;
        decode_scalar_body(&payload[9..])
    }

    pub fn put(&mut self, name: &str, value: f64) -> Result<(), PvaError> {
        let sid = self.sid(name)?;
        let ioid = self.alloc();
        self.stream
            .write_all(&op_request(CMD_PUT, sid, ioid, 0, &scalar_body(value)))?;
        let _ =
            self.read_matching(|cmd, payload| cmd == CMD_PUT && ioid_of(payload) == Some(ioid))?;
        Ok(())
    }

    pub fn subscribe(&mut self, name: &str) -> Result<u32, PvaError> {
        let sid = self.sid(name)?;
        let ioid = self.alloc();
        self.stream
            .write_all(&op_request(CMD_MONITOR, sid, ioid, 0, &[]))?;
        Ok(ioid)
    }

    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>, PvaError> {
        self.pull(Duration::from_millis(20))?;
        let mut events = Vec::new();
        let mut keep = VecDeque::new();
        while let Some((cmd, payload)) = self.inbox.pop_front() {
            if cmd != CMD_MONITOR {
                keep.push_back((cmd, payload));
                continue;
            }
            let sub_id = ioid_of(&payload).unwrap_or(0);
            let sid = payload
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes);
            let value = decode_scalar_body(payload.get(9..).unwrap_or(&[])).unwrap_or(0.0);
            let name = self
                .channels
                .iter()
                .find(|(_, id)| Some(**id) == sid)
                .map(|(name, _)| name.clone())
                .unwrap_or_default();
            events.push(MonitorEvent {
                name,
                sub_id,
                value,
            });
        }
        self.inbox = keep;
        Ok(events)
    }

    fn sid(&self, name: &str) -> Result<u32, PvaError> {
        self.channels
            .get(name)
            .copied()
            .ok_or_else(|| PvaError::new(format!("PVA channel {name} is not created")))
    }

    fn alloc(&mut self) -> u32 {
        let id = self.next_ioid;
        self.next_ioid += 1;
        id
    }

    fn read_matching(
        &mut self,
        pred: impl Fn(u8, &[u8]) -> bool,
    ) -> Result<(u8, Vec<u8>), PvaError> {
        for _ in 0..40 {
            if let Some(pos) = self
                .inbox
                .iter()
                .position(|(cmd, payload)| pred(*cmd, payload))
            {
                return self
                    .inbox
                    .remove(pos)
                    .ok_or_else(|| PvaError::new("PVA inbox entry disappeared"));
            }
            self.pull(Duration::from_secs(2))?;
        }
        Err(PvaError::new("timed out waiting for a PVA reply"))
    }

    fn pull(&mut self, timeout: Duration) -> Result<(), PvaError> {
        while let Some((header, total)) = split_frame(&self.buf)? {
            let payload = self.buf[8..total].to_vec();
            self.inbox.push_back((header.command, payload));
            self.buf.drain(..total);
        }
        self.stream.set_read_timeout(Some(timeout))?;
        let mut tmp = [0u8; 8192];
        match self.stream.read(&mut tmp) {
            Ok(0) => return Err(PvaError::new("PVA server closed the connection")),
            Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        }
        while let Some((header, total)) = split_frame(&self.buf)? {
            let payload = self.buf[8..total].to_vec();
            self.inbox.push_back((header.command, payload));
            self.buf.drain(..total);
        }
        Ok(())
    }
}

fn ioid_of(payload: &[u8]) -> Option<u32> {
    payload
        .get(4..8)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}
