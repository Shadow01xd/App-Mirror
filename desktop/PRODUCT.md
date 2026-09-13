# TYU Desktop
<!-- impeccable:product-schema 1 -->
## Platform
Native desktop / Windows (Rust + Slint).
## Product Purpose
A desktop workspace for using a phone and computer together. The active Wi-Fi connection lives on the left; modes and their connection diagram live on the right.
## Capabilities and Constraints
Native desktop UI integrated with the shared Rust TYU Core. Normal pairing uses a pinned TyuLink QR, QUIC/TLS and explicit desktop approval, with persistent identity/trust. Real Core discovery, files, remote storage, clipboard/input events and media transport exist; the current UI exposes only part of these APIs. No platform capture or virtual drivers are implemented. Monitor: PC to phone. Mirror: phone to PC. Bypass: bidirectional services without screen transmission. The old HTTP/browser pairing is an explicit --legacy-pairing development fallback; screenshots and the original --smoke retain fixtures. See ../docs/BACKEND_STATUS.md for precise implementation and validation limits.
## Brand Commitments
User explicitly requests Android's black surfaces, yellow #F6CC45, Material Outlined icons and motion vocabulary, with a desktop-specific layout. Composition is delegated to the implementer.
## Evidence on Hand
../Android/PRODUCT.md, ui/theme, ui/icons and ui/components/TyuDeviceStage.kt. Android screenshots predate current sans typography; current source is authoritative.
## Users
People using their phone alongside their PC; no narrower audience established.
