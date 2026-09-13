//! Legacy HTTP development fallback, enabled by --legacy-pairing or explicit UI fixtures.
//! This does not authenticate a TyuLink peer or grant access to Core services.
//! A phone must request access and the desktop must approve it.
//! No capture, file access or screen transport is exposed by this server.
use std::{
    collections::VecDeque,
    net::{IpAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Response, Server};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Peer {
    pub token: String,
    pub name: String,
    pub last_seen: Instant,
}

pub struct PairingState {
    pub ticket: String,
    issued: Instant,
    pub pending: Option<Peer>,
    pub active: Option<Peer>,
    outcomes: VecDeque<(String, &'static str)>,
}

impl Default for PairingState {
    fn default() -> Self {
        Self {
            ticket: Uuid::new_v4().to_string(),
            issued: Instant::now(),
            pending: None,
            active: None,
            outcomes: VecDeque::new(),
        }
    }
}

impl PairingState {
    fn rotate(&mut self) {
        self.ticket = Uuid::new_v4().to_string();
        self.issued = Instant::now();
    }
    fn finish(&mut self, token: String, status: &'static str) {
        self.outcomes.push_back((token, status));
        while self.outcomes.len() > 32 {
            self.outcomes.pop_front();
        }
    }
    pub fn request(&mut self, token: &str, name: &str) -> Result<(), &'static str> {
        self.expire();
        if token != self.ticket {
            return Err("expired");
        }
        if self.active.is_some() {
            return Err("busy");
        }
        if self.pending.is_some() {
            return Ok(());
        }
        let name: String = name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect();
        if name.is_empty() {
            return Err("name");
        }
        self.pending = Some(Peer {
            token: token.into(),
            name,
            last_seen: Instant::now(),
        });
        Ok(())
    }
    pub fn decide(&mut self, accept: bool) -> Option<Peer> {
        self.expire();
        let peer = self.pending.take()?;
        if accept {
            let mut connected = peer.clone();
            connected.last_seen = Instant::now();
            self.active = Some(connected);
        } else {
            self.finish(peer.token.clone(), "rejected");
        }
        self.rotate();
        Some(peer)
    }
    pub fn disconnect(&mut self) {
        if let Some(peer) = self.active.take() {
            self.finish(peer.token, "disconnected");
        }
        self.rotate();
    }
    pub fn expire(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.last_seen.elapsed() > Duration::from_secs(120))
        {
            let peer = self.pending.take().unwrap();
            self.finish(peer.token, "expired");
            self.rotate();
        }
        if self
            .active
            .as_ref()
            .is_some_and(|p| p.last_seen.elapsed() > Duration::from_secs(30))
        {
            self.disconnect();
        }
        if self.pending.is_none() && self.issued.elapsed() > Duration::from_secs(600) {
            self.rotate();
        }
    }
    pub fn status(&mut self, token: &str) -> &'static str {
        self.expire();
        if let Some(peer) = self.active.as_mut().filter(|p| p.token == token) {
            peer.last_seen = Instant::now();
            return "accepted";
        }
        if self.pending.as_ref().is_some_and(|p| p.token == token) {
            return "pending";
        }
        self.outcomes
            .iter()
            .rev()
            .find(|(t, _)| t == token)
            .map_or("expired", |(_, s)| *s)
    }
}

pub struct PairingServer {
    pub state: Arc<Mutex<PairingState>>,
    pub base_url: String,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl PairingServer {
    pub fn start() -> Result<Self, String> {
        let probe = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        // UDP connect selects the outbound interface; it sends no packet.
        probe.connect("192.0.2.1:80").map_err(|e| e.to_string())?;
        let ip = probe.local_addr().map_err(|e| e.to_string())?.ip();
        match ip {
            IpAddr::V4(ip) if ip.is_private() => Self::bind(IpAddr::V4(ip)),
            _ => Err("No se encontró una dirección de red local. Conecta este PC a Wi-Fi.".into()),
        }
    }
    fn bind(ip: IpAddr) -> Result<Self, String> {
        let server = Server::http((ip, 0)).map_err(|e| e.to_string())?;
        let address = server
            .server_addr()
            .to_ip()
            .ok_or("No se pudo abrir el puerto local")?;
        let base_url = format!("http://{address}");
        let state = Arc::new(Mutex::new(PairingState::default()));
        let shared = state.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(100)) else {
                    continue;
                };
                let parsed = url::Url::parse(&format!("http://localhost{}", req.url()));
                let Ok(parsed) = parsed else {
                    let _ = req.respond(Response::empty(400));
                    continue;
                };
                let token = parsed
                    .query_pairs()
                    .find(|(k, _)| k == "token")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                let (code, content_type, body) = match (req.method(), parsed.path()) {
                    (&Method::Get, "/") => (
                        200,
                        "text/html; charset=utf-8",
                        include_str!("pairing.html").to_string(),
                    ),
                    (&Method::Post, "/request") => {
                        let name = parsed
                            .query_pairs()
                            .find(|(k, _)| k == "name")
                            .map(|(_, v)| v.into_owned())
                            .unwrap_or_default();
                        let result = shared.lock().unwrap().request(&token, &name);
                        match result {
                            Ok(()) => (202, "application/json", r#"{"status":"pending"}"#.into()),
                            Err(reason) => (
                                409,
                                "application/json",
                                serde_json::json!({"status":reason}).to_string(),
                            ),
                        }
                    }
                    (&Method::Get, "/status") => {
                        let status = shared.lock().unwrap().status(&token);
                        (
                            200,
                            "application/json",
                            serde_json::json!({"status":status}).to_string(),
                        )
                    }
                    (&Method::Post, "/leave") => {
                        let mut state = shared.lock().unwrap();
                        if state.active.as_ref().is_some_and(|p| p.token == token) {
                            state.disconnect();
                        }
                        if state.pending.as_ref().is_some_and(|p| p.token == token) {
                            state.decide(false);
                        }
                        (
                            200,
                            "application/json",
                            r#"{"status":"disconnected"}"#.into(),
                        )
                    }
                    _ => (404, "text/plain", "No encontrado".into()),
                };
                let response = Response::from_string(body).with_status_code(code)
                    .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
                    .with_header(Header::from_bytes("Referrer-Policy", "no-referrer").unwrap())
                    .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
                    .with_header(Header::from_bytes("Content-Security-Policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'").unwrap());
                let _ = req.respond(response);
            }
        });
        Ok(Self {
            state,
            base_url,
            stopping,
            worker: Some(worker),
        })
    }
    pub fn url(&self) -> String {
        format!(
            "{}/?token={}",
            self.base_url,
            self.state.lock().unwrap().ticket
        )
    }
}

impl Drop for PairingServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        // Do not hold up the UI if an incomplete client request is still being read.
        if self.worker.as_ref().is_some_and(|w| w.is_finished())
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn http(server: &PairingServer, method: &str, path: &str) -> String {
        let address = server.base_url.trim_start_matches("http://");
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    #[test]
    fn real_http_request_approval_heartbeat_and_disconnect() {
        let server = PairingServer::bind("127.0.0.1".parse().unwrap()).unwrap();
        let ticket = server.state.lock().unwrap().ticket.clone();
        assert!(http(&server, "POST", "/request?token=invalid&name=Phone").contains("409"));
        assert!(http(&server, "GET", "/request?name=Phone").contains("404"));
        assert!(
            http(
                &server,
                "POST",
                &format!("/request?token={ticket}&name=Mi%20Pixel")
            )
            .contains("202")
        );
        assert_eq!(
            server.state.lock().unwrap().pending.as_ref().unwrap().name,
            "Mi Pixel"
        );
        assert!(http(&server, "GET", &format!("/status?token={ticket}")).contains("pending"));
        assert!(server.state.lock().unwrap().active.is_none());
        server.state.lock().unwrap().decide(true);
        assert!(http(&server, "GET", &format!("/status?token={ticket}")).contains("accepted"));
        http(&server, "POST", "/leave?token=invalid");
        assert!(server.state.lock().unwrap().active.is_some());
        http(&server, "POST", &format!("/leave?token={ticket}"));
        assert!(server.state.lock().unwrap().active.is_none());
        assert!(http(&server, "GET", &format!("/status?token={ticket}")).contains("disconnected"));
    }
    #[test]
    fn request_requires_explicit_acceptance_and_rotates_qr() {
        let mut state = PairingState::default();
        let ticket = state.ticket.clone();
        assert!(state.request("wrong", "Phone").is_err());
        state.request(&ticket, "Mi teléfono").unwrap();
        assert!(state.active.is_none());
        assert_eq!(state.status(&ticket), "pending");
        state.decide(true).unwrap();
        assert_eq!(state.status(&ticket), "accepted");
        assert_ne!(state.ticket, ticket);
        assert_eq!(state.active.as_ref().unwrap().name, "Mi teléfono");
        state.disconnect();
        assert_eq!(state.status(&ticket), "disconnected");
    }
    #[test]
    fn rejection_and_timeout_do_not_connect() {
        let mut state = PairingState::default();
        let ticket = state.ticket.clone();
        state.request(&ticket, "Phone").unwrap();
        state.decide(false);
        assert!(state.active.is_none());
        assert_eq!(state.status(&ticket), "rejected");
        assert!(state.request(&ticket, "Phone").is_err());
        let ticket = state.ticket.clone();
        state.request(&ticket, "Phone").unwrap();
        state.pending.as_mut().unwrap().last_seen = Instant::now() - Duration::from_secs(121);
        assert!(state.decide(true).is_none());
        assert_eq!(state.status(&ticket), "expired");
        assert!(state.active.is_none());
    }
    #[test]
    fn silent_phone_disconnects_and_empty_names_are_rejected() {
        let mut state = PairingState::default();
        let ticket = state.ticket.clone();
        assert!(state.request(&ticket, "  ").is_err());
        state.request(&ticket, "Phone").unwrap();
        state.decide(true);
        state.active.as_mut().unwrap().last_seen = Instant::now() - Duration::from_secs(31);
        state.expire();
        assert!(state.active.is_none());
    }
}
