//! Isolated Bypass receivers. Neither pictures nor PCM enter the Mirror decoder.

use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc,
    thread,
    time::Duration,
};
use tyu_core::*;

pub struct Status {
    pub session: SessionId,
    pub service: ServiceKind,
    pub active: bool,
    pub message: String,
}
struct Sink {
    peer: DeviceId,
    service: ServiceKind,
    sender: mpsc::SyncSender<bytes::Bytes>,
    last_keyframe: Option<std::time::Instant>,
}
pub struct Receivers {
    sinks: HashMap<SessionId, Sink>,
    owned: VecDeque<SessionId>,
    status: mpsc::Sender<Status>,
    commands: tokio::sync::mpsc::Sender<TyuCommand>,
}
impl Receivers {
    pub fn new(
        status: mpsc::Sender<Status>,
        commands: tokio::sync::mpsc::Sender<TyuCommand>,
    ) -> Self {
        Self {
            sinks: HashMap::new(),
            owned: VecDeque::new(),
            status,
            commands,
        }
    }
    pub fn event(&mut self, event: &TyuEvent) -> bool {
        match event {
            TyuEvent::ServiceStarted {
                peer,
                session,
                service,
            } if matches!(service, ServiceKind::Camera | ServiceKind::Microphone) => {
                let (peer, session, service) = (*peer, *session, *service);
                self.owned.push_back(session);
                if self.owned.len() > 256 {
                    self.owned.pop_front();
                }
                if self.sinks.values().any(|s| s.service == service) {
                    let _ = self
                        .commands
                        .try_send(TyuCommand::StopService { peer, session });
                    return true;
                }
                let (sender, rx) = mpsc::sync_channel::<bytes::Bytes>(8);
                self.sinks.insert(
                    session,
                    Sink {
                        peer,
                        service,
                        sender,
                        last_keyframe: None,
                    },
                );
                let status = self.status.clone();
                let commands = self.commands.clone();
                thread::spawn(move || {
                    let result: std::result::Result<(), String> = if service == ServiceKind::Camera
                    {
                        (|| {
                            let mut camera =
                                super::bypass_camera::Camera::new().map_err(|e| e.to_string())?;
                            let mut ready = false;
                            while let Ok(frame) = rx.recv() {
                                if camera.push(&frame).map_err(|e| e.to_string())? && !ready {
                                    ready = true;
                                    let _ = status.send(Status {
                                        session,
                                        service,
                                        active: true,
                                        message: "Cámara disponible: selecciona TYU Camera".into(),
                                    });
                                }
                            }
                            Ok(())
                        })()
                    } else {
                        microphone(session, rx, &status).map_err(|e| e.to_string())
                    };
                    if let Err(message) = result {
                        let _ = status.send(Status {
                            session,
                            service,
                            active: false,
                            message,
                        });
                        let _ = commands.blocking_send(TyuCommand::StopService { peer, session });
                    }
                });
            }
            TyuEvent::MediaFrame {
                session,
                stream,
                data,
                ..
            } => {
                if let Some(sink) = self.sinks.get_mut(session) {
                    if matches!(
                        sink.sender.try_send(data.clone()),
                        Err(mpsc::TrySendError::Full(_))
                    ) && sink.service == ServiceKind::Camera
                        && sink
                            .last_keyframe
                            .is_none_or(|t| t.elapsed() >= Duration::from_millis(500))
                    {
                        let mut frame =
                            Frame::new(Message::MediaKeyframeRequest { stream: *stream });
                        frame.session = Some(*session);
                        if self
                            .commands
                            .try_send(TyuCommand::Request {
                                peer: sink.peer,
                                frame,
                            })
                            .is_ok()
                        {
                            sink.last_keyframe = Some(std::time::Instant::now());
                        }
                    }
                    return true;
                }
                // Late Bypass packets must never enter the existing Mirror decoder.
                if self.owned.contains(session) {
                    return true;
                }
            }
            TyuEvent::ServiceStopped { session, .. } => {
                self.sinks.remove(session);
            }
            TyuEvent::Disconnected { peer } => {
                self.sinks.retain(|_, s| s.peer != *peer);
            }
            _ => {}
        }
        false
    }
}
fn microphone(
    session: SessionId,
    rx: mpsc::Receiver<bytes::Bytes>,
    status: &mpsc::Sender<Status>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let microphone = super::bypass_microphone::Microphone::new()?;
    let mut ready = false;
    while let Ok(bytes) = rx.recv() {
        microphone.push(&bytes)?;
        if !ready {
            ready = true;
            let _ = status.send(Status {
                session,
                service: ServiceKind::Microphone,
                active: true,
                message: "Micrófono disponible: selecciona TYU Microphone".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bypass_overflow_requests_keyframe_and_late_packets_never_reach_mirror() {
        let (status, _) = mpsc::channel();
        let (commands, mut rx) = tokio::sync::mpsc::channel(8);
        let mut receivers = Receivers::new(status, commands);
        let peer = DeviceId::new();
        let session = SessionId::new();
        let stream = StreamId::new();
        let (sender, _frames) = mpsc::sync_channel(1);
        sender.send(bytes::Bytes::from_static(b"queued")).unwrap();
        receivers.owned.push_back(session);
        receivers.sinks.insert(
            session,
            Sink {
                peer,
                service: ServiceKind::Camera,
                sender,
                last_keyframe: None,
            },
        );
        let frame = TyuEvent::MediaFrame {
            peer,
            session,
            stream,
            data: bytes::Bytes::from_static(b"overflow"),
        };
        assert!(receivers.event(&frame));
        assert!(matches!(
            rx.try_recv().unwrap(),
            TyuCommand::Request {
                frame: Frame {
                    message: Message::MediaKeyframeRequest { .. },
                    ..
                },
                ..
            }
        ));
        assert!(receivers.event(&frame));
        assert!(rx.try_recv().is_err());
        receivers.sinks.remove(&session);
        assert!(receivers.event(&frame));
        assert!(!receivers.event(&TyuEvent::MediaFrame {
            peer,
            session: SessionId::new(),
            stream,
            data: bytes::Bytes::new()
        }));
    }
}
