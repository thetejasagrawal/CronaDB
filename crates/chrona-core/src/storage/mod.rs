//! Storage layer: redb-backed tables, counters, and string interning.
//!
//! The storage layer does not know about nodes, edges, or time — it manipulates
//! bytes in named tables. Higher layers (graph, temporal) build on this.

pub mod counters;
pub mod keys;
pub mod strings;
pub mod tables;

use crate::error::{Error, Result};
use crate::time::Ts;
use redb::Database;
use std::path::Path;

/// Initialize a freshly-created database file: create tables and stamp the
/// metadata that identifies this as a Chrona database.
pub(crate) fn initialize_new(db: &Database) -> Result<()> {
    let txn = db.begin_write()?;

    // Touch every table so it exists in the file.
    let _ = txn.open_table(tables::NODES)?;
    let _ = txn.open_table(tables::EDGES)?;
    let _ = txn.open_table(tables::FWD_ADJ)?;
    let _ = txn.open_table(tables::REV_ADJ)?;
    let _ = txn.open_table(tables::EVENTS)?;
    let _ = txn.open_table(tables::TEMPORAL_IDX)?;
    let _ = txn.open_table(tables::SUPERSEDES_IDX)?;
    let _ = txn.open_table(tables::STRINGS_FWD)?;
    let _ = txn.open_table(tables::STRINGS_REV)?;
    let _ = txn.open_table(tables::EXT_IDS)?;

    // Stamp metadata.
    {
        let mut meta = txn.open_table(tables::META)?;
        meta.insert(tables::meta_keys::MAGIC, tables::MAGIC)?;
        meta.insert(
            tables::meta_keys::FORMAT_VERSION,
            tables::FORMAT_VERSION.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::WRITER_VERSION,
            env!("CARGO_PKG_VERSION").as_bytes(),
        )?;
        meta.insert(
            tables::meta_keys::CREATED_AT,
            Ts::now().to_sortable_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::REQUIRED_FEATURES,
            tables::REQUIRED_FEATURES.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::OPTIONAL_FEATURES,
            tables::OPTIONAL_FEATURES.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::NODE_ID_COUNTER,
            0u64.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::EDGE_ID_COUNTER,
            0u64.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::EVENT_ID_COUNTER,
            0u64.to_be_bytes().as_slice(),
        )?;
        meta.insert(
            tables::meta_keys::STRING_ID_COUNTER,
            0u32.to_be_bytes().as_slice(),
        )?;
    }

    txn.commit()?;
    Ok(())
}

/// Verify that a freshly-opened database is a Chrona database we can read.
///
/// Checks magic, format version, and required-features bitmap.
pub(crate) fn verify_existing(db: &Database) -> Result<()> {
    let txn = db.begin_read()?;
    let meta = match txn.open_table(tables::META) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            return Err(Error::Format(
                "file is not a Chrona database (missing meta table)".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    // Check magic.
    let magic_guard = meta
        .get(tables::meta_keys::MAGIC)?
        .ok_or_else(|| Error::Format("missing chrona.magic".into()))?;
    if magic_guard.value() != tables::MAGIC {
        return Err(Error::Format(format!(
            "bad magic: expected {:?}, got {:?}",
            tables::MAGIC,
            magic_guard.value()
        )));
    }
    drop(magic_guard);

    // Check format version.
    let version_guard = meta
        .get(tables::meta_keys::FORMAT_VERSION)?
        .ok_or_else(|| Error::Format("missing chrona.format_version".into()))?;
    let v = version_guard.value();
    if v.len() != 2 {
        return Err(Error::Format(format!(
            "format_version has bad length: {}",
            v.len()
        )));
    }
    let version = u16::from_be_bytes([v[0], v[1]]);
    drop(version_guard);
    if version > tables::FORMAT_VERSION {
        return Err(Error::Format(format!(
            "file is format version {} but this library only supports up to {}; upgrade required",
            version,
            tables::FORMAT_VERSION
        )));
    }

    // Check required features.
    let rf_guard = meta
        .get(tables::meta_keys::REQUIRED_FEATURES)?
        .ok_or_else(|| Error::Format("missing chrona.required_features".into()))?;
    let rv = rf_guard.value();
    if rv.len() != 8 {
        return Err(Error::Format(format!(
            "required_features has bad length: {}",
            rv.len()
        )));
    }
    let required = u64::from_be_bytes([rv[0], rv[1], rv[2], rv[3], rv[4], rv[5], rv[6], rv[7]]);
    drop(rf_guard);
    let supported = tables::REQUIRED_FEATURES;
    let missing = required & !supported;
    if missing != 0 {
        return Err(Error::Format(format!(
            "file requires features not supported by this library (missing bits 0b{:b})",
            missing
        )));
    }

    Ok(())
}

/// Open or create a Chrona database file at the given path.
pub(crate) fn open_or_create(path: &Path) -> Result<Database> {
    let exists = path.exists();
    let db = Database::create(path)?;
    if !exists {
        initialize_new(&db)?;
    } else {
        verify_existing(&db)?;
    }
    Ok(db)
}
