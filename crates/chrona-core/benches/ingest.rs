//! Benchmark: append-heavy ingest throughput and cold open.

use chrona_core::{Db, EdgeInput, Ts};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use tempfile::TempDir;

fn bench_ingest_hot(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest_hot");
    let n = 10_000;
    group.throughput(Throughput::Elements(n as u64));
    group.sample_size(10);
    group.bench_function("add_edges_10k_single_txn", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let dir = TempDir::new().unwrap();
                let db = Db::open(dir.path().join("i.chrona")).unwrap();
                let start = std::time::Instant::now();
                db.write(|w| {
                    for i in 0..n {
                        w.add_edge(EdgeInput {
                            from: format!("n{}", i),
                            to: format!("n{}", i + 1),
                            edge_type: "E".into(),
                            valid_from: Ts::from_nanos(i as i64),
                            valid_to: None,
                            observed_at: Ts::from_nanos(i as i64),
                            source: "bench".into(),
                            confidence: 1.0,
                            properties: Default::default(),
                        })?;
                    }
                    Ok(())
                })
                .unwrap();
                total += start.elapsed();
            }
            total
        });
    });
    group.finish();
}

fn bench_cold_open(c: &mut Criterion) {
    // Pre-create a populated database; then measure the time to open it.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cold.chrona");
    {
        let db = Db::open(&path).unwrap();
        db.write(|w| {
            for i in 0..1000 {
                w.add_edge(EdgeInput {
                    from: format!("n{}", i),
                    to: format!("n{}", (i + 1) % 1000),
                    edge_type: "E".into(),
                    valid_from: Ts::from_nanos(i as i64),
                    valid_to: None,
                    observed_at: Ts::from_nanos(i as i64),
                    source: "bench".into(),
                    confidence: 1.0,
                    properties: Default::default(),
                })?;
            }
            Ok(())
        })
        .unwrap();
    }

    let mut group = c.benchmark_group("open");
    group.sample_size(50);
    group.bench_function("open_1k_edge_db", |b| {
        b.iter(|| {
            let db = Db::open(&path).unwrap();
            criterion::black_box(db);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_ingest_hot, bench_cold_open);
criterion_main!(benches);
