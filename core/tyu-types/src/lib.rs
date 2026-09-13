//! Platform neutral domain values. UI models deliberately live in their clients.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        impl $name { pub fn new() -> Self { Self(Uuid::new_v4()) } }
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
        }
    )+};
}
id!(DeviceId, SessionId, TransferId, RequestId, StreamId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}
impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevicePlatform {
    Android,
    Windows,
    Rimi,
    LinuxFuture,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TyuMode {
    Monitor,
    Mirror,
    Bypass,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    MonitorSource,
    MonitorReceiver,
    MirrorSource,
    MirrorReceiver,
    CameraSource,
    CameraReceiver,
    MicrophoneSource,
    MicrophoneReceiver,
    StorageProvider,
    StorageConsumer,
    FileSend,
    FileReceive,
    ClipboardSource,
    ClipboardReceiver,
    TouchSource,
    InputReceiver,
    Proximity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub platform: DevicePlatform,
    pub capabilities: Vec<Capability>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Discovering,
    Pairing,
    Connecting,
    Connected,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustState {
    Unknown,
    Pending,
    Trusted,
    Revoked,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceKind {
    Camera,
    Microphone,
    Storage,
    Files,
    Clipboard,
    Input,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Negotiating,
    Ready,
    Stopped,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Negotiating,
    Ready,
    Stopped,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferMetadata {
    pub id: TransferId,
    pub name: String,
    pub size: u64,
    pub blake3: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageMetadata {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    HEVC,
    AV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    Opus,
    PCM,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Portrait,
    Landscape,
    PortraitFlipped,
    LandscapeFlipped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoFormat {
    pub width: u32,
    pub height: u32,
    pub fps: u16,
    pub bitrate: u32,
    pub codec: VideoCodec,
    pub orientation: Orientation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate: u32,
    pub codec: AudioCodec,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaMetadata {
    Video(VideoFormat),
    Audio(AudioFormat),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    TouchDown { contact: u8, x: f32, y: f32 },
    TouchMove { contact: u8, x: f32, y: f32 },
    TouchUp { contact: u8, x: f32, y: f32 },
    MouseMove { x: f32, y: f32 },
    MouseButton { button: u8, pressed: bool },
    Wheel { x: f32, y: f32 },
    KeyDown { code: u32 },
    KeyUp { code: u32 },
    TextInput(String),
}
impl InputEvent {
    pub fn valid(&self) -> bool {
        let norm = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        match self {
            Self::TouchDown { contact, x, y }
            | Self::TouchMove { contact, x, y }
            | Self::TouchUp { contact, x, y } => *contact < 32 && norm(*x) && norm(*y),
            Self::MouseMove { x, y } => norm(*x) && norm(*y),
            Self::Wheel { x, y } => {
                x.is_finite() && y.is_finite() && x.abs() <= 10000.0 && y.abs() <= 10000.0
            }
            Self::MouseButton { button, .. } => *button < 8,
            Self::TextInput(text) => text.len() <= 4096,
            _ => true,
        }
    }
}
/// Map normalized coordinates to the receiver's orientation and viewport.
pub fn map_viewport(
    x: f32,
    y: f32,
    orientation: Orientation,
    width: u32,
    height: u32,
) -> Option<(f32, f32)> {
    if !(InputEvent::MouseMove { x, y }).valid() || width == 0 || height == 0 {
        return None;
    }
    let (x, y) = match orientation {
        Orientation::Portrait => (x, y),
        Orientation::Landscape => (1.0 - y, x),
        Orientation::PortraitFlipped => (1.0 - x, 1.0 - y),
        Orientation::LandscapeFlipped => (y, 1.0 - x),
    };
    Some((x * (width - 1) as f32, y * (height - 1) as f32))
}
