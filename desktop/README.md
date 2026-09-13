# TYU Desktop

Native Rust Edition 2024 / Slint 1.17.1 client, now part of the root Cargo workspace.
The existing composition, theme and navigation are preserved. Android remains a
separate Gradle application and connects through the same Rust Core via JNI.

## Run

```powershell
cargo run -p tyu-desktop
cargo run -p tyu-test-peer -- pair 'tyu://pair/...'
```

Normal startup uses TYU Core: LAN discovery, QUIC/TLS 1.3, pinned QR invitations,
explicit approval, persistent identity and trusted reconnect. This QR requires a
native TYU-compatible client; a phone browser is not the product connection path.
Install the Android APK and select this computer in the discovery list, or open
**Conectar mediante QR** and scan its QR. Both first-pairing paths require approval
in Desktop. Update both clients together. See [Android connection](../docs/ANDROID_CONNECTION.md).

The backend runtime owns networking. Slint callbacks send commands through
`src/backend/core_bridge.rs`; bounded Core events update the existing AppState.
No screen/camera/microphone capture, OS input injection or virtual drivers exist.
Protocol negotiation must not be interpreted as an operating platform device.

Real files can be sent by the peer into the Desktop private Received directory.
Desktop `--send-file <path>` sends a file when the next peer connects. Native file
picker, full remote storage browser and multi-device workspace remain UI work.
Use `--data-dir <directory>` for isolated development identities.

`TYU_ADVERTISE_IP` selects the QR's interface. mDNS advertises eligible interfaces.
For local CLI testing, explicitly set `TYU_PAIRING_URI_FILE` to export the current
short-lived invitation; protect and remove this bearer invitation after use.

## Development fallback and screenshots

```powershell
cargo run -p tyu-desktop -- --legacy-pairing
cargo run -p tyu-desktop -- --smoke
cargo run -p tyu-desktop -- --view connected --screenshot capture.png
```

`src/pairing.rs` and `src/pairing.html` are retained as an unencrypted HTTP
fallback. Only explicit legacy/fixture modes start that server. HTTP tokens do
not authorize Core services. The original native smoke verifies legacy callbacks;
new Rust tests independently verify the real Desktop bridge with QUIC peers.

`--view connected|mirror|bypass|pairing` preserves screenshot fixtures.
Normal startup has no sample connected devices or synthetic transfer clock.

## Verify

Run at the repository root:

```powershell
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tyu-desktop -- --smoke
```

Workspace builds now write to the root `target/`; the retained historical
`desktop/Cargo.lock` is not the active workspace lockfile.

See [architecture](../docs/ARCHITECTURE.md), [protocol](../docs/PROTOCOL.md),
[security](../docs/SECURITY.md) and [backend status](../docs/BACKEND_STATUS.md).
