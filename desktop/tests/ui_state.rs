#[path = "../src/mock/mod.rs"]
pub mod mock;
#[path = "../src/model/mod.rs"]
pub mod model;
#[path = "../src/navigation/mod.rs"]
pub mod navigation;
#[path = "../src/state/mod.rs"]
pub mod state;

use model::{ConnectionState, TyuMode};
use navigation::Route;
use state::AppState;

#[test]
fn desktop_starts_empty_and_connects_only_the_accepted_peer() {
    let mut state = AppState::desktop();
    assert!(!state.connected());
    assert!(state.devices.nearby.is_empty());
    state.connect_peer("Mi teléfono".into(), "peer-id".into());
    assert!(state.connected());
    assert_eq!(state.device_name(), "Mi teléfono");
    assert_eq!(state.devices.nearby.len(), 1);
    state.action("disconnect");
    assert!(!state.connected());
    assert!(state.devices.nearby.is_empty());
    assert_eq!(state.page, 0);
}

#[test]
fn app_state_starts_disconnected_on_the_welcome_route() {
    let state = AppState::default();

    assert_eq!(state.navigation.current(), Route::Welcome);
    assert_eq!(state.devices.connection, ConnectionState::Disconnected);
    assert_eq!(state.mode.selected, TyuMode::Monitor);
    assert!(!state.mode.active);
    assert!(state.transfers.items.is_empty());
}

#[test]
fn disconnect_stops_all_services_and_interrupts_transfer() {
    let mut state = AppState::demo();
    state.action("session");
    state.toggle("camera", true);
    state.toggle("microphone", true);
    state.toggle("storage", true);
    state.action("add-files");
    state.action("transfer");
    state.tick();
    state.action("disconnect");
    assert!(!state.connected());
    assert!(!state.mode.active);
    assert!(!state.camera && !state.microphone && !state.storage);
    assert!(!state.transfer_running);
    assert!(matches!(
        state.transfers.items[0].status,
        model::TransferStatus::Failed(_)
    ));
    state.action("session");
    state.toggle("camera", true);
    assert!(!state.mode.active && !state.camera);
}

#[test]
fn invalid_device_selection_keeps_the_only_connection() {
    let mut state = AppState::demo();
    state.action("session");
    state.option("device", 1);
    assert!(state.connected());
    assert!(state.mode.active);
    assert_eq!(state.device_name(), "Teléfono de ejemplo");
}

#[test]
fn transfers_pause_resume_and_finish_without_exceeding_total() {
    let mut state = AppState::demo();
    state.action("add-files");
    state.action("transfer");
    state.tick();
    state.action("transfer");
    let before = state.transfers.items[0].transferred_bytes;
    for _ in 0..20 {
        state.tick();
    }
    assert_eq!(state.transfers.items[0].transferred_bytes, before);
    state.action("transfer");
    for _ in 0..150 {
        state.tick();
    }
    assert!(!state.transfer_running);
    assert!(
        state
            .transfers
            .items
            .iter()
            .all(|f| f.status == model::TransferStatus::Completed
                && f.transferred_bytes == f.total_bytes)
    );
}

#[test]
fn settings_survive_navigation_and_modes_stop_previous_session() {
    let mut state = AppState::demo();
    state.toggle("motion", true);
    state.option("orientation", 2);
    state.option("page", 3);
    state.option("page", 0);
    assert!(state.reduced_motion);
    assert_eq!(state.orientation, 2);
    state.action("session");
    state.option("quality", 1);
    assert_eq!(state.quality, 0);
    state.option("mode", 1);
    assert!(!state.mode.active);
    assert_eq!(state.mode.selected, TyuMode::Mirror);
}
