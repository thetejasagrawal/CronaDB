//! Human-readable formatting of query results.
//!
//! The CLI and REPL use this to present results. Library users who want
//! structured access should inspect `QueryResult` directly.

use crate::exec::QueryResult;
use chrona_core::{DiffEntry, EdgeView};
use std::fmt::Write;

/// Render a query result as a text string suitable for terminal display.
pub fn render(result: &QueryResult) -> String {
    match result {
        QueryResult::Edges(v) => render_edges(v),
        QueryResult::Path(Some(v)) => render_path(v),
        QueryResult::Path(None) => "(no path)\n".to_string(),
        QueryResult::Diff(d) => render_diff(d),
    }
}

fn render_edges(edges: &[EdgeView]) -> String {
    if edges.is_empty() {
        return "(no edges)\n".into();
    }
    let mut out = String::new();
    for e in edges {
        writeln!(
            &mut out,
            "{:<12} -[{}]-> {:<12}  valid=[{}..{})  obs={}  src={}  conf={:.2}",
            e.from_ext_id,
            e.edge_type,
            e.to_ext_id,
            e.valid_from,
            e.valid_to
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "open".to_string()),
            e.observed_at,
            if e.source.is_empty() { "-" } else { &e.source },
            e.confidence,
        )
        .unwrap();
    }
    out
}

fn render_path(edges: &[EdgeView]) -> String {
    if edges.is_empty() {
        return "(source == target)\n".into();
    }
    let mut out = String::new();
    write!(&mut out, "{}", edges[0].from_ext_id).unwrap();
    for e in edges {
        write!(&mut out, " -[{}]-> {}", e.edge_type, e.to_ext_id).unwrap();
    }
    out.push('\n');
    out
}

fn render_diff(d: &chrona_core::DiffSummary) -> String {
    let mut out = String::new();
    writeln!(
        &mut out,
        "summary: +{} nodes, -{} nodes, +{} edges, !{} invalidated, ~{} superseded",
        d.nodes_added, d.nodes_removed, d.edges_added, d.edges_invalidated, d.edges_superseded
    )
    .unwrap();
    for entry in &d.entries {
        match entry {
            DiffEntry::NodeAdded { at, event, .. } => {
                writeln!(&mut out, "+ node  @{}  (ev {})", at, event).unwrap()
            }
            DiffEntry::NodeRemoved { node, at, event } => {
                writeln!(&mut out, "- node  {}  @{}  (ev {})", node, at, event).unwrap()
            }
            DiffEntry::EdgeAdded { at, event, .. } => {
                writeln!(&mut out, "+ edge  @{}  (ev {})", at, event).unwrap()
            }
            DiffEntry::EdgeInvalidated { edge, at, event } => {
                writeln!(&mut out, "! edge  {}  @{}  (ev {})", edge, at, event).unwrap()
            }
            DiffEntry::EdgeSuperseded { at, event, .. } => {
                writeln!(&mut out, "~ edge  @{}  (ev {})", at, event).unwrap()
            }
            DiffEntry::PropertySet { at, event } => {
                writeln!(&mut out, "P prop  @{}  (ev {})", at, event).unwrap()
            }
        }
    }
    out
}
