//! Control-system plugins.
//!
//! caQtDM selects a plugin from the text before `://` in a channel name.
//! [`Hub`] does the same and polls each plugin with a caller-supplied time.
//!
//! EPICS Channel Access and PV Access live in `caqtdm-epics-ca` and
//! `caqtdm-epics-pva`. They implement [`DataSource`] and are registered on
//! the hub by the application. The plugins in this crate cover the rest of
//! the upstream set: simulated data, soft PVs, the environment, a GPS time
//! base, an archive fixture, Modbus TCP registers, and partial OPC UA and
//! bsread stand-ins.

mod archive;
mod bsread;
mod env;
mod error;
mod gps;
mod hub;
mod intern;
mod modbus;
mod opcua;
mod pattern;
mod sim;
mod source;

pub use archive::{
    build_archive_url, parse_archive_request, parse_fixture, ArchiveRequest, ArchiveSample,
    ArchiveSource,
};
pub use bsread::BsreadSource;
pub use env::EnvSource;
pub use error::DsError;
pub use gps::GpsSource;
pub use hub::Hub;
pub use intern::InternSource;
pub use modbus::{
    decode_response, encode_request, exchange, serve_loopback, Mbap, ModbusSource, Pdu,
    RegisterMap, FN_READ_HOLDING, FN_WRITE_MULTIPLE, FN_WRITE_SINGLE,
};
pub use opcua::OpcuaSource;
pub use sim::SimSource;
pub use source::{DataSource, Update};

/// Register the plugins that do not need a network peer.
///
/// `epics3` and `epics4` are left to the CA and PVA crates. A `sim` plugin is
/// also registered under `demo`, which is how caQtDM's demo plugin is selected
/// (`demo://sine`).
pub fn register_builtin(hub: &mut Hub) {
    hub.register(Box::new(SimSource::new("sim")));
    hub.register(Box::new(SimSource::new("demo")));
    hub.register(Box::new(InternSource::new()));
    hub.register(Box::new(EnvSource::new()));
    hub.register(Box::new(GpsSource::new()));
    hub.register(Box::new(ArchiveSource::empty()));
    hub.register(Box::new(ModbusSource::new(RegisterMap::new())));
    hub.register(Box::new(OpcuaSource::new()));
    hub.register(Box::new(BsreadSource::new()));
}
