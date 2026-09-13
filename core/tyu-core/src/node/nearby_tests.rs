use super::*;
use crate::identity::{IdentityStore, MemoryStore, TrustedPeerStore};

async fn next(node: &mut TyuNode, predicate: impl Fn(&TyuEvent) -> bool) -> TyuEvent {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = node.events.recv().await.unwrap();
            if predicate(&event) {
                return event;
            }
        }
    })
    .await
    .expect("native event timed out")
}

// Seed only the result of discovery; all admission, TLS, approval and persistence
// run through the production actor and real QUIC sockets.
async fn discovered(
    store: Arc<MemoryStore>,
    peer: DeviceId,
    addresses: Vec<SocketAddr>,
) -> TyuNode {
    let mut config = NodeConfig::new("Phone", DevicePlatform::Android, store.clone());
    config.bind = "127.0.0.1:0".parse().unwrap();
    let identity = DeviceIdentity::generate("Phone").unwrap();
    store.save_identity(&identity).unwrap();
    let device = DeviceInfo {
        id: identity.id(),
        name: "Phone".into(),
        platform: DevicePlatform::Android,
        capabilities: vec![],
    };
    let endpoint = transport::endpoint(&identity, config.bind).unwrap();
    let address = endpoint.local_addr().unwrap();
    let (commands, rx) = mpsc::channel(128);
    let (events, evrx) = mpsc::channel(256);
    let (media, mediarx) = mpsc::channel(64);
    let cancel = CancellationToken::new();
    let mut actor = Actor::new(
        config,
        identity.clone(),
        device.clone(),
        endpoint,
        events,
        cancel.clone(),
    )
    .unwrap();
    actor
        .internal_event(Internal::Found {
            id: peer,
            endpoints: addresses,
        })
        .await;
    let token = cancel.clone();
    let worker = tokio::spawn(async move {
        tokio::select! { _=token.cancelled()=>{}, _=actor.run(rx, mediarx)=>{} }
        actor.shutdown().await;
    });
    TyuNode {
        device,
        address,
        fingerprint: identity.fingerprint(),
        commands,
        events: evrx,
        media,
        cancel,
        worker: Some(worker),
    }
}
async fn desktop(store: Arc<MemoryStore>, nearby: bool) -> TyuNode {
    let mut config = NodeConfig::new("PC", DevicePlatform::Windows, store);
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.allow_nearby_pairing = nearby;
    TyuNode::start(config).await.unwrap()
}
async fn approval(a: &mut TyuNode) -> RequestId {
    let TyuEvent::PairingRequest { request, .. } =
        next(a, |e| matches!(e, TyuEvent::PairingRequest { .. })).await
    else {
        unreachable!()
    };
    request
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn discovered_pair_requires_approval_rejects_retries_reconnects_and_revokes() {
    let a_store = Arc::new(MemoryStore::default());
    let b_store = Arc::new(MemoryStore::default());
    let mut a = desktop(a_store.clone(), true).await;
    let mut b = discovered(
        b_store.clone(),
        a.device.id,
        vec![
            SocketAddr::from(([127, 0, 0, 2], a.address.port())),
            a.address,
        ],
    )
    .await;
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    let request = approval(&mut a).await;
    assert!(a_store.peers().unwrap().is_empty());
    assert!(b_store.peers().unwrap().is_empty());
    a.command(TyuCommand::RejectPairing { request })
        .await
        .unwrap();
    next(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Rejected,
                ..
            }
        )
    })
    .await;
    assert!(a_store.peers().unwrap().is_empty());
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    let request = approval(&mut a).await;
    a.command(TyuCommand::AcceptPairing { request })
        .await
        .unwrap();
    next(&mut a, |e| matches!(e, TyuEvent::Connected { .. })).await;
    next(&mut b, |e| matches!(e, TyuEvent::Connected { .. })).await;
    assert_eq!(a_store.peers().unwrap()[0].fingerprint, b.fingerprint);
    assert_eq!(b_store.peers().unwrap()[0].fingerprint, a.fingerprint);
    b.command(TyuCommand::Disconnect { peer: a.device.id })
        .await
        .unwrap();
    next(&mut a, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
    next(&mut b, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    next(&mut a, |e| {
        assert!(!matches!(e, TyuEvent::PairingRequest { .. }));
        matches!(e, TyuEvent::Connected { .. })
    })
    .await;
    next(&mut b, |e| matches!(e, TyuEvent::Connected { .. })).await;
    a.command(TyuCommand::ForgetPeer { peer: b.device.id })
        .await
        .unwrap();
    next(&mut b, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    next(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Revoked,
                ..
            }
        )
    })
    .await;
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn discovery_pairing_is_opt_in_and_wrong_identity_cannot_prompt() {
    let a_store = Arc::new(MemoryStore::default());
    let mut a = desktop(a_store.clone(), false).await;
    let mut b = discovered(
        Arc::new(MemoryStore::default()),
        a.device.id,
        vec![a.address],
    )
    .await;
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    next(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Unauthorized,
                ..
            }
        )
    })
    .await;
    assert!(a_store.peers().unwrap().is_empty());
    b.shutdown().await.unwrap();
    let wrong_id = DeviceId::new();
    let mut wrong = discovered(Arc::new(MemoryStore::default()), wrong_id, vec![a.address]).await;
    wrong
        .command(TyuCommand::Connect { peer: wrong_id })
        .await
        .unwrap();
    next(&mut wrong, |e| {
        matches!(e, TyuEvent::ConnectionFailed { .. })
    })
    .await;
    while let Ok(event) = a.events.try_recv() {
        assert!(!matches!(event, TyuEvent::PairingRequest { .. }));
    }
    wrong.shutdown().await.unwrap();
    a.shutdown().await.unwrap();
}
