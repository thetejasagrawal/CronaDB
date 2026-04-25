//! Builds a small but interesting demo database.
//!
//! Used by `chrona demo <path>` to seed something a new user can poke at
//! immediately. The graph models a tiny startup over four months of 2026
//! with a reorganization, a job change, and a project launch — picked so
//! that scrolling the time cursor in `chrona tui` shows visible structural
//! change (edges appearing, disappearing, being superseded).

use chrona_core::{Db, EdgeId, EdgeInput, Props, Ts};

use crate::anyhow_like::Result;

/// Seed `db` with the demo graph. Intended to be called on a freshly created
/// file. Running it twice will append duplicate edges, by design (we want
/// `chrona demo` to be a no-magic builder).
pub fn seed(db: &Db) -> Result<()> {
    upsert_nodes(db)?;
    seed_january(db)?;
    seed_february(db)?;
    seed_march_reorg(db)?;
    seed_alice_job_change(db)?;
    seed_april_projects(db)?;
    Ok(())
}

fn upsert_nodes(db: &Db) -> Result<()> {
    let people = ["alice", "bob", "carol", "dan", "eve", "frank"];
    let orgs = ["acme", "beta_labs"];
    let projects = ["pluto", "quasar"];
    db.write(|w| {
        for p in people {
            w.upsert_node(p, Some("person"))?;
        }
        for o in orgs {
            w.upsert_node(o, Some("org"))?;
        }
        for p in projects {
            w.upsert_node(p, Some("project"))?;
        }
        Ok(())
    })?;
    Ok(())
}

fn seed_january(db: &Db) -> Result<()> {
    let t_jan = Ts::parse("2026-01-15T09:00:00Z")?;
    let t_jan_reports = Ts::parse("2026-01-20T09:00:00Z")?;
    let t_jan_collab = Ts::parse("2026-01-25T14:00:00Z")?;

    db.write(|w| {
        // alice, bob, carol, dan all WORKS_AT acme
        for who in ["alice", "bob", "carol", "dan"] {
            w.add_edge(works_at(who, "acme", t_jan, "hr", 1.0))?;
        }
        // bob, carol report to alice initially.
        w.add_edge(reports_to("bob", "alice", t_jan_reports))?;
        w.add_edge(reports_to("carol", "alice", t_jan_reports))?;
        // Slack-derived collaboration edges, lower confidence.
        for (a, b) in [
            ("alice", "bob"),
            ("alice", "carol"),
            ("bob", "carol"),
            ("carol", "dan"),
        ] {
            w.add_edge(works_with(a, b, t_jan_collab, 0.85))?;
        }
        Ok(())
    })?;
    Ok(())
}

fn seed_february(db: &Db) -> Result<()> {
    let t_feb = Ts::parse("2026-02-10T09:00:00Z")?;
    db.write(|w| {
        w.add_edge(works_at("eve", "acme", t_feb, "hr", 1.0))?;
        w.add_edge(reports_to("eve", "alice", t_feb))?;
        Ok(())
    })?;
    Ok(())
}

fn seed_march_reorg(db: &Db) -> Result<()> {
    let t_reorg = Ts::parse("2026-03-01T09:00:00Z")?;
    let just_before = Ts::parse("2026-02-28T23:59:59Z")?;

    // Find carol's REPORTS_TO alice edge first (read transaction), then
    // supersede it in a separate write transaction.
    let carol_reports_alice = {
        let snap = db.begin_read()?;
        let carol = snap
            .get_node_id("carol")?
            .ok_or("carol not found in demo seed")?;
        let mut found = None;
        for e in snap.neighbors_as_of(carol, just_before)? {
            let view = snap.view_edge(&e)?;
            if view.edge_type == "REPORTS_TO" && view.to_ext_id == "alice" {
                found = Some(e.id);
                break;
            }
        }
        found.ok_or("could not locate carol -> alice REPORTS_TO edge")?
    };

    db.write(|w| {
        w.supersede_edge(
            carol_reports_alice,
            EdgeInput {
                from: "carol".into(),
                to: "bob".into(),
                edge_type: "REPORTS_TO".into(),
                valid_from: t_reorg,
                valid_to: None,
                observed_at: t_reorg,
                source: "hr".into(),
                confidence: 1.0,
                properties: Props::new(),
            },
        )?;
        Ok(())
    })?;
    Ok(())
}

fn seed_alice_job_change(db: &Db) -> Result<()> {
    let t_alice_leaves = Ts::parse("2026-03-15T17:00:00Z")?;
    let t_alice_joins = Ts::parse("2026-03-16T09:00:00Z")?;

    // Find alice's WORKS_AT acme edge in a read transaction, then mutate.
    let alice_at_acme: Option<EdgeId> = {
        let snap = db.begin_read()?;
        let alice = snap
            .get_node_id("alice")?
            .ok_or("alice not found in demo seed")?;
        let mut found = None;
        for e in snap.neighbors_as_of(alice, t_alice_leaves)? {
            let view = snap.view_edge(&e)?;
            if view.edge_type == "WORKS_AT" && view.to_ext_id == "acme" {
                found = Some(e.id);
                break;
            }
        }
        found
    };

    db.write(|w| {
        if let Some(id) = alice_at_acme {
            w.invalidate_edge(id, t_alice_leaves)?;
        }
        w.add_edge(works_at(
            "alice",
            "beta_labs",
            t_alice_joins,
            "linkedin",
            0.95,
        ))?;
        Ok(())
    })?;
    Ok(())
}

fn seed_april_projects(db: &Db) -> Result<()> {
    let t_pluto = Ts::parse("2026-04-01T10:00:00Z")?;
    let t_quasar = Ts::parse("2026-04-05T10:00:00Z")?;

    db.write(|w| {
        for who in ["alice", "bob", "carol"] {
            w.add_edge(works_on(who, "pluto", t_pluto, 0.99))?;
        }
        // Frank starts at acme, then joins quasar with dan.
        w.add_edge(works_at("frank", "acme", t_quasar, "hr", 1.0))?;
        for who in ["dan", "frank"] {
            w.add_edge(works_on(who, "quasar", t_quasar, 0.99))?;
        }
        Ok(())
    })?;
    Ok(())
}

// ---- helpers ----

fn works_at(person: &str, org: &str, t: Ts, source: &str, confidence: f32) -> EdgeInput {
    EdgeInput {
        from: person.into(),
        to: org.into(),
        edge_type: "WORKS_AT".into(),
        valid_from: t,
        valid_to: None,
        observed_at: t,
        source: source.into(),
        confidence,
        properties: Props::new(),
    }
}

fn reports_to(from: &str, to: &str, t: Ts) -> EdgeInput {
    EdgeInput {
        from: from.into(),
        to: to.into(),
        edge_type: "REPORTS_TO".into(),
        valid_from: t,
        valid_to: None,
        observed_at: t,
        source: "hr".into(),
        confidence: 1.0,
        properties: Props::new(),
    }
}

fn works_with(a: &str, b: &str, t: Ts, confidence: f32) -> EdgeInput {
    EdgeInput {
        from: a.into(),
        to: b.into(),
        edge_type: "WORKS_WITH".into(),
        valid_from: t,
        valid_to: None,
        observed_at: t,
        source: "slack".into(),
        confidence,
        properties: Props::new(),
    }
}

fn works_on(person: &str, project: &str, t: Ts, confidence: f32) -> EdgeInput {
    EdgeInput {
        from: person.into(),
        to: project.into(),
        edge_type: "WORKS_ON".into(),
        valid_from: t,
        valid_to: None,
        observed_at: t,
        source: "github".into(),
        confidence,
        properties: Props::new(),
    }
}

/// A short list of suggested queries to print after seeding. Each one is
/// designed to make the temporal model visible without requiring much setup.
pub fn suggested_queries() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "alice's neighbors right now (after she joined beta_labs)",
            r#"FIND NEIGHBORS OF "alice""#,
        ),
        (
            "alice's neighbors mid-February (still at acme)",
            r#"FIND NEIGHBORS OF "alice" AT "2026-02-15""#,
        ),
        (
            "carol's reporting line as of Feb (reports to alice)",
            r#"WHO WAS CONNECTED TO "carol" ON "2026-02-15""#,
        ),
        (
            "carol's reporting line mid-March (reports to bob, post-reorg)",
            r#"WHO WAS CONNECTED TO "carol" ON "2026-03-15""#,
        ),
        (
            "high-confidence collaborators of alice today",
            r#"FIND NEIGHBORS OF "alice" WHERE confidence >= 0.9 LIMIT 10"#,
        ),
        (
            "two-hop reach from acme right now",
            r#"FIND 2 HOPS FROM "acme""#,
        ),
        (
            "shortest collaboration path from alice to dan",
            r#"SHOW PATH FROM "alice" TO "dan""#,
        ),
    ]
}
