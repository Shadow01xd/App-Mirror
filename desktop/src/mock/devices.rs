use crate::model::{ConnectionState, Device, DeviceStatus};
pub fn devices() -> Vec<Device> {
    ["Teléfono de ejemplo"]
        .iter()
        .enumerate()
        .map(|(i, name)| Device {
            id: format!("demo-{}", i),
            name: name.to_string(),
            status: DeviceStatus::Available,
            connection: ConnectionState::Disconnected,
        })
        .collect()
}
