pub mod discovery;
mod files;
pub mod identity;
pub mod media;
mod node;
pub mod sessions;
pub mod storage;
mod transport;
pub use node::*;
pub use tyu_protocol::{Frame, Message, StorageOp, StorageReply, TyuErrorCode};
pub use tyu_types::*;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Code(#[from] TyuErrorCode),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("transport: {0}")]
    Transport(String),
    #[error("persistence unavailable or corrupt")]
    Store,
}
impl CoreError {
    pub fn code(&self) -> TyuErrorCode {
        match self {
            Self::Code(c) => *c,
            Self::Io(_) | Self::Store => TyuErrorCode::Io,
            Self::Transport(_) => TyuErrorCode::NotAvailable,
        }
    }
}
pub type Result<T> = std::result::Result<T, CoreError>;
