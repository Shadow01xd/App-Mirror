# Tyu Desktop

Native Windows desktop client built with Rust and Slint.

This module builds independently from `../Android`. It shares TYU colors and the
Material icon family, with its own desktop composition and connection service.

## Structure

- `src/navigation`: routes and navigation history.
- `src/state`: application-facing state containers.
- `src/model`: platform-neutral desktop domain types.
- `src/mock`: deterministic fixtures for development and tests only.
- `src/pairing.rs`: local HTTP pairing service and approval state.
- `src/pairing.html`: phone browser connection page served by the desktop.
- `src/ui/theme`: visual tokens.
- `src/ui/components`: reusable Slint primitives.
- `src/ui/layouts`: desktop composition shells.
- `src/ui/screens`: route-level Slint surfaces.

## Run

```text
cargo run
```

## Verify

```text
cargo fmt --check
cargo test
```

## Connecting a phone

The application starts with no connected devices. Scan the QR using the phone's
camera, open the link, enter the phone name and choose **Solicitar conexión**.
The desktop displays the supplied name and **Aceptar / Rechazar**.
Accepting establishes the local pairing session and opens the device workspace.
Rejecting keeps the current desktop page and issues a new QR. There are no
automatically connected sample devices.

Both devices must be on the same private LAN. The server binds only to the
selected private IPv4 interface and a free port. Windows Firewall may require
allowing the executable on private networks; TYU does not change firewall rules.
If the PC has multiple network adapters, its default route determines the address.

QR tickets expire after ten minutes, requests after two minutes. The phone
checks its session every two seconds; closing the page or losing contact
disconnects it after thirty seconds. Keep the phone page open while connected.
Tickets rotate after accept, reject and disconnect.

This is a local HTTP pairing channel, not encrypted screen transport. Only the
name, request and connection status are exchanged. Screen extension, mirroring,
camera/audio capture and file transport are not implemented. The three mode
buttons currently select their desktop views and animate connection direction.
The existing Android app has no integration with this service yet; scanning
uses the phone browser and does not modify Android code.

## UI validation

`cargo test` covers approval/rejection, token replay, expiry, lost heartbeat,
HTTP endpoints and desktop state. `cargo run -- --smoke` exercises native UI
callbacks against the local pairing service.

Capture with the software renderer (PowerShell):

```powershell
$env:SLINT_BACKEND = 'winit-software'
$env:SLINT_SCALE_FACTOR = '1'
.\target\debug\tyu-desktop.exe --screenshot .impeccable/review/empty-v3.png --width 1260 --height 820
.\target\debug\tyu-desktop.exe --view connected --screenshot .impeccable/review/connected-v3.png --width 1260 --height 820
```

`--view connected|mirror|bypass|pairing` is an explicit screenshot fixture;
normal startup never uses those fixture devices.
