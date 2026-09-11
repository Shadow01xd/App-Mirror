#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Route {
    #[default]
    Welcome,
    SetupPhone,
    Nearby,
    Pairing,
    QrInstall,
    ConnectionError,
    Home,
    Monitor,
    MonitorActive,
    Mirror,
    MirrorActive,
    Bypass,
    Camera,
    Microphone,
    Storage,
    Transfer,
    TransferProgress,
    Sessions,
    DeviceDetails,
    Settings,
}
