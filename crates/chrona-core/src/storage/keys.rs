//! Composite key encoding for redb tables.
//!
//! redb supports native `u64` keys but cannot compose them without a wrapping
//! byte type. We use `&[u8]` keys and build them with these helpers. All
//! multi-byte integers are big-endian; timestamps are sign-bit flipped so that
//! lexicographic byte order matches numeric order across the full i64 range.

use crate::id::{EdgeId, NodeId};
use crate::time::Ts;

/// Encode a u64 as 8 big-endian bytes. Used for standalone node and edge keys.
#[inline]
pub fn u64_be(v: u64) -> [u8; 8] {
    v.to_be_bytes()
}

/// Encode a `NodeId` as 8 big-endian bytes.
#[inline]
pub fn node_key(n: NodeId) -> [u8; 8] {
    u64_be(n.raw())
}

/// Encode an `EdgeId` as 8 big-endian bytes.
#[inline]
pub fn edge_key(e: EdgeId) -> [u8; 8] {
    u64_be(e.raw())
}

/// Encode an adjacency key: `(node, valid_from, edge_id)` — 24 bytes.
///
/// Used for both `fwd_adj` (keyed by `from`) and `rev_adj` (keyed by `to`).
/// Range-scanning on `(node, Ts::MIN, 0)..(node, T, u64::MAX)` returns every
/// edge whose `valid_from <= T`.
#[inline]
pub fn adj_key(node: NodeId, valid_from: Ts, edge: EdgeId) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[0..8].copy_from_slice(&node_key(node));
    out[8..16].copy_from_slice(&valid_from.to_sortable_bytes());
    out[16..24].copy_from_slice(&edge_key(edge));
    out
}

/// Prefix key for range-scanning all edges of a node: `(node, Ts::MIN, 0)`.
#[inline]
pub fn adj_prefix_min(node: NodeId) -> [u8; 24] {
    adj_key(node, Ts::MIN, EdgeId::ZERO)
}

/// Upper-bound key for range-scanning edges of a node with `valid_from <= t`.
#[inline]
pub fn adj_upper_for_ts(node: NodeId, t: Ts) -> [u8; 24] {
    adj_key(node, t, EdgeId::from_raw(u64::MAX))
}

/// Upper-bound key for all edges of a node: `(node, Ts::MAX, u64::MAX)`.
#[inline]
pub fn adj_prefix_max(node: NodeId) -> [u8; 24] {
    adj_key(node, Ts::MAX, EdgeId::from_raw(u64::MAX))
}

/// Event key: `(ts, event_id)` — 16 bytes.
#[inline]
pub fn event_key(ts: Ts, id: u64) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&ts.to_sortable_bytes());
    out[8..16].copy_from_slice(&u64_be(id));
    out
}

/// Event range lower bound: `(t, 0)`.
#[inline]
pub fn event_lower(t: Ts) -> [u8; 16] {
    event_key(t, 0)
}

/// Event range upper bound: `(t, u64::MAX)`.
#[inline]
pub fn event_upper(t: Ts) -> [u8; 16] {
    event_key(t, u64::MAX)
}

/// Temporal index key: `(valid_from, edge_id)` — 16 bytes.
#[inline]
pub fn temporal_idx_key(valid_from: Ts, edge: EdgeId) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&valid_from.to_sortable_bytes());
    out[8..16].copy_from_slice(&edge_key(edge));
    out
}

/// Supersedes index key: `(old, new)` — 16 bytes.
#[inline]
pub fn supersedes_key(old: EdgeId, new: EdgeId) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&edge_key(old));
    out[8..16].copy_from_slice(&edge_key(new));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adj_range_is_total_per_node() {
        let n = NodeId::from_raw(42);
        let low = adj_prefix_min(n);
        let high = adj_prefix_max(n);
        assert!(low < high);
        // All keys for this node should sit inside [low, high].
        let inner = adj_key(n, Ts(0), EdgeId::from_raw(100));
        assert!(inner > low && inner < high);
        // A different node's keys should NOT collide.
        let other = adj_key(NodeId::from_raw(43), Ts::MIN, EdgeId::ZERO);
        assert!(other > high);
    }

    #[test]
    fn event_range_sorts_by_time_then_id() {
        let a = event_key(Ts(1_000), 5);
        let b = event_key(Ts(1_000), 6);
        let c = event_key(Ts(2_000), 0);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn adj_sorts_by_time_within_node() {
        let n = NodeId::from_raw(1);
        let e = EdgeId::from_raw(100);
        let t1 = adj_key(n, Ts(100), e);
        let t2 = adj_key(n, Ts(200), e);
        let t3 = adj_key(n, Ts(300), e);
        assert!(t1 < t2);
        assert!(t2 < t3);
    }

    #[test]
    fn negative_ts_sorts_correctly() {
        let n = NodeId::from_raw(1);
        let e = EdgeId::from_raw(100);
        let neg = adj_key(n, Ts(-100), e);
        let zero = adj_key(n, Ts(0), e);
        let pos = adj_key(n, Ts(100), e);
        assert!(neg < zero);
        assert!(zero < pos);
    }
}
