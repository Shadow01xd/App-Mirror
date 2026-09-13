//! Persistent identities and explicitly granted trust. Secrets never implement Debug.
use crate::{CoreError, Result, TyuErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tyu_types::{DeviceId, DeviceInfo, ProtocolVersion};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceFingerprint(pub [u8; 32]);
impl DeviceFingerprint {
    pub fn of(cert: &[u8]) -> Self {
        Self(Sha256::digest(cert).into())
    }
    pub fn device_id(self) -> DeviceId {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&self.0[..16]);
        DeviceId(tyu_types_uuid(bytes))
    }
}
// A UUID-shaped opaque ID derived from the certificate, not a caller-supplied name.
fn tyu_types_uuid(bytes: [u8; 16]) -> uuid::Uuid {
    uuid::Uuid::from_bytes(bytes)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceCertificate(pub Vec<u8>);
#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub certificate: DeviceCertificate,
    private_key: Vec<u8>,
    pub friendly_name: String,
}
impl Drop for DeviceIdentity {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.private_key);
    }
}
impl DeviceIdentity {
    pub fn generate(name: &str) -> Result<Self> {
        if name.trim().is_empty() || name.len() > 128 {
            return Err(TyuErrorCode::Limit.into());
        }
        let cert = rcgen::generate_simple_self_signed(vec!["tyu.local".into()])
            .map_err(|_| CoreError::Store)?;
        Ok(Self {
            certificate: DeviceCertificate(cert.cert.der().to_vec()),
            private_key: cert.signing_key.serialize_der(),
            friendly_name: name.into(),
        })
    }
    pub fn fingerprint(&self) -> DeviceFingerprint {
        DeviceFingerprint::of(&self.certificate.0)
    }
    pub fn id(&self) -> DeviceId {
        self.fingerprint().device_id()
    }
    pub(crate) fn key(&self) -> rustls::pki_types::PrivateKeyDer<'static> {
        rustls::pki_types::PrivatePkcs8KeyDer::from(self.private_key.clone()).into()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub device: DeviceInfo,
    pub fingerprint: DeviceFingerprint,
    pub endpoint: SocketAddr,
    pub revoked: bool,
}
pub trait IdentityStore: Send + Sync {
    fn load_identity(&self) -> Result<Option<DeviceIdentity>>;
    fn save_identity(&self, identity: &DeviceIdentity) -> Result<()>;
}
pub trait TrustedPeerStore: Send + Sync {
    fn peers(&self) -> Result<Vec<TrustedPeer>>;
    fn put_peer(&self, peer: TrustedPeer) -> Result<()>;
    fn revoke(&self, peer: DeviceId) -> Result<()>;
}
pub trait Store: IdentityStore + TrustedPeerStore {}
impl<T: IdentityStore + TrustedPeerStore> Store for T {}
#[derive(Clone, Serialize, Deserialize)]
struct StoreData {
    version: u16,
    identity: Option<DeviceIdentity>,
    peers: Vec<TrustedPeer>,
}
impl Default for StoreData {
    fn default() -> Self {
        Self {
            version: 1,
            identity: None,
            peers: Vec::new(),
        }
    }
}
#[derive(Default)]
pub struct MemoryStore {
    data: Mutex<StoreData>,
}
impl IdentityStore for MemoryStore {
    fn load_identity(&self) -> Result<Option<DeviceIdentity>> {
        Ok(self
            .data
            .lock()
            .map_err(|_| CoreError::Store)?
            .identity
            .clone())
    }
    fn save_identity(&self, identity: &DeviceIdentity) -> Result<()> {
        self.data.lock().map_err(|_| CoreError::Store)?.identity = Some(identity.clone());
        Ok(())
    }
}
fn put(data: &mut StoreData, peer: TrustedPeer) -> Result<()> {
    if peer.device.id != peer.fingerprint.device_id() {
        return Err(TyuErrorCode::Unauthorized.into());
    }
    if let Some(existing) = data
        .peers
        .iter_mut()
        .find(|p| p.device.id == peer.device.id)
    {
        if existing.revoked {
            return Err(TyuErrorCode::Revoked.into());
        }
        *existing = peer;
    } else {
        if data.peers.len() >= 256 {
            return Err(TyuErrorCode::Limit.into());
        }
        data.peers.push(peer);
    }
    Ok(())
}
impl TrustedPeerStore for MemoryStore {
    fn peers(&self) -> Result<Vec<TrustedPeer>> {
        Ok(self
            .data
            .lock()
            .map_err(|_| CoreError::Store)?
            .peers
            .clone())
    }
    fn put_peer(&self, peer: TrustedPeer) -> Result<()> {
        put(&mut *self.data.lock().map_err(|_| CoreError::Store)?, peer)
    }
    fn revoke(&self, peer: DeviceId) -> Result<()> {
        if let Some(p) = self
            .data
            .lock()
            .map_err(|_| CoreError::Store)?
            .peers
            .iter_mut()
            .find(|p| p.device.id == peer)
        {
            p.revoked = true;
        }
        Ok(())
    }
}

/// Versioned per-user persistence; DPAPI on Windows, 0700/0600 on Unix.
/// Corrupt stores fail closed: never silently rotate identity or erase trust.
pub struct WindowsPersistentStore {
    path: PathBuf,
    data: Mutex<StoreData>,
}
impl WindowsPersistentStore {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        }
        let path = root.join("identity-v1.bin");
        let data = match std::fs::metadata(&path) {
            Ok(m) => {
                if m.len() > 2 * 1024 * 1024 {
                    return Err(CoreError::Store);
                }
                let sealed = std::fs::read(&path)?;
                let plain = Zeroizing::new(unprotect(&sealed)?);
                let (data, rest): (StoreData, &[u8]) =
                    postcard::take_from_bytes(&plain).map_err(|_| CoreError::Store)?;
                if data.version != 1 || !rest.is_empty() || data.peers.len() > 256 {
                    return Err(CoreError::Store);
                }
                data
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreData::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }
    fn update(&self, op: impl FnOnce(&mut StoreData) -> Result<()>) -> Result<()> {
        let mut guard = self.data.lock().map_err(|_| CoreError::Store)?;
        let mut next = guard.clone();
        op(&mut next)?;
        let plain = Zeroizing::new(postcard::to_allocvec(&next).map_err(|_| CoreError::Store)?);
        let sealed = Zeroizing::new(protect(&plain)?);
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        use std::io::Write;
        let result = (|| -> Result<()> {
            file.write_all(&sealed)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _cleanup = std::fs::remove_file(&temporary);
        }
        result?;
        *guard = next;
        Ok(())
    }
}
impl IdentityStore for WindowsPersistentStore {
    fn load_identity(&self) -> Result<Option<DeviceIdentity>> {
        Ok(self
            .data
            .lock()
            .map_err(|_| CoreError::Store)?
            .identity
            .clone())
    }
    fn save_identity(&self, identity: &DeviceIdentity) -> Result<()> {
        self.update(|d| {
            d.identity = Some(identity.clone());
            Ok(())
        })
    }
}
impl TrustedPeerStore for WindowsPersistentStore {
    fn peers(&self) -> Result<Vec<TrustedPeer>> {
        Ok(self
            .data
            .lock()
            .map_err(|_| CoreError::Store)?
            .peers
            .clone())
    }
    fn put_peer(&self, peer: TrustedPeer) -> Result<()> {
        self.update(|d| put(d, peer))
    }
    fn revoke(&self, peer: DeviceId) -> Result<()> {
        self.update(|d| {
            if let Some(p) = d.peers.iter_mut().find(|p| p.device.id == peer) {
                p.revoked = true;
            }
            Ok(())
        })
    }
}
#[cfg(windows)]
fn crypt(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len().try_into().map_err(|_| CoreError::Store)?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI allocates output with LocalAlloc. Copy then zero/free on both paths.
    unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|_| CoreError::Store)?;
        let slice = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        zeroize::Zeroize::zeroize(slice);
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(result)
    }
}
#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>> {
    crypt(data, true)
}
#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    crypt(data, false)
}
#[cfg(not(windows))]
fn protect(data: &[u8]) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}
#[cfg(not(windows))]
fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn random_nonce() -> Result<[u8; 32]> {
    let mut n = [0; 32];
    getrandom::fill(&mut n).map_err(|_| CoreError::Store)?;
    Ok(n)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PairingNonce(pub [u8; 32]);
#[derive(Clone, Serialize, Deserialize)]
pub struct PairingTicket {
    pub version: ProtocolVersion,
    pub endpoint: SocketAddr,
    pub ticket: [u8; 32],
    pub expires: u64,
    pub fingerprint: DeviceFingerprint,
}
impl std::fmt::Debug for PairingTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingTicket")
            .field("endpoint", &self.endpoint)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}
impl PairingTicket {
    pub fn uri(&self) -> Result<String> {
        Ok(format!(
            "tyu://pair/{}",
            hex::encode(postcard::to_allocvec(self).map_err(|_| CoreError::Store)?)
        ))
    }
    pub fn parse(uri: &str) -> Result<Self> {
        if uri.len() > 1024 {
            return Err(TyuErrorCode::Limit.into());
        }
        let raw = hex::decode(
            uri.strip_prefix("tyu://pair/")
                .ok_or(TyuErrorCode::Malformed)?,
        )
        .map_err(|_| TyuErrorCode::Malformed)?;
        let (ticket, rest): (Self, &[u8]) =
            postcard::take_from_bytes(&raw).map_err(|_| TyuErrorCode::Malformed)?;
        if !rest.is_empty() || ticket.endpoint.port() == 0 || ticket.endpoint.ip().is_unspecified()
        {
            return Err(TyuErrorCode::Malformed.into());
        }
        tyu_protocol::negotiate(ticket.version)?;
        if ticket.expires <= now_seconds() {
            return Err(TyuErrorCode::Expired.into());
        }
        Ok(ticket)
    }
}
#[derive(Default)]
pub(crate) struct TicketBook {
    active: Option<PairingTicket>,
    used: HashMap<[u8; 32], u64>,
}
impl TicketBook {
    pub fn issue(
        &mut self,
        endpoint: SocketAddr,
        fingerprint: DeviceFingerprint,
    ) -> Result<PairingTicket> {
        self.used.retain(|_, expiry| *expiry > now_seconds());
        let ticket = PairingTicket {
            version: ProtocolVersion::V1,
            endpoint,
            ticket: random_nonce()?,
            expires: now_seconds() + 120,
            fingerprint,
        };
        self.active = Some(ticket.clone());
        Ok(ticket)
    }
    pub fn consume(&mut self, ticket: [u8; 32]) -> Result<u64> {
        if self.used.contains_key(&ticket) {
            return Err(TyuErrorCode::Replay.into());
        }
        let active = self.active.as_ref().ok_or(TyuErrorCode::Unauthorized)?;
        if active.expires <= now_seconds() {
            return Err(TyuErrorCode::Expired.into());
        }
        if active.ticket != ticket {
            return Err(TyuErrorCode::Unauthorized.into());
        }
        let expiry = active.expires;
        self.used.insert(ticket, expiry);
        self.active = None;
        Ok(expiry)
    }
    pub fn expire(&mut self) -> bool {
        if self
            .active
            .as_ref()
            .is_some_and(|t| t.expires <= now_seconds())
        {
            self.active = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_time_tickets_expire_and_reject_unknown_values() {
        let mut book = TicketBook::default();
        let address = "127.0.0.1:12345".parse().unwrap();
        let pin = DeviceFingerprint([7; 32]);
        let ticket = book.issue(address, pin).unwrap();
        assert!(book.consume([0; 32]).is_err());
        assert!(book.consume(ticket.ticket).is_ok());
        assert!(matches!(
            book.consume(ticket.ticket),
            Err(CoreError::Code(TyuErrorCode::Replay))
        ));
        let ticket = book.issue(address, pin).unwrap();
        book.active.as_mut().unwrap().expires = 0;
        assert!(matches!(
            book.consume(ticket.ticket),
            Err(CoreError::Code(TyuErrorCode::Expired))
        ));
        assert!(book.expire());
    }
}
