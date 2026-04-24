//! # chrona-core
//!
//! Embedded temporal graph engine. This crate provides the core storage,
//! graph model, temporal layer, and event log.
//!
//! The public surface is the [`Db`] type, which gives access to read
//! [`Snapshot`]s and write [`WriteTxn`]s.
//!
//! ```no_run
//! use chrona_core::{Db, EdgeInput, Ts};
//!
//! let db = Db::open("demo.chrona")?;
//!
//! db.write(|w| {
//!     w.upsert_node("alice", Some("person"))?;
//!     w.upsert_node("bob", Some("person"))?;
//!     w.add_edge(EdgeInput {
//!         from: "alice".into(),
//!         to: "bob".into(),
//!         edge_type: "WORKS_WITH".into(),
//!         valid_from: Ts::now(),
//!         valid_to: None,
//!         observed_at: Ts::now(),
//!         source: "manual".into(),
//!         confidence: 1.0,
//!         properties: Default::default(),
//!     })?;
//!     Ok(())
//! })?;
//!
//! let snap = db.begin_read()?;
//! let alice_id = snap.get_node_id("alice")?.unwrap();
//! for edge in snap.neighbors_as_of(alice_id, Ts::now())? {
//!     println!("{:?}", edge);
//! }
//! # Ok::<(), chrona_core::Error>(())
//! ```

#![warn(rust_2018_idioms)]

pub mod codec;
pub mod db;
pub mod error;
pub mod graph;
pub mod id;
pub mod props;
pub mod storage;
pub mod temporal;
pub mod time;
pub mod verify;

pub use db::{Db, Snapshot, Stats, WriteTxn};
pub use error::{Error, Result};
pub use graph::{Edge, EdgeInput, EdgeView, Node, NodeInput};
pub use id::{EdgeId, EventId, NodeId, StringId};
pub use props::{PropValue, Props};
pub use temporal::{DiffEntry, DiffSummary, EventKind, EventRecord};
pub use time::Ts;
pub use verify::VerifyReport;
