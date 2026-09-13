# Security model and current limits

## Adversaries and trust boundary

The LAN is untrusted: discovery spoofing, rogue peers, MITM, captured packets, malformed frames and hostile storage paths are expected. The current user's OS account, executable, configured storage root and UI approval are trusted. Local administrators, compromised endpoints, malicious processes running as the same user, hard links already planted by such processes, and physical QR observation are outside this first implementation's protection boundary.

No cloud account, external pairing service, telemetry, relay or server is involved. Build tools download dependencies; runtime protocol traffic is local discovery plus explicitly selected QUIC endpoints.

## Identity, MITM and replay

DeviceIdentity is an rcgen certificate and persistent private key. SHA-256 of the full certificate is its fingerprint; the first 16 digest bytes form the opaque UUID-shaped DeviceId. A different certificate cannot claim a trusted name/ID. TLS CertificateVerify uses rustls/ring's signature verification, on both ends. TLS 1.2 and early data are disabled.

This is a pinned self-issued identity system, not public Web PKI. Hostname, CA-chain policy and X.509 expiry are not the trust mechanism. Pin changes require explicit identity recovery; automatic certificate rotation and PKI revocation are not implemented.

Unknown client certificates can complete the cryptographic TLS handshake solely to enter a bounded, timed quarantine. They cannot use file, storage, input, media or ordinary control operations until their trust or explicit pairing approval succeeds. Known reconnects never accept a replacement certificate. QR server certificates are pinned directly to a scanned invitation; reconnect uses stored full fingerprints.

Discovery pairing is an explicit trust-on-first-use workflow: the initiating user selects a discovered certificate-derived ID, TLS verifies that ID and private-key possession, and Desktop must separately approve the incoming client. Only then is the full certificate fingerprint persisted. The default Core configuration rejects this workflow; Desktop enables it. mDNS itself is unauthenticated and a spoofed device name is not proof of ownership. Without a scanned fingerprint, first-use discovery does not claim QR's out-of-band resistance to impersonation. Existing pins are never downgraded to a discovery ID. Approval requests remain bounded to eight pending requests and 120 seconds; closed/expired requests cannot be approved later.

Tickets use OS randomness, expire after 120 seconds, and are consumed atomically before approval. Reuse after rejection or acceptance is rejected. Captured TLS records cannot be replayed into a new connection. A bounded recent-request cache detects duplicate command IDs within a live session; it is not persistent application-wide idempotency. The displayed fingerprint prefix is an identity hint, not a new cryptographic short-authentication-string scheme.

Revocation persists a tombstone, closes active connections through ForgetPeer and is rechecked when in-flight handshakes are admitted. Re-pairing a revoked identity is intentionally not an implicit un-revoke operation.

## Secret persistence

Identity, friendly name and trusted peers are stored together in a versioned binary store. On Windows the complete serialized store is protected by current-user DPAPI (`CryptProtectData`, UI forbidden), using windows-rs. No machine-wide DPAPI scope is used. Plaintext temporary serializations/private keys are zeroized on drop. Updates write a unique temporary file, sync it and replace the previous store. Corrupt, oversized or un-decryptable stores fail closed; no silent reset or identity rotation occurs.

On Unix the store uses private directory/file modes (0700/0600). Android JNI uses this store under `Application.filesDir/tyu-core`, inside the app sandbox, with app backup disabled. It does not provide hardware-backed Android Keystore encryption. Uninstalling or clearing app data removes identity and trust; Android Keystore wrapping and recovery remain future hardening work.

Concurrent independent processes must use separate profile directories. Cross-process store locking, backup/recovery and stronger crash durability across every filesystem are not implemented. Tickets, TLS session keys, media frames and expired invitations are not part of the identity store.

## Filesystem and resource controls

Remote paths reject `..`, empty components, absolute paths, drive letters, backslashes, NTFS alternate data streams, reserved device names and invalid portable filenames. `cap-std` confines resolution and operations to an open directory capability, including symlinks/reparse paths. Windows tests exercise an actual escaping NTFS junction. The provider does not perform recursive delete.

File receive is opt-in via a configured inbox and capability; trusted senders may then send files into that inbox. Files are streamed, size-limited, BLAKE3-verified and committed without overwriting a destination. Peer+transfer IDs isolate resume metadata; only one receiver per transfer can run. Defaults permit at most 16 active receive jobs and 1024 total inbox entries. Outgoing transfers are bounded to 32 records. Per-file size is configurable (8 GiB by default). These are bounds, not a disk-space reservation system: incomplete transfers occupy disk until resumed or cleaned locally.

Protocol envelopes are checked before allocating payload buffers. Command/event queues, connections, streams, pending requests, directory listings, clipboard, text input, file chunks and media assemblies are bounded. TLS/control/RPC timeouts and cancellation handle lost peers. A trusted malicious peer can hold a paused file stream open while keeping QUIC alive; disconnect/revoke ends it. Fine-grained per-peer service permissions, bandwidth quotas, transfer inactivity policies and global disk quotas remain hardening work.

Media may be dropped under datagram congestion. It cannot be used to bypass negotiated stream ownership. FFI media leases have explicit lifetime and count limits; callers must obey C pointer validity and shutdown ownership rules.

## Logging and legacy HTTP

Tracing is local and configurable through `RUST_LOG`. Network diagnostics record typed codes and lifecycle failures, not private keys, file contents or full pairing tickets. The test CLI prints its own invitation intentionally so it can be pasted into another client. The optional Desktop URI handoff file is a caller-requested development mechanism, not persistent trust storage.

`desktop/src/pairing.rs` and `pairing.html` remain an **unencrypted development fallback**, enabled only by `--legacy-pairing`, `--view`, or the original `--smoke`. HTTP approval does not grant Core trust or open Core services. Do not treat its browser token as a TyuLink credential.

## Verification boundaries

Tests cover real TLS/QUIC pairing/reconnect, wrong pin, unknown identity, rejection, expiry, replay, revocation, corrupt persistence, path attacks, junction escape, bounded protocol errors, file integrity/resume/cancel, media reordering and task shutdown. This is not an independent security audit or a fuzzing campaign. Multi-host hostile-network testing, sustained resource-exhaustion testing, certificate lifecycle/recovery and platform keystores remain required before production release.

Implementation references: [Quinn TLS integration](https://docs.rs/quinn/latest/quinn/crypto/rustls/index.html), [cap-std confinement](https://docs.rs/cap-std/latest/cap_std/), [mDNS daemon](https://docs.rs/mdns-sd/latest/mdns_sd/struct.ServiceDaemon.html), [Windows junction test helper](https://docs.rs/junction/latest/junction/), [Android backup exclusion domains](https://developer.android.com/identity/data/autobackup).
