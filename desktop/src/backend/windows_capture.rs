//! Screen capture + H.264 encode (GPU via Media Foundation, OpenH264 as fallback), feeding an
//! already-negotiated Monitor stream (PC -> phone). Runs on its own thread; capture and encoder
//! calls are blocking and must not run on the Tokio backend runtime.
//!
//! Frames come from DXGI Desktop Duplication (works for the Parsec virtual monitor and includes
//! every composed window); GDI BitBlt is the fallback. The mouse cursor is drawn on top with GDI
//! since neither source composes it.
use super::hw_encode::HwEncoder;
use super::virtual_display::Region;
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod,
        UsageType,
    },
    formats::{BgraSliceU8, YUVBuffer},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc::Sender;
use tyu_core::*;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIFactory1,
    IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, GetDIBits, HBITMAP,
    HDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, GetCursorInfo, GetIconInfo,
    GetSystemMetrics, ICONINFO, SM_CXSCREEN, SM_CYSCREEN,
};
use windows::core::Interface;

/// Real primary-display size.
pub fn screen_size() -> (u32, u32) {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN).max(2) as u32 & !1;
        let height = GetSystemMetrics(SM_CYSCREEN).max(2) as u32 & !1;
        (width, height)
    }
}

fn primary_region() -> Region {
    let (w, h) = screen_size();
    (0, 0, w, h)
}

/// Size actually put on the wire for a capture region (the primary display when `None`):
/// very wide desktops are halved so a keyframe stays a few hundred packets and the phone decodes
/// comfortably. This is the size announced in MediaStart.
pub fn transmit_size(region: Option<Region>) -> (u32, u32) {
    let (w, h) = region.map_or_else(screen_size, |(_, _, w, h)| (w & !1, h & !1));
    if w > 2000 {
        ((w / 2) & !1, (h / 2) & !1)
    } else {
        (w, h)
    }
}

/// 2x2 box downscale of a BGRA8 image.
fn halve(bgra: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let (nw, nh) = ((w / 2) & !1, (h / 2) & !1);
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        let (r0, r1) = (2 * y * w * 4, (2 * y + 1) * w * 4);
        for x in 0..nw {
            let (c0, c1) = (r0 + 8 * x, r1 + 8 * x);
            for k in 0..4 {
                let sum = bgra[c0 + k] as u32
                    + bgra[c0 + 4 + k] as u32
                    + bgra[c1 + k] as u32
                    + bgra[c1 + 4 + k] as u32;
                out.push((sum / 4) as u8);
            }
        }
    }
    (out, nw, nh)
}

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    keyframe: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl CaptureHandle {
    /// The receiver lost a frame: the next encoded frame becomes an IDR.
    pub fn request_keyframe(&self) {
        self.keyframe.store(true, Ordering::Relaxed);
    }
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// GDI BitBlt of one monitor rectangle. Fallback when Desktop Duplication is unavailable.
struct GdiCapture {
    screen: HDC,
    mem: HDC,
    bitmap: HBITMAP,
    region: Region,
    buffer: Vec<u8>,
}
impl GdiCapture {
    fn open(region: Region) -> Option<Self> {
        unsafe {
            let (_, _, width, height) = region;
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let mem = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, width as i32, height as i32);
            SelectObject(mem, bitmap.into());
            Some(Self {
                screen,
                mem,
                bitmap,
                region,
                buffer: vec![0u8; (width * height * 4) as usize],
            })
        }
    }
    fn frame(&mut self) -> Option<(&[u8], bool)> {
        let (x, y, width, height) = self.region;
        unsafe {
            BitBlt(
                self.mem,
                0,
                0,
                width as i32,
                height as i32,
                Some(self.screen),
                x,
                y,
                SRCCOPY,
            )
            .ok()?;
            let mut info = bgra_header(width, height);
            let copied = GetDIBits(
                self.mem,
                self.bitmap,
                0,
                height,
                Some(self.buffer.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            (copied > 0).then_some((self.buffer.as_slice(), true))
        }
    }
}
impl Drop for GdiCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.mem);
            ReleaseDC(None, self.screen);
        }
    }
}

/// DXGI Desktop Duplication of the output whose desktop rectangle matches `region`.
struct DxgiCapture {
    context: ID3D11DeviceContext,
    device: ID3D11Device,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    width: u32,
    height: u32,
    buffer: Vec<u8>,
    has_frame: bool,
}
impl DxgiCapture {
    fn open(region: Region) -> Option<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let mut adapter_index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
                adapter_index += 1;
                let mut output_index = 0;
                while let Ok(output) = adapter.EnumOutputs(output_index) {
                    output_index += 1;
                    let Ok(desc) = output.GetDesc() else {
                        continue;
                    };
                    let r = desc.DesktopCoordinates;
                    let found = (
                        r.left,
                        r.top,
                        (r.right - r.left) as u32,
                        (r.bottom - r.top) as u32,
                    );
                    if found != region {
                        continue;
                    }
                    let (mut device, mut context) = (None, None);
                    if D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_FLAG(0),
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )
                    .is_err()
                    {
                        continue;
                    }
                    let (Some(device), Some(context)) = (device, context) else {
                        continue;
                    };
                    let Ok(output1) = output.cast::<IDXGIOutput1>() else {
                        continue;
                    };
                    let Ok(duplication) = output1.DuplicateOutput(&device) else {
                        continue;
                    };
                    let (width, height) = (region.2 & !1, region.3 & !1);
                    return Some(Self {
                        context,
                        device,
                        duplication,
                        staging: None,
                        width,
                        height,
                        buffer: vec![0u8; (width * height * 4) as usize],
                        has_frame: false,
                    });
                }
            }
            None
        }
    }
    /// Newest desktop image; the previous one when nothing changed within 16 ms. `None` means the
    /// duplication was lost (mode change, UAC desktop) and must be recreated.
    /// `(pixels, fresh)`: `fresh` is false when the desktop did not change within 8 ms and the
    /// previous picture is returned. `None` means the duplication was lost (mode change, UAC
    /// desktop) and must be recreated.
    fn frame(&mut self) -> Option<(&[u8], bool)> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            match self
                .duplication
                .AcquireNextFrame(8, &mut info, &mut resource)
            {
                Ok(()) => {}
                Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                    return self.has_frame.then_some((self.buffer.as_slice(), false));
                }
                Err(_) => return None,
            }
            let copied = resource
                .and_then(|r| r.cast::<ID3D11Texture2D>().ok())
                .and_then(|texture| self.copy_out(&texture));
            let _ = self.duplication.ReleaseFrame();
            if copied.is_none() {
                return self.has_frame.then_some((self.buffer.as_slice(), false));
            }
            self.has_frame = true;
            Some((self.buffer.as_slice(), true))
        }
    }
    unsafe fn copy_out(&mut self, texture: &ID3D11Texture2D) -> Option<()> {
        unsafe {
            if self.staging.is_none() {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                texture.GetDesc(&mut desc);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.BindFlags = 0;
                desc.MiscFlags = 0;
                let mut staging = None;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut staging))
                    .ok()?;
                self.staging = staging;
            }
            let staging = self.staging.as_ref()?;
            self.context.CopyResource(staging, texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;
            let row = (self.width * 4) as usize;
            let pitch = mapped.RowPitch as usize;
            let src = mapped.pData as *const u8;
            for y in 0..self.height as usize {
                std::ptr::copy_nonoverlapping(
                    src.add(y * pitch),
                    self.buffer.as_mut_ptr().add(y * row),
                    row.min(pitch),
                );
            }
            self.context.Unmap(staging, 0);
            Some(())
        }
    }
}

enum Grabber {
    Dxgi(DxgiCapture),
    Gdi(GdiCapture),
}

fn bgra_header(width: u32, height: u32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // negative = top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Draws the current mouse cursor into a BGRA frame of the given monitor.
/// Draws the mouse cursor into BGRA frames through one persistent DIB section (a fresh DIB per
/// frame costs two 8 MB allocations at 1080p60).
struct CursorOverlay {
    dc: HDC,
    dib: HBITMAP,
    bits: *mut u8,
    width: u32,
    height: u32,
}
impl CursorOverlay {
    fn new(width: u32, height: u32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let info = bgra_header(width, height);
            let mut bits = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(dc);
                return None;
            };
            SelectObject(dc, dib.into());
            Some(Self {
                dc,
                dib,
                bits: bits as *mut u8,
                width,
                height,
            })
        }
    }
    fn draw(&self, bgra: &mut [u8], origin: (i32, i32)) {
        unsafe {
            let mut cursor = CURSORINFO {
                cbSize: size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut cursor).is_err() || cursor.flags.0 & CURSOR_SHOWING.0 == 0 {
                return;
            }
            let mut icon = ICONINFO::default();
            if GetIconInfo(cursor.hCursor.into(), &mut icon).is_err() {
                return;
            }
            let _ = DeleteObject(icon.hbmMask.into());
            let _ = DeleteObject(icon.hbmColor.into());
            let x = cursor.ptScreenPos.x - origin.0 - icon.xHotspot as i32;
            let y = cursor.ptScreenPos.y - origin.1 - icon.yHotspot as i32;
            if x < -64 || y < -64 || x > self.width as i32 || y > self.height as i32 {
                return;
            }
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), self.bits, bgra.len());
            if DrawIconEx(
                self.dc,
                x,
                y,
                cursor.hCursor.into(),
                0,
                0,
                0,
                None,
                DI_NORMAL,
            )
            .is_ok()
            {
                let _ = GdiFlush();
                std::ptr::copy_nonoverlapping(self.bits, bgra.as_mut_ptr(), bgra.len());
            }
        }
    }
}
impl Drop for CursorOverlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
// Only the capture thread that created it touches the DIB.
unsafe impl Send for CursorOverlay {}

pub fn start(
    media: Sender<(DeviceId, media::MediaPacket)>,
    peer: DeviceId,
    session: SessionId,
    stream: StreamId,
    region: Option<Region>,
    fps: u32,
    bitrate: u32,
) -> CaptureHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let keyframe = Arc::new(AtomicBool::new(false));
    let (stop_flag, keyframe_flag) = (stop.clone(), keyframe.clone());
    let thread = std::thread::Builder::new()
        .name("tyu-monitor-capture".into())
        .spawn(move || {
            capture_loop(
                media,
                peer,
                session,
                stream,
                region.unwrap_or_else(primary_region),
                fps,
                bitrate,
                &stop_flag,
                &keyframe_flag,
            )
        })
        .ok();
    CaptureHandle {
        stop,
        keyframe,
        thread,
    }
}

fn open_grabber(region: Region) -> Option<Grabber> {
    if let Some(dxgi) = DxgiCapture::open(region) {
        return Some(Grabber::Dxgi(dxgi));
    }
    tracing::warn!("Desktop Duplication unavailable; using GDI capture");
    GdiCapture::open(region).map(Grabber::Gdi)
}

#[allow(clippy::too_many_arguments)]
fn capture_loop(
    media: Sender<(DeviceId, media::MediaPacket)>,
    peer: DeviceId,
    session: SessionId,
    stream: StreamId,
    region: Region,
    fps: u32,
    bitrate: u32,
    stop: &AtomicBool,
    keyframe_wanted: &AtomicBool,
) {
    let region = (region.0, region.1, region.2 & !1, region.3 & !1);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let Some(mut grabber) = open_grabber(region) else {
        tracing::warn!("screen capture unavailable");
        return;
    };
    let (width, height) = (region.2 as usize, region.3 as usize);
    let (tw, th) = transmit_size(Some(region));
    let mut headers: Vec<u8> = Vec::new();
    let mut codec = match HwEncoder::new(tw, th, fps, bitrate) {
        Some(hw) => {
            tracing::info!(encoder = hw.name(), "Monitor uses the GPU H.264 encoder");
            headers = hw.sequence_header();
            if !nal_types(&headers).contains(&7) {
                headers.clear(); // not Annex-B; the first IDR carries SPS/PPS inline anyway
            }
            Codec::Hw(hw)
        }
        None => {
            tracing::warn!("no hardware H.264 encoder; Monitor falls back to OpenH264");
            match Codec::soft(fps, bitrate) {
                Some(soft) => soft,
                None => {
                    tracing::warn!("H.264 encoder unavailable");
                    return;
                }
            }
        }
    };
    let frame_interval = std::time::Duration::from_secs_f64(1.0 / fps.max(1) as f64);
    let started = std::time::Instant::now();
    let mut frame_id: u64 = 0;
    let mut scratch = vec![0u8; width * height * 4];
    let cursor = CursorOverlay::new(width as u32, height as u32);
    let mut last_sent = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let tick = std::time::Instant::now();
        let captured = match &mut grabber {
            Grabber::Dxgi(dxgi) => match dxgi.frame() {
                Some(frame) => Some(frame),
                None => {
                    // Duplication lost (display change / secure desktop): reopen next tick.
                    if let Some(next) = open_grabber(region) {
                        grabber = next;
                    }
                    None
                }
            },
            Grabber::Gdi(gdi) => gdi.frame(),
        };
        // A static desktop is not re-encoded (saves CPU and bandwidth); one frame per second
        // still goes out so the receiver keeps getting keyframes.
        let wanted = keyframe_wanted.load(Ordering::Relaxed);
        if let Some((frame, fresh)) = captured
            && (fresh || wanted || last_sent.elapsed() >= std::time::Duration::from_secs(1))
        {
            last_sent = std::time::Instant::now();
            scratch.copy_from_slice(frame);
            if let Some(cursor) = &cursor {
                cursor.draw(&mut scratch, (region.0, region.1));
            }
            let (bgra, w, h) = if width > 2000 {
                halve(&scratch, width, height)
            } else {
                (std::mem::take(&mut scratch), width, height)
            };
            let force = keyframe_wanted.swap(false, Ordering::Relaxed);
            let encoded = codec.encode(&bgra, w, h, force);
            if width <= 2000 {
                scratch = bgra;
            }
            if encoded.is_none()
                && let Codec::Hw(_) = codec
            {
                // GPU encoder died mid-session (driver reset): finish on the CPU.
                tracing::warn!("hardware encoder failed; switching Monitor to OpenH264");
                match Codec::soft(fps, bitrate) {
                    Some(soft) => codec = soft,
                    None => return,
                }
                keyframe_wanted.store(true, Ordering::Relaxed);
            }
            if let Some((mut bytes, keyframe)) = encoded {
                ensure_headers(&mut headers, &mut bytes, keyframe);
                if !bytes.is_empty()
                    && let Ok(packets) = media::packetize(
                        session,
                        stream,
                        frame_id,
                        started.elapsed().as_micros() as u64,
                        keyframe,
                        bytes.into(),
                        1200,
                    )
                {
                    frame_id += 1;
                    // A keyframe is hundreds of packets; block for capacity instead of dropping
                    // (dropping any packet discards the whole frame at the receiver).
                    for packet in packets {
                        if media.blocking_send((peer, packet)).is_err() {
                            return;
                        }
                    }
                }
            }
        }
        let elapsed = tick.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }
}

#[allow(clippy::large_enum_variant)] // one instance per session; boxing buys nothing
enum Codec {
    Hw(HwEncoder),
    Soft(Encoder),
}
impl Codec {
    fn soft(fps: u32, bitrate: u32) -> Option<Self> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .usage_type(UsageType::ScreenContentRealTime)
            // Software 1080p only keeps up with slice threading and the fastest preset.
            .num_threads(4)
            .complexity(Complexity::Low)
            // OpenH264 does not support these for screen content and logs a warning otherwise.
            .adaptive_quantization(false)
            .background_detection(false)
            // A keyframe every second, so a receiver that joins late or loses packets recovers.
            .intra_frame_period(IntraFramePeriod::from_num_frames(fps.max(1)));
        Encoder::with_api_config(OpenH264API::from_source(), config)
            .ok()
            .map(Self::Soft)
    }
    /// One BGRA frame -> (Annex-B access unit, is keyframe). `Some(empty)` when the encoder is
    /// still working on it, `None` when it failed.
    fn encode(&mut self, bgra: &[u8], w: usize, h: usize, force: bool) -> Option<(Vec<u8>, bool)> {
        match self {
            Self::Hw(hw) => {
                let mut bytes = hw.encode(bgra, force)?;
                let types = nal_types(&bytes);
                if types.contains(&12) {
                    // GPU encoders pad to the bitrate with filler NALs; the network doesn't need them.
                    bytes = split_nals(&bytes)
                        .into_iter()
                        .filter(|u| nal_type(u) != Some(12))
                        .flatten()
                        .copied()
                        .collect();
                }
                Some((bytes, types.contains(&5)))
            }
            Self::Soft(encoder) => {
                let yuv = YUVBuffer::from_rgb8_source(BgraSliceU8::new(bgra, (w, h)));
                if force {
                    encoder.force_intra_frame();
                }
                let encoded = encoder.encode(&yuv).ok()?;
                let keyframe = matches!(encoded.frame_type(), FrameType::IDR | FrameType::I);
                let mut bytes = Vec::new();
                encoded.write_vec(&mut bytes);
                Some((bytes, keyframe))
            }
        }
    }
}

/// NAL unit types present in an Annex-B stream.
pub fn nal_types(stream: &[u8]) -> Vec<u8> {
    split_nals(stream)
        .iter()
        .filter_map(|u| nal_type(u))
        .collect()
}

/// H.264 NAL unit type of one Annex-B unit, or `None` if it has no start code.
fn nal_type(unit: &[u8]) -> Option<u8> {
    let start = unit.windows(3).position(|w| w == [0, 0, 1])?;
    unit.get(start + 3).map(|b| b & 0x1F)
}

/// Splits an Annex-B stream into NAL units, each keeping its own 3- or 4-byte start code.
fn split_nals(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts: Vec<usize> = stream
        .windows(3)
        .enumerate()
        .filter(|(_, w)| *w == [0, 0, 1])
        .map(|(i, _)| {
            if i > 0 && stream[i - 1] == 0 {
                i - 1
            } else {
                i
            }
        })
        .collect();
    starts.dedup();
    starts
        .iter()
        .enumerate()
        .map(|(k, &s)| &stream[s..starts.get(k + 1).copied().unwrap_or(stream.len())])
        .collect()
}

/// Every keyframe must be self-contained: cache SPS (7) / PPS (8) as they appear and prepend
/// them to any keyframe the encoder emitted without headers.
pub fn ensure_headers(headers: &mut Vec<u8>, frame: &mut Vec<u8>, keyframe: bool) {
    let units = split_nals(frame);
    let has_sps = units.iter().any(|u| nal_type(u) == Some(7));
    if has_sps {
        headers.clear();
        for unit in units.iter().filter(|u| matches!(nal_type(u), Some(7 | 8))) {
            headers.extend_from_slice(unit);
        }
    } else if keyframe && !headers.is_empty() {
        let mut with_headers = headers.clone();
        with_headers.append(frame);
        *frame = with_headers;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn keyframes_without_headers_get_cached_sps_pps_prepended() {
        let sps = [0, 0, 0, 1, 0x67, 1, 2];
        let pps = [0, 0, 0, 1, 0x68, 3];
        let idr = [0, 0, 0, 1, 0x65, 9, 9];
        let mut headers = Vec::new();
        let mut first = [sps.as_slice(), &pps, &idr].concat();
        ensure_headers(&mut headers, &mut first, true);
        assert_eq!(headers, [sps.as_slice(), &pps].concat());
        let mut later = idr.to_vec();
        ensure_headers(&mut headers, &mut later, true);
        assert_eq!(later, [sps.as_slice(), &pps, &idr].concat());
        let mut delta = vec![0, 0, 0, 1, 0x41, 5];
        ensure_headers(&mut headers, &mut delta, false);
        assert_eq!(delta, [0, 0, 0, 1, 0x41, 5]);
    }

    /// Tests that duplicate or reconfigure the real display cannot overlap.
    pub(crate) static DISPLAY: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Prints the per-stage cost of the Monitor pipeline at 1080p on this machine (run with
    /// `--nocapture`); the budget for 60 fps is 16 ms per frame.
    #[test]
    fn monitor_pipeline_cost_at_1080p() {
        let _display = DISPLAY.lock().unwrap_or_else(|e| e.into_inner());
        let (w, h) = screen_size();
        if w < 1920 || h < 1080 {
            return;
        }
        let Some(mut dxgi) = DxgiCapture::open(primary_region()) else {
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let full = loop {
            if let Some((frame, _)) = dxgi.frame()
                && frame.iter().any(|&b| b != 0)
            {
                break frame.to_vec();
            }
            assert!(std::time::Instant::now() < deadline);
        };
        // Top-left 1920x1080 of the real desktop as a realistic screen-content sample.
        let (cw, ch) = (1920usize, 1080usize);
        let mut bgra = vec![0u8; cw * ch * 4];
        for row in 0..ch {
            bgra[row * cw * 4..(row + 1) * cw * 4]
                .copy_from_slice(&full[row * w as usize * 4..row * w as usize * 4 + cw * 4]);
        }
        let cursor = CursorOverlay::new(cw as u32, ch as u32).unwrap();
        let t = std::time::Instant::now();
        for _ in 0..10 {
            cursor.draw(&mut bgra, (0, 0));
        }
        let cursor_ms = t.elapsed().as_secs_f64() * 100.0;
        let t = std::time::Instant::now();
        let mut yuv = None;
        for _ in 0..10 {
            yuv = Some(YUVBuffer::from_rgb8_source(BgraSliceU8::new(
                &bgra,
                (cw, ch),
            )));
        }
        let convert_ms = t.elapsed().as_secs_f64() * 100.0;
        let yuv = yuv.unwrap();
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(8_000_000))
            .max_frame_rate(FrameRate::from_hz(60.0))
            .usage_type(UsageType::ScreenContentRealTime)
            .num_threads(4)
            .complexity(Complexity::Low)
            .adaptive_quantization(false)
            .background_detection(false);
        let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config).unwrap();
        let t = std::time::Instant::now();
        let mut bytes = 0usize;
        for _ in 0..30 {
            bytes += encoder.encode(&yuv).unwrap().to_vec().len();
        }
        let encode_ms = t.elapsed().as_secs_f64() * 1000.0 / 30.0;
        eprintln!(
            "1080p per frame: cursor {cursor_ms:.1} ms, bgra->yuv {convert_ms:.1} ms, encode {encode_ms:.1} ms ({} kB avg)",
            bytes / 30 / 1000
        );
    }

    /// Desktop Duplication of the primary display must produce a real, non-empty picture.
    #[test]
    fn desktop_duplication_grabs_the_primary_display() {
        let _display = DISPLAY.lock().unwrap_or_else(|e| e.into_inner());
        let (w, h) = screen_size();
        if w < 16 {
            return;
        }
        let Some(mut dxgi) = DxgiCapture::open(primary_region()) else {
            eprintln!("Desktop Duplication unavailable here; skipping");
            return;
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some((frame, _fresh)) = dxgi.frame() {
                assert_eq!(frame.len(), (w * h * 4) as usize);
                if frame.iter().any(|&b| b != 0) {
                    break;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no desktop frame arrived"
            );
        }
    }
}
