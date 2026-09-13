use crate::{Result, TyuErrorCode};
use std::collections::HashMap;
use tyu_types::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionKind {
    Display(TyuMode),
    Service(ServiceKind),
}
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub peer: DeviceId,
    pub kind: SessionKind,
    pub state: SessionState,
}
#[derive(Default)]
pub struct SessionManager {
    pub sessions: HashMap<SessionId, Session>,
    pub media: HashMap<StreamId, (SessionId, MediaMetadata)>,
    pub media_senders: HashMap<StreamId, DeviceId>,
}
impl SessionManager {
    pub fn start(
        &mut self,
        id: SessionId,
        peer: DeviceId,
        kind: SessionKind,
        local: &[Capability],
        remote: &[Capability],
    ) -> Result<()> {
        if self.sessions.len() >= 64 || self.sessions.contains_key(&id) {
            return Err(TyuErrorCode::Limit.into());
        }
        if !supports(&kind, local, remote) {
            return Err(TyuErrorCode::Unsupported.into());
        }
        if matches!(kind, SessionKind::Display(_))
            && self
                .sessions
                .values()
                .any(|s| s.peer == peer && matches!(s.kind, SessionKind::Display(_)))
        {
            return Err(TyuErrorCode::InvalidState.into());
        }
        self.sessions.insert(
            id,
            Session {
                id,
                peer,
                kind,
                state: SessionState::Ready,
            },
        );
        Ok(())
    }
    pub fn stop(&mut self, id: SessionId, peer: DeviceId) -> Result<Session> {
        if self.sessions.get(&id).is_none_or(|s| s.peer != peer) {
            return Err(TyuErrorCode::InvalidState.into());
        }
        self.media.retain(|_, (session, _)| *session != id);
        self.media_senders
            .retain(|stream, _| self.media.contains_key(stream));
        self.sessions
            .remove(&id)
            .ok_or(TyuErrorCode::InvalidState.into())
    }
    pub fn disconnect(&mut self, peer: DeviceId) {
        self.sessions.retain(|_, s| s.peer != peer);
        self.media
            .retain(|_, (id, _)| self.sessions.contains_key(id));
        self.media_senders
            .retain(|stream, _| self.media.contains_key(stream));
    }
}
pub fn supports(kind: &SessionKind, a: &[Capability], b: &[Capability]) -> bool {
    use Capability::*;
    let pair = |source, receiver| {
        a.contains(&source) && b.contains(&receiver) || b.contains(&source) && a.contains(&receiver)
    };
    match kind {
        SessionKind::Display(TyuMode::Monitor) => pair(MonitorSource, MonitorReceiver),
        SessionKind::Display(TyuMode::Mirror) => pair(MirrorSource, MirrorReceiver),
        SessionKind::Display(TyuMode::Bypass) => true,
        SessionKind::Service(service) => match service {
            ServiceKind::Camera => pair(CameraSource, CameraReceiver),
            ServiceKind::Microphone => pair(MicrophoneSource, MicrophoneReceiver),
            ServiceKind::Storage => pair(StorageProvider, StorageConsumer),
            ServiceKind::Files => pair(FileSend, FileReceive),
            ServiceKind::Clipboard => pair(ClipboardSource, ClipboardReceiver),
            ServiceKind::Input => pair(TouchSource, InputReceiver),
        },
    }
}
