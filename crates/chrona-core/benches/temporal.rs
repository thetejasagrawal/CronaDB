//! Benchmark: temporal (as-of) queries and diffs.

use chrona_core::{Db, EdgeInput, Ts};
use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

fn populate_temporal(n_nodes: usize, versions_per_edge: usize) -> (TempDir, Db) {
    let dir = TempDir::new().unwrap();
    let db = Db::open(dir.path().join("t.chrona")).unwrap();
    db.write(|w| {
        for i in 0..n_nodes {
            let from = format!("n{}", i);
            let to = format!("n{}", (i + 1) % n_nodes);
            for v in 0..versions_per_edge {
                w.add_edge(EdgeInput {
                    from: from.clone(),
                    to: to.clone(),
                    edge_type: "E".into(),
                    valid_from: Ts::from_nanos((v as i64) * 1_000_000_000),
                    valid_to: Some(Ts::from_nanos(((v + 1) as i64) * 1_000_000_000)),
                    observed_at: Ts::from_nanos((v as i64) * 1_000_000_000),
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

fn bench_as_of(c: &mut Criterion) {
    let (_d, db) = populate_temporal(500, 10);
    let snap = db.begin_read().unwrap();
    let nid = snap.get_node_id("n100").unwrap().unwrap();
    // Ask for the state at a mid-range time.
    let t = Ts::from_nanos(5 * 1_000_000_000);

    c.bench_function("neighbors_as_of_500x10", |b| {
        b.iter(|| {
            let edges = snap.neighbors_as_of(nid, t).unwrap();
            criterion::black_box(edges);
        });
    });
}

fn bench_diff(c: &mut Criterion) {
    let (_d, db) = populate_temporal(500, 10);
    let snap = db.begin_read().unwrap();

    c.bench_function("diff_full_history_500x10", |b| {
        b.iter(|| {
            let d = snap.diff_between(Ts::MIN, Ts::MAX).unwrap();
            criterion::black_box(d);
        });
    });
}

criterion_group!(benches, bench_as_of, bench_diff);
criterion_main!(benches);
