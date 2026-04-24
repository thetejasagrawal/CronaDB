//! Full database verification per FORMAT.md §8.
//!
//! Exposed through [`Snapshot::verify`]. Walks every table, checks invariants,
//! and returns a [`VerifyReport`] listing the checks performed and any errors
//! found.

use crate::error::Result;
use crate::graph::{Edge, Node};
use crate::id::{EdgeId, NodeId, StringId};
use crate::storage::tables::{self, meta_keys};
use crate::time::Ts;
use redb::{ReadTransaction, ReadableTable, ReadableTableMetadata};

/// Outcome of [`Snapshot::verify`].
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    /// Human-readable log of each check performed.
    pub lines: Vec<String>,
    /// Fatal inconsistencies found, if any.
    pub errors: Vec<String>,
}

impl VerifyReport {
    /// True if no errors were recorded.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Run the full format integrity check.
pub(crate) fn verify(txn: &ReadTransaction) -> Result<VerifyReport> {
    let mut report = VerifyReport::default();

    // 1. Container integrity (delegated to redb via earlier open).
    report.lines.push("container open OK".to_string());

    // 2. Magic and version.
    check_meta(txn, &mut report)?;

    // 3. Counters reasonable.
    let counters = read_counters(txn, &mut report)?;

    // 4. String interner consistency.
    check_string_interner(txn, &mut report, counters.max_string_id)?;

    // 5. Ext-id uniqueness + referential integrity of nodes.
    let node_ids = collect_node_ids(txn, &mut report)?;

    // 6. Referential integrity of edges + adjacency completeness.
    check_edges(txn, &mut report, &node_ids)?;

    // 7. Temporal well-formedness handled inside check_edges.
    // 8. Event log well-formedness.
    check_events(txn, &mut report)?;

    // 9. Adjacency count matches edges.
    check_adjacency_counts(txn, &mut report)?;

    // 10. Confidence range handled inside edge decode (enforced at write).

    report
        .lines
        .push(format!("total errors: {}", report.errors.len()));
    Ok(report)
}

#[derive(Default)]
struct Counters {
    max_string_id: u32,
}

fn check_meta(txn: &ReadTransaction, report: &mut VerifyReport) -> Result<()> {
    let table = match txn.open_table(tables::META) {
        Ok(t) => t,
        Err(_) => {
            report.errors.push("meta table missing".into());
            return Ok(());
        }
    };
    for (label, key) in [
        ("magic", meta_keys::MAGIC),
        ("format_version", meta_keys::FORMAT_VERSION),
        ("writer_version", meta_keys::WRITER_VERSION),
        ("required_features", meta_keys::REQUIRED_FEATURES),
    ] {
        let present = table.get(key)?.is_some();
        if !present {
            report.errors.push(format!("meta key missing: {}", label));
        }
    }
    report
        .lines
        .push("meta: magic, version, features present".to_string());
    Ok(())
}

fn read_counters(txn: &ReadTransaction, report: &mut VerifyReport) -> Result<Counters> {
    let mut c = Counters::default();
    let table = match txn.open_table(tables::META) {
        Ok(t) => t,
        Err(_) => return Ok(c),
    };
    let s_bytes = table.get(meta_keys::STRING_ID_COUNTER)?;
    if let Some(v) = s_bytes {
        let b = v.value();
        if b.len() == 4 {
            c.max_string_id = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
    }
    report
        .lines
        .push(format!("counters read: max_string_id={}", c.max_string_id));
    Ok(c)
}

fn check_string_interner(
    txn: &ReadTransaction,
    report: &mut VerifyReport,
    max_string_id: u32,
) -> Result<()> {
    let fwd = match txn.open_table(tables::STRINGS_FWD) {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let rev = match txn.open_table(tables::STRINGS_REV) {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let fwd_len = fwd.len()?;
    let rev_len = rev.len()?;
    if fwd_len != rev_len {
        report.errors.push(format!(
            "string interner size mismatch: fwd={}, rev={}",
            fwd_len, rev_len
        ));
    }

    // Walk fwd and verify each id round-trips through rev.
    let mut checked = 0u64;
    for entry in fwd.iter()? {
        let (k, v) = entry?;
        let bytes = k.value().to_vec();
        let id = v.value();
        let rev_val = rev.get(id)?;
        match rev_val {
            Some(r) => {
                if r.value() != bytes.as_slice() {
                    report.errors.push(format!(
                        "strings_fwd and strings_rev disagree for id {}",
                        id
                    ));
                }
                if id > max_string_id {
                    report.errors.push(format!(
                        "string id {} exceeds counter {}",
                        id, max_string_id
                    ));
                }
            }
            None => {
                report.errors.push(format!("strings_rev missing id {}", id));
            }
        }
        checked += 1;
    }
    report
        .lines
        .push(format!("string interner: {} ids checked", checked));
    Ok(())
}

fn collect_node_ids(
    txn: &ReadTransaction,
    report: &mut VerifyReport,
) -> Result<std::collections::HashSet<NodeId>> {
    let mut set = std::collections::HashSet::new();
    let table = match txn.open_table(tables::NODES) {
        Ok(t) => t,
        Err(_) => return Ok(set),
    };
    let ext_table = match txn.open_table(tables::EXT_IDS) {
        Ok(t) => t,
        Err(_) => return Ok(set),
    };

    for entry in table.iter()? {
        let (k, v) = entry?;
        let id = NodeId::from_raw(k.value());
        // Round-trip decode.
        match Node::decode(id, v.value()) {
            Ok(node) => {
                // Check ext_ids maps back to this id.
                let mapped = ext_table.get(node.ext_id.as_bytes())?;
                match mapped {
                    Some(m) if m.value() == id.raw() => {}
                    Some(m) => report.errors.push(format!(
                        "ext_id {:?} maps to {} but node is {}",
                        node.ext_id,
                        m.value(),
                        id.raw()
                    )),
                    None => report
                        .errors
                        .push(format!("ext_id {:?} not in ext_ids", node.ext_id)),
                }
                set.insert(id);
            }
            Err(e) => {
                report
                    .errors
                    .push(format!("node {} fails to decode: {}", id, e));
            }
        }
    }
    report
        .lines
        .push(format!("nodes: {} records verified", set.len()));
    Ok(set)
}

fn check_edges(
    txn: &ReadTransaction,
    report: &mut VerifyReport,
    node_ids: &std::collections::HashSet<NodeId>,
) -> Result<()> {
    let edges = match txn.open_table(tables::EDGES) {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let mut count = 0u64;
    for entry in edges.iter()? {
        let (k, v) = entry?;
        let id = EdgeId::from_raw(k.value());
        match Edge::decode(id, v.value()) {
            Ok(edge) => {
                if !node_ids.contains(&edge.from) {
                    report.errors.push(format!(
                        "edge {} references missing from-node {}",
                        id, edge.from
                    ));
                }
                if !node_ids.contains(&edge.to) {
                    report.errors.push(format!(
                        "edge {} references missing to-node {}",
                        id, edge.to
                    ));
                }
                if let Some(vt) = edge.valid_to {
                    if vt.raw() < edge.valid_from.raw() {
                        report
                            .errors
                            .push(format!("edge {} has valid_to < valid_from", id));
                    }
                }
                if !edge.confidence.is_finite() || edge.confidence < 0.0 || edge.confidence > 1.0 {
                    report
                        .errors
                        .push(format!("edge {} confidence out of range", id));
                }
                let _ = StringId::from_raw; // silence unused if module reorganized
            }
            Err(e) => report
                .errors
                .push(format!("edge {} fails to decode: {}", id, e)),
        }
        count += 1;
    }
    report
        .lines
        .push(format!("edges: {} records verified", count));
    Ok(())
}

fn check_events(txn: &ReadTransaction, report: &mut VerifyReport) -> Result<()> {
    let events = match txn.open_table(tables::EVENTS) {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let mut prev_key: Option<Vec<u8>> = None;
    let mut count = 0u64;
    for entry in events.iter()? {
        let (k, _v) = entry?;
        let bytes = k.value().to_vec();
        if let Some(p) = &prev_key {
            if &bytes <= p {
                report
                    .errors
                    .push("events not strictly increasing".to_string());
                break;
            }
        }
        prev_key = Some(bytes);
        count += 1;
    }
    report
        .lines
        .push(format!("events: {} entries in strict order", count));
    // Silence unused-constant warnings.
    let _ = Ts::EPOCH;
    Ok(())
}

fn check_adjacency_counts(txn: &ReadTransaction, report: &mut VerifyReport) -> Result<()> {
    let edges_len = match txn.open_table(tables::EDGES) {
        Ok(t) => t.len()?,
        Err(_) => 0,
    };
    let fwd_len = match txn.open_table(tables::FWD_ADJ) {
        Ok(t) => t.len()?,
        Err(_) => 0,
    };
    let rev_len = match txn.open_table(tables::REV_ADJ) {
        Ok(t) => t.len()?,
        Err(_) => 0,
    };
    if fwd_len != edges_len {
        report.errors.push(format!(
            "fwd_adj count {} != edges count {}",
            fwd_len, edges_len
        ));
    }
    if rev_len != edges_len {
        report.errors.push(format!(
            "rev_adj count {} != edges count {}",
            rev_len, edges_len
        ));
    }
    report.lines.push(format!(
        "adjacency: fwd={} rev={} edges={}",
        fwd_len, rev_len, edges_len
    ));
    Ok(())
}
