#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TransferStatus {
    #[default]
    Queued,
    InProgress,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub id: String,
    pub file_name: String,
    pub direction: TransferDirection,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
    /// Where the file lives on this PC: the source for a send, the delivered copy for a
    /// download. `None` until a download finishes.
    pub path: Option<String>,
}

impl Transfer {
    pub fn progress(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }

        (self.transferred_bytes as f32 / self.total_bytes as f32).clamp(0.0, 1.0)
    }
}
