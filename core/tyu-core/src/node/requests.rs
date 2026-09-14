use super::{
    actor::{Actor, Internal, PendingRequest, Transfer},
    connection, *,
};
use crate::{
    Message,
    files::{self, TransferControl},
    media::{MediaPacket, Reassembler},
    sessions::{SessionKind, supports},
    transport,
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};

/// First bytes of every unidirectional media stream, so receivers can tell them apart.
pub(super) const MEDIA_STREAM_TAG: &[u8] = b"TYUM";

impl Actor {
    pub async fn command(&mut self, command: TyuCommand) -> Result<()> {
        match command {
            TyuCommand::StartDiscovery => {
                self.start_discovery()?;
                self.emit(TyuEvent::DiscoveryStarted).await;
            }
            TyuCommand::StopDiscovery => {
                self.discovery.take();
                self.emit(TyuEvent::DiscoveryStopped).await;
            }
            TyuCommand::BeginPairing { mut endpoint } => {
                endpoint.set_port(self.endpoint.local_addr()?.port());
                if endpoint.ip().is_unspecified() {
                    return Err(TyuErrorCode::Malformed.into());
                }
                let ticket = self
                    .context
                    .tickets
                    .lock()
                    .map_err(|_| CoreError::Store)?
                    .issue(endpoint, self.context.identity.fingerprint())?;
                self.emit(TyuEvent::PairingStarted { ticket }).await;
            }
            TyuCommand::AcceptPairing { request } => self.decide(request, true).await?,
            TyuCommand::RejectPairing { request } => self.decide(request, false).await?,
            TyuCommand::Pair { ticket } => {
                let peer = ticket.fingerprint.device_id();
                let mut addresses = vec![ticket.endpoint];
                addresses.extend(self.discovered.get(&peer).into_iter().flatten().copied());
                self.connect(
                    peer,
                    addresses,
                    transport::ServerIdentity::Trusted(ticket.fingerprint),
                    Some(ticket),
                )
                .await?;
            }
            TyuCommand::Connect { peer } => {
                let trusted = self
                    .context
                    .store
                    .peers()?
                    .into_iter()
                    .find(|p| p.device.id == peer);
                let mut addresses = self.discovered.get(&peer).cloned().unwrap_or_default();
                let identity = if let Some(trusted) = trusted {
                    if trusted.revoked {
                        return Err(TyuErrorCode::Revoked.into());
                    }
                    addresses.push(trusted.endpoint);
                    transport::ServerIdentity::Trusted(trusted.fingerprint)
                } else {
                    if addresses.is_empty() {
                        return Err(TyuErrorCode::NotAvailable.into());
                    }
                    transport::ServerIdentity::Discovered(peer)
                };
                self.connect(peer, addresses, identity, None).await?;
            }
            TyuCommand::Disconnect { peer } => {
                self.remove(peer).await;
            }
            TyuCommand::ForgetPeer { peer } => {
                self.context.store.revoke(peer)?;
                self.remove(peer).await;
            }
            TyuCommand::StartMode { peer, mode } => {
                let mut f = Frame::new(Message::ModeStart(mode));
                f.session = Some(SessionId::new());
                self.request(peer, f).await?;
            }
            TyuCommand::StopMode { peer, session } => {
                let mut f = Frame::new(Message::ModeStop);
                f.session = Some(session);
                self.request(peer, f).await?;
            }
            TyuCommand::StartService { peer, service } => {
                let mut f = Frame::new(Message::ServiceStart(service));
                f.session = Some(SessionId::new());
                self.request(peer, f).await?;
            }
            TyuCommand::StopService { peer, session } => {
                let mut f = Frame::new(Message::ServiceStop);
                f.session = Some(session);
                self.request(peer, f).await?;
            }
            TyuCommand::Request { peer, frame } => self.request(peer, frame).await?,
            TyuCommand::SendClipboard { peer, text } => {
                self.request(peer, Frame::new(Message::Clipboard(text)))
                    .await?
            }
            TyuCommand::Ping { peer, value } => {
                self.request(peer, Frame::new(Message::Ping(value))).await?
            }
            TyuCommand::SendInput {
                peer,
                session,
                event,
            } => {
                let mut frame = Frame::new(Message::Input(event));
                frame.session = Some(session);
                self.rpc(peer, frame, false)?;
            }
            TyuCommand::Storage { peer, operation } => {
                self.rpc(peer, Frame::new(Message::Storage(operation)), true)?
            }
            TyuCommand::ListStorage { peer, path } => self.rpc(
                peer,
                Frame::new(Message::Storage(StorageOp::List { path })),
                true,
            )?,
            TyuCommand::ReadStorage {
                peer,
                path,
                offset,
                length,
            } => self.rpc(
                peer,
                Frame::new(Message::Storage(StorageOp::Read {
                    path,
                    offset,
                    length,
                })),
                true,
            )?,
            TyuCommand::WriteStorage {
                peer,
                path,
                offset,
                data,
            } => self.rpc(
                peer,
                Frame::new(Message::Storage(StorageOp::Write { path, offset, data })),
                true,
            )?,
            TyuCommand::SendFile { peer, id, path } => {
                if self.transfers.len() >= 32 || self.transfers.contains_key(&id) {
                    return Err(TyuErrorCode::Limit.into());
                }
                let (state, _) = watch::channel(TransferControl::Running);
                self.transfers.insert(
                    id,
                    Transfer {
                        peer,
                        path,
                        state,
                        running: false,
                    },
                );
                self.send_file(id)?;
            }
            TyuCommand::PauseTransfer { id } => {
                let t = self.transfers.get(&id).ok_or(TyuErrorCode::InvalidState)?;
                t.state.send_replace(TransferControl::Paused);
                self.emit(TyuEvent::TransferPaused { id }).await;
            }
            TyuCommand::ResumeTransfer { id } => {
                let t = self.transfers.get(&id).ok_or(TyuErrorCode::InvalidState)?;
                t.state.send_replace(TransferControl::Running);
                if !t.running {
                    self.send_file(id)?;
                }
            }
            TyuCommand::CancelTransfer { id } => {
                let t = self.transfers.get(&id).ok_or(TyuErrorCode::InvalidState)?;
                t.state.send_replace(TransferControl::Cancelled);
            }
            TyuCommand::Shutdown => self.context.cancel.cancel(),
        }
        Ok(())
    }
    async fn decide(&mut self, request: RequestId, accept: bool) -> Result<()> {
        let pending = self
            .approvals
            .remove(&request)
            .ok_or(TyuErrorCode::Expired)?;
        let code = if pending.expires <= crate::identity::now_seconds() {
            Some(TyuErrorCode::Expired)
        } else if !accept {
            Some(TyuErrorCode::Rejected)
        } else {
            None
        };
        pending
            .reply
            .send(code.map_or(Ok(()), |c| Err(c.into())))
            .map_err(|_| TyuErrorCode::Expired)?;
        if let Some(code) = code {
            self.emit(TyuEvent::PairingRejected { request, code }).await;
        }
        Ok(())
    }
    async fn connect(
        &mut self,
        peer: DeviceId,
        mut addresses: Vec<SocketAddr>,
        pin: transport::ServerIdentity,
        ticket: Option<PairingTicket>,
    ) -> Result<()> {
        addresses.retain(|a| {
            a.is_ipv4() == self.config.bind.is_ipv4() && !a.ip().is_unspecified() && a.port() != 0
        });
        addresses.sort_unstable();
        addresses.dedup();
        if addresses.is_empty() {
            return Err(TyuErrorCode::NotAvailable.into());
        }
        if self.links.contains_key(&peer)
            || self.connecting.contains_key(&peer)
            || self.tasks.len() >= self.config.max_connections * 4
        {
            return Err(TyuErrorCode::Limit.into());
        }
        self.emit(TyuEvent::Connecting { peer }).await;
        let mut ctx = self.context.clone();
        let token = ctx.cancel.child_token();
        ctx.cancel = token.clone();
        self.connecting.insert(peer, token.clone());
        let endpoint = self.endpoint.clone();
        self.tasks.spawn(async move {tokio::select! {_=token.cancelled()=>{let _sent=ctx.internal.send(Internal::Failed {peer,code:TyuErrorCode::Cancelled}).await;},_=async {
            let event=match connection::outgoing(ctx.clone(),endpoint,addresses,pin,ticket).await {Ok(auth)=>Internal::Ready(Box::new(auth)),Err(e)=>Internal::Failed {peer,code:e.code()}};
            let _sent=ctx.internal.send(event).await;
        }=>{}}});
        Ok(())
    }
    async fn request(&mut self, peer: DeviceId, frame: Frame) -> Result<()> {
        if self.pending.len() >= 128 || self.pending.contains_key(&frame.request) {
            return Err(TyuErrorCode::Limit.into());
        }
        match &frame.message {
            Message::ModeStart(mode) => self.check_kind(peer, &SessionKind::Display(*mode))?,
            Message::ServiceStart(kind) => self.check_kind(peer, &SessionKind::Service(*kind))?,
            Message::Clipboard(_) => self.direction(
                peer,
                Capability::ClipboardSource,
                Capability::ClipboardReceiver,
                false,
            )?,
            Message::ModeStop
            | Message::ServiceStop
            | Message::MediaStart { .. }
            | Message::MediaConfig { .. }
            | Message::MediaStop { .. }
            | Message::MediaKeyframeRequest { .. } => self.check_session(peer, frame.session)?,
            Message::Ping(_) | Message::Heartbeat | Message::Goodbye => {}
            _ => return Err(TyuErrorCode::Unsupported.into()),
        }
        self.queue(peer, frame.clone()).await?;
        self.pending.insert(
            frame.request,
            PendingRequest {
                peer,
                frame,
                created: Instant::now(),
            },
        );
        Ok(())
    }
    pub async fn inbound(
        &mut self,
        peer: DeviceId,
        frame: Frame,
        reply: Option<oneshot::Sender<Frame>>,
    ) {
        if matches!(
            frame.message,
            Message::Ack | Message::Error { .. } | Message::Pong(_)
        ) {
            let Some(pending) = self.pending.remove(&frame.request) else {
                return;
            };
            if pending.peer != peer || pending.frame.session != frame.session {
                self.pending.insert(pending.frame.request, pending);
                return;
            }
            match frame.message {
                Message::Error { code } => {
                    self.emit(TyuEvent::Error {
                        request: Some(frame.request),
                        code,
                    })
                    .await
                }
                Message::Pong(value) => self.emit(TyuEvent::Pong { peer, value }).await,
                Message::Ack => {
                    if let Err(error) = self.apply(peer, &pending.frame, false).await {
                        self.emit(TyuEvent::Error {
                            request: Some(frame.request),
                            code: error.code(),
                        })
                        .await;
                    }
                }
                _ => {}
            }
            return;
        }
        let seen = self.replay.entry(peer).or_default();
        if seen.contains(&frame.request) {
            let response = frame.reply(Message::Error {
                code: TyuErrorCode::Replay,
            });
            if let Some(reply) = reply {
                let _sent = reply.send(response);
            } else {
                let _queued = self.queue(peer, response).await;
            }
            return;
        }
        seen.push_back(frame.request);
        if seen.len() > 1024 {
            seen.pop_front();
        }
        let result = self.apply(peer, &frame, true).await;
        let response = frame.reply(match result {
            Ok(()) => {
                if let Message::Ping(value) = frame.message {
                    Message::Pong(value)
                } else {
                    Message::Ack
                }
            }
            Err(error) => Message::Error { code: error.code() },
        });
        if let Some(reply) = reply {
            let _sent = reply.send(response);
        } else if let Err(error) = self.queue(peer, response).await {
            self.emit(TyuEvent::Error {
                request: Some(frame.request),
                code: error.code(),
            })
            .await;
        }
    }
    fn check_kind(&self, peer: DeviceId, kind: &SessionKind) -> Result<()> {
        let link = self.links.get(&peer).ok_or(TyuErrorCode::NotAvailable)?;
        if !supports(
            kind,
            &self.context.device.capabilities,
            &link.device.capabilities,
        ) {
            return Err(TyuErrorCode::Unsupported.into());
        }
        Ok(())
    }
    fn direction(
        &self,
        peer: DeviceId,
        source: Capability,
        receiver: Capability,
        inbound: bool,
    ) -> Result<()> {
        let remote = &self
            .links
            .get(&peer)
            .ok_or(TyuErrorCode::NotAvailable)?
            .device
            .capabilities;
        let local = &self.context.device.capabilities;
        let (a, b) = if inbound {
            (remote, local)
        } else {
            (local, remote)
        };
        if !a.contains(&source) || !b.contains(&receiver) {
            return Err(TyuErrorCode::Unsupported.into());
        }
        Ok(())
    }
    fn check_session(&self, peer: DeviceId, session: Option<SessionId>) -> Result<()> {
        if session
            .and_then(|id| self.sessions.sessions.get(&id))
            .is_none_or(|s| s.peer != peer)
        {
            return Err(TyuErrorCode::InvalidState.into());
        }
        Ok(())
    }
    async fn apply(&mut self, peer: DeviceId, frame: &Frame, inbound: bool) -> Result<()> {
        match &frame.message {
            Message::ModeStart(mode) => {
                let session = frame.session.ok_or(TyuErrorCode::InvalidState)?;
                let remote = &self
                    .links
                    .get(&peer)
                    .ok_or(TyuErrorCode::NotAvailable)?
                    .device
                    .capabilities;
                self.sessions.start(
                    session,
                    peer,
                    SessionKind::Display(*mode),
                    &self.context.device.capabilities,
                    remote,
                )?;
                self.emit(TyuEvent::ModeStarted {
                    peer,
                    session,
                    mode: *mode,
                })
                .await;
            }
            Message::ServiceStart(service) => {
                let session = frame.session.ok_or(TyuErrorCode::InvalidState)?;
                let remote = &self
                    .links
                    .get(&peer)
                    .ok_or(TyuErrorCode::NotAvailable)?
                    .device
                    .capabilities;
                self.sessions.start(
                    session,
                    peer,
                    SessionKind::Service(*service),
                    &self.context.device.capabilities,
                    remote,
                )?;
                self.emit(TyuEvent::ServiceStarted {
                    peer,
                    session,
                    service: *service,
                })
                .await;
            }
            Message::ModeStop | Message::ServiceStop => {
                let session = frame.session.ok_or(TyuErrorCode::InvalidState)?;
                let ended = self.sessions.stop(session, peer)?;
                match ended.kind {
                    SessionKind::Display(_) => {
                        self.emit(TyuEvent::ModeStopped { peer, session }).await
                    }
                    SessionKind::Service(_) => {
                        self.emit(TyuEvent::ServiceStopped { peer, session }).await
                    }
                }
            }
            Message::Clipboard(text) => {
                self.direction(
                    peer,
                    Capability::ClipboardSource,
                    Capability::ClipboardReceiver,
                    inbound,
                )?;
                if inbound {
                    self.emit(TyuEvent::ClipboardReceived {
                        peer,
                        text: text.clone(),
                    })
                    .await;
                }
            }
            Message::Input(event) => {
                self.check_session(peer, frame.session)?;
                self.direction(
                    peer,
                    Capability::TouchSource,
                    Capability::InputReceiver,
                    inbound,
                )?;
                if inbound {
                    self.emit(TyuEvent::InputReceived {
                        peer,
                        event: event.clone(),
                    })
                    .await;
                }
            }
            Message::MediaStart { stream, format } => {
                self.check_session(peer, frame.session)?;
                let session = frame.session.ok_or(TyuErrorCode::InvalidState)?;
                self.media_direction(peer, session, format, inbound)?;
                if self.sessions.media.len() >= 64 || self.sessions.media.contains_key(stream) {
                    return Err(TyuErrorCode::Limit.into());
                }
                if self.sessions.sessions.get(&session).is_none_or(|s| {
                    matches!(
                        s.kind,
                        SessionKind::Display(TyuMode::Bypass)
                            | SessionKind::Service(
                                ServiceKind::Storage
                                    | ServiceKind::Files
                                    | ServiceKind::Clipboard
                                    | ServiceKind::Input
                            )
                    )
                }) {
                    return Err(TyuErrorCode::Unsupported.into());
                }
                self.sessions
                    .media
                    .insert(*stream, (session, format.clone()));
                self.sessions.media_senders.insert(
                    *stream,
                    if inbound {
                        peer
                    } else {
                        self.context.device.id
                    },
                );
                self.emit(TyuEvent::MediaStarted {
                    peer,
                    session,
                    stream: *stream,
                })
                .await;
            }
            Message::MediaConfig { stream, format } => {
                self.check_session(peer, frame.session)?;
                self.media_direction(
                    peer,
                    frame.session.ok_or(TyuErrorCode::InvalidState)?,
                    format,
                    inbound,
                )?;
                let sender = if inbound {
                    peer
                } else {
                    self.context.device.id
                };
                if self.sessions.media_senders.get(stream) != Some(&sender) {
                    return Err(TyuErrorCode::Unauthorized.into());
                }
                let entry = self
                    .sessions
                    .media
                    .get_mut(stream)
                    .ok_or(TyuErrorCode::InvalidState)?;
                if Some(entry.0) != frame.session {
                    return Err(TyuErrorCode::InvalidState.into());
                }
                entry.1 = format.clone();
            }
            Message::MediaStop { stream } => {
                self.check_session(peer, frame.session)?;
                if self
                    .sessions
                    .media
                    .get(stream)
                    .is_none_or(|(s, _)| Some(*s) != frame.session)
                {
                    return Err(TyuErrorCode::InvalidState.into());
                }
                self.sessions.media.remove(stream);
                self.sessions.media_senders.remove(stream);
                self.media_writers.remove(stream);
                if let Some(r) = self.reassembler.get_mut(&peer) {
                    r.remove_stream(frame.session.ok_or(TyuErrorCode::InvalidState)?, *stream);
                }
                self.emit(TyuEvent::MediaStopped {
                    peer,
                    stream: *stream,
                })
                .await;
            }
            Message::MediaKeyframeRequest { stream } => {
                self.check_session(peer, frame.session)?;
                self.emit(TyuEvent::KeyframeRequested {
                    peer,
                    session: frame.session.ok_or(TyuErrorCode::InvalidState)?,
                    stream: *stream,
                })
                .await;
            }
            Message::Ping(_) | Message::Heartbeat => {}
            Message::Goodbye => {
                self.remove(peer).await;
            }
            _ => return Err(TyuErrorCode::Unsupported.into()),
        }
        Ok(())
    }
    fn media_direction(
        &self,
        peer: DeviceId,
        session: SessionId,
        format: &MediaMetadata,
        inbound: bool,
    ) -> Result<()> {
        use Capability::*;
        let kind = &self
            .sessions
            .sessions
            .get(&session)
            .ok_or(TyuErrorCode::InvalidState)?
            .kind;
        let pair = match (kind, format) {
            (SessionKind::Display(TyuMode::Monitor), MediaMetadata::Video(_)) => {
                (MonitorSource, MonitorReceiver)
            }
            (SessionKind::Display(TyuMode::Mirror), MediaMetadata::Video(_)) => {
                (MirrorSource, MirrorReceiver)
            }
            (SessionKind::Service(ServiceKind::Camera), MediaMetadata::Video(_)) => {
                (CameraSource, CameraReceiver)
            }
            (SessionKind::Service(ServiceKind::Microphone), MediaMetadata::Audio(_)) => {
                (MicrophoneSource, MicrophoneReceiver)
            }
            _ => return Err(TyuErrorCode::Unsupported.into()),
        };
        self.direction(peer, pair.0, pair.1, inbound)
    }
    fn rpc(&mut self, peer: DeviceId, frame: Frame, storage: bool) -> Result<()> {
        frame.validate()?;
        if self.tasks.len() >= 64 {
            return Err(TyuErrorCode::Limit.into());
        }
        if storage {
            self.direction(
                peer,
                Capability::StorageConsumer,
                Capability::StorageProvider,
                false,
            )?;
        } else {
            self.direction(
                peer,
                Capability::TouchSource,
                Capability::InputReceiver,
                false,
            )?;
            self.check_session(peer, frame.session)?;
        }
        let link = self.links.get(&peer).ok_or(TyuErrorCode::NotAvailable)?;
        let connection = link.connection.clone();
        let cancel = link.cancel.clone();
        let events = self.context.events.clone();
        self.tasks.spawn(async move {
            let result=tokio::select! {_=cancel.cancelled()=>Err(TyuErrorCode::Cancelled.into()),result=transport::timed(async {
                let (mut tx,mut rx)=connection.open_bi().await.map_err(transport::neterr)?;transport::write(&mut tx,&frame).await?;tx.finish().map_err(transport::neterr)?;
                let reply=transport::read(&mut rx).await?;if reply.request!=frame.request {return Err(TyuErrorCode::Malformed.into());}
                match reply.message {Message::StorageResult(result)=>Ok(Some(result)),Message::Ack=>Ok(None),Message::Error {code}=>Err(code.into()),_=>Err(TyuErrorCode::Malformed.into())}
            })=>result};
            let event=if storage {TyuEvent::StorageResult {peer,request:frame.request,result:result.map(|r|r.unwrap_or(StorageReply::Done)).map_err(|e|e.code())}} else {match result {Ok(_)=>return,Err(e)=>TyuEvent::Error {request:Some(frame.request),code:e.code()}}};
            let _sent=events.send(event).await;
        });
        Ok(())
    }
    fn send_file(&mut self, id: TransferId) -> Result<()> {
        let peer = self
            .transfers
            .get(&id)
            .ok_or(TyuErrorCode::InvalidState)?
            .peer;
        self.direction(peer, Capability::FileSend, Capability::FileReceive, false)?;
        let t = self
            .transfers
            .get_mut(&id)
            .ok_or(TyuErrorCode::InvalidState)?;
        let link = self.links.get(&peer).ok_or(TyuErrorCode::NotAvailable)?;
        let connection = link.connection.clone();
        let cancel = link.cancel.clone();
        let job = files::SendJob {
            peer,
            id,
            path: t.path.clone(),
            max_size: self.config.max_file_size,
            state: t.state.subscribe(),
            cancel: cancel.clone(),
            events: self.context.events.clone(),
        };
        t.running = true;
        let internal = self.context.internal.clone();
        self.tasks.spawn(async move {
            let result=tokio::select! {_=cancel.cancelled()=>Err(TyuErrorCode::Cancelled.into()),result=files::send(connection,job)=>result};
            let _sent=internal.send(Internal::TransferDone {peer,id,result}).await;
        });
        Ok(())
    }
    pub fn send_media(&mut self, peer: DeviceId, packet: MediaPacket) -> Result<()> {
        self.check_session(peer, Some(packet.session_id))?;
        if self.sessions.media_senders.get(&packet.stream_id) != Some(&self.context.device.id) {
            return Err(TyuErrorCode::Unauthorized.into());
        }
        if self
            .sessions
            .media
            .get(&packet.stream_id)
            .is_none_or(|(s, _)| *s != packet.session_id)
        {
            return Err(TyuErrorCode::InvalidState.into());
        }
        let link = self.links.get(&peer).ok_or(TyuErrorCode::NotAvailable)?;
        let bytes = packet.encode()?;
        let stream = packet.stream_id;
        let writer = match self.media_writers.get(&stream) {
            Some(writer) if !writer.is_closed() => writer.clone(),
            _ => {
                // Packets of one media stream travel in order on one unidirectional QUIC
                // stream: lost packets are retransmitted instead of dropping the frame.
                let (tx, mut rx) = mpsc::channel::<bytes::Bytes>(4096);
                let connection = link.connection.clone();
                let cancel = link.cancel.clone();
                self.tasks.spawn(async move {
                    let Ok(mut send) = connection.open_uni().await else {
                        return;
                    };
                    if send.write_all(MEDIA_STREAM_TAG).await.is_err() {
                        return;
                    }
                    loop {
                        tokio::select! {
                            _ = cancel.cancelled() => break,
                            next = rx.recv() => {
                                let Some(bytes) = next else { break; };
                                if send.write_all(&(bytes.len() as u32).to_be_bytes()).await.is_err()
                                    || send.write_all(&bytes).await.is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    let _ = send.finish();
                });
                self.media_writers.insert(stream, tx.clone());
                tx
            }
        };
        writer
            .try_send(bytes)
            .map_err(|_| TyuErrorCode::Limit.into())
    }
    pub async fn receive_media(&mut self, peer: DeviceId, packet: MediaPacket) {
        if self.sessions.media_senders.get(&packet.stream_id) != Some(&peer) {
            return;
        }
        if self.check_session(peer, Some(packet.session_id)).is_err()
            || self
                .sessions
                .media
                .get(&packet.stream_id)
                .is_none_or(|(s, _)| *s != packet.session_id)
        {
            return;
        }
        let session = packet.session_id;
        let stream = packet.stream_id;
        let result = self
            .reassembler
            .entry(peer)
            // Wi-Fi bursts spread a large keyframe over hundreds of ms; a tight window would
            // discard it and stall the stream until the next one.
            .or_insert_with(|| Reassembler::new(Duration::from_millis(500)))
            .push(packet, Instant::now());
        match result {
            Ok(Some(data)) => {
                self.emit(TyuEvent::MediaFrame {
                    peer,
                    session,
                    stream,
                    data,
                })
                .await
            }
            Err(error) => {
                self.emit(TyuEvent::Error {
                    request: None,
                    code: error.code(),
                })
                .await
            }
            _ => {}
        }
    }
}
