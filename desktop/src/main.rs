pub mod app;
pub mod backend;
pub mod mock;
pub mod model;
pub mod navigation;
pub mod pairing;
pub mod state;
pub mod ui;

fn main() -> Result<(), slint::PlatformError> {
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tyu_core=info".into()),
        )
        .try_init()
    {
        eprintln!("Local logging unavailable: {error}");
    }
    app::run()
}
