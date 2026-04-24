//! Example: using Chrona as agent memory.
//!
//! An LLM-style agent builds up beliefs about the world, revises them, and
//! sometimes invalidates them. Chrona's temporal model is a natural fit.
//!
//! Run with:
//! ```text
//! cargo run --example agent_memory -p chrona-core
//! ```

use chrona_core::{Db, EdgeInput, PropValue, Props, Ts};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let path = dir.path().join("agent.chrona");
    let db = Db::open(&path)?;

    println!("Agent Memory Example");
    println!("====================");
    println!("Database: {}\n", path.display());

    let t0 = Ts::parse("2026-01-15T10:00:00Z")?;
    let t1 = Ts::parse("2026-02-01T10:00:00Z")?;
    let t2 = Ts::parse("2026-03-15T10:00:00Z")?;

    // Step 1: Agent initially believes Alice works at AcmeCorp (inferred from email).
    let e1 = db.write(|w| {
        let mut props = Props::new();
        props.insert(
            "email_sender".into(),
            PropValue::String("alice@acme.com".into()),
        );
        w.add_edge(EdgeInput {
            from: "alice".into(),
            to: "AcmeCorp".into(),
            edge_type: "WORKS_AT".into(),
            valid_from: t0,
            valid_to: None,
            observed_at: t0,
            source: "email_analysis".into(),
            confidence: 0.65,
            properties: props,
        })
    })?;
    println!("[t0] Inferred: alice WORKS_AT AcmeCorp (conf=0.65, from email)");

    // Step 2: Agent reads LinkedIn; confidence rises. Supersede with higher
    // confidence.
    let e2 = db.write(|w| {
        w.supersede_edge(
            e1,
            EdgeInput {
                from: "alice".into(),
                to: "AcmeCorp".into(),
                edge_type: "WORKS_AT".into(),
                valid_from: t1,
                valid_to: None,
                observed_at: t1,
                source: "linkedin".into(),
                confidence: 0.95,
                properties: Props::new(),
            },
        )
    })?;
    println!("[t1] Verified: alice WORKS_AT AcmeCorp (conf=0.95, from LinkedIn). Superseded e1.");

    // Step 3: Alice posts "I'm starting at BetaInc!" — previous belief is wrong
    // from that point forward.
    let e3 = db.write(|w| {
        w.supersede_edge(
            e2,
            EdgeInput {
                from: "alice".into(),
                to: "BetaInc".into(),
                edge_type: "WORKS_AT".into(),
                valid_from: t2,
                valid_to: None,
                observed_at: t2,
                source: "twitter".into(),
                confidence: 0.99,
                properties: Props::new(),
            },
        )
    })?;
    println!("[t2] Revised: alice WORKS_AT BetaInc (conf=0.99, from Twitter). Superseded e2.\n");

    // Now interrogate the belief history.
    let snap = db.begin_read()?;
    let alice = snap.get_node_id("alice")?.unwrap();

    println!("Querying belief state at three different times:\n");

    for (label, when) in [
        ("t0 + 1 day", Ts::parse("2026-01-16T10:00:00Z")?),
        ("t1 + 1 day", Ts::parse("2026-02-02T10:00:00Z")?),
        ("t2 + 1 day", Ts::parse("2026-03-16T10:00:00Z")?),
    ] {
        let edges = snap.neighbors_as_of(alice, when)?;
        println!("At {}:", label);
        if edges.is_empty() {
            println!("  (no beliefs)");
        }
        for e in edges {
            let view = snap.view_edge(&e)?;
            println!(
                "  alice -[{}]-> {}   (src={}, conf={:.2})",
                view.edge_type, view.to_ext_id, view.source, view.confidence
            );
        }
        println!();
    }

    // Walk the revision chain.
    println!("Revision chain for e3 (latest):");
    let mut cur = Some(e3);
    while let Some(id) = cur {
        let edge = snap.get_edge(id)?.unwrap();
        let view = snap.view_edge(&edge)?;
        println!(
            "  {} -> {} ({}) src={} conf={:.2} valid_from={}",
            id, view.to_ext_id, view.edge_type, view.source, view.confidence, view.valid_from
        );
        cur = edge.supersedes;
    }

    Ok(())
}
