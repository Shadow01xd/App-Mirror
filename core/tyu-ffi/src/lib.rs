//! C ABI for control and leased media buffers. See include/tyu.h for ownership.
#[cfg(target_os = "android")]
mod android;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tyu_core::{identity::WindowsPersistentStore, *};
const INVALID: isize = -1;
const STATE: isize = -2;
const LIMIT: isize = -3;
const FAILURE: isize = -4;
const BUSY: isize = -5;
type Registry = Mutex<HashMap<u64, Arc<Mutex<Handle>>>>;
static REGISTRY: OnceLock<Registry> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Handle {
    #[cfg(target_os = "android")]
    store: Arc<dyn tyu_core::identity::Store>,
    runtime: tokio::runtime::Runtime,
    config: Option<NodeConfig>,
    node: Option<TyuNode>,
    pending: Option<Vec<u8>>,
    media: VecDeque<MediaFrame>,
    leases: HashMap<u64, bytes::Bytes>,
}
struct MediaFrame {
    peer: DeviceId,
    session: SessionId,
    stream: StreamId,
    data: bytes::Bytes,
}
fn handles() -> &'static Registry {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}
fn get(id: u64) -> Option<Arc<Mutex<Handle>>> {
    handles().lock().ok()?.get(&id).cloned()
}
fn ffi(f: impl FnOnce() -> isize) -> isize {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(FAILURE)
}
/// Create a stopped node in the supplied private data directory. Returns an opaque handle or 0.
/// # Safety
/// `directory` must point to `length` readable bytes of UTF-8 for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tyu_initialize(directory: *const u8, length: usize) -> u64 {
    let result = ffi(|| {
        if directory.is_null() || length == 0 || length > 4096 {
            return INVALID;
        }
        // The caller owns this input and it is never retained.
        let bytes = unsafe { std::slice::from_raw_parts(directory, length) };
        let Ok(path) = std::str::from_utf8(bytes) else {
            return INVALID;
        };
        let Ok(store) = WindowsPersistentStore::open(Path::new(path)) else {
            return FAILURE;
        };
        let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        else {
            return FAILURE;
        };
        let store = Arc::new(store);
        let mut config = NodeConfig::new("TYU Native", DevicePlatform::Unknown, store.clone());
        config.capabilities = vec![
            Capability::FileSend,
            Capability::ClipboardSource,
            Capability::ClipboardReceiver,
            Capability::TouchSource,
            Capability::InputReceiver,
        ];
        let Ok(mut registry) = handles().lock() else {
            return FAILURE;
        };
        if registry.len() >= 16 {
            return LIMIT;
        }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        registry.insert(
            id,
            Arc::new(Mutex::new(Handle {
                #[cfg(target_os = "android")]
                store,
                runtime,
                config: Some(config),
                node: None,
                pending: None,
                media: VecDeque::new(),
                leases: HashMap::new(),
            })),
        );
        id as isize
    });
    if result > 0 { result as u64 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn tyu_start(handle: u64) -> isize {
    ffi(|| {
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        let Some(config) = h.config.take() else {
            return STATE;
        };
        match h.runtime.block_on(TyuNode::start(config)) {
            Ok(node) => {
                h.node = Some(node);
                0
            }
            Err(_) => FAILURE,
        }
    })
}
/// Submit postcard TyuCommand bytes. Does not wait for network completion.
/// # Safety
/// `data` must point to `length` readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tyu_send_command(handle: u64, data: *const u8, length: usize) -> isize {
    ffi(|| {
        if data.is_null() || length > 128 * 1024 {
            return INVALID;
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, length) };
        let Ok((command, remaining)) = postcard::take_from_bytes::<TyuCommand>(bytes) else {
            return INVALID;
        };
        if !remaining.is_empty() {
            return INVALID;
        }
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(h) = h.lock() else {
            return FAILURE;
        };
        let Some(node) = &h.node else {
            return STATE;
        };
        match node.commands.try_send(command) {
            Ok(()) => 0,
            Err(_) => BUSY,
        }
    })
}
fn pump(h: &mut Handle) -> std::result::Result<(), isize> {
    if h.pending.is_some() {
        return Ok(());
    }
    let node = h.node.as_mut().ok_or(STATE)?;
    for _ in 0..256 {
        match node.events.try_recv() {
            Ok(TyuEvent::MediaFrame {
                peer,
                session,
                stream,
                data,
            }) => {
                // Deep enough that the consumer, not this queue, decides which frames to skip:
                // dropping an arbitrary P-frame here breaks decoding until the next keyframe.
                if h.media.len() >= 120 {
                    h.media.pop_front();
                }
                h.media.push_back(MediaFrame {
                    peer,
                    session,
                    stream,
                    data,
                });
            }
            Ok(event) => {
                h.pending = Some(postcard::to_allocvec(&event).map_err(|_| FAILURE)?);
                break;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return Err(STATE),
        }
    }
    Ok(())
}
/// Poll postcard control events. Returns 0 if empty, byte count on success.
/// If capacity is insufficient, returns required count without consuming the event.
/// # Safety
/// A non-null `output` must reference `capacity` writable bytes. Null is allowed with capacity 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tyu_poll_event(handle: u64, output: *mut u8, capacity: usize) -> isize {
    ffi(|| {
        if output.is_null() && capacity != 0 {
            return INVALID;
        }
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        if let Err(code) = pump(&mut h) {
            return code;
        }
        let Some(event) = &h.pending else {
            return 0;
        };
        let length = event.len();
        if capacity < length {
            return length as isize;
        }
        if output.is_null() {
            return INVALID;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(event.as_ptr(), output, length);
        }
        h.pending = None;
        length as isize
    })
}
#[repr(C)]
pub struct TyuMediaView {
    pub lease: u64,
    pub data: *const u8,
    pub length: usize,
    pub peer: [u8; 16],
    pub session: [u8; 16],
    pub stream: [u8; 16],
}
/// Borrow an immutable frame until tyu_release_media or shutdown. No frame serialization.
/// # Safety
/// `output` must reference one writable, correctly aligned TyuMediaView.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tyu_acquire_media(handle: u64, output: *mut TyuMediaView) -> isize {
    ffi(|| {
        if output.is_null() {
            return INVALID;
        }
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        if h.leases.len() >= 4 {
            return BUSY;
        }
        let Some(frame) = h.media.pop_front() else {
            return 0;
        };
        let lease = NEXT.fetch_add(1, Ordering::Relaxed);
        let view = TyuMediaView {
            lease,
            data: frame.data.as_ptr(),
            length: frame.data.len(),
            peer: *frame.peer.0.as_bytes(),
            session: *frame.session.0.as_bytes(),
            stream: *frame.stream.0.as_bytes(),
        };
        h.leases.insert(lease, frame.data);
        unsafe {
            output.write(view);
        }
        1
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn tyu_release_media(handle: u64, lease: u64) -> isize {
    ffi(|| {
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        if h.leases.remove(&lease).is_some() {
            0
        } else {
            INVALID
        }
    })
}
/// Terminates and joins all node tasks; invalidates handle and media leases.
#[unsafe(no_mangle)]
pub extern "C" fn tyu_shutdown(handle: u64) -> isize {
    ffi(|| {
        let h = {
            let Ok(mut registry) = handles().lock() else {
                return FAILURE;
            };
            registry.remove(&handle)
        };
        let Some(h) = h else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        if let Some(node) = h.node.take()
            && h.runtime.block_on(node.shutdown()).is_err()
        {
            return FAILURE;
        }
        h.leases.clear();
        h.media.clear();
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_handles_and_null_buffers_are_rejected() {
        assert_eq!(tyu_start(0), INVALID);
        assert_eq!(tyu_shutdown(0), INVALID);
        unsafe {
            assert_eq!(tyu_initialize(std::ptr::null(), 10), 0);
            assert_eq!(tyu_send_command(0, std::ptr::null(), 0), INVALID);
            assert_eq!(tyu_poll_event(0, std::ptr::null_mut(), 0), INVALID);
        }
    }

    #[test]
    fn real_lifecycle_and_poll_buffer_ownership() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().to_str().unwrap().as_bytes();
        let handle = unsafe { tyu_initialize(path.as_ptr(), path.len()) };
        assert_ne!(handle, 0);
        assert_eq!(tyu_start(handle), 0);
        let bytes = postcard::to_allocvec(&TyuCommand::BeginPairing {
            endpoint: "127.0.0.1:1".parse().unwrap(),
        })
        .unwrap();
        assert_eq!(
            unsafe { tyu_send_command(handle, bytes.as_ptr(), bytes.len()) },
            0
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let length = loop {
            let length = unsafe { tyu_poll_event(handle, std::ptr::null_mut(), 0) };
            if length > 0 {
                break length;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let mut too_small = [0u8; 1];
        assert_eq!(
            unsafe { tyu_poll_event(handle, too_small.as_mut_ptr(), 1) },
            length
        );
        let mut bytes = vec![0; length as usize];
        assert_eq!(
            unsafe { tyu_poll_event(handle, bytes.as_mut_ptr(), bytes.len()) },
            length
        );
        assert!(matches!(
            postcard::from_bytes::<TyuEvent>(&bytes).unwrap(),
            TyuEvent::PairingStarted { .. }
        ));
        assert_eq!(tyu_shutdown(handle), 0);
        assert_eq!(tyu_start(handle), INVALID);
    }
}
