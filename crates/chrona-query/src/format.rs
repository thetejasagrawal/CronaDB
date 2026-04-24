//! Human-readable and JSON formatting of query results.
//!
//! The CLI and REPL use [`render`] for terminal display and [`render_json`]
//! when `--json` is requested. Library users who want structured access
//! should inspect `QueryResult` directly.

use crate::exec::QueryResult;
use chrona_core::{DiffEntry, EdgeView, PropValue, Props};
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

/// Render a query result as single-line JSON.
///
/// The shape is stable enough for scripting:
///
/// ```json
/// {"kind": "edges", "count": 2, "edges": [...]}
/// {"kind": "path", "found": true, "edges": [...]}
/// {"kind": "diff", "summary": {...}, "entries": [...]}
/// ```
pub fn render_json(result: &QueryResult) -> String {
    let mut out = String::with_capacity(256);
    match result {
        QueryResult::Edges(v) => {
            out.push_str("{\"kind\":\"edges\",\"count\":");
            write!(&mut out, "{}", v.len()).unwrap();
            out.push_str(",\"edges\":[");
            write_edge_array(&mut out, v);
            out.push_str("]}");
        }
        QueryResult::Path(Some(edges)) => {
            out.push_str("{\"kind\":\"path\",\"found\":true,\"edges\":[");
            write_edge_array(&mut out, edges);
            out.push_str("]}");
        }
        QueryResult::Path(None) => {
            out.push_str("{\"kind\":\"path\",\"found\":false,\"edges\":[]}");
        }
        QueryResult::Diff(d) => {
            out.push_str("{\"kind\":\"diff\",\"summary\":");
            write_diff_summary(&mut out, d);
            out.push_str(",\"entries\":[");
            let mut first = true;
            for entry in &d.entries {
                if !first {
                    out.push(',');
                }
                first = false;
                write_diff_entry(&mut out, entry);
            }
            out.push_str("]}");
        }
    }
    out.push('\n');
    out
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

// ---- JSON ----

fn write_edge_array(out: &mut String, edges: &[EdgeView]) {
    let mut first = true;
    for e in edges {
        if !first {
            out.push(',');
        }
        first = false;
        write_edge_json(out, e);
    }
}

fn write_edge_json(out: &mut String, e: &EdgeView) {
    out.push('{');
    write!(out, "\"id\":{}", e.id.raw()).unwrap();
    write!(out, ",\"from\":").unwrap();
    write_json_string(out, &e.from_ext_id);
    write!(out, ",\"to\":").unwrap();
    write_json_string(out, &e.to_ext_id);
    write!(out, ",\"type\":").unwrap();
    write_json_string(out, &e.edge_type);
    write!(out, ",\"valid_from\":").unwrap();
    write_json_string(out, &e.valid_from.to_rfc3339());
    write!(out, ",\"valid_to\":").unwrap();
    match e.valid_to {
        Some(t) => write_json_string(out, &t.to_rfc3339()),
        None => out.push_str("null"),
    }
    write!(out, ",\"observed_at\":").unwrap();
    write_json_string(out, &e.observed_at.to_rfc3339());
    write!(out, ",\"source\":").unwrap();
    write_json_string(out, &e.source);
    write!(out, ",\"confidence\":{}", e.confidence).unwrap();
    if let Some(sup) = e.supersedes {
        write!(out, ",\"supersedes\":{}", sup.raw()).unwrap();
    } else {
        out.push_str(",\"supersedes\":null");
    }
    if !e.properties.is_empty() {
        out.push_str(",\"properties\":");
        write_props_json(out, &e.properties);
    }
    out.push('}');
}

fn write_diff_summary(out: &mut String, d: &chrona_core::DiffSummary) {
    write!(
        out,
        "{{\"nodes_added\":{},\"nodes_removed\":{},\"edges_added\":{},\
         \"edges_invalidated\":{},\"edges_superseded\":{},\"properties_updated\":{}}}",
        d.nodes_added,
        d.nodes_removed,
        d.edges_added,
        d.edges_invalidated,
        d.edges_superseded,
        d.properties_updated
    )
    .unwrap();
}

fn write_diff_entry(out: &mut String, entry: &DiffEntry) {
    match entry {
        DiffEntry::NodeAdded { at, event, .. } => {
            out.push_str("{\"kind\":\"node_added\",\"at\":");
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
        DiffEntry::NodeRemoved { node, at, event } => {
            write!(
                out,
                "{{\"kind\":\"node_removed\",\"node\":{},\"at\":",
                node.raw()
            )
            .unwrap();
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
        DiffEntry::EdgeAdded { at, event, .. } => {
            out.push_str("{\"kind\":\"edge_added\",\"at\":");
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
        DiffEntry::EdgeInvalidated { edge, at, event } => {
            write!(
                out,
                "{{\"kind\":\"edge_invalidated\",\"edge\":{},\"at\":",
                edge.raw()
            )
            .unwrap();
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
        DiffEntry::EdgeSuperseded { at, event, .. } => {
            out.push_str("{\"kind\":\"edge_superseded\",\"at\":");
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
        DiffEntry::PropertySet { at, event } => {
            out.push_str("{\"kind\":\"property_set\",\"at\":");
            write_json_string(out, &at.to_rfc3339());
            write!(out, ",\"event\":{}}}", event.raw()).unwrap();
        }
    }
}

fn write_props_json(out: &mut String, p: &Props) {
    out.push('{');
    let mut first = true;
    for (k, v) in p {
        if !first {
            out.push(',');
        }
        first = false;
        write_json_string(out, k);
        out.push(':');
        match v {
            PropValue::Null => out.push_str("null"),
            PropValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            PropValue::Int(n) => write!(out, "{}", n).unwrap(),
            PropValue::Float(f) => {
                if f.is_finite() {
                    write!(out, "{}", f).unwrap();
                } else {
                    out.push_str("null");
                }
            }
            PropValue::String(s) => write_json_string(out, s),
            PropValue::Bytes(_) => out.push_str("\"<bytes>\""),
        }
    }
    out.push('}');
}

/// Minimal JSON string escaper: handles quotes, backslashes, control chars.
fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrona_core::{EdgeId, NodeId, Ts};

    fn sample_edge() -> EdgeView {
        EdgeView {
            id: EdgeId::from_raw(7),
            from: NodeId::from_raw(1),
            from_ext_id: "alice".into(),
            to: NodeId::from_raw(2),
            to_ext_id: "bob".into(),
            edge_type: "KNOWS".into(),
            valid_from: Ts::parse("2026-01-01").unwrap(),
            valid_to: None,
            observed_at: Ts::parse("2026-01-01").unwrap(),
            source: "slack".into(),
            confidence: 0.9,
            supersedes: None,
            properties: Props::new(),
        }
    }

    #[test]
    fn render_empty_edges() {
        let r = QueryResult::Edges(vec![]);
        assert_eq!(render(&r), "(no edges)\n");
    }

    #[test]
    fn render_json_edges() {
        let r = QueryResult::Edges(vec![sample_edge()]);
        let j = render_json(&r);
        assert!(j.starts_with("{\"kind\":\"edges\",\"count\":1"));
        assert!(j.contains("\"from\":\"alice\""));
        assert!(j.contains("\"type\":\"KNOWS\""));
    }

    #[test]
    fn render_json_path_not_found() {
        let r = QueryResult::Path(None);
        assert_eq!(
            render_json(&r),
            "{\"kind\":\"path\",\"found\":false,\"edges\":[]}\n"
        );
    }

    #[test]
    fn render_json_path_found() {
        let r = QueryResult::Path(Some(vec![sample_edge()]));
        let j = render_json(&r);
        assert!(j.contains("\"found\":true"));
        assert!(j.contains("\"edges\":["));
    }

    #[test]
    fn json_string_escapes_quotes() {
        let mut out = String::new();
        write_json_string(&mut out, "a\"b\\c\nd");
        assert_eq!(out, "\"a\\\"b\\\\c\\nd\"");
    }
}
