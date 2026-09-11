use crate::model::TyuMode;

#[derive(Debug, Default)]
pub struct ModeState {
    pub selected: TyuMode,
    pub active: bool,
}
