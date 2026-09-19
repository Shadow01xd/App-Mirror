//! TYU's own Windows 11 software camera. The bundled host publishes a Media Foundation device;
//! its registered TYU COM media source is loaded by the Windows Camera Frame Server.
use openh264::formats::YUVSource;
use std::os::windows::process::CommandExt;
use std::{
    io::{self, BufRead, Write},
    process::{Child, ChildStdin, Command, Stdio},
};
pub struct Camera {
    host: Child,
    input: Option<ChildStdin>,
    decoder: openh264::decoder::Decoder,
}
impl Camera {
    pub fn new() -> io::Result<Self> {
        let bundled = std::env::current_exe()?
            .parent()
            .unwrap()
            .join("TyuCameraHost.exe");
        let installed =
            std::path::PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_default())
                .join("TYU/Camera/TyuCameraHost.exe");
        let executable = if bundled.exists() { bundled } else { installed };
        if !executable.exists() {
            return Err(io::Error::other(
                "Instala el componente TYU Camera con tools/install-tyu-camera.ps1 (administrador)",
            ));
        }
        let mut host = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()?;
        let output = host.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = io::BufReader::new(output).read_line(&mut line);
            let _ = tx.send(line);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(15)) {
            Ok(line) if line.trim() == "READY" => {}
            response => {
                let _ = host.kill();
                let _ = host.wait();
                return Err(io::Error::other(format!(
                    "No se pudo activar TYU Camera. Comprueba su instalación y el permiso de cámara de Windows: {response:?}"
                )));
            }
        }
        let input = host.stdin.take();
        let decoder = match openh264::decoder::Decoder::new() {
            Ok(d) => d,
            Err(e) => {
                let _ = host.kill();
                let _ = host.wait();
                return Err(io::Error::other(e));
            }
        };
        Ok(Self {
            host,
            input,
            decoder,
        })
    }
    pub fn push(&mut self, data: &[u8]) -> io::Result<bool> {
        let Some(picture) = self.decoder.decode(data).map_err(io::Error::other)? else {
            return Ok(false);
        };
        if picture.dimensions() != (1280, 720) {
            return Err(io::Error::other("TYU Camera requiere video 1280 x 720"));
        }
        let mut frame = vec![0; 1280 * 720 * 3 / 2];
        let (ys, us, vs) = picture.strides();
        for row in 0..720 {
            frame[row * 1280..(row + 1) * 1280]
                .copy_from_slice(&picture.y()[row * ys..row * ys + 1280]);
        }
        for row in 0..360 {
            for col in 0..640 {
                let target = 1280 * 720 + row * 1280 + col * 2;
                frame[target] = picture.u()[row * us + col];
                frame[target + 1] = picture.v()[row * vs + col];
            }
        }
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("Cámara detenida"))?;
        input.write_all(&(frame.len() as u32).to_le_bytes())?;
        input.write_all(&frame)?;
        Ok(true)
    }
}
impl Drop for Camera {
    fn drop(&mut self) {
        self.input.take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if self.host.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = self.host.kill();
        let _ = self.host.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_VERSION, MFCreateAttributes,
        MFEnumDeviceSources, MFSTARTUP_FULL, MFStartup,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};

    /// Names of every video capture device Windows apps can pick.
    fn cameras() -> Vec<String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 1).unwrap();
            let attributes = attributes.unwrap();
            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .unwrap();
            let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attributes, &mut list, &mut count).unwrap();
            let names = (0..count as usize)
                .filter_map(|i| {
                    let device = std::ptr::read(list.add(i))?;
                    let mut name = windows::core::PWSTR::null();
                    let mut length = 0u32;
                    device
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                            &mut name,
                            &mut length,
                        )
                        .ok()?;
                    let text = name.to_string().ok();
                    CoTaskMemFree(Some(name.as_ptr().cast()));
                    text
                })
                .collect();
            CoTaskMemFree(Some(list as *const _));
            names
        }
    }

    /// With the component installed, activating Bypass camera publishes "TYU Camera" to every
    /// Windows app; skipped where the component is not installed.
    #[test]
    fn tyu_camera_is_listed_as_a_windows_camera_while_active() {
        let camera = match Camera::new() {
            Ok(camera) => camera,
            Err(error) => {
                eprintln!("TYU Camera not installed here: {error}; skipping");
                return;
            }
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let names = cameras();
            if names.iter().any(|n| n.starts_with("TYU Camera")) {
                eprintln!("cameras: {names:?}");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "TYU Camera missing from {names:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        drop(camera);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while cameras().iter().any(|n| n.starts_with("TYU Camera")) {
            assert!(
                std::time::Instant::now() < deadline,
                "TYU Camera still listed after stop"
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
}
