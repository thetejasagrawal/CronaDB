//! Integration tests for the full `Db` API.
//!
//! These exercise every layer at once: storage, graph, temporal, and the
//! public `Db` / `Snapshot` / `WriteTxn` surface.

use chrona_core::{Db, DiffSummary, EdgeInput, EventKind, PropValue, Props, Ts};
use tempfile::TempDir;

fn tempdb() -> (TempDir, Db) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.chrona");
    let db = Db::open(&path).unwrap();
    (dir, db)
}

fn ts(s: &str) -> Ts {
    Ts::parse(s).unwrap()
}

#[test]
fn open_create_reopen() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("foo.chrona");

    let db1 = Db::open(&path).unwrap();
    assert_eq!(db1.path(), path);
    drop(db1);

    // Reopen the same file.
    let db2 = Db::open(&path).unwrap();
    let stats = db2.begin_read().unwrap().stats().unwrap();
    assert_eq!(stats.node_count, 0);
}

#[test]
fn upsert_node_is_idempotent() {
    let (_d, db) = tempdb();
    let a = db
        .write(|w| w.upsert_node("alice", Some("person")))
        .unwrap();
    let a2 = db
        .write(|w| w.upsert_node("alice", Some("person")))
        .unwrap();
    assert_eq!(a, a2);
}

#[test]
fn add_edge_and_read_neighbors() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.upsert_node("alice", Some("person"))?;
        w.upsert_node("bob", Some("person"))?;
        w.add_edge(EdgeInput {
            from: "alice".into(),
            to: "bob".into(),
            edge_type: "KNOWS".into(),
            valid_from: ts("2026-01-01"),
            valid_to: None,
            observed_at: ts("2026-01-01"),
            source: "manual".into(),
            confidence: 0.9,
            properties: Default::default(),
        })?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let alice = snap.get_node_id("alice").unwrap().unwrap();
    let edges = snap.neighbors_as_of(alice, ts("2026-01-15")).unwrap();
    assert_eq!(edges.len(), 1);
    let bob = snap.get_node_id("bob").unwrap().unwrap();
    assert_eq!(edges[0].to, bob);
}

#[test]
fn as_of_filters_pre_valid_from() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput {
            from: "a".into(),
            to: "b".into(),
            edge_type: "T".into(),
            valid_from: ts("2026-02-01"),
            valid_to: None,
            observed_at: ts("2026-02-01"),
            source: "".into(),
            confidence: 1.0,
            properties: Default::default(),
        })?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let a = snap.get_node_id("a").unwrap().unwrap();
    // Before valid_from: no edges.
    assert!(snap
        .neighbors_as_of(a, ts("2026-01-15"))
        .unwrap()
        .is_empty());
    // After valid_from: one edge.
    assert_eq!(snap.neighbors_as_of(a, ts("2026-03-15")).unwrap().len(), 1);
}

#[test]
fn as_of_filters_post_valid_to() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput {
            from: "a".into(),
            to: "b".into(),
            edge_type: "T".into(),
            valid_from: ts("2026-01-01"),
            valid_to: Some(ts("2026-03-01")),
            observed_at: ts("2026-01-01"),
            source: "".into(),
            confidence: 1.0,
            properties: Default::default(),
        })?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let a = snap.get_node_id("a").unwrap().unwrap();
    // Inside validity window.
    assert_eq!(snap.neighbors_as_of(a, ts("2026-02-01")).unwrap().len(), 1);
    // Exactly at valid_to (exclusive upper).
    assert_eq!(snap.neighbors_as_of(a, ts("2026-03-01")).unwrap().len(), 0);
    // After valid_to.
    assert_eq!(snap.neighbors_as_of(a, ts("2026-04-01")).unwrap().len(), 0);
}

#[test]
fn invalidate_edge_retroactively() {
    let (_d, db) = tempdb();

    let edge_id = db
        .write(|w| {
            w.add_edge(EdgeInput {
                from: "a".into(),
                to: "b".into(),
                edge_type: "T".into(),
                valid_from: ts("2026-01-01"),
                valid_to: None,
                observed_at: ts("2026-01-01"),
                source: "".into(),
                confidence: 1.0,
                properties: Default::default(),
            })
        })
        .unwrap();

    // Invalidate mid-2026.
    db.write(|w| w.invalidate_edge(edge_id, ts("2026-06-01")))
        .unwrap();

    let snap = db.begin_read().unwrap();
    let a = snap.get_node_id("a").unwrap().unwrap();

    assert_eq!(snap.neighbors_as_of(a, ts("2026-03-01")).unwrap().len(), 1);
    assert_eq!(snap.neighbors_as_of(a, ts("2026-07-01")).unwrap().len(), 0);

    // Check the edge's valid_to was updated.
    let edge = snap.get_edge(edge_id).unwrap().unwrap();
    assert_eq!(edge.valid_to, Some(ts("2026-06-01")));
}

#[test]
fn supersede_edge_creates_revision_chain() {
    let (_d, db) = tempdb();

    let e1 = db
        .write(|w| {
            w.add_edge(EdgeInput {
                from: "alice".into(),
                to: "company_x".into(),
                edge_type: "WORKS_AT".into(),
                valid_from: ts("2025-01-01"),
                valid_to: None,
                observed_at: ts("2025-01-01"),
                source: "hr".into(),
                confidence: 1.0,
                properties: Default::default(),
            })
        })
        .unwrap();

    let e2 = db
        .write(|w| {
            w.supersede_edge(
                e1,
                EdgeInput {
                    from: "alice".into(),
                    to: "company_y".into(),
                    edge_type: "WORKS_AT".into(),
                    valid_from: ts("2026-02-01"),
                    valid_to: None,
                    observed_at: ts("2026-02-01"),
                    source: "hr".into(),
                    confidence: 1.0,
                    properties: Default::default(),
                },
            )
        })
        .unwrap();

    let snap = db.begin_read().unwrap();
    let alice = snap.get_node_id("alice").unwrap().unwrap();

    // In 2025, alice works at company_x.
    let old = snap.neighbors_as_of(alice, ts("2025-06-01")).unwrap();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].id, e1);

    // In 2026, alice works at company_y (and e2.supersedes = e1).
    let new = snap.neighbors_as_of(alice, ts("2026-06-01")).unwrap();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].id, e2);
    assert_eq!(new[0].supersedes, Some(e1));
}

#[test]
fn n_hops_finds_transitive_as_of() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput::new("a", "b", "T"))?;
        w.add_edge(EdgeInput::new("b", "c", "T"))?;
        w.add_edge(EdgeInput::new("c", "d", "T"))?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let a = snap.get_node_id("a").unwrap().unwrap();
    let hops = snap.n_hops_as_of(a, 2, Ts::now()).unwrap();
    // At 2 hops from a, we see a->b and b->c.
    assert_eq!(hops.len(), 2);
}

#[test]
fn shortest_path_finds_route() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput::new("a", "b", "T"))?;
        w.add_edge(EdgeInput::new("b", "c", "T"))?;
        w.add_edge(EdgeInput::new("c", "d", "T"))?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let a = snap.get_node_id("a").unwrap().unwrap();
    let d = snap.get_node_id("d").unwrap().unwrap();
    let path = snap.path_as_of(a, d, Ts::now()).unwrap().unwrap();
    assert_eq!(path.len(), 3);
}

#[test]
fn events_between_range_scan() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput::new("a", "b", "T"))?;
        w.add_edge(EdgeInput::new("b", "c", "T"))?;
        w.add_edge(EdgeInput::new("c", "d", "T"))?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let all = snap.events_between(Ts::MIN, Ts::MAX).unwrap();
    // Each add_edge emits a NodeAdded * 2 + EdgeObserved * 1.
    // Plus 3 add_edges = 3 * (2+1) = 9 events, minus duplicates for shared
    // endpoints: a, b, c, d = 4 nodes + 3 edges = 7 events total.
    assert_eq!(all.len(), 7);
    let edges_observed = all
        .iter()
        .filter(|e| e.kind == EventKind::EdgeObserved)
        .count();
    assert_eq!(edges_observed, 3);
}

#[test]
fn diff_summary_counts_changes() {
    let (_d, db) = tempdb();

    db.write(|w| {
        w.add_edge(EdgeInput::new("a", "b", "T"))?;
        Ok(())
    })
    .unwrap();

    let snap = db.begin_read().unwrap();
    let diff: DiffSummary = snap.diff_between(Ts::MIN, Ts::MAX).unwrap();
    assert_eq!(diff.nodes_added, 2); // a, b
    assert_eq!(diff.edges_added, 1);
}

#[test]
fn stats_after_inserts() {
    let (_d, db) = tempdb();
    db.write(|w| {
        w.add_edge(EdgeInput::new("a", "b", "T"))?;
        w.add_edge(EdgeInput::new("b", "c", "T"))?;
        Ok(())
    })
    .unwrap();
    let snap = db.begin_read().unwrap();
    let s = snap.stats().unwrap();
    assert_eq!(s.node_count, 3); // a, b, c
    assert_eq!(s.edge_count, 2);
    assert!(s.string_count >= 1); // at least "T" interned
}

#[test]
fn view_edge_resolves_strings() {
    let (_d, db) = tempdb();
    let eid = db
        .write(|w| {
            w.add_edge(EdgeInput {
                from: "alice".into(),
                to: "bob".into(),
                edge_type: "KNOWS".into(),
                valid_from: ts("2026-01-01"),
                valid_to: None,
                observed_at: ts("2026-01-01"),
                source: "email".into(),
                confidence: 0.8,
                properties: Default::default(),
            })
        })
        .unwrap();

    let snap = db.begin_read().unwrap();
    let edge = snap.get_edge(eid).unwrap().unwrap();
    let view = snap.view_edge(&edge).unwrap();
    assert_eq!(view.edge_type, "KNOWS");
    assert_eq!(view.source, "email");
    assert_eq!(view.from_ext_id, "alice");
    assert_eq!(view.to_ext_id, "bob");
}

#[test]
fn confidence_out_of_range_rejected() {
    let (_d, db) = tempdb();
    let r = db.write(|w| {
        w.add_edge(EdgeInput {
            from: "a".into(),
            to: "b".into(),
            edge_type: "T".into(),
            valid_from: Ts::now(),
            valid_to: None,
            observed_at: Ts::now(),
            source: "".into(),
            confidence: 1.5,
            properties: Default::default(),
        })
    });
    assert!(r.is_err());
}

#[test]
fn props_roundtrip_through_edge() {
    let (_d, db) = tempdb();
    let mut props = Props::new();
    props.insert("channel".into(), PropValue::String("general".into()));
    props.insert("score".into(), PropValue::Float(0.42));

    let eid = db
        .write(|w| {
            w.add_edge(EdgeInput {
                from: "a".into(),
                to: "b".into(),
                edge_type: "T".into(),
                valid_from: ts("2026-01-01"),
                valid_to: None,
                observed_at: ts("2026-01-01"),
                source: "".into(),
                confidence: 1.0,
                properties: props.clone(),
            })
        })
        .unwrap();

    let snap = db.begin_read().unwrap();
    let e = snap.get_edge(eid).unwrap().unwrap();
    assert_eq!(e.props, props);
}

#[test]
fn abort_discards_writes() {
    let (_d, db) = tempdb();
    let mut w = db.begin_write().unwrap();
    w.upsert_node("alice", None).unwrap();
    w.abort();

    let snap = db.begin_read().unwrap();
    assert_eq!(snap.stats().unwrap().node_count, 0);
}

#[test]
fn reopen_preserves_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("persist.chrona");
    let eid = {
        let db = Db::open(&path).unwrap();
        db.write(|w| w.add_edge(EdgeInput::new("alice", "bob", "KNOWS")))
            .unwrap()
    };

    // Reopen; everything should still be there.
    let db = Db::open(&path).unwrap();
    let snap = db.begin_read().unwrap();
    assert!(snap.get_edge(eid).unwrap().is_some());
    assert!(snap.get_node_id("alice").unwrap().is_some());
}

#[test]
fn reject_nonexistent_file_with_unknown_contents() {
    // If we write random bytes and try to open as chrona, expect format error.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("junk.chrona");
    std::fs::write(&path, b"not a redb file").unwrap();
    assert!(Db::open(&path).is_err());
}
