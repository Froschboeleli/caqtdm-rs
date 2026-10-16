use caqtdm_core::{ChannelData, ChannelSpec, PutValue};

use crate::error::DsError;

/// One monitor update. `id` is local to the plugin that produced it. The
/// [`crate::Hub`] remaps it to a display-wide id.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub id: u64,
    pub data: ChannelData,
}

/// Synchronous control-system plugin.
///
/// caQtDM's `ControlsInterface` is callback based. This trait polls instead,
/// so a background thread can own the socket and the UI thread only drains a
/// queue. `now_ms` is supplied by the caller. Plugins must not read the clock
/// themselves, which keeps tests deterministic.
pub trait DataSource: Send {
    fn name(&self) -> &str;

    fn add_monitor(&mut self, spec: &ChannelSpec) -> Result<u64, DsError>;

    fn clear_monitor(&mut self, id: u64) -> Result<(), DsError>;

    fn set_value(&mut self, spec: &ChannelSpec, value: &PutValue) -> Result<(), DsError>;

    fn poll(&mut self, now_ms: u64) -> Vec<Update>;

    fn describe(&self, spec: &ChannelSpec) -> String {
        spec.name.clone()
    }
}
