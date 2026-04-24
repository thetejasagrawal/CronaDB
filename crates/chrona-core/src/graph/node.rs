//! Node record: encoding, decoding, and in-memory representation.
//!
//! Wire format (FORMAT.md §3.1):
//! ```text
//! version: u8 (=1)
//! flags:   u8 (bit0=has_type)
//! created_at: i64 (sortable)
//! type_id: u32 BE                 (present unless flag cleared)
//! ext_id_len: u32 BE
//! ext_id: UTF-8 bytes
//! props_len: u32 BE
//! props: CBOR bytes
//! ```

use crate::codec::{ByteReader, ByteWriter};
use crate::error::{Error, Result};
use crate::id::{NodeId, StringId};
use crate::props::{self, Props};
use crate::time::Ts;

const NODE_RECORD_VERSION: u8 = 1;

/// Flag bits for `NodeRecord`.
mod flags {
    pub const HAS_TYPE: u8 = 1 << 0;
    pub const RESERVED_MASK: u8 = !HAS_TYPE;
}

/// In-memory representation of a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub ext_id: String,
    pub type_id: Option<StringId>,
    pub created_at: Ts,
    pub props: Props,
}

impl Node {
    /// Serialize to the on-disk byte format.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut w = ByteWriter::with_capacity(64 + self.ext_id.len());
        w.write_u8(NODE_RECORD_VERSION);

        let mut flag_byte = 0u8;
        if self.type_id.is_some() {
            flag_byte |= flags::HAS_TYPE;
        }
        w.write_u8(flag_byte);

        w.write_i64_sortable(self.created_at.raw());
        w.write_u32(self.type_id.map(|s| s.raw()).unwrap_or(0));
        w.write_len_prefixed(self.ext_id.as_bytes());

        let prop_bytes = props::encode(&self.props)?;
        w.write_len_prefixed(&prop_bytes);

        Ok(w.finish())
    }

    /// Deserialize from on-disk bytes. The `id` is supplied externally because
    /// the byte format does not carry it (it is the table key).
    pub fn decode(id: NodeId, bytes: &[u8]) -> Result<Self> {
        let mut r = ByteReader::new(bytes);

        let version = r.read_u8()?;
        if version != NODE_RECORD_VERSION {
            return Err(Error::Format(format!(
                "unsupported node record version: {}",
                version
            )));
        }

        let flag_byte = r.read_u8()?;
        if flag_byte & flags::RESERVED_MASK != 0 {
            return Err(Error::Format(format!(
                "reserved flag bits set on node: 0x{:02x}",
                flag_byte
            )));
        }

        let created_at = Ts::from_nanos(r.read_i64_sortable()?);
        let raw_type = r.read_u32()?;
        let type_id = if flag_byte & flags::HAS_TYPE != 0 {
            Some(StringId::from_raw(raw_type))
        } else {
            None
        };
        let ext_id_bytes = r.read_len_prefixed()?;
        let ext_id = String::from_utf8(ext_id_bytes.to_vec())
            .map_err(|e| Error::Format(format!("ext_id is not UTF-8: {}", e)))?;

        let prop_bytes = r.read_len_prefixed()?;
        let props = props::decode(prop_bytes)?;

        Ok(Node {
            id,
            ext_id,
            type_id,
            created_at,
            props,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::PropValue;

    #[test]
    fn roundtrip_untyped() {
        let n = Node {
            id: NodeId::from_raw(1),
            ext_id: "alice".into(),
            type_id: None,
            created_at: Ts::from_nanos(1_700_000_000_000_000_000),
            props: Props::new(),
        };
        let bytes = n.encode().unwrap();
        let back = Node::decode(n.id, &bytes).unwrap();
        assert_eq!(n, back);
    }

    #[test]
    fn roundtrip_typed_with_props() {
        let mut props = Props::new();
        props.insert("role".into(), PropValue::String("founder".into()));
        let n = Node {
            id: NodeId::from_raw(2),
            ext_id: "bob".into(),
            type_id: Some(StringId::from_raw(42)),
            created_at: Ts::from_nanos(1_700_000_000_000_000_000),
            props,
        };
        let bytes = n.encode().unwrap();
        let back = Node::decode(n.id, &bytes).unwrap();
        assert_eq!(n, back);
    }

    #[test]
    fn decode_truncated_errors() {
        let mut bytes = vec![1u8, 0]; // version + flags only
                                      // missing created_at, type_id, ext_id_len, props_len
        let err = Node::decode(NodeId::from_raw(1), &bytes);
        assert!(err.is_err());
        bytes.extend_from_slice(&[0u8; 8]); // created_at
        bytes.extend_from_slice(&[0u8; 4]); // type_id
                                            // still missing ext_id_len
        assert!(Node::decode(NodeId::from_raw(1), &bytes).is_err());
    }

    #[test]
    fn reject_unknown_version() {
        let mut bytes = vec![99u8, 0]; // bad version
        bytes.extend_from_slice(&[0u8; 16]);
        assert!(Node::decode(NodeId::from_raw(1), &bytes).is_err());
    }
}
