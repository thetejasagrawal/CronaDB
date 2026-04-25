//! Public database API: `Db`, `WriteTxn`, `Snapshot`.
//!
//! This is the primary surface developers use. Everything in the lower layers
//! (storage, graph, temporal) is orchestrated here.

use crate::error::{Error, Result};
use crate::graph::{Edge, EdgeInput, EdgeView, Node, NodeInput};
use crate::id::{EdgeId, EventId, NodeId, StringId};
use crate::storage::{self, counters, keys, strings, tables};
use crate::temporal::event::{self as event_mod, EventKind, EventRecord};
use crate::temporal::{traverse, DiffSummary};
use crate::time::Ts;
use redb::{Database, ReadTransaction, ReadableTable, ReadableTableMetadata, WriteTransaction};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A Chrona database — an owning handle over a single `.chrona` file.
///
/// `Db` is `Send + Sync` and cheap to clone (internally an `Arc`).
#[derive(Clone)]
pub struct Db {
    inner: Arc<DbInner>,
}

struct DbInner {
    db: Database,
    path: PathBuf,
}

impl Db {
    /// Open an existing database at `path`, or create a new empty one.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let _span = tracing::info_span!("chrona.open", path = %path.display()).entered();
        let db = storage::open_or_create(path)?;
        Ok(Db {
            inner: Arc::new(DbInner {
                db,
                path: path.to_path_buf(),
            }),
        })
    }

    /// Start a read snapshot.
    ///
    /// Multiple read snapshots may exist concurrently. A snapshot is unaffected
    /// by writes that commit after it was taken.
    pub fn begin_read(&self) -> Result<Snapshot> {
        let _span = tracing::debug_span!("chrona.txn.read").entered();
        let txn = self.inner.db.begin_read()?;
        Ok(Snapshot { txn })
    }

    /// Start a write transaction.
    ///
    /// Only one write transaction can exist at a time. This call blocks if
    /// another writer is active.
    pub fn begin_write(&self) -> Result<WriteTxn> {
        let _span = tracing::debug_span!("chrona.txn.write").entered();
        let txn = self.inner.db.begin_write()?;
        Ok(WriteTxn { txn: Some(txn) })
    }

    /// Absolute path to the underlying file.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Run a read-only closure over a snapshot. Convenience wrapper.
    pub fn read<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Snapshot) -> Result<T>,
    {
        let s = self.begin_read()?;
        f(&s)
    }

    /// Run a write closure and commit on success. If the closure returns an
    /// error, the transaction is aborted (dropped).
    pub fn write<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut WriteTxn) -> Result<T>,
    {
        let mut w = self.begin_write()?;
        let out = f(&mut w)?;
        w.commit()?;
        Ok(out)
    }
}

// ---------------- Snapshot ----------------

/// A read-only, consistent view of the database at a point in time.
pub struct Snapshot {
    txn: ReadTransaction,
}

impl Snapshot {
    /// Resolve an external id to its internal `NodeId`, if present.
    pub fn get_node_id(&self, ext_id: &str) -> Result<Option<NodeId>> {
        let table = match self.txn.open_table(tables::EXT_IDS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let got = table.get(ext_id.as_bytes())?;
        let out = got.map(|v| NodeId::from_raw(v.value()));
        Ok(out)
    }

    /// Fetch a node by internal id.
    pub fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        let table = match self.txn.open_table(tables::NODES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let got = table.get(id.raw())?;
        let Some(bytes) = got else {
            return Ok(None);
        };
        let node = Node::decode(id, bytes.value())?;
        Ok(Some(node))
    }

    /// Fetch a node by its external id.
    pub fn get_node_by_ext_id(&self, ext_id: &str) -> Result<Option<Node>> {
        let Some(nid) = self.get_node_id(ext_id)? else {
            return Ok(None);
        };
        self.get_node(nid)
    }

    /// Fetch an edge by internal id.
    pub fn get_edge(&self, id: EdgeId) -> Result<Option<Edge>> {
        let table = match self.txn.open_table(tables::EDGES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let got = table.get(id.raw())?;
        let Some(bytes) = got else {
            return Ok(None);
        };
        let edge = Edge::decode(id, bytes.value())?;
        Ok(Some(edge))
    }

    /// Resolve a `StringId` to its string value.
    pub fn resolve_string(&self, id: StringId) -> Result<String> {
        strings::resolve_string(&self.txn, id)?
            .ok_or_else(|| Error::Internal(format!("dangling StringId: {}", id.raw())))
    }

    /// Return all forward-direction edges from `node` that were live at `as_of`.
    pub fn neighbors_as_of(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>> {
        scan_adjacency(&self.txn, node, as_of, /* forward */ true)
    }

    /// Return all reverse-direction edges to `node` that were live at `as_of`.
    pub fn reverse_neighbors_as_of(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>> {
        scan_adjacency(&self.txn, node, as_of, /* forward */ false)
    }

    /// Current-time convenience: neighbors as of now.
    pub fn neighbors(&self, node: NodeId) -> Result<Vec<Edge>> {
        self.neighbors_as_of(node, Ts::now())
    }

    /// Expand to `hops` hops, as-of `as_of`.
    pub fn n_hops_as_of(&self, start: NodeId, hops: u8, as_of: Ts) -> Result<Vec<Edge>> {
        traverse::n_hops(self, start, hops, as_of)
    }

    /// Shortest path from `src` to `dst`, evaluated at `as_of`.
    pub fn path_as_of(&self, src: NodeId, dst: NodeId, as_of: Ts) -> Result<Option<Vec<Edge>>> {
        traverse::shortest_path(self, src, dst, as_of)
    }

    /// Stream all events in the range `[t1, t2]` (inclusive on both ends).
    pub fn events_between(&self, t1: Ts, t2: Ts) -> Result<Vec<EventRecord>> {
        let table = match self.txn.open_table(tables::EVENTS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let lower = keys::event_lower(t1);
        let upper = keys::event_upper(t2);
        let mut out = Vec::new();
        for entry in table.range(lower.as_slice()..=upper.as_slice())? {
            let (k, v) = entry?;
            let key_bytes = k.value();
            if key_bytes.len() != 16 {
                return Err(Error::Format(format!(
                    "event key has unexpected length {}",
                    key_bytes.len()
                )));
            }
            let ts_bytes: [u8; 8] = key_bytes[..8].try_into().unwrap();
            let ts = Ts::from_sortable_bytes(ts_bytes);
            let id_bytes: [u8; 8] = key_bytes[8..16].try_into().unwrap();
            let eid = EventId::from_raw(u64::from_be_bytes(id_bytes));
            if let Some(rec) = EventRecord::decode_value(eid, ts, v.value())? {
                out.push(rec);
            }
        }
        Ok(out)
    }

    /// Aggregate events in `[t1, t2]` into a `DiffSummary`.
    pub fn diff_between(&self, t1: Ts, t2: Ts) -> Result<DiffSummary> {
        let mut s = DiffSummary::default();
        for ev in self.events_between(t1, t2)? {
            s.push(&ev)?;
        }
        Ok(s)
    }

    /// Return database-wide statistics.
    pub fn stats(&self) -> Result<Stats> {
        Ok(Stats {
            node_count: node_count(&self.txn)?,
            edge_count: edge_count(&self.txn)?,
            event_count: event_count(&self.txn)?,
            string_count: string_count(&self.txn)?,
        })
    }

    /// Run a full integrity check against the database.
    ///
    /// Returns a [`crate::VerifyReport`] with one line per check performed,
    /// plus a list of any errors. A clean report is an empty errors list.
    pub fn verify(&self) -> Result<crate::verify::VerifyReport> {
        crate::verify::verify(&self.txn)
    }

    /// Return every node in the database, sorted by internal id.
    pub fn all_nodes(&self) -> Result<Vec<Node>> {
        let table = match self.txn.open_table(tables::NODES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (k, v) = entry?;
            let id = NodeId::from_raw(k.value());
            let node = Node::decode(id, v.value())?;
            out.push(node);
        }
        Ok(out)
    }

    /// Return every edge in the database as a resolved `EdgeView`.
    pub fn all_edges_view(&self) -> Result<Vec<EdgeView>> {
        let table = match self.txn.open_table(tables::EDGES) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (k, v) = entry?;
            let id = EdgeId::from_raw(k.value());
            let edge = Edge::decode(id, v.value())?;
            out.push(self.view_edge(&edge)?);
        }
        Ok(out)
    }

    /// Walk the revision chain starting at `edge_id`, following `supersedes`
    /// backwards to the original observation.
    ///
    /// The returned vector starts with `edge_id` itself and ends with the
    /// oldest edge in the chain. Returns an empty vec if `edge_id` does not
    /// exist. Loops are detected and cut off defensively.
    pub fn revision_chain(&self, edge_id: EdgeId) -> Result<Vec<Edge>> {
        let mut out = Vec::new();
        let mut seen: std::collections::HashSet<EdgeId> = std::collections::HashSet::new();
        let mut cur = Some(edge_id);
        while let Some(id) = cur {
            if !seen.insert(id) {
                // Cycle defense — shouldn't happen but don't hang.
                break;
            }
            match self.get_edge(id)? {
                Some(e) => {
                    cur = e.supersedes;
                    out.push(e);
                }
                None => {
                    if out.is_empty() {
                        return Ok(Vec::new());
                    }
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Produce a resolved `EdgeView` from an `Edge`, looking up string ids.
    pub fn view_edge(&self, edge: &Edge) -> Result<EdgeView> {
        let edge_type = self.resolve_string(edge.type_id)?;
        let source = if edge.source_id == StringId::ZERO {
            String::new()
        } else {
            self.resolve_string(edge.source_id)?
        };
        let from_ext = self
            .get_node(edge.from)?
            .ok_or_else(|| Error::Internal(format!("dangling from: {}", edge.from)))?
            .ext_id;
        let to_ext = self
            .get_node(edge.to)?
            .ok_or_else(|| Error::Internal(format!("dangling to: {}", edge.to)))?
            .ext_id;
        Ok(EdgeView {
            id: edge.id,
            from: edge.from,
            from_ext_id: from_ext,
            to: edge.to,
            to_ext_id: to_ext,
            edge_type,
            valid_from: edge.valid_from,
            valid_to: edge.valid_to,
            observed_at: edge.observed_at,
            source,
            confidence: edge.confidence,
            supersedes: edge.supersedes,
            properties: edge.props.clone(),
        })
    }
}

impl traverse::EdgeScanner for Snapshot {
    fn scan_fwd(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>> {
        self.neighbors_as_of(node, as_of)
    }
    fn scan_rev(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>> {
        self.reverse_neighbors_as_of(node, as_of)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub node_count: u64,
    pub edge_count: u64,
    pub event_count: u64,
    pub string_count: u64,
}

// ---------------- WriteTxn ----------------

/// An open write transaction. Commit by calling [`WriteTxn::commit`]; otherwise
/// the transaction is aborted on drop.
pub struct WriteTxn {
    txn: Option<WriteTransaction>,
}

impl WriteTxn {
    fn txn(&self) -> &WriteTransaction {
        self.txn.as_ref().expect("txn already consumed")
    }

    /// Look up a node's internal id by its external id.
    pub fn get_node_id(&self, ext_id: &str) -> Result<Option<NodeId>> {
        let table = self.txn().open_table(tables::EXT_IDS)?;
        let got = table.get(ext_id.as_bytes())?;
        let out = got.map(|v| NodeId::from_raw(v.value()));
        Ok(out)
    }

    /// Insert a new node (or return the existing id if present).
    pub fn upsert_node(
        &mut self,
        ext_id: impl AsRef<str>,
        node_type: Option<&str>,
    ) -> Result<NodeId> {
        self.upsert_node_full(NodeInput {
            ext_id: ext_id.as_ref().to_owned(),
            node_type: node_type.map(str::to_owned),
            properties: Default::default(),
        })
    }

    /// Full-feature upsert with properties.
    pub fn upsert_node_full(&mut self, input: NodeInput) -> Result<NodeId> {
        if input.ext_id.is_empty() {
            return Err(Error::Schema("ext_id must be non-empty".into()));
        }

        // Fast path: already exists.
        if let Some(id) = self.get_node_id(&input.ext_id)? {
            return Ok(id);
        }

        let type_id = match &input.node_type {
            Some(s) if !s.is_empty() => Some(strings::intern(self.txn(), s.as_bytes())?),
            _ => None,
        };

        let id = counters::next_node_id(self.txn())?;
        let node = Node {
            id,
            ext_id: input.ext_id.clone(),
            type_id,
            created_at: Ts::now(),
            props: input.properties.clone(),
        };

        // Write node record.
        {
            let bytes = node.encode()?;
            let mut nodes = self.txn().open_table(tables::NODES)?;
            nodes.insert(id.raw(), bytes.as_slice())?;
        }
        // Write ext_id mapping.
        {
            let mut ext = self.txn().open_table(tables::EXT_IDS)?;
            ext.insert(input.ext_id.as_bytes(), id.raw())?;
        }

        // Append NodeAdded event.
        self.append_event(EventKind::NodeAdded, event_mod::payload_node_added(&node)?)?;

        Ok(id)
    }

    /// Insert a new edge, returning its id.
    ///
    /// This call:
    /// 1. Resolves (or upserts) the endpoints by external id.
    /// 2. Interns the edge type and source name.
    /// 3. Writes the edge record, forward/reverse adjacency, temporal index.
    /// 4. Appends an `EdgeObserved` event.
    pub fn add_edge(&mut self, input: EdgeInput) -> Result<EdgeId> {
        if input.edge_type.is_empty() {
            return Err(Error::Schema("edge_type must be non-empty".into()));
        }
        if input.from == input.to && input.from.is_empty() {
            return Err(Error::Schema("from/to must be non-empty".into()));
        }
        // Resolve or upsert endpoints.
        let from = self.upsert_node(&input.from, None)?;
        let to = self.upsert_node(&input.to, None)?;
        self.add_edge_internal(from, to, input, None)
    }

    fn add_edge_internal(
        &mut self,
        from: NodeId,
        to: NodeId,
        input: EdgeInput,
        supersedes: Option<EdgeId>,
    ) -> Result<EdgeId> {
        let type_id = strings::intern(self.txn(), input.edge_type.as_bytes())?;
        let source_id = if input.source.is_empty() {
            StringId::ZERO
        } else {
            strings::intern(self.txn(), input.source.as_bytes())?
        };

        let id = counters::next_edge_id(self.txn())?;
        let edge = Edge {
            id,
            from,
            to,
            type_id,
            valid_from: input.valid_from,
            valid_to: input.valid_to,
            observed_at: input.observed_at,
            source_id,
            confidence: input.confidence,
            supersedes,
            props: input.properties,
        };
        edge.validate()?;

        // Canonical record.
        {
            let bytes = edge.encode()?;
            let mut t = self.txn().open_table(tables::EDGES)?;
            t.insert(id.raw(), bytes.as_slice())?;
        }
        // Forward adjacency.
        {
            let mut t = self.txn().open_table(tables::FWD_ADJ)?;
            t.insert(
                keys::adj_key(from, edge.valid_from, id).as_slice(),
                to.raw(),
            )?;
        }
        // Reverse adjacency.
        {
            let mut t = self.txn().open_table(tables::REV_ADJ)?;
            t.insert(
                keys::adj_key(to, edge.valid_from, id).as_slice(),
                from.raw(),
            )?;
        }
        // Temporal index.
        {
            let mut t = self.txn().open_table(tables::TEMPORAL_IDX)?;
            t.insert(
                keys::temporal_idx_key(edge.valid_from, id).as_slice(),
                [].as_slice(),
            )?;
        }
        // Supersedes index.
        if let Some(old) = supersedes {
            let mut t = self.txn().open_table(tables::SUPERSEDES_IDX)?;
            t.insert(keys::supersedes_key(old, id).as_slice(), [].as_slice())?;
        }

        // Append EdgeObserved event.
        self.append_event(
            EventKind::EdgeObserved,
            event_mod::payload_edge_observed(&edge)?,
        )?;

        Ok(id)
    }

    /// Invalidate an edge by setting its `valid_to`. Fails if the edge does not
    /// exist or already has a smaller `valid_to`.
    pub fn invalidate_edge(&mut self, id: EdgeId, at: Ts) -> Result<()> {
        let edge = self.read_edge(id)?;
        if let Some(existing) = edge.valid_to {
            if existing.raw() <= at.raw() {
                return Ok(()); // already invalidated no later than this
            }
        }
        let mut updated = edge;
        updated.valid_to = Some(at);
        let bytes = updated.encode()?;
        {
            let mut t = self.txn().open_table(tables::EDGES)?;
            t.insert(id.raw(), bytes.as_slice())?;
        }
        self.append_event(
            EventKind::EdgeInvalidated,
            event_mod::payload_edge_invalidated(id, at),
        )?;
        Ok(())
    }

    /// Supersede an existing edge with a new one. The old edge's `valid_to` is
    /// set to the new edge's `valid_from`; the new edge has `supersedes = old`.
    pub fn supersede_edge(&mut self, old: EdgeId, new_input: EdgeInput) -> Result<EdgeId> {
        let old_edge = self.read_edge(old)?;
        // Resolve or upsert endpoints for the new edge (they may differ).
        let new_from = self.upsert_node(&new_input.from, None)?;
        let new_to = self.upsert_node(&new_input.to, None)?;
        let new_valid_from = new_input.valid_from;

        // Update the old edge's valid_to.
        self.invalidate_edge(old, new_valid_from)?;
        // Append the new edge, carrying supersedes.
        let new_id = self.add_edge_internal(new_from, new_to, new_input, Some(old))?;

        // Emit an explicit EdgeSuperseded event for audit/diff.
        let mut payload = old.raw().to_be_bytes().to_vec();
        let new_edge = self.read_edge(new_id)?;
        payload.extend_from_slice(&new_edge.encode()?);
        self.append_event(EventKind::EdgeSuperseded, payload)?;

        let _ = old_edge; // silence unused warnings on ancillary info
        Ok(new_id)
    }

    fn read_edge(&self, id: EdgeId) -> Result<Edge> {
        let t = self.txn().open_table(tables::EDGES)?;
        let got = t.get(id.raw())?;
        let bytes = got.ok_or_else(|| Error::NotFound(format!("edge {}", id)))?;
        let edge = Edge::decode(id, bytes.value())?;
        Ok(edge)
    }

    fn append_event(&self, kind: EventKind, payload: Vec<u8>) -> Result<()> {
        let id = counters::next_event_id(self.txn())?;
        let ts = Ts::now();
        let rec = EventRecord {
            id,
            timestamp: ts,
            kind,
            payload,
        };
        let mut t = self.txn().open_table(tables::EVENTS)?;
        t.insert(
            keys::event_key(ts, id.raw()).as_slice(),
            rec.encode_value().as_slice(),
        )?;
        Ok(())
    }

    /// Commit the transaction, making its writes durable and visible to
    /// future readers.
    pub fn commit(mut self) -> Result<()> {
        let _span = tracing::debug_span!("chrona.commit").entered();
        if let Some(txn) = self.txn.take() {
            txn.commit()?;
        }
        Ok(())
    }

    /// Explicitly abort the transaction (drop without commit).
    pub fn abort(mut self) {
        drop(self.txn.take());
    }
}

// ---------------- Shared helpers ----------------

fn node_count(txn: &ReadTransaction) -> Result<u64> {
    match txn.open_table(tables::NODES) {
        Ok(t) => Ok(t.len()?),
        Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
        Err(e) => Err(e.into()),
    }
}
fn edge_count(txn: &ReadTransaction) -> Result<u64> {
    match txn.open_table(tables::EDGES) {
        Ok(t) => Ok(t.len()?),
        Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
        Err(e) => Err(e.into()),
    }
}
fn event_count(txn: &ReadTransaction) -> Result<u64> {
    match txn.open_table(tables::EVENTS) {
        Ok(t) => Ok(t.len()?),
        Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
        Err(e) => Err(e.into()),
    }
}
fn string_count(txn: &ReadTransaction) -> Result<u64> {
    match txn.open_table(tables::STRINGS_FWD) {
        Ok(t) => Ok(t.len()?),
        Err(redb::TableError::TableDoesNotExist(_)) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

/// Scan a node's adjacency (fwd or rev) for all edges live at `as_of`.
fn scan_adjacency(
    txn: &ReadTransaction,
    node: NodeId,
    as_of: Ts,
    forward: bool,
) -> Result<Vec<Edge>> {
    let adj_def = if forward {
        tables::FWD_ADJ
    } else {
        tables::REV_ADJ
    };
    let adj = match txn.open_table(adj_def) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let edges = match txn.open_table(tables::EDGES) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let lower = keys::adj_prefix_min(node);
    let upper = keys::adj_upper_for_ts(node, as_of);

    let mut out = Vec::new();
    let range_iter = adj.range::<&[u8]>(lower.as_slice()..=upper.as_slice())?;
    for entry in range_iter {
        let (k, _v) = entry?;
        let key_bytes = k.value();
        if key_bytes.len() != 24 {
            return Err(Error::Format(format!(
                "adjacency key has unexpected length {}",
                key_bytes.len()
            )));
        }
        let eid_bytes: [u8; 8] = key_bytes[16..24].try_into().unwrap();
        let eid = EdgeId::from_raw(u64::from_be_bytes(eid_bytes));
        let got = edges.get(eid.raw())?;
        let Some(v) = got else {
            continue; // should not happen if invariants hold
        };
        let edge = Edge::decode(eid, v.value())?;
        if edge.is_live_at(as_of) {
            out.push(edge);
        }
    }
    Ok(out)
}
