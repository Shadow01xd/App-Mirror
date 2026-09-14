//! JNI connection adapter. JSON exists only at this small control boundary;
//! TyuLink remains binary and media never crosses this interface as JSON.
use super::*;
use jni::{
    JNIEnv,
    objects::{JByteArray, JObject, JString},
    sys::{jboolean, jint, jlong, jstring},
};
use serde_json::{Value, json};

fn string(env: &mut JNIEnv<'_>, value: JString<'_>) -> Option<String> {
    env.get_string(&value).ok().map(Into::into)
}
fn output(env: &JNIEnv<'_>, value: Value) -> jstring {
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_initialize(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    path: JString<'_>,
    name: JString<'_>,
) -> jlong {
    let Some(path) = string(&mut env, path) else {
        return 0;
    };
    let Some(name) = string(&mut env, name) else {
        return 0;
    };
    let handle = unsafe { tyu_initialize(path.as_ptr(), path.len()) };
    let status = ffi(|| {
        let Some(h) = get(handle) else {
            return INVALID;
        };
        let Ok(mut h) = h.lock() else {
            return FAILURE;
        };
        let Some(config) = h.config.as_mut() else {
            return STATE;
        };
        config.name = name;
        config.platform = DevicePlatform::Android;
        config.capabilities = vec![
            Capability::FileSend,
            Capability::FileReceive,
            Capability::ClipboardSource,
            Capability::ClipboardReceiver,
            Capability::MirrorSource,
            Capability::MonitorReceiver,
            Capability::InputReceiver,
        ];
        config.receive_directory = Some(Path::new(&path).join("received"));
        0
    });
    if status != 0 {
        tyu_shutdown(handle);
        return 0;
    }
    handle as jlong
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_start(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
) -> jint {
    tyu_start(handle as u64) as jint
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_shutdown(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
) -> jint {
    tyu_shutdown(handle as u64) as jint
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_snapshot(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
) -> jstring {
    let result = std::panic::catch_unwind(|| {
        let h = get(handle as u64).ok_or(INVALID)?;
        let h = h.lock().map_err(|_| FAILURE)?;
        let node = h.node.as_ref().ok_or(STATE)?;
        let peers=h.store.peers().map_err(|_|FAILURE)?.into_iter().filter(|p|!p.revoked).map(|p|json!({"id":p.device.id.to_string(),"name":p.device.name,"endpoint":p.endpoint.to_string(),"trusted":true})).collect::<Vec<_>>();
        Ok::<_, isize>(json!({"id":node.device.id.to_string(),"peers":peers}))
    });
    output(
        &env,
        result
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(|| json!({"error":"Native state unavailable"})),
    )
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_command(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
    kind: JString<'_>,
    value: JString<'_>,
) -> jstring {
    let kind = string(&mut env, kind);
    let value = string(&mut env, value);
    let result = std::panic::catch_unwind(|| -> std::result::Result<Value, String> {
        let kind = kind.ok_or("Invalid command")?;
        let value = value.ok_or("Invalid value")?;
        if value.len() > 65536 {
            return Err("Value too large".into());
        }
        let mut selected = None;
        let command = match kind.as_str() {
            "discover" => TyuCommand::StartDiscovery,
            "stop-discovery" => TyuCommand::StopDiscovery,
            "pair" => {
                let ticket =
                    tyu_core::identity::PairingTicket::parse(&value).map_err(|e| e.to_string())?;
                selected = Some(ticket.fingerprint.device_id());
                TyuCommand::Pair { ticket }
            }
            "connect" | "disconnect" | "forget" | "ping" | "mirror" | "monitor" => {
                let peer = DeviceId(value.parse().map_err(|_| "Invalid device ID")?);
                selected = Some(peer);
                match kind.as_str() {
                    "connect" => TyuCommand::Connect { peer },
                    "disconnect" => TyuCommand::Disconnect { peer },
                    "forget" => TyuCommand::ForgetPeer { peer },
                    "mirror" => TyuCommand::StartMode {
                        peer,
                        mode: TyuMode::Mirror,
                    },
                    "monitor" => TyuCommand::StartMode {
                        peer,
                        mode: TyuMode::Monitor,
                    },
                    _ => TyuCommand::Ping { peer, value: 42 },
                }
            }
            "stop-mode" | "media-start" | "keyframe" => {
                let v: Value = serde_json::from_str(&value).map_err(|_| "Invalid value")?;
                let field = |k: &str| v[k].as_str().ok_or("Invalid value");
                let peer = DeviceId(field("peer")?.parse().map_err(|_| "Invalid device ID")?);
                let session = SessionId(field("session")?.parse().map_err(|_| "Invalid session")?);
                selected = Some(peer);
                if kind == "stop-mode" {
                    TyuCommand::StopMode { peer, session }
                } else if kind == "keyframe" {
                    let stream = StreamId(field("stream")?.parse().map_err(|_| "Invalid stream")?);
                    let mut f = Frame::new(Message::MediaKeyframeRequest { stream });
                    f.session = Some(session);
                    TyuCommand::Request { peer, frame: f }
                } else {
                    let stream = StreamId::new();
                    let format = MediaMetadata::Video(VideoFormat {
                        width: v["width"].as_u64().ok_or("Invalid value")? as u32,
                        height: v["height"].as_u64().ok_or("Invalid value")? as u32,
                        fps: v["fps"].as_u64().ok_or("Invalid value")? as u16,
                        bitrate: v["bitrate"].as_u64().ok_or("Invalid value")? as u32,
                        codec: VideoCodec::H264,
                        orientation: Orientation::Portrait,
                    });
                    let mut f = Frame::new(Message::MediaStart { stream, format });
                    f.session = Some(session);
                    TyuCommand::Request { peer, frame: f }
                }
            }
            _ => return Err("Unsupported operation".into()),
        };
        let h = get(handle as u64).ok_or("Invalid native handle")?;
        let h = h.lock().map_err(|_| "Native state unavailable")?;
        let node = h.node.as_ref().ok_or("Core is not started")?;
        node.commands
            .try_send(command)
            .map_err(|_| "Core command queue full")?;
        Ok(json!({"ok":true,"peer":selected.map(|p|p.to_string())}))
    });
    output(
        &env,
        match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"error":error}),
            Err(_) => json!({"error":"Native failure"}),
        },
    )
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_poll(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
) -> jstring {
    let result = std::panic::catch_unwind(|| -> std::result::Result<Value, isize> {
        let h = get(handle as u64).ok_or(INVALID)?;
        let mut h = h.lock().map_err(|_| FAILURE)?;
        for _ in 0..32 {
            pump(&mut h)?;
            let Some(bytes) = h.pending.take() else {
                return Ok(Value::Null);
            };
            let event = postcard::from_bytes::<TyuEvent>(&bytes).map_err(|_| FAILURE)?;
            let value = match event {
                TyuEvent::DiscoveryStarted => json!({"type":"discovery"}),
                TyuEvent::DeviceFound { id, endpoints } => {
                    json!({"type":"found","id":id.to_string(),"endpoints":endpoints})
                }
                TyuEvent::DeviceLost { id } => json!({"type":"lost","id":id.to_string()}),
                TyuEvent::Connecting { peer } => {
                    json!({"type":"connecting","peer":peer.to_string()})
                }
                TyuEvent::Connected { device, address } => {
                    json!({"type":"connected","id":device.id.to_string(),"name":device.name,"capabilities":device.capabilities,"address":address.ip().to_string()})
                }
                TyuEvent::Disconnected { peer } => {
                    json!({"type":"disconnected","peer":peer.to_string()})
                }
                TyuEvent::ConnectionFailed { peer, code } => {
                    json!({"type":"failed","peer":peer.to_string(),"error":code.to_string()})
                }
                TyuEvent::Error { code, .. } => json!({"type":"error","error":code.to_string()}),
                TyuEvent::Pong { value, .. } => json!({"type":"pong","value":value}),
                TyuEvent::MetricsUpdated { rtt_micros, .. } => {
                    json!({"type":"metrics","rttMicros":rtt_micros})
                }
                TyuEvent::ModeStarted { session, mode, .. } => {
                    json!({"type":"mode-started","session":session.to_string(),"mode":format!("{mode:?}")})
                }
                TyuEvent::ModeStopped { session, .. } => {
                    json!({"type":"mode-stopped","session":session.to_string()})
                }
                TyuEvent::MediaStarted {
                    session, stream, ..
                } => {
                    json!({"type":"media-started","session":session.to_string(),"stream":stream.to_string()})
                }
                TyuEvent::MediaStopped { stream, .. } => {
                    json!({"type":"media-stopped","stream":stream.to_string()})
                }
                TyuEvent::KeyframeRequested { .. } => json!({"type":"keyframe"}),
                TyuEvent::InputReceived { event, .. } => {
                    json!({"type":"input","event":serde_json::to_value(&event).unwrap_or(Value::Null)})
                }
                _ => continue,
            };
            return Ok(value);
        }
        Ok(Value::Null)
    });
    output(
        &env,
        result
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(|| json!({"type":"error","error":"Native state unavailable"})),
    )
}
/// Pops the oldest complete incoming video access unit (Monitor: PC -> phone), or null when none
/// is queued. Frames arrive here through the same bounded queue the C API leases from.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_acquireMediaFrame(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
) -> jni::sys::jbyteArray {
    let frame = std::panic::catch_unwind(|| -> Option<bytes::Bytes> {
        let h = get(handle as u64)?;
        let mut h = h.lock().ok()?;
        pump(&mut h).ok()?;
        h.media.pop_front().map(|f| f.data)
    })
    .ok()
    .flatten();
    match frame {
        Some(data) => env
            .byte_array_from_slice(&data)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}
/// Packetizes one already-encoded frame (e.g. a MediaCodec H.264 access unit) and hands it to
/// the actor to send as TyuLink datagrams on an already-started Mirror media stream. Binary
/// frame bytes never cross the JSON command/poll boundary used for control.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_tyu_app_backend_NativeCore_sendMediaFrame(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    handle: jlong,
    peer: JString<'_>,
    session: JString<'_>,
    stream: JString<'_>,
    frame_id: jlong,
    timestamp_micros: jlong,
    keyframe: jboolean,
    data: JByteArray<'_>,
) -> jboolean {
    let peer = string(&mut env, peer);
    let session = string(&mut env, session);
    let stream = string(&mut env, stream);
    let bytes = env.convert_byte_array(&data).ok();
    let sent = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<()> {
        let peer = DeviceId(peer?.parse().ok()?);
        let session = SessionId(session?.parse().ok()?);
        let stream = StreamId(stream?.parse().ok()?);
        let bytes = bytes?;
        let h = get(handle as u64)?;
        let h = h.lock().ok()?;
        let node = h.node.as_ref()?;
        let packets = media::packetize(
            session,
            stream,
            frame_id as u64,
            timestamp_micros as u64,
            keyframe != 0,
            bytes.into(),
            1200,
        )
        .ok()?;
        for packet in packets {
            h.runtime.block_on(node.send_media(peer, packet)).ok()?;
        }
        Some(())
    }))
    .ok()
    .flatten()
    .is_some();
    sent as jboolean
}
