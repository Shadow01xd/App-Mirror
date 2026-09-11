use crate::{
    model::{ConnectionState, TransferStatus},
    state::AppState,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc, time::Duration};

slint::include_modules!();

fn stable_model<T: Clone + PartialEq + 'static>(current: ModelRc<T>, rows: Vec<T>) -> ModelRc<T> {
    if current.iter().eq(rows.iter().cloned()) {
        current
    } else {
        ModelRc::new(VecModel::from(rows))
    }
}

pub fn sync(window: &DesktopShell, state: &AppState) {
    let data = window.global::<Data>();
    data.set_devices(stable_model(
        data.get_devices(),
        state
            .device_rows()
            .into_iter()
            .map(|d| {
                let connected = d.connection == ConnectionState::Connected;
                DeviceItem {
                    name: d.name.into(),
                    detail: if connected {
                        "Conectado · Wi-Fi".into()
                    } else {
                        "Disponible para vincular".into()
                    },
                    connected,
                }
            })
            .collect::<Vec<_>>(),
    ));
    data.set_files(stable_model(
        data.get_files(),
        state
            .transfers
            .items
            .iter()
            .map(|f| FileItem {
                name: f.file_name.clone().into(),
                size: format!("{:.1} MB · ejemplo", f.total_bytes as f64 / 1_000_000.0).into(),
                progress: f.progress(),
                status: match &f.status {
                    TransferStatus::Queued => "En cola".into(),
                    TransferStatus::Completed => "Completado".into(),
                    TransferStatus::Failed(s) => s.clone().into(),
                    TransferStatus::InProgress if state.transfer_paused => "En pausa".into(),
                    TransferStatus::InProgress => {
                        format!("{} %", (f.progress() * 100.0) as u32).into()
                    }
                },
            })
            .collect::<Vec<_>>(),
    ));
    data.set_sessions(stable_model(
        data.get_sessions(),
        state
            .history
            .iter()
            .map(|s| s.clone().into())
            .collect::<Vec<slint::SharedString>>(),
    ));
    data.set_page(state.page);
    data.set_mode(state.mode_index());
    data.set_selected_device(state.selected_device as i32);
    data.set_device_name(state.device_name().into());
    data.set_pending_name(state.pending_name.clone().into());
    data.set_connected(state.connected());
    data.set_active(state.mode.active);
    data.set_scanning(state.scan_ticks > 0);
    data.set_dialog(state.dialog);
    data.set_service(state.service);
    data.set_notice(state.notice.clone().into());
    data.set_elapsed(
        format!(
            "{:02}:{:02}",
            state.elapsed_ticks / 600,
            (state.elapsed_ticks / 10) % 60
        )
        .into(),
    );
    data.set_display_mode(state.display_mode);
    data.set_orientation(state.orientation);
    data.set_quality(state.quality);
    data.set_camera_source(state.camera_source);
    data.set_camera(state.camera);
    data.set_microphone(state.microphone);
    data.set_storage(state.storage);
    data.set_noise_reduction(state.noise_reduction);
    data.set_reduced_motion(state.reduced_motion);
    data.set_notifications(state.notifications);
    data.set_transfer_running(state.transfer_running);
    data.set_transfer_paused(state.transfer_paused);
    window.global::<Motion>().set_reduced(state.reduced_motion);
}

fn sync_pairing(
    window: &DesktopShell,
    state: &mut AppState,
    server: &crate::pairing::PairingServer,
) {
    let mut hub = server.state.lock().unwrap();
    hub.expire();
    if let Some(peer) = &hub.pending {
        state.pending_name = peer.name.clone();
        state.dialog = 2;
    } else if state.dialog == 2 {
        state.dialog = 0;
        state.pending_name.clear();
    }
    if hub.active.is_none() && state.connected() {
        state.action("disconnect");
    }
    let ticket = hub.ticket.clone();
    drop(hub);
    let data = window.global::<Data>();
    let url = format!("{}/?token={ticket}", server.base_url);
    if data.get_pairing_url() != url {
        match qrcode::QrCode::new(url.as_bytes()) {
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
                data.set_pairing_qr(slint::Image::from_rgb8(pixels));
                data.set_pairing_url(url.into());
            }
            Err(error) => data.set_pairing_error(format!("No se pudo crear el QR: {error}").into()),
        }
    }
}

pub fn run() -> Result<(), slint::PlatformError> {
    let window = DesktopShell::new()?;
    window
        .global::<Data>()
        .on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
    let state = Rc::new(RefCell::new(AppState::desktop()));
    let pairing = match crate::pairing::PairingServer::start() {
        Ok(server) => Some(Rc::new(server)),
        Err(error) => {
            window
                .global::<Data>()
                .set_pairing_error(format!("No se pudo iniciar la conexión local: {error}").into());
            None
        }
    };
    if let Some(server) = &pairing {
        sync_pairing(&window, &mut state.borrow_mut(), server);
    }
    let args: Vec<String> = std::env::args().collect();
    // Reproducible native screenshots. Only enabled by explicit developer CLI arguments.
    if let Some(index) = args.iter().position(|a| a == "--view") {
        if let Some(view) = args.get(index + 1) {
            let mut s = state.borrow_mut();
            // Fixtures are opt-in captures only, never the normal startup state.
            if matches!(view.as_str(), "connected" | "mirror" | "bypass" | "active") {
                s.connect_peer("Mi teléfono".into(), "screenshot-fixture".into());
            }
            match view.as_str() {
                "mirror" => s.option("mode", 1),
                "bypass" => s.option("mode", 2),
                "active" => s.action("session"),
                "disconnected" => s.disconnect(),
                "settings" => s.option("page", 3),
                "activity" => {
                    s.action("session");
                    s.action("session");
                    s.option("page", 2);
                }
                "pairing" => {
                    if let Some(server) = &pairing {
                        let mut hub = server.state.lock().unwrap();
                        let ticket = hub.ticket.clone();
                        let _ = hub.request(&ticket, "Mi teléfono");
                        s.pending_name = "Mi teléfono".into();
                        s.dialog = 2;
                    }
                }
                "camera" => s.option("service", 0),
                _ => {}
            }
            s.notice.clear();
        }
    }
    sync(&window, &state.borrow());
    {
        let weak = window.as_weak();
        let state = state.clone();
        let pairing = pairing.clone();
        window.global::<Data>().on_action(move |command| {
            match command.as_str() {
                "accept-pair" | "reject-pair" | "close-dialog" => {
                    if let Some(server) = &pairing {
                        let peer = server
                            .state
                            .lock()
                            .unwrap()
                            .decide(command == "accept-pair");
                        let mut state = state.borrow_mut();
                        state.dialog = 0;
                        state.pending_name.clear();
                        if command == "accept-pair" {
                            if let Some(peer) = peer {
                                state.connect_peer(peer.name, peer.token);
                            }
                        }
                    }
                }
                "disconnect" => {
                    if let Some(server) = &pairing {
                        server.state.lock().unwrap().disconnect();
                    }
                    state.borrow_mut().action("disconnect");
                }
                _ => state.borrow_mut().action(command.as_str()),
            }
            if let Some(window) = weak.upgrade() {
                if let Some(server) = &pairing {
                    sync_pairing(&window, &mut state.borrow_mut(), server);
                }
                sync(&window, &state.borrow());
            }
        });
    }
    {
        let weak = window.as_weak();
        let state = state.clone();
        window.global::<Data>().on_option(move |key, value| {
            state.borrow_mut().option(key.as_str(), value);
            if let Some(window) = weak.upgrade() {
                sync(&window, &state.borrow());
            }
        });
    }
    {
        let weak = window.as_weak();
        let state = state.clone();
        window.global::<Data>().on_toggle(move |key, value| {
            state.borrow_mut().toggle(key.as_str(), value);
            if let Some(window) = weak.upgrade() {
                sync(&window, &state.borrow());
            }
        });
    }
    if args.iter().any(|arg| arg == "--smoke") {
        let data = window.global::<Data>();
        assert!(!data.get_connected());
        assert_eq!(data.get_devices().row_count(), 0);
        let server = pairing
            .as_ref()
            .expect("LAN pairing server required for smoke");
        let ticket = server.state.lock().unwrap().ticket.clone();
        server
            .state
            .lock()
            .unwrap()
            .request(&ticket, "Teléfono de prueba")
            .unwrap();
        sync_pairing(&window, &mut state.borrow_mut(), server);
        sync(&window, &state.borrow());
        assert_eq!(data.get_dialog(), 2);
        data.invoke_action("reject-pair".into());
        assert!(!data.get_connected());
        assert_eq!(data.get_dialog(), 0);
        let ticket = server.state.lock().unwrap().ticket.clone();
        server
            .state
            .lock()
            .unwrap()
            .request(&ticket, "Teléfono de prueba")
            .unwrap();
        sync_pairing(&window, &mut state.borrow_mut(), server);
        sync(&window, &state.borrow());
        data.invoke_action("accept-pair".into());
        assert!(data.get_connected());
        assert!(data.invoke_matches("Teléfono de ejemplo".into(), "TELÉFONO".into()));
        data.invoke_option("mode".into(), 1);
        assert_eq!(data.get_mode(), 1);
        data.invoke_action("session".into());
        assert!(data.get_active());
        data.invoke_toggle("camera".into(), true);
        assert!(data.get_camera());
        data.invoke_action("disconnect".into());
        assert!(!data.get_connected() && !data.get_active() && !data.get_camera());
        assert_eq!(data.get_devices().row_count(), 0);
        data.invoke_option("page".into(), 3);
        data.invoke_toggle("motion".into(), true);
        assert!(window.global::<Motion>().get_reduced());
        println!("Native UI callback smoke test passed");
        return Ok(());
    }
    let timer = slint::Timer::default();
    let weak = window.as_weak();
    let fixture = args.iter().any(|a| a == "--view");
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            state.borrow_mut().tick();
            if let Some(window) = weak.upgrade() {
                if !fixture {
                    if let Some(server) = &pairing {
                        sync_pairing(&window, &mut state.borrow_mut(), server);
                    }
                }
                sync(&window, &state.borrow());
            }
        },
    );
    if let Some(index) = args.iter().position(|a| a == "--screenshot") {
        if let Some(path) = args.get(index + 1) {
            let path = path.clone();
            let width = args
                .iter()
                .position(|a| a == "--width")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1260);
            let height = args
                .iter()
                .position(|a| a == "--height")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(920);
            window
                .window()
                .set_size(slint::LogicalSize::new(width as f32, height as f32));
            let weak = window.as_weak();
            slint::Timer::single_shot(Duration::from_millis(1300), move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let buffer = window.window().take_snapshot().expect("snapshot failed");
                image::save_buffer(
                    &path,
                    buffer.as_bytes(),
                    buffer.width(),
                    buffer.height(),
                    image::ColorType::Rgba8,
                )
                .expect("save screenshot");
                println!("Screenshot: {}", path);
                slint::quit_event_loop().expect("quit");
            });
        }
    }
    window.run()
}
