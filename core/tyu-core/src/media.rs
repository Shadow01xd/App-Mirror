//! Independent video/audio datagrams. Bounded incomplete frames and expiry.
use crate::{Result, TyuErrorCode};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};
use tyu_types::{SessionId, StreamId};
pub const MAX_MEDIA_FRAME: usize = 4 * 1024 * 1024;
pub const MAX_PACKETS: usize = 8192;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaPacket {
    pub session_id: SessionId,
    pub stream_id: StreamId,
    pub frame_id: u64,
    pub packet_index: u16,
    pub packet_count: u16,
    pub timestamp: u64,
    pub flags: u8,
    pub keyframe: bool,
    pub payload: Bytes,
}
impl MediaPacket {
    pub fn encode(&self) -> Result<Bytes> {
        self.validate()?;
        Ok(postcard::to_allocvec(self)
            .map_err(|_| TyuErrorCode::Malformed)?
            .into())
    }
    pub fn decode(bytes: Bytes) -> Result<Self> {
        if bytes.len() > 65535 {
            return Err(TyuErrorCode::Limit.into());
        }
        let (packet, rest): (Self, &[u8]) =
            postcard::take_from_bytes(&bytes).map_err(|_| TyuErrorCode::Malformed)?;
        if !rest.is_empty() {
            return Err(TyuErrorCode::Malformed.into());
        }
        packet.validate()?;
        Ok(packet)
    }
    fn validate(&self) -> Result<()> {
        if self.packet_count == 0
            || self.packet_count as usize > MAX_PACKETS
            || self.packet_index >= self.packet_count
            || self.payload.len() > 64000
        {
            return Err(TyuErrorCode::Malformed.into());
        }
        Ok(())
    }
}
pub fn packetize(
    session: SessionId,
    stream: StreamId,
    frame: u64,
    timestamp: u64,
    keyframe: bool,
    data: Bytes,
    mtu: usize,
) -> Result<Vec<MediaPacket>> {
    if !(256..=65535).contains(&mtu) || data.len() > MAX_MEDIA_FRAME || data.is_empty() {
        return Err(TyuErrorCode::Limit.into());
    }
    let chunk = mtu - 128;
    let count = data.len().div_ceil(chunk);
    if count > MAX_PACKETS {
        return Err(TyuErrorCode::Limit.into());
    }
    Ok((0..count)
        .map(|index| MediaPacket {
            session_id: session,
            stream_id: stream,
            frame_id: frame,
            packet_index: index as u16,
            packet_count: count as u16,
            timestamp,
            flags: 0,
            keyframe,
            payload: data.slice(index * chunk..((index + 1) * chunk).min(data.len())),
        })
        .collect())
}
struct Pending {
    first: Instant,
    header: MediaPacket,
    parts: Vec<Option<Bytes>>,
    bytes: usize,
    received: usize,
}
#[derive(Debug, Default, Clone)]
pub struct MediaStats {
    pub completed: u64,
    pub dropped: u64,
    pub duplicates: u64,
    pub missing_packets: u64,
    pub assembly_micros: u64,
}
pub struct Reassembler {
    pending: BTreeMap<(SessionId, StreamId, u64), Pending>,
    last: HashMap<(SessionId, StreamId), u64>,
    pub stats: MediaStats,
    max_age: Duration,
}
impl Reassembler {
    pub fn new(max_age: Duration) -> Self {
        Self {
            pending: BTreeMap::new(),
            last: HashMap::new(),
            stats: MediaStats::default(),
            max_age,
        }
    }
    pub fn expire(&mut self, now: Instant) -> Vec<(SessionId, StreamId)> {
        let mut requests = Vec::new();
        self.pending.retain(|(s, t, _), p| {
            if now.saturating_duration_since(p.first) >= self.max_age {
                self.stats.dropped += 1;
                self.stats.missing_packets += (p.parts.len() - p.received) as u64;
                requests.push((*s, *t));
                false
            } else {
                true
            }
        });
        requests
    }
    pub fn push(&mut self, p: MediaPacket, now: Instant) -> Result<Option<Bytes>> {
        p.validate()?;
        let stream = (p.session_id, p.stream_id);
        let key = (p.session_id, p.stream_id, p.frame_id);
        if self
            .last
            .get(&stream)
            .is_some_and(|last| p.frame_id <= *last)
        {
            self.stats.dropped += 1;
            return Ok(None);
        }
        if self.last.len() >= 64 && !self.last.contains_key(&stream) {
            return Err(TyuErrorCode::Limit.into());
        }
        if !self.pending.contains_key(&key) && self.pending.len() >= 8 {
            return Err(TyuErrorCode::Limit.into());
        }
        let entry = self.pending.entry(key).or_insert_with(|| Pending {
            first: now,
            header: p.clone(),
            parts: vec![None; p.packet_count as usize],
            bytes: 0,
            received: 0,
        });
        if entry.parts.len() != p.packet_count as usize
            || entry.header.timestamp != p.timestamp
            || entry.header.keyframe != p.keyframe
            || entry.header.flags != p.flags
        {
            return Err(TyuErrorCode::Malformed.into());
        }
        if entry.parts[p.packet_index as usize].is_some() {
            self.stats.duplicates += 1;
            return Ok(None);
        }
        entry.bytes += p.payload.len();
        if entry.bytes > MAX_MEDIA_FRAME {
            self.pending.remove(&key);
            return Err(TyuErrorCode::Limit.into());
        }
        entry.parts[p.packet_index as usize] = Some(p.payload);
        entry.received += 1;
        if entry.received != entry.parts.len() {
            return Ok(None);
        }
        let entry = self
            .pending
            .remove(&key)
            .ok_or(TyuErrorCode::InvalidState)?;
        let mut frame = Vec::with_capacity(entry.bytes);
        for part in entry.parts {
            frame.extend_from_slice(&part.ok_or(TyuErrorCode::Malformed)?);
        }
        self.last.insert(stream, p.frame_id);
        self.stats.completed += 1;
        self.stats.assembly_micros = now.saturating_duration_since(entry.first).as_micros() as u64;
        self.pending
            .retain(|(s, t, f), _| (*s, *t) != stream || *f > p.frame_id);
        Ok(Some(frame.into()))
    }
    pub fn remove_stream(&mut self, session: SessionId, stream: StreamId) {
        self.last.remove(&(session, stream));
        self.pending
            .retain(|(s, t, _), _| (*s, *t) != (session, stream));
    }
}
