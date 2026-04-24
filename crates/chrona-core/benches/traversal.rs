//! Benchmark: adjacency traversal (1-hop) on a synthetic dataset.
//!
//! Run with: `cargo bench -p chrona-core --bench traversal`

use chrona_core::{Db, EdgeInput, Ts};
use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

fn populate_graph(n_nodes: usize, edges_per_node: usize) -> (TempDir, Db) {
    let dir = TempDir::new().unwrap();
    let db = Db::open(dir.path().join("bench.chrona")).unwrap();
    db.write(|w| {
        for i in 0..n_nodes {
            let from = format!("n{}", i);
            for j in 0..edges_per_node {
                let to = format!("n{}", (i + j + 1) % n_nodes);
                w.add_edge(EdgeInput {
                    from: from.clone(),
                    to,
                    edge_type: "E".into(),
                    valid_from: Ts::from_nanos(i as i64 * 1_000_000),
                    valid_to: None,
                    observed_at: Ts::from_nanos(i as i64 * 1_000_000),
                    source: "bench".into(),
                    confidence: 1.0,
                    properties: Default::default(),
                })?;
            }
        }
        Ok(())
    })
    .unwrap();
    (dir, db)
}

fn bench_one_hop(c: &mut Criterion) {
    let (_d, db) = populate_graph(1_000, 5);
    let snap = db.begin_read().unwrap();
    let nid = snap.get_node_id("n500").unwrap().unwrap();

    c.bench_function("one_hop_1k_nodes_5_edges_each", |b| {
        b.iter(|| {
            let edges = snap.neighbors_as_of(nid, Ts::now()).unwrap();
            criterion::black_box(edges);
        });
    });
}

fn bench_two_hop(c: &mut Criterion) {
    let (_d, db) = populate_graph(1_000, 5);
    let snap = db.begin_read().unwrap();
    let nid = snap.get_node_id("n500").unwrap().unwrap();

    c.bench_function("two_hop_1k_nodes", |b| {
        b.iter(|| {
            let edges = snap.n_hops_as_of(nid, 2, Ts::now()).unwrap();
            criterion::black_box(edges);
        });
    });
}

criterion_group!(benches, bench_one_hop, bench_two_hop);
criterion_main!(benches);
