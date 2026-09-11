#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceStatus {
    Available,
    Paired,
    #[default]
    Offline,
}
