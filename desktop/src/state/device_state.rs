use crate::model::{ConnectionState, Device};

#[derive(Debug, Default)]
pub struct DeviceState {
    pub nearby: Vec<Device>,
    pub selected: Option<Device>,
    pub connection: ConnectionState,
}
