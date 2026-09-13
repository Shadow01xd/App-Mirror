# TYU Core

Rust Edition 2024 workspace. Desktop remains native Slint 1.17.1; Android remains an independent Gradle/Compose application. No cloud, telemetry or web application runtime was added.

```text
Android / Kotlin / ConnectionViewModel
          |
       tyu-ffi             Desktop / Slint
          |                      |
          |                desktop/backend
          |                      |
       tyu-core <--- TyuLink ---> tyu-core
          |
    TYU Test Peer (native CLI, same Core)
```

## Modules

| Crate | Responsibility |
| --- | --- |
| tyu-types | Serializable IDs, capabilities, devices, sessions, storage, media and input values; no UI/platform dependencies |
| tyu-protocol | Frozen v1 binary message schema, explicit envelope, limits, validation and typed errors |
| tyu-core | Tokio actor, TLS identity/admission, QUIC streams, discovery, sessions, file engine, storage provider and media packetization |
| tyu-ffi | Host-compilable C ABI, opaque handles, command/event buffers, explicit shutdown and leased media buffers |
| tyu-test-peer | Native Android-like protocol peer; explicit test capabilities, persistent trust, interactive commands |
| tyu-desktop | Existing Slint frontend and gradual adaptation of Core events to AppState |

The UI `Device` and Core `DeviceInfo` remain different types. The same applies to presentation transfer/session values. Android's existing Compose surfaces now consume native connection events through a Kotlin ViewModel and JNI. The visual fixture graph remains explicitly selectable in debug tests.

## Ownership and concurrency

`TyuNode::start(NodeConfig)` creates a QUIC endpoint and a bounded command/event actor. Commands enqueue work; completion and failures arrive asynchronously. Enqueuing a command is not a success acknowledgement for its network operation.

The actor owns connection/session maps, approvals and pending requests. Connection workers own their control readers, auxiliary streams and datagram input. Files and storage do not share the control stream. Tasks are tracked in `JoinSet`; connection cancellation propagates through `CancellationToken`. Explicit `shutdown()` cancels, closes QUIC, drains tasks and waits for endpoint cleanup. Dropping a node requests cancellation; callers should await shutdown for deterministic teardown.

Default limits: 16 connections, 16 bidirectional streams per connection, 128 commands, 256 events, 128 pending control requests, 64 sessions, 64 media streams, 32 tracked outgoing transfers. Slow consumers exert backpressure and must keep draining events. The backend does not buffer an unlimited event history.

`StorageProvider` is UI/platform-neutral. `LocalFilesystemProvider` delegates path resolution to `cap-std` directory capabilities and imposes additional portable filename checks. Android SAF will implement this trait later. Blocking storage calls use Tokio's blocking pool.

## Modes and media

Monitor means PC to phone; Mirror means phone to PC. Bypass creates a service-only session. A display session and multiple independent services may coexist. `Ready` means protocol negotiation succeeded; it does not mean a screen, camera or microphone driver exists.

Video and audio use separate `StreamId`s. Media start/config requires the matching session, format and source/receiver capabilities. Only the negotiated sender may publish datagrams. Packet slices share `Bytes` backing storage during packetization; current wire encoding and final reassembly make bounded copies. This is not a zero-copy capture pipeline.

## Desktop migration

The normal application starts `desktop/src/backend/runtime.rs`, on a dedicated worker thread. The existing native timer drains bounded events into `CoreBridge`; `slint::invoke_from_event_loop` wakes native processing. Slint callbacks enqueue commands and never operate sockets or Tokio tasks.

Normal startup uses Core pairing. The existing HTTP implementation is retained under its original filenames as an explicitly selected **development fallback**: `--legacy-pairing`. `--view` and the original `--smoke` use the legacy/fixture path to preserve existing tests and screenshots. Normal startup does not generate demo transfers or advance their progress clock.

The current UI represents one selected peer, while Core supports multiple connections. Available devices populate existing presentation rows. Capabilities are retained by the bridge. A complete remote file browser, native file picker, multi-peer workspace and detailed metrics controls remain frontend integration work; the Core APIs already perform these operations.

## Run

```powershell
cargo run -p tyu-desktop
cargo run -p tyu-test-peer -- capabilities
cargo run -p tyu-test-peer -- pair 'tyu://pair/...'
```

Accept the incoming request in Desktop. In the peer terminal, `ping`, `bypass`, `send-file C:\path\file.bin`, `list`, and `clipboard text` use the active authenticated connection. `connect UUID` reconnects using persistent trust; discovery supplies a refreshed endpoint when available. Automatic reconnect/backoff is not implemented.

Desktop accepts `--send-file <path>` to send a real file after the next connection, and `--data-dir <directory>` for isolated development profiles. Received files normally go under the current user's local app data, `TYU Desktop/Received`. The exact native path follows `directories::ProjectDirs` and is not inside the source tree.

The QR prefers the IPv4 address selected by the OS routing table, rather than the first interface (which may be a virtual host-only adapter). `TYU_ADVERTISE_IP` explicitly selects another interface. mDNS advertises all eligible interfaces. Core races up to eight compatible endpoints through TLS and sends one application admission request on the winning connection. `TYU_PAIRING_URI_FILE` optionally exports the short-lived URI to a caller-selected file for local CLI testing; treat that file as a bearer invitation and remove it after use. Without this opt-in, tickets are not written to files or tracing logs.

The peer supports `--state`, `--receive`, `--storage`, `--bind`, `--endpoint`. Its printed QR defaults to loopback for local testing; set `--endpoint LAN_IP:PORT` for another machine. `--approve` explicitly enables unattended pairing approval for test setups; Desktop never auto-approves.

## FFI

`core/tyu-ffi/include/tyu.h` documents initialize/start/send/poll/acquire/release/shutdown and ownership rules. C control buffers use postcard, with a provisional v0.1 binding schema. Media frames are leased immutable native pointers, bounded to four active leases. Poll control events to drain incoming frames into the media queue. NDK is not required to build the host workspace.

Android JNI configures the same Core with Android identity/capabilities and a private inbox. Its small connection-control boundary uses JSON strings for discover, pair, connect, disconnect, forget, ping and events. TyuLink on the network remains binary; media is never JSON. `ConnectionViewModel` serializes native calls on an IO dispatcher, polls bounded events, survives Activity recreation and shuts down the native runtime when cleared. Foreground-service/background-lifetime guarantees are not implemented.

Gradle builds ARM64 and x86_64 Rust libraries through `tools/build-android-core.ps1` and packages them in the APK. The current build helper requires Windows, Rust targets and an Android NDK. See [Android connection](ANDROID_CONNECTION.md) for build and acceptance instructions. SAF, media capture, OS clipboard and hardware-backed Android keystore integration remain separate work.
