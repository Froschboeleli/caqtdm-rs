//! EPICS Channel Access for the display manager.
//!
//! This is a scalar client and a matching in-process server. The header,
//! search datagram, and DBR layouts follow `caProto.h` / `db_access.h`. It
//! does not replace a site IOC: waveforms beyond a repeated scalar, beacons,
//! and the full DBR matrix are left to a production CA library. The
//! `epics-rs` crates are edition 2024 and pull an IOC stack, so they are not
//! linked here.

mod client;
mod dbr;
mod ioc;
mod protocol;
mod source;

pub use client::{CaClient, MonitorEvent};
pub use dbr::{
    decode_dbr, encode_dbr, meta_size, value_size, Record, DBR_CTRL_DOUBLE, DBR_DOUBLE,
    DBR_GR_DOUBLE, DBR_STRING, DBR_TIME_DOUBLE,
};
pub use ioc::SoftIoc;
pub use protocol::{
    align8, decode_header, decode_search_reply, pad_string, search_message, CaError, CaHeader,
    CA_MAJOR, CA_MINOR, CMD_ERROR, CMD_EVENTS_OFF, CMD_EVENTS_ON, CMD_EVENT_CANCEL, CMD_NOT_FOUND,
    CMD_SEARCH, CMD_SERVER_DISCONN, CMD_WRITE, DBE_LOG, DO_REPLY,
};
pub use source::CaSource;
