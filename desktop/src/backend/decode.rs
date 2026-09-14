//! Dedicated H.264 decode thread. Incoming frames are decoded off the UI and backend threads;
//! only the newest RGB picture is kept, so a slow consumer drops pictures instead of stalling Core.
//!
//! Decoding uses the Windows Media Foundation H.264 decoder (multithreaded, SIMD, can reach
//! 1080p60) and falls back to OpenH264 when Media Foundation is unavailable.
use openh264::{decoder::Decoder as SoftDecoder, formats::YUVSource};
use std::mem::ManuallyDrop;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
};
use windows::Win32::Media::MediaFoundation::{
    CLSID_MSH264DecoderMFT, IMF2DBuffer, IMFMediaBuffer, IMFSample, IMFTransform,
    MF_E_NOTACCEPTING, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
    MF_LOW_LATENCY, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_SUBTYPE, MF_VERSION, MFCreateMediaType,
    MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video, MFSTARTUP_FULL, MFStartup,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFVideoFormat_H264, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::Interface;

pub type RgbFrame = (u32, u32, Vec<u8>);

enum Job {
    Decode(bytes::Bytes),
    Reset,
}

#[derive(Clone)]
pub struct DecodeHandle {
    jobs: SyncSender<Job>,
    latest: Arc<Mutex<Option<RgbFrame>>>,
    keyframe_needed: Arc<AtomicBool>,
}
impl DecodeHandle {
    /// True once after the decoder skipped ahead and now needs a keyframe from the sender.
    pub fn take_keyframe_request(&self) -> bool {
        self.keyframe_needed.swap(false, Ordering::Relaxed)
    }
    /// Queues one encoded access unit. The queue is deep because dropping an H.264 frame before
    /// decoding breaks every following frame until the next keyframe.
    pub fn push(&self, data: bytes::Bytes) {
        match self.jobs.try_send(Job::Decode(data)) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => tracing::warn!("decode thread gone"),
        }
    }
    /// Forgets stream state so a new session starts clean (waits for its own keyframe).
    pub fn reset(&self) {
        let _ = self.jobs.try_send(Job::Reset);
        if let Ok(mut latest) = self.latest.lock() {
            *latest = None;
        }
    }
    pub fn take_frame(&self) -> Option<RgbFrame> {
        self.latest.lock().ok()?.take()
    }
}

pub fn start() -> DecodeHandle {
    let (jobs, rx) = sync_channel(256);
    let latest = Arc::new(Mutex::new(None));
    let keyframe_needed = Arc::new(AtomicBool::new(false));
    let (slot, flag) = (latest.clone(), keyframe_needed.clone());
    let spawned = std::thread::Builder::new()
        .name("tyu-decode".into())
        .spawn(move || run(rx, slot, flag));
    if let Err(error) = spawned {
        tracing::error!(%error, "decode thread unavailable");
    }
    DecodeHandle {
        jobs,
        latest,
        keyframe_needed,
    }
}

/// Whether an Annex-B access unit starts a decodable point (contains an IDR slice or an SPS).
pub fn is_keyframe(unit: &[u8]) -> bool {
    unit.windows(4)
        .any(|w| w[0] == 0 && w[1] == 0 && w[2] == 1 && matches!(w[3] & 0x1F, 5 | 7))
}

/// Latency control: when more than a few frames are waiting, jump to the newest keyframe (or
/// drop everything and ask for one) instead of playing seconds behind.
fn catch_up(
    first: bytes::Bytes,
    rx: &Receiver<Job>,
    keyframe_needed: &AtomicBool,
) -> Vec<bytes::Bytes> {
    let mut pending = vec![first];
    for job in rx.try_iter() {
        match job {
            Job::Decode(data) => pending.push(data),
            Job::Reset => {
                pending.clear();
                keyframe_needed.store(false, Ordering::Relaxed);
            }
        }
    }
    // ~100 ms of backlog at 60 fps before skipping; lower flaps between skip and catch-up.
    if pending.len() <= 6 {
        return pending;
    }
    match pending.iter().rposition(|unit| is_keyframe(unit)) {
        Some(index) => pending.split_off(index),
        None => {
            keyframe_needed.store(true, Ordering::Relaxed);
            Vec::new()
        }
    }
}

enum Backend {
    Mft(Mft),
    Soft(SoftDecoder),
}
impl Backend {
    fn open() -> Option<Self> {
        if let Some(mft) = Mft::new() {
            return Some(Self::Mft(mft));
        }
        tracing::warn!("Media Foundation H.264 decoder unavailable; using OpenH264");
        SoftDecoder::new().ok().map(Self::Soft)
    }
    /// Decodes one access unit; `Ok(None)` when the decoder needs more data.
    fn decode(&mut self, data: &[u8]) -> Result<Option<RgbFrame>, ()> {
        match self {
            Self::Mft(mft) => mft.decode(data),
            Self::Soft(decoder) => match decoder.decode(data) {
                Ok(Some(yuv)) => {
                    let (w, h) = yuv.dimensions();
                    if w == 0 || h == 0 {
                        return Ok(None);
                    }
                    let mut rgb = vec![0u8; w * h * 3];
                    yuv.write_rgb8(&mut rgb);
                    Ok(Some((w as u32, h as u32, rgb)))
                }
                Ok(None) => Ok(None),
                Err(_) => Err(()),
            },
        }
    }
}

fn run(rx: Receiver<Job>, latest: Arc<Mutex<Option<RgbFrame>>>, keyframe_needed: Arc<AtomicBool>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut backend: Option<Backend> = None;
    let mut errors = 0u32;
    // After skipping ahead, feeding P-frames whose references were dropped paints garbage
    // (flickering, artifacts); hold everything until the requested keyframe arrives.
    let mut waiting_for_keyframe = false;
    while let Ok(job) = rx.recv() {
        let first = match job {
            Job::Reset => {
                backend = None;
                waiting_for_keyframe = false;
                continue;
            }
            Job::Decode(data) => data,
        };
        let batch = catch_up(first, &rx, &keyframe_needed);
        if batch.is_empty() {
            waiting_for_keyframe = true;
        }
        for data in batch {
            if waiting_for_keyframe {
                if !is_keyframe(&data) {
                    continue;
                }
                waiting_for_keyframe = false;
            }
            if backend.is_none() {
                backend = Backend::open();
            }
            let Some(current) = backend.as_mut() else {
                continue;
            };
            match current.decode(&data) {
                Ok(Some(frame)) => {
                    errors = 0;
                    if let Ok(mut slot) = latest.lock() {
                        *slot = Some(frame);
                    }
                    // Wake the UI timer early; harmless when no event loop is running (tests).
                    let _ = slint::invoke_from_event_loop(|| {});
                }
                Ok(None) => {}
                Err(()) => {
                    // Broken reference chain: stop painting until a fresh keyframe, and ask for it.
                    errors += 1;
                    waiting_for_keyframe = true;
                    keyframe_needed.store(true, Ordering::Relaxed);
                    if errors > 120 {
                        backend = None;
                        errors = 0;
                    }
                }
            }
        }
    }
}

/// Windows Media Foundation H.264 decoder MFT producing NV12, converted to RGB8 here.
pub(super) struct Mft {
    transform: IMFTransform,
    /// Coded (macroblock-aligned) size of the NV12 buffer.
    width: u32,
    height: u32,
    /// Visible picture inside the coded buffer: x, y, width, height.
    crop: (u32, u32, u32, u32),
    stride: i32,
    provides_samples: bool,
    output_size: u32,
    time: i64,
}
impl Mft {
    pub(super) fn new() -> Option<Self> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).ok()?;
            let transform: IMFTransform =
                CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER).ok()?;
            // Without low latency the decoder holds up to 16 frames when the SPS carries no
            // reordering bound (OpenH264 and most phone encoders), i.e. it never emits live video.
            if let Ok(attributes) = transform.GetAttributes() {
                let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
            }
            let input = MFCreateMediaType().ok()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).ok()?;
            transform.SetInputType(0, &input, 0).ok()?;
            let mut mft = Self {
                transform,
                width: 0,
                height: 0,
                crop: (0, 0, 0, 0),
                stride: 0,
                provides_samples: false,
                output_size: 0,
                time: 0,
            };
            mft.negotiate_output()?;
            mft.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .ok()?;
            mft.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .ok()?;
            Some(mft)
        }
    }
    unsafe fn negotiate_output(&mut self) -> Option<()> {
        unsafe {
            let mut index = 0;
            loop {
                let candidate = self.transform.GetOutputAvailableType(0, index).ok()?;
                index += 1;
                if candidate.GetGUID(&MF_MT_SUBTYPE).ok()? != MFVideoFormat_NV12 {
                    continue;
                }
                self.transform.SetOutputType(0, &candidate, 0).ok()?;
                let size = candidate.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
                self.width = (size >> 32) as u32;
                self.height = size as u32;
                // The frame size is macroblock-aligned (1080 -> 1088); the aperture is what the
                // phone actually shows. Without it the padding renders as a green stripe.
                self.crop = (0, 0, self.width, self.height);
                let mut area = [0u8; 16];
                if candidate
                    .GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut area, None)
                    .is_ok()
                {
                    let x = i16::from_le_bytes([area[2], area[3]]).max(0) as u32;
                    let y = i16::from_le_bytes([area[6], area[7]]).max(0) as u32;
                    let w =
                        i32::from_le_bytes([area[8], area[9], area[10], area[11]]).max(0) as u32;
                    let h =
                        i32::from_le_bytes([area[12], area[13], area[14], area[15]]).max(0) as u32;
                    if w > 0 && h > 0 && x + w <= self.width && y + h <= self.height {
                        self.crop = (x, y, w, h);
                    }
                }
                self.stride = candidate
                    .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                    .map(|s| s as i32)
                    .unwrap_or(self.width as i32);
                let info = self.transform.GetOutputStreamInfo(0).ok()?;
                self.provides_samples =
                    info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
                self.output_size = info.cbSize.max(self.width * self.height * 3 / 2);
                return Some(());
            }
        }
    }
    pub(super) fn decode(&mut self, data: &[u8]) -> Result<Option<RgbFrame>, ()> {
        self.decode_detailed(data).map_err(|error| {
            tracing::debug!(error, "Media Foundation decode");
        })
    }
    /// Standard synchronous MFT loop: feed input, drain output; when the decoder refuses input
    /// because it holds output, drain first and retry.
    fn decode_detailed(&mut self, data: &[u8]) -> Result<Option<RgbFrame>, String> {
        unsafe {
            let sample = MFCreateSample().map_err(|e| format!("MFCreateSample: {e}"))?;
            let buffer = MFCreateMemoryBuffer(data.len() as u32)
                .map_err(|e| format!("MFCreateMemoryBuffer: {e}"))?;
            let mut ptr = std::ptr::null_mut();
            buffer
                .Lock(&mut ptr, None, None)
                .map_err(|e| format!("Lock: {e}"))?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = buffer.Unlock();
            buffer
                .SetCurrentLength(data.len() as u32)
                .map_err(|e| format!("SetCurrentLength: {e}"))?;
            sample
                .AddBuffer(&buffer)
                .map_err(|e| format!("AddBuffer: {e}"))?;
            let _ = sample.SetSampleTime(self.time);
            let _ = sample.SetSampleDuration(166_667);
            self.time += 166_667; // 60 fps in 100 ns units; only monotonicity matters
            let mut picture = None;
            for attempt in 0..4 {
                match self.transform.ProcessInput(0, &sample, 0) {
                    Ok(()) => {
                        self.drain(&mut picture)?;
                        return Ok(picture);
                    }
                    Err(error) if error.code() == MF_E_NOTACCEPTING => {
                        self.drain(&mut picture)?;
                        if attempt == 3 {
                            return Err("decoder keeps refusing input".into());
                        }
                    }
                    Err(error) => return Err(format!("ProcessInput: {error}")),
                }
            }
            Ok(picture)
        }
    }
    unsafe fn drain(&mut self, picture: &mut Option<RgbFrame>) -> Result<(), String> {
        unsafe {
            loop {
                let ours = if self.provides_samples {
                    None
                } else {
                    let out = MFCreateSample().map_err(|e| format!("MFCreateSample: {e}"))?;
                    // Before the stream size is known cbSize can be tiny; NV12 4K fits in 16 MiB.
                    let out_buffer = MFCreateMemoryBuffer(self.output_size.max(16 << 20))
                        .map_err(|e| format!("MFCreateMemoryBuffer: {e}"))?;
                    out.AddBuffer(&out_buffer)
                        .map_err(|e| format!("AddBuffer: {e}"))?;
                    Some(out)
                };
                let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: ManuallyDrop::new(ours),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0u32;
                let result = self.transform.ProcessOutput(0, &mut buffers, &mut status);
                let produced = ManuallyDrop::into_inner(std::ptr::read(&buffers[0].pSample));
                drop(ManuallyDrop::into_inner(std::ptr::read(
                    &buffers[0].pEvents,
                )));
                match result {
                    Ok(()) => {
                        if let Some(sample) = produced
                            && let Some(frame) = self.to_rgb(&sample)
                        {
                            *picture = Some(frame);
                        }
                    }
                    Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                    Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                        self.negotiate_output()
                            .ok_or_else(|| "no NV12 output type after stream change".to_string())?;
                    }
                    Err(error) => return Err(format!("ProcessOutput: {error}")),
                }
            }
        }
    }
    unsafe fn to_rgb(&self, sample: &IMFSample) -> Option<RgbFrame> {
        unsafe {
            let buffer: IMFMediaBuffer = sample.ConvertToContiguousBuffer().ok()?;
            let coded_h = self.height as usize;
            let crop = (
                self.crop.0 as usize,
                self.crop.1 as usize,
                self.crop.2 as usize,
                self.crop.3 as usize,
            );
            if crop.2 == 0 || crop.3 == 0 {
                return None;
            }
            let rgb = if let Ok(planar) = buffer.cast::<IMF2DBuffer>() {
                let mut scan0 = std::ptr::null_mut();
                let mut pitch = 0i32;
                planar.Lock2D(&mut scan0, &mut pitch).ok()?;
                let pitch = pitch.unsigned_abs() as usize;
                let frame = std::slice::from_raw_parts(scan0, pitch * coded_h * 3 / 2);
                let rgb = nv12_to_rgb(frame, pitch, coded_h, crop);
                let _ = planar.Unlock2D();
                rgb
            } else {
                let mut ptr = std::ptr::null_mut();
                let mut len = 0u32;
                buffer.Lock(&mut ptr, None, Some(&mut len)).ok()?;
                let pitch = self.stride.unsigned_abs() as usize;
                let frame = std::slice::from_raw_parts(ptr, len as usize);
                let rgb = (frame.len() >= pitch * coded_h * 3 / 2)
                    .then(|| nv12_to_rgb(frame, pitch, coded_h, crop));
                let _ = buffer.Unlock();
                rgb?
            };
            Some((crop.2 as u32, crop.3 as u32, rgb))
        }
    }
}

/// BT.601 limited-range NV12 -> packed RGB8 of the visible `crop` (x, y, w, h) inside a buffer
/// of `coded_h` rows. Rows are split across threads: at 1080p60 this conversion is the single
/// biggest CPU cost of the receive path.
fn nv12_to_rgb(
    frame: &[u8],
    pitch: usize,
    coded_h: usize,
    crop: (usize, usize, usize, usize),
) -> Vec<u8> {
    let (x0, y0, w, h) = crop;
    let mut out = vec![0u8; w * h * 3];
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    // Even row counts keep every chunk aligned to whole UV pairs.
    let rows_per_chunk = (h.div_ceil(threads) + 1) & !1;
    std::thread::scope(|scope| {
        for (index, chunk) in out.chunks_mut(rows_per_chunk * w * 3).enumerate() {
            let rows = chunk.len() / (w * 3);
            let start = index * rows_per_chunk;
            scope.spawn(move || {
                convert_rows(frame, pitch, coded_h, (x0, y0 + start, w, rows), chunk)
            });
        }
    });
    out
}

fn convert_rows(
    frame: &[u8],
    pitch: usize,
    coded_h: usize,
    crop: (usize, usize, usize, usize),
    out: &mut [u8],
) {
    let (x0, y0, w, h) = crop;
    let uv_base = pitch * coded_h;
    for row in 0..h {
        let y_start = (y0 + row) * pitch + x0;
        let y_row = &frame[y_start..y_start + w];
        let uv_start = uv_base + ((y0 + row) / 2) * pitch + (x0 & !1);
        // UV pairs cover two columns; an odd width still needs its last pair to stay in range.
        let uv_len = ((w + 1) & !1).min(frame.len() - uv_start);
        let uv_row = &frame[uv_start..uv_start + uv_len];
        let dst = &mut out[row * w * 3..(row + 1) * w * 3];
        for col in 0..w {
            let y = 298 * (y_row[col] as i32 - 16);
            let pair = col & !1;
            let u = uv_row[pair] as i32 - 128;
            let v = uv_row[(pair + 1).min(uv_len - 1)] as i32 - 128;
            dst[col * 3] = ((y + 409 * v + 128) >> 8).clamp(0, 255) as u8;
            dst[col * 3 + 1] = ((y - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
            dst[col * 3 + 2] = ((y + 516 * u + 128) >> 8).clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The Media Foundation path must decode a real OpenH264-encoded picture with correct colour.
    #[test]
    fn media_foundation_decodes_a_red_picture() {
        use openh264::{
            encoder::{Encoder, EncoderConfig},
            formats::{RgbSliceU8, YUVBuffer},
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let Some(mut mft) = Mft::new() else {
            eprintln!("Media Foundation unavailable; skipping");
            return;
        };
        let (w, h) = (320usize, 240usize);
        let pixels: Vec<u8> = [200u8, 30, 30]
            .iter()
            .copied()
            .cycle()
            .take(w * h * 3)
            .collect();
        let yuv = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&pixels, (w, h)));
        let mut encoder =
            Encoder::with_api_config(openh264::OpenH264API::from_source(), EncoderConfig::new())
                .unwrap();
        let mut decoded = None;
        for _ in 0..8 {
            let unit = encoder.encode(&yuv).unwrap().to_vec();
            match mft.decode_detailed(&unit) {
                Ok(Some(frame)) => {
                    decoded = Some(frame);
                    break;
                }
                Ok(None) => {}
                Err(error) => panic!("Media Foundation: {error}"),
            }
        }
        let (dw, dh, rgb) = decoded.expect("no picture from Media Foundation");
        assert_eq!((dw, dh), (320, 240));
        let centre = ((120 * 320 + 160) * 3) as usize;
        assert!(
            rgb[centre] > 150 && rgb[centre + 1] < 90 && rgb[centre + 2] < 90,
            "{:?}",
            &rgb[centre..centre + 3]
        );
    }
}
