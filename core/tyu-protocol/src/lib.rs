//! TyuLink v1: explicit fixed envelope, frozen postcard payload schema.
use serde::{Deserialize, Serialize};
use tyu_types::*;

pub const ALPN: &[u8] = b"tyu/1";
pub const MAX_FRAME: usize = 128 * 1024;
pub const MAX_CLIPBOARD: usize = 64 * 1024;
pub const MAX_CHUNK: usize = 64 * 1024;
pub const HEADER: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum TyuErrorCode {
    #[error("malformed message")]
    Malformed,
    #[error("unsupported protocol version")]
    Version,
    #[error("unknown message type")]
    UnknownType,
    #[error("resource limit exceeded")]
    Limit,
    #[error("authentication failed")]
    Unauthorized,
    #[error("pairing ticket expired")]
    Expired,
    #[error("pairing ticket already used")]
    Replay,
    #[error("request rejected")]
    Rejected,
    #[error("peer revoked")]
    Revoked,
    #[error("unsupported capability")]
    Unsupported,
    #[error("not implemented for this platform")]
    NotImplementedForPlatform,
    #[error("not available")]
    NotAvailable,
    #[error("path denied")]
    PathDenied,
    #[error("integrity failure")]
    Integrity,
    #[error("operation cancelled")]
    Cancelled,
    #[error("operation timed out")]
    Timeout,
    #[error("I/O failure")]
    Io,
    #[error("invalid session state")]
    InvalidState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageOp {
    List {
        path: String,
    },
    Stat {
        path: String,
    },
    Read {
        path: String,
        offset: u64,
        length: u32,
    },
    Write {
        path: String,
        offset: u64,
        data: Vec<u8>,
    },
    CreateDir {
        path: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Delete {
        path: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageReply {
    Entries(Vec<StorageMetadata>),
    Data(Vec<u8>),
    Done,
}

// Append-only within v1. Never reorder these variants or their fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    Hello(DeviceInfo),
    HelloAck(DeviceInfo),
    PairRequest {
        ticket: [u8; 32],
        nonce: [u8; 32],
    },
    PairConfirm,
    PairReject(TyuErrorCode),
    PairComplete,
    Auth,
    Capabilities(Vec<Capability>),
    Heartbeat,
    Ping(u64),
    Pong(u64),
    Goodbye,
    ModeStart(TyuMode),
    ModeStop,
    ServiceStart(ServiceKind),
    ServiceStop,
    FileOffer(TransferMetadata),
    FileAccept {
        offset: u64,
    },
    FileReject(TyuErrorCode),
    FileProgress {
        transferred: u64,
    },
    FileCancel,
    FileComplete,
    Storage(StorageOp),
    StorageResult(StorageReply),
    MediaStart {
        stream: StreamId,
        format: MediaMetadata,
    },
    MediaStop {
        stream: StreamId,
    },
    MediaConfig {
        stream: StreamId,
        format: MediaMetadata,
    },
    MediaKeyframeRequest {
        stream: StreamId,
    },
    Input(InputEvent),
    Clipboard(String),
    Error {
        code: TyuErrorCode,
    },
    Ack,
    PairNearbyRequest {
        nonce: [u8; 32],
    },
}
impl Message {
    pub fn kind(&self) -> u16 {
        match self {
            Self::Hello(_) => 1,
            Self::HelloAck(_) => 2,
            Self::PairRequest { .. } => 3,
            Self::PairConfirm => 4,
            Self::PairReject(_) => 5,
            Self::PairComplete => 6,
            Self::Auth => 7,
            Self::Capabilities(_) => 8,
            Self::Heartbeat => 9,
            Self::Ping(_) => 10,
            Self::Pong(_) => 11,
            Self::Goodbye => 12,
            Self::ModeStart(_) => 13,
            Self::ModeStop => 14,
            Self::ServiceStart(_) => 15,
            Self::ServiceStop => 16,
            Self::FileOffer(_) => 17,
            Self::FileAccept { .. } => 18,
            Self::FileReject(_) => 19,
            Self::FileProgress { .. } => 20,
            Self::FileCancel => 21,
            Self::FileComplete => 22,
            Self::Storage(_) => 23,
            Self::StorageResult(_) => 24,
            Self::MediaStart { .. } => 25,
            Self::MediaStop { .. } => 26,
            Self::MediaConfig { .. } => 27,
            Self::MediaKeyframeRequest { .. } => 28,
            Self::Input(_) => 29,
            Self::Clipboard(_) => 30,
            Self::Error { .. } => 31,
            Self::Ack => 32,
            Self::PairNearbyRequest { .. } => 33,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub request: RequestId,
    pub session: Option<SessionId>,
    pub message: Message,
}
impl Frame {
    pub fn new(message: Message) -> Self {
        Self {
            request: RequestId::new(),
            session: None,
            message,
        }
    }
    pub fn reply(&self, message: Message) -> Self {
        Self {
            request: self.request,
            session: self.session,
            message,
        }
    }
    pub fn validate(&self) -> Result<(), TyuErrorCode> {
        use Message::*;
        let bounded = |s: &str, n: usize| {
            if s.len() <= n {
                Ok(())
            } else {
                Err(TyuErrorCode::Limit)
            }
        };
        match &self.message {
            Hello(d) | HelloAck(d) => {
                bounded(&d.name, 128)?;
                if d.name.trim().is_empty() || d.capabilities.len() > 32 {
                    return Err(TyuErrorCode::Malformed);
                }
            }
            Capabilities(c) if c.len() > 32 => return Err(TyuErrorCode::Limit),
            Clipboard(s) => bounded(s, MAX_CLIPBOARD)?,
            Input(e) if !e.valid() => return Err(TyuErrorCode::Malformed),
            FileOffer(m) => validate_filename(&m.name)?,
            Storage(op) => match op {
                StorageOp::List { path }
                | StorageOp::Stat { path }
                | StorageOp::CreateDir { path }
                | StorageOp::Delete { path } => bounded(path, 1024)?,
                StorageOp::Read { path, length, .. } => {
                    bounded(path, 1024)?;
                    if *length as usize > MAX_CHUNK {
                        return Err(TyuErrorCode::Limit);
                    }
                }
                StorageOp::Write { path, data, .. } => {
                    bounded(path, 1024)?;
                    if data.len() > MAX_CHUNK {
                        return Err(TyuErrorCode::Limit);
                    }
                }
                StorageOp::Rename { from, to } => {
                    bounded(from, 1024)?;
                    bounded(to, 1024)?;
                }
            },
            StorageResult(StorageReply::Entries(e)) => {
                if e.len() > 256 {
                    return Err(TyuErrorCode::Limit);
                }
                for entry in e {
                    bounded(&entry.path, 1024)?;
                }
            }
            StorageResult(StorageReply::Data(d)) if d.len() > MAX_CHUNK => {
                return Err(TyuErrorCode::Limit);
            }
            MediaStart { format, .. } | MediaConfig { format, .. } => validate_media(format)?,
            _ => {}
        }
        if matches!(
            self.message,
            ModeStart(_)
                | ModeStop
                | ServiceStart(_)
                | ServiceStop
                | MediaStart { .. }
                | MediaStop { .. }
                | MediaConfig { .. }
                | MediaKeyframeRequest { .. }
                | Input(_)
        ) && self.session.is_none()
        {
            return Err(TyuErrorCode::InvalidState);
        }
        Ok(())
    }
}
pub fn validate_media(format: &MediaMetadata) -> Result<(), TyuErrorCode> {
    let ok = match format {
        MediaMetadata::Video(v) => {
            v.width > 0
                && v.width <= 8192
                && v.height > 0
                && v.height <= 8192
                && v.fps > 0
                && v.fps <= 240
                && v.bitrate <= 200_000_000
        }
        MediaMetadata::Audio(a) => {
            (8000..=192000).contains(&a.sample_rate)
                && (1..=8).contains(&a.channels)
                && a.bitrate <= 20_000_000
        }
    };
    if ok { Ok(()) } else { Err(TyuErrorCode::Limit) }
}
pub fn validate_filename(name: &str) -> Result<(), TyuErrorCode> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 240
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "/\\:<>\"|?*".contains(c))
        || matches!(name, "." | "..")
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return Err(TyuErrorCode::PathDenied);
    }
    Ok(())
}
pub fn negotiate(remote: ProtocolVersion) -> Result<ProtocolVersion, TyuErrorCode> {
    if remote.major != 1 {
        Err(TyuErrorCode::Version)
    } else {
        Ok(ProtocolVersion {
            major: 1,
            minor: ProtocolVersion::V1.minor,
        })
    }
}
pub fn encode(frame: &Frame) -> Result<Vec<u8>, TyuErrorCode> {
    frame.validate()?;
    let payload = postcard::to_allocvec(frame).map_err(|_| TyuErrorCode::Malformed)?;
    if payload.len() > MAX_FRAME {
        return Err(TyuErrorCode::Limit);
    }
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(b"TYU1");
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&frame.message.kind().to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}
pub fn payload_length(header: &[u8]) -> Result<usize, TyuErrorCode> {
    if header.len() != HEADER || &header[..4] != b"TYU1" {
        return Err(TyuErrorCode::Malformed);
    }
    negotiate(ProtocolVersion {
        major: header[4] as u16,
        minor: header[5] as u16,
    })?;
    if header[5] != 0 {
        return Err(TyuErrorCode::Version);
    }
    let kind = u16::from_be_bytes([header[6], header[7]]);
    if !(1..=33).contains(&kind) {
        return Err(TyuErrorCode::UnknownType);
    }
    let len = u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
    if len > MAX_FRAME {
        return Err(TyuErrorCode::Limit);
    }
    Ok(len)
}
pub fn decode(bytes: &[u8]) -> Result<Frame, TyuErrorCode> {
    if bytes.len() < HEADER {
        return Err(TyuErrorCode::Malformed);
    }
    let len = payload_length(&bytes[..HEADER])?;
    if bytes.len() != HEADER + len {
        return Err(TyuErrorCode::Malformed);
    }
    let (frame, remainder): (Frame, &[u8]) =
        postcard::take_from_bytes(&bytes[HEADER..]).map_err(|_| TyuErrorCode::Malformed)?;
    if !remainder.is_empty() || frame.message.kind() != u16::from_be_bytes([bytes[6], bytes[7]]) {
        return Err(TyuErrorCode::Malformed);
    }
    frame.validate()?;
    Ok(frame)
}
