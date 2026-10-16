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
mod archive_sites;
mod bsread;
mod env;
mod error;
mod gps;
mod hub;
mod intern;
mod modbus;
mod opcua;
mod opcua_bin;
mod opcua_handshake;
mod opcua_sec;
mod pattern;
mod sim;
mod source;
mod zmtp;

pub use archive::{
    build_archive_url, fetch_archiver, parse_archive_request, parse_archiver_json, parse_fixture,
    samples_to_json, serve_archiver, ArchiveRequest, ArchiveSample, ArchiveSource,
};
pub use archive_sites::{
    apply_gmv, decode_gmv_reply, decode_gmv_request, encode_gmv_reply, encode_gmv_request,
    fetch_gmv, gmv_window, hipa_query, parse_sf_response, post_sf, pro_query, serve_gmv, serve_sf,
    sf_query, GmvQuery, GmvSample, SfSample, HIPA_HOST, PRO_HOST,
};
pub use bsread::{
    bsread_parts, decode_bsread, decode_bsread_parts, encode_bsread, fetch_bsread, serve_bsread,
    BsMessage, BsreadSource,
};
pub use env::EnvSource;
pub use error::DsError;
pub use gps::GpsSource;
pub use hub::Hub;
pub use intern::InternSource;
pub use modbus::{
    decode_response, encode_request, exchange, serve_loopback, Mbap, ModbusSource, Pdu,
    RegisterMap, FN_READ_COILS, FN_READ_DISCRETE, FN_READ_HOLDING, FN_READ_INPUT, FN_WRITE_COIL,
    FN_WRITE_COILS, FN_WRITE_MULTIPLE, FN_WRITE_SINGLE,
};
pub use opcua::OpcuaSource;
pub use opcua_bin::{
    decode_ack, decode_hello, decode_read_response, encode_ack, encode_hello, encode_node_id,
    encode_read_request, read_node, serve_read, NodeId, READ_REQUEST, READ_RESPONSE, STATUS_GOOD,
    VARIANT_DOUBLE,
};
pub use opcua_handshake::{
    client_finish, client_open, identity_named, p_sha256, public_from_cert, server_accept,
    thumbprint, ChannelKeys, Identity,
};
pub use opcua_sec::{
    open as open_secure_chunk, seal as seal_secure_chunk, SecurityPolicy, SymKeys,
    POLICY_AES128_SHA256_RSAOAEP, POLICY_BASIC128RSA15, POLICY_BASIC256, POLICY_BASIC256SHA256,
    POLICY_NONE,
};
pub use sim::SimSource;
pub use source::{DataSource, Update};
pub use zmtp::{
    greeting, greeting_for, mechanism_of, parse_greeting, recv_bsread, send_bsread,
    send_bsread_curve, send_bsread_plain, serve_curve, serve_plain, serve_zmtp, CurveIdentity,
};

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
