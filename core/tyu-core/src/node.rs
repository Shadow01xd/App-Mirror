use crate::{
    CoreError, Frame, Result, StorageOp, StorageReply, TyuErrorCode,
    identity::{DeviceFingerprint, DeviceIdentity, PairingTicket, Store},
    storage::StorageProvider,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tyu_types::*;
#[path = "node/actor.rs"]
mod actor;
#[path = "node/connection.rs"]
mod connection;
#[path = "node/requests.rs"]
mod requests;

pub struct NodeConfig {
    pub name: String,
    pub platform: DevicePlatform,
    pub capabilities: Vec<Capability>,
    pub bind: SocketAddr,
    pub store: Arc<dyn Store>,
    pub receive_directory: Option<PathBuf>,
    pub storage: Option<Arc<dyn StorageProvider>>,
    pub max_file_size: u64,
    pub max_connections: usize,
    /// Permit discovery pairing requests, always subject to explicit local approval.
    pub allow_nearby_pairing: bool,
}
impl NodeConfig {
    pub fn new(name: impl Into<String>, platform: DevicePlatform, store: Arc<dyn Store>) -> Self {
        Self {
            name: name.into(),
            platform,
            capabilities: vec![],
            bind: SocketAddr::from(([0, 0, 0, 0], 0)),
            store,
            receive_directory: None,
            storage: None,
            max_file_size: 8 * 1024 * 1024 * 1024,
            max_connections: 16,
            allow_nearby_pairing: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TyuCommand {
    StartDiscovery,
    StopDiscovery,
    BeginPairing {
        endpoint: SocketAddr,
    },
    AcceptPairing {
        request: RequestId,
    },
    RejectPairing {
        request: RequestId,
    },
    Pair {
        ticket: PairingTicket,
    },
    Connect {
        peer: DeviceId,
    },
    Disconnect {
        peer: DeviceId,
    },
    ForgetPeer {
        peer: DeviceId,
    },
    StartMode {
        peer: DeviceId,
        mode: TyuMode,
    },
    StopMode {
        peer: DeviceId,
        session: SessionId,
    },
    StartService {
        peer: DeviceId,
        service: ServiceKind,
    },
    StopService {
        peer: DeviceId,
        session: SessionId,
    },
    SendFile {
        peer: DeviceId,
        id: TransferId,
        path: PathBuf,
    },
    PauseTransfer {
        id: TransferId,
    },
    ResumeTransfer {
        id: TransferId,
    },
    CancelTransfer {
        id: TransferId,
    },
    ListStorage {
        peer: DeviceId,
        path: String,
    },
    ReadStorage {
        peer: DeviceId,
        path: String,
        offset: u64,
        length: u32,
    },
    WriteStorage {
        peer: DeviceId,
        path: String,
        offset: u64,
        data: Vec<u8>,
    },
    Storage {
        peer: DeviceId,
        operation: StorageOp,
    },
    SendClipboard {
        peer: DeviceId,
        text: String,
    },
    SendInput {
        peer: DeviceId,
        session: SessionId,
        event: InputEvent,
    },
    Request {
        peer: DeviceId,
        frame: Frame,
    },
    Ping {
        peer: DeviceId,
        value: u64,
    },
    Shutdown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TyuEvent {
    DiscoveryStarted,
    DiscoveryStopped,
    DeviceFound {
        id: DeviceId,
        endpoints: Vec<SocketAddr>,
    },
    DeviceUpdated {
        device: DeviceInfo,
    },
    DeviceLost {
        id: DeviceId,
    },
    PairingStarted {
        ticket: PairingTicket,
    },
    PairingRequest {
        request: RequestId,
        device: DeviceInfo,
        fingerprint: DeviceFingerprint,
    },
    PairingCode {
        request: RequestId,
        code: String,
    },
    PairingCompleted {
        peer: DeviceId,
    },
    PairingRejected {
        request: RequestId,
        code: TyuErrorCode,
    },
    PairingExpired,
    Connecting {
        peer: DeviceId,
    },
    Connected {
        device: DeviceInfo,
        /// Peer endpoint the winning QUIC connection uses (Wi-Fi, USB tether, ...).
        address: SocketAddr,
    },
    Disconnected {
        peer: DeviceId,
    },
    ConnectionFailed {
        peer: DeviceId,
        code: TyuErrorCode,
    },
    CapabilitiesNegotiated {
        peer: DeviceId,
        local: Vec<Capability>,
        remote: Vec<Capability>,
    },
    ModeStarted {
        peer: DeviceId,
        session: SessionId,
        mode: TyuMode,
    },
    ModeStopped {
        peer: DeviceId,
        session: SessionId,
    },
    ServiceStarted {
        peer: DeviceId,
        session: SessionId,
        service: ServiceKind,
    },
    ServiceStopped {
        peer: DeviceId,
        session: SessionId,
    },
    TransferOffered {
        peer: DeviceId,
        metadata: TransferMetadata,
    },
    TransferProgress {
        peer: DeviceId,
        id: TransferId,
        bytes: u64,
        total: u64,
    },
    TransferPaused {
        id: TransferId,
    },
    TransferComplete {
        peer: DeviceId,
        id: TransferId,
    },
    TransferFailed {
        peer: DeviceId,
        id: TransferId,
        code: TyuErrorCode,
    },
    StorageResult {
        peer: DeviceId,
        request: RequestId,
        result: std::result::Result<StorageReply, TyuErrorCode>,
    },
    MediaStarted {
        peer: DeviceId,
        session: SessionId,
        stream: StreamId,
    },
    MediaStopped {
        peer: DeviceId,
        stream: StreamId,
    },
    MediaFrame {
        peer: DeviceId,
        session: SessionId,
        stream: StreamId,
        data: Bytes,
    },
    KeyframeRequested {
        peer: DeviceId,
        session: SessionId,
        stream: StreamId,
    },
    ClipboardReceived {
        peer: DeviceId,
        text: String,
    },
    InputReceived {
        peer: DeviceId,
        event: InputEvent,
    },
    Pong {
        peer: DeviceId,
        value: u64,
    },
    MetricsUpdated {
        peer: DeviceId,
        rtt_micros: u64,
        sent_bytes: u64,
        received_bytes: u64,
    },
    Error {
        request: Option<RequestId>,
        code: TyuErrorCode,
    },
    Stopped,
}
pub struct TyuNode {
    pub device: DeviceInfo,
    pub address: SocketAddr,
    pub fingerprint: DeviceFingerprint,
    pub commands: mpsc::Sender<TyuCommand>,
    pub events: mpsc::Receiver<TyuEvent>,
    media: mpsc::Sender<(DeviceId, crate::media::MediaPacket)>,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
}
impl TyuNode {
    pub async fn start(config: NodeConfig) -> Result<Self> {
        if config.max_connections == 0
            || config.max_connections > 64
            || config.capabilities.len() > 32
        {
            return Err(TyuErrorCode::Limit.into());
        }
        let identity = match config.store.load_identity()? {
            Some(i) => i,
            None => {
                let i = DeviceIdentity::generate(&config.name)?;
                config.store.save_identity(&i)?;
                i
            }
        };
        let device = DeviceInfo {
            id: identity.id(),
            name: identity.friendly_name.clone(),
            platform: config.platform,
            capabilities: config.capabilities.clone(),
        };
        Frame::new(crate::Message::Hello(device.clone())).validate()?;
        let endpoint = crate::transport::endpoint(&identity, config.bind)?;
        let address = endpoint.local_addr()?;
        let (commands, rx) = mpsc::channel(128);
        let (events, evrx) = mpsc::channel(256);
        let (media, mediarx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let mut actor = actor::Actor::new(
            config,
            identity.clone(),
            device.clone(),
            endpoint,
            events,
            cancel.clone(),
        )?;
        let token = cancel.clone();
        let worker = tokio::spawn(async move {
            tokio::select! {_=token.cancelled()=>{},_=actor.run(rx,mediarx)=>{}}
            actor.shutdown().await;
        });
        Ok(Self {
            device,
            address,
            fingerprint: identity.fingerprint(),
            commands,
            events: evrx,
            media,
            cancel,
            worker: Some(worker),
        })
    }
    pub async fn command(&self, command: TyuCommand) -> Result<()> {
        self.commands
            .send(command)
            .await
            .map_err(|_| TyuErrorCode::Cancelled.into())
    }
    pub async fn send_media(
        &self,
        peer: DeviceId,
        packet: crate::media::MediaPacket,
    ) -> Result<()> {
        self.media
            .send((peer, packet))
            .await
            .map_err(|_| TyuErrorCode::Cancelled.into())
    }
    pub async fn shutdown(mut self) -> Result<()> {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker
                .await
                .map_err(|_| CoreError::Code(TyuErrorCode::InvalidState))?;
        }
        Ok(())
    }
}
impl Drop for TyuNode {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
