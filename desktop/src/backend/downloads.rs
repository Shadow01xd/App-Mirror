//! Where received files end up, and opening them from the app.
//!
//! The core writes incoming files into its own staging folder (its receive directory is fixed
//! when the node starts). TYU then moves each finished file to the folder the user picked, so
//! the destination can change at any time without restarting anything.
use std::io;
use std::path::{Path, PathBuf};

/// Staging folder the core writes into. Also the fallback destination.
pub fn staging() -> PathBuf {
    documents()
}

pub fn documents() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.document_dir().map(|p| p.join("TYU")))
        .unwrap_or_else(|| PathBuf::from("TYU"))
}

pub fn downloads() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.join("TYU")))
        .unwrap_or_else(documents)
}

fn setting() -> Option<PathBuf> {
    super::windows_store::data_directory()
        .ok()
        .map(|root| root.join("receive-directory.txt"))
}

/// The folder the user chose, or Documents/TYU the first time.
pub fn destination() -> PathBuf {
    let saved = setting()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| !path.as_os_str().is_empty());
    saved.unwrap_or_else(documents)
}

pub fn set_destination(path: &Path) -> io::Result<()> {
    let file = setting().ok_or_else(|| io::Error::other("No hay carpeta de datos"))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, path.to_string_lossy().as_bytes())
}

/// Moves a finished download into the chosen folder, never overwriting: `photo.jpg` becomes
/// `photo (2).jpg` when taken. Returns where the file ended up.
pub fn deliver(name: &str) -> io::Result<PathBuf> {
    deliver_from(&staging(), name, &destination())
}

fn deliver_from(staging: &Path, name: &str, target_dir: &Path) -> io::Result<PathBuf> {
    let source = staging.join(name);
    if staging == target_dir {
        return Ok(source);
    }
    if !source.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "El archivo recibido ya no está",
        ));
    }
    std::fs::create_dir_all(target_dir)?;
    let target = unique(target_dir, name);
    // Rename is atomic but only within one volume; Documents and D:\ are often different disks.
    if std::fs::rename(&source, &target).is_err() {
        std::fs::copy(&source, &target)?;
        std::fs::remove_file(&source)?;
    }
    Ok(target)
}

fn unique(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 2..10_000 {
        let attempt = match &extension {
            Some(extension) => format!("{stem} ({n}).{extension}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = directory.join(attempt);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(name)
}

/// Opens a file or folder with whatever Windows uses for it.
pub fn open(path: &Path) -> io::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
}

/// Opens the containing folder with the file already selected.
pub fn reveal(path: &Path) -> io::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A received file must never silently replace one that is already there.
    #[test]
    fn delivering_twice_keeps_both_files() {
        let root = std::env::temp_dir().join(format!("tyu-deliver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("nota.txt"), b"primero").unwrap();
        assert_eq!(unique(&root, "nota.txt"), root.join("nota (2).txt"));
        std::fs::write(root.join("nota (2).txt"), b"segundo").unwrap();
        assert_eq!(unique(&root, "nota.txt"), root.join("nota (3).txt"));
        assert_eq!(unique(&root, "otra.txt"), root.join("otra.txt"));
        // Extension-less names keep working.
        std::fs::write(root.join("README"), b"x").unwrap();
        assert_eq!(unique(&root, "README"), root.join("README (2)"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A received file lands in the chosen folder, with its bytes intact, leaving nothing
    /// behind in staging — and a second file of the same name does not destroy the first.
    #[test]
    fn a_received_file_is_delivered_to_the_chosen_folder() {
        let root = std::env::temp_dir().join(format!("tyu-inbox-{}", std::process::id()));
        let (staging, chosen) = (root.join("staging"), root.join("Descargas"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&staging).unwrap();

        std::fs::write(staging.join("foto.jpg"), b"primera foto").unwrap();
        let first = deliver_from(&staging, "foto.jpg", &chosen).unwrap();
        assert_eq!(first, chosen.join("foto.jpg"));
        assert_eq!(std::fs::read(&first).unwrap(), b"primera foto");
        assert!(
            !staging.join("foto.jpg").exists(),
            "staging was not emptied"
        );

        std::fs::write(staging.join("foto.jpg"), b"segunda foto").unwrap();
        let second = deliver_from(&staging, "foto.jpg", &chosen).unwrap();
        assert_eq!(second, chosen.join("foto (2).jpg"));
        assert_eq!(std::fs::read(&first).unwrap(), b"primera foto");
        assert_eq!(std::fs::read(&second).unwrap(), b"segunda foto");

        // Nothing to deliver is an error, not a silent success.
        assert!(deliver_from(&staging, "no-existe.bin", &chosen).is_err());
        // Delivering into the staging folder itself is a no-op, not a self-move.
        std::fs::write(staging.join("quieto.txt"), b"x").unwrap();
        let same = deliver_from(&staging, "quieto.txt", &staging).unwrap();
        assert_eq!(std::fs::read(same).unwrap(), b"x");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The chosen folder survives a restart, and the default is Documents/TYU.
    #[test]
    fn the_chosen_folder_is_remembered() {
        let original = destination();
        let wanted = std::env::temp_dir().join("tyu-destination-test");
        set_destination(&wanted).unwrap();
        assert_eq!(destination(), wanted);
        set_destination(&original).unwrap();
        assert_eq!(destination(), original);
        assert!(downloads().ends_with("TYU"));
        assert!(documents().ends_with("TYU"));
    }
}
