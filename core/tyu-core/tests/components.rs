use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tyu_core::{identity::*, media::*, storage::*, *};
#[test]
fn persistent_identity_and_revocation_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = WindowsPersistentStore::open(dir.path()).unwrap();
    let identity = DeviceIdentity::generate("PC").unwrap();
    store.save_identity(&identity).unwrap();
    let peer_identity = DeviceIdentity::generate("Phone").unwrap();
    let peer = TrustedPeer {
        device: DeviceInfo {
            id: peer_identity.id(),
            name: "Phone".into(),
            platform: DevicePlatform::Android,
            capabilities: vec![],
        },
        fingerprint: peer_identity.fingerprint(),
        endpoint: "127.0.0.1:9000".parse().unwrap(),
        revoked: false,
    };
    store.put_peer(peer.clone()).unwrap();
    store.revoke(peer.device.id).unwrap();
    drop(store);
    let reopened = WindowsPersistentStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened.load_identity().unwrap().unwrap().fingerprint(),
        identity.fingerprint()
    );
    assert!(reopened.peers().unwrap()[0].revoked);
    assert!(reopened.put_peer(peer).is_err());
    std::fs::write(dir.path().join("identity-v1.bin"), b"corrupt").unwrap();
    assert!(WindowsPersistentStore::open(dir.path()).is_err());
}
#[tokio::test]
async fn store_restores_node_id_and_shutdown_is_bounded() {
    let store = Arc::new(MemoryStore::default());
    let n = TyuNode::start(NodeConfig::new(
        "PC",
        DevicePlatform::Windows,
        store.clone(),
    ))
    .await
    .unwrap();
    let id = n.device.id;
    tokio::time::timeout(Duration::from_secs(5), n.shutdown())
        .await
        .unwrap()
        .unwrap();
    let n = TyuNode::start(NodeConfig::new("PC", DevicePlatform::Windows, store))
        .await
        .unwrap();
    assert_eq!(n.device.id, id);
    n.shutdown().await.unwrap();
}
#[test]
fn storage_real_crud_and_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let p = LocalFilesystemProvider::new(dir.path(), 1024).unwrap();
    p.mkdir("photos").unwrap();
    p.write("photos/a.txt", 0, b"hello world").unwrap();
    assert_eq!(p.read_range("photos/a.txt", 6, 5).unwrap(), b"world");
    assert_eq!(p.stat("photos/a.txt").unwrap().size, 11);
    p.rename("photos/a.txt", "photos/b.txt").unwrap();
    assert_eq!(p.list("photos").unwrap().len(), 1);
    p.delete("photos/b.txt").unwrap();
    p.delete("photos").unwrap();
    assert!(p.list("").unwrap().is_empty());
    for path in [
        "../x",
        "a/../../x",
        "/tmp/x",
        "C:/Windows/x",
        "a\\..\\x",
        "x:stream",
        "a//b",
    ] {
        assert!(p.write(path, 0, b"no").is_err());
        assert!(p.read_range(path, 0, 10).is_err());
        assert!(p.mkdir(path).is_err());
    }
    assert!(p.write("large", 1024, b"x").is_err());
    assert!(p.read_range("large", 0, 1000000).is_err());
}

#[cfg(windows)]
#[test]
fn storage_rejects_windows_junction_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"unchanged").unwrap();
    let link = root.path().join("escape");
    junction::create(outside.path(), &link).unwrap();
    let p = LocalFilesystemProvider::new(root.path(), 1024).unwrap();
    assert!(p.read_range("escape/secret.txt", 0, 100).is_err());
    assert!(p.write("escape/secret.txt", 0, b"changed").is_err());
    assert!(p.mkdir("escape/newdir").is_err());
    assert_eq!(
        std::fs::read(outside.path().join("secret.txt")).unwrap(),
        b"unchanged"
    );
    junction::delete(link).unwrap();
}
#[cfg(unix)]
#[test]
fn storage_rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    let p = LocalFilesystemProvider::new(root.path(), 1024).unwrap();
    assert!(p.read_range("link/secret", 0, 6).is_err());
    assert!(p.write("link/secret", 0, b"bad").is_err());
}
#[test]
fn media_fragment_reorder_duplicate_and_old_frame() {
    let s = SessionId::new();
    let stream = StreamId::new();
    let data = bytes::Bytes::from(vec![42; 5000]);
    let packets = packetize(s, stream, 1, 10, true, data.clone(), 1200).unwrap();
    let now = Instant::now();
    let mut r = Reassembler::new(Duration::from_millis(100));
    assert!(r.push(packets[1].clone(), now).unwrap().is_none());
    assert!(r.push(packets[1].clone(), now).unwrap().is_none());
    let mut complete = None;
    for p in packets.iter().rev() {
        if let Some(frame) = r
            .push(MediaPacket::decode(p.encode().unwrap()).unwrap(), now)
            .unwrap()
        {
            complete = Some(frame);
        }
    }
    assert_eq!(complete, Some(data));
    assert!(r.push(packets[0].clone(), now).unwrap().is_none());
    assert!(r.stats.duplicates > 0);
    assert!(r.stats.dropped > 0);
}
#[test]
fn media_missing_packets_expire_and_request_keyframe() {
    let s = SessionId::new();
    let t = StreamId::new();
    let now = Instant::now();
    let mut r = Reassembler::new(Duration::from_millis(50));
    let p = packetize(s, t, 1, 0, false, vec![1; 3000].into(), 1200).unwrap();
    r.push(p[0].clone(), now).unwrap();
    assert_eq!(r.expire(now + Duration::from_millis(51)), vec![(s, t)]);
    assert_eq!(r.stats.missing_packets, 2);
}
#[test]
fn media_resource_bounds() {
    assert!(
        packetize(
            SessionId::new(),
            StreamId::new(),
            0,
            0,
            false,
            vec![1; 10].into(),
            128
        )
        .is_err()
    );
    let mut p = packetize(
        SessionId::new(),
        StreamId::new(),
        0,
        0,
        false,
        vec![1; 10].into(),
        1200,
    )
    .unwrap()
    .remove(0);
    p.packet_count = 65535;
    assert!(p.encode().is_err());
}
#[test]
fn discovery_advertisement_and_parser() {
    let id = DeviceId::new();
    let info = tyu_core::discovery::advertisement(id, 12345, 5).unwrap();
    assert_eq!(info.get_type(), tyu_core::discovery::SERVICE);
    assert_eq!(
        info.get_property_val_str("id"),
        Some(id.to_string().as_str())
    );
    assert_eq!(
        tyu_core::discovery::parse_properties("1.0", &id.to_string(), 12345).unwrap(),
        id
    );
    assert!(tyu_core::discovery::parse_properties("2.0", &id.to_string(), 12345).is_err());
}
#[test]
fn modes_and_multiple_services_are_independent() {
    use Capability::*;
    use tyu_core::sessions::*;
    let mut m = SessionManager::default();
    let peer = DeviceId::new();
    let a = [MonitorSource, CameraReceiver, MicrophoneReceiver];
    let b = [MonitorReceiver, CameraSource, MicrophoneSource];
    for kind in [
        SessionKind::Display(TyuMode::Monitor),
        SessionKind::Service(ServiceKind::Camera),
        SessionKind::Service(ServiceKind::Microphone),
    ] {
        m.start(SessionId::new(), peer, kind, &a, &b).unwrap();
    }
    assert_eq!(m.sessions.len(), 3);
    m.disconnect(peer);
    assert!(m.sessions.is_empty());
}
