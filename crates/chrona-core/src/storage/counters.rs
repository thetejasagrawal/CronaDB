//! Monotonic id counters stored in the `meta` table.
//!
//! Each counter is stored as 8 big-endian bytes (u64) or 4 big-endian bytes
//! (u32 for `string_id_counter`). Each `next_*` function atomically increments
//! its counter within the given write transaction.

use crate::error::{Error, Result};
use crate::id::{EdgeId, EventId, NodeId, StringId};
use crate::storage::tables::{self, meta_keys};
use redb::{ReadableTable, WriteTransaction};

fn read_u64(txn: &WriteTransaction, key: &[u8]) -> Result<u64> {
    let table = txn.open_table(tables::META)?;
    let got = table.get(key)?;
    let out = match got {
        Some(v) => {
            let b = v.value();
            if b.len() != 8 {
                return Err(Error::Format(format!(
                    "meta key {:?} has unexpected length {}",
                    std::str::from_utf8(key).unwrap_or("?"),
                    b.len()
                )));
            }
            u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        }
        None => 0,
    };
    Ok(out)
}

fn write_u64(txn: &WriteTransaction, key: &[u8], v: u64) -> Result<()> {
    let mut table = txn.open_table(tables::META)?;
    table.insert(key, v.to_be_bytes().as_slice())?;
    Ok(())
}

fn read_u32(txn: &WriteTransaction, key: &[u8]) -> Result<u32> {
    let table = txn.open_table(tables::META)?;
    let got = table.get(key)?;
    let out = match got {
        Some(v) => {
            let b = v.value();
            if b.len() != 4 {
                return Err(Error::Format(format!(
                    "meta key {:?} has unexpected length {}",
                    std::str::from_utf8(key).unwrap_or("?"),
                    b.len()
                )));
            }
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
        None => 0,
    };
    Ok(out)
}

fn write_u32(txn: &WriteTransaction, key: &[u8], v: u32) -> Result<()> {
    let mut table = txn.open_table(tables::META)?;
    table.insert(key, v.to_be_bytes().as_slice())?;
    Ok(())
}

/// Allocate the next `NodeId`.
pub fn next_node_id(txn: &WriteTransaction) -> Result<NodeId> {
    let current = read_u64(txn, meta_keys::NODE_ID_COUNTER)?;
    let next = current + 1;
    write_u64(txn, meta_keys::NODE_ID_COUNTER, next)?;
    Ok(NodeId::from_raw(next))
}

/// Allocate the next `EdgeId`.
pub fn next_edge_id(txn: &WriteTransaction) -> Result<EdgeId> {
    let current = read_u64(txn, meta_keys::EDGE_ID_COUNTER)?;
    let next = current + 1;
    write_u64(txn, meta_keys::EDGE_ID_COUNTER, next)?;
    Ok(EdgeId::from_raw(next))
}

/// Allocate the next `EventId`.
pub fn next_event_id(txn: &WriteTransaction) -> Result<EventId> {
    let current = read_u64(txn, meta_keys::EVENT_ID_COUNTER)?;
    let next = current + 1;
    write_u64(txn, meta_keys::EVENT_ID_COUNTER, next)?;
    Ok(EventId::from_raw(next))
}

/// Allocate the next `StringId`.
pub fn next_string_id(txn: &WriteTransaction) -> Result<StringId> {
    let current = read_u32(txn, meta_keys::STRING_ID_COUNTER)?;
    let next = current + 1;
    write_u32(txn, meta_keys::STRING_ID_COUNTER, next)?;
    Ok(StringId::from_raw(next))
}
