//! TLS proves possession of both private keys. Application admission then checks
//! trust or a one-time ticket before any general-purpose stream is serviced.
use crate::{
    CoreError, Frame, Result, TyuErrorCode,
    identity::{DeviceFingerprint, DeviceIdentity},
};
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
};
use std::{net::SocketAddr, sync::Arc, time::Duration};

#[derive(Debug)]
struct PinnedServer {
    pin: ServerIdentity,
}
#[derive(Debug, Clone, Copy)]
pub enum ServerIdentity {
    Trusted(DeviceFingerprint),
    Discovered(tyu_types::DeviceId),
}
impl ServerIdentity {
    fn matches(self, fingerprint: DeviceFingerprint) -> bool {
        match self {
            Self::Trusted(pin) => pin == fingerprint,
            Self::Discovered(id) => id == fingerprint.device_id(),
        }
    }
}
fn schemes() -> Vec<SignatureScheme> {
    rustls::crypto::ring::default_provider()
        .signature_verification_algorithms
        .supported_schemes()
}
fn signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
        message,
        cert,
        dss,
        &rustls::crypto::ring::default_provider().signature_verification_algorithms,
    )
}
impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if !intermediates.is_empty()
            || end.len() > 8192
            || !self.pin.matches(DeviceFingerprint::of(end))
        {
            return Err(rustls::Error::General("identity pin mismatch".into()));
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &CertificateDer<'_>,
        _d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.3 required".into()))
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        signature(m, c, d)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}
#[derive(Debug)]
struct QuarantinedClient;
impl ClientCertVerifier for QuarantinedClient {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        if end.len() > 8192 || !intermediates.is_empty() {
            return Err(rustls::Error::General("invalid device certificate".into()));
        }
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &CertificateDer<'_>,
        _d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.3 required".into()))
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        signature(m, c, d)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}
fn config() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config.keep_alive_interval(Some(Duration::from_secs(5)));
    config.max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(
        20000,
    ))));
    config.max_concurrent_bidi_streams(16u32.into());
    config.max_concurrent_uni_streams(0u32.into());
    config.stream_receive_window(256_000u32.into());
    config.receive_window(4_000_000u32.into());
    config.send_window(4_000_000);
    config.datagram_receive_buffer_size(Some(1024 * 1024));
    config.datagram_send_buffer_size(1024 * 1024);
    Arc::new(config)
}
pub fn endpoint(identity: &DeviceIdentity, bind: SocketAddr) -> Result<quinn::Endpoint> {
    let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(neterr)?
    .with_client_cert_verifier(Arc::new(QuarantinedClient))
    .with_single_cert(
        vec![CertificateDer::from(identity.certificate.0.clone())],
        identity.key(),
    )
    .map_err(neterr)?;
    tls.alpn_protocols = vec![tyu_protocol::ALPN.to_vec()];
    tls.max_early_data_size = 0;
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).map_err(neterr)?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    server.transport_config(config());
    Ok(quinn::Endpoint::server(server, bind)?)
}
pub async fn connect(
    endpoint: &quinn::Endpoint,
    identity: &DeviceIdentity,
    address: SocketAddr,
    pin: DeviceFingerprint,
) -> Result<quinn::Connection> {
    connect_identity(endpoint, identity, address, ServerIdentity::Trusted(pin)).await
}
pub async fn connect_identity(
    endpoint: &quinn::Endpoint,
    identity: &DeviceIdentity,
    address: SocketAddr,
    pin: ServerIdentity,
) -> Result<quinn::Connection> {
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(neterr)?
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(PinnedServer { pin }))
    .with_client_auth_cert(
        vec![CertificateDer::from(identity.certificate.0.clone())],
        identity.key(),
    )
    .map_err(neterr)?;
    tls.alpn_protocols = vec![tyu_protocol::ALPN.to_vec()];
    tls.enable_early_data = false;
    let mut client = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(neterr)?,
    ));
    client.transport_config(config());
    timed(async {
        endpoint
            .connect_with(client, address, "tyu.local")
            .map_err(neterr)?
            .await
            .map_err(neterr)
    })
    .await
}
pub fn peer_pin(connection: &quinn::Connection) -> Result<DeviceFingerprint> {
    let identity = connection
        .peer_identity()
        .ok_or(TyuErrorCode::Unauthorized)?;
    let chain = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| TyuErrorCode::Unauthorized)?;
    let cert = chain.first().ok_or(TyuErrorCode::Unauthorized)?;
    Ok(DeviceFingerprint::of(cert))
}
pub fn neterr(error: impl std::fmt::Display) -> CoreError {
    CoreError::Transport(error.to_string())
}
pub async fn timed<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(15), future)
        .await
        .map_err(|_| TyuErrorCode::Timeout)?
}
pub async fn write(send: &mut quinn::SendStream, frame: &Frame) -> Result<()> {
    let bytes = tyu_protocol::encode(frame)?;
    send.write_all(&bytes).await.map_err(neterr)
}
pub async fn read(recv: &mut quinn::RecvStream) -> Result<Frame> {
    let mut header = [0; tyu_protocol::HEADER];
    recv.read_exact(&mut header).await.map_err(neterr)?;
    let len = tyu_protocol::payload_length(&header)?;
    let mut bytes = vec![0; tyu_protocol::HEADER + len];
    bytes[..tyu_protocol::HEADER].copy_from_slice(&header);
    recv.read_exact(&mut bytes[tyu_protocol::HEADER..])
        .await
        .map_err(neterr)?;
    Ok(tyu_protocol::decode(&bytes)?)
}
