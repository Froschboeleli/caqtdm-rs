//! pvxs `NTScalar` field description and leaf data.
//!
//! The layout follows pvData: an empty field name, tag `0x80`, struct id
//! `epics:nt/NTScalar:1.0`, then `value`, `alarm` (`alarm_t`), and
//! `timeStamp` (`time_t`) in that order. Leaf data is little-endian and
//! uses the same depth-first order. The scalar GET body in `proto` is a
//! different, smaller encoding and is left unchanged.

use crate::proto::{decode_string, encode_size, encode_string, PvaError};

const TAG_STRUCTURE: u8 = 0x80;
const TYPE_INT: u8 = 0x22;
const TYPE_LONG: u8 = 0x23;
const TYPE_DOUBLE: u8 = 0x43;
const TYPE_STRING: u8 = 0x60;

pub const STRUCT_ID: &str = "epics:nt/NTScalar:1.0";
pub const ALARM_ID: &str = "alarm_t";
pub const TIME_ID: &str = "time_t";

#[derive(Debug, Clone, PartialEq)]
pub struct NtScalar {
    pub value: f64,
    pub severity: i32,
    pub status: i32,
    pub message: String,
    pub seconds: i64,
    pub nanoseconds: i32,
    pub user_tag: i32,
}

pub fn encode_introspection() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&encode_string("", false));
    out.push(TAG_STRUCTURE);
    out.extend_from_slice(&encode_string(STRUCT_ID, false));
    out.extend_from_slice(&encode_size(3, false));
    write_scalar(&mut out, "value", TYPE_DOUBLE);
    write_nested(
        &mut out,
        "alarm",
        ALARM_ID,
        &[
            ("severity", TYPE_INT),
            ("status", TYPE_INT),
            ("message", TYPE_STRING),
        ],
    );
    write_nested(
        &mut out,
        "timeStamp",
        TIME_ID,
        &[
            ("secondsPastEpoch", TYPE_LONG),
            ("nanoseconds", TYPE_INT),
            ("userTag", TYPE_INT),
        ],
    );
    out
}

fn write_scalar(out: &mut Vec<u8>, name: &str, type_code: u8) {
    out.extend_from_slice(&encode_string(name, false));
    out.push(type_code);
}

fn write_nested(out: &mut Vec<u8>, name: &str, struct_id: &str, fields: &[(&str, u8)]) {
    out.extend_from_slice(&encode_string(name, false));
    out.push(TAG_STRUCTURE);
    out.extend_from_slice(&encode_string(struct_id, false));
    out.extend_from_slice(&encode_size(fields.len() as u32, false));
    for (field, type_code) in fields {
        write_scalar(out, field, *type_code);
    }
}

/// Leaf values in depth-first order, with no bitset in front.
pub fn encode_values(value: &NtScalar) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&value.value.to_le_bytes());
    out.extend_from_slice(&value.severity.to_le_bytes());
    out.extend_from_slice(&value.status.to_le_bytes());
    out.extend_from_slice(&encode_string(&value.message, false));
    out.extend_from_slice(&value.seconds.to_le_bytes());
    out.extend_from_slice(&value.nanoseconds.to_le_bytes());
    out.extend_from_slice(&value.user_tag.to_le_bytes());
    out
}

pub fn decode_values(bytes: &[u8]) -> Result<NtScalar, PvaError> {
    if bytes.len() < 16 {
        return Err(PvaError::new("NTScalar value is short"));
    }
    let value = f64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let severity = i32::from_le_bytes(bytes[8..12].try_into().unwrap_or([0; 4]));
    let status = i32::from_le_bytes(bytes[12..16].try_into().unwrap_or([0; 4]));
    let (message, used) = decode_string(&bytes[16..], false)?;
    let rest = &bytes[16 + used..];
    if rest.len() < 16 {
        return Err(PvaError::new("NTScalar timestamp is short"));
    }
    let seconds = i64::from_le_bytes(rest[0..8].try_into().unwrap_or([0; 8]));
    let nanoseconds = i32::from_le_bytes(rest[8..12].try_into().unwrap_or([0; 4]));
    let user_tag = i32::from_le_bytes(rest[12..16].try_into().unwrap_or([0; 4]));
    Ok(NtScalar {
        value,
        severity,
        status,
        message,
        seconds,
        nanoseconds,
        user_tag,
    })
}

/// Bit 0 set: the whole structure is present. Size-prefixed, one byte.
pub fn encode_full_bitset() -> Vec<u8> {
    let mut out = encode_size(1, false);
    out.push(0x01);
    out
}

pub fn decode_introspection(bytes: &[u8]) -> Result<(), PvaError> {
    let (name, n) = decode_string(bytes, false)?;
    if !name.is_empty() {
        return Err(PvaError::new("NTScalar top-level name must be empty"));
    }
    let rest = &bytes[n..];
    if rest.first().copied() != Some(TAG_STRUCTURE) {
        return Err(PvaError::new("NTScalar is not a structure"));
    }
    let (id, _) = decode_string(&rest[1..], false)?;
    if id != STRUCT_ID {
        return Err(PvaError::new(format!("unexpected struct id {id}")));
    }
    if !contains(bytes, ALARM_ID) || !contains(bytes, TIME_ID) {
        return Err(PvaError::new("NTScalar is missing alarm_t or time_t"));
    }
    Ok(())
}

fn contains(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introspection_and_values_round_trip() {
        let bytes = encode_introspection();
        decode_introspection(&bytes).unwrap();
        assert!(contains(&bytes, "value"));
        assert!(contains(&bytes, "severity"));
        assert!(contains(&bytes, "secondsPastEpoch"));
        let sample = NtScalar {
            value: 1.25,
            severity: 1,
            status: 2,
            message: "HIGH".into(),
            seconds: 1_700_000_000,
            nanoseconds: 250,
            user_tag: 7,
        };
        assert_eq!(decode_values(&encode_values(&sample)).unwrap(), sample);
        assert_eq!(encode_full_bitset(), vec![1, 0x01]);
    }
}
