//! redb table definitions for Chrona.
//!
//! Every logical table from FORMAT.md §4 maps to exactly one redb table
//! here. Table name strings are prefixed with `chrona_` to leave room for
//! future non-chrona tables in the same file (should we ever share the file
//! with other consumers — unlikely but cheap to future-proof).

use redb::TableDefinition;

/// `nodes`: `NodeId (u64 BE)` → `NodeRecord bytes`
pub const NODES: TableDefinition<'static, u64, &[u8]> = TableDefinition::new("chrona_nodes");

/// `edges`: `EdgeId (u64 BE)` → `EdgeRecord bytes`
pub const EDGES: TableDefinition<'static, u64, &[u8]> = TableDefinition::new("chrona_edges");

/// `fwd_adj`: `(NodeId, Ts, EdgeId)` as 24-byte composite key → `NodeId (u64 BE)` as value.
pub const FWD_ADJ: TableDefinition<'static, &[u8], u64> = TableDefinition::new("chrona_fwd_adj");

/// `rev_adj`: symmetric to `fwd_adj` but keyed by the target node.
pub const REV_ADJ: TableDefinition<'static, &[u8], u64> = TableDefinition::new("chrona_rev_adj");

/// `events`: `(Ts, EventId)` composite key → `EventRecord bytes`.
pub const EVENTS: TableDefinition<'static, &[u8], &[u8]> = TableDefinition::new("chrona_events");

/// `temporal_idx`: `(Ts valid_from, EdgeId)` composite key → empty value.
pub const TEMPORAL_IDX: TableDefinition<'static, &[u8], &[u8]> =
    TableDefinition::new("chrona_temporal_idx");

/// `supersedes_idx`: `(old EdgeId, new EdgeId)` → empty value.
pub const SUPERSEDES_IDX: TableDefinition<'static, &[u8], &[u8]> =
    TableDefinition::new("chrona_supersedes_idx");

/// `strings_fwd`: UTF-8 bytes → StringId (u32 BE)
pub const STRINGS_FWD: TableDefinition<'static, &[u8], u32> =
    TableDefinition::new("chrona_strings_fwd");

/// `strings_rev`: StringId (u32 BE) → UTF-8 bytes
pub const STRINGS_REV: TableDefinition<'static, u32, &[u8]> =
    TableDefinition::new("chrona_strings_rev");

/// `ext_ids`: UTF-8 ext_id → NodeId (u64 BE)
pub const EXT_IDS: TableDefinition<'static, &[u8], u64> = TableDefinition::new("chrona_ext_ids");

/// `meta`: UTF-8 key → bytes.
pub const META: TableDefinition<'static, &[u8], &[u8]> = TableDefinition::new("chrona_meta");

// Meta keys.
pub mod meta_keys {
    pub const MAGIC: &[u8] = b"chrona.magic";
    pub const FORMAT_VERSION: &[u8] = b"chrona.format_version";
    pub const WRITER_VERSION: &[u8] = b"chrona.writer_version";
    pub const CREATED_AT: &[u8] = b"chrona.created_at";
    pub const REQUIRED_FEATURES: &[u8] = b"chrona.required_features";
    pub const OPTIONAL_FEATURES: &[u8] = b"chrona.optional_features";
    pub const NODE_ID_COUNTER: &[u8] = b"chrona.node_id_counter";
    pub const EDGE_ID_COUNTER: &[u8] = b"chrona.edge_id_counter";
    pub const EVENT_ID_COUNTER: &[u8] = b"chrona.event_id_counter";
    pub const STRING_ID_COUNTER: &[u8] = b"chrona.string_id_counter";
}

/// Current file format version.
pub const FORMAT_VERSION: u16 = 1;

/// Magic identifying a Chrona database.
pub const MAGIC: &[u8] = b"CHRN";

/// Required feature bitmap for files written by this writer.
pub const REQUIRED_FEATURES: u64 = 0b1111; // bits 0..=3 per FORMAT.md §6.2

/// Optional feature bitmap (currently empty).
pub const OPTIONAL_FEATURES: u64 = 0;
