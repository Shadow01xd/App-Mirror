//! JNI connection adapter. JSON exists only at this small control boundary;
//! TyuLink remains binary and media never crosses this interface as JSON.
use super::*;
use jni::{
    JNIEnv,
    objects::{JObject, JString},
    sys::{jint, jlong, jstring},
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
            "connect" | "disconnect" | "forget" | "ping" => {
                let peer = DeviceId(value.parse().map_err(|_| "Invalid device ID")?);
                selected = Some(peer);
                match kind.as_str() {
                    "connect" => TyuCommand::Connect { peer },
                    "disconnect" => TyuCommand::Disconnect { peer },
                    "forget" => TyuCommand::ForgetPeer { peer },
                    _ => TyuCommand::Ping { peer, value: 42 },
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
                TyuEvent::Connected { device } => {
                    json!({"type":"connected","id":device.id.to_string(),"name":device.name,"capabilities":device.capabilities})
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
