//! # chrona-query
//!
//! The query language and executor for Chrona. Turns DSL strings like
//! `FIND 2 HOPS FROM "alice" AT "2026-03-01"` into operations on a
//! [`chrona_core::Snapshot`].
//!
//! ```no_run
//! use chrona_core::Db;
//! use chrona_query::{parse, execute, render};
//!
//! let db = Db::open("demo.chrona")?;
//! let snap = db.begin_read()?;
//! let ast = parse(r#"FIND NEIGHBORS OF "alice""#)?;
//! let result = execute(&snap, ast)?;
//! println!("{}", render(&result));
//! # Ok::<(), chrona_core::Error>(())
//! ```

#![warn(rust_2018_idioms)]

pub mod ast;
pub mod exec;
pub mod format;
pub mod lexer;
pub mod parser;

pub use ast::{Query, TimeClause};
pub use exec::{execute, QueryResult};
pub use format::render;
pub use parser::parse;
