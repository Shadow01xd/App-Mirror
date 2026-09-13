# Backend status

This is a real first backend implementation, with incomplete product/platform integration. “Negotiated” never means hardware capture or a driver is operational.

| Component | Implemented | Tested | Platform blocked | Next step |
| --- | --- | --- | --- | --- |
| Workspace/types | Six members; neutral serializable domain values | Host compile/tests; ARM64 and x86_64 Android builds | No | Android target CI |
| TyuLink v1 | Binary framing, typed errors, limits, message schema | Golden fixture, roundtrip, malformed/type/version/size tests | No | Full interoperability vectors and fuzzing |
| Identity/trust | Persistent certificate, QR pin, mutual TLS proof, approval, revocation | Valid/rejected/expired/replayed tickets, unknown/wrong identity, revoked reconnect | No | Recovery, certificate lifecycle, cross-process store locking |
| Persistence | DPAPI on Windows; Android app-private versioned files with backup excluded | Windows restart/revocation/corruption; Android process restart and trusted reconnect | Hardware-backed Android Keystore absent | Keystore wrapping and recovery |
| QUIC/Core | Real Tokio node, reliable control and auxiliary streams, datagrams, metrics, cancellation | Real loopback nodes, ping, disconnect and trusted reconnect | No | Automatic reconnect/backoff, long-duration stress tests |
| Discovery | mDNS publish/browse, multiple interfaces, endpoint refresh | TXT parser, start/stop/restart lifecycle | LAN multicast environment affects visibility | Multi-host/multi-adapter and IPv6 acceptance tests |
| Capabilities/sessions | Directional capability validation, display and independent services | Bypass/Monitor/Camera plus concurrent service unit tests | Capture is separate | Concurrent negotiation conflict recovery |
| Files | Chunked streams, BLAKE3, progress, pause, resume after disconnect, cancel, no-clobber commit | Real temporary files, matching hashes and partial resume | Atomic commit currently needs hard-link filesystem support | Native file picker, persisted outgoing queue, disk/inactivity quotas |
| Storage | list/stat/read/write/mkdir/rename/delete through confined provider | Real CRUD, traversal; Windows junction test | Android SAF absent | Pagination, SAF and full Desktop browser |
| Media transport | Video/audio formats, fragments, reorder, assembly, expiry, keyframe request, sender ownership | Component tests and real camera-test datagrams across QUIC | Capture/codec/driver adapters absent | Codec pipelines, jitter and bitrate control |
| Input | Validated touch/multitouch/mouse/keys/text, viewport mapping, independent channel | Serialization, normalization, real delivery | OS input injection absent | Windows/Android injection adapters and permissions |
| Clipboard | Bounded UTF-8 transport and events | Real peer-to-peer text | OS clipboard adapter absent | Native clipboard integration and loop prevention |
| Desktop bridge | Native worker, command/event mapping, Core QR/approval/connection, real transfer events, discovery rows | Existing state tests plus actual bridge/Core pairing test | Screen/camera/microphone remain unavailable | Full storage/transfer controls, multi-peer selection |
| Test peer | Same Core, persistent identity, LAN advertise/discover, pair/connect, files/storage root, terminal commands | Built with workspace; Core behavior covered by loopback | Uses synthetic media capabilities | Scriptable multi-process acceptance suite |
| C FFI / JNI | initialize/start/send/poll/shutdown, bounded immutable media leases, C header; Android connection commands/events | Host lifecycle/buffers; ARM64 Clippy; native JNI on x86_64 emulator | Host build does not need NDK | Provider configuration and outbound media buffers |
| Legacy HTTP | Retained as explicit development fallback | Original HTTP and native smoke tests | No | Remove only after native Android pairing ships |
| Android connection | Compose + JNI + same Core; discovery pairing with Desktop approval, QR scanner/deep link, trust, disconnect/reconnect/forget, Activity lifecycle; no URI textbox | APK builds for ARM64/x86_64; real approval/reconnect tests; see correction record below | Physical phone and QR camera scan not exercised; SAF/capture absent | Physical LAN validation, foreground lifecycle, native files/SAF UI |

## Validation record

Baseline before edits: Desktop 12 tests and original `--smoke` passed. Android initially reported an unset SDK location; using the installed `%LOCALAPPDATA%/Android/Sdk` via `ANDROID_HOME` allowed `assembleDebug`, `testDebugUnitTest` and `lintDebug` to finish successfully. Unit-test task reported NO-SOURCE. Lint warnings remain visible; no baseline/suppression was added.

Validation on 2026-09-12: the workspace suite has 38 tests: 17 Core, 13 Desktop, 2 FFI and 6 protocol. No tests were disabled. The added cancellation case cancels a pending pairing, checks that neither store gained trust, then pairs successfully with a fresh ticket.

Exact breakdown: Core unit 2, Core component 9, Core loopback 6, Desktop unit/bridge 5, Desktop navigation 2, Desktop state 6, FFI 2, protocol 6 = 38.

```powershell
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tyu-desktop -- --smoke
cargo run -p tyu-test-peer -- capabilities
```

All six commands above pass: formatting, workspace check, Clippy with warnings denied, all 38 tests, Desktop native smoke and the test-peer capability command. A Windows executable lock occurred while the acceptance Desktop was still open; after closing that test instance cleanly, the suite and smoke completed successfully.

Android `assembleDebug`, `assembleDebugAndroidTest` and `lintDebug` pass. `testDebugUnitTest` reports NO-SOURCE; actual Android testing uses instrumentation. Lint has 0 errors and 4 visible warnings (dependency freshness and the existing optional-font resource lookup); no suppressions were introduced. NDK r30 builds `libtyu_ffi.so` for both ARM64 and x86_64. Android-target `cargo clippy -p tyu-ffi --target aarch64-linux-android -- -D warnings` passes.

An isolated Android 16.1 x86_64 emulator connected over real QUIC to the actual Slint Desktop process using the host alias `10.0.2.2`. `NativeConnectionTest` pasted the current invitation into the existing Compose screen, waited for approval invoked through Desktop's accessibility button, disconnected, reconnected with stored trust and recreated the Activity: PASS (29.065 s). A second invocation after `am force-stop`, using `tyuReconnectOnly=true`, restored the peer and reconnected without a QR or Desktop approval: PASS (14.212 s). `TyuJourneyTest`: all 4 original visual journeys pass (89.928 s). Espresso was updated from the incompatible old transitive version to 3.7.0 to fix Android 16's removed reflective InputManager API; no test was skipped.

The emulator test verifies the native connection and UI approval path, not multi-host multicast propagation or a physical camera scanning a QR. ARM64 is build-verified; no physical phone was connected for execution. See [Android connection instructions](ANDROID_CONNECTION.md).

## Explicitly unfinished

### Connection correction after physical-phone feedback

The initial emulator-only result did not catch a multi-interface bug: Desktop chose the first IPv4 address, `192.168.56.1` on a host-only virtual adapter, before Wi-Fi `192.168.1.195`. The QR now selects the OS-routed IPv4 interface. For discovered peers and trusted reconnects, Core tries up to eight compatible advertised endpoints concurrently, verifies TLS, and submits just one approval request on the winning connection.

Untrusted discovery selection now sends PAIR_NEARBY_REQUEST and waits for Desktop approval, without requiring a QR first. Existing trust still uses full fingerprint pinning. New tests cover initial approval, rejection/retry, trusted reconnect, revocation, opt-out, wrong identity, and one unreachable route alongside a reachable route. QR renewal and stale rejection events no longer close an unrelated pending Desktop approval. The native Android QR page has no pasted-link field, and discovery starts automatically.

After these corrections the workspace has 42 tests (19 Core, 14 Desktop, 2 FFI, 7 protocol). The APK builds for ARM64 and x86_64. Desktop must be rebuilt/restarted along with Android because older Desktop builds reject the new discovery-pairing message.

Correction verification: all 42 Rust tests pass, host and ARM64 JNI Clippy pass with warnings denied, and the Desktop smoke passes. The actual Desktop invitation was checked to contain Wi-Fi `192.168.1.195`. The updated Android native test connects to that Wi-Fi address (without the emulator-only alias), waits for approval invoked through the real Slint button, disconnects, reconnects and recreates the Activity: PASS, 12.145 seconds. It supplies a decoded QR payload directly to the scanner's existing ViewModel handler; no product textbox was reintroduced. Attempts to inject an external intent interfered with ActivityScenario's lifecycle tracking, so that input mechanism is not counted as validated.

The `tyuNearby=true` Android test was also attempted but failed waiting for a discovery row: the emulator did not receive the host's multicast advertisements. This is not counted as an Android discovery acceptance pass. Discovery-result selection through Core is covered by real QUIC tests with seeded discovery endpoints, including approval/rejection/reconnect and a dead alternate route. No physical phone was available over ADB in this correction run; physical multicast and camera scanning remain to be checked on the user's LAN.

- Android previews and the explicit debug `tyu.fixture=true` graph retain original visual mocks. Normal connection state comes from JNI/Core; unsupported services do not become fake active sessions.
- Desktop screenshot fixtures and the opt-in HTTP smoke retain demo session/service behavior. Normal Core mode does not start fake screen/camera/microphone capture or tick synthetic file progress.
- Windows Indirect Display Driver, virtual camera and virtual microphone do not exist in this phase. Screen capture and OS input/clipboard adapters are also absent; Explorer integration was not added.
- Android MediaProjection, CameraX, AudioRecord and SAF are not implemented. JNI connection integration is implemented; background service, hardware-backed keystore and platform clipboard integration are not.
- Core negotiation and transport are functional; media capture, decode/render, native storage navigation UI, service permissions, automatic reconnect and production-grade persistence recovery are not complete.

Recommended next phase: test a physical ARM64 phone on a multi-device LAN, add Android NDK CI and harden protocol/binding schemas. Expose native files and SAF with permission/lifecycle handling. Add platform media capture/codec/render adapters after those paths work end-to-end; develop Windows virtual drivers as a separate project.
