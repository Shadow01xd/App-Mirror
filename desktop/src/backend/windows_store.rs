use std::path::PathBuf;
pub use tyu_core::identity::WindowsPersistentStore;
pub fn data_directory() -> tyu_core::Result<PathBuf> {
    directories::ProjectDirs::from("org", "TYU", "TYU Desktop")
        .map(|p| p.data_local_dir().to_owned())
        .ok_or(tyu_core::CoreError::Store)
}
