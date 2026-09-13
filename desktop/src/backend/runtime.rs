use std::{net::SocketAddr, sync::Arc, thread};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tyu_core::*;
pub struct BackendRuntime {
    pub events: mpsc::Receiver<TyuEvent>,
    commands: mpsc::Sender<TyuCommand>,
    cancel: CancellationToken,
    worker: Option<thread::JoinHandle<()>>,
}
impl BackendRuntime {
    pub fn start(data_root: Option<std::path::PathBuf>) -> std::io::Result<Self> {
        let (commands, mut rx) = mpsc::channel(128);
        let (events, event_rx) = mpsc::channel(256);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let worker=thread::Builder::new().name("tyu-backend".into()).spawn(move || {
            let runtime=match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {Ok(r)=>r,Err(error)=>{tracing::error!(%error,"backend runtime unavailable");return;}};
            runtime.block_on(async move {
                let result=async {
                    let root=match data_root {Some(root)=>root,None=>super::windows_store::data_directory()?};
                    let store=Arc::new(super::windows_store::WindowsPersistentStore::open(&root)?);
                    let mut config=NodeConfig::new("TYU Desktop",DevicePlatform::Windows,store);
                    config.allow_nearby_pairing=true;
                    use Capability::*;config.capabilities=vec![MonitorSource,MirrorReceiver,CameraReceiver,MicrophoneReceiver,StorageConsumer,FileSend,FileReceive,ClipboardSource,ClipboardReceiver,InputReceiver];
                    config.receive_directory=Some(root.join("Received"));TyuNode::start(config).await
                }.await;
                let mut node=match result {Ok(n)=>n,Err(error)=>{tracing::error!(code=?error.code(),"Core startup failed");let _sent=events.send(TyuEvent::Error {request:None,code:error.code()}).await;return;}};
                let initial=node.command(TyuCommand::StartDiscovery).await;
                if let Err(error)=initial {let _sent=events.send(TyuEvent::Error {request:None,code:error.code()}).await;}
                // Enumerate real interfaces. An explicit endpoint chooses which LAN QR to present;
                // mDNS advertises every interface independently of this QR selection.
                let ip=super::network::pairing_ip();
                let pairing_endpoint=SocketAddr::new(ip,node.address.port());
                if let Err(error)=node.command(TyuCommand::BeginPairing {endpoint:pairing_endpoint}).await {let _sent=events.send(TyuEvent::Error {request:None,code:error.code()}).await;}
                loop {tokio::select! {
                    _=token.cancelled()=>break,
                    command=rx.recv()=>{let Some(command)=command else {break;};if node.command(command).await.is_err() {break;}},
                    event=node.events.recv()=>{let Some(event)=event else {break;};
                        if matches!(event,TyuEvent::PairingExpired|TyuEvent::PairingRejected {..}|TyuEvent::PairingCompleted {..}) && let Err(error)=node.command(TyuCommand::BeginPairing {endpoint:pairing_endpoint}).await {tracing::warn!(code=?error.code(),"QR renewal failed");}
                        tokio::select! {_=token.cancelled()=>break,result=events.send(event)=>{if result.is_err() {break;}}}
                        // Wake native event processing. State updates remain on its existing timer.
                        if let Err(error)=slint::invoke_from_event_loop(||{}) {tracing::debug!(%error,"UI event loop not running");}
                    }
                }}
                if let Err(error)=node.shutdown().await {tracing::error!(code=?error.code(),"Core shutdown failed");}
            });
        })?;
        Ok(Self {
            events: event_rx,
            commands,
            cancel,
            worker: Some(worker),
        })
    }
    pub fn send(&self, command: TyuCommand) -> tyu_core::Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| TyuErrorCode::Limit.into())
    }
}
impl Drop for BackendRuntime {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("backend thread failed during shutdown");
        }
    }
}
