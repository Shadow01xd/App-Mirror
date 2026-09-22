//! Adapts domain events to existing Desktop state. Slint owns presentation only.
use super::components;
use super::downloads;
use super::runtime::BackendRuntime;
use super::usb;
use super::virtual_display::VirtualDisplay;
use super::windows_capture;
use crate::{
    app::{Data, DesktopShell, MirrorWindow},
    model::{ConnectionState, Device, DeviceStatus, Transfer, TransferDirection, TransferStatus},
    state::AppState,
};
use slint::ComponentHandle;
use std::collections::HashMap;
use tyu_core::*;
pub struct CoreBridge {
    runtime: BackendRuntime,
    bypass_sessions: HashMap<ServiceKind, SessionId>,
    peers: HashMap<DeviceId, DeviceInfo>,
    pending: Option<RequestId>,
    mode_session: Option<SessionId>,
    active_mode: Option<TyuMode>,
    /// Incoming video stream of the Mirror session, for keyframe requests.
    mirror_stream: Option<StreamId>,
    pub pairing_uri: Option<String>,
    pub capabilities: HashMap<DeviceId, Vec<Capability>>,
    send_on_connect: Option<std::path::PathBuf>,
    mirror_reset: bool,
    monitor_capture: Option<windows_capture::CaptureHandle>,
    /// Plugged while a Monitor session runs and the Parsec VDD driver is installed.
    virtual_display: Option<VirtualDisplay>,
    /// Detached phone-screen window; created on first "Abrir en ventana", hidden when closed.
    popout: Option<MirrorWindow>,
    popout_visible: bool,
    /// Frame size the detached window was last sized for.
    popout_size: Option<(u32, u32)>,
    /// Touches from the detached window, drained on each poll.
    popout_input: (
        std::sync::mpsc::Sender<Touch>,
        std::sync::mpsc::Receiver<Touch>,
    ),
    /// A winget install of an optional component running in the background.
    install: Option<components::Install>,
    /// Phones appearing on and leaving the USB cable.
    usb_events: std::sync::mpsc::Receiver<usb::UsbEvent>,
    /// The tunnel to the phone currently on the cable; dropping it closes the forward.
    cable: Option<usb::Cable>,
}
/// (kind, normalized x, normalized y) as emitted by the mirror surfaces.
type Touch = (String, f32, f32);

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
            bypass_sessions: HashMap::new(),
            peers: HashMap::new(),
            pending: None,
            mode_session: None,
            active_mode: None,
            mirror_stream: None,
            pairing_uri: None,
            capabilities: HashMap::new(),
            send_on_connect,
            mirror_reset: false,
            monitor_capture: None,
            virtual_display: None,
            popout: None,
            popout_visible: false,
            popout_size: None,
            popout_input: std::sync::mpsc::channel(),
            install: None,
            usb_events: usb::watch(),
            cable: None,
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
    /// The phone an action should act on. Anything that operates over a live link (sending a
    /// file, opening its storage, hanging up) means *the connected* phone — which is not always
    /// the row highlighted in the sidebar, and getting that wrong made those buttons do nothing.
    fn active(&self, state: &AppState) -> Option<DeviceId> {
        state
            .devices
            .nearby
            .iter()
            .find(|d| d.connection == ConnectionState::Connected)
            .and_then(|d| d.id.parse().ok())
            .map(DeviceId)
            .or_else(|| self.selected(state))
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
            "install-display" | "install-audio" if self.install.is_none() => {
                let component = if command == "install-display" {
                    components::Component::Display
                } else {
                    components::Component::AudioCable
                };
                state.installing = component.name().into();
                state.notice = format!(
                    "Descargando el {} del fabricante; confirma el aviso de Windows",
                    component.name()
                );
                self.install = Some(components::install(component));
            }
            "connect" | "confirm-pair" => {
                if let Some(peer) = self.selected(state) {
                    self.send(state, TyuCommand::Connect { peer });
                }
            }
            "disconnect" => {
                if let Some(peer) = self.active(state) {
                    self.send(state, TyuCommand::Disconnect { peer });
                }
            }
            // Hang up whatever link is up and come back over the cable, for someone on Wi-Fi who
            // just plugged the phone in.
            "use-cable" => match usb::devices_ready().first() {
                Some(serial) => {
                    let serial = serial.clone();
                    if let Some(peer) = self.active(state) {
                        self.send(state, TyuCommand::Disconnect { peer });
                    }
                    self.cable = None;
                    self.cable_event(usb::UsbEvent::Attached(serial), state);
                }
                None => {
                    state.notice = if usb::adb().is_none() {
                        "Para el cable hace falta adb; instálalo o usa Wi-Fi".into()
                    } else {
                        "No hay ningún teléfono con depuración USB en el cable".into()
                    }
                }
            },
            "forget" => {
                if let Some(peer) = self.selected(state) {
                    self.send(state, TyuCommand::ForgetPeer { peer });
                }
            }
            "session" => {
                if let Some(peer) = self.selected(state) {
                    if let Some(session) = self.mode_session {
                        self.send(state, TyuCommand::StopMode { peer, session });
                    } else {
                        let mode = match state.mode_index() {
                            0 => tyu_core::TyuMode::Monitor,
                            1 => tyu_core::TyuMode::Mirror,
                            _ => tyu_core::TyuMode::Bypass,
                        };
                        self.send(state, TyuCommand::StartMode { peer, mode });
                    }
                }
            }
            "popout" => {
                if self.popout.is_none() {
                    match MirrorWindow::new() {
                        Ok(window) => {
                            let tx = self.popout_input.0.clone();
                            window.on_input(move |kind, x, y| {
                                let _ = tx.send((kind.to_string(), x, y));
                            });
                            let keys = self.popout_input.0.clone();
                            window.on_key(move |text| {
                                let _ = keys.send((format!("key:{text}"), 0.0, 0.0));
                            });
                            let closed = self.popout_input.0.clone();
                            window.window().on_close_requested(move || {
                                let _ = closed.send(("closed".into(), 0.0, 0.0));
                                slint::CloseRequestResponse::HideWindow
                            });
                            self.popout = Some(window)
                        }
                        Err(error) => {
                            state.notice = format!("No se pudo abrir la ventana: {error}")
                        }
                    }
                }
                if let Some(window) = &self.popout {
                    window.set_device_name(state.device_name().into());
                    match window.show() {
                        Ok(()) => self.popout_visible = true,
                        Err(error) => {
                            state.notice = format!("No se pudo abrir la ventana: {error}")
                        }
                    }
                }
            }
            "popin" => self.hide_popout(),
            "open-storage" => {
                if let Err(error) = self.runtime.storage.open() {
                    state.notice = error.to_string();
                }
            }
            "add-files" => {
                let Some(peer) = self.active(state) else {
                    state.notice = "Conecta un teléfono para enviarle archivos".into();
                    return;
                };
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("Enviar archivos al teléfono")
                    .pick_files()
                {
                    let count = paths.len();
                    for path in paths {
                        let id = TransferId::new();
                        state.transfers.items.push(Transfer {
                            id: id.to_string(),
                            file_name: path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            direction: TransferDirection::Send,
                            total_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
                            transferred_bytes: 0,
                            status: TransferStatus::Queued,
                            path: Some(path.to_string_lossy().into_owned()),
                        });
                        self.send(state, TyuCommand::SendFile { peer, id, path });
                    }
                    state.notice = match count {
                        1 => "Enviando 1 archivo al teléfono".into(),
                        n => format!("Enviando {n} archivos al teléfono"),
                    };
                }
            }
            "dest-downloads" | "dest-documents" | "dest-choose" => {
                let chosen = match command {
                    "dest-downloads" => Some(downloads::downloads()),
                    "dest-documents" => Some(downloads::documents()),
                    _ => rfd::FileDialog::new()
                        .set_title("¿Dónde quieres guardar lo que recibas?")
                        .set_directory(downloads::destination())
                        .pick_folder(),
                };
                if let Some(path) = chosen {
                    state.notice = match downloads::set_destination(&path) {
                        Ok(()) => {
                            state.receive_dir = path.to_string_lossy().into_owned();
                            format!("Lo que recibas se guardará en {}", state.receive_dir)
                        }
                        Err(error) => format!("No se pudo guardar la carpeta: {error}"),
                    };
                }
            }
            "open-destination" => {
                let folder = std::path::PathBuf::from(&state.receive_dir);
                let _ = std::fs::create_dir_all(&folder);
                if let Err(error) = downloads::open(&folder) {
                    state.notice = format!("No se pudo abrir la carpeta: {error}");
                }
            }
            // Everything that is not still moving: finished, failed and never-started alike.
            // Only clearing completed ones left failures stuck on screen for good.
            "clear-transfers" => {
                state
                    .transfers
                    .items
                    .retain(|t| t.status == TransferStatus::InProgress);
                state.notice = "Lista de transferencias limpiada".into();
            }
            // Per-file buttons carry the transfer id: "open:<id>" and "reveal:<id>".
            _ if command.starts_with("open:") || command.starts_with("reveal:") => {
                let (verb, id) = command.split_once(':').unwrap_or_default();
                let target = state
                    .transfers
                    .items
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(|t| t.path.clone());
                match target {
                    Some(path) => {
                        let path = std::path::PathBuf::from(path);
                        let result = if verb == "open" {
                            downloads::open(&path)
                        } else {
                            downloads::reveal(&path)
                        };
                        if let Err(error) = result {
                            state.notice = format!("No se pudo abrir: {error}");
                        }
                    }
                    None => state.notice = "Ese archivo ya no está en este equipo".into(),
                }
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
            "camera" | "microphone" | "storage" => {
                if let Some(peer) = self.selected(state) {
                    let service = match key {
                        "camera" => ServiceKind::Camera,
                        "microphone" => ServiceKind::Microphone,
                        _ => ServiceKind::Storage,
                    };
                    if value {
                        self.send(state, TyuCommand::StartService { peer, service });
                        state.notice = "Confirma el permiso en el teléfono".into();
                    } else if let Some(session) = self.bypass_sessions.get(&service).copied() {
                        self.send(state, TyuCommand::StopService { peer, session });
                    }
                }
            }
            _ => state.toggle(key, value),
        }
    }
    /// A phone plugged into (or pulled from) the USB cable. With debugging authorised, TYU
    /// tunnels straight to it and connects on its own — the phone only has to be paired once,
    /// over Wi-Fi, because the cable carries no way to prove a new identity.
    fn cable_event(&mut self, event: usb::UsbEvent, state: &mut AppState) {
        match event {
            usb::UsbEvent::Attached(serial) => {
                // Already linked over Wi-Fi: leave it alone. "Usar cable" is how someone asks
                // to move a working connection onto the wire.
                if state
                    .devices
                    .nearby
                    .iter()
                    .any(|d| d.connection == ConnectionState::Connected)
                {
                    state.notice =
                        "Teléfono en el cable. Pulsa \"Usar cable\" para cambiar de Wi-Fi a USB"
                            .into();
                    return;
                }
                match usb::Cable::open(&serial) {
                    Ok(cable) => match cable.peer.parse() {
                        Ok(id) => {
                            // An unpaired phone is refused by the core itself, and the error
                            // reaches the user through the usual event path.
                            state.notice = "Teléfono en el cable: conectando por USB…".into();
                            let endpoint = cable.endpoint;
                            self.cable = Some(cable);
                            self.send(
                                state,
                                TyuCommand::ConnectAt {
                                    peer: DeviceId(id),
                                    endpoint,
                                },
                            );
                        }
                        Err(_) => state.notice = "El teléfono envió una identidad inválida".into(),
                    },
                    Err(error) => state.notice = error,
                }
            }
            usb::UsbEvent::Unauthorized(_) => {
                state.notice =
                    "Teléfono conectado por cable: acepta la depuración USB en su pantalla".into();
            }
            usb::UsbEvent::Detached(_) => {
                if self.cable.take().is_some() {
                    state.notice = "Cable desconectado".into();
                }
            }
        }
    }
    /// Re-reads what lives outside the app: the saved download folder and which optional
    /// Windows components are installed. Each component check touches the device stack, so this
    /// runs on demand (startup, after an install) and never on the frame timer.
    pub fn refresh_settings(&self, state: &mut AppState) {
        state.receive_dir = downloads::destination().display().to_string();
        state.needs_display = !components::Component::Display.present();
        state.needs_audio = !components::Component::AudioCable.present();
    }
    fn hide_popout(&mut self) {
        self.popout_visible = false;
        if let Some(window) = &self.popout {
            let _ = window.hide();
        }
    }
    /// Mouse on the mirrored picture → touch on the phone (Mirror session only).
    pub fn input(&mut self, kind: &str, x: f32, y: f32, state: &mut AppState) {
        if self.active_mode != Some(tyu_core::TyuMode::Mirror) {
            return;
        }
        let (Some(peer), Some(session)) = (self.selected(state), self.mode_session) else {
            return;
        };
        let event = match kind {
            "down" => InputEvent::TouchDown { contact: 0, x, y },
            "move" => InputEvent::TouchMove { contact: 0, x, y },
            "up" => InputEvent::TouchUp { contact: 0, x, y },
            // Keyboard: Slint key text. Special keys map to Android key codes the phone
            // understands; everything else is typed as text.
            key if key.starts_with("key:") => match &key[4..] {
                "\u{8}" => InputEvent::KeyDown { code: 67 }, // KEYCODE_DEL (backspace)
                "\n" => InputEvent::KeyDown { code: 66 },    // KEYCODE_ENTER
                "\u{1b}" => InputEvent::KeyDown { code: 4 }, // KEYCODE_BACK
                "\u{7f}" => InputEvent::KeyDown { code: 112 }, // KEYCODE_FORWARD_DEL
                // Slint reports modifier/function keys as private-use code points; only real
                // characters are typed.
                text if !text.is_empty()
                    && !text
                        .chars()
                        .any(|c| c.is_control() || ('\u{e000}'..='\u{f8ff}').contains(&c)) =>
                {
                    InputEvent::TextInput(text.to_string())
                }
                _ => return,
            },
            _ => return,
        };
        self.send(
            state,
            TyuCommand::SendInput {
                peer,
                session,
                event,
            },
        );
    }
    pub fn poll(&mut self, state: &mut AppState, window: &DesktopShell) {
        // Bounded work on each native tick; network backpressure lives in Core.
        for _ in 0..128 {
            let Ok(event) = self.runtime.events.try_recv() else {
                break;
            };
            self.event(event, state);
        }
        while let Ok(event) = self.usb_events.try_recv() {
            self.cable_event(event, state);
        }
        if let Some(result) = self.runtime.storage.poll_mount() {
            state.storage_pending = false;
            state.storage = result.is_ok();
            state.notice = match result {
                Ok(message) => message,
                Err(error) => format!(
                    "No se pudo abrir el almacenamiento: {error}. Revisa el permiso de archivos en Android y vuelve a activarlo."
                ),
            };
            if !state.storage
                && let Some(session) = self.bypass_sessions.get(&ServiceKind::Storage).copied()
                && let Some(peer) = self.selected(state)
            {
                self.send(state, TyuCommand::StopService { peer, session });
            }
        }
        if let Some(install) = &self.install
            && let Some(result) = install.finished()
        {
            let component = install.component;
            self.install = None;
            state.installing.clear();
            state.notice = match result {
                Ok(()) => format!("Listo: {} instalado", component.name()),
                Err(error) => error,
            };
            self.refresh_settings(state);
        }
        while let Ok(status) = self.runtime.bypass_status.try_recv() {
            let current = self.bypass_sessions.get(&status.service);
            // A failing receiver queues StopService too; preserve its error after that event is drained.
            if current != Some(&status.session)
                && (status.active
                    || current.is_some()
                    || !state
                        .devices
                        .nearby
                        .iter()
                        .any(|d| d.connection == ConnectionState::Connected))
            {
                continue;
            }
            match status.service {
                ServiceKind::Camera => state.camera = status.active,
                ServiceKind::Microphone => state.microphone = status.active,
                _ => {}
            }
            state.notice = status.message;
        }
        while let Ok((kind, x, y)) = self.popout_input.1.try_recv() {
            if kind == "closed" {
                self.popout_visible = false;
            } else {
                self.input(&kind, x, y, state);
            }
        }
        // The decoder skipped ahead to cut latency and needs a fresh keyframe from the phone.
        if self.runtime.decoder_needs_keyframe()
            && let (Some(peer), Some(session), Some(stream)) =
                (self.selected(state), self.mode_session, self.mirror_stream)
        {
            let mut frame = Frame::new(Message::MediaKeyframeRequest { stream });
            frame.session = Some(session);
            self.send(state, TyuCommand::Request { peer, frame });
        }
        window.global::<Data>().set_popout_open(self.popout_visible);
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
        if let Some((w, h, rgb)) = self.runtime.take_frame() {
            let pixels = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(&rgb, w, h);
            let image = slint::Image::from_rgb8(pixels);
            // Only the visible surface gets the (large) texture upload each frame.
            if self.popout_visible
                && let Some(popout) = &self.popout
            {
                if self.popout_size != Some((w, h)) {
                    self.popout_size = Some((w, h));
                    let height = 900.0f32;
                    let width = (height * w as f32 / h.max(1) as f32).max(320.0);
                    popout
                        .window()
                        .set_size(slint::LogicalSize::new(width, height));
                }
                popout.set_frame(image);
            } else {
                window.global::<Data>().set_mirror_frame(image);
            }
        } else if self.mirror_reset {
            self.mirror_reset = false;
            window
                .global::<Data>()
                .set_mirror_frame(slint::Image::default());
            if let Some(popout) = &self.popout {
                popout.set_frame(slint::Image::default());
            }
            self.hide_popout();
            self.popout_size = None;
        }
    }
    fn event(&mut self, event: TyuEvent, state: &mut AppState) {
        match &event {
            TyuEvent::ServiceStarted {
                peer,
                session,
                service,
            } => {
                self.bypass_sessions.insert(*service, *session);
                if *service == ServiceKind::Storage {
                    let name = self
                        .peers
                        .get(peer)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Android");
                    match self.runtime.storage.mount(*peer, name) {
                        Ok(()) => {
                            state.storage = false;
                            state.storage_pending = true;
                            state.notice = "Confirma el permiso de archivos en Android. Preparando el acceso al teléfono…".into();
                        }
                        Err(e) => state.notice = format!("No se pudo montar el teléfono: {e}"),
                    }
                } else {
                    state.notice = "Esperando la captura del teléfono".into();
                }
                return;
            }
            TyuEvent::ServiceStopped { session, .. } => {
                let service = self
                    .bypass_sessions
                    .iter()
                    .find(|(_, s)| *s == session)
                    .map(|(kind, _)| *kind);
                if let Some(service) = service {
                    self.bypass_sessions.remove(&service);
                    match service {
                        ServiceKind::Camera => state.camera = false,
                        ServiceKind::Microphone => state.microphone = false,
                        ServiceKind::Storage => {
                            state.storage = false;
                            state.storage_pending = false;
                            self.runtime.storage.unmount();
                        }
                        _ => {}
                    }
                }
                return;
            }
            TyuEvent::Disconnected { .. } => {
                self.bypass_sessions.clear();
                self.runtime.storage.unmount();
            }
            _ => {}
        }
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
            TyuEvent::Connected { device, address } => {
                let id = device.id;
                self.peers.insert(id, device.clone());
                // AppState keeps one selected connection; Core independently supports multiple peers.
                state.connect_peer(device.name, id.to_string());
                state.link = super::network::link_label(address.ip()).into();
                state.notice = format!("Conexión TyuLink autenticada por {}", state.link);
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
                    state.link = "Wi-Fi".into();
                    self.mode_session = None;
                    self.active_mode = None;
                    state.notice = "Dispositivo desconectado".into();
                    if state.page == 5 {
                        state.option("page", 0);
                    }
                    self.runtime.reset_decoder();
                    self.mirror_reset = true;
                    if let Some(capture) = self.monitor_capture.take() {
                        capture.stop();
                    }
                    self.virtual_display = None;
                }
            }
            TyuEvent::CapabilitiesNegotiated { peer, remote, .. } => {
                self.capabilities.insert(peer, remote);
            }
            TyuEvent::ModeStarted {
                peer,
                session,
                mode,
            } => {
                self.mode_session = Some(session);
                self.active_mode = Some(mode);
                match mode {
                    tyu_core::TyuMode::Bypass => state.mode.active = true,
                    tyu_core::TyuMode::Mirror => {
                        state.mode.active = true;
                        state.option("page", 5);
                    }
                    tyu_core::TyuMode::Monitor => {
                        // Desktop is the only side with MonitorSource; always publish this
                        // computer's own screen once a Monitor session negotiates, regardless
                        // of which side called StartMode first.
                        state.mode.active = true;
                        // With the Parsec VDD driver the phone becomes a real extended monitor;
                        // without it the primary display is duplicated.
                        self.virtual_display = VirtualDisplay::create();
                        let region = self.virtual_display.as_ref().map(|d| d.region);
                        let (width, height) = windows_capture::transmit_size(region);
                        let mut frame = Frame::new(Message::MediaStart {
                            stream: StreamId::new(),
                            format: MediaMetadata::Video(VideoFormat {
                                width,
                                height,
                                fps: 60,
                                bitrate: 8_000_000,
                                codec: VideoCodec::H264,
                                orientation: Orientation::Landscape,
                            }),
                        });
                        frame.session = Some(session);
                        self.send(state, TyuCommand::Request { peer, frame });
                    }
                }
                state.notice = match mode {
                    tyu_core::TyuMode::Mirror => {
                        "Sesión de espejo negociada; esperando video del teléfono".into()
                    }
                    tyu_core::TyuMode::Monitor if self.virtual_display.is_some() => {
                        "Monitor virtual conectado: el teléfono es ahora una pantalla extendida"
                            .into()
                    }
                    tyu_core::TyuMode::Monitor => {
                        "Duplicando la pantalla principal (instala Parsec VDD para escritorio extendido)".into()
                    }
                    tyu_core::TyuMode::Bypass => "Sesión negociada".into(),
                };
            }
            TyuEvent::MediaStarted {
                peer,
                session,
                stream,
                ..
            } if self.active_mode == Some(tyu_core::TyuMode::Monitor)
                && Some(session) == self.mode_session =>
            {
                if let Some(capture) = self.monitor_capture.take() {
                    capture.stop();
                }
                self.monitor_capture = Some(windows_capture::start(
                    self.runtime.media_sender(),
                    peer,
                    session,
                    stream,
                    self.virtual_display.as_ref().map(|d| d.region),
                    60,
                    8_000_000,
                ));
                state.notice = if self.virtual_display.is_some() {
                    "Transmitiendo el monitor virtual al teléfono".into()
                } else {
                    "Compartiendo esta pantalla con el teléfono".into()
                };
            }
            TyuEvent::MediaStarted { stream, .. }
                if self.active_mode == Some(tyu_core::TyuMode::Mirror) =>
            {
                self.mirror_stream = Some(stream);
                state.notice = "El teléfono empezó a transmitir; recibiendo video".into();
            }
            TyuEvent::KeyframeRequested { .. } => {
                if let Some(capture) = &self.monitor_capture {
                    capture.request_keyframe();
                }
            }
            TyuEvent::ModeStopped { .. } => {
                self.mode_session = None;
                self.active_mode = None;
                self.mirror_stream = None;
                state.mode.active = false;
                if state.page == 5 {
                    state.option("page", 0);
                }
                self.runtime.reset_decoder();
                self.mirror_reset = true;
                if let Some(capture) = self.monitor_capture.take() {
                    capture.stop();
                }
                self.virtual_display = None;
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
                state.notice = format!("Recibiendo {} del teléfono", metadata.name);
                state.transfers.items.push(Transfer {
                    id: metadata.id.to_string(),
                    file_name: metadata.name,
                    direction: TransferDirection::Receive,
                    total_bytes: metadata.size,
                    transferred_bytes: 0,
                    status: TransferStatus::InProgress,
                    path: None,
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
                let mut message = "Transferencia completada".to_string();
                if let Some(t) = state
                    .transfers
                    .items
                    .iter_mut()
                    .find(|t| t.id == id.to_string())
                {
                    t.transferred_bytes = t.total_bytes;
                    t.status = TransferStatus::Completed;
                    message = match t.direction {
                        TransferDirection::Send => format!("{} enviado al teléfono", t.file_name),
                        // The core drops downloads in its own folder; move it where the user
                        // asked, and remember the final path so the app can open it.
                        TransferDirection::Receive => match downloads::deliver(&t.file_name) {
                            Ok(path) => {
                                let shown = path.display().to_string();
                                t.path = Some(shown.clone());
                                format!("{} guardado en {shown}", t.file_name)
                            }
                            Err(error) => {
                                t.status = TransferStatus::Failed("No se pudo guardar".into());
                                format!("{} llegó pero no se pudo guardar: {error}", t.file_name)
                            }
                        },
                    };
                }
                state.transfer_running = false;
                state.notice = message;
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
            if let TyuEvent::Connected { device, .. } = event {
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

    fn phone_config(name: &str) -> NodeConfig {
        use Capability::*;
        let mut config = NodeConfig::new(
            name,
            DevicePlatform::Android,
            Arc::new(MemoryStore::default()),
        );
        config.capabilities = vec![MirrorSource, MonitorReceiver, FileSend, ClipboardSource];
        config
    }
    /// Pairs a phone-like node with the bridge and returns the phone plus the desktop's id.
    async fn connect_phone(
        bridge: &mut CoreBridge,
        state: &mut AppState,
        name: &str,
    ) -> (TyuNode, DeviceId) {
        next(bridge, state, |e| {
            matches!(e, TyuEvent::PairingStarted { .. })
        })
        .await;
        let ticket = PairingTicket::parse(bridge.pairing_uri.as_ref().unwrap()).unwrap();
        let mut phone = TyuNode::start(phone_config(name)).await.unwrap();
        phone.command(TyuCommand::Pair { ticket }).await.unwrap();
        next(bridge, state, |e| {
            matches!(e, TyuEvent::PairingRequest { .. })
        })
        .await;
        bridge.action("accept-pair", state);
        next(bridge, state, |e| matches!(e, TyuEvent::Connected { .. })).await;
        let desktop = phone_event(&mut phone, |e| matches!(e, TyuEvent::Connected { .. })).await;
        let TyuEvent::Connected { device, .. } = desktop else {
            unreachable!()
        };
        (phone, device.id)
    }
    async fn phone_event(phone: &mut TyuNode, predicate: impl Fn(&TyuEvent) -> bool) -> TyuEvent {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let event = phone.events.recv().await.unwrap();
                if predicate(&event) {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }
    /// A solid-colour picture encoded with the same OpenH264 build the app ships.
    fn encoded_test_frame(width: usize, height: usize, rgb: [u8; 3]) -> Vec<u8> {
        use openh264::{
            encoder::{Encoder, EncoderConfig},
            formats::{RgbSliceU8, YUVBuffer},
        };
        let pixels: Vec<u8> = rgb
            .iter()
            .copied()
            .cycle()
            .take(width * height * 3)
            .collect();
        let yuv = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&pixels, (width, height)));
        let mut encoder =
            Encoder::with_api_config(openh264::OpenH264API::from_source(), EncoderConfig::new())
                .unwrap();
        encoder.encode(&yuv).unwrap().to_vec()
    }
    async fn wait_for_frame(bridge: &mut CoreBridge, state: &mut AppState) -> (u32, u32, Vec<u8>) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            while let Ok(event) = bridge.runtime.events.try_recv() {
                bridge.event(event, state);
            }
            if let Some(frame) = bridge.runtime.take_frame() {
                return frame;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no decoded frame arrived"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mirror_real_h264_from_the_phone_is_decoded_and_displayed() {
        let root = tempfile::tempdir().unwrap();
        let mut bridge = CoreBridge::start_in(Some(root.path().to_owned())).unwrap();
        let mut state = AppState::desktop();
        let (mut phone, desktop) = connect_phone(&mut bridge, &mut state, "Phone").await;
        // Same path the real UI takes: pick "Espejo", press "Iniciar".
        state.option("mode", 1);
        bridge.action("session", &mut state);
        let TyuEvent::ModeStarted { session, .. } = phone_event(&mut phone, |e| {
            matches!(
                e,
                TyuEvent::ModeStarted {
                    mode: TyuMode::Mirror,
                    ..
                }
            )
        })
        .await
        else {
            unreachable!()
        };
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::ModeStarted { .. })
        })
        .await;
        assert!(state.mode.active);
        let stream = StreamId::new();
        let mut frame = Frame::new(Message::MediaStart {
            stream,
            format: MediaMetadata::Video(VideoFormat {
                width: 320,
                height: 240,
                fps: 30,
                bitrate: 1_000_000,
                codec: VideoCodec::H264,
                orientation: Orientation::Portrait,
            }),
        });
        frame.session = Some(session);
        phone
            .command(TyuCommand::Request {
                peer: desktop,
                frame,
            })
            .await
            .unwrap();
        phone_event(&mut phone, |e| matches!(e, TyuEvent::MediaStarted { .. })).await;
        // A short stream rather than a single picture: hardware-style decoders hold a frame or
        // two before emitting the first one.
        let encoded: bytes::Bytes = encoded_test_frame(320, 240, [200, 30, 30]).into();
        for frame_id in 1..=6u64 {
            for packet in media::packetize(
                session,
                stream,
                frame_id,
                frame_id * 16_667,
                true,
                encoded.clone(),
                1200,
            )
            .unwrap()
            {
                phone.send_media(desktop, packet).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        let (w, h, rgb) = wait_for_frame(&mut bridge, &mut state).await;
        assert_eq!((w, h), (320, 240));
        // Centre pixel is clearly red after a lossy round trip.
        let centre = ((120 * 320 + 160) * 3) as usize;
        assert!(
            rgb[centre] > 150 && rgb[centre + 1] < 90 && rgb[centre + 2] < 90,
            "{:?}",
            &rgb[centre..centre + 3]
        );
        bridge.action("session", &mut state);
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::ModeStopped { .. })
        })
        .await;
        assert!(!state.mode.active);
        phone.shutdown().await.unwrap();
        drop(bridge);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::await_holding_lock)] // the whole test owns the display
    async fn monitor_captures_this_screen_and_the_phone_can_decode_it() {
        let _display = windows_capture::tests::DISPLAY
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let (width, height) = windows_capture::screen_size();
        if width < 16 || height < 16 {
            eprintln!("no interactive desktop; skipping GDI capture test");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let mut bridge = CoreBridge::start_in(Some(root.path().to_owned())).unwrap();
        let mut state = AppState::desktop();
        let (mut phone, _desktop) = connect_phone(&mut bridge, &mut state, "Phone").await;
        state.option("mode", 0);
        bridge.action("session", &mut state);
        // ModeStarted makes the bridge publish MediaStart itself; MediaStarted starts the capture thread.
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::MediaStarted { .. })
        })
        .await;
        assert!(bridge.monitor_capture.is_some());
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let decoded = loop {
            let TyuEvent::MediaFrame { data, .. } =
                phone_event(&mut phone, |e| matches!(e, TyuEvent::MediaFrame { .. })).await
            else {
                unreachable!()
            };
            if let Ok(Some(picture)) = decoder.decode(&data) {
                use openh264::formats::YUVSource;
                break picture.dimensions();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "phone never decoded a picture"
            );
        };
        // With Parsec VDD installed the stream is the plugged virtual monitor, else the primary.
        let region = bridge.virtual_display.as_ref().map(|d| d.region);
        let (expected_w, expected_h) = windows_capture::transmit_size(region);
        assert_eq!(decoded, (expected_w as usize, expected_h as usize));
        if region.is_some() {
            assert!(state.notice.contains("monitor virtual"), "{}", state.notice);
        }
        bridge.action("session", &mut state);
        next(&mut bridge, &mut state, |e| {
            matches!(e, TyuEvent::ModeStopped { .. })
        })
        .await;
        assert!(bridge.monitor_capture.is_none());
        assert!(bridge.virtual_display.is_none());
        phone.shutdown().await.unwrap();
        drop(bridge);
    }
}
