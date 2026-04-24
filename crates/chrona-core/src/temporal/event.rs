//! Event record: the canonical source-of-truth log.
//!
//! Wire format (FORMAT.md §3.3–§3.4):
//! ```text
//! version: u8 (=1)
//! flags:   u8 (reserved, =0)
//! kind:    u8
//! payload_len: u32 BE
//! payload: bytes
//! ```

use crate::codec::{ByteReader, ByteWriter};
use crate::error::{Error, Result};
use crate::graph::{Edge, Node};
use crate::id::{EdgeId, EventId, NodeId};
use crate::time::Ts;

const EVENT_RECORD_VERSION: u8 = 1;

/// Event kind values. Must remain stable once assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    NodeAdded = 1,
    NodeRemoved = 2,
    EdgeObserved = 3,
    EdgeInvalidated = 4,
    EdgeSuperseded = 5,
    PropertySet = 6,
}

impl EventKind {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => EventKind::NodeAdded,
            2 => EventKind::NodeRemoved,
            3 => EventKind::EdgeObserved,
            4 => EventKind::EdgeInvalidated,
            5 => EventKind::EdgeSuperseded,
            6 => EventKind::PropertySet,
            _ => return None,
        })
    }
}

/// An event as stored in the log: kind + opaque payload bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub id: EventId,
    pub timestamp: Ts,
    pub kind: EventKind,
    pub payload: Vec<u8>,
}

impl EventRecord {
    /// Encode the event value (not the key — the key is `(ts, event_id)`).
    pub fn encode_value(&self) -> Vec<u8> {
        let mut w = ByteWriter::with_capacity(self.payload.len() + 8);
        w.write_u8(EVENT_RECORD_VERSION);
        w.write_u8(0); // flags reserved
        w.write_u8(self.kind as u8);
        w.write_len_prefixed(&self.payload);
        w.finish()
    }

    /// Decode the value bytes, given the id and timestamp from the key.
    pub fn decode_value(id: EventId, timestamp: Ts, bytes: &[u8]) -> Result<Option<Self>> {
        let mut r = ByteReader::new(bytes);
        let version = r.read_u8()?;
        if version != EVENT_RECORD_VERSION {
            return Err(Error::Format(format!(
                "unsupported event record version: {}",
                version
            )));
        }
        let _flags = r.read_u8()?; // reserved
        let kind_byte = r.read_u8()?;
        let payload = r.read_len_prefixed()?.to_vec();

        // Unknown kinds: FORMAT.md §3.4 says readers MUST skip.
        let Some(kind) = EventKind::from_u8(kind_byte) else {
            return Ok(None);
        };

        Ok(Some(EventRecord {
            id,
            timestamp,
            kind,
            payload,
        }))
    }
}

// ---- Payload encoders ----

pub fn payload_node_added(node: &Node) -> Result<Vec<u8>> {
    node.encode()
}

pub fn payload_node_removed(id: NodeId) -> Vec<u8> {
    id.raw().to_be_bytes().to_vec()
}

pub fn payload_edge_observed(edge: &Edge) -> Result<Vec<u8>> {
    edge.encode()
}

pub fn payload_edge_invalidated(id: EdgeId, at: Ts) -> Vec<u8> {
    let mut w = ByteWriter::with_capacity(16);
    w.write_u64(id.raw());
    w.write_i64_sortable(at.raw());
    w.finish()
}

pub fn payload_edge_superseded(old: EdgeId, new: &Edge) -> Result<Vec<u8>> {
    let mut w = ByteWriter::with_capacity(64);
    w.write_u64(old.raw());
    let new_bytes = new.encode()?;
    w.write_bytes(&new_bytes);
    Ok(w.finish())
}

// ---- Payload decoders ----

pub fn decode_node_removed(payload: &[u8]) -> Result<NodeId> {
    if payload.len() != 8 {
        return Err(Error::Format(format!(
            "NodeRemoved payload must be 8 bytes, got {}",
            payload.len()
        )));
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(payload);
    Ok(NodeId::from_raw(u64::from_be_bytes(b)))
}

pub fn decode_edge_invalidated(payload: &[u8]) -> Result<(EdgeId, Ts)> {
    if payload.len() != 16 {
        return Err(Error::Format(format!(
            "EdgeInvalidated payload must be 16 bytes, got {}",
            payload.len()
        )));
    }
    let mut r = ByteReader::new(payload);
    let id = EdgeId::from_raw(r.read_u64()?);
    let at = Ts::from_nanos(r.read_i64_sortable()?);
    Ok((id, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_event_record() {
        let rec = EventRecord {
            id: EventId::from_raw(42),
            timestamp: Ts::from_nanos(1_000),
            kind: EventKind::EdgeInvalidated,
            payload: payload_edge_invalidated(EdgeId::from_raw(7), Ts::from_nanos(500)),
        };
        let bytes = rec.encode_value();
        let back = EventRecord::decode_value(rec.id, rec.timestamp, &bytes)
            .unwrap()
            .unwrap();
        assert_eq!(rec, back);
    }

    #[test]
    fn unknown_kind_skipped() {
        // version=1, flags=0, kind=99 (unknown), payload_len=0
        let bytes = vec![1u8, 0, 99, 0, 0, 0, 0];
        let res = EventRecord::decode_value(EventId::from_raw(1), Ts::EPOCH, &bytes).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn edge_invalidated_payload_roundtrip() {
        let p = payload_edge_invalidated(EdgeId::from_raw(99), Ts::from_nanos(12345));
        let (id, at) = decode_edge_invalidated(&p).unwrap();
        assert_eq!(id.raw(), 99);
        assert_eq!(at.raw(), 12345);
    }

    #[test]
    fn node_removed_payload_roundtrip() {
        let p = payload_node_removed(NodeId::from_raw(5));
        assert_eq!(decode_node_removed(&p).unwrap().raw(), 5);
    }
}
