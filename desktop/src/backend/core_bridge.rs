//! Adapts domain events to existing Desktop state. Slint owns presentation only.
use super::runtime::BackendRuntime;
use crate::{
    app::{Data, DesktopShell},
    model::{ConnectionState, Device, DeviceStatus, Transfer, TransferDirection, TransferStatus},
    state::AppState,
};
use slint::ComponentHandle;
use std::collections::HashMap;
use tyu_core::*;
pub struct CoreBridge {
    runtime: BackendRuntime,
    peers: HashMap<DeviceId, DeviceInfo>,
    pending: Option<RequestId>,
    mode_session: Option<SessionId>,
    pub pairing_uri: Option<String>,
    pub capabilities: HashMap<DeviceId, Vec<Capability>>,
    send_on_connect: Option<std::path::PathBuf>,
}

impl CoreBridge {
    pub fn start() -> std::io::Result<Self> {
        let args: Vec<_> = std::env::args().collect();
        let data_root = args
            .iter()
            .position(|a| a == "--data-dir")
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from);
        Self::start_in(data_root)
    }
    fn start_in(data_root: Option<std::path::PathBuf>) -> std::io::Result<Self> {
        let args: Vec<_> = std::env::args().collect();
        let send_on_connect = args
            .iter()
            .position(|a| a == "--send-file")
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from);
        Ok(Self {
            runtime: BackendRuntime::start(data_root)?,
            peers: HashMap::new(),
            pending: None,
            mode_session: None,
            pairing_uri: None,
            capabilities: HashMap::new(),
            send_on_connect,
        })
    }
    fn selected(&self, state: &AppState) -> Option<DeviceId> {
        state
            .devices
            .nearby
            .get(state.selected_device)
            .and_then(|d| d.id.parse().ok())
            .map(DeviceId)
    }
    fn send(&self, state: &mut AppState, command: TyuCommand) {
        if let Err(error) = self.runtime.send(command) {
            state.notice = format!("No se pudo enviar la operación: {error}");
        }
    }
    pub fn action(&mut self, command: &str, state: &mut AppState) {
        match command {
            "accept-pair" | "reject-pair" | "close-dialog" => {
                if let Some(request) = self.pending.take() {
                    self.send(
                        state,
                        if command == "accept-pair" {
                            TyuCommand::AcceptPairing { request }
                        } else {
                            TyuCommand::RejectPairing { request }
                        },
                    );
                }
                state.dialog = 0;
                state.pending_name.clear();
            }
            "discover" => self.send(state, TyuCommand::StartDiscovery),
            "connect" | "confirm-pair" => {
                if let Some(peer) = self.selected(state) {
                    self.send(state, TyuCommand::Connect { peer });
                }
            }
            "disconnect" => {
                if let Some(peer) = self.selected(state) {
                    self.send(state, TyuCommand::Disconnect { peer });
                }
            }
            "forget" => {
                if let Some(peer) = self.selected(state) {
                    self.send(state, TyuCommand::ForgetPeer { peer });
                }
            }
            "session" => {
                if let Some(peer) = self.selected(state) {
                    if let Some(session) = self.mode_session {
                        self.send(state, TyuCommand::StopMode { peer, session });
                    } else if state.mode_index() == 2 {
                        self.send(
                            state,
                            TyuCommand::StartMode {
                                peer,
                                mode: tyu_core::TyuMode::Bypass,
                            },
                        );
                    } else {
                        state.notice =
                            "Captura y pantalla virtual aún no disponibles en Windows".into();
                    }
                }
            }
            "add-files" => {
                state.notice = "Usa --send-file <ruta> para enviar un archivo al conectar".into()
            }
            "transfer" | "cancel-transfer" => {
                for item in state.transfers.items.clone() {
                    if let Ok(id) = item.id.parse() {
                        self.send(
                            state,
                            if command == "cancel-transfer" {
                                TyuCommand::CancelTransfer { id: TransferId(id) }
                            } else if state.transfer_paused {
                                TyuCommand::ResumeTransfer { id: TransferId(id) }
                            } else {
                                TyuCommand::PauseTransfer { id: TransferId(id) }
                            },
                        );
                    }
                }
            }
            "clear-history" => state.action(command),
            _ => {}
        }
    }
    pub fn option(&mut self, key: &str, value: i32, state: &mut AppState) {
        if key == "mode"
            && value != state.mode_index()
            && let (Some(peer), Some(session)) = (self.selected(state), self.mode_session.take())
        {
            self.send(state, TyuCommand::StopMode { peer, session });
        }
        state.option(key, value);
    }
    pub fn toggle(&mut self, key: &str, value: bool, state: &mut AppState) {
        match key {
            "camera" | "microphone" => {
                state.notice = "Captura y dispositivos virtuales aún no disponibles".into()
            }
            "storage" => {
                if let Some(peer) = self.selected(state) {
                    self.send(
                        state,
                        TyuCommand::ListStorage {
                            peer,
                            path: "".into(),
                        },
                    );
                }
            }
            _ => state.toggle(key, value),
        }
    }
    pub fn poll(&mut self, state: &mut AppState, window: &DesktopShell) {
        // Bounded work on each native tick; network backpressure lives in Core.
        for _ in 0..128 {
            let Ok(event) = self.runtime.events.try_recv() else {
                break;
            };
            self.event(event, state);
        }
        if let Some(uri) = &self.pairing_uri
            && window.global::<Data>().get_pairing_url() != uri.as_str()
        {
            match qrcode::QrCode::new(uri.as_bytes()) {
                Ok(code) => {
                    let qr = code
                        .render::<image::Luma<u8>>()
                        .min_dimensions(448, 448)
                        .build();
                    let rgb = image::DynamicImage::ImageLuma8(qr).to_rgb8();
                    let pixels = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                    );
                    let data = window.global::<Data>();
                    data.set_pairing_qr(slint::Image::from_rgb8(pixels));
                    data.set_pairing_url(uri.clone().into());
                    data.set_pairing_error("".into());
                }
                Err(error) => window
                    .global::<Data>()
                    .set_pairing_error(format!("No se pudo crear el QR: {error}").into()),
            }
        }
    }
    fn event(&mut self, event: TyuEvent, state: &mut AppState) {
        match event {
            TyuEvent::PairingStarted { ticket } => match ticket.uri() {
                Ok(uri) => {
                    // Explicit opt-in handoff file is useful for a CLI peer on the same workstation.
                    if let Ok(path) = std::env::var("TYU_PAIRING_URI_FILE")
                        && let Err(error) = std::fs::write(path, &uri)
                    {
                        state.notice = format!("No se pudo exportar el QR: {error}");
                    }
                    self.pairing_uri = Some(uri);
                }
                Err(error) => state.notice = error.to_string(),
            },
            TyuEvent::PairingRequest {
                request, device, ..
            } => {
                self.pending = Some(request);
                state.pending_name = device.name;
                state.dialog = 2;
            }
            TyuEvent::PairingRejected { request, .. } if self.pending == Some(request) => {
                self.pending = None;
                state.dialog = 0;
                state.pending_name.clear();
            }
            TyuEvent::DeviceFound { id, .. }
                if !state.devices.nearby.iter().any(|d| d.id == id.to_string()) =>
            {
                state.devices.nearby.push(Device {
                    id: id.to_string(),
                    name: format!("TYU {}", &id.to_string()[..8]),
                    status: DeviceStatus::Available,
                    connection: ConnectionState::Disconnected,
                });
            }
            TyuEvent::DeviceLost { id } => {
                state.devices.nearby.retain(|d| {
                    d.id != id.to_string() || d.connection == ConnectionState::Connected
                });
            }
            TyuEvent::Connected { device } => {
                let id = device.id;
                self.peers.insert(id, device.clone());
                // AppState keeps one selected connection; Core independently supports multiple peers.
                state.connect_peer(device.name, id.to_string());
                state.notice = "Conexión TyuLink autenticada".into();
                if let Some(path) = self.send_on_connect.take() {
                    self.send(
                        state,
                        TyuCommand::SendFile {
                            peer: id,
                            id: TransferId::new(),
                            path,
                        },
                    );
                }
            }
            TyuEvent::Disconnected { peer } => {
                self.peers.remove(&peer);
                self.capabilities.remove(&peer);
                if self.selected(state) == Some(peer) {
                    state.disconnect();
                    self.mode_session = None;
                    state.notice = "Dispositivo desconectado".into();
                }
            }
            TyuEvent::CapabilitiesNegotiated { peer, remote, .. } => {
                self.capabilities.insert(peer, remote);
            }
            TyuEvent::ModeStarted { session, mode, .. } => {
                self.mode_session = Some(session);
                if mode == tyu_core::TyuMode::Bypass {
                    state.mode.active = true;
                }
                state.notice = "Sesión negociada; captura de pantalla aún no disponible".into();
            }
            TyuEvent::ModeStopped { .. } => {
                self.mode_session = None;
                state.mode.active = false;
            }
            TyuEvent::ServiceStarted { service, .. } => {
                state.notice = format!(
                    "{service:?} negociado; el adaptador de plataforma aún no está disponible"
                )
            }
            TyuEvent::TransferOffered { metadata, .. }
                if !state
                    .transfers
                    .items
                    .iter()
                    .any(|t| t.id == metadata.id.to_string()) =>
            {
                state.transfers.items.push(Transfer {
                    id: metadata.id.to_string(),
                    file_name: metadata.name,
                    direction: TransferDirection::Receive,
                    total_bytes: metadata.size,
                    transferred_bytes: 0,
                    status: TransferStatus::InProgress,
                });
            }
            TyuEvent::TransferProgress {
                id, bytes, total, ..
            } => {
                if let Some(t) = state
                    .transfers
                    .items
                    .iter_mut()
                    .find(|t| t.id == id.to_string())
                {
                    t.transferred_bytes = bytes;
                    t.total_bytes = total;
                    t.status = TransferStatus::InProgress;
                }
                state.transfer_running = true;
                state.transfer_paused = false;
            }
            TyuEvent::TransferComplete { id, .. } => {
                if let Some(t) = state
                    .transfers
                    .items
                    .iter_mut()
                    .find(|t| t.id == id.to_string())
                {
                    t.transferred_bytes = t.total_bytes;
                    t.status = TransferStatus::Completed;
                }
                state.transfer_running = false;
                state.notice = "Archivo recibido o enviado con integridad verificada".into();
            }
            TyuEvent::TransferPaused { .. } => state.transfer_paused = true,
            TyuEvent::TransferFailed { id, code, .. } => {
                if let Some(t) = state
                    .transfers
                    .items
                    .iter_mut()
                    .find(|t| t.id == id.to_string())
                {
                    t.status = TransferStatus::Failed(code.to_string());
                }
                state.transfer_running = false;
                state.notice = format!("Transferencia: {code}");
            }
            TyuEvent::StorageResult { result, .. } => {
                state.notice = match result {
                    Ok(StorageReply::Entries(e)) => {
                        format!("Almacenamiento remoto: {} elementos", e.len())
                    }
                    Ok(StorageReply::Data(d)) => format!("Leídos {} bytes remotos", d.len()),
                    Ok(StorageReply::Done) => "Operación remota completada".into(),
                    Err(e) => format!("Almacenamiento: {e}"),
                }
            }
            TyuEvent::ClipboardReceived { text, .. } => {
                state.notice = format!(
                    "Texto recibido: {} bytes; integración del portapapeles pendiente",
                    text.len()
                )
            }
            TyuEvent::Error { code, .. } | TyuEvent::ConnectionFailed { code, .. } => {
                state.notice = format!("TYU: {code}")
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};
    use tyu_core::identity::{MemoryStore, PairingTicket};
    async fn next(
        bridge: &mut CoreBridge,
        state: &mut AppState,
        predicate: impl Fn(&TyuEvent) -> bool,
    ) -> TyuEvent {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let event = bridge.runtime.events.recv().await.unwrap();
                let matched = predicate(&event);
                bridge.event(event.clone(), state);
                if matched {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bridge_approves_real_peer_and_updates_existing_app_state() {
        let root = tempfile::tempdir().unwrap();
        let mut bridge = CoreBridge::start_in(Some(root.path().to_owned())).unwrap();
        let mut state = AppState::desktop();
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::PairingStarted { .. })
        })
        .await;
        let ticket = PairingTicket::parse(bridge.pairing_uri.as_ref().unwrap()).unwrap();
        let mut config = NodeConfig::new(
            "Native peer",
            DevicePlatform::Android,
            Arc::new(MemoryStore::default()),
        );
        config.capabilities = vec![Capability::FileSend, Capability::ClipboardSource];
        let mut peer = TyuNode::start(config).await.unwrap();
        peer.command(TyuCommand::Pair { ticket }).await.unwrap();
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::PairingRequest { .. })
        })
        .await;
        assert_eq!(state.dialog, 2);
        assert!(!state.connected());
        let pending = bridge.pending;
        // QR renewal or an older cancelled request must not dismiss this approval.
        bridge.event(TyuEvent::PairingExpired, &mut state);
        bridge.event(
            TyuEvent::PairingRejected {
                request: RequestId::new(),
                code: TyuErrorCode::Expired,
            },
            &mut state,
        );
        assert_eq!(bridge.pending, pending);
        assert_eq!(state.dialog, 2);
        bridge.action("accept-pair", &mut state);
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::Connected { .. })
        })
        .await;
        assert!(state.connected());
        assert_eq!(state.device_name(), "Native peer");
        let desktop = loop {
            let event = tokio::time::timeout(Duration::from_secs(10), peer.events.recv())
                .await
                .unwrap()
                .unwrap();
            if let TyuEvent::Connected { device } = event {
                break device.id;
            }
        };
        peer.command(TyuCommand::SendClipboard {
            peer: desktop,
            text: "native control".into(),
        })
        .await
        .unwrap();
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::ClipboardReceived { .. })
        })
        .await;
        assert!(state.notice.contains("14 bytes"));
        bridge.action("session", &mut state);
        assert!(!state.mode.active);
        bridge.toggle("camera", true, &mut state);
        assert!(!state.camera);
        bridge.action("disconnect", &mut state);
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::Disconnected { .. })
        })
        .await;
        assert!(!state.connected());
        peer.shutdown().await.unwrap();
        drop(bridge);
    }
}
