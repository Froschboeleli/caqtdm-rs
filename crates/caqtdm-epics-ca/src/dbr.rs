//! DBR payload codecs used by the soft IOC and the client.
//!
//! Native types 0..6, STS = native + 7, TIME = native + 14, GR = native + 21,
//! CTRL = native + 28. Metadata sizes follow `db_access.h` RISC padding:
//! a double STS value starts at offset 8, a TIME double value at offset 16,
//! and `DBR_CTRL_DOUBLE` carries precision, 8-byte units, and eight limits
//! before the value.

use caqtdm_core::{ChannelData, Limits, PvValue, Severity};

use crate::protocol::CaError;

pub const DBR_STRING: u16 = 0;
pub const DBR_SHORT: u16 = 1;
pub const DBR_FLOAT: u16 = 2;
pub const DBR_ENUM: u16 = 3;
pub const DBR_CHAR: u16 = 4;
pub const DBR_LONG: u16 = 5;
pub const DBR_DOUBLE: u16 = 6;
pub const DBR_TIME_DOUBLE: u16 = 20;
pub const DBR_GR_DOUBLE: u16 = 27;
pub const DBR_CTRL_DOUBLE: u16 = 34;

pub const MAX_STRING: usize = 40;
pub const MAX_UNITS: usize = 8;
pub const MAX_ENUM_STATES: usize = 16;
pub const MAX_ENUM_STRING: usize = 26;

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub name: String,
    pub value: f64,
    pub precision: i16,
    pub units: String,
    pub limits: Limits,
    pub severity: i16,
    pub status: i16,
    pub seconds: u32,
    pub nanoseconds: u32,
    pub generation: u64,
    /// Extra elements after [`Record::value`]. Empty means a scalar.
    pub waveform: Vec<f64>,
    pub enum_strings: Vec<String>,
}

impl Record {
    pub fn new(name: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            value,
            precision: 3,
            units: String::new(),
            limits: Limits {
                lower_display: 0.0,
                upper_display: 100.0,
                lower_alarm: f64::NAN,
                lower_warning: f64::NAN,
                upper_warning: f64::NAN,
                upper_alarm: f64::NAN,
                lower_ctrl: 0.0,
                upper_ctrl: 100.0,
            },
            severity: 0,
            status: 0,
            seconds: 0,
            nanoseconds: 0,
            generation: 1,
            waveform: Vec::new(),
            enum_strings: Vec::new(),
        }
    }

    pub fn element(&self, index: usize) -> f64 {
        if index == 0 {
            self.waveform.first().copied().unwrap_or(self.value)
        } else {
            self.waveform.get(index).copied().unwrap_or(0.0)
        }
    }

    pub fn count(&self) -> u32 {
        if self.waveform.is_empty() {
            1
        } else {
            self.waveform.len() as u32
        }
    }

    pub fn to_channel(&self) -> ChannelData {
        let mut data = ChannelData::scalar(self.element(0));
        data.precision = i32::from(self.precision);
        data.units = self.units.clone();
        data.limits = self.limits;
        data.severity = Severity::from_epics(self.severity);
        data.status = self.status;
        data.description = self.name.clone();
        data.enum_strings = self.enum_strings.clone();
        if self.waveform.len() > 1 {
            data.value = PvValue::Doubles(self.waveform.clone());
            data.element_count = self.waveform.len();
        }
        data
    }
}

pub fn native_of(dbr: u16) -> u16 {
    match dbr {
        0..=6 => dbr,
        7..=13 => dbr - 7,
        14..=20 => dbr - 14,
        21..=27 => dbr - 21,
        28..=34 => dbr - 28,
        _ => DBR_DOUBLE,
    }
}

pub fn value_size(native: u16) -> usize {
    match native {
        DBR_STRING => MAX_STRING,
        DBR_SHORT | DBR_ENUM => 2,
        DBR_FLOAT | DBR_LONG => 4,
        DBR_CHAR => 1,
        _ => 8,
    }
}

/// Bytes of metadata before the first value element.
pub fn meta_size(dbr: u16) -> usize {
    let native = native_of(dbr);
    match dbr {
        0..=6 => 0,
        7..=13 => 4 + sts_pad(native),
        14..=20 => 12 + time_pad(native),
        21..=27 => gr_ctrl_meta(native, false),
        28..=34 => gr_ctrl_meta(native, true),
        _ => 0,
    }
}

fn sts_pad(native: u16) -> usize {
    match native {
        DBR_CHAR => 1,
        DBR_DOUBLE => 4,
        _ => 0,
    }
}

fn time_pad(native: u16) -> usize {
    match native {
        DBR_SHORT | DBR_ENUM => 2,
        DBR_CHAR => 3,
        DBR_DOUBLE => 4,
        _ => 0,
    }
}

fn gr_ctrl_meta(native: u16, ctrl: bool) -> usize {
    let n = if ctrl { 8 } else { 6 };
    let head = 4;
    match native {
        DBR_STRING => head + sts_pad(native),
        DBR_ENUM => head + 2 + MAX_ENUM_STATES * MAX_ENUM_STRING,
        DBR_FLOAT => head + 4 + MAX_UNITS + n * 4,
        DBR_DOUBLE => head + 4 + MAX_UNITS + n * 8,
        DBR_SHORT => head + MAX_UNITS + n * 2,
        DBR_LONG => head + MAX_UNITS + n * 4,
        DBR_CHAR => head + MAX_UNITS + n + 1,
        _ => head,
    }
}

pub fn encode_dbr(record: &Record, dbr: u16, count: u32) -> Result<Vec<u8>, CaError> {
    let count = count.max(1) as usize;
    let mut buf = Vec::new();
    write_meta(&mut buf, record, dbr);
    let native = native_of(dbr);
    for index in 0..count {
        write_native(&mut buf, native, record.element(index));
    }
    Ok(buf)
}

pub fn decode_dbr(bytes: &[u8], dbr: u16) -> Result<ChannelData, CaError> {
    let meta = meta_size(dbr);
    if bytes.len() < meta {
        return Err(CaError::new("DBR payload shorter than its metadata"));
    }
    let mut data = ChannelData::scalar(0.0);
    if (7..=34).contains(&dbr) && bytes.len() >= 4 {
        data.status = i16::from_be_bytes([bytes[0], bytes[1]]);
        data.severity = Severity::from_epics(i16::from_be_bytes([bytes[2], bytes[3]]));
    }
    read_gr_ctrl(&mut data, bytes, dbr);
    let native = native_of(dbr);
    let size = value_size(native).max(1);
    let rest = &bytes[meta..];
    let count = (rest.len() / size).max(1);
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let at = index * size;
        let slice = rest.get(at..).unwrap_or(&[]);
        if slice.len() < size && index > 0 {
            break;
        }
        values.push(read_native(slice, native)?);
    }
    data.element_count = values.len();
    data.value = collapse_values(native, values);
    data.connected = true;
    data.access_read = true;
    data.access_write = true;
    Ok(data)
}

fn collapse_values(native: u16, values: Vec<PvValue>) -> PvValue {
    if values.len() <= 1 {
        return values.into_iter().next().unwrap_or(PvValue::Double(0.0));
    }
    match native {
        DBR_STRING => PvValue::Strings(values.into_iter().map(|value| value.as_text()).collect()),
        DBR_FLOAT | DBR_DOUBLE => PvValue::Doubles(
            values
                .into_iter()
                .map(|value| value.as_f64().unwrap_or(0.0))
                .collect(),
        ),
        _ => PvValue::Ints(
            values
                .into_iter()
                .map(|value| value.as_i64().unwrap_or(0))
                .collect(),
        ),
    }
}

fn read_gr_ctrl(data: &mut ChannelData, bytes: &[u8], dbr: u16) {
    if !(21..=34).contains(&dbr) {
        return;
    }
    let native = native_of(dbr);
    let ctrl = dbr >= 28;
    let n = if ctrl { 8 } else { 6 };
    match native {
        DBR_DOUBLE | DBR_FLOAT => {
            if bytes.len() < 16 {
                return;
            }
            data.precision = i32::from(i16::from_be_bytes([bytes[4], bytes[5]]));
            data.units = unpad(&bytes[8..16]);
            let mut limits = [0.0; 8];
            for (i, slot) in limits.iter_mut().enumerate().take(n) {
                if native == DBR_DOUBLE {
                    let at = 16 + i * 8;
                    *slot = bytes
                        .get(at..at + 8)
                        .and_then(|s| s.try_into().ok())
                        .map(f64::from_be_bytes)
                        .unwrap_or(0.0);
                } else {
                    let at = 16 + i * 4;
                    *slot = bytes
                        .get(at..at + 4)
                        .and_then(|s| s.try_into().ok())
                        .map(f32::from_be_bytes)
                        .map(f64::from)
                        .unwrap_or(0.0);
                }
            }
            data.limits = limits_from(limits);
        }
        DBR_SHORT | DBR_LONG | DBR_CHAR => {
            if bytes.len() < 12 {
                return;
            }
            data.units = unpad(&bytes[4..12]);
            let mut limits = [0.0; 8];
            for (i, slot) in limits.iter_mut().enumerate().take(n) {
                *slot = match native {
                    DBR_SHORT => {
                        let at = 12 + i * 2;
                        bytes
                            .get(at..at + 2)
                            .and_then(|s| s.try_into().ok())
                            .map(i16::from_be_bytes)
                            .map(f64::from)
                            .unwrap_or(0.0)
                    }
                    DBR_LONG => {
                        let at = 12 + i * 4;
                        bytes
                            .get(at..at + 4)
                            .and_then(|s| s.try_into().ok())
                            .map(i32::from_be_bytes)
                            .map(f64::from)
                            .unwrap_or(0.0)
                    }
                    _ => f64::from(*bytes.get(12 + i).unwrap_or(&0)),
                };
            }
            data.limits = limits_from(limits);
        }
        DBR_ENUM => {
            if bytes.len() < 6 {
                return;
            }
            let count = i16::from_be_bytes([bytes[4], bytes[5]]).max(0) as usize;
            let mut labels = Vec::new();
            for index in 0..count.min(MAX_ENUM_STATES) {
                let at = 6 + index * MAX_ENUM_STRING;
                let end = at + MAX_ENUM_STRING;
                if end > bytes.len() {
                    break;
                }
                labels.push(unpad(&bytes[at..end]));
            }
            data.enum_strings = labels;
        }
        _ => {}
    }
}

fn limits_from(limits: [f64; 8]) -> Limits {
    Limits {
        upper_display: limits[0],
        lower_display: limits[1],
        upper_alarm: limits[2],
        upper_warning: limits[3],
        lower_warning: limits[4],
        lower_alarm: limits[5],
        upper_ctrl: limits[6],
        lower_ctrl: limits[7],
    }
}

fn write_meta(buf: &mut Vec<u8>, record: &Record, dbr: u16) {
    if dbr <= 6 {
        return;
    }
    buf.extend_from_slice(&record.status.to_be_bytes());
    buf.extend_from_slice(&record.severity.to_be_bytes());
    let native = native_of(dbr);
    if (7..=13).contains(&dbr) {
        buf.resize(buf.len() + sts_pad(native), 0);
        return;
    }
    if (14..=20).contains(&dbr) {
        buf.extend_from_slice(&record.seconds.to_be_bytes());
        buf.extend_from_slice(&record.nanoseconds.to_be_bytes());
        buf.resize(buf.len() + time_pad(native), 0);
        return;
    }
    if !(21..=34).contains(&dbr) {
        return;
    }
    match native {
        DBR_ENUM => {
            let count = record.enum_strings.len().min(MAX_ENUM_STATES) as i16;
            buf.extend_from_slice(&count.to_be_bytes());
            for index in 0..MAX_ENUM_STATES {
                let mut raw = [0u8; MAX_ENUM_STRING];
                if let Some(text) = record.enum_strings.get(index) {
                    let bytes = text.as_bytes();
                    let n = bytes.len().min(MAX_ENUM_STRING - 1);
                    raw[..n].copy_from_slice(&bytes[..n]);
                }
                buf.extend_from_slice(&raw);
            }
        }
        DBR_SHORT | DBR_LONG | DBR_CHAR => {
            write_units(buf, &record.units);
            let n = if dbr >= 28 { 8 } else { 6 };
            let limits = limit_array(record);
            for value in limits.iter().take(n) {
                match native {
                    DBR_SHORT => buf.extend_from_slice(&(*value as i16).to_be_bytes()),
                    DBR_LONG => buf.extend_from_slice(&(*value as i32).to_be_bytes()),
                    _ => buf.push(*value as u8),
                }
            }
            if native == DBR_CHAR {
                buf.push(0);
            }
        }
        DBR_DOUBLE | DBR_FLOAT => {
            buf.extend_from_slice(&record.precision.to_be_bytes());
            buf.extend_from_slice(&0i16.to_be_bytes());
            write_units(buf, &record.units);
            let n = if dbr >= 28 { 8 } else { 6 };
            let limits = limit_array(record);
            for value in limits.iter().take(n) {
                if native == DBR_DOUBLE {
                    buf.extend_from_slice(&value.to_be_bytes());
                } else {
                    buf.extend_from_slice(&(*value as f32).to_be_bytes());
                }
            }
        }
        _ => {
            let missing = meta_size(dbr).saturating_sub(buf.len());
            buf.resize(buf.len() + missing, 0);
        }
    }
}

fn limit_array(record: &Record) -> [f64; 8] {
    [
        record.limits.upper_display,
        record.limits.lower_display,
        record.limits.upper_alarm,
        record.limits.upper_warning,
        record.limits.lower_warning,
        record.limits.lower_alarm,
        record.limits.upper_ctrl,
        record.limits.lower_ctrl,
    ]
}

fn write_units(buf: &mut Vec<u8>, units: &str) {
    let mut raw = [0u8; MAX_UNITS];
    let bytes = units.as_bytes();
    let n = bytes.len().min(MAX_UNITS - 1);
    raw[..n].copy_from_slice(&bytes[..n]);
    buf.extend_from_slice(&raw);
}

fn write_native(buf: &mut Vec<u8>, native: u16, value: f64) {
    match native {
        DBR_STRING => {
            let mut raw = [0u8; MAX_STRING];
            let text = format!("{value}");
            let bytes = text.as_bytes();
            let n = bytes.len().min(MAX_STRING - 1);
            raw[..n].copy_from_slice(&bytes[..n]);
            buf.extend_from_slice(&raw);
        }
        DBR_SHORT | DBR_ENUM => buf.extend_from_slice(&(value as i16).to_be_bytes()),
        DBR_FLOAT => buf.extend_from_slice(&(value as f32).to_be_bytes()),
        DBR_CHAR => buf.push(value as u8),
        DBR_LONG => buf.extend_from_slice(&(value as i32).to_be_bytes()),
        _ => buf.extend_from_slice(&value.to_be_bytes()),
    }
}

fn read_native(bytes: &[u8], native: u16) -> Result<PvValue, CaError> {
    match native {
        DBR_STRING => Ok(PvValue::String(unpad(
            bytes.get(..MAX_STRING).unwrap_or(bytes),
        ))),
        DBR_SHORT | DBR_ENUM => {
            let raw = take2(bytes)?;
            Ok(PvValue::Int(i64::from(i16::from_be_bytes(raw))))
        }
        DBR_FLOAT => {
            let raw = take4(bytes)?;
            Ok(PvValue::Double(f64::from(f32::from_be_bytes(raw))))
        }
        DBR_CHAR => Ok(PvValue::Int(i64::from(*bytes.first().unwrap_or(&0)))),
        DBR_LONG => {
            let raw = take4(bytes)?;
            Ok(PvValue::Int(i64::from(i32::from_be_bytes(raw))))
        }
        _ => {
            let raw = take8(bytes)?;
            Ok(PvValue::Double(f64::from_be_bytes(raw)))
        }
    }
}

fn take2(bytes: &[u8]) -> Result<[u8; 2], CaError> {
    bytes
        .get(..2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CaError::new("short DBR value"))
}

fn take4(bytes: &[u8]) -> Result<[u8; 4], CaError> {
    bytes
        .get(..4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CaError::new("short DBR value"))
}

fn take8(bytes: &[u8]) -> Result<[u8; 8], CaError> {
    bytes
        .get(..8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| CaError::new("short DBR value"))
}

fn unpad(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Decode a native write payload into a number.
pub fn decode_put(bytes: &[u8], native: u16) -> Result<f64, CaError> {
    Ok(read_native(bytes, native)?.as_f64().unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_round_trips_every_native_class() {
        let mut record = Record::new("WF", 1.0);
        record.waveform = vec![1.0, 2.5, -4.0];
        record.units = "mm".into();
        record.enum_strings = vec!["off".into(), "on".into()];
        record.limits.upper_display = 10.0;
        record.limits.lower_display = 0.0;
        record.severity = 1;
        record.status = 2;
        for native in 0u16..=6 {
            for class in [0u16, 7, 14, 21, 28] {
                let dbr = native + class;
                let encoded = encode_dbr(&record, dbr, 3).unwrap();
                assert_eq!(
                    encoded.len(),
                    meta_size(dbr) + 3 * value_size(native),
                    "dbr {dbr} length"
                );
                let decoded = decode_dbr(&encoded, dbr).unwrap();
                assert_eq!(decoded.element_count, 3, "dbr {dbr} count");
                match &decoded.value {
                    PvValue::Strings(values) => {
                        assert_eq!(values, &["1".to_string(), "2.5".into(), "-4".into()]);
                    }
                    PvValue::Doubles(values) => {
                        assert!((values[0] - 1.0).abs() < 1e-5);
                        assert!((values[1] - 2.5).abs() < 1e-5);
                        assert!((values[2] + 4.0).abs() < 1e-5);
                    }
                    PvValue::Ints(values) => {
                        let expected: Vec<i64> = if native == DBR_CHAR {
                            vec![1, 2, 0]
                        } else {
                            vec![1, 2, -4]
                        };
                        assert_eq!(values, &expected, "dbr {dbr}");
                    }
                    other => panic!("dbr {dbr} unexpected {other:?}"),
                }
                if dbr >= 7 {
                    assert_eq!(decoded.status, 2);
                    assert_eq!(decoded.severity, Severity::Minor);
                }
                if native == DBR_ENUM && dbr >= 21 {
                    assert_eq!(decoded.enum_strings, ["off".to_string(), "on".into()]);
                }
                if matches!(native, DBR_DOUBLE | DBR_FLOAT | DBR_SHORT | DBR_LONG) && dbr >= 21 {
                    assert_eq!(decoded.units, "mm");
                    assert!((decoded.limits.upper_display - 10.0).abs() < 1e-4);
                }
            }
        }
    }
}
