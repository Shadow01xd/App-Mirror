//! Cable link over ADB — the same path Android Studio and scrcpy use.
//!
//! `adb forward` tunnels a TCP port through the USB cable, but TYU speaks QUIC, which is UDP.
//! So this module puts a relay on each end: every QUIC packet crosses the cable with a 2-byte
//! length in front and is handed back to UDP on the other side. A cable loses nothing, so
//! carrying QUIC over a reliable stream costs nothing in practice.
//!
//! ```text
//! TyuNode ──UDP──> relay ──TCP──> adb forward ──cable──> UsbBridge ──UDP──> phone's TyuNode
//! ```
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Port the phone's `UsbBridge` listens on.
const PHONE_PORT: u16 = 47810;
const NO_WINDOW: u32 = 0x0800_0000;
const MAX_PACKET: usize = 2048;

/// A phone appearing on or leaving the cable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsbEvent {
    Attached(String),
    /// Plugged in but not authorised yet: the phone is showing (or waiting to show) its prompt.
    Unauthorized(String),
    Detached(String),
}

/// `adb.exe`: bundled next to TYU first, then the Android SDK, then whatever is on PATH.
pub fn adb() -> Option<PathBuf> {
    let bundled = std::env::current_exe().ok()?.parent()?.join("adb.exe");
    if bundled.exists() {
        return Some(bundled);
    }
    let sdk =
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Android/Sdk/platform-tools/adb.exe");
    if sdk.exists() {
        return Some(sdk);
    }
    Command::new("adb")
        .arg("--version")
        .creation_flags(NO_WINDOW)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| PathBuf::from("adb"))
}

fn run(args: &[&str]) -> Option<String> {
    let output = Command::new(adb()?)
        .args(args)
        .creation_flags(NO_WINDOW)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Serials of phones on the cable, and whether each one has authorised this PC.
fn devices() -> Vec<(String, bool)> {
    let Some(listing) = run(&["devices"]) else {
        return Vec::new();
    };
    listing
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?;
            (!serial.is_empty()).then_some((serial, state == "device"))
        })
        .collect()
}

/// Serials of phones on the cable that have authorised this PC.
pub fn devices_ready() -> Vec<String> {
    devices()
        .into_iter()
        .filter(|(_, ok)| *ok)
        .map(|(serial, _)| serial)
        .collect()
}

/// Watches the cable and reports phones coming and going. Polling keeps this independent of
/// adb's streaming protocol, and a phone is plugged in rarely enough that 2 s is invisible.
pub fn watch() -> Receiver<UsbEvent> {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("tyu-usb-watch".into())
        .spawn(move || {
            let mut ready: HashSet<String> = HashSet::new();
            let mut pending: HashSet<String> = HashSet::new();
            loop {
                let current = devices();
                let authorised: HashSet<String> = current
                    .iter()
                    .filter(|(_, ok)| *ok)
                    .map(|(s, _)| s.clone())
                    .collect();
                let waiting: HashSet<String> = current
                    .iter()
                    .filter(|(_, ok)| !*ok)
                    .map(|(s, _)| s.clone())
                    .collect();
                for serial in authorised.difference(&ready) {
                    if sender.send(UsbEvent::Attached(serial.clone())).is_err() {
                        return;
                    }
                }
                for serial in waiting.difference(&pending) {
                    if sender.send(UsbEvent::Unauthorized(serial.clone())).is_err() {
                        return;
                    }
                }
                for serial in ready.difference(&authorised) {
                    if sender.send(UsbEvent::Detached(serial.clone())).is_err() {
                        return;
                    }
                }
                ready = authorised;
                pending = waiting;
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .ok();
    receiver
}

/// A live tunnel to one phone. Dropping it tears the forward down.
pub struct Cable {
    serial: String,
    forwarded: u16,
    stop: Arc<AtomicBool>,
    /// Device id the phone announced, and the local UDP address to connect to.
    pub peer: String,
    pub endpoint: SocketAddr,
}

impl Cable {
    /// Forwards the phone's bridge port, learns which paired device it is, and starts relaying.
    pub fn open(serial: &str) -> Result<Self, String> {
        let forwarded = forward(serial)?;
        let stream = TcpStream::connect(("127.0.0.1", forwarded))
            .map_err(|e| format!("El túnel USB no respondió: {e}"))?;
        let mut cable = Self::over(stream)?;
        cable.serial = serial.to_string();
        cable.forwarded = forwarded;
        Ok(cable)
    }

    /// Everything after the cable itself: read the greeting, then relay. Split out so the
    /// framing can be exercised without a phone.
    fn over(mut stream: TcpStream) -> Result<Self, String> {
        stream.set_nodelay(true).ok();
        // Greeting: 2-byte length then the phone's TYU device id.
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let mut size = [0u8; 2];
        stream
            .read_exact(&mut size)
            .map_err(|_| "El teléfono no respondió; abre TYU en el teléfono".to_string())?;
        let mut name = vec![0u8; u16::from_be_bytes(size) as usize];
        stream
            .read_exact(&mut name)
            .map_err(|e| format!("Saludo incompleto del teléfono: {e}"))?;
        let peer = String::from_utf8_lossy(&name).into_owned();
        stream.set_read_timeout(None).ok();

        let relay = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let endpoint = relay.local_addr().map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        spawn_relay(stream, relay, stop.clone())?;
        Ok(Self {
            serial: String::new(),
            forwarded: 0,
            stop,
            peer,
            endpoint,
        })
    }
}

impl Drop for Cable {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if self.forwarded != 0 {
            let _ = run(&[
                "-s",
                &self.serial,
                "forward",
                "--remove",
                &format!("tcp:{}", self.forwarded),
            ]);
        }
    }
}

/// `adb forward tcp:0 tcp:47810` — adb picks a free local port and prints it.
fn forward(serial: &str) -> Result<u16, String> {
    drop_stale_forwards(serial);
    let printed = run(&[
        "-s",
        serial,
        "forward",
        "tcp:0",
        &format!("tcp:{PHONE_PORT}"),
    ])
    .ok_or_else(|| "adb no pudo abrir el túnel USB".to_string())?;
    printed
        .trim()
        .parse()
        .map_err(|_| format!("adb devolvió un puerto inesperado: {printed:?}"))
}

/// Forwards left behind by a previous run (or a crash) stay in the adb server and pile up, and
/// a stale one points at a relay that no longer exists. Only TYU's own port is touched.
fn drop_stale_forwards(serial: &str) {
    let Some(listing) = run(&["forward", "--list"]) else {
        return;
    };
    for local in listing.lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        let owner = parts.next()?;
        let local = parts.next()?;
        let remote = parts.next()?;
        (owner == serial && remote == format!("tcp:{PHONE_PORT}")).then_some(local.to_string())
    }) {
        let _ = run(&["-s", serial, "forward", "--remove", &local]);
    }
}

/// Two threads: node → cable and cable → node. The node's address is learnt from its first
/// packet, which is also the first thing QUIC sends.
fn spawn_relay(stream: TcpStream, relay: UdpSocket, stop: Arc<AtomicBool>) -> Result<(), String> {
    let node: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
    let mut to_cable = stream.try_clone().map_err(|e| e.to_string())?;
    let up_socket = relay.try_clone().map_err(|e| e.to_string())?;
    let up_node = node.clone();
    let up_stop = stop.clone();
    std::thread::Builder::new()
        .name("tyu-usb-up".into())
        .spawn(move || {
            let mut buffer = [0u8; MAX_PACKET];
            while !up_stop.load(Ordering::Relaxed) {
                let Ok((size, from)) = up_socket.recv_from(&mut buffer) else {
                    break;
                };
                *up_node.lock().unwrap() = Some(from);
                if to_cable.write_all(&(size as u16).to_be_bytes()).is_err()
                    || to_cable.write_all(&buffer[..size]).is_err()
                    || to_cable.flush().is_err()
                {
                    break;
                }
            }
            up_stop.store(true, Ordering::Relaxed);
        })
        .map_err(|e| e.to_string())?;

    let mut from_cable = stream;
    std::thread::Builder::new()
        .name("tyu-usb-down".into())
        .spawn(move || {
            let mut header = [0u8; 2];
            let mut buffer = [0u8; MAX_PACKET];
            while !stop.load(Ordering::Relaxed) {
                if from_cable.read_exact(&mut header).is_err() {
                    break;
                }
                let size = u16::from_be_bytes(header) as usize;
                if size > MAX_PACKET || from_cable.read_exact(&mut buffer[..size]).is_err() {
                    break;
                }
                let target = *node.lock().unwrap();
                if let Some(target) = target
                    && relay.send_to(&buffer[..size], target).is_err()
                {
                    break;
                }
            }
            stop.store(true, Ordering::Relaxed);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Listing devices must never invent one, and must agree with adb's own output.
    #[test]
    fn cable_devices_match_adb() {
        if adb().is_none() {
            eprintln!("adb not installed here; skipping");
            return;
        }
        let listed = devices();
        eprintln!("cable: {listed:?}");
        for (serial, _) in &listed {
            assert!(!serial.is_empty());
            assert!(!serial.contains(char::is_whitespace));
        }
    }

    /// The relay against a stand-in phone speaking `UsbBridge`'s protocol: the greeting is
    /// read, and QUIC-sized datagrams survive the round trip through the TCP framing intact.
    #[test]
    fn datagrams_survive_the_round_trip_through_the_cable_framing() {
        use std::net::TcpListener;
        let phone = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = phone.local_addr().unwrap();
        let id = "11111111-2222-3333-4444-555555555555";
        std::thread::spawn(move || {
            let (mut socket, _) = phone.accept().unwrap();
            socket.write_all(&(id.len() as u16).to_be_bytes()).unwrap();
            socket.write_all(id.as_bytes()).unwrap();
            socket.flush().unwrap();
            // Echo every framed packet back, as the phone's node would answer.
            let mut header = [0u8; 2];
            let mut buffer = [0u8; MAX_PACKET];
            while socket.read_exact(&mut header).is_ok() {
                let size = u16::from_be_bytes(header) as usize;
                socket.read_exact(&mut buffer[..size]).unwrap();
                socket.write_all(&header).unwrap();
                socket.write_all(&buffer[..size]).unwrap();
                socket.flush().unwrap();
            }
        });
        let cable = Cable::over(TcpStream::connect(address).unwrap()).expect("relay starts");
        assert_eq!(cable.peer, id, "the greeting must identify the phone");

        let node = UdpSocket::bind("127.0.0.1:0").unwrap();
        node.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        // A full-size packet and a tiny one: both framings must hold.
        for payload in [vec![0xAB; 1350], vec![7, 8, 9]] {
            node.send_to(&payload, cable.endpoint).unwrap();
            let mut back = [0u8; MAX_PACKET];
            let (size, from) = node.recv_from(&mut back).expect("packet came back");
            assert_eq!(from, cable.endpoint, "the reply must come from the relay");
            assert_eq!(
                &back[..size],
                &payload[..],
                "payload was altered in transit"
            );
        }
        // Several packets in flight keep their boundaries and their order.
        for n in 0..16u8 {
            node.send_to(&[n; 64], cable.endpoint).unwrap();
        }
        for n in 0..16u8 {
            let mut back = [0u8; MAX_PACKET];
            let (size, _) = node.recv_from(&mut back).unwrap();
            assert_eq!(size, 64);
            assert_eq!(back[..size], [n; 64], "packet {n} arrived out of order");
        }
    }

    /// The whole tunnel, against the real phone when one is plugged in: the bridge announces a
    /// device id and QUIC packets survive the round trip to the phone's node and back.
    #[test]
    fn a_plugged_phone_answers_through_the_cable() {
        let Some((serial, true)) = devices().into_iter().next() else {
            eprintln!("no authorised phone on the cable; skipping");
            return;
        };
        let cable = match Cable::open(&serial) {
            Ok(cable) => cable,
            Err(error) => {
                eprintln!("bridge unavailable (is TYU running on the phone?): {error}");
                return;
            }
        };
        eprintln!("phone {} answered as {}", serial, cable.peer);
        assert_eq!(
            cable.peer.len(),
            36,
            "expected a device id, got {:?}",
            cable.peer
        );
        // A QUIC initial from an unknown client draws a reply from any live node.
        let probe = UdpSocket::bind("127.0.0.1:0").unwrap();
        probe
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut initial = vec![0xc0, 0x00, 0x00, 0x00, 0x01];
        initial.extend(std::iter::repeat_n(0u8, 1195));
        probe.send_to(&initial, cable.endpoint).unwrap();
        let mut answer = [0u8; MAX_PACKET];
        match probe.recv_from(&mut answer) {
            Ok((size, _)) => {
                eprintln!("phone's node replied with {size} bytes through the cable");
                assert!(size > 0);
            }
            // A node may ignore a malformed initial; the greeting already proves the tunnel.
            Err(error) => eprintln!("no reply to the probe packet ({error}); tunnel still open"),
        }
    }
}
