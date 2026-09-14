use super::{TyuEvent, actor::Internal, requests::MEDIA_STREAM_TAG};

/// Reads length-prefixed media packets from one reliable unidirectional stream until it ends.
async fn media_stream(context: Context, peer: DeviceId, mut recv: quinn::RecvStream) -> Result<()> {
    let mut tag = [0u8; 4];
    if recv.read_exact(&mut tag).await.is_err() || tag != MEDIA_STREAM_TAG {
        return Ok(());
    }
    loop {
        let mut len = [0u8; 4];
        if recv.read_exact(&mut len).await.is_err() {
            return Ok(());
        }
        let len = u32::from_be_bytes(len) as usize;
        if len == 0 || len > 65535 {
            return Err(TyuErrorCode::Malformed.into());
        }
        let mut bytes = vec![0u8; len];
        recv.read_exact(&mut bytes)
            .await
            .map_err(|_| TyuErrorCode::Malformed)?;
        if let Ok(packet) = crate::media::MediaPacket::decode(bytes.into())
            && context
                .internal
                .send(Internal::Media { peer, packet })
                .await
                .is_err()
        {
            return Err(TyuErrorCode::Cancelled.into());
        }
    }
}
use crate::{
    Frame, Message, Result, TyuErrorCode, files,
    identity::{
        DeviceFingerprint, DeviceIdentity, PairingTicket, Store, TicketBook, TrustedPeer,
        now_seconds, random_nonce,
    },
    storage::StorageProvider,
    transport,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use tyu_types::*;
#[derive(Clone)]
pub(super) struct Context {
    pub identity: DeviceIdentity,
    pub device: DeviceInfo,
    pub store: Arc<dyn Store>,
    pub tickets: Arc<Mutex<TicketBook>>,
    pub internal: mpsc::Sender<Internal>,
    pub events: mpsc::Sender<TyuEvent>,
    pub cancel: CancellationToken,
    pub receive: Option<Arc<files::ReceiveRoot>>,
    pub storage: Option<Arc<dyn StorageProvider>>,
    pub allow_nearby_pairing: bool,
}
pub(super) struct Authenticated {
    pub connection: quinn::Connection,
    pub device: DeviceInfo,
    pub send: quinn::SendStream,
    pub recv: quinn::RecvStream,
}
fn validate_peer(device: &DeviceInfo, pin: DeviceFingerprint) -> Result<()> {
    if device.id != pin.device_id() {
        return Err(TyuErrorCode::Unauthorized.into());
    }
    Ok(())
}
pub(super) async fn outgoing(
    context: Context,
    endpoint: quinn::Endpoint,
    addresses: Vec<std::net::SocketAddr>,
    expected: transport::ServerIdentity,
    ticket: Option<PairingTicket>,
) -> Result<Authenticated> {
    // Race TLS only: send exactly one approval request on the first verified route.
    // A virtual/unreachable address must not hide a reachable Wi-Fi address.
    let mut attempts = JoinSet::new();
    for address in addresses.into_iter().take(8) {
        let endpoint = endpoint.clone();
        let identity = context.identity.clone();
        attempts.spawn(async move {
            match expected {
                transport::ServerIdentity::Trusted(pin) => {
                    transport::connect(&endpoint, &identity, address, pin).await
                }
                transport::ServerIdentity::Discovered(_) => {
                    transport::connect_identity(&endpoint, &identity, address, expected).await
                }
            }
        });
    }
    let mut failure = TyuErrorCode::NotAvailable.into();
    let connection = loop {
        match attempts.join_next().await {
            Some(Ok(Ok(connection))) => break connection,
            Some(Ok(Err(error))) => failure = error,
            Some(Err(_)) => failure = TyuErrorCode::Cancelled.into(),
            None => return Err(failure),
        }
    };
    attempts.abort_all();
    while let Some(result) = attempts.join_next().await {
        if let Ok(Ok(unused)) = result {
            unused.close(0u32.into(), b"alternate route");
        }
    }
    let pin = transport::peer_pin(&connection)?;
    let nearby = matches!(expected, transport::ServerIdentity::Discovered(_));
    let result = outgoing_handshake(&context, &connection, pin, ticket, nearby).await;
    if result.is_err() {
        connection.close(1u32.into(), b"authentication failed");
    }
    result.map(|(device, send, recv)| Authenticated {
        connection,
        device,
        send,
        recv,
    })
}
async fn outgoing_handshake(
    context: &Context,
    connection: &quinn::Connection,
    pin: DeviceFingerprint,
    ticket: Option<PairingTicket>,
    nearby: bool,
) -> Result<(DeviceInfo, quinn::SendStream, quinn::RecvStream)> {
    let (mut send, mut recv) = connection.open_bi().await.map_err(transport::neterr)?;
    let hello = Frame::new(Message::Hello(context.device.clone()));
    transport::write(&mut send, &hello).await?;
    let reply = transport::timed(transport::read(&mut recv)).await?;
    if reply.request != hello.request {
        return Err(TyuErrorCode::Malformed.into());
    }
    let Message::HelloAck(device) = reply.message else {
        return Err(TyuErrorCode::Unauthorized.into());
    };
    validate_peer(&device, pin)?;
    let pairing = ticket.is_some() || nearby;
    let auth = Frame::new(if let Some(t) = ticket {
        if t.expires <= now_seconds() {
            return Err(TyuErrorCode::Expired.into());
        }
        Message::PairRequest {
            ticket: t.ticket,
            nonce: random_nonce()?,
        }
    } else if nearby {
        Message::PairNearbyRequest {
            nonce: random_nonce()?,
        }
    } else {
        Message::Auth
    });
    transport::write(&mut send, &auth).await?;
    let reply = tokio::time::timeout(Duration::from_secs(120), transport::read(&mut recv))
        .await
        .map_err(|_| TyuErrorCode::Timeout)??;
    if reply.request != auth.request {
        return Err(TyuErrorCode::Malformed.into());
    }
    match reply.message {
        Message::PairComplete if pairing => {}
        Message::Ack if !pairing => {}
        Message::PairReject(e) | Message::Error { code: e } => return Err(e.into()),
        _ => return Err(TyuErrorCode::Unauthorized.into()),
    }
    context.store.put_peer(TrustedPeer {
        device: device.clone(),
        fingerprint: pin,
        endpoint: connection.remote_address(),
        revoked: false,
    })?;
    transport::write(&mut send, &auth.reply(Message::PairConfirm)).await?;
    if pairing {
        context
            .events
            .send(TyuEvent::PairingCompleted { peer: device.id })
            .await
            .map_err(|_| TyuErrorCode::Cancelled)?;
    }
    Ok((device, send, recv))
}
pub(super) async fn incoming(
    context: Context,
    connection: quinn::Connection,
) -> Result<Authenticated> {
    let result = tokio::select! {
        _=connection.closed()=>Err(TyuErrorCode::Cancelled.into()),
        result=incoming_handshake(&context,&connection)=>result,
    };
    if result.is_err() {
        connection.close(1u32.into(), b"authentication failed");
    }
    result.map(|(device, send, recv)| Authenticated {
        connection,
        device,
        send,
        recv,
    })
}
async fn incoming_handshake(
    context: &Context,
    connection: &quinn::Connection,
) -> Result<(DeviceInfo, quinn::SendStream, quinn::RecvStream)> {
    let pin = transport::peer_pin(connection)?;
    let (mut send, mut recv) =
        transport::timed(async { connection.accept_bi().await.map_err(transport::neterr) }).await?;
    let hello = transport::timed(transport::read(&mut recv)).await?;
    let Message::Hello(device) = &hello.message else {
        return Err(TyuErrorCode::Malformed.into());
    };
    let device = device.clone();
    validate_peer(&device, pin)?;
    transport::write(
        &mut send,
        &hello.reply(Message::HelloAck(context.device.clone())),
    )
    .await?;
    let auth = transport::timed(transport::read(&mut recv)).await?;
    let authorization = authorize(context, &device, pin, &auth).await;
    if let Err(e) = authorization {
        transport::write(&mut send, &auth.reply(Message::PairReject(e.code()))).await?;
        send.finish().map_err(transport::neterr)?;
        let _ = tokio::time::timeout(Duration::from_secs(1), send.stopped()).await;
        return Err(e);
    }
    let pairing = matches!(
        auth.message,
        Message::PairRequest { .. } | Message::PairNearbyRequest { .. }
    );
    if pairing {
        context.store.put_peer(TrustedPeer {
            device: device.clone(),
            fingerprint: pin,
            endpoint: connection.remote_address(),
            revoked: false,
        })?;
    }
    transport::write(
        &mut send,
        &auth.reply(if pairing {
            Message::PairComplete
        } else {
            Message::Ack
        }),
    )
    .await?;
    let confirmed = transport::timed(transport::read(&mut recv)).await?;
    if confirmed.request != auth.request || confirmed.message != Message::PairConfirm {
        return Err(TyuErrorCode::Malformed.into());
    }
    if pairing {
        context
            .events
            .send(TyuEvent::PairingCompleted { peer: device.id })
            .await
            .map_err(|_| TyuErrorCode::Cancelled)?;
    }
    Ok((device, send, recv))
}
async fn authorize(
    context: &Context,
    device: &DeviceInfo,
    pin: DeviceFingerprint,
    auth: &Frame,
) -> Result<()> {
    let trusted = context
        .store
        .peers()?
        .into_iter()
        .find(|p| p.device.id == device.id);
    if trusted.as_ref().is_some_and(|p| p.revoked) {
        return Err(TyuErrorCode::Revoked.into());
    }
    match auth.message {
        Message::Auth => {
            if trusted.is_none_or(|p| p.fingerprint != pin) {
                return Err(TyuErrorCode::Unauthorized.into());
            }
            Ok(())
        }
        Message::PairRequest { .. } | Message::PairNearbyRequest { .. } => {
            let expiry = if let Message::PairRequest { ticket, .. } = auth.message {
                context
                    .tickets
                    .lock()
                    .map_err(|_| crate::CoreError::Store)?
                    .consume(ticket)?
            } else {
                if !context.allow_nearby_pairing {
                    return Err(TyuErrorCode::Unauthorized.into());
                }
                now_seconds() + 120
            };
            let (reply, rx) = oneshot::channel();
            context
                .internal
                .send(Internal::Approval {
                    request: auth.request,
                    device: device.clone(),
                    fingerprint: pin,
                    expiry,
                    reply,
                })
                .await
                .map_err(|_| TyuErrorCode::Cancelled)?;
            tokio::time::timeout(
                Duration::from_secs(expiry.saturating_sub(now_seconds())),
                rx,
            )
            .await
            .map_err(|_| TyuErrorCode::Expired)?
            .map_err(|_| TyuErrorCode::Rejected)?
        }
        _ => Err(TyuErrorCode::Unauthorized.into()),
    }
}
pub(super) async fn run(
    context: Context,
    auth: Authenticated,
    mut outgoing: mpsc::Receiver<Frame>,
    generation: usize,
) {
    let peer = auth.device.id;
    let connection = auth.connection.clone();
    let cancel = context.cancel.child_token();
    let mut children: JoinSet<Result<()>> = JoinSet::new();
    let incoming = context.internal.clone();
    let mut recv = auth.recv;
    let mut send = auth.send;
    children.spawn(async move {
        loop {
            let frame = match transport::read(&mut recv).await {
                Ok(frame) => frame,
                Err(error) => break Err(error),
            };
            if incoming
                .send(Internal::Inbound {
                    peer,
                    generation,
                    frame,
                    reply: None,
                })
                .await
                .is_err()
            {
                break Err(TyuErrorCode::Cancelled.into());
            }
        }
    });
    let mut metrics = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _=context.cancel.cancelled()=>break,
            _=connection.closed()=>break,
            child=children.join_next(),if !children.is_empty()=>{if matches!(child,Some(Ok(Err(_)))|Some(Err(_))) {break;}},
            frame=outgoing.recv()=>{let Some(frame)=frame else {break;};if transport::write(&mut send,&frame).await.is_err() {break;}},
            stream=connection.accept_bi()=>{
                let Ok((send,recv))=stream else {break;};
                if children.len()>=16 {drop(send);drop(recv);continue;}
                let ctx=context.clone();let device=auth.device.clone();let token=cancel.clone();
                children.spawn(async move {tokio::select! {_=token.cancelled()=>Ok(()),result=auxiliary(ctx,device,generation,send,recv)=>result}});
            },
            datagram=connection.read_datagram()=>{
                let Ok(data)=datagram else {break;};
                if let Ok(packet)=crate::media::MediaPacket::decode(data) && context.internal.send(Internal::Media {peer,packet}).await.is_err() {break;}
            },
            stream=connection.accept_uni()=>{
                let Ok(recv)=stream else {break;};
                if children.len()>=16 {drop(recv);continue;}
                let ctx=context.clone();let token=cancel.clone();
                children.spawn(async move {tokio::select! {_=token.cancelled()=>Ok(()),result=media_stream(ctx,peer,recv)=>result}});
            },
            _=metrics.tick()=>{
                let stats=connection.stats();
                if context.events.send(TyuEvent::MetricsUpdated {peer,rtt_micros:connection.rtt().as_micros() as u64,sent_bytes:stats.udp_tx.bytes,received_bytes:stats.udp_rx.bytes}).await.is_err() {break;}
            }
        }
    }
    cancel.cancel();
    connection.close(0u32.into(), b"goodbye");
    children.abort_all();
    while children.join_next().await.is_some() {}
    let _sent = context
        .internal
        .send(Internal::Lost { peer, generation })
        .await;
}
async fn auxiliary(
    context: Context,
    device: DeviceInfo,
    generation: usize,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) -> Result<()> {
    let frame = transport::timed(transport::read(&mut recv)).await?;
    let peer = device.id;
    match &frame.message {
        Message::FileOffer(meta)
            if device.capabilities.contains(&Capability::FileSend)
                && context
                    .device
                    .capabilities
                    .contains(&Capability::FileReceive) =>
        {
            let Some(root) = context.receive.clone() else {
                transport::write(
                    &mut send,
                    &frame.reply(Message::FileReject(TyuErrorCode::NotAvailable)),
                )
                .await?;
                return Ok(());
            };
            let id = meta.id;
            if let Err(error) =
                files::receive(peer, frame, send, recv, root, context.events.clone()).await
            {
                context
                    .events
                    .send(TyuEvent::TransferFailed {
                        peer,
                        id,
                        code: error.code(),
                    })
                    .await
                    .map_err(|_| TyuErrorCode::Cancelled)?;
            }
            Ok(())
        }
        Message::Storage(op)
            if device.capabilities.contains(&Capability::StorageConsumer)
                && context
                    .device
                    .capabilities
                    .contains(&Capability::StorageProvider) =>
        {
            let result = if let Some(provider) = context.storage.clone() {
                let op = op.clone();
                tokio::task::spawn_blocking(move || provider.execute(op))
                    .await
                    .map_err(|_| TyuErrorCode::Io)?
            } else {
                Err(TyuErrorCode::NotAvailable.into())
            };
            let message = match result {
                Ok(result) => Message::StorageResult(result),
                Err(e) => Message::Error { code: e.code() },
            };
            transport::write(&mut send, &frame.reply(message)).await?;
            send.finish().map_err(transport::neterr)?;
            Ok(())
        }
        Message::Input(_) => {
            let (reply, rx) = oneshot::channel();
            context
                .internal
                .send(Internal::Inbound {
                    peer,
                    generation,
                    frame,
                    reply: Some(reply),
                })
                .await
                .map_err(|_| TyuErrorCode::Cancelled)?;
            let response = rx.await.map_err(|_| TyuErrorCode::Cancelled)?;
            transport::write(&mut send, &response).await?;
            send.finish().map_err(transport::neterr)?;
            Ok(())
        }
        _ => {
            transport::write(
                &mut send,
                &frame.reply(Message::Error {
                    code: TyuErrorCode::Unsupported,
                }),
            )
            .await?;
            send.finish().map_err(transport::neterr)?;
            Ok(())
        }
    }
}
