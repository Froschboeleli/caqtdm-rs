//! Modbus TCP (MBAP) codec and an in-process holding-register map.
//!
//! Function codes implemented: 3 (read holding), 6 (write single), 16 (write
//! multiple). Channel names are `holding:<address>` where address is the
//! zero-based register, or a 4xxxx number (40001 is register 0).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use caqtdm_core::{ChannelData, ChannelSpec, PutValue, PvValue};

use crate::error::DsError;
use crate::source::{DataSource, Update};

pub const FN_READ_HOLDING: u8 = 0x03;
pub const FN_WRITE_SINGLE: u8 = 0x06;
pub const FN_WRITE_MULTIPLE: u8 = 0x10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mbap {
    pub transaction: u16,
    pub protocol: u16,
    pub unit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pdu {
    ReadHolding { address: u16, count: u16 },
    ReadHoldingResponse { registers: Vec<u16> },
    WriteSingle { address: u16, value: u16 },
    WriteMultiple { address: u16, values: Vec<u16> },
    WriteMultipleResponse { address: u16, count: u16 },
    Exception { function: u8, code: u8 },
}

pub fn encode_request(mbap: &Mbap, pdu: &Pdu) -> Result<Vec<u8>, DsError> {
    let body = encode_pdu(pdu)?;
    let mut out = Vec::with_capacity(7 + body.len());
    out.extend_from_slice(&mbap.transaction.to_be_bytes());
    out.extend_from_slice(&mbap.protocol.to_be_bytes());
    out.extend_from_slice(&(body.len() as u16 + 1).to_be_bytes());
    out.push(mbap.unit);
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_response(bytes: &[u8]) -> Result<(Mbap, Pdu), DsError> {
    if bytes.len() < 8 {
        return Err(DsError::new("modbus frame shorter than MBAP"));
    }
    let transaction = u16::from_be_bytes([bytes[0], bytes[1]]);
    let protocol = u16::from_be_bytes([bytes[2], bytes[3]]);
    let length = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let unit = bytes[6];
    if bytes.len() < 6 + length {
        return Err(DsError::new("modbus frame truncated"));
    }
    let pdu = decode_pdu(&bytes[7..6 + length])?;
    Ok((
        Mbap {
            transaction,
            protocol,
            unit,
        },
        pdu,
    ))
}

fn encode_pdu(pdu: &Pdu) -> Result<Vec<u8>, DsError> {
    let mut body = Vec::new();
    match pdu {
        Pdu::ReadHolding { address, count } => {
            body.push(FN_READ_HOLDING);
            body.extend_from_slice(&address.to_be_bytes());
            body.extend_from_slice(&count.to_be_bytes());
        }
        Pdu::ReadHoldingResponse { registers } => {
            body.push(FN_READ_HOLDING);
            let nbytes = registers.len() * 2;
            if nbytes > 250 {
                return Err(DsError::new("modbus read response too large"));
            }
            body.push(nbytes as u8);
            for reg in registers {
                body.extend_from_slice(&reg.to_be_bytes());
            }
        }
        Pdu::WriteSingle { address, value } => {
            body.push(FN_WRITE_SINGLE);
            body.extend_from_slice(&address.to_be_bytes());
            body.extend_from_slice(&value.to_be_bytes());
        }
        Pdu::WriteMultiple { address, values } => {
            body.push(FN_WRITE_MULTIPLE);
            body.extend_from_slice(&address.to_be_bytes());
            body.extend_from_slice(&(values.len() as u16).to_be_bytes());
            body.push((values.len() * 2) as u8);
            for reg in values {
                body.extend_from_slice(&reg.to_be_bytes());
            }
        }
        Pdu::WriteMultipleResponse { address, count } => {
            body.push(FN_WRITE_MULTIPLE);
            body.extend_from_slice(&address.to_be_bytes());
            body.extend_from_slice(&count.to_be_bytes());
        }
        Pdu::Exception { function, code } => {
            body.push(function | 0x80);
            body.push(*code);
        }
    }
    Ok(body)
}

fn decode_pdu(bytes: &[u8]) -> Result<Pdu, DsError> {
    let function = *bytes
        .first()
        .ok_or_else(|| DsError::new("empty modbus pdu"))?;
    if function & 0x80 != 0 {
        let code = *bytes.get(1).unwrap_or(&0);
        return Ok(Pdu::Exception {
            function: function & 0x7f,
            code,
        });
    }
    match function {
        FN_READ_HOLDING if bytes.len() == 5 => Ok(Pdu::ReadHolding {
            address: u16::from_be_bytes([bytes[1], bytes[2]]),
            count: u16::from_be_bytes([bytes[3], bytes[4]]),
        }),
        FN_READ_HOLDING => {
            let nbytes = *bytes
                .get(1)
                .ok_or_else(|| DsError::new("modbus read missing length"))?
                as usize;
            if bytes.len() < 2 + nbytes || nbytes % 2 != 0 {
                return Err(DsError::new("modbus read length mismatch"));
            }
            let mut registers = Vec::new();
            let mut i = 2;
            while i < 2 + nbytes {
                registers.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
                i += 2;
            }
            Ok(Pdu::ReadHoldingResponse { registers })
        }
        FN_WRITE_SINGLE if bytes.len() >= 5 => Ok(Pdu::WriteSingle {
            address: u16::from_be_bytes([bytes[1], bytes[2]]),
            value: u16::from_be_bytes([bytes[3], bytes[4]]),
        }),
        FN_WRITE_MULTIPLE if bytes.len() == 5 => Ok(Pdu::WriteMultipleResponse {
            address: u16::from_be_bytes([bytes[1], bytes[2]]),
            count: u16::from_be_bytes([bytes[3], bytes[4]]),
        }),
        FN_WRITE_MULTIPLE => {
            if bytes.len() < 6 {
                return Err(DsError::new("modbus write-multiple truncated"));
            }
            let address = u16::from_be_bytes([bytes[1], bytes[2]]);
            let count = u16::from_be_bytes([bytes[3], bytes[4]]) as usize;
            let nbytes = bytes[5] as usize;
            if nbytes != count * 2 || bytes.len() < 6 + nbytes {
                return Err(DsError::new("modbus write-multiple length mismatch"));
            }
            let mut values = Vec::new();
            let mut i = 6;
            while i < 6 + nbytes {
                values.push(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
                i += 2;
            }
            Ok(Pdu::WriteMultiple { address, values })
        }
        other => Err(DsError::new(format!("unsupported modbus function {other}"))),
    }
}

#[derive(Clone)]
pub struct RegisterMap {
    holding: BTreeMap<u16, u16>,
}

impl RegisterMap {
    pub fn new() -> Self {
        Self {
            holding: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, address: u16, value: u16) {
        self.holding.insert(address, value);
    }

    pub fn get(&self, address: u16) -> u16 {
        self.holding.get(&address).copied().unwrap_or(0)
    }

    pub fn handle(&mut self, pdu: &Pdu) -> Pdu {
        match pdu {
            Pdu::ReadHolding { address, count } => {
                if *count == 0 || *count > 125 {
                    return Pdu::Exception {
                        function: FN_READ_HOLDING,
                        code: 3,
                    };
                }
                let registers = (*address..address.saturating_add(*count))
                    .map(|a| self.get(a))
                    .collect();
                Pdu::ReadHoldingResponse { registers }
            }
            Pdu::WriteSingle { address, value } => {
                self.set(*address, *value);
                Pdu::WriteSingle {
                    address: *address,
                    value: *value,
                }
            }
            Pdu::WriteMultiple { address, values } => {
                for (i, value) in values.iter().enumerate() {
                    self.set(address.saturating_add(i as u16), *value);
                }
                Pdu::WriteMultipleResponse {
                    address: *address,
                    count: values.len() as u16,
                }
            }
            other => Pdu::Exception {
                function: function_of(other),
                code: 1,
            },
        }
    }
}

impl Default for RegisterMap {
    fn default() -> Self {
        Self::new()
    }
}

fn function_of(pdu: &Pdu) -> u8 {
    match pdu {
        Pdu::ReadHolding { .. } | Pdu::ReadHoldingResponse { .. } => FN_READ_HOLDING,
        Pdu::WriteSingle { .. } => FN_WRITE_SINGLE,
        Pdu::WriteMultiple { .. } | Pdu::WriteMultipleResponse { .. } => FN_WRITE_MULTIPLE,
        Pdu::Exception { function, .. } => *function,
    }
}

/// Serve `map` on 127.0.0.1 until `stop` is set. Returns the bound address.
pub fn serve_loopback(
    map: RegisterMap,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let map = std::sync::Mutex::new(map);
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = handle_connection(&mut stream, &map);
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

fn handle_connection(
    stream: &mut TcpStream,
    map: &std::sync::Mutex<RegisterMap>,
) -> std::io::Result<()> {
    let mut header = [0u8; 7];
    if stream.read_exact(&mut header).is_err() {
        return Ok(());
    }
    let length = u16::from_be_bytes([header[4], header[5]]) as usize;
    if length == 0 {
        return Ok(());
    }
    let mut rest = vec![0u8; length - 1];
    stream.read_exact(&mut rest)?;
    let mut frame = header.to_vec();
    frame.extend_from_slice(&rest);
    let (mbap, pdu) = match decode_response(&frame) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let reply = map.lock().expect("modbus map").handle(&pdu);
    if let Ok(bytes) = encode_request(&mbap, &reply) {
        stream.write_all(&bytes)?;
    }
    Ok(())
}

pub fn exchange(addr: std::net::SocketAddr, mbap: &Mbap, pdu: &Pdu) -> Result<Pdu, DsError> {
    let mut stream = TcpStream::connect(addr).map_err(|e| DsError::new(e.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| DsError::new(e.to_string()))?;
    let bytes = encode_request(mbap, pdu)?;
    stream
        .write_all(&bytes)
        .map_err(|e| DsError::new(e.to_string()))?;
    let mut header = [0u8; 7];
    stream
        .read_exact(&mut header)
        .map_err(|e| DsError::new(e.to_string()))?;
    let length = u16::from_be_bytes([header[4], header[5]]) as usize;
    let mut rest = vec![0u8; length.saturating_sub(1)];
    stream
        .read_exact(&mut rest)
        .map_err(|e| DsError::new(e.to_string()))?;
    let mut frame = header.to_vec();
    frame.extend_from_slice(&rest);
    Ok(decode_response(&frame)?.1)
}

pub struct ModbusSource {
    map: RegisterMap,
    monitors: BTreeMap<u64, u16>,
    next: u64,
}

impl ModbusSource {
    pub fn new(map: RegisterMap) -> Self {
        Self {
            map,
            monitors: BTreeMap::new(),
            next: 1,
        }
    }
}

impl DataSource for ModbusSource {
    fn name(&self) -> &str {
        "modbus"
    }

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError> {
        let address = parse_address(&spec.name)?;
        let id = self.next;
        self.next += 1;
        self.monitors.insert(id, address);
        Ok(id)
    }

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError> {
        self.monitors
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| DsError::new(format!("unknown modbus monitor {id}")))
    }

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError> {
        let address = parse_address(&spec.name)?;
        let raw = value.as_f64().unwrap_or(0.0).clamp(0.0, 65535.0) as u16;
        self.map.set(address, raw);
        Ok(())
    }

    fn poll(&mut self, _now_ms: u64) -> Vec<Update> {
        self.monitors
            .iter()
            .map(|(id, address)| {
                let raw = self.map.get(*address);
                let mut data = ChannelData::scalar(f64::from(raw));
                data.value = PvValue::Int(i64::from(raw));
                data.description = format!("holding {address}");
                Update { id: *id, data }
            })
            .collect()
    }
}

fn parse_address(name: &str) -> Result<u16, DsError> {
    let body = name
        .strip_prefix("holding:")
        .or_else(|| name.strip_prefix("hr:"))
        .unwrap_or(name);
    let n: u32 = body
        .parse()
        .map_err(|_| DsError::new(format!("bad modbus address {name}")))?;
    if (40001..=49999).contains(&n) {
        Ok((n - 40001) as u16)
    } else if n <= u32::from(u16::MAX) {
        Ok(n as u16)
    } else {
        Err(DsError::new(format!("modbus address {name} out of range")))
    }
}
