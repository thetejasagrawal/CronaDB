//! User-defined properties on nodes and edges.
//!
//! Properties are a map from string keys to arbitrary CBOR-encoded values.
//! The core engine treats property bytes as opaque; higher layers (query,
//! bindings) may interpret them.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A value that can be stored as a property.
///
/// We support a small set of canonical types and fall back to a `Bytes`
/// variant for pre-encoded CBOR blobs. BTreeMap preserves lexicographic key
/// order, which is what we need for canonical CBOR output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
}

/// Map of property keys to values.
pub type Props = BTreeMap<String, PropValue>;

/// Encode a property map as canonical CBOR.
///
/// Returns an empty vec for an empty map; downstream code checks for this
/// and sets a "no properties" flag.
pub fn encode(props: &Props) -> Result<Vec<u8>> {
    if props.is_empty() {
        return Ok(Vec::new());
    }
    let mut buf = Vec::with_capacity(64);
    ciborium::ser::into_writer(props, &mut buf)
        .map_err(|e| Error::Internal(format!("cbor encode failed: {}", e)))?;
    Ok(buf)
}

/// Decode canonical CBOR bytes back into a property map.
pub fn decode(bytes: &[u8]) -> Result<Props> {
    if bytes.is_empty() {
        return Ok(Props::new());
    }
    ciborium::de::from_reader(bytes)
        .map_err(|e| Error::Format(format!("cbor decode failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_encodes_to_empty() {
        let p = Props::new();
        assert!(encode(&p).unwrap().is_empty());
    }

    #[test]
    fn roundtrip_various() {
        let mut p = Props::new();
        p.insert("name".into(), PropValue::String("alice".into()));
        p.insert("age".into(), PropValue::Int(30));
        p.insert("active".into(), PropValue::Bool(true));
        p.insert("score".into(), PropValue::Float(0.95));
        p.insert("nothing".into(), PropValue::Null);

        let bytes = encode(&p).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(p, decoded);
    }

    #[test]
    fn decode_empty_slice() {
        let p = decode(&[]).unwrap();
        assert!(p.is_empty());
    }

    #[test]
    fn deterministic_encoding() {
        // BTreeMap iteration order makes encoding deterministic across runs.
        let mut p1 = Props::new();
        p1.insert("b".into(), PropValue::Int(2));
        p1.insert("a".into(), PropValue::Int(1));

        let mut p2 = Props::new();
        p2.insert("a".into(), PropValue::Int(1));
        p2.insert("b".into(), PropValue::Int(2));

        assert_eq!(encode(&p1).unwrap(), encode(&p2).unwrap());
    }
}
