//! EPICS Channel Access for the display manager.
//!
//! The header, search datagram, and DBR layouts follow `caProto.h` /
//! `db_access.h`. Native types 0 through 6, plus STS, TIME, GR, and CTRL,
//! round-trip as scalars or waveforms. The soft IOC serves each native type
//! on create, read, write, and monitor.
//! `epics-rs` is edition 2024 and is not linked.

mod client;
mod dbr;
mod ioc;
mod protocol;
mod source;

pub use client::{CaClient, MonitorEvent};
pub use dbr::{
    decode_dbr, decode_puts, encode_dbr, meta_size, native_of, value_size, Record, DBR_CHAR,
    DBR_CTRL_DOUBLE, DBR_DOUBLE, DBR_ENUM, DBR_FLOAT, DBR_GR_DOUBLE, DBR_LONG, DBR_SHORT,
    DBR_STRING, DBR_TIME_DOUBLE,
};
pub use ioc::SoftIoc;
pub use protocol::{
    align8, decode_header, decode_search_reply, pad_string, search_message, CaError, CaHeader,
    CA_MAJOR, CA_MINOR, CMD_ERROR, CMD_EVENTS_OFF, CMD_EVENTS_ON, CMD_EVENT_CANCEL, CMD_NOT_FOUND,
    CMD_SEARCH, CMD_SERVER_DISCONN, CMD_WRITE, DBE_LOG, DO_REPLY,
};
pub use source::CaSource;
