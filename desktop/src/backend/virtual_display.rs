//! Extended-desktop Monitor mode through the Parsec Virtual Display Driver (a signed IddCx
//! driver the user installs). TYU plugs a virtual monitor while a Monitor session runs, so
//! Windows sees a real second display; the capture thread then streams just that monitor.
//! Protocol reference: https://github.com/nomi-san/parsec-vdd (core/parsec-vdd.h).
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_NO_BUFFERING, FILE_FLAG_OVERLAPPED,
    FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::{DeviceIoControl, GetOverlappedResultEx, OVERLAPPED};
use windows::Win32::System::Threading::CreateEventW;
use windows::core::{BOOL, GUID, PCWSTR};

const VDD_ADAPTER_GUID: GUID = GUID::from_u128(0x00b41627_04c4_429e_a26e_0265cf50c8fa);
const VDD_IOCTL_ADD: u32 = 0x0022e004;
const VDD_IOCTL_REMOVE: u32 = 0x0022a008;
const VDD_IOCTL_UPDATE: u32 = 0x0022a00c;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

/// Screen-space rectangle of a monitor: (x, y, width, height).
pub type Region = (i32, i32, u32, u32);

pub struct VirtualDisplay {
    handle: HANDLE,
    index: i32,
    stop: Arc<AtomicBool>,
    keepalive: Option<std::thread::JoinHandle<()>>,
    pub region: Region,
}
// The raw device handle is only used from this struct's own methods and its keep-alive thread.
unsafe impl Send for VirtualDisplay {}

impl VirtualDisplay {
    /// Plugs a virtual monitor and waits for Windows to bring it up. `None` when the driver is
    /// not installed or the monitor never appeared (the display is unplugged again in that case).
    pub fn create() -> Option<Self> {
        let handle = open_device()?;
        let before = monitors();
        let index = ioctl(handle, VDD_IOCTL_ADD, &[]);
        if index < 0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return None;
        }
        ioctl(handle, VDD_IOCTL_UPDATE, &[]);
        let stop = Arc::new(AtomicBool::new(false));
        let keepalive = {
            let stop = stop.clone();
            let raw = handle.0 as usize;
            std::thread::Builder::new()
                .name("tyu-vdd-keepalive".into())
                .spawn(move || {
                    // The driver unplugs its displays unless pinged at least every 100 ms.
                    while !stop.load(Ordering::Relaxed) {
                        ioctl(HANDLE(raw as *mut _), VDD_IOCTL_UPDATE, &[]);
                        std::thread::sleep(std::time::Duration::from_millis(60));
                    }
                })
                .ok()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let region = loop {
            if let Some(new) = monitors().into_iter().find(|m| !before.contains(m)) {
                break Some(new);
            }
            if std::time::Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        let mut display = Self {
            handle,
            index,
            stop,
            keepalive,
            region: (0, 0, 0, 0),
        };
        match region {
            Some(region) => {
                display.region = region;
                Some(display)
            }
            None => {
                drop(display);
                None
            }
        }
    }
}
impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.keepalive.take() {
            let _ = thread.join();
        }
        // The driver reads the display index as a 16-bit big-endian value.
        ioctl(
            self.handle,
            VDD_IOCTL_REMOVE,
            &(self.index as u16).to_be_bytes(),
        );
        ioctl(self.handle, VDD_IOCTL_UPDATE, &[]);
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

fn open_device() -> Option<HANDLE> {
    unsafe {
        let set = SetupDiGetClassDevsW(
            Some(&VDD_ADAPTER_GUID),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
        .ok()?;
        let mut found = None;
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        let mut member = 0;
        while SetupDiEnumDeviceInterfaces(set, None, &VDD_ADAPTER_GUID, member, &mut interface)
            .is_ok()
        {
            member += 1;
            let mut required = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(
                set,
                &interface,
                None,
                0,
                Some(&mut required),
                None,
            );
            if required == 0 {
                continue;
            }
            let mut buffer = vec![0u8; required as usize];
            let detail = buffer
                .as_mut_ptr()
                .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &interface,
                Some(detail),
                required,
                Some(&mut required),
                None,
            )
            .is_err()
            {
                continue;
            }
            let path = PCWSTR((*detail).DevicePath.as_ptr());
            if let Ok(handle) = CreateFileW(
                path,
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL
                    | FILE_FLAG_NO_BUFFERING
                    | FILE_FLAG_OVERLAPPED
                    | FILE_FLAG_WRITE_THROUGH,
                None,
            ) {
                found = Some(handle);
                break;
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
        found
    }
}

/// Same shape as the reference `VddIoControl`: 32-byte in buffer, DWORD out, 5 s timeout.
fn ioctl(handle: HANDLE, code: u32, data: &[u8]) -> i32 {
    unsafe {
        let mut input = [0u8; 32];
        let n = data.len().min(32);
        input[..n].copy_from_slice(&data[..n]);
        let mut output = 0u32;
        let Ok(event) = CreateEventW(None, true, false, PCWSTR::null()) else {
            return -1;
        };
        let mut overlapped = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        let _ = DeviceIoControl(
            handle,
            code,
            Some(input.as_ptr().cast()),
            input.len() as u32,
            Some((&mut output as *mut u32).cast()),
            size_of::<u32>() as u32,
            None,
            Some(&mut overlapped),
        );
        let mut transferred = 0u32;
        let result = GetOverlappedResultEx(handle, &overlapped, &mut transferred, 5000, false);
        let _ = CloseHandle(event);
        if result.is_err() {
            return -1;
        }
        output as i32
    }
}

unsafe extern "system" fn collect(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> BOOL {
    unsafe {
        let out = &mut *(data.0 as *mut Vec<Region>);
        let r = *rect;
        out.push((
            r.left,
            r.top,
            (r.right - r.left) as u32,
            (r.bottom - r.top) as u32,
        ));
    }
    BOOL(1)
}

/// All monitors of the virtual desktop.
pub fn monitors() -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut out as *mut Vec<Region> as isize),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Plugs and unplugs a real virtual monitor; skipped when the driver is not installed.
    #[test]
    fn plugging_a_virtual_monitor_adds_a_display_and_dropping_removes_it() {
        let _display = super::super::windows_capture::tests::DISPLAY
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(handle) = open_device() else {
            eprintln!("Parsec VDD not installed; skipping");
            return;
        };
        unsafe {
            let _ = CloseHandle(handle);
        }
        let before = monitors().len();
        let display = VirtualDisplay::create().expect("virtual monitor did not appear");
        assert_eq!(monitors().len(), before + 1);
        assert!(
            display.region.2 >= 640 && display.region.3 >= 480,
            "{:?}",
            display.region
        );
        drop(display);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while monitors().len() != before {
            assert!(
                std::time::Instant::now() < deadline,
                "virtual monitor was not removed"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
