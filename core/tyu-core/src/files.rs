//! Chunked QUIC transfer, .part files, incremental BLAKE3, durable offset resume.
use crate::{CoreError, Frame, Message, Result, TyuErrorCode, TyuEvent, transport};
use cap_std::fs::{Dir, OpenOptions};
use std::io::Read;
use std::{io::SeekFrom, path::Path, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;
use tyu_types::{DeviceId, TransferId, TransferMetadata};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferControl {
    Running,
    Paused,
    Cancelled,
}
pub(crate) struct ReceiveRoot {
    pub dir: Dir,
    pub max_size: u64,
    active: std::sync::Mutex<std::collections::HashSet<(DeviceId, TransferId)>>,
}
impl ReceiveRoot {
    pub fn new(path: &Path, max_size: u64) -> Result<Self> {
        std::fs::create_dir_all(path)?;
        Ok(Self {
            dir: Dir::open_ambient_dir(path, cap_std::ambient_authority())?,
            max_size,
            active: std::sync::Mutex::new(std::collections::HashSet::new()),
        })
    }
}
pub(crate) async fn metadata(
    path: &Path,
    id: TransferId,
    max_size: u64,
) -> Result<(tokio::fs::File, TransferMetadata)> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(TyuErrorCode::PathDenied)?
        .to_string();
    tyu_protocol::validate_filename(&name)?;
    let mut file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    if size > max_size {
        return Err(TyuErrorCode::Limit.into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; tyu_protocol::MAX_CHUNK];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_size {
            return Err(TyuErrorCode::Limit.into());
        }
        hasher.update(&buffer[..n]);
    }
    if total != size {
        return Err(TyuErrorCode::Integrity.into());
    }
    file.seek(SeekFrom::Start(0)).await?;
    Ok((
        file,
        TransferMetadata {
            id,
            name,
            size,
            blake3: *hasher.finalize().as_bytes(),
        },
    ))
}
pub(crate) struct SendJob {
    pub peer: DeviceId,
    pub id: TransferId,
    pub path: std::path::PathBuf,
    pub max_size: u64,
    pub state: watch::Receiver<TransferControl>,
    pub cancel: CancellationToken,
    pub events: mpsc::Sender<TyuEvent>,
}
pub(crate) async fn send(connection: quinn::Connection, mut job: SendJob) -> Result<()> {
    let (mut file, metadata) = metadata(&job.path, job.id, job.max_size).await?;
    let (mut tx, mut rx) = connection.open_bi().await.map_err(transport::neterr)?;
    let offer = Frame::new(Message::FileOffer(metadata.clone()));
    transport::write(&mut tx, &offer).await?;
    let reply = transport::timed(transport::read(&mut rx)).await?;
    if reply.request != offer.request {
        return Err(TyuErrorCode::Malformed.into());
    }
    let mut offset = match reply.message {
        Message::FileAccept { offset } if offset <= metadata.size => offset,
        Message::FileReject(e) | Message::Error { code: e } => return Err(e.into()),
        _ => return Err(TyuErrorCode::Malformed.into()),
    };
    file.seek(SeekFrom::Start(offset)).await?;
    job.events
        .send(TyuEvent::TransferOffered {
            peer: job.peer,
            metadata: metadata.clone(),
        })
        .await
        .map_err(|_| TyuErrorCode::Cancelled)?;
    let mut buffer = vec![0; tyu_protocol::MAX_CHUNK];
    while offset < metadata.size {
        while *job.state.borrow() == TransferControl::Paused {
            tokio::select! { _=job.cancel.cancelled()=>return Err(TyuErrorCode::Cancelled.into()), changed=job.state.changed()=>changed.map_err(|_|TyuErrorCode::Cancelled)? }
        }
        if *job.state.borrow() == TransferControl::Cancelled {
            let _ = tx.reset(1u32.into());
            return Err(TyuErrorCode::Cancelled.into());
        }
        let n = file
            .read(&mut buffer[..((metadata.size - offset) as usize).min(tyu_protocol::MAX_CHUNK)])
            .await?;
        if n == 0 {
            return Err(TyuErrorCode::Integrity.into());
        }
        tokio::select! {_=job.cancel.cancelled()=>return Err(TyuErrorCode::Cancelled.into()),r=tx.write_all(&buffer[..n])=>r.map_err(transport::neterr)?}
        offset += n as u64;
        job.events
            .send(TyuEvent::TransferProgress {
                peer: job.peer,
                id: job.id,
                bytes: offset,
                total: metadata.size,
            })
            .await
            .map_err(|_| TyuErrorCode::Cancelled)?;
    }
    tx.finish().map_err(transport::neterr)?;
    let complete = transport::timed(transport::read(&mut rx)).await?;
    if complete.request != offer.request {
        return Err(TyuErrorCode::Malformed.into());
    }
    match complete.message {
        Message::FileComplete => Ok(()),
        Message::Error { code } => Err(code.into()),
        _ => Err(TyuErrorCode::Malformed.into()),
    }
}
pub(crate) async fn receive(
    peer: DeviceId,
    offer: Frame,
    mut tx: quinn::SendStream,
    mut rx: quinn::RecvStream,
    root: Arc<ReceiveRoot>,
    events: mpsc::Sender<TyuEvent>,
) -> Result<()> {
    let Message::FileOffer(meta) = &offer.message else {
        return Err(TyuErrorCode::Malformed.into());
    };
    let meta = meta.clone();
    let _lease = ReceiveLease::acquire(root.clone(), peer, meta.id)?;
    if meta.size > root.max_size {
        transport::write(
            &mut tx,
            &offer.reply(Message::FileReject(TyuErrorCode::Limit)),
        )
        .await?;
        return Err(TyuErrorCode::Limit.into());
    }
    let result = receive_inner(peer, &meta, &offer, &mut tx, &mut rx, &root, &events).await;
    let response = match &result {
        Ok(()) => Message::FileComplete,
        Err(e) => Message::Error { code: e.code() },
    };
    transport::write(&mut tx, &offer.reply(response)).await?;
    tx.finish().map_err(transport::neterr)?;
    result
}
async fn receive_inner(
    peer: DeviceId,
    meta: &TransferMetadata,
    offer: &Frame,
    tx: &mut quinn::SendStream,
    rx: &mut quinn::RecvStream,
    root: &ReceiveRoot,
    events: &mpsc::Sender<TyuEvent>,
) -> Result<()> {
    tyu_protocol::validate_filename(&meta.name)?;
    // Names include peer + transfer identity: peers cannot resume another peer's partial file.
    let part = format!("{peer}-{}.part", meta.id);
    let sidecar = format!("{peer}-{}.meta", meta.id);
    let expected = postcard::to_allocvec(meta).map_err(|_| TyuErrorCode::Malformed)?;
    let resumed = root.dir.try_exists(&sidecar)?;
    if resumed {
        if root
            .dir
            .symlink_metadata(&sidecar)?
            .file_type()
            .is_symlink()
            || root.dir.symlink_metadata(&part)?.file_type().is_symlink()
        {
            return Err(TyuErrorCode::PathDenied.into());
        }
        let mut existing = root.dir.open(&sidecar)?;
        let mut saved = Vec::new();
        std::io::Read::take(&mut existing, 4096).read_to_end(&mut saved)?;
        if saved != expected {
            return Err(TyuErrorCode::Integrity.into());
        }
    } else {
        // Includes completed files: a full inbox must be emptied locally before accepting more.
        if root.dir.entries()?.take(1025).count() >= 1024 {
            return Err(TyuErrorCode::Limit.into());
        }
        let mut file = root
            .dir
            .open_with(&sidecar, OpenOptions::new().write(true).create_new(true))?;
        std::io::Write::write_all(&mut file, &expected)?;
        file.sync_all()?;
    }
    let file = root.dir.open_with(
        &part,
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(!resumed),
    )?;
    let mut file = tokio::fs::File::from_std(file.into_std());
    let mut offset = file.metadata().await?.len();
    if offset > meta.size {
        return Err(TyuErrorCode::Integrity.into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; tyu_protocol::MAX_CHUNK];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    transport::write(tx, &offer.reply(Message::FileAccept { offset })).await?;
    events
        .send(TyuEvent::TransferOffered {
            peer,
            metadata: meta.clone(),
        })
        .await
        .map_err(|_| TyuErrorCode::Cancelled)?;
    while offset < meta.size {
        let remaining = (meta.size - offset).min(buffer.len() as u64) as usize;
        let n = rx
            .read(&mut buffer[..remaining])
            .await
            .map_err(transport::neterr)?
            .ok_or(TyuErrorCode::Integrity)?;
        file.write_all(&buffer[..n]).await?;
        hasher.update(&buffer[..n]);
        offset += n as u64;
        events
            .send(TyuEvent::TransferProgress {
                peer,
                id: meta.id,
                bytes: offset,
                total: meta.size,
            })
            .await
            .map_err(|_| TyuErrorCode::Cancelled)?;
    }
    if transport::timed(async { rx.read(&mut [0; 1]).await.map_err(transport::neterr) })
        .await?
        .is_some()
    {
        return Err(TyuErrorCode::Limit.into());
    }
    file.sync_all().await?;
    drop(file);
    if hasher.finalize().as_bytes() != &meta.blake3 {
        root.dir.remove_file(&part)?;
        root.dir.remove_file(&sidecar)?;
        return Err(TyuErrorCode::Integrity.into());
    }
    // Hard-link commits atomically without replacing a user's existing destination.
    root.dir.hard_link(&part, &root.dir, &meta.name)?;
    root.dir.remove_file(&part)?;
    root.dir.remove_file(&sidecar)?;
    events
        .send(TyuEvent::TransferComplete { peer, id: meta.id })
        .await
        .map_err(|_| CoreError::Code(TyuErrorCode::Cancelled))?;
    Ok(())
}

struct ReceiveLease {
    root: Arc<ReceiveRoot>,
    key: (DeviceId, TransferId),
}
impl ReceiveLease {
    fn acquire(root: Arc<ReceiveRoot>, peer: DeviceId, id: TransferId) -> Result<Self> {
        {
            let mut active = root.active.lock().map_err(|_| CoreError::Store)?;
            if active.len() >= 16 || !active.insert((peer, id)) {
                return Err(TyuErrorCode::Limit.into());
            }
        }
        Ok(Self {
            root,
            key: (peer, id),
        })
    }
}
impl Drop for ReceiveLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.root.active.lock() {
            active.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::DeviceIdentity;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn received_hash_mismatch_is_never_published() {
        let a = DeviceIdentity::generate("A").unwrap();
        let b = DeviceIdentity::generate("B").unwrap();
        let endpoint_a = transport::endpoint(&a, "127.0.0.1:0".parse().unwrap()).unwrap();
        let endpoint_b = transport::endpoint(&b, "127.0.0.1:0".parse().unwrap()).unwrap();
        let (client, server) = tokio::join!(
            transport::connect(
                &endpoint_a,
                &a,
                endpoint_b.local_addr().unwrap(),
                b.fingerprint()
            ),
            async { endpoint_b.accept().await.unwrap().await.unwrap() }
        );
        let client = client.unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root = Arc::new(ReceiveRoot::new(destination.path(), 1024).unwrap());
        let (events, _rx) = mpsc::channel(16);
        let metadata = TransferMetadata {
            id: TransferId::new(),
            name: "bad.bin".into(),
            size: 3,
            blake3: [0; 32],
        };
        let (sent, received) = tokio::join!(
            async {
                let (mut tx, mut rx) = client.open_bi().await.unwrap();
                let offer = Frame::new(Message::FileOffer(metadata));
                transport::write(&mut tx, &offer).await.unwrap();
                assert!(matches!(
                    transport::read(&mut rx).await.unwrap().message,
                    Message::FileAccept { offset: 0 }
                ));
                tx.write_all(b"bad").await.unwrap();
                tx.finish().unwrap();
                transport::read(&mut rx).await.unwrap()
            },
            async {
                let (tx, mut rx) = server.accept_bi().await.unwrap();
                let offer = transport::read(&mut rx).await.unwrap();
                receive(a.id(), offer, tx, rx, root, events).await
            }
        );
        assert!(matches!(
            sent.message,
            Message::Error {
                code: TyuErrorCode::Integrity
            }
        ));
        assert!(received.is_err());
        assert!(!destination.path().join("bad.bin").exists());
        endpoint_a.close(0u32.into(), b"test complete");
        endpoint_b.close(0u32.into(), b"test complete");
    }
}
