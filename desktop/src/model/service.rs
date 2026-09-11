use super::TyuMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub mode: TyuMode,
    pub enabled: bool,
}
