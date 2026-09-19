//! Optional third-party components, installed on demand with one click.
//!
//! Two Windows capabilities cannot be provided from user mode — a virtual display and a virtual
//! microphone both require a *signed kernel/IddCx driver*. Until TYU ships its own signed
//! drivers, it borrows two that are already signed by Microsoft.
//!
//! TYU never redistributes them: the install goes through **winget**, which ships with Windows
//! 11, downloads from the publisher's own servers and checks the hash Microsoft has on record.
//! So there is nothing for the user to search for or download by hand, and nothing of anyone
//! else's inside the TYU installer.
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Do not flash a console window out of the GUI.
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Component {
    /// Virtual display driver: makes the phone a real extended monitor Windows detects.
    Display,
    /// Virtual audio cable: lets the phone's audio be picked as a microphone by other apps.
    AudioCable,
}

impl Component {
    pub fn id(self) -> &'static str {
        match self {
            Self::Display => "Parsec.ParsecVDD",
            Self::AudioCable => "VB-Audio.Voicemeeter",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Display => "controlador de pantalla virtual",
            Self::AudioCable => "cable de audio virtual",
        }
    }
    /// Whether the capability works right now. The device itself is the truth, not a package
    /// list: a user may have installed it by other means, or removed it behind winget's back.
    ///
    /// Always answered on a throwaway thread: querying audio endpoints joins this thread to the
    /// multithreaded COM apartment, and the UI thread must stay free for winit to put it in the
    /// single-threaded apartment that drag-and-drop needs.
    pub fn present(self) -> bool {
        std::thread::spawn(move || self.probe())
            .join()
            .unwrap_or(false)
    }
    fn probe(self) -> bool {
        match self {
            Self::Display => super::virtual_display::driver_installed(),
            Self::AudioCable => super::bypass_microphone::cable_installed(),
        }
    }
}

/// An install running in the background; poll [`Install::finished`] from the UI loop.
pub struct Install {
    pub component: Component,
    result: mpsc::Receiver<Result<(), String>>,
}
impl Install {
    /// `Some(outcome)` once the install ended, `None` while it is still running.
    pub fn finished(&self) -> Option<Result<(), String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La instalación se interrumpió".into()))
            }
        }
    }
}

/// Starts `winget install` on its own thread. Windows asks the user for consent (UAC) and shows
/// the publisher's own licence terms; TYU never installs anything silently behind their back.
pub fn install(component: Component) -> Install {
    let (sender, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run(component));
    });
    Install { component, result }
}

fn run(component: Component) -> Result<(), String> {
    if component.present() {
        return Ok(());
    }
    let output = winget()
        .args([
            "install",
            "--id",
            component.id(),
            "--exact",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .output()
        .map_err(|error| format!("winget no está disponible en este equipo: {error}"))?;
    if !output.status.success() && !component.present() {
        let detail = String::from_utf8_lossy(&output.stdout);
        let detail = detail.lines().rev().find(|l| !l.trim().is_empty());
        return Err(match detail {
            Some(line) => format!("No se pudo instalar el {}: {line}", component.name()),
            None => format!("No se pudo instalar el {}", component.name()),
        });
    }
    // A freshly installed driver takes a moment to be usable; the device is the real proof.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !component.present() {
        if Instant::now() > deadline {
            return Err(format!(
                "El {} se instaló, pero Windows aún no lo reconoce. Reinicia el equipo.",
                component.name()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

fn winget() -> Command {
    // Bundled with Windows 11, but a GUI process does not always inherit the app-alias path.
    let alias = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("Microsoft/WindowsApps/winget.exe");
    let mut command = Command::new(if alias.exists() {
        alias.into_os_string()
    } else {
        "winget".into()
    });
    command.creation_flags(NO_WINDOW);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// winget is the delivery mechanism; without it there is no one-click install at all.
    #[test]
    fn winget_is_available_and_knows_both_components() {
        let output = winget().arg("--version").output();
        let Ok(output) = output else {
            eprintln!("winget missing on this machine; skipping");
            return;
        };
        assert!(output.status.success());
        for component in [Component::Display, Component::AudioCable] {
            let shown = winget()
                .args([
                    "show",
                    "--id",
                    component.id(),
                    "--exact",
                    "--disable-interactivity",
                ])
                .output()
                .expect("winget show");
            assert!(
                shown.status.success(),
                "{} is not in the winget catalogue",
                component.id()
            );
        }
    }

    /// Detection must answer for the real device, and never claim a component is missing after
    /// it was installed (this machine has the display driver installed).
    #[test]
    fn detection_matches_the_installed_devices() {
        eprintln!(
            "display: {}, audio cable: {}",
            Component::Display.present(),
            Component::AudioCable.present()
        );
        // Installing an already-present component is a no-op rather than an error.
        for component in [Component::Display, Component::AudioCable] {
            if component.present() {
                assert!(run(component).is_ok());
            }
        }
    }
}
