#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Discovering,
    Pairing,
    Connected,
    Failed(String),
}
