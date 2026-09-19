//! Plays the phone's PCM through a WASAPI render endpoint; no kernel driver.
//!
//! To show up as a *microphone* for other apps the audio has to go into a virtual cable whose
//! capture side is the mic: VB-CABLE ("CABLE Input" → "CABLE Output") or Steam's built-in one
//! ("Speakers (Steam Streaming Microphone)" → "Microphone (Steam Streaming Microphone)"). Without
//! a cable the phone is simply heard on the default output, which still proves the link works.
use std::{io, time::Duration};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, IAudioClient, IAudioRenderClient,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX, eConsole,
    eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};

const RATE: u32 = 48_000;
const BUFFER_HNS: i64 = 2_000_000; // 200 ms of room; the phone sends 20 ms chunks

/// Render endpoints that loop into a capture endpoint, best first: (render name, mic name).
const CABLES: [(&str, &str); 3] = [
    ("CABLE Input", "CABLE Output (VB-Audio Virtual Cable)"),
    (
        "VoiceMeeter Input",
        "VoiceMeeter Output (VB-Audio VoiceMeeter VAIO)",
    ),
    (
        "Steam Streaming Microphone",
        "Microphone (Steam Streaming Microphone)",
    ),
];

/// Whether some virtual cable is installed, i.e. whether the phone can act as a real microphone
/// instead of just being heard through the speakers.
pub fn cable_installed() -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return false;
        };
        matches!(pick(&enumerator), Ok((_, _, Some(_))))
    }
}

pub struct Microphone {
    client: IAudioClient,
    render: IAudioRenderClient,
    frames: u32,
    endpoint: String,
    /// The capture endpoint apps should pick, when the chosen output is a virtual cable.
    mic: Option<&'static str>,
}
unsafe impl Send for Microphone {}

impl Microphone {
    pub fn new() -> io::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(other)?;
            let (device, endpoint, mic) = pick(&enumerator)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(other)?;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 1,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            // Shared mode + auto-convert: the engine resamples/mixes to whatever the device runs.
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    BUFFER_HNS,
                    0,
                    &format,
                    None,
                )
                .map_err(other)?;
            let frames = client.GetBufferSize().map_err(other)?;
            let render: IAudioRenderClient = client.GetService().map_err(other)?;
            client.Start().map_err(other)?;
            Ok(Self {
                client,
                render,
                frames,
                endpoint,
                mic,
            })
        }
    }

    /// What the UI tells the user once audio flows.
    pub fn label(&self) -> String {
        match self.mic {
            Some(mic) => format!("Micrófono listo: en tus apps elige «{mic}»"),
            None => format!(
                "El teléfono se oye por «{}». Para usarlo como micrófono instala VB-CABLE (gratis) y TYU lo usará solo",
                self.endpoint
            ),
        }
    }

    /// Queues 16-bit mono 48 kHz PCM; blocks briefly while the device drains a full buffer.
    pub fn push(&self, pcm: &[u8]) -> io::Result<()> {
        if pcm.is_empty() || pcm.len() > 9600 || !pcm.len().is_multiple_of(2) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Formato PCM de micrófono inválido",
            ));
        }
        let wanted = (pcm.len() / 2) as u32;
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        unsafe {
            loop {
                let padding = self.client.GetCurrentPadding().map_err(other)?;
                if self.frames - padding >= wanted {
                    let buffer = self.render.GetBuffer(wanted).map_err(other)?;
                    std::ptr::copy_nonoverlapping(pcm.as_ptr(), buffer, pcm.len());
                    self.render.ReleaseBuffer(wanted, 0).map_err(other)?;
                    return Ok(());
                }
                if std::time::Instant::now() > deadline {
                    return Err(other("El dispositivo de audio no consume el sonido"));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
impl Drop for Microphone {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

fn other<E: std::fmt::Display>(error: E) -> io::Error {
    io::Error::other(error.to_string())
}

/// A virtual cable's render side when one is installed, else the default output device.
unsafe fn pick(
    enumerator: &IMMDeviceEnumerator,
) -> io::Result<(IMMDevice, String, Option<&'static str>)> {
    unsafe {
        let devices = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(other)?;
        let count = devices.GetCount().map_err(other)?;
        let named: Vec<(IMMDevice, String)> = (0..count)
            .filter_map(|i| {
                let device = devices.Item(i).ok()?;
                let name = friendly_name(&device)?;
                Some((device, name))
            })
            .collect();
        for (needle, mic) in CABLES {
            if let Some((device, name)) = named.iter().find(|(_, n)| n.contains(needle)) {
                return Ok((device.clone(), name.clone(), Some(mic)));
            }
        }
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|_| other("No hay ningún dispositivo de salida de audio"))?;
        let name = friendly_name(&device).unwrap_or_else(|| "Salida predeterminada".into());
        Ok((device, name, None))
    }
}

unsafe fn friendly_name(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let text = PropVariantToStringAlloc(&value).ok().map(|p| {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.as_ptr().cast()));
            s
        });
        let _ = PropVariantClear(&mut value);
        text.filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Opens the endpoint this machine would use and streams a second of silence through it
    /// (skipped on machines without an audio output).
    #[test]
    fn phone_pcm_reaches_an_audio_endpoint() {
        let Ok(microphone) = Microphone::new() else {
            eprintln!("no audio output; skipping");
            return;
        };
        eprintln!("endpoint: {} → {}", microphone.endpoint, microphone.label());
        let chunk = vec![0u8; 1920]; // 20 ms
        for _ in 0..50 {
            microphone.push(&chunk).expect("audio engine accepts PCM");
        }
        let padding = unsafe { microphone.client.GetCurrentPadding().unwrap() };
        assert!(padding > 0, "nothing queued in the audio engine");
        assert!(microphone.push(&[1, 2, 3]).is_err());
    }
}
