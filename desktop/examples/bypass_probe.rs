//! Explicit local Android integration host. Never used by the production application.
//! cargo run -p tyu-desktop --example bypass_probe -- <output-directory>
#[path = "../src/backend/bypass_camera.rs"]
mod bypass_camera;
#[path = "../src/backend/bypass_storage.rs"]
mod bypass_storage;
use std::{sync::Arc, time::Duration};
use tyu_core::*;
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Supply an isolated output directory")?,
    );
    std::fs::create_dir_all(&out)?;
    let mut config = NodeConfig::new(
        "TYU Bypass Test",
        DevicePlatform::Windows,
        Arc::new(identity::MemoryStore::default()),
    );
    config.bind = "0.0.0.0:0".parse()?;
    config.receive_directory = Some(out.join("Received"));
    config.capabilities = vec![
        Capability::CameraReceiver,
        Capability::MicrophoneReceiver,
        Capability::StorageConsumer,
        Capability::FileReceive,
    ];
    let mut node = TyuNode::start(config).await?;
    let mut storage = bypass_storage::StorageBridge::start(node.commands.clone())?;
    node.command(TyuCommand::BeginPairing {
        endpoint: format!("10.0.2.2:{}", node.address.port()).parse()?,
    })
    .await?;
    let mut camera_session = None;
    let mut camera = None;
    let mut pictures = 0;
    let mut audio = 0;
    let mut files = 0;
    let deadline = tokio::time::sleep(Duration::from_secs(180));
    tokio::pin!(deadline);
    let mut status_tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _=&mut deadline=>break,
            _=status_tick.tick()=>{
                if let Some(result)=storage.poll_mount() {
                    println!("STORAGE_MOUNTED {}",result?);
                    if std::env::var_os("TYU_PROBE_OPEN_STORAGE").is_some(){storage.open()?;}
                }
            },
            event=node.events.recv()=>{
                let Some(event)=event else {break;};
                if storage.broker.deliver(&event) {continue;}
                match event {
                    TyuEvent::PairingStarted {ticket}=>{std::fs::write(out.join("invitation.txt"),ticket.uri()?)?;println!("READY");}
                    TyuEvent::PairingRequest {request,..}=>node.command(TyuCommand::AcceptPairing {request}).await?,
                    TyuEvent::ServiceStarted {peer,session,service}=>{
                        println!("SERVICE {service:?}");
                        if service==ServiceKind::Camera {camera_session=Some(session);}
                        if service==ServiceKind::Storage {storage.mount(peer,"Android Bypass Test")?;println!("STORAGE_PENDING");}
                    }
                    TyuEvent::MediaStarted {session,format:MediaMetadata::Video(format),..} if Some(session)==camera_session => {
                        camera=Some(bypass_camera::Camera::with_format(format.width,format.height,format.fps)?);
                        println!("CAMERA_FORMAT {}x{}@{}",format.width,format.height,format.fps);
                    }
                    TyuEvent::MediaFrame {session,data,..}=>{
                        if Some(session)==camera_session {if let Some(camera)=camera.as_mut() && camera.push(&data)? {pictures+=1;if pictures==30 {println!("CAMERA_30_FRAMES");}}}
                        else {audio+=data.len();}
                    }
                    TyuEvent::TransferComplete {..}=>{files+=1;println!("FILE_RECEIVED");}
                    TyuEvent::Disconnected {..}=>break,
                    TyuEvent::Error {code,..}=>println!("ERROR {code}"),
                    _=>{}
                }
            }
        }
    }
    node.shutdown().await?;
    println!("RESULT pictures={pictures} audio_bytes={audio} files={files}");
    if pictures < 30 || audio < 1920 || files < 1 {
        return Err("Bypass probe incomplete".into());
    }
    Ok(())
}
