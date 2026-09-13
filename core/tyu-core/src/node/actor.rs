use super::{
    connection::{self, Authenticated, Context},
    *,
};
use crate::{
    Message,
    discovery::{self, Discovery},
    files::{self, TransferControl},
    identity::{TicketBook, now_seconds},
    media::{MediaPacket, Reassembler},
    sessions::SessionManager,
    transport,
};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::{
    sync::{oneshot, watch},
    task::JoinSet,
};
#[cfg(test)]
#[path = "nearby_tests.rs"]
mod nearby_tests;
pub(super) enum Internal {
    Found {
        id: DeviceId,
        endpoints: Vec<SocketAddr>,
    },
    Approval {
        request: RequestId,
        device: DeviceInfo,
        fingerprint: DeviceFingerprint,
        expiry: u64,
        reply: oneshot::Sender<Result<()>>,
    },
    Ready(Box<Authenticated>),
    Failed {
        peer: DeviceId,
        code: TyuErrorCode,
    },
    Inbound {
        peer: DeviceId,
        generation: usize,
        frame: Frame,
        reply: Option<oneshot::Sender<Frame>>,
    },
    Lost {
        peer: DeviceId,
        generation: usize,
    },
    Media {
        peer: DeviceId,
        packet: MediaPacket,
    },
    TransferDone {
        peer: DeviceId,
        id: TransferId,
        result: Result<()>,
    },
}
pub(super) struct Link {
    pub connection: quinn::Connection,
    pub device: DeviceInfo,
    pub outgoing: mpsc::Sender<Frame>,
    pub generation: usize,
    pub cancel: CancellationToken,
}
pub(super) struct PendingApproval {
    pub expires: u64,
    pub reply: oneshot::Sender<Result<()>>,
}
pub(super) struct PendingRequest {
    pub peer: DeviceId,
    pub frame: Frame,
    pub created: Instant,
}
pub(super) struct Transfer {
    pub peer: DeviceId,
    pub path: PathBuf,
    pub state: watch::Sender<TransferControl>,
    pub running: bool,
}
pub(super) struct Actor {
    pub config: NodeConfig,
    pub context: Context,
    pub endpoint: quinn::Endpoint,
    pub tasks: JoinSet<()>,
    pub links: HashMap<DeviceId, Link>,
    pub approvals: HashMap<RequestId, PendingApproval>,
    pub pending: HashMap<RequestId, PendingRequest>,
    pub transfers: HashMap<TransferId, Transfer>,
    pub sessions: SessionManager,
    pub discovery: Option<Discovery>,
    internal: mpsc::Receiver<Internal>,
    generation: usize,
    pub reassembler: HashMap<DeviceId, Reassembler>,
    pub replay: HashMap<DeviceId, std::collections::VecDeque<RequestId>>,
    pub discovered: HashMap<DeviceId, Vec<SocketAddr>>,
    pub connecting: HashMap<DeviceId, CancellationToken>,
}
impl Actor {
    pub fn new(
        config: NodeConfig,
        identity: DeviceIdentity,
        device: DeviceInfo,
        endpoint: quinn::Endpoint,
        events: mpsc::Sender<TyuEvent>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let (tx, internal) = mpsc::channel(256);
        let receive = config
            .receive_directory
            .as_ref()
            .map(|p| files::ReceiveRoot::new(p, config.max_file_size).map(Arc::new))
            .transpose()?;
        let context = Context {
            identity,
            device,
            store: config.store.clone(),
            tickets: Arc::new(Mutex::new(TicketBook::default())),
            internal: tx,
            events,
            cancel,
            receive,
            storage: config.storage.clone(),
            allow_nearby_pairing: config.allow_nearby_pairing,
        };
        Ok(Self {
            config,
            context,
            endpoint,
            tasks: JoinSet::new(),
            links: HashMap::new(),
            approvals: HashMap::new(),
            pending: HashMap::new(),
            transfers: HashMap::new(),
            sessions: SessionManager::default(),
            discovery: None,
            internal,
            generation: 0,
            reassembler: HashMap::new(),
            replay: HashMap::new(),
            discovered: HashMap::new(),
            connecting: HashMap::new(),
        })
    }
    pub async fn emit(&self, event: TyuEvent) {
        let _sent = self.context.events.send(event).await;
    }
    pub async fn run(
        &mut self,
        mut commands: mpsc::Receiver<TyuCommand>,
        mut media: mpsc::Receiver<(DeviceId, MediaPacket)>,
    ) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                            command=commands.recv()=>{let Some(command)=command else {break;};if matches!(command,TyuCommand::Shutdown) {break;}
            if let Err(error)=self.command(command).await {self.emit(TyuEvent::Error {request:None,code:error.code()}).await;}},
                            message=self.internal.recv()=>{if let Some(message)=message {self.internal_event(message).await;}},
                            incoming=self.endpoint.accept()=>{
                                let Some(incoming)=incoming else {break;};
                                if self.tasks.len()>=self.config.max_connections*4 || self.links.len()>=self.config.max_connections {incoming.refuse();continue;}
                                let ctx=self.context.clone();let cancel=ctx.cancel.clone();
                                self.tasks.spawn(async move {tokio::select! {_=cancel.cancelled()=>{},_=async {
                                    let result=transport::timed(async {incoming.await.map_err(transport::neterr)}).await;
                                    match result {Ok(conn)=>match connection::incoming(ctx.clone(),conn).await {Ok(auth)=>{let _sent=ctx.internal.send(Internal::Ready(Box::new(auth))).await;},Err(error)=>{tracing::debug!(code=?error.code(),"incoming admission rejected");}},Err(error)=>{tracing::debug!(code=?error.code(),"QUIC handshake rejected");}}
                                }=>{}}});
                            },
                            packet=media.recv()=>{if let Some((peer,packet))=packet && let Err(error)=self.send_media(peer,packet) {self.emit(TyuEvent::Error {request:None,code:error.code()}).await;}},
                            _=tick.tick()=>self.tick().await,
                            result=self.tasks.join_next(),if !self.tasks.is_empty()=>{if let Some(Err(error))=result {tracing::error!(%error,"backend task failed");self.emit(TyuEvent::Error {request:None,code:TyuErrorCode::InvalidState}).await;}},
                        }
        }
    }
    pub async fn shutdown(&mut self) {
        self.context.cancel.cancel();
        self.endpoint.close(0u32.into(), b"shutdown");
        self.discovery.take();
        self.approvals.clear();
        // Let connection workers drain their own child tasks before forced cancellation.
        if tokio::time::timeout(Duration::from_secs(2), async {
            while self.tasks.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            self.tasks.abort_all();
        }
        while self.tasks.join_next().await.is_some() {}
        self.links.clear();
        self.sessions = SessionManager::default();
        self.transfers.clear();
        let _idle = tokio::time::timeout(Duration::from_secs(3), self.endpoint.wait_idle()).await;
        let _sent = self.context.events.try_send(TyuEvent::Stopped);
    }
    async fn internal_event(&mut self, event: Internal) {
        match event {
            Internal::Found { id, endpoints } => {
                if self.discovered.len() < 256 || self.discovered.contains_key(&id) {
                    self.discovered.insert(id, endpoints.clone());
                    self.emit(TyuEvent::DeviceFound { id, endpoints }).await;
                }
            }
            Internal::Approval {
                request,
                device,
                fingerprint,
                expiry,
                reply,
            } => {
                if self.approvals.len() >= 8 {
                    let _sent = reply.send(Err(TyuErrorCode::Limit.into()));
                    return;
                }
                self.approvals.insert(
                    request,
                    PendingApproval {
                        expires: expiry,
                        reply,
                    },
                );
                self.emit(TyuEvent::PairingRequest {
                    request,
                    device,
                    fingerprint,
                })
                .await;
                self.emit(TyuEvent::PairingCode {
                    request,
                    code: hex::encode(&fingerprint.0[..4]),
                })
                .await;
            }
            Internal::Ready(auth) => {
                let peer = auth.device.id;
                if let Some(cancel) = self.connecting.remove(&peer)
                    && cancel.is_cancelled()
                {
                    auth.connection.close(0u32.into(), b"cancelled");
                    return;
                }
                // Recheck revocation after an in-flight handshake and before publishing Connected.
                if self
                    .context
                    .store
                    .peers()
                    .map_or(true, |p| p.iter().any(|p| p.device.id == peer && p.revoked))
                    || self.links.contains_key(&peer)
                    || self.links.len() >= self.config.max_connections
                {
                    auth.connection.close(1u32.into(), b"admission rejected");
                    return;
                }
                let (tx, rx) = mpsc::channel(128);
                self.generation += 1;
                let generation = self.generation;
                let token = self.context.cancel.child_token();
                self.links.insert(
                    peer,
                    Link {
                        connection: auth.connection.clone(),
                        device: auth.device.clone(),
                        outgoing: tx,
                        generation,
                        cancel: token.clone(),
                    },
                );
                self.emit(TyuEvent::Connected {
                    device: auth.device.clone(),
                })
                .await;
                self.emit(TyuEvent::CapabilitiesNegotiated {
                    peer,
                    local: self.context.device.capabilities.clone(),
                    remote: auth.device.capabilities.clone(),
                })
                .await;
                let mut ctx = self.context.clone();
                ctx.cancel = token;
                self.tasks
                    .spawn(connection::run(ctx, *auth, rx, generation));
            }
            Internal::Failed { peer, code } => {
                self.connecting.remove(&peer);
                self.emit(TyuEvent::ConnectionFailed { peer, code }).await
            }
            Internal::Lost { peer, generation } => {
                if self
                    .links
                    .get(&peer)
                    .is_some_and(|l| l.generation == generation)
                {
                    self.remove(peer).await;
                }
            }
            Internal::Inbound {
                peer,
                generation,
                frame,
                reply,
            } => {
                if self
                    .links
                    .get(&peer)
                    .is_none_or(|l| l.generation != generation)
                {
                    return;
                }
                self.inbound(peer, frame, reply).await;
            }
            Internal::Media { peer, packet } => self.receive_media(peer, packet).await,
            Internal::TransferDone { peer, id, result } => {
                if let Some(t) = self.transfers.get_mut(&id) {
                    t.running = false;
                }
                match result {
                    Ok(()) => {
                        self.transfers.remove(&id);
                        self.emit(TyuEvent::TransferComplete { peer, id }).await;
                    }
                    Err(error) => {
                        self.emit(TyuEvent::TransferFailed {
                            peer,
                            id,
                            code: error.code(),
                        })
                        .await
                    }
                }
            }
        }
    }
    pub async fn remove(&mut self, peer: DeviceId) {
        if let Some(cancel) = self.connecting.get(&peer) {
            cancel.cancel();
        }
        if let Some(link) = self.links.remove(&peer) {
            link.cancel.cancel();
            link.connection.close(0u32.into(), b"disconnect");
        }
        self.sessions.disconnect(peer);
        self.pending.retain(|_, p| p.peer != peer);
        self.reassembler.remove(&peer);
        self.replay.remove(&peer);
        self.emit(TyuEvent::Disconnected { peer }).await;
    }
    pub async fn queue(&self, peer: DeviceId, frame: Frame) -> Result<()> {
        frame.validate()?;
        let link = self.links.get(&peer).ok_or(TyuErrorCode::NotAvailable)?;
        link.outgoing
            .try_send(frame)
            .map_err(|_| TyuErrorCode::Limit.into())
    }
    async fn tick(&mut self) {
        let now = now_seconds();
        let expired: Vec<_> = self
            .approvals
            .iter()
            .filter(|(_, p)| p.expires <= now || p.reply.is_closed())
            .map(|(r, _)| *r)
            .collect();
        for request in expired {
            if let Some(p) = self.approvals.remove(&request) {
                let _sent = p.reply.send(Err(TyuErrorCode::Expired.into()));
            }
            self.emit(TyuEvent::PairingRejected {
                request,
                code: TyuErrorCode::Expired,
            })
            .await;
        }
        let expired = self
            .context
            .tickets
            .lock()
            .map(|mut t| t.expire())
            .unwrap_or(false);
        if expired {
            self.emit(TyuEvent::PairingExpired).await;
        }
        let expired: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.created.elapsed() > Duration::from_secs(15))
            .map(|(r, _)| *r)
            .collect();
        for request in expired {
            self.pending.remove(&request);
            self.emit(TyuEvent::Error {
                request: Some(request),
                code: TyuErrorCode::Timeout,
            })
            .await;
        }
        let mut requests = Vec::new();
        for (peer, reassembly) in &mut self.reassembler {
            for (session, stream) in reassembly.expire(Instant::now()) {
                requests.push((*peer, session, stream));
            }
        }
        for (peer, session, stream) in requests {
            let mut frame = Frame::new(Message::MediaKeyframeRequest { stream });
            frame.session = Some(session);
            if let Err(error) = self.queue(peer, frame).await {
                self.emit(TyuEvent::Error {
                    request: None,
                    code: error.code(),
                })
                .await;
            }
        }
    }
    pub fn start_discovery(&mut self) -> Result<()> {
        if self.discovery.is_some() {
            return Ok(());
        }
        let discovery = Discovery::start(
            self.context.device.id,
            self.endpoint.local_addr()?.port(),
            self.context.device.capabilities.len(),
        )?;
        let browser = discovery
            .daemon
            .browse(discovery::SERVICE)
            .map_err(transport::neterr)?;
        let events = self.context.events.clone();
        let internal = self.context.internal.clone();
        let local = self.context.device.id;
        let cancel = self.context.cancel.clone();
        self.tasks.spawn(async move {
            let mut seen=HashMap::new();
            loop {tokio::select! {_=cancel.cancelled()=>break,event=browser.recv_async()=>{
                match event {
                    Ok(mdns_sd::ServiceEvent::ServiceResolved(info))=>{
                        let Some(v)=info.get_property_val_str("v") else {continue;};let Some(id)=info.get_property_val_str("id") else {continue;};
                        let Ok(id)=discovery::parse_properties(v,id,info.get_port()) else {continue;};if id==local {continue;}
                        let endpoints=info.get_addresses().iter().map(|ip|SocketAddr::new(ip.to_ip_addr(),info.get_port())).collect();
                        if seen.len()>=256 && !seen.contains_key(info.get_fullname()) {continue;}
                        seen.insert(info.get_fullname().to_string(),id);
                        if internal.send(Internal::Found {id,endpoints}).await.is_err() {break;}
                    }
                    Ok(mdns_sd::ServiceEvent::ServiceRemoved(_,name))=>{if let Some(id)=seen.remove(&name) && events.send(TyuEvent::DeviceLost {id}).await.is_err() {break;}}
                    Err(_)=>break,_=>{}
                }
            }}}
        });
        self.discovery = Some(discovery);
        Ok(())
    }
}
