# TYU Desktop
<!-- impeccable:product-schema 1 -->
## Platform
Native desktop / Windows (Rust + Slint).
## Product Purpose
A desktop workspace for using a phone and computer together. The active Wi-Fi connection lives on the left; modes and their connection diagram live on the right.
## Capabilities and Constraints
Independent desktop UI with real local HTTP pairing via QR and explicit desktop approval. Starts with no phone connected. The phone browser submits its chosen device name; accepting opens the connected workspace, rejecting preserves the current page. No hardware capture, screen streaming or real file access is implemented. The desktop home exposes no USB or transfer surface. Monitor: PC to phone. Mirror: phone to PC. Bypass: bidirectional device integration.
## Brand Commitments
User explicitly requests Android's black surfaces, yellow #F6CC45, Material Outlined icons and motion vocabulary, with a desktop-specific layout. Composition is delegated to the implementer.
## Evidence on Hand
../Android/PRODUCT.md, ui/theme, ui/icons and ui/components/TyuDeviceStage.kt. Android screenshots predate current sans typography; current source is authoritative.
## Users
People using their phone alongside their PC; no narrower audience established.
