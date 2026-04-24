//! Benchmark: append-heavy ingest throughput.

use chrona_core::{Db, EdgeInput, Ts};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use tempfile::TempDir;

fn bench_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest");
    let n = 10_000;
    group.throughput(Throughput::Elements(n as u64));
    group.sample_size(10);
    group.bench_function("add_edges_10k", |b| {
        b.iter_with_large_drop(|| {
            let dir = TempDir::new().unwrap();
            let db = Db::open(dir.path().join("i.chrona")).unwrap();
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
            (dir, db)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_ingest);
criterion_main!(benches);
