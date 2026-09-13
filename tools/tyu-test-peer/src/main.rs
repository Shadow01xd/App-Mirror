use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tyu_core::{identity::*, storage::LocalFilesystemProvider, *};

fn android_capabilities() -> Vec<Capability> {
    use Capability::*;
    vec![
        MonitorReceiver,
        MirrorSource,
        CameraSource,
        MicrophoneSource,
        StorageProvider,
        StorageConsumer,
        FileSend,
        FileReceive,
        ClipboardSource,
        ClipboardReceiver,
        TouchSource,
        InputReceiver,
    ]
}
fn value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
fn peer(text: &str) -> Result<DeviceId> {
    text.parse()
        .map(DeviceId)
        .map_err(|_| TyuErrorCode::Malformed.into())
}
fn command(line: &str, connected: Option<DeviceId>) -> Result<TyuCommand> {
    let (verb, tail) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    let selected = || connected.ok_or(CoreError::Code(TyuErrorCode::NotAvailable));
    Ok(match verb {
        "discover" | "advertise" => TyuCommand::StartDiscovery,
        "pair" => TyuCommand::Pair {
            ticket: PairingTicket::parse(tail)?,
        },
        "connect" => TyuCommand::Connect { peer: peer(tail)? },
        "disconnect" => TyuCommand::Disconnect { peer: selected()? },
        "forget" => TyuCommand::ForgetPeer { peer: peer(tail)? },
        "approve" => TyuCommand::AcceptPairing {
            request: RequestId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "reject" => TyuCommand::RejectPairing {
            request: RequestId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "send-file" => TyuCommand::SendFile {
            peer: selected()?,
            id: TransferId::new(),
            path: PathBuf::from(tail),
        },
        "pause" => TyuCommand::PauseTransfer {
            id: TransferId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "resume" => TyuCommand::ResumeTransfer {
            id: TransferId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "cancel" => TyuCommand::CancelTransfer {
            id: TransferId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "list" => TyuCommand::ListStorage {
            peer: selected()?,
            path: tail.into(),
        },
        "clipboard" => TyuCommand::SendClipboard {
            peer: selected()?,
            text: tail.into(),
        },
        "ping" => TyuCommand::Ping {
            peer: selected()?,
            value: 42,
        },
        "monitor" => TyuCommand::StartMode {
            peer: selected()?,
            mode: TyuMode::Monitor,
        },
        "mirror" => TyuCommand::StartMode {
            peer: selected()?,
            mode: TyuMode::Mirror,
        },
        "bypass" => TyuCommand::StartMode {
            peer: selected()?,
            mode: TyuMode::Bypass,
        },
        "camera" => TyuCommand::StartService {
            peer: selected()?,
            service: ServiceKind::Camera,
        },
        "microphone" => TyuCommand::StartService {
            peer: selected()?,
            service: ServiceKind::Microphone,
        },
        "stop-mode" => TyuCommand::StopMode {
            peer: selected()?,
            session: SessionId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "stop-service" => TyuCommand::StopService {
            peer: selected()?,
            session: SessionId(tail.parse().map_err(|_| TyuErrorCode::Malformed)?),
        },
        "quit" => TyuCommand::Shutdown,
        _ => return Err(TyuErrorCode::Unsupported.into()),
    })
}
fn help() {
    println!(
        "TYU test peer — Core real, capacidades media de prueba\n\
Uso: tyu-test-peer <advertise|discover|pair URI|connect DEVICE|capabilities|receive|storage-root PATH|send-file PATH|ping>\n\
Opciones: --state DIR --receive DIR --storage DIR --bind IP:PORT --endpoint IP:PORT --approve\n\
En terminal: approve REQUEST, reject REQUEST, connect DEVICE, pair URI, send-file PATH, pause ID, resume ID, cancel ID, list PATH, clipboard TEXT, ping, monitor, mirror, bypass, camera, microphone, stop-mode SESSION, stop-service SESSION, disconnect, quit.\n\
--approve autoriza explícitamente pairing automático para pruebas locales. Captura y drivers no están implementados."
    );
}
struct Terminal(bool);
impl Drop for Terminal {
    fn drop(&mut self) {
        if self.0 {
            let _result = crossterm::terminal::disable_raw_mode();
        }
    }
}
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tyu_core=info".into()),
        )
        .try_init()
        .ok();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        help();
        return Ok(());
    }
    if args[0] == "capabilities" {
        println!("{:?}", android_capabilities());
        return Ok(());
    }
    let root = value(&args, "--state")
        .map(PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("org", "TYU", "TYU Test Peer")
                .map(|p| p.data_local_dir().to_owned())
        })
        .ok_or("user data directory unavailable")?;
    let store = Arc::new(WindowsPersistentStore::open(&root)?);
    let mut config = NodeConfig::new("TYU Test Peer", DevicePlatform::Android, store.clone());
    config.capabilities = android_capabilities();
    config.bind = value(&args, "--bind")
        .unwrap_or_else(|| "0.0.0.0:0".into())
        .parse()?;
    config.receive_directory = Some(
        value(&args, "--receive")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("received")),
    );
    let storage = value(&args, "--storage").or_else(|| {
        (args[0] == "storage-root")
            .then(|| args.get(1).cloned())
            .flatten()
    });
    if let Some(path) = storage {
        config.storage = Some(Arc::new(LocalFilesystemProvider::new(
            std::path::Path::new(&path),
            8 * 1024 * 1024 * 1024,
        )?));
    } else {
        config
            .capabilities
            .retain(|c| *c != Capability::StorageProvider);
    }
    let mut node = TyuNode::start(config).await?;
    println!(
        "Device: {} | QUIC: {} | State: {}",
        node.device.id,
        node.address,
        root.display()
    );
    node.command(TyuCommand::StartDiscovery).await?;
    let endpoint = value(&args, "--endpoint")
        .unwrap_or_else(|| format!("127.0.0.1:{}", node.address.port()))
        .parse()?;
    node.command(TyuCommand::BeginPairing { endpoint }).await?;
    let approve = args.iter().any(|a| a == "--approve");
    let mut connected = None;
    let mut deferred = None;
    match args[0].as_str() {
        "pair" | "connect" => {
            node.command(command(
                &format!("{} {}", args[0], args.get(1).ok_or("missing argument")?),
                None,
            )?)
            .await?
        }
        "send-file" | "ping" => {
            let trusted = store
                .peers()?
                .into_iter()
                .find(|p| !p.revoked)
                .ok_or("pair first, or use an interactive terminal")?;
            deferred = Some(if args[0] == "send-file" {
                format!("send-file {}", args.get(1).ok_or("missing path")?)
            } else {
                "ping".into()
            });
            node.command(TyuCommand::Connect {
                peer: trusted.device.id,
            })
            .await?;
        }
        "advertise" | "discover" | "receive" | "storage-root" => {}
        _ => {
            help();
            node.shutdown().await?;
            return Ok(());
        }
    }
    let terminal = Terminal(std::io::stdin().is_terminal());
    if terminal.0 {
        crossterm::terminal::enable_raw_mode()?;
    }
    let mut line = String::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    loop {
        tokio::select! {
            signal=tokio::signal::ctrl_c()=>{signal?;break;},
            ev=node.events.recv()=>{let Some(ev)=ev else {break;};match &ev {
                TyuEvent::PairingStarted {ticket}=>println!("\r\nPair URI (expires {}): {}\r",ticket.expires,ticket.uri()?),
                TyuEvent::PairingRequest {request,device,fingerprint}=>{println!("\r\nPair request {request}: {} [{fingerprint:?}]\r",device.name);if approve {node.command(TyuCommand::AcceptPairing {request:*request}).await?;}},
                TyuEvent::Connected {device}=>{connected=Some(device.id);println!("\r\nConnected: {} ({})\r",device.name,device.id);if let Some(line)=deferred.take() {node.command(command(&line,connected)?).await?;}},
                TyuEvent::Disconnected {peer}=>{if connected==Some(*peer) {connected=None;}println!("\r\nDisconnected {peer}\r");},
                TyuEvent::MetricsUpdated {..}|TyuEvent::TransferProgress {..}=>{},
                TyuEvent::ClipboardReceived {peer,text}=>println!("\r\nClipboard from {peer}: {} UTF-8 bytes\r",text.len()),
                _=>println!("\r\n{ev:?}\r"),
            }},
            _=tick.tick(),if terminal.0=>{
                if crossterm::event::poll(Duration::ZERO)? && let crossterm::event::Event::Key(key)=crossterm::event::read()? {
                    use crossterm::event::{KeyCode,KeyModifiers,KeyEventKind};
                    if key.kind!=KeyEventKind::Press {continue;}
                    if key.modifiers.contains(KeyModifiers::CONTROL)&&key.code==KeyCode::Char('c') {break;}
                    match key.code {
                        KeyCode::Char(c)=>{line.push(c);print!("{c}");std::io::stdout().flush()?;},
                        KeyCode::Backspace if line.pop().is_some()=>{print!("\x08 \x08");std::io::stdout().flush()?;},
                        KeyCode::Enter=>{println!("\r");if line.trim()=="quit" {break;}match command(&line,connected) {Ok(c)=>node.command(c).await?,Err(e)=>println!("{e}\r")};line.clear();},_=>{}
                    }
                }
            }
        }
    }
    drop(terminal);
    node.shutdown().await?;
    Ok(())
}
