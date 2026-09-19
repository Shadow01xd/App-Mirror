//! Loopback WebDAV facade. Only the authenticated TyuLink connection reaches the phone.
//! Windows WebClient consumes this facade; no phone HTTP server or Internet service is used.
use std::{
    collections::HashMap,
    io::{self, Read},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use tiny_http::{Header, Request, Response, Server, StatusCode};
use tyu_core::*;

type Reply = std::result::Result<StorageReply, TyuErrorCode>;
#[derive(Clone)]
struct DavLock {
    token: String,
    expires: std::time::Instant,
    recursive: bool,
}
type Locks = Arc<Mutex<HashMap<String, DavLock>>>;
fn lock_body(token: &str, recursive: bool) -> String {
    format!(
        "<?xml version=\"1.0\"?><D:prop xmlns:D=\"DAV:\"><D:lockdiscovery><D:activelock><D:locktype><D:write/></D:locktype><D:lockscope><D:exclusive/></D:lockscope><D:depth>{}</D:depth><D:timeout>Second-600</D:timeout><D:locktoken><D:href>{}</D:href></D:locktoken></D:activelock></D:lockdiscovery></D:prop>",
        if recursive { "infinity" } else { "0" },
        xml(token)
    )
}
#[derive(Clone)]
pub struct Broker {
    commands: tokio::sync::mpsc::Sender<TyuCommand>,
    pending: Arc<Mutex<HashMap<RequestId, mpsc::SyncSender<Reply>>>>,
}
impl Broker {
    pub fn new(commands: tokio::sync::mpsc::Sender<TyuCommand>) -> Self {
        Self {
            commands,
            pending: Arc::default(),
        }
    }
    pub fn deliver(&self, event: &TyuEvent) -> bool {
        if let TyuEvent::StorageResult {
            request, result, ..
        } = event
            && let Some(sender) = self.pending.lock().unwrap().remove(request)
        {
            let _ = sender.send(result.clone());
            return true;
        }
        false
    }
    fn call(&self, peer: DeviceId, operation: StorageOp) -> io::Result<StorageReply> {
        let request = RequestId::new();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending.lock().unwrap().insert(request, tx);
        let result = self
            .commands
            .try_send(TyuCommand::StorageRequest {
                peer,
                request,
                operation,
            })
            .map_err(|_| io::Error::other("TYU ocupado"))
            .and_then(|_| {
                rx.recv_timeout(Duration::from_secs(20))
                    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "El teléfono no responde"))
            })
            .and_then(|r| {
                r.map_err(|e| {
                    io::Error::new(
                        match e {
                            TyuErrorCode::PathDenied => io::ErrorKind::PermissionDenied,
                            TyuErrorCode::NotFound => io::ErrorKind::NotFound,
                            TyuErrorCode::AlreadyExists => io::ErrorKind::AlreadyExists,
                            _ => io::ErrorKind::Other,
                        },
                        e.to_string(),
                    )
                })
            });
        self.pending.lock().unwrap().remove(&request);
        result
    }
}
pub struct StorageBridge {
    pub broker: Broker,
    peer: Arc<Mutex<Option<DeviceId>>>,
    stopped: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
    unc: String,
    namespace: String,
    mounted: AtomicBool,
    locks: Locks,
}
impl StorageBridge {
    pub fn start(commands: tokio::sync::mpsc::Sender<TyuCommand>) -> io::Result<Self> {
        let server = Arc::new(Server::http("127.0.0.1:0").map_err(io::Error::other)?);
        let port = server.server_addr().to_ip().unwrap().port();
        let secret = uuid::Uuid::new_v4().simple().to_string();
        let prefix = format!("/{secret}");
        let broker = Broker::new(commands);
        let peer = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let locks: Locks = Arc::default();
        let mut workers = Vec::new();
        for _ in 0..4 {
            let (server, broker, peer, stopped, prefix) = (
                server.clone(),
                broker.clone(),
                peer.clone(),
                stopped.clone(),
                prefix.clone(),
            );
            let locks = locks.clone();
            workers.push(thread::spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(200)) {
                        let selected = *peer.lock().unwrap();
                        serve(request, selected, &broker, &prefix, &locks);
                    }
                }
            }));
        }
        Ok(Self {
            broker,
            peer,
            stopped,
            workers,
            unc: format!(r"\\127.0.0.1@{port}\DavWWWRoot\{secret}"),
            namespace: "{F60330CB-9D30-476C-B550-5368C6F78203}".into(),
            mounted: AtomicBool::new(false),
            locks,
        })
    }
    pub fn mount(&self, peer: DeviceId, name: &str) -> io::Result<()> {
        *self.peer.lock().unwrap() = Some(peer);
        namespace(&self.namespace, Some((&format!("TYU: {name}"), &self.unc)))?;
        self.mounted.store(true, Ordering::Release);
        Ok(())
    }
    pub fn unmount(&self) {
        *self.peer.lock().unwrap() = None;
        self.locks.lock().unwrap().clear();
        if self.mounted.swap(false, Ordering::AcqRel) {
            let _ = namespace(&self.namespace, None);
        }
    }
    /// Whether the phone's folder is published in this PC's Explorer.
    pub fn is_mounted(&self) -> bool {
        self.mounted.load(Ordering::Acquire)
    }
    pub fn open(&self) -> io::Result<()> {
        // Explorer happily opens on a path that leads nowhere, so spawning it is never proof
        // that anything worked; refuse before that rather than leaving an empty window.
        if !self.is_mounted() {
            return Err(io::Error::other(
                "Activa Almacenamiento en Bypass para poder abrir el teléfono",
            ));
        }
        std::process::Command::new("explorer.exe")
            .arg(&self.unc)
            .spawn()
            .map(|_| ())
    }
}
impl Drop for StorageBridge {
    fn drop(&mut self) {
        self.unmount();
        self.stopped.store(true, Ordering::Release);
        self.broker.pending.lock().unwrap().clear();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
fn namespace(id: &str, target: Option<(&str, &str)>) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let script = r#"
$ErrorActionPreference='Stop'
$id=$env:TYU_NAMESPACE
$key='HKCU:\Software\Classes\CLSID\'+$id
$entry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\'+$id
if ($env:TYU_TARGET) {
 New-Item $key -Force | Out-Null
 Set-Item $key $env:TYU_LABEL
 New-ItemProperty $key 'System.IsPinnedToNameSpaceTree' -Value 1 -PropertyType DWord -Force | Out-Null
 New-Item ($key+'\InProcServer32') -Force -Value 'shell32.dll' | Out-Null
 New-Item ($key+'\DefaultIcon') -Force -Value 'imageres.dll,-2' | Out-Null
 New-Item ($key+'\Instance') -Force | Out-Null
 New-ItemProperty ($key+'\Instance') 'CLSID' -Value '{0E5AAE11-A475-4c5b-AB00-C66DE400274E}' -Force | Out-Null
 New-Item ($key+'\Instance\InitPropertyBag') -Force | Out-Null
 New-ItemProperty ($key+'\Instance\InitPropertyBag') 'TargetFolderPath' -Value $env:TYU_TARGET -Force | Out-Null
 New-ItemProperty ($key+'\Instance\InitPropertyBag') 'Attributes' -Value 17 -PropertyType DWord -Force | Out-Null
 New-Item ($key+'\ShellFolder') -Force | Out-Null
 New-ItemProperty ($key+'\ShellFolder') 'Attributes' -Value 0xf080004d -PropertyType DWord -Force | Out-Null
 New-ItemProperty ($key+'\ShellFolder') 'FolderValueFlags' -Value 40 -PropertyType DWord -Force | Out-Null
 New-Item $entry -Force -Value $env:TYU_LABEL | Out-Null
} else {
 if (Test-Path -LiteralPath $entry) { Remove-Item -LiteralPath $entry -Recurse -Force }
 if (Test-Path -LiteralPath $key) { Remove-Item -LiteralPath $key -Recurse -Force }
}
"#;
    let result = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("TYU_NAMESPACE", id)
        .env("TYU_LABEL", target.map(|t| t.0).unwrap_or(""))
        .env("TYU_TARGET", target.map(|t| t.1).unwrap_or(""))
        .creation_flags(0x08000000)
        .output()?;
    if result.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).to_string(),
        ))
    }
}
fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).unwrap()
}
fn finish(request: Request, status: u16, body: String) {
    if std::env::var_os("TYU_DAV_TRACE").is_some() {
        eprintln!("DAV {} -> {status}", request.method());
    }
    let _ = request.respond(
        Response::from_string(body)
            .with_status_code(StatusCode(status))
            .with_header(header("Content-Type", "application/xml; charset=utf-8"))
            .with_header(header("DAV", "1, 2"))
            .with_header(header(
                "Allow",
                "OPTIONS, PROPFIND, GET, HEAD, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK",
            )),
    );
}
fn path(url: &str, prefix: &str) -> io::Result<String> {
    let relative = url
        .strip_prefix(prefix)
        .filter(|p| p.is_empty() || p.starts_with('/'))
        .ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))?;
    let decoded = percent_encoding::percent_decode_str(relative.trim_matches('/'))
        .decode_utf8()
        .map_err(io::Error::other)?;
    tyu_core::storage::relative(&decoded, true).map_err(io::Error::other)?;
    Ok(decoded.to_string())
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn href(prefix: &str, path: &str) -> String {
    format!(
        "{prefix}/{}",
        path.split('/')
            .map(
                |s| percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
                    .to_string()
            )
            .collect::<Vec<_>>()
            .join("/")
    )
}
fn entries(reply: StorageReply) -> io::Result<Vec<StorageMetadata>> {
    match reply {
        StorageReply::Entries(e) => Ok(e),
        _ => Err(io::Error::other("Invalid storage response")),
    }
}
fn serve(mut req: Request, peer: Option<DeviceId>, broker: &Broker, prefix: &str, locks: &Locks) {
    // Host and bearer path prevent a web page from using this loopback bridge through DNS rebinding.
    let local_host = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .is_some_and(|h| h.value.as_str().starts_with("127.0.0.1:"));
    if !local_host {
        finish(req, 403, String::new());
        return;
    }
    if req.method().as_str() == "OPTIONS" {
        finish(req, 200, String::new());
        return;
    }
    let (Some(peer), Ok(p)) = (peer, path(req.url(), prefix)) else {
        finish(req, 403, String::new());
        return;
    };
    let method = req.method().as_str().to_string();
    let condition = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("If"))
        .map(|h| h.value.as_str())
        .unwrap_or("")
        .to_owned();
    if method == "LOCK" || method == "UNLOCK" {
        let mut table = locks.lock().unwrap();
        table.retain(|_, l| l.expires > std::time::Instant::now());
        if method == "UNLOCK" {
            let supplied = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Lock-Token"))
                .map(|h| h.value.as_str().trim_matches(['<', '>']));
            if table
                .get(&p)
                .is_some_and(|l| Some(l.token.as_str()) == supplied)
            {
                table.remove(&p);
                finish(req, 204, String::new());
            } else {
                finish(req, 409, String::new());
            }
            return;
        }
        if let Some(lock) = table.get_mut(&p) {
            if condition.contains(&lock.token) {
                lock.expires = std::time::Instant::now() + Duration::from_secs(600);
                let body = lock_body(&lock.token, lock.recursive);
                finish(req, 200, body);
            } else {
                finish(req, 423, String::new());
            }
            return;
        }
        let recursive = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Depth"))
            .is_none_or(|h| h.value.as_str() != "0");
        if table.iter().any(|(path, l)| {
            (l.recursive && p.starts_with(&format!("{path}/")))
                || (recursive && path.starts_with(&format!("{p}/")))
        }) {
            finish(req, 423, String::new());
            return;
        }
        // The local bridge implements exclusive write locks. Shared locks are deliberately unsupported.
        let mut body = String::new();
        if req
            .as_reader()
            .take(8193)
            .read_to_string(&mut body)
            .is_err()
            || body.len() > 8192
            || !body.contains("exclusive")
        {
            finish(req, 400, String::new());
            return;
        }
        if table.len() >= 256 {
            finish(req, 503, String::new());
            return;
        }
        let status = match broker.call(peer, StorageOp::Stat { path: p.clone() }) {
            Ok(_) => 200,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if broker
                    .call(
                        peer,
                        StorageOp::Write {
                            path: p.clone(),
                            offset: 0,
                            data: vec![],
                        },
                    )
                    .is_err()
                {
                    finish(req, 409, String::new());
                    return;
                }
                201
            }
            Err(_) => {
                finish(req, 403, String::new());
                return;
            }
        };
        let token = format!("opaquelocktoken:{}", uuid::Uuid::new_v4());
        table.insert(
            p,
            DavLock {
                token: token.clone(),
                recursive,
                expires: std::time::Instant::now() + Duration::from_secs(600),
            },
        );
        let _ = req.respond(
            Response::from_string(lock_body(&token, recursive))
                .with_status_code(StatusCode(status))
                .with_header(header("Content-Type", "application/xml; charset=utf-8"))
                .with_header(header("Lock-Token", &format!("<{token}>"))),
        );
        return;
    }
    if matches!(method.as_str(), "PUT" | "DELETE" | "MOVE" | "MKCOL") {
        let mut table = locks.lock().unwrap();
        table.retain(|_, l| l.expires > std::time::Instant::now());
        let destination = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Destination"))
            .and_then(|h| url::Url::parse(h.value.as_str()).ok())
            .and_then(|u| path(u.path(), prefix).ok());
        let denied = table.iter().any(|(path, l)| {
            let affected = |target: &str| {
                target == path
                    || (l.recursive && target.starts_with(&format!("{path}/")))
                    || (matches!(method.as_str(), "DELETE" | "MOVE")
                        && path.starts_with(&format!("{target}/")))
            };
            (affected(&p) || destination.as_deref().is_some_and(affected))
                && !condition.contains(&l.token)
        });
        if denied {
            finish(req, 423, String::new());
            return;
        }
    }
    if method == "GET" || method == "HEAD" {
        let stat = broker
            .call(peer, StorageOp::Stat { path: p.clone() })
            .and_then(entries)
            .and_then(|e| {
                e.into_iter()
                    .next()
                    .ok_or_else(|| io::Error::other("Missing file"))
            });
        match stat {
            Ok(meta) if !meta.is_dir => {
                let length = usize::try_from(meta.size).unwrap_or(usize::MAX);
                let reader = RemoteReader {
                    broker: broker.clone(),
                    peer,
                    path: p,
                    offset: 0,
                    remaining: if method == "HEAD" { 0 } else { meta.size },
                };
                let _ = req.respond(Response::new(
                    StatusCode(200),
                    vec![
                        header("Content-Type", "application/octet-stream"),
                        header("Cache-Control", "no-store"),
                    ],
                    reader,
                    Some(length),
                    None,
                ));
            }
            _ => finish(req, 404, String::new()),
        }
        return;
    }
    let result: io::Result<(u16, String)> = (|| {
        match method.as_str() {
            "PROPFIND" => {
                let depth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Depth"))
                    .map(|h| h.value.as_str())
                    .unwrap_or("1");
                if depth != "0" && depth != "1" {
                    return Ok((403, String::new()));
                }
                let mut items = entries(broker.call(peer, StorageOp::Stat { path: p.clone() })?)?;
                if depth == "1" && items.first().is_some_and(|m| m.is_dir) {
                    let mut offset = 0;
                    loop {
                        let page = entries(broker.call(
                            peer,
                            StorageOp::ListPage {
                                path: p.clone(),
                                offset,
                            },
                        )?)?;
                        let done = page.len() < 128;
                        items.extend(page);
                        if done {
                            break;
                        }
                        offset += 128;
                        if offset > 131072 {
                            return Err(io::Error::other(
                                "La carpeta supera el límite de elementos",
                            ));
                        }
                    }
                }
                let mut body = String::from(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">",
                );
                for m in items {
                    let p = if m.path == "." { "" } else { &m.path };
                    let h = format!(
                        "{}{}",
                        href(prefix, p),
                        if m.is_dir && !p.is_empty() { "/" } else { "" }
                    );
                    body.push_str(&format!("<D:response><D:href>{}</D:href><D:propstat><D:prop><D:displayname>{}</D:displayname><D:resourcetype>{}</D:resourcetype><D:getcontentlength>{}</D:getcontentlength><D:getcontenttype>application/octet-stream</D:getcontenttype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",xml(&h),xml(p.rsplit('/').next().unwrap_or("")),if m.is_dir {"<D:collection/>"} else {""},m.size));
                }
                body.push_str("</D:multistatus>");
                Ok((207, body))
            }
            "MKCOL" => {
                broker.call(peer, StorageOp::CreateDir { path: p })?;
                Ok((201, String::new()))
            }
            "DELETE" => {
                broker.call(peer, StorageOp::Delete { path: p })?;
                Ok((204, String::new()))
            }
            "MOVE" => {
                let dest = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Destination"))
                    .ok_or_else(|| io::Error::other("Missing destination"))?;
                let dest = url::Url::parse(dest.value.as_str()).map_err(io::Error::other)?;
                if dest.host_str() != Some("127.0.0.1") {
                    return Ok((403, String::new()));
                }
                broker.call(
                    peer,
                    StorageOp::Rename {
                        from: p,
                        to: path(dest.path(), prefix)?,
                    },
                )?;
                Ok((201, String::new()))
            }
            "PUT" => {
                // Upload to a sibling first: cancelled uploads never replace a complete phone file.
                let parent = p
                    .rsplit_once('/')
                    .map(|v| format!("{}/", v.0))
                    .unwrap_or_default();
                let staging = format!("{parent}.tyu-{}.part", uuid::Uuid::new_v4());
                let upload: io::Result<()> = (|| {
                    let mut offset = 0;
                    let mut buffer = vec![0; 65536];
                    broker.call(
                        peer,
                        StorageOp::Write {
                            path: staging.clone(),
                            offset: 0,
                            data: vec![],
                        },
                    )?;
                    loop {
                        let n = req.as_reader().read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        broker.call(
                            peer,
                            StorageOp::Write {
                                path: staging.clone(),
                                offset,
                                data: buffer[..n].to_vec(),
                            },
                        )?;
                        offset += n as u64;
                    }
                    broker.call(
                        peer,
                        StorageOp::Rename {
                            from: staging.clone(),
                            to: p,
                        },
                    )?;
                    Ok(())
                })();
                if upload.is_err() {
                    let _ = broker.call(peer, StorageOp::Delete { path: staging });
                }
                upload?;
                Ok((201, String::new()))
            }
            _ => Ok((405, String::new())),
        }
    })();
    match result {
        Ok((code, body)) => finish(req, code, body),
        Err(e) => finish(
            req,
            match e.kind() {
                io::ErrorKind::PermissionDenied => 403,
                io::ErrorKind::NotFound => 404,
                io::ErrorKind::AlreadyExists => 405,
                _ => 502,
            },
            xml(&e.to_string()),
        ),
    }
}
struct RemoteReader {
    broker: Broker,
    peer: DeviceId,
    path: String,
    offset: u64,
    remaining: u64,
}
impl Read for RemoteReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let size = self.remaining.min(buffer.len() as u64).min(65536) as u32;
        if size == 0 {
            return Ok(0);
        }
        let StorageReply::Data(data) = self.broker.call(
            self.peer,
            StorageOp::Read {
                path: self.path.clone(),
                offset: self.offset,
                length: size,
            },
        )?
        else {
            return Err(io::Error::other("Invalid file reply"));
        };
        if data.is_empty() || data.len() > size as usize {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "El archivo cambió durante la lectura",
            ));
        }
        buffer[..data.len()].copy_from_slice(&data);
        self.offset += data.len() as u64;
        self.remaining -= data.len() as u64;
        Ok(data.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    async fn next(node: &mut TyuNode, predicate: impl Fn(&TyuEvent) -> bool) -> TyuEvent {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let e = node.events.recv().await.unwrap();
                if predicate(&e) {
                    break e;
                }
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn webdav_over_authenticated_quic_reads_writes_renames_and_revokes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("DCIM")).unwrap();
        std::fs::write(root.path().join("DCIM/foto.txt"), b"phone original").unwrap();
        let shared = Arc::new(storage::SharedStorageProvider::default());
        shared.set_root(Some(root.path()), 8 * 1024 * 1024).unwrap();
        let mut pc_config = NodeConfig::new(
            "PC",
            DevicePlatform::Windows,
            Arc::new(identity::MemoryStore::default()),
        );
        pc_config.bind = "127.0.0.1:0".parse().unwrap();
        pc_config.capabilities = vec![Capability::StorageConsumer];
        let mut phone_config = NodeConfig::new(
            "Phone",
            DevicePlatform::Android,
            Arc::new(identity::MemoryStore::default()),
        );
        phone_config.bind = "127.0.0.1:0".parse().unwrap();
        phone_config.capabilities = vec![Capability::StorageProvider];
        phone_config.storage = Some(shared.clone());
        let mut pc = TyuNode::start(pc_config).await.unwrap();
        let mut phone = TyuNode::start(phone_config).await.unwrap();
        pc.command(TyuCommand::BeginPairing {
            endpoint: pc.address,
        })
        .await
        .unwrap();
        let TyuEvent::PairingStarted { ticket } =
            next(&mut pc, |e| matches!(e, TyuEvent::PairingStarted { .. })).await
        else {
            panic!()
        };
        phone.command(TyuCommand::Pair { ticket }).await.unwrap();
        let TyuEvent::PairingRequest { request, .. } =
            next(&mut pc, |e| matches!(e, TyuEvent::PairingRequest { .. })).await
        else {
            panic!()
        };
        pc.command(TyuCommand::AcceptPairing { request })
            .await
            .unwrap();
        next(&mut pc, |e| matches!(e, TyuEvent::Connected { .. })).await;
        next(&mut phone, |e| matches!(e, TyuEvent::Connected { .. })).await;
        let bridge = StorageBridge::start(pc.commands.clone()).unwrap();
        *bridge.peer.lock().unwrap() = Some(phone.device.id);
        let broker = bridge.broker.clone();
        let worker = tokio::spawn(async move {
            while let Some(e) = pc.events.recv().await {
                broker.deliver(&e);
            }
        });
        let unc = bridge.unc.clone();
        let shared2 = shared.clone();
        tokio::task::spawn_blocking(move || {
            let port=unc.split('@').nth(1).unwrap().split('\\').next().unwrap();
            let token=unc.rsplit('\\').next().unwrap();
            let address=format!("127.0.0.1:{port}"); let base=format!("/{token}");
            let http=|method:&str,path:&str,headers:&str,body:&str| {
                let mut tcp=std::net::TcpStream::connect(&address).unwrap(); tcp.set_read_timeout(Some(Duration::from_secs(25))).unwrap();
                write!(tcp,"{method} {base}/{path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",body.len()).unwrap();
                let mut reply=String::new();tcp.read_to_string(&mut reply).unwrap();reply
            };
            let listing=http("PROPFIND","","Depth: 1\r\n",""); assert!(listing.starts_with("HTTP/1.1 207"),"{listing}");assert!(listing.contains("DCIM"));
            let read=http("GET","DCIM/foto.txt","","");assert!(read.ends_with("phone original"),"{read}");
            assert!(http("PROPFIND","DCIM/missing.txt","Depth: 0\r\n","").starts_with("HTTP/1.1 404"));
            assert!(http("PUT","DCIM/nuevo.txt","","larger content").starts_with("HTTP/1.1 201"));
            let locked=http("LOCK","DCIM/nuevo.txt","Depth: 0\r\n","<D:lockinfo xmlns:D=\"DAV:\"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>");
            assert!(locked.starts_with("HTTP/1.1 200"),"{locked}");
            let lock=locked.lines().find_map(|line|line.strip_prefix("Lock-Token: ")).unwrap().trim();
            assert!(http("PUT","DCIM/nuevo.txt","","blocked").starts_with("HTTP/1.1 423"));
            assert!(http("PUT","DCIM/nuevo.txt",&format!("If: ({lock})\r\n"),"allowed").starts_with("HTTP/1.1 201"));
            assert!(http("UNLOCK","DCIM/nuevo.txt",&format!("Lock-Token: {lock}\r\n"),"").starts_with("HTTP/1.1 204"));
            assert!(http("PUT","DCIM/nuevo.txt","","short").starts_with("HTTP/1.1 201"));
            assert!(http("GET","DCIM/nuevo.txt","","").ends_with("short"));
            let moved=http("MOVE","DCIM/nuevo.txt",&format!("Destination: http://{address}{base}/DCIM/final.txt\r\n"),"");assert!(moved.starts_with("HTTP/1.1 201"),"{moved}");
            assert!(http("DELETE","DCIM/final.txt","","").starts_with("HTTP/1.1 204"));
            assert!(!http("PROPFIND","%2e%2e","Depth: 1\r\n","").starts_with("HTTP/1.1 207"));
            shared2.set_root(None,0).unwrap();
            assert!(http("PROPFIND","","Depth: 1\r\n","").starts_with("HTTP/1.1 403"));
        }).await.unwrap();
        worker.abort();
        phone.shutdown().await.unwrap();
        drop(bridge);
    }
    #[test]
    fn rejects_other_tokens_and_escaped_traversal() {
        assert!(path("/secret-other/DCIM", "/secret").is_err());
        for p in ["/secret/%2e%2e/x", "/secret/DCIM/%2fetc", "/secret/a%5cb"] {
            assert!(path(p, "/secret").is_err(), "{p}");
        }
        assert_eq!(
            path("/secret/DCIM/foto%20uno.jpg", "/secret").unwrap(),
            "DCIM/foto uno.jpg"
        );
        assert_eq!(xml("a&<b>"), "a&amp;&lt;b&gt;");
    }
}
