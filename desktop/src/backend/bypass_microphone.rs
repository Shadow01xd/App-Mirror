//! Writes phone PCM directly to the TYU virtual capture driver; no playback device or external cable.
use std::{io, mem::size_of};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE},
        Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING},
        System::IO::DeviceIoControl,
    },
    core::PCWSTR,
};
const INFO: u32 = 0x8000e000;
const WRITE: u32 = 0x8000e004;
const RESET: u32 = 0x8000e008;
#[repr(C)]
#[derive(Default)]
struct Info {
    version: u32,
    rate: u32,
    channels: u32,
    bits: u32,
    ready: u32,
    reserved: u32,
}
pub struct Microphone {
    handle: HANDLE,
}
impl Microphone {
    pub fn new() -> io::Result<Self> {
        let path: Vec<u16> = r"\\.\TYUMicrophone".encode_utf16().chain(Some(0)).collect();
        let handle=unsafe {CreateFileW(PCWSTR(path.as_ptr()),GENERIC_READ.0|GENERIC_WRITE.0,FILE_SHARE_MODE(0),None,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,None)}
            .map_err(|e|io::Error::other(format!("No se pudo abrir TYU Microphone. Instala el controlador firmado de TYU o cierra la otra sesión que lo esté usando: {e}")))?;
        let result = Self { handle };
        let mut info = Info::default();
        let mut returned = 0;
        unsafe {
            DeviceIoControl(
                handle,
                INFO,
                None,
                0,
                Some((&mut info as *mut Info).cast()),
                size_of::<Info>() as u32,
                Some(&mut returned),
                None,
            )
        }
        .map_err(io::Error::other)?;
        if returned != size_of::<Info>() as u32
            || info.version != 1
            || info.rate != 48000
            || info.channels != 1
            || info.bits != 16
            || info.ready != 1
        {
            return Err(io::Error::other(
                "TYU Microphone no está listo o su controlador necesita actualizarse",
            ));
        }
        result.reset()?;
        Ok(result)
    }
    pub fn push(&self, pcm: &[u8]) -> io::Result<()> {
        if pcm.is_empty() || pcm.len() > 9600 || !pcm.len().is_multiple_of(2) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Formato PCM de micrófono inválido",
            ));
        }
        let mut returned = 0;
        unsafe {
            DeviceIoControl(
                self.handle,
                WRITE,
                Some(pcm.as_ptr().cast()),
                pcm.len() as u32,
                None,
                0,
                Some(&mut returned),
                None,
            )
        }
        .map_err(io::Error::other)
    }
    fn reset(&self) -> io::Result<()> {
        let mut returned = 0;
        unsafe {
            DeviceIoControl(
                self.handle,
                RESET,
                None,
                0,
                None,
                0,
                Some(&mut returned),
                None,
            )
        }
        .map_err(io::Error::other)
    }
}
impl Drop for Microphone {
    fn drop(&mut self) {
        let _ = self.reset();
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
