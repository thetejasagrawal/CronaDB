//! Query executor: takes an AST plus a read snapshot and produces a result.

use crate::ast::{Filter, Query, TimeClause};
use crate::filter::{apply_limit, matches};
use chrona_core::{DiffSummary, EdgeView, Error, Snapshot, Ts};

/// The result of executing a query.
#[derive(Clone, Debug)]
pub enum QueryResult {
    /// A list of edges (for `FIND NEIGHBORS`, `FIND HOPS`, `WHO CONNECTED`).
    Edges(Vec<EdgeView>),
    /// A single path (for `SHOW PATH`), or `None` if no path was found.
    Path(Option<Vec<EdgeView>>),
    /// A structured diff summary (for `DIFF GRAPH` and `WHAT CHANGED`).
    Diff(DiffSummary),
}

impl QueryResult {
    /// Number of top-level rows / entries in the result.
    pub fn len(&self) -> usize {
        match self {
            QueryResult::Edges(v) => v.len(),
            QueryResult::Path(Some(v)) => v.len(),
            QueryResult::Path(None) => 0,
            QueryResult::Diff(d) => d.entries.len(),
        }
    }

    /// True if the result contains no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn resolve_node(snap: &Snapshot, ext_id: &str) -> Result<chrona_core::NodeId, Error> {
    snap.get_node_id(ext_id)?
        .ok_or_else(|| Error::NotFound(format!("node {:?}", ext_id)))
}

fn resolve_time(clause: &Option<TimeClause>, default: Ts) -> Result<Ts, Error> {
    match clause {
        None => Ok(default),
        Some(TimeClause::At(s)) => Ts::parse(s),
        Some(TimeClause::Before(s)) => Ts::parse(s),
        Some(TimeClause::After(s)) => Ts::parse(s),
    }
}

fn materialize_edges(
    snap: &Snapshot,
    edges: Vec<chrona_core::Edge>,
) -> Result<Vec<EdgeView>, Error> {
    let mut out = Vec::with_capacity(edges.len());
    for e in edges {
        out.push(snap.view_edge(&e)?);
    }
    Ok(out)
}

fn apply_filter(views: Vec<EdgeView>, filter: &Filter) -> Result<Vec<EdgeView>, Error> {
    if filter.is_empty() {
        return Ok(views);
    }
    let mut out = Vec::with_capacity(views.len());
    for v in views {
        if matches(filter, &v)? {
            out.push(v);
        }
    }
    Ok(out)
}

/// Execute a query against a read snapshot.
pub fn execute(snap: &Snapshot, query: Query) -> Result<QueryResult, Error> {
    match query {
        Query::Neighbors {
            node,
            time,
            filter,
            limit,
        } => {
            let nid = resolve_node(snap, &node)?;
            let t = resolve_time(&time, Ts::now())?;
            let edges = snap.neighbors_as_of(nid, t)?;
            let views = materialize_edges(snap, edges)?;
            let filtered = apply_filter(views, &filter)?;
            Ok(QueryResult::Edges(apply_limit(filtered, limit)))
        }

        Query::Hops {
            hops,
            node,
            time,
            filter,
            limit,
        } => {
            let nid = resolve_node(snap, &node)?;
            let t = resolve_time(&time, Ts::now())?;
            let edges = snap.n_hops_as_of(nid, hops, t)?;
            let views = materialize_edges(snap, edges)?;
            let filtered = apply_filter(views, &filter)?;
            Ok(QueryResult::Edges(apply_limit(filtered, limit)))
        }

        Query::Path {
            from,
            to,
            time,
            filter,
            limit,
        } => {
            let src = resolve_node(snap, &from)?;
            let dst = resolve_node(snap, &to)?;
            let t = resolve_time(&time, Ts::now())?;
            let p = snap.path_as_of(src, dst, t)?;
            let materialized = match p {
                Some(edges) => Some(materialize_edges(snap, edges)?),
                None => None,
            };
            // Filter / limit only meaningful if path exists.
            let result = match materialized {
                Some(views) => {
                    let filtered = apply_filter(views, &filter)?;
                    Some(apply_limit(filtered, limit))
                }
                None => None,
            };
            Ok(QueryResult::Path(result))
        }

        Query::WhoConnected {
            node,
            on,
            filter,
            limit,
        } => {
            let nid = resolve_node(snap, &node)?;
            let t = Ts::parse(&on)?;
            let mut edges = snap.neighbors_as_of(nid, t)?;
            let rev = snap.reverse_neighbors_as_of(nid, t)?;
            edges.extend(rev);
            let views = materialize_edges(snap, edges)?;
            let filtered = apply_filter(views, &filter)?;
            Ok(QueryResult::Edges(apply_limit(filtered, limit)))
        }

        Query::Diff { t1, t2, node } => {
            let t1 = Ts::parse(&t1)?;
            let t2 = Ts::parse(&t2)?;
            // `FOR NODE` filter in v1: we still scan the global event log; the
            // diff summary captures every event. A future index could filter
            // by node efficiently.
            let _ = node;
            let summary = snap.diff_between(t1, t2)?;
            Ok(QueryResult::Diff(summary))
        }

        Query::Changed { t1, t2, node } => {
            let t1 = Ts::parse(&t1)?;
            let t2 = Ts::parse(&t2)?;
            let _ = node;
            let summary = snap.diff_between(t1, t2)?;
            Ok(QueryResult::Diff(summary))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use chrona_core::{Db, EdgeInput};
    use tempfile::TempDir;

    fn ts(s: &str) -> Ts {
        Ts::parse(s).unwrap()
    }

    fn build_sample() -> (TempDir, Db) {
        let dir = TempDir::new().unwrap();
        let db = Db::open(dir.path().join("t.chrona")).unwrap();
        db.write(|w| {
            w.add_edge(EdgeInput {
                from: "alice".into(),
                to: "bob".into(),
                edge_type: "WORKS_WITH".into(),
                valid_from: ts("2026-01-15"),
                valid_to: None,
                observed_at: ts("2026-01-15"),
                source: "slack".into(),
                confidence: 0.9,
                properties: Default::default(),
            })?;
            w.add_edge(EdgeInput {
                from: "carol".into(),
                to: "alice".into(),
                edge_type: "REPORTS_TO".into(),
                valid_from: ts("2026-02-01"),
                valid_to: None,
                observed_at: ts("2026-02-01"),
                source: "hr".into(),
                confidence: 1.0,
                properties: Default::default(),
            })?;
            w.add_edge(EdgeInput {
                from: "dan".into(),
                to: "alice".into(),
                edge_type: "ADVISES".into(),
                valid_from: ts("2026-01-01"),
                valid_to: Some(ts("2026-03-15")),
                observed_at: ts("2026-01-01"),
                source: "email".into(),
                confidence: 0.7,
                properties: Default::default(),
            })?;
            Ok(())
        })
        .unwrap();
        (dir, db)
    }

    #[test]
    fn exec_find_neighbors() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"FIND NEIGHBORS OF "alice""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_who_was_connected_includes_both_directions() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"WHO WAS CONNECTED TO "alice" ON "2026-02-15""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_who_was_connected_respects_validity() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"WHO WAS CONNECTED TO "alice" ON "2026-04-01""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_path_finds_route() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"SHOW PATH FROM "carol" TO "bob" AT "2026-03-01""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Path(Some(v)) => assert_eq!(v.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_diff_counts() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"DIFF GRAPH BETWEEN "2000-01-01" AND "2099-01-01""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Diff(d) => {
                assert!(d.nodes_added >= 4);
                assert_eq!(d.edges_added, 3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_unknown_node_errors() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"FIND NEIGHBORS OF "ghost""#).unwrap();
        assert!(execute(&snap, q).is_err());
    }

    #[test]
    fn exec_hops_two_levels() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"FIND 2 HOPS FROM "carol" AT "2026-03-01""#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn exec_where_filters_type() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"WHO WAS CONNECTED TO "alice" ON "2026-02-15" WHERE type = "WORKS_WITH""#)
            .unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].edge_type, "WORKS_WITH");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn exec_where_filters_confidence() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"WHO WAS CONNECTED TO "alice" ON "2026-02-15" WHERE confidence >= 0.9"#)
            .unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => {
                // WORKS_WITH (0.9) and REPORTS_TO (1.0); not ADVISES (0.7)
                assert_eq!(v.len(), 2);
                assert!(v.iter().all(|e| e.confidence >= 0.9));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn exec_limit_truncates() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(r#"WHO WAS CONNECTED TO "alice" ON "2026-02-15" LIMIT 1"#).unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 1),
            _ => panic!(),
        }
    }

    #[test]
    fn exec_where_and_limit() {
        let (_d, db) = build_sample();
        let snap = db.begin_read().unwrap();
        let q = parse(
            r#"WHO WAS CONNECTED TO "alice" ON "2026-02-15" WHERE confidence >= 0.8 LIMIT 10"#,
        )
        .unwrap();
        let r = execute(&snap, q).unwrap();
        match r {
            QueryResult::Edges(v) => assert_eq!(v.len(), 2),
            _ => panic!(),
        }
    }
}
