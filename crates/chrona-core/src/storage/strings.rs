//! Bidirectional string interner backed by two redb tables.
//!
//! String ids are `u32` and are assigned monotonically. They are never reused
//! in format v1. Two tables maintain consistency: `strings_fwd` maps
//! `bytes → id`, and `strings_rev` maps `id → bytes`.

use crate::error::Result;
use crate::id::StringId;
use crate::storage::tables;
use redb::{ReadTransaction, ReadableTable, WriteTransaction};

/// Read-side lookup: UTF-8 bytes → StringId.
pub fn lookup(txn: &ReadTransaction, s: &[u8]) -> Result<Option<StringId>> {
    let table = match txn.open_table(tables::STRINGS_FWD) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let got = table.get(s)?;
    let out = got.map(|v| StringId::from_raw(v.value()));
    Ok(out)
}

/// Read-side reverse lookup: StringId → UTF-8 bytes.
pub fn resolve(txn: &ReadTransaction, id: StringId) -> Result<Option<Vec<u8>>> {
    let table = match txn.open_table(tables::STRINGS_REV) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let got = table.get(id.raw())?;
    let out = got.map(|v| v.value().to_vec());
    Ok(out)
}

/// Intern a string within a write transaction, returning its id.
///
/// If the string already exists, returns the existing id. Otherwise allocates
/// a new id by incrementing the counter in the meta table, inserts both
/// tables, and returns the new id.
pub fn intern(txn: &WriteTransaction, s: &[u8]) -> Result<StringId> {
    // Fast path: is it already there?
    {
        let fwd = txn.open_table(tables::STRINGS_FWD)?;
        let got = fwd.get(s)?;
        if let Some(v) = got {
            return Ok(StringId::from_raw(v.value()));
        }
    }

    // Allocate a new id.
    let next = super::counters::next_string_id(txn)?;

    // Insert into both tables.
    {
        let mut fwd = txn.open_table(tables::STRINGS_FWD)?;
        fwd.insert(s, next.raw())?;
    }
    {
        let mut rev = txn.open_table(tables::STRINGS_REV)?;
        rev.insert(next.raw(), s)?;
    }

    Ok(next)
}

/// Read-side resolve StringId to a String (UTF-8-validated).
pub fn resolve_string(txn: &ReadTransaction, id: StringId) -> Result<Option<String>> {
    let Some(bytes) = resolve(txn, id)? else {
        return Ok(None);
    };
    Ok(Some(String::from_utf8(bytes).map_err(|e| {
        crate::error::Error::Format(format!("interned string is not UTF-8: {}", e))
    })?))
}
