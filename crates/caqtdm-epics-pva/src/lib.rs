//! PV Access support for the display manager.
//!
//! Search frames match the pvxs byte layout (header, size-prefixed strings,
//! wildcard address of 16 zero bytes). GET, PUT, and MONITOR still use a
//! small scalar body so the existing loopback stays stable. `ntscalar`
//! encodes the pvData `epics:nt/NTScalar:1.0` field description and its
//! leaf values. `epics-pva-rs` is edition 2024 and is not linked.

mod client;
mod ntscalar;
mod proto;
mod server;
mod source;

pub use client::{MonitorEvent, PvaClient};
pub use ntscalar::{
    decode_introspection, decode_values, encode_full_bitset, encode_introspection, encode_values,
    NtScalar, STRUCT_ID,
};
pub use proto::{
    decode_header, decode_search_payload, decode_size, discover_frame, encode_size, encode_string,
    search_frame, Header, PvaError, CMD_BEACON, CMD_CONNECTION_VALIDATION, CMD_SEARCH,
    FLAG_BIG_ENDIAN, MAGIC, STATUS_OK, SUB_INIT, TYPE_DOUBLE, TYPE_STRING, VERSION,
};
pub use server::PvaServer;
pub use source::PvaSource;
