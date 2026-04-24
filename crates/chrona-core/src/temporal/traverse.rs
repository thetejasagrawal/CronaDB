//! Traversal primitives: neighbors, n-hops, shortest path.
//!
//! These are built on top of the snapshot-level edge scans. All take an
//! optional "as-of" timestamp; when `None`, the query uses the current clock
//! reading.

use crate::error::Result;
use crate::graph::Edge;
use crate::id::NodeId;
use crate::time::Ts;
use std::collections::{HashMap, HashSet, VecDeque};

/// Interface expected by the traversal algorithms. Implemented by `Snapshot`
/// in `db.rs`.
pub trait EdgeScanner {
    /// Return all edges from `node` that were live at (or at least had
    /// `valid_from <= as_of`) the given timestamp, applying the live filter.
    fn scan_fwd(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>>;

    /// Reverse direction.
    fn scan_rev(&self, node: NodeId, as_of: Ts) -> Result<Vec<Edge>>;
}

/// Return all direct neighbors of `node` alive at `as_of`.
pub fn neighbors<S: EdgeScanner>(scanner: &S, node: NodeId, as_of: Ts) -> Result<Vec<Edge>> {
    scanner.scan_fwd(node, as_of)
}

/// BFS out to `hops` hops, returning every unique edge encountered.
///
/// Deduplicates on `edge_id`. Does not deduplicate on node pairs — you get
/// every edge that satisfies `valid_from <= as_of` from the frontier nodes.
pub fn n_hops<S: EdgeScanner>(
    scanner: &S,
    start: NodeId,
    hops: u8,
    as_of: Ts,
) -> Result<Vec<Edge>> {
    if hops == 0 {
        return Ok(Vec::new());
    }

    let mut visited_nodes: HashSet<NodeId> = HashSet::new();
    let mut seen_edges: HashSet<u64> = HashSet::new();
    let mut result: Vec<Edge> = Vec::new();
    let mut frontier: VecDeque<(NodeId, u8)> = VecDeque::new();

    visited_nodes.insert(start);
    frontier.push_back((start, 0));

    while let Some((node, depth)) = frontier.pop_front() {
        if depth >= hops {
            continue;
        }
        let edges = scanner.scan_fwd(node, as_of)?;
        for e in edges {
            if seen_edges.insert(e.id.raw()) {
                let to = e.to;
                result.push(e);
                if visited_nodes.insert(to) {
                    frontier.push_back((to, depth + 1));
                }
            }
        }
    }

    Ok(result)
}

/// Bidirectional BFS for shortest path from `src` to `dst`.
///
/// Returns the sequence of edges forming the path, or `None` if no path
/// exists within the live edge set at `as_of`.
pub fn shortest_path<S: EdgeScanner>(
    scanner: &S,
    src: NodeId,
    dst: NodeId,
    as_of: Ts,
) -> Result<Option<Vec<Edge>>> {
    if src == dst {
        return Ok(Some(Vec::new()));
    }

    // Parent pointers: node → (previous node, edge taken to reach it).
    let mut parent: HashMap<NodeId, (NodeId, Edge)> = HashMap::new();
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<NodeId> = VecDeque::new();

    visited.insert(src);
    queue.push_back(src);

    while let Some(cur) = queue.pop_front() {
        if cur == dst {
            // Reconstruct.
            let mut path = Vec::new();
            let mut node = cur;
            while let Some((prev, edge)) = parent.remove(&node) {
                path.push(edge);
                node = prev;
                if node == src {
                    break;
                }
            }
            path.reverse();
            return Ok(Some(path));
        }

        let edges = scanner.scan_fwd(cur, as_of)?;
        for e in edges {
            let nxt = e.to;
            if visited.insert(nxt) {
                parent.insert(nxt, (cur, e));
                queue.push_back(nxt);
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Edge;
    use crate::id::{EdgeId, StringId};
    use crate::props::Props;

    struct MockScanner {
        fwd: HashMap<NodeId, Vec<Edge>>,
    }

    impl EdgeScanner for MockScanner {
        fn scan_fwd(&self, node: NodeId, _as_of: Ts) -> Result<Vec<Edge>> {
            Ok(self.fwd.get(&node).cloned().unwrap_or_default())
        }
        fn scan_rev(&self, _node: NodeId, _as_of: Ts) -> Result<Vec<Edge>> {
            Ok(Vec::new())
        }
    }

    fn e(id: u64, from: u64, to: u64) -> Edge {
        Edge {
            id: EdgeId::from_raw(id),
            from: NodeId::from_raw(from),
            to: NodeId::from_raw(to),
            type_id: StringId::from_raw(1),
            valid_from: Ts::EPOCH,
            valid_to: None,
            observed_at: Ts::EPOCH,
            source_id: StringId::from_raw(1),
            confidence: 1.0,
            supersedes: None,
            props: Props::new(),
        }
    }

    fn scanner(edges: Vec<Edge>) -> MockScanner {
        let mut fwd: HashMap<NodeId, Vec<Edge>> = HashMap::new();
        for e in edges {
            fwd.entry(e.from).or_default().push(e);
        }
        MockScanner { fwd }
    }

    #[test]
    fn neighbors_returns_direct() {
        let s = scanner(vec![e(1, 1, 2), e(2, 1, 3), e(3, 2, 4)]);
        let ns = neighbors(&s, NodeId::from_raw(1), Ts::now()).unwrap();
        assert_eq!(ns.len(), 2);
    }

    #[test]
    fn two_hops_finds_transitive() {
        let s = scanner(vec![e(1, 1, 2), e(2, 2, 3), e(3, 3, 4)]);
        let r = n_hops(&s, NodeId::from_raw(1), 2, Ts::now()).unwrap();
        let edge_ids: HashSet<u64> = r.iter().map(|e| e.id.raw()).collect();
        assert!(edge_ids.contains(&1));
        assert!(edge_ids.contains(&2));
        assert!(!edge_ids.contains(&3));
    }

    #[test]
    fn shortest_path_linear() {
        let s = scanner(vec![e(1, 1, 2), e(2, 2, 3), e(3, 3, 4)]);
        let p = shortest_path(&s, NodeId::from_raw(1), NodeId::from_raw(4), Ts::now())
            .unwrap()
            .unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].id.raw(), 1);
        assert_eq!(p[1].id.raw(), 2);
        assert_eq!(p[2].id.raw(), 3);
    }

    #[test]
    fn shortest_path_none() {
        let s = scanner(vec![e(1, 1, 2), e(2, 3, 4)]);
        let p = shortest_path(&s, NodeId::from_raw(1), NodeId::from_raw(4), Ts::now()).unwrap();
        assert!(p.is_none());
    }

    #[test]
    fn shortest_path_picks_shorter_branch() {
        // 1 -> 2 -> 3 -> 4
        // 1 -> 4 (direct)
        let s = scanner(vec![e(1, 1, 2), e(2, 2, 3), e(3, 3, 4), e(4, 1, 4)]);
        let p = shortest_path(&s, NodeId::from_raw(1), NodeId::from_raw(4), Ts::now())
            .unwrap()
            .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id.raw(), 4);
    }
}
