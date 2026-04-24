//! Edge record: encoding, decoding, and in-memory representation.
//!
//! Wire format (FORMAT.md §3.2):
//! ```text
//! version: u8 (=1)
//! flags:   u8 (bit0=has_valid_to, bit1=has_supersedes, bit2=has_props)
//! from: u64 BE
//! to: u64 BE
//! type_id: u32 BE
//! valid_from: i64 (sortable)
//! [valid_to: i64 (sortable)]   — if has_valid_to
//! observed_at: i64 (sortable)
//! source_id: u32 BE
//! confidence: f32 BE
//! [supersedes: u64 BE]         — if has_supersedes
//! props_len: u32 BE
//! props: CBOR bytes
//! ```

use crate::codec::{ByteReader, ByteWriter};
use crate::error::{Error, Result};
use crate::id::{EdgeId, NodeId, StringId};
use crate::props::{self, Props};
use crate::time::Ts;

const EDGE_RECORD_VERSION: u8 = 1;

mod flags {
    pub const HAS_VALID_TO: u8 = 1 << 0;
    pub const HAS_SUPERSEDES: u8 = 1 << 1;
    pub const HAS_PROPS: u8 = 1 << 2;
    pub const RESERVED_MASK: u8 = !(HAS_VALID_TO | HAS_SUPERSEDES | HAS_PROPS);
}

/// In-memory representation of an edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub type_id: StringId,
    pub valid_from: Ts,
    pub valid_to: Option<Ts>,
    pub observed_at: Ts,
    pub source_id: StringId,
    pub confidence: f32,
    pub supersedes: Option<EdgeId>,
    pub props: Props,
}

impl Edge {
    /// Is this edge "live" at time `t`?
    #[inline]
    pub fn is_live_at(&self, t: Ts) -> bool {
        if self.valid_from.raw() > t.raw() {
            return false;
        }
        match self.valid_to {
            None => true,
            Some(end) => t.raw() < end.raw(),
        }
    }

    /// Validate confidence and value-range constraints.
    pub fn validate(&self) -> Result<()> {
        if !self.confidence.is_finite() || self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(Error::Schema(format!(
                "confidence must be in [0, 1]; got {}",
                self.confidence
            )));
        }
        if let Some(vt) = self.valid_to {
            if vt.raw() < self.valid_from.raw() {
                return Err(Error::Schema("valid_to must be >= valid_from".into()));
            }
        }
        if self.from == NodeId::ZERO || self.to == NodeId::ZERO {
            return Err(Error::Schema("edge endpoints must be non-zero".into()));
        }
        Ok(())
    }

    /// Serialize to on-disk bytes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let prop_bytes = props::encode(&self.props)?;

        let mut w = ByteWriter::with_capacity(64 + prop_bytes.len());
        w.write_u8(EDGE_RECORD_VERSION);

        let mut flag_byte = 0u8;
        if self.valid_to.is_some() {
            flag_byte |= flags::HAS_VALID_TO;
        }
        if self.supersedes.is_some() {
            flag_byte |= flags::HAS_SUPERSEDES;
        }
        if !prop_bytes.is_empty() {
            flag_byte |= flags::HAS_PROPS;
        }
        w.write_u8(flag_byte);

        w.write_u64(self.from.raw());
        w.write_u64(self.to.raw());
        w.write_u32(self.type_id.raw());
        w.write_i64_sortable(self.valid_from.raw());
        if let Some(vt) = self.valid_to {
            w.write_i64_sortable(vt.raw());
        }
        w.write_i64_sortable(self.observed_at.raw());
        w.write_u32(self.source_id.raw());
        w.write_f32(self.confidence);
        if let Some(sup) = self.supersedes {
            w.write_u64(sup.raw());
        }
        w.write_len_prefixed(&prop_bytes);

        Ok(w.finish())
    }

    /// Deserialize from on-disk bytes. `id` comes from the table key.
    pub fn decode(id: EdgeId, bytes: &[u8]) -> Result<Self> {
        let mut r = ByteReader::new(bytes);

        let version = r.read_u8()?;
        if version != EDGE_RECORD_VERSION {
            return Err(Error::Format(format!(
                "unsupported edge record version: {}",
                version
            )));
        }

        let flag_byte = r.read_u8()?;
        if flag_byte & flags::RESERVED_MASK != 0 {
            return Err(Error::Format(format!(
                "reserved flag bits set on edge: 0x{:02x}",
                flag_byte
            )));
        }

        let from = NodeId::from_raw(r.read_u64()?);
        let to = NodeId::from_raw(r.read_u64()?);
        let type_id = StringId::from_raw(r.read_u32()?);

        let valid_from = Ts::from_nanos(r.read_i64_sortable()?);
        let valid_to = if flag_byte & flags::HAS_VALID_TO != 0 {
            Some(Ts::from_nanos(r.read_i64_sortable()?))
        } else {
            None
        };
        let observed_at = Ts::from_nanos(r.read_i64_sortable()?);
        let source_id = StringId::from_raw(r.read_u32()?);
        let confidence = r.read_f32()?;

        let supersedes = if flag_byte & flags::HAS_SUPERSEDES != 0 {
            Some(EdgeId::from_raw(r.read_u64()?))
        } else {
            None
        };

        let prop_bytes = r.read_len_prefixed()?;
        let has_props_flag = flag_byte & flags::HAS_PROPS != 0;
        if has_props_flag == prop_bytes.is_empty() {
            return Err(Error::Format(format!(
                "edge has_props flag disagrees with props_len (flag={}, len={})",
                has_props_flag,
                prop_bytes.len()
            )));
        }
        let props = props::decode(prop_bytes)?;

        Ok(Edge {
            id,
            from,
            to,
            type_id,
            valid_from,
            valid_to,
            observed_at,
            source_id,
            confidence,
            supersedes,
            props,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_edge(id: u64) -> Edge {
        Edge {
            id: EdgeId::from_raw(id),
            from: NodeId::from_raw(1),
            to: NodeId::from_raw(2),
            type_id: StringId::from_raw(1),
            valid_from: Ts::from_nanos(1_000),
            valid_to: None,
            observed_at: Ts::from_nanos(2_000),
            source_id: StringId::from_raw(2),
            confidence: 0.9,
            supersedes: None,
            props: Props::new(),
        }
    }

    #[test]
    fn roundtrip_minimal() {
        let e = sample_edge(10);
        let b = e.encode().unwrap();
        assert_eq!(Edge::decode(e.id, &b).unwrap(), e);
    }

    #[test]
    fn roundtrip_with_optionals() {
        let mut e = sample_edge(11);
        e.valid_to = Some(Ts::from_nanos(5_000));
        e.supersedes = Some(EdgeId::from_raw(9));
        let b = e.encode().unwrap();
        assert_eq!(Edge::decode(e.id, &b).unwrap(), e);
    }

    #[test]
    fn confidence_out_of_range_rejected() {
        let mut e = sample_edge(12);
        e.confidence = 1.5;
        assert!(e.validate().is_err());
        assert!(e.encode().is_err());
    }

    #[test]
    fn nan_confidence_rejected() {
        let mut e = sample_edge(13);
        e.confidence = f32::NAN;
        assert!(e.encode().is_err());
    }

    #[test]
    fn valid_to_before_from_rejected() {
        let mut e = sample_edge(14);
        e.valid_from = Ts::from_nanos(100);
        e.valid_to = Some(Ts::from_nanos(50));
        assert!(e.encode().is_err());
    }

    #[test]
    fn is_live_at_semantics() {
        let mut e = sample_edge(15);
        e.valid_from = Ts::from_nanos(100);
        e.valid_to = Some(Ts::from_nanos(200));
        assert!(!e.is_live_at(Ts::from_nanos(50)));
        assert!(e.is_live_at(Ts::from_nanos(100)));
        assert!(e.is_live_at(Ts::from_nanos(150)));
        assert!(!e.is_live_at(Ts::from_nanos(200))); // exclusive upper
        assert!(!e.is_live_at(Ts::from_nanos(300)));
    }

    #[test]
    fn is_live_at_open_ended() {
        let mut e = sample_edge(16);
        e.valid_from = Ts::from_nanos(100);
        e.valid_to = None;
        assert!(e.is_live_at(Ts::from_nanos(99_999_999)));
        assert!(!e.is_live_at(Ts::from_nanos(50)));
    }
}
