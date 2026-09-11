#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TyuMode {
    #[default]
    Monitor,
    Mirror,
    Bypass,
    Camera,
    Microphone,
    Storage,
}
