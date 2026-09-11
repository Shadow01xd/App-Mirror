pub mod app;
pub mod mock;
pub mod model;
pub mod navigation;
pub mod pairing;
pub mod state;
pub mod ui;

fn main() -> Result<(), slint::PlatformError> {
    app::run()
}
