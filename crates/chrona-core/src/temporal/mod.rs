//! Temporal layer: event log, traversal, and diffs.

pub mod diff;
pub mod event;
pub mod traverse;

pub use diff::{DiffEntry, DiffSummary};
pub use event::{EventKind, EventRecord};
pub use traverse::{n_hops, neighbors, shortest_path, EdgeScanner};
