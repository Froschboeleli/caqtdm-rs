//! PV Access support for the display manager.
//!
//! Search frames match the pvxs byte layout (header, size-prefixed strings,
//! wildcard address of 16 zero bytes). GET, PUT, and MONITOR use a documented
//! scalar body so this client and this server can exchange doubles. A pvxs
//! server that expects `FieldDesc` introspection will not complete those
//! operations against this codec. That limit is intentional: the `epics-pva-rs`
//! crate is edition 2024 and is not linked.

mod client;
mod proto;
mod server;
mod source;

pub use client::{MonitorEvent, PvaClient};
pub use proto::{
    decode_header, decode_search_payload, decode_size, discover_frame, encode_size, encode_string,
    search_frame, Header, PvaError, CMD_BEACON, CMD_CONNECTION_VALIDATION, CMD_SEARCH,
    FLAG_BIG_ENDIAN, MAGIC, STATUS_OK, SUB_INIT, TYPE_DOUBLE, TYPE_STRING, VERSION,
};
pub use server::PvaServer;
pub use source::PvaSource;
