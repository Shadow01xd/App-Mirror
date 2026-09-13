use std::{sync::Arc, time::Duration};
use tyu_core::{identity::*, storage::*, *};
async fn event(node: &mut TyuNode, predicate: impl Fn(&TyuEvent) -> bool) -> TyuEvent {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let e = node.events.recv().await.expect("node closed");
            if predicate(&e) {
                return e;
            }
            if matches!(
                e,
                TyuEvent::Error { .. } | TyuEvent::ConnectionFailed { .. }
            ) {
                eprintln!("Unexpected event: {e:?}");
            }
        }
    })
    .await
    .expect("event timeout")
}
fn config(name: &str, store: Arc<MemoryStore>, android: bool) -> NodeConfig {
    use Capability::*;
    let mut c = NodeConfig::new(
        name,
        if android {
            DevicePlatform::Android
        } else {
            DevicePlatform::Windows
        },
        store,
    );
    c.bind = "127.0.0.1:0".parse().unwrap();
    c.capabilities = if android {
        vec![
            MonitorReceiver,
            MirrorSource,
            CameraSource,
            MicrophoneSource,
            StorageProvider,
            FileSend,
            FileReceive,
            ClipboardSource,
            ClipboardReceiver,
            TouchSource,
        ]
    } else {
        vec![
            MonitorSource,
            MirrorReceiver,
            CameraReceiver,
            MicrophoneReceiver,
            StorageConsumer,
            FileSend,
            FileReceive,
            ClipboardSource,
            ClipboardReceiver,
            InputReceiver,
        ]
    };
    c
}
async fn ticket(a: &mut TyuNode) -> PairingTicket {
    a.command(TyuCommand::BeginPairing {
        endpoint: a.address,
    })
    .await
    .unwrap();
    let TyuEvent::PairingStarted { ticket } =
        event(a, |e| matches!(e, TyuEvent::PairingStarted { .. })).await
    else {
        unreachable!()
    };
    ticket
}
async fn pair(a: &mut TyuNode, b: &mut TyuNode) -> PairingTicket {
    let ticket = ticket(a).await;
    b.command(TyuCommand::Pair {
        ticket: ticket.clone(),
    })
    .await
    .unwrap();
    let TyuEvent::PairingRequest { request, .. } =
        event(a, |e| matches!(e, TyuEvent::PairingRequest { .. })).await
    else {
        unreachable!()
    };
    a.command(TyuCommand::AcceptPairing { request })
        .await
        .unwrap();
    event(a, |e| matches!(e, TyuEvent::Connected { .. })).await;
    event(b, |e| matches!(e, TyuEvent::Connected { .. })).await;
    ticket
}
async fn disconnect(a: &mut TyuNode, b: &mut TyuNode) {
    a.command(TyuCommand::Disconnect { peer: b.device.id })
        .await
        .unwrap();
    event(a, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
    event(b, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_pending_pairing_does_not_create_trust_and_allows_retry() {
    let a_store = Arc::new(MemoryStore::default());
    let b_store = Arc::new(MemoryStore::default());
    let mut a = TyuNode::start(config("Desktop", a_store.clone(), false))
        .await
        .unwrap();
    let mut b = TyuNode::start(config("Android", b_store.clone(), true))
        .await
        .unwrap();
    let invitation = ticket(&mut a).await;
    b.command(TyuCommand::Pair { ticket: invitation })
        .await
        .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::PairingRequest { .. })).await;
    b.command(TyuCommand::Disconnect { peer: a.device.id })
        .await
        .unwrap();
    event(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Cancelled,
                ..
            }
        )
    })
    .await;
    assert!(a_store.peers().unwrap().is_empty());
    assert!(b_store.peers().unwrap().is_empty());
    pair(&mut a, &mut b).await;
    assert_eq!(a_store.peers().unwrap().len(), 1);
    assert_eq!(b_store.peers().unwrap().len(), 1);
    disconnect(&mut a, &mut b).await;
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_real_loopback_journey() {
    let a_store = Arc::new(MemoryStore::default());
    let b_store = Arc::new(MemoryStore::default());
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let storage = tempfile::tempdir().unwrap();
    std::fs::write(storage.path().join("hello.txt"), b"remote storage").unwrap();
    let mut b_cfg = config("Android-like", b_store.clone(), true);
    b_cfg.receive_directory = Some(destination.path().to_owned());
    b_cfg.storage = Some(Arc::new(
        LocalFilesystemProvider::new(storage.path(), 1000000).unwrap(),
    ));
    let mut a = TyuNode::start(config("Windows-like", a_store.clone(), false))
        .await
        .unwrap();
    let mut b = TyuNode::start(b_cfg).await.unwrap();
    pair(&mut a, &mut b).await;
    assert_eq!(a_store.peers().unwrap().len(), 1);
    assert_eq!(b_store.peers().unwrap().len(), 1);
    disconnect(&mut a, &mut b).await;
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::Connected { .. })).await;
    event(&mut b, |e| {
        matches!(e, TyuEvent::CapabilitiesNegotiated { .. })
    })
    .await;
    a.command(TyuCommand::Ping {
        peer: b.device.id,
        value: 42,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::Pong { value: 42, .. })).await;
    a.command(TyuCommand::StartMode {
        peer: b.device.id,
        mode: TyuMode::Bypass,
    })
    .await
    .unwrap();
    let TyuEvent::ModeStarted {
        session: bypass, ..
    } = event(&mut a, |e| {
        matches!(
            e,
            TyuEvent::ModeStarted {
                mode: TyuMode::Bypass,
                ..
            }
        )
    })
    .await
    else {
        unreachable!()
    };
    let path = source.path().join("payload.bin");
    let data: Vec<u8> = (0..2_000_000).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let transfer = TransferId::new();
    a.command(TyuCommand::SendFile {
        peer: b.device.id,
        id: transfer,
        path,
    })
    .await
    .unwrap();
    // Both event queues are drained during transfer to exercise bounded backpressure.
    let (a_done, b_done) = tokio::join!(
        event(
            &mut a,
            |e| matches!(e,TyuEvent::TransferComplete {id,..} if *id==transfer)
        ),
        event(
            &mut b,
            |e| matches!(e,TyuEvent::TransferComplete {id,..} if *id==transfer)
        )
    );
    assert!(matches!(a_done, TyuEvent::TransferComplete { .. }));
    assert!(matches!(b_done, TyuEvent::TransferComplete { .. }));
    assert_eq!(
        blake3::hash(&std::fs::read(destination.path().join("payload.bin")).unwrap()),
        blake3::hash(&data)
    );
    a.command(TyuCommand::ListStorage {
        peer: b.device.id,
        path: "".into(),
    })
    .await
    .unwrap();
    let TyuEvent::StorageResult { result, .. } =
        event(&mut a, |e| matches!(e, TyuEvent::StorageResult { .. })).await
    else {
        unreachable!()
    };
    assert!(matches!(result,Ok(StorageReply::Entries(e)) if e.len()==1));
    a.command(TyuCommand::StartService {
        peer: b.device.id,
        service: ServiceKind::Camera,
    })
    .await
    .unwrap();
    let TyuEvent::ServiceStarted {
        session: camera, ..
    } = event(&mut a, |e| {
        matches!(
            e,
            TyuEvent::ServiceStarted {
                service: ServiceKind::Camera,
                ..
            }
        )
    })
    .await
    else {
        unreachable!()
    };
    // Real media bytes over DATAGRAM, without platform capture APIs.
    let stream = StreamId::new();
    let mut f = Frame::new(Message::MediaStart {
        stream,
        format: MediaMetadata::Video(VideoFormat {
            width: 640,
            height: 480,
            fps: 30,
            bitrate: 1000000,
            codec: VideoCodec::H264,
            orientation: Orientation::Landscape,
        }),
    });
    f.session = Some(camera);
    b.command(TyuCommand::Request {
        peer: a.device.id,
        frame: f,
    })
    .await
    .unwrap();
    event(&mut b, |e| matches!(e, TyuEvent::MediaStarted { .. })).await;
    let bytes = bytes::Bytes::from(vec![7; 4000]);
    for packet in media::packetize(camera, stream, 1, 0, true, bytes.clone(), 1200).unwrap() {
        b.send_media(a.device.id, packet).await.unwrap();
    }
    let TyuEvent::MediaFrame { data: received, .. } =
        event(&mut a, |e| matches!(e, TyuEvent::MediaFrame { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(received, bytes);
    a.command(TyuCommand::StopService {
        peer: b.device.id,
        session: camera,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::ServiceStopped { .. })).await;
    a.command(TyuCommand::StopMode {
        peer: b.device.id,
        session: bypass,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::ModeStopped { .. })).await;
    a.command(TyuCommand::StartMode {
        peer: b.device.id,
        mode: TyuMode::Monitor,
    })
    .await
    .unwrap();
    let TyuEvent::ModeStarted {
        session: monitor, ..
    } = event(&mut a, |e| {
        matches!(
            e,
            TyuEvent::ModeStarted {
                mode: TyuMode::Monitor,
                ..
            }
        )
    })
    .await
    else {
        unreachable!()
    };
    b.command(TyuCommand::SendInput {
        peer: a.device.id,
        session: monitor,
        event: InputEvent::TouchDown {
            contact: 1,
            x: 0.2,
            y: 0.8,
        },
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::InputReceived { .. })).await;
    a.command(TyuCommand::StopMode {
        peer: b.device.id,
        session: monitor,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::ModeStopped { .. })).await;
    b.command(TyuCommand::SendClipboard {
        peer: a.device.id,
        text: "Hola 🙂".into(),
    })
    .await
    .unwrap();
    event(
        &mut a,
        |e| matches!(e,TyuEvent::ClipboardReceived {text,..} if text=="Hola 🙂"),
    )
    .await;
    disconnect(&mut a, &mut b).await;
    let (a, b) = tokio::join!(a.shutdown(), b.shutdown());
    a.unwrap();
    b.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_expired_replayed_wrong_pin_and_revoked() {
    let a_store = Arc::new(MemoryStore::default());
    let mut a = TyuNode::start(config("A", a_store.clone(), false))
        .await
        .unwrap();
    let mut b = TyuNode::start(config("B", Arc::new(MemoryStore::default()), true))
        .await
        .unwrap();
    let t = ticket(&mut a).await;
    let uri = t.uri().unwrap();
    assert_eq!(
        PairingTicket::parse(&uri).unwrap().fingerprint,
        a.fingerprint
    );
    let mut expired = t.clone();
    expired.expires = 0;
    assert!(PairingTicket::parse(&expired.uri().unwrap()).is_err());
    let mut wrong = t.clone();
    wrong.fingerprint = DeviceFingerprint([9; 32]);
    b.command(TyuCommand::Pair { ticket: wrong }).await.unwrap();
    event(&mut b, |e| matches!(e, TyuEvent::ConnectionFailed { .. })).await;
    b.command(TyuCommand::Pair { ticket: t.clone() })
        .await
        .unwrap();
    let TyuEvent::PairingRequest { request, .. } =
        event(&mut a, |e| matches!(e, TyuEvent::PairingRequest { .. })).await
    else {
        unreachable!()
    };
    a.command(TyuCommand::RejectPairing { request })
        .await
        .unwrap();
    event(&mut b, |e| {
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
    b.command(TyuCommand::Pair { ticket: t }).await.unwrap();
    event(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Replay,
                ..
            }
        )
    })
    .await;
    pair(&mut a, &mut b).await;
    disconnect(&mut a, &mut b).await;
    a_store.revoke(b.device.id).unwrap();
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    event(&mut b, |e| {
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
async fn files_pause_resume_after_connection_loss_and_cancel() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let path = source.path().join("resume.bin");
    let data = vec![37; 32 * 1024 * 1024];
    std::fs::write(&path, &data).unwrap();
    let mut a = TyuNode::start(config("A", Arc::new(MemoryStore::default()), false))
        .await
        .unwrap();
    let mut cfg = config("B", Arc::new(MemoryStore::default()), true);
    cfg.receive_directory = Some(destination.path().to_owned());
    let mut b = TyuNode::start(cfg).await.unwrap();
    pair(&mut a, &mut b).await;
    let id = TransferId::new();
    a.command(TyuCommand::SendFile {
        peer: b.device.id,
        id,
        path: path.clone(),
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::TransferProgress { .. })).await;
    a.command(TyuCommand::PauseTransfer { id }).await.unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::TransferPaused { .. })).await;
    // A reliable control ping still completes while a file stream is paused.
    a.command(TyuCommand::Ping {
        peer: b.device.id,
        value: 99,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::Pong { value: 99, .. })).await;
    a.command(TyuCommand::Disconnect { peer: b.device.id })
        .await
        .unwrap();
    let mut failed = false;
    let mut lost = false;
    while !failed || !lost {
        let e = event(&mut a, |e| {
            matches!(
                e,
                TyuEvent::TransferFailed { .. } | TyuEvent::Disconnected { .. }
            )
        })
        .await;
        failed |= matches!(e, TyuEvent::TransferFailed { .. });
        lost |= matches!(e, TyuEvent::Disconnected { .. });
    }
    event(&mut b, |e| matches!(e, TyuEvent::Disconnected { .. })).await;
    let partial = destination
        .path()
        .join(format!("{}-{id}.part", a.device.id));
    let offset = std::fs::metadata(&partial).unwrap().len();
    assert!(offset > 0 && offset < data.len() as u64);
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::Connected { .. })).await;
    event(&mut b, |e| matches!(e, TyuEvent::Connected { .. })).await;
    a.command(TyuCommand::ResumeTransfer { id }).await.unwrap();
    tokio::join!(
        event(&mut a, |e| matches!(e, TyuEvent::TransferComplete { .. })),
        event(&mut b, |e| matches!(e, TyuEvent::TransferComplete { .. }))
    );
    assert_eq!(
        blake3::hash(&std::fs::read(destination.path().join("resume.bin")).unwrap()),
        blake3::hash(&data)
    );
    assert!(!partial.exists());
    let cancel_path = source.path().join("cancel.bin");
    std::fs::write(&cancel_path, &data).unwrap();
    let id = TransferId::new();
    a.command(TyuCommand::SendFile {
        peer: b.device.id,
        id,
        path: cancel_path,
    })
    .await
    .unwrap();
    event(&mut a, |e| matches!(e, TyuEvent::TransferProgress { .. })).await;
    a.command(TyuCommand::CancelTransfer { id }).await.unwrap();
    event(&mut a, |e| {
        matches!(
            e,
            TyuEvent::TransferFailed {
                code: TyuErrorCode::Cancelled,
                ..
            }
        )
    })
    .await;
    assert!(!destination.path().join("cancel.bin").exists());
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_identity_cannot_authenticate_without_pairing() {
    let mut a = TyuNode::start(config("A", Arc::new(MemoryStore::default()), false))
        .await
        .unwrap();
    let store = Arc::new(MemoryStore::default());
    store
        .put_peer(TrustedPeer {
            device: a.device.clone(),
            fingerprint: a.fingerprint,
            endpoint: a.address,
            revoked: false,
        })
        .unwrap();
    let mut b = TyuNode::start(config("B", store, true)).await.unwrap();
    b.command(TyuCommand::Connect { peer: a.device.id })
        .await
        .unwrap();
    event(&mut b, |e| {
        matches!(
            e,
            TyuEvent::ConnectionFailed {
                code: TyuErrorCode::Unauthorized,
                ..
            }
        )
    })
    .await;
    assert!(!matches!(
        a.events.try_recv(),
        Ok(TyuEvent::Connected { .. })
    ));
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_can_start_stop_restart_and_shutdown() {
    let mut n = TyuNode::start(config("Discovery", Arc::new(MemoryStore::default()), false))
        .await
        .unwrap();
    for _ in 0..2 {
        n.command(TyuCommand::StartDiscovery).await.unwrap();
        event(&mut n, |e| matches!(e, TyuEvent::DiscoveryStarted)).await;
        n.command(TyuCommand::StopDiscovery).await.unwrap();
        event(&mut n, |e| matches!(e, TyuEvent::DiscoveryStopped)).await;
    }
    tokio::time::timeout(Duration::from_secs(5), n.shutdown())
        .await
        .unwrap()
        .unwrap();
}
