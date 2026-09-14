use super::{DeviceState, ModeState, TransferState};
use crate::model::{ConnectionState, Device, DeviceStatus, TransferStatus, TyuMode};
use crate::navigation::{Navigator, Route};

#[derive(Debug, Default)]
pub struct AppState {
    pub navigation: Navigator,
    pub devices: DeviceState,
    pub mode: ModeState,
    pub transfers: TransferState,
    pub selected_device: usize,
    pub pending_name: String,
    pub page: i32,
    pub dialog: i32,
    pub service: i32,
    pub display_mode: i32,
    pub orientation: i32,
    pub quality: i32,
    pub camera_source: i32,
    pub camera: bool,
    pub microphone: bool,
    pub storage: bool,
    pub noise_reduction: bool,
    pub reduced_motion: bool,
    pub notifications: bool,
    pub transfer_running: bool,
    pub transfer_paused: bool,
    pub history: Vec<String>,
    pub notice: String,
    /// How the selected phone is reached: "Wi-Fi" or "USB" (tethered cable).
    pub link: String,
    pub elapsed_ticks: u64,
    pub scan_ticks: u32,
    notice_ticks: u32,
}

impl AppState {
    pub fn desktop() -> Self {
        Self {
            notifications: true,
            noise_reduction: true,
            service: -1,
            link: "Wi-Fi".into(),
            ..Self::default()
        }
    }

    pub fn connect_peer(&mut self, name: String, id: String) {
        self.disconnect();
        self.devices.nearby = vec![Device {
            id,
            name,
            status: DeviceStatus::Paired,
            connection: ConnectionState::Disconnected,
        }];
        self.selected_device = 0;
        self.pending_name.clear();
        self.page = 0;
        self.connect();
        self.record("Teléfono conectado");
    }

    pub fn demo() -> Self {
        let mut state = Self {
            notifications: true,
            noise_reduction: true,
            service: -1,
            ..Self::default()
        };
        state.devices.nearby = crate::mock::devices();
        state.connect();
        state.history.clear();
        state.notice.clear();
        state
    }

    pub fn connected(&self) -> bool {
        self.devices.connection == ConnectionState::Connected
    }

    pub fn device_name(&self) -> &str {
        self.devices
            .nearby
            .get(self.selected_device)
            .map_or("Sin dispositivo", |d| d.name.as_str())
    }

    pub fn mode_index(&self) -> i32 {
        match self.mode.selected {
            TyuMode::Monitor => 0,
            TyuMode::Mirror => 1,
            _ => 2,
        }
    }

    fn report(&mut self, message: &str) {
        self.notice = message.into();
        self.notice_ticks = 40;
    }

    fn record(&mut self, message: &str) {
        self.history.insert(0, message.into());
        self.history.truncate(100);
        self.report(message);
    }

    fn connect(&mut self) {
        if let Some(device) = self.devices.nearby.get_mut(self.selected_device) {
            device.connection = ConnectionState::Connected;
            device.status = DeviceStatus::Paired;
            self.devices.selected = Some(device.clone());
            self.devices.connection = ConnectionState::Connected;
            self.dialog = 0;
            self.scan_ticks = 0;
            self.navigation.replace(Route::Home);
        }
    }

    pub fn disconnect(&mut self) {
        self.mode.active = false;
        self.elapsed_ticks = 0;
        self.camera = false;
        self.microphone = false;
        self.storage = false;
        self.transfer_running = false;
        self.transfer_paused = false;
        for item in &mut self.transfers.items {
            if item.status == TransferStatus::InProgress {
                item.status = TransferStatus::Failed("Conexión interrumpida".into());
            }
        }
        for device in &mut self.devices.nearby {
            device.connection = ConnectionState::Disconnected;
        }
        self.devices.connection = ConnectionState::Disconnected;
        self.devices.selected = None;
    }

    pub fn option(&mut self, key: &str, value: i32) {
        match key {
            // 5 is the full-size mirror view; it keeps Home as its navigation route.
            "page" if (0..=5).contains(&value) => {
                self.page = value;
                self.navigation.navigate(match value {
                    1 => Route::Transfer,
                    2 => Route::Sessions,
                    3 => Route::Settings,
                    4 => Route::DeviceDetails,
                    _ => Route::Home,
                });
            }
            "device" if value >= 0 && (value as usize) < self.devices.nearby.len() => {
                if self.selected_device != value as usize {
                    self.disconnect();
                    self.selected_device = value as usize;
                }
                self.page = 0;
                self.navigation.navigate(Route::Home);
            }
            "mode" if (0..=2).contains(&value) => {
                if self.mode_index() != value && self.mode.active {
                    self.mode.active = false;
                    self.record("Sesión detenida al cambiar de modo");
                }
                self.mode.selected = match value {
                    0 => TyuMode::Monitor,
                    1 => TyuMode::Mirror,
                    _ => TyuMode::Bypass,
                };
                self.elapsed_ticks = 0;
                self.display_mode = 0;
            }
            "display" if !self.mode.active && (0..=1).contains(&value) => self.display_mode = value,
            "orientation" if !self.mode.active && (0..=2).contains(&value) => {
                self.orientation = value
            }
            "quality" if !self.mode.active && (0..=2).contains(&value) => self.quality = value,
            "camera-source" if (0..=1).contains(&value) => self.camera_source = value,
            "service" if (0..=1).contains(&value) => {
                self.service = value;
                self.dialog = 3;
            }
            _ => {}
        }
    }

    pub fn toggle(&mut self, key: &str, value: bool) {
        match key {
            "motion" => self.reduced_motion = value,
            "notifications" => {
                self.notifications = value;
                self.notice.clear();
            }
            "noise" => self.noise_reduction = value,
            "camera" | "microphone" | "storage" if self.connected() => {
                let name = match key {
                    "camera" => {
                        self.camera = value;
                        "Cámara"
                    }
                    "microphone" => {
                        self.microphone = value;
                        "Micrófono"
                    }
                    _ => {
                        self.storage = value;
                        "Archivos"
                    }
                };
                self.record(&format!(
                    "{} {} · demostración",
                    name,
                    if value { "activado" } else { "desactivado" }
                ));
            }
            _ => {}
        }
    }

    pub fn action(&mut self, action: &str) {
        match action {
            "disconnect" => {
                self.disconnect();
                self.devices.nearby.clear();
                self.page = 0;
                self.record("Dispositivo desconectado");
            }
            "discover" => {
                self.scan_ticks = 15;
                self.dialog = 1;
            }
            "pair" => {
                self.dialog = 2;
                self.scan_ticks = 0;
            }
            "confirm-pair" if self.dialog == 2 => {
                self.connect();
                self.record("Vinculación de ejemplo confirmada");
            }
            "close-dialog" => {
                self.dialog = 0;
                self.scan_ticks = 0;
            }
            "session" if self.connected() => {
                self.mode.active = !self.mode.active;
                self.elapsed_ticks = 0;
                let mode = ["Monitor", "Espejo", "Bypass"][self.mode_index() as usize];
                self.record(&format!(
                    "{} {} · {}",
                    mode,
                    if self.mode.active {
                        "iniciado"
                    } else {
                        "detenido"
                    },
                    self.device_name()
                ));
            }
            "clear-history" => {
                self.history.clear();
                self.report("Historial de esta sesión eliminado");
            }
            "add-files" if !self.transfer_running => {
                self.transfers.items = crate::mock::transfers();
                self.report("Tres archivos de ejemplo añadidos");
            }
            "transfer" if self.connected() && !self.transfers.items.is_empty() => {
                if self.transfer_running {
                    self.transfer_paused = !self.transfer_paused;
                } else {
                    for item in &mut self.transfers.items {
                        if item.status != TransferStatus::Completed {
                            item.status = TransferStatus::Queued;
                        }
                    }
                    if self
                        .transfers
                        .items
                        .iter()
                        .all(|x| x.status == TransferStatus::Completed)
                    {
                        self.report("Todos los archivos ya se enviaron. Añade nuevos ejemplos.");
                        return;
                    }
                    self.transfer_running = true;
                    self.transfer_paused = false;
                    self.record("Transferencia de ejemplo iniciada");
                }
            }
            "cancel-transfer" => {
                self.transfer_running = false;
                self.transfer_paused = false;
                for item in &mut self.transfers.items {
                    if item.status != TransferStatus::Completed {
                        item.status = TransferStatus::Failed("Cancelado".into());
                    }
                }
                self.record("Transferencia cancelada");
            }
            _ => {}
        }
    }

    /// One 100ms demo clock step. All timers run on the UI thread.
    pub fn tick(&mut self) {
        self.scan_ticks = self.scan_ticks.saturating_sub(1);
        self.notice_ticks = self.notice_ticks.saturating_sub(1);
        if self.notice_ticks == 0 {
            self.notice.clear();
        }
        if self.mode.active {
            self.elapsed_ticks += 1;
        }
        if !self.connected() || !self.transfer_running || self.transfer_paused {
            return;
        }
        if let Some(item) = self
            .transfers
            .items
            .iter_mut()
            .find(|t| t.status != TransferStatus::Completed)
        {
            item.status = TransferStatus::InProgress;
            item.transferred_bytes =
                (item.transferred_bytes + (item.total_bytes / 35).max(1)).min(item.total_bytes);
            if item.transferred_bytes == item.total_bytes {
                item.status = TransferStatus::Completed;
            }
        }
        if self
            .transfers
            .items
            .iter()
            .all(|t| t.status == TransferStatus::Completed)
        {
            self.transfer_running = false;
            self.record("Transferencia de ejemplo completada");
        }
    }

    pub fn device_rows(&self) -> Vec<Device> {
        self.devices.nearby.clone()
    }
}
