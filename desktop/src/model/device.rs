use super::{ConnectionState, DeviceStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub status: DeviceStatus,
    pub connection: ConnectionState,
}
