//! Hardware H.264 encoder through a Media Foundation transform (AMD/NVIDIA/Intel driver MFT).
//! Software OpenH264 needs ~55 ms per 1080p frame on this class of CPU (18 fps); the GPU encoder
//! does it in a couple of milliseconds, which is what makes 60 fps Monitor possible.
//!
//! Hardware MFTs are asynchronous: they announce `METransformNeedInput` / `METransformHaveOutput`
//! events instead of the synchronous ProcessInput/ProcessOutput contract the decoder uses.
use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize,
    CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFActivate,
    IMFMediaEventGenerator, IMFSample, IMFTransform, METransformHaveOutput, METransformNeedInput,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_EVENT_FLAG_NO_WAIT, MF_LOW_LATENCY,
    MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_MPEG2_PROFILE,
    MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC, MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION, MFCreateMediaType,
    MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video, MFSTARTUP_FULL, MFStartup,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_FRIENDLY_NAME_Attribute, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, eAVEncCommonRateControlMode_LowDelayVBR, eAVEncH264VProfile_Main,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_UI4};
use windows::core::Interface;

pub struct HwEncoder {
    transform: IMFTransform,
    /// Present for asynchronous (all hardware) MFTs.
    events: Option<IMFMediaEventGenerator>,
    codec: Option<ICodecAPI>,
    name: String,
    width: u32,
    height: u32,
    /// Input requests announced by the encoder that have not been satisfied yet.
    need_input: u32,
    provides_samples: bool,
    output_size: u32,
    frame_duration: i64,
    time: i64,
    ready: VecDeque<Vec<u8>>,
}
// The transform is only ever touched from the capture thread.
unsafe impl Send for HwEncoder {}

fn variant_u32(value: u32) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = value;
    }
    v
}
fn variant_bool(value: bool) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = if value {
            VARIANT_TRUE
        } else {
            Default::default()
        };
    }
    v
}

impl HwEncoder {
    /// First hardware H.264 encoder that accepts NV12 at this size, or `None` (no GPU encoder, a
    /// remote desktop session, ...); the caller then falls back to software.
    pub fn new(width: u32, height: u32, fps: u32, bitrate: u32) -> Option<Self> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).ok()?;
            let input_info = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_NV12,
            };
            let output_info = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_H264,
            };
            let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&input_info),
                Some(&output_info),
                &mut activates,
                &mut count,
            )
            .ok()?;
            if activates.is_null() {
                return None;
            }
            let candidates: Vec<IMFActivate> = (0..count as usize)
                .filter_map(|i| std::ptr::read(activates.add(i)))
                .collect();
            CoTaskMemFree(Some(activates as *const _));
            for activate in candidates {
                let name = {
                    let mut buffer = [0u16; 256];
                    activate
                        .GetString(&MFT_FRIENDLY_NAME_Attribute, &mut buffer, None)
                        .ok()
                        .map(|_| {
                            String::from_utf16_lossy(&buffer)
                                .trim_end_matches('\0')
                                .to_string()
                        })
                        .unwrap_or_else(|| "hardware H.264 encoder".into())
                };
                let Ok(transform) = activate.ActivateObject::<IMFTransform>() else {
                    continue;
                };
                match Self::configure(transform, name.clone(), width, height, fps, bitrate) {
                    Some(encoder) => return Some(encoder),
                    None => tracing::debug!(name, "hardware encoder rejected the configuration"),
                }
            }
            None
        }
    }

    unsafe fn configure(
        transform: IMFTransform,
        name: String,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> Option<Self> {
        unsafe {
            let mut is_async = false;
            if let Ok(attributes) = transform.GetAttributes() {
                is_async = attributes.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) == 1;
                if is_async {
                    attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).ok()?;
                }
                let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
            }
            let frame_size = ((width as u64) << 32) | height as u64;
            let frame_rate = ((fps.max(1) as u64) << 32) | 1;
            // Live-stream tuning before the types are set (AMD ignores rate-control changes
            // afterwards); every knob is best effort since drivers differ in what they expose.
            let codec = transform.cast::<ICodecAPI>().ok();
            if let Some(codec) = &codec {
                let set = |key: &windows::core::GUID, value: VARIANT| {
                    if let Err(error) = codec.SetValue(key, &value) {
                        tracing::debug!(?key, %error, "encoder property");
                    }
                };
                // Low-delay VBR: no filler NALs (CBR pads every frame to the bitrate, wasting
                // bandwidth on a static desktop) and no rate-control look-ahead.
                set(
                    &CODECAPI_AVEncCommonRateControlMode,
                    variant_u32(eAVEncCommonRateControlMode_LowDelayVBR.0 as u32),
                );
                set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(bitrate));
                set(&CODECAPI_AVLowLatencyMode, variant_bool(true));
                set(&CODECAPI_AVEncMPVDefaultBPictureCount, variant_u32(0));
                // A keyframe every second so a receiver that joins late or drops recovers.
                set(&CODECAPI_AVEncMPVGOPSize, variant_u32(fps.max(1)));
            }
            // Encoders want the output type before the input type.
            let output = MFCreateMediaType().ok()?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).ok()?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).ok()?;
            output.SetUINT64(&MF_MT_FRAME_SIZE, frame_size).ok()?;
            output.SetUINT64(&MF_MT_FRAME_RATE, frame_rate).ok()?;
            output
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .ok()?;
            output
                .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)
                .ok()?;
            let _ = output.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 0);
            transform.SetOutputType(0, &output, 0).ok()?;
            let input = MFCreateMediaType().ok()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).ok()?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, frame_size).ok()?;
            input.SetUINT64(&MF_MT_FRAME_RATE, frame_rate).ok()?;
            input
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .ok()?;
            transform.SetInputType(0, &input, 0).ok()?;
            let info = transform.GetOutputStreamInfo(0).ok()?;
            let provides_samples = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
            let events = if is_async {
                Some(transform.cast::<IMFMediaEventGenerator>().ok()?)
            } else {
                None
            };
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .ok()?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .ok()?;
            Some(Self {
                transform,
                events,
                codec,
                name,
                width,
                height,
                need_input: 0,
                provides_samples,
                output_size: info.cbSize.max(width * height),
                frame_duration: 10_000_000 / fps.max(1) as i64,
                time: 0,
                ready: VecDeque::new(),
            })
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// SPS/PPS as the encoder reports them, for receivers that join before the next IDR.
    pub fn sequence_header(&self) -> Vec<u8> {
        unsafe {
            let Ok(current) = self.transform.GetOutputCurrentType(0) else {
                return Vec::new();
            };
            let Ok(size) = current.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else {
                return Vec::new();
            };
            let mut blob = vec![0u8; size as usize];
            match current.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None) {
                Ok(()) => blob,
                Err(_) => Vec::new(),
            }
        }
    }

    /// Encodes one BGRA frame of the configured size. Returns the next finished Annex-B access
    /// unit; `Some(empty)` when the encoder is still working on it (it comes out next call),
    /// `None` when the encoder failed and the caller should fall back to software.
    pub fn encode(&mut self, bgra: &[u8], force_keyframe: bool) -> Option<Vec<u8>> {
        if bgra.len() != (self.width * self.height * 4) as usize {
            return None;
        }
        unsafe {
            if force_keyframe && let Some(codec) = &self.codec {
                let _ = codec.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &variant_u32(1));
            }
            let sample = self.nv12_sample(bgra)?;
            if self.events.is_some() {
                let deadline = Instant::now() + Duration::from_millis(250);
                while self.need_input == 0 {
                    if Instant::now() > deadline {
                        return None;
                    }
                    self.pump()?;
                    if self.need_input == 0 {
                        std::thread::sleep(Duration::from_micros(200));
                    }
                }
                self.need_input -= 1;
                self.transform.ProcessInput(0, &sample, 0).ok()?;
                // The GPU answers within a few milliseconds; if not, keep the frame budget and
                // pick the picture up on the next call.
                let deadline = Instant::now() + Duration::from_millis(12);
                while self.ready.is_empty() && Instant::now() < deadline {
                    self.pump()?;
                    if self.ready.is_empty() {
                        std::thread::sleep(Duration::from_micros(200));
                    }
                }
            } else {
                self.transform.ProcessInput(0, &sample, 0).ok()?;
                loop {
                    match self.process_output() {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(()) => return None,
                    }
                }
            }
            Some(self.ready.pop_front().unwrap_or_default())
        }
    }

    /// Handles every pending encoder event without blocking.
    unsafe fn pump(&mut self) -> Option<()> {
        unsafe {
            let events = self.events.clone()?;
            loop {
                let Ok(event) = events.GetEvent(MF_EVENT_FLAG_NO_WAIT) else {
                    return Some(());
                };
                let kind = event.GetType().ok()?;
                if kind == METransformNeedInput.0 as u32 {
                    self.need_input += 1;
                } else if kind == METransformHaveOutput.0 as u32 {
                    self.process_output().ok()?;
                }
            }
        }
    }

    /// One ProcessOutput call; `Ok(true)` when a picture came out.
    unsafe fn process_output(&mut self) -> Result<bool, ()> {
        unsafe {
            let ours = if self.provides_samples {
                None
            } else {
                let out = MFCreateSample().map_err(|_| ())?;
                let buffer = MFCreateMemoryBuffer(self.output_size).map_err(|_| ())?;
                out.AddBuffer(&buffer).map_err(|_| ())?;
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
                        && let Some(bytes) = Self::sample_bytes(&sample)
                        && !bytes.is_empty()
                    {
                        self.ready.push_back(bytes);
                        return Ok(true);
                    }
                    Ok(false)
                }
                Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(false),
                Err(error) => {
                    tracing::debug!(%error, "hardware encoder ProcessOutput");
                    Err(())
                }
            }
        }
    }

    unsafe fn sample_bytes(sample: &IMFSample) -> Option<Vec<u8>> {
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer().ok()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut ptr, None, Some(&mut len)).ok()?;
            let bytes = std::slice::from_raw_parts(ptr, len as usize).to_vec();
            let _ = buffer.Unlock();
            Some(bytes)
        }
    }

    unsafe fn nv12_sample(&mut self, bgra: &[u8]) -> Option<IMFSample> {
        unsafe {
            let (w, h) = (self.width as usize, self.height as usize);
            // OpenH264's SIMD BGRA->I420 (~2 ms at 1080p), then U/V interleaved for NV12.
            let yuv = YUVBuffer::from_rgb8_source(BgraSliceU8::new(bgra, (w, h)));
            let size = w * h * 3 / 2;
            let buffer = MFCreateMemoryBuffer(size as u32).ok()?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).ok()?;
            let out = std::slice::from_raw_parts_mut(ptr, size);
            let (ys, us, vs) = (yuv.strides().0, yuv.strides().1, yuv.strides().2);
            for row in 0..h {
                out[row * w..(row + 1) * w].copy_from_slice(&yuv.y()[row * ys..row * ys + w]);
            }
            let uv = &mut out[w * h..];
            for row in 0..h / 2 {
                let (u, v) = (&yuv.u()[row * us..], &yuv.v()[row * vs..]);
                let line = &mut uv[row * w..(row + 1) * w];
                for x in 0..w / 2 {
                    line[2 * x] = u[x];
                    line[2 * x + 1] = v[x];
                }
            }
            let _ = buffer.Unlock();
            buffer.SetCurrentLength(size as u32).ok()?;
            let sample = MFCreateSample().ok()?;
            sample.AddBuffer(&buffer).ok()?;
            let _ = sample.SetSampleTime(self.time);
            let _ = sample.SetSampleDuration(self.frame_duration);
            self.time += self.frame_duration;
            Some(sample)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    /// On a machine with a GPU encoder: 1080p frames come back as H.264 that a full decoder
    /// (Media Foundation, the same class as the phone's MediaCodec) turns into pictures with the
    /// right colours, keyframes can be forced, and encoding stays well inside the 16 ms budget.
    #[test]
    fn hardware_encoder_produces_h264_fast() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .try_init();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let Some(mut encoder) = HwEncoder::new(1920, 1080, 60, 8_000_000) else {
            eprintln!("no hardware H.264 encoder here; skipping");
            return;
        };
        eprintln!("hardware encoder: {}", encoder.name());
        let mut bgra = vec![0u8; 1920 * 1080 * 4];
        let mut decoder = super::super::decode::Mft::new().expect("Media Foundation decoder");
        let mut idr = 0;
        let mut decoded = 0;
        let mut bytes_total = 0;
        let mut encode_ms = 0.0;
        let started = Instant::now();
        for i in 0..60u32 {
            // Moving red bar on grey so there is something to encode (BGRA).
            bgra.fill(40);
            for row in 0..1080 {
                let start = (row * 1920 + (i as usize * 20) % 1800) * 4;
                for px in bgra[start..start + 400].chunks_mut(4) {
                    px.copy_from_slice(&[20, 20, 220, 255]);
                }
            }
            let t = Instant::now();
            let bytes = encoder.encode(&bgra, i == 40).expect("encoder alive");
            let call_ms = t.elapsed().as_secs_f64() * 1000.0;
            encode_ms += call_ms;
            if bytes.is_empty() {
                eprintln!("frame {i}: no output yet after {call_ms:.1} ms");
                continue;
            }
            if i < 5 || i % 10 == 0 {
                eprintln!("frame {i}: encode call {call_ms:.1} ms");
            }
            bytes_total += bytes.len();
            let units = super::super::windows_capture::nal_types(&bytes);
            if units.contains(&5) {
                idr += 1;
                eprintln!("frame {i}: IDR, nals {units:?}");
            }
            if let Ok(Some((w, h, rgb))) = decoder.decode(&bytes) {
                decoded += 1;
                assert_eq!((w, h), (1920, 1080));
                // Grey background, red bar: check a pixel of each on the middle row.
                let (bar_x, bg_x) = (
                    ((i as usize * 20) % 1800) + 50,
                    ((i as usize * 20) % 1800 + 1000) % 1920,
                );
                let px = |x: usize| &rgb[(540 * 1920 + x) * 3..(540 * 1920 + x) * 3 + 3];
                assert!(
                    px(bar_x)[0] > 150 && px(bar_x)[2] < 80,
                    "frame {i}: bar {:?}",
                    px(bar_x)
                );
                assert!(px(bg_x)[0] < 80, "frame {i}: background {:?}", px(bg_x));
            }
        }
        let per_frame = encode_ms / 60.0;
        eprintln!(
            "hardware encode: {per_frame:.1} ms per 1080p frame (loop incl. decode {:.1} ms), {decoded} decoded, {idr} IDR, {} kB/frame",
            started.elapsed().as_secs_f64() * 1000.0 / 60.0,
            bytes_total / 60 / 1000
        );
        assert!(
            idr >= 2,
            "expected the first IDR plus the forced one, got {idr}"
        );
        assert!(decoded > 40, "only {decoded} frames decoded");
        assert!(
            per_frame < 12.0,
            "{per_frame:.1} ms per frame is too close to the 60 fps budget"
        );
    }
}
