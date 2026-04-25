//! Python bindings for Chrona.
//!
//! Exposes `Db`, `WriteTxn`, `Snapshot`, `Edge`, and a top-level
//! `query` / `query_json` interface. See the module README for usage.

#![allow(non_local_definitions)] // pyo3 macro false-positive on some versions

use chrona_core::{
    Db as CoreDb, EdgeInput as CoreEdgeInput, PropValue, Props, Snapshot as CoreSnapshot, Ts,
    WriteTxn as CoreWriteTxn,
};
use chrona_query::{execute, parse, render_json, QueryResult};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

fn err_py(e: chrona_core::Error) -> PyErr {
    match e {
        chrona_core::Error::Query(_) | chrona_core::Error::Schema(_) => {
            PyValueError::new_err(e.to_string())
        }
        _ => PyRuntimeError::new_err(e.to_string()),
    }
}

fn parse_ts(s: &str) -> PyResult<Ts> {
    Ts::parse(s).map_err(err_py)
}

fn props_from_py(py_props: Option<&Bound<'_, PyDict>>) -> PyResult<Props> {
    let mut out = Props::new();
    let Some(dict) = py_props else {
        return Ok(out);
    };
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let value = if v.is_none() {
            PropValue::Null
        } else if let Ok(b) = v.extract::<bool>() {
            PropValue::Bool(b)
        } else if let Ok(i) = v.extract::<i64>() {
            PropValue::Int(i)
        } else if let Ok(f) = v.extract::<f64>() {
            PropValue::Float(f)
        } else if let Ok(s) = v.extract::<String>() {
            PropValue::String(s)
        } else if let Ok(b) = v.extract::<Vec<u8>>() {
            PropValue::Bytes(b)
        } else {
            return Err(PyValueError::new_err(format!(
                "property value for key {:?} has unsupported type",
                key
            )));
        };
        out.insert(key, value);
    }
    Ok(out)
}

// ---------------- Edge ----------------

/// A live edge view returned by a query or a snapshot read.
///
/// Frozen, immutable. Fields:
/// - `id` (int): the edge's internal id.
/// - `from_ext_id`, `to_ext_id` (str): the external ids of the endpoints.
/// - `edge_type` (str).
/// - `valid_from`, `valid_to` (str RFC 3339; `valid_to` may be `None` for
///   open-ended edges).
/// - `observed_at` (str RFC 3339): when the system learned about it.
/// - `source` (str): provenance label.
/// - `confidence` (float in [0.0, 1.0]).
/// - `supersedes` (int or `None`): id of the edge this one revises.
#[pyclass(module = "chrona", frozen)]
#[derive(Clone)]
struct Edge {
    #[pyo3(get)]
    id: u64,
    #[pyo3(get)]
    from_ext_id: String,
    #[pyo3(get)]
    to_ext_id: String,
    #[pyo3(get)]
    edge_type: String,
    #[pyo3(get)]
    valid_from: String,
    #[pyo3(get)]
    valid_to: Option<String>,
    #[pyo3(get)]
    observed_at: String,
    #[pyo3(get)]
    source: String,
    #[pyo3(get)]
    confidence: f32,
    #[pyo3(get)]
    supersedes: Option<u64>,
}

impl From<chrona_core::EdgeView> for Edge {
    fn from(v: chrona_core::EdgeView) -> Self {
        Self {
            id: v.id.raw(),
            from_ext_id: v.from_ext_id,
            to_ext_id: v.to_ext_id,
            edge_type: v.edge_type,
            valid_from: v.valid_from.to_rfc3339(),
            valid_to: v.valid_to.map(|t| t.to_rfc3339()),
            observed_at: v.observed_at.to_rfc3339(),
            source: v.source,
            confidence: v.confidence,
            supersedes: v.supersedes.map(|e| e.raw()),
        }
    }
}

#[pymethods]
impl Edge {
    fn __repr__(&self) -> String {
        format!(
            "Edge(id={}, {}-[{}]->{}, valid_from={}, conf={:.2})",
            self.id,
            self.from_ext_id,
            self.edge_type,
            self.to_ext_id,
            self.valid_from,
            self.confidence
        )
    }
}

// ---------------- Node ----------------

/// A node view.
///
/// Frozen, immutable. Fields:
/// - `id` (int): internal id.
/// - `ext_id` (str): the external id you used when upserting.
/// - `node_type` (str or `None`).
/// - `created_at` (str RFC 3339).
#[pyclass(module = "chrona", frozen)]
#[derive(Clone)]
struct Node {
    #[pyo3(get)]
    id: u64,
    #[pyo3(get)]
    ext_id: String,
    #[pyo3(get)]
    node_type: Option<String>,
    #[pyo3(get)]
    created_at: String,
}

// ---------------- Snapshot ----------------

/// A read-only point-in-time view of the database.
///
/// Snapshots are cheap and can run concurrently with a writer. Use as a
/// context manager:
///
/// ```python
/// with db.read() as snap:
///     alice = snap.node_id("alice")
///     for edge in snap.neighbors_as_of(alice, "2026-02-01"):
///         print(edge)
/// ```
#[pyclass(module = "chrona", unsendable)]
struct Snapshot {
    inner: RefCell<Option<CoreSnapshot>>,
}

impl Snapshot {
    fn with<R>(&self, f: impl FnOnce(&CoreSnapshot) -> PyResult<R>) -> PyResult<R> {
        let borrow = self.inner.borrow();
        let snap = borrow
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("snapshot closed"))?;
        f(snap)
    }
}

#[pymethods]
impl Snapshot {
    /// Look up the internal node id for an external id.
    fn node_id(&self, ext_id: &str) -> PyResult<Option<u64>> {
        self.with(|snap| {
            let id = snap.get_node_id(ext_id).map_err(err_py)?;
            Ok(id.map(|n| n.raw()))
        })
    }

    /// All forward neighbors live at `when`.
    fn neighbors_as_of(&self, node_id: u64, when: &str) -> PyResult<Vec<Edge>> {
        self.with(|snap| {
            let t = parse_ts(when)?;
            let edges = snap
                .neighbors_as_of(chrona_core::NodeId::from_raw(node_id), t)
                .map_err(err_py)?;
            let mut out = Vec::with_capacity(edges.len());
            for e in edges {
                out.push(snap.view_edge(&e).map_err(err_py)?.into());
            }
            Ok(out)
        })
    }

    /// All reverse neighbors live at `when`.
    fn reverse_neighbors_as_of(&self, node_id: u64, when: &str) -> PyResult<Vec<Edge>> {
        self.with(|snap| {
            let t = parse_ts(when)?;
            let edges = snap
                .reverse_neighbors_as_of(chrona_core::NodeId::from_raw(node_id), t)
                .map_err(err_py)?;
            let mut out = Vec::with_capacity(edges.len());
            for e in edges {
                out.push(snap.view_edge(&e).map_err(err_py)?.into());
            }
            Ok(out)
        })
    }

    /// BFS out to `hops` hops.
    fn n_hops_as_of(&self, node_id: u64, hops: u32, when: &str) -> PyResult<Vec<Edge>> {
        if hops > u8::MAX as u32 {
            return Err(PyValueError::new_err("hops must fit in u8"));
        }
        self.with(|snap| {
            let t = parse_ts(when)?;
            let edges = snap
                .n_hops_as_of(chrona_core::NodeId::from_raw(node_id), hops as u8, t)
                .map_err(err_py)?;
            let mut out = Vec::with_capacity(edges.len());
            for e in edges {
                out.push(snap.view_edge(&e).map_err(err_py)?.into());
            }
            Ok(out)
        })
    }

    /// Shortest path; returns None if no path.
    fn path_as_of(&self, src: u64, dst: u64, when: &str) -> PyResult<Option<Vec<Edge>>> {
        self.with(|snap| {
            let t = parse_ts(when)?;
            let p = snap
                .path_as_of(
                    chrona_core::NodeId::from_raw(src),
                    chrona_core::NodeId::from_raw(dst),
                    t,
                )
                .map_err(err_py)?;
            match p {
                Some(edges) => {
                    let mut out = Vec::with_capacity(edges.len());
                    for e in edges {
                        out.push(snap.view_edge(&e).map_err(err_py)?.into());
                    }
                    Ok(Some(out))
                }
                None => Ok(None),
            }
        })
    }

    /// Database-wide stats as a dict.
    fn stats(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        self.with(|snap| {
            let s = snap.stats().map_err(err_py)?;
            let d = PyDict::new_bound(py);
            d.set_item("nodes", s.node_count)?;
            d.set_item("edges", s.edge_count)?;
            d.set_item("events", s.event_count)?;
            d.set_item("strings", s.string_count)?;
            Ok(d.into())
        })
    }

    /// Context-manager entry.
    fn __enter__<'py>(slf: PyRef<'py, Self>) -> PyRef<'py, Self> {
        slf
    }

    /// Context-manager exit: drop the inner snapshot.
    #[pyo3(signature = (_exc_type=None, _exc_value=None, _traceback=None))]
    fn __exit__(
        &self,
        _exc_type: Option<PyObject>,
        _exc_value: Option<PyObject>,
        _traceback: Option<PyObject>,
    ) -> bool {
        *self.inner.borrow_mut() = None;
        false
    }
}

// ---------------- WriteTxn ----------------

/// An open write transaction. One writer at a time per database.
///
/// Use as a context manager — the transaction commits on `__exit__`:
///
/// ```python
/// with db.write() as w:
///     w.upsert_node("alice", node_type="person")
///     w.add_edge(from_="alice", to="bob", edge_type="WORKS_WITH",
///                valid_from="2026-01-15", source="slack", confidence=0.9)
/// ```
#[pyclass(module = "chrona", unsendable)]
struct WriteTxn {
    inner: RefCell<Option<CoreWriteTxn>>,
}

impl WriteTxn {
    fn with_mut<R>(&self, f: impl FnOnce(&mut CoreWriteTxn) -> PyResult<R>) -> PyResult<R> {
        let mut borrow = self.inner.borrow_mut();
        let txn = borrow
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("transaction closed"))?;
        f(txn)
    }
}

#[pymethods]
impl WriteTxn {
    /// Upsert a node by external id. Returns the internal id.
    #[pyo3(signature = (ext_id, node_type = None, properties = None))]
    fn upsert_node(
        &self,
        ext_id: &str,
        node_type: Option<&str>,
        properties: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        let props = props_from_py(properties)?;
        self.with_mut(|txn| {
            let input = chrona_core::NodeInput {
                ext_id: ext_id.to_owned(),
                node_type: node_type.map(str::to_owned),
                properties: props,
            };
            let id = txn.upsert_node_full(input).map_err(err_py)?;
            Ok(id.raw())
        })
    }

    /// Insert a new edge. Returns its internal id.
    #[pyo3(signature = (from_, to, edge_type, valid_from = None, valid_to = None,
                         observed_at = None, source = None, confidence = 1.0,
                         properties = None))]
    #[allow(clippy::too_many_arguments)]
    fn add_edge(
        &self,
        from_: &str,
        to: &str,
        edge_type: &str,
        valid_from: Option<&str>,
        valid_to: Option<&str>,
        observed_at: Option<&str>,
        source: Option<&str>,
        confidence: f32,
        properties: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        let props = props_from_py(properties)?;
        let vf = match valid_from {
            Some(s) => parse_ts(s)?,
            None => Ts::now(),
        };
        let vt = match valid_to {
            Some(s) => Some(parse_ts(s)?),
            None => None,
        };
        let oa = match observed_at {
            Some(s) => parse_ts(s)?,
            None => vf,
        };
        self.with_mut(|txn| {
            let input = CoreEdgeInput {
                from: from_.to_owned(),
                to: to.to_owned(),
                edge_type: edge_type.to_owned(),
                valid_from: vf,
                valid_to: vt,
                observed_at: oa,
                source: source.unwrap_or("").to_owned(),
                confidence,
                properties: props,
            };
            let id = txn.add_edge(input).map_err(err_py)?;
            Ok(id.raw())
        })
    }

    /// Invalidate an edge at `at`.
    fn invalidate_edge(&self, edge_id: u64, at: &str) -> PyResult<()> {
        let t = parse_ts(at)?;
        self.with_mut(|txn| {
            txn.invalidate_edge(chrona_core::EdgeId::from_raw(edge_id), t)
                .map_err(err_py)
        })
    }

    /// Commit. Subsequent calls fail.
    fn commit(&self) -> PyResult<()> {
        let mut borrow = self.inner.borrow_mut();
        let txn = borrow
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("transaction already closed"))?;
        txn.commit().map_err(err_py)
    }

    /// Context-manager entry.
    fn __enter__<'py>(slf: PyRef<'py, Self>) -> PyRef<'py, Self> {
        slf
    }

    /// Context-manager exit: commit on success, abort on exception.
    #[pyo3(signature = (exc_type=None, _exc_value=None, _traceback=None))]
    fn __exit__(
        &self,
        exc_type: Option<PyObject>,
        _exc_value: Option<PyObject>,
        _traceback: Option<PyObject>,
    ) -> PyResult<bool> {
        let mut borrow = self.inner.borrow_mut();
        let Some(txn) = borrow.take() else {
            return Ok(false);
        };
        if exc_type.is_some() {
            // abort on exception
            drop(txn);
        } else {
            txn.commit().map_err(err_py)?;
        }
        Ok(false)
    }
}

// ---------------- Db ----------------

/// A Chrona database handle.
#[pyclass(module = "chrona")]
struct Db {
    inner: Arc<Mutex<CoreDb>>,
    #[pyo3(get)]
    path: String,
}

#[pymethods]
impl Db {
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let db = CoreDb::open(path).map_err(err_py)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(db)),
            path: path.to_owned(),
        })
    }

    /// Start a write transaction.
    fn write(&self) -> PyResult<WriteTxn> {
        let db = self.inner.lock().unwrap();
        let txn = db.begin_write().map_err(err_py)?;
        Ok(WriteTxn {
            inner: RefCell::new(Some(txn)),
        })
    }

    /// Start a read snapshot.
    fn read(&self) -> PyResult<Snapshot> {
        let db = self.inner.lock().unwrap();
        let snap = db.begin_read().map_err(err_py)?;
        Ok(Snapshot {
            inner: RefCell::new(Some(snap)),
        })
    }

    /// Run a DSL query and return a Python list of results.
    ///
    /// For edge-returning queries: list of `Edge`.
    /// For path queries: `list[Edge]` or `None`.
    /// For diff queries: raise — use `query_json` instead.
    fn query(&self, q: &str, py: Python<'_>) -> PyResult<PyObject> {
        let db = self.inner.lock().unwrap();
        let snap = db.begin_read().map_err(err_py)?;
        let ast = parse(q).map_err(err_py)?;
        let result = execute(&snap, ast).map_err(err_py)?;
        match result {
            QueryResult::Edges(v) => {
                let list = PyList::empty_bound(py);
                for ev in v {
                    list.append(Py::new(py, Edge::from(ev))?)?;
                }
                Ok(list.into())
            }
            QueryResult::Path(Some(v)) => {
                let list = PyList::empty_bound(py);
                for ev in v {
                    list.append(Py::new(py, Edge::from(ev))?)?;
                }
                Ok(list.into())
            }
            QueryResult::Path(None) => Ok(py.None()),
            QueryResult::Diff(_) => Err(PyValueError::new_err(
                "diff results are structured; use db.query_json(q) instead",
            )),
        }
    }

    /// Run a DSL query and return JSON as a string. Works for all query types.
    fn query_json(&self, q: &str) -> PyResult<String> {
        let db = self.inner.lock().unwrap();
        let snap = db.begin_read().map_err(err_py)?;
        let ast = parse(q).map_err(err_py)?;
        let result = execute(&snap, ast).map_err(err_py)?;
        Ok(render_json(&result))
    }

    /// Absolute path to the database file.
    fn __repr__(&self) -> String {
        format!("Db(path={:?})", self.path)
    }
}

// ---------------- Module ----------------

/// Chrona Python bindings.
#[pymodule]
fn chrona(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<Db>()?;
    m.add_class::<WriteTxn>()?;
    m.add_class::<Snapshot>()?;
    m.add_class::<Edge>()?;
    m.add_class::<Node>()?;
    Ok(())
}
