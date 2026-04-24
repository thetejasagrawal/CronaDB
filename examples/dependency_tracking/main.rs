//! Example: dependency tracking across infrastructure changes.
//!
//! Tracks which services depend on which vendors over time, then queries
//! "which dependencies existed before the outage?"
//!
//! Run with:
//! ```text
//! cargo run --example dependency_tracking -p chrona-core
//! ```

use chrona_core::{Db, EdgeInput, Ts};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let db = Db::open(dir.path().join("deps.chrona"))?;

    println!("Dependency Tracking Example");
    println!("===========================\n");

    // Initial architecture as of 2026-01-01.
    db.write(|w| {
        w.add_edge(EdgeInput {
            from: "api-gateway".into(),
            to: "redis-1".into(),
            edge_type: "DEPENDS_ON".into(),
            valid_from: Ts::parse("2026-01-01")?,
            valid_to: None,
            observed_at: Ts::parse("2026-01-01")?,
            source: "terraform".into(),
            confidence: 1.0,
            properties: Default::default(),
        })?;
        w.add_edge(EdgeInput {
            from: "api-gateway".into(),
            to: "postgres-primary".into(),
            edge_type: "DEPENDS_ON".into(),
            valid_from: Ts::parse("2026-01-01")?,
            valid_to: None,
            observed_at: Ts::parse("2026-01-01")?,
            source: "terraform".into(),
            confidence: 1.0,
            properties: Default::default(),
        })?;
        w.add_edge(EdgeInput {
            from: "worker".into(),
            to: "redis-1".into(),
            edge_type: "DEPENDS_ON".into(),
            valid_from: Ts::parse("2026-01-01")?,
            valid_to: None,
            observed_at: Ts::parse("2026-01-01")?,
            source: "terraform".into(),
            confidence: 1.0,
            properties: Default::default(),
        })?;
        Ok(())
    })?;
    println!("[2026-01-01] Initial deps recorded.");

    // Migration: api-gateway moves to redis-2, worker stays on redis-1.
    let old_api_redis = {
        let snap = db.begin_read()?;
        let api = snap.get_node_id("api-gateway")?.unwrap();
        snap.neighbors_as_of(api, Ts::parse("2026-02-15")?)?
            .iter()
            .find(|e| {
                let v = snap.view_edge(e).unwrap();
                v.to_ext_id == "redis-1"
            })
            .map(|e| e.id)
            .unwrap()
    };

    db.write(|w| {
        w.supersede_edge(
            old_api_redis,
            EdgeInput {
                from: "api-gateway".into(),
                to: "redis-2".into(),
                edge_type: "DEPENDS_ON".into(),
                valid_from: Ts::parse("2026-03-01")?,
                valid_to: None,
                observed_at: Ts::parse("2026-03-01")?,
                source: "terraform".into(),
                confidence: 1.0,
                properties: Default::default(),
            },
        )?;
        Ok(())
    })?;
    println!("[2026-03-01] api-gateway migrated from redis-1 to redis-2.");

    // Outage at 2026-03-15.
    println!("\n*** Outage detected at 2026-03-15 involving redis-1 ***\n");

    let snap = db.begin_read()?;

    // Question: what was depending on redis-1 just before the outage?
    let redis1 = snap.get_node_id("redis-1")?.unwrap();
    let before_outage = Ts::parse("2026-03-14T23:59:59Z")?;
    let depending = snap.reverse_neighbors_as_of(redis1, before_outage)?;
    println!("Dependencies on redis-1 as of {}:", before_outage);
    for e in depending {
        let view = snap.view_edge(&e)?;
        println!("  {} (valid since {})", view.from_ext_id, view.valid_from);
    }

    // Compare: what was depending on redis-1 two months earlier?
    let earlier = Ts::parse("2026-01-15T00:00:00Z")?;
    let earlier_depending = snap.reverse_neighbors_as_of(redis1, earlier)?;
    println!("\nDependencies on redis-1 as of {}:", earlier);
    for e in earlier_depending {
        let view = snap.view_edge(&e)?;
        println!("  {} (valid since {})", view.from_ext_id, view.valid_from);
    }

    println!("\nDiff: api-gateway was removed from redis-1's consumers between these dates.");

    Ok(())
}
