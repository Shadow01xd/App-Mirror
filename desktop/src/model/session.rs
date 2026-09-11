use super::TyuMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub device_id: String,
    pub mode: TyuMode,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}
