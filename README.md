# Chrona

**SQLite for graphs that change over time.**

Chrona is an open-source embedded temporal graph database. One file, no server,
time-travel built in. Store changing relationships with full validity windows,
provenance, and event history — then query not just what is true now, but what
was true before and what changed.

[![CI](https://github.com/chrona-db/chrona/actions/workflows/ci.yml/badge.svg)](https://github.com/chrona-db/chrona/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/badge/crates.io-1.0.0-blue.svg)](https://crates.io/crates/chrona-core)
[![PyPI](https://img.shields.io/badge/pypi-1.0.0-blue.svg)](https://pypi.org/project/chrona/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

> ✅ **Status: 1.0 released.** File format and public API stable under SemVer.
> See [CHANGELOG.md](./CHANGELOG.md) for the stability contract.

---

## What it does

```
$ chrona init memory.chrona
Created memory.chrona

$ chrona query memory.chrona 'FIND NEIGHBORS OF "alice"'
bob        WORKS_WITH  valid=[2026-01-15..)  src=slack  conf=0.90
carol      REPORTS_TO  valid=[2026-02-01..)  src=hr     conf=1.00

$ chrona query memory.chrona 'WHO WAS CONNECTED TO "alice" ON "2026-01-20"'
bob        WORKS_WITH  valid=[2026-01-15..)  src=slack  conf=0.90

$ chrona query memory.chrona 'WHAT CHANGED BETWEEN "2026-02-01" AND "2026-04-01"'
+ carol -[REPORTS_TO]-> alice   (2026-02-01)
- dan   -[ADVISES]->    alice   (invalidated 2026-03-15)
```

One file on disk. No server. Sub-second responses on laptop workloads. Python
and TypeScript bindings planned for 0.2.

## Why another graph database?

Because most graph databases are either too heavy (Neo4j, TigerGraph) or treat
time as an afterthought. Chrona is designed around one belief: **time and
provenance are not optional metadata — they are part of the graph itself.**

Every edge in Chrona carries:
- `valid_from` / `valid_to` — when the relationship is true
- `observed_at` — when you learned about it
- `source` — where it came from
- `confidence` — how trustworthy it is
- `supersedes` — revision chain

That turns graphs from a static structure into a living system you can rewind.

## Install

```bash
# From source (only option today)
git clone https://github.com/chrona-db/chrona
cd chrona
cargo install --path crates/chrona-cli
```

Once 0.1.0 lands on crates.io:

```bash
cargo install chrona-cli
```

## Quickstart (3 minutes)

```bash
# Create a new database
chrona init demo.chrona

# Import some relationships
chrona import demo.chrona --csv people.csv

# Ask simple questions
chrona query demo.chrona 'FIND NEIGHBORS OF "alice"'
chrona query demo.chrona 'FIND 2 HOPS FROM "alice" AT "2026-03-01"'
chrona query demo.chrona 'SHOW PATH FROM "alice" TO "dan" BEFORE "2026-04-01"'

# See what changed
chrona query demo.chrona 'WHAT CHANGED BETWEEN "2026-03-01" AND "2026-04-01"'

# Open a REPL
chrona repl demo.chrona
```

## Query language

Six canonical shapes, plus `WHERE` filters and `LIMIT`:

```sql
FIND NEIGHBORS OF "alice"
FIND 2 HOPS FROM "alice" AT "2026-03-01"
SHOW PATH FROM "alice" TO "dan" BEFORE "2026-04-01"
WHO WAS CONNECTED TO "alice" ON "2026-03-01"
WHAT CHANGED BETWEEN "2026-03-01" AND "2026-04-01"
DIFF GRAPH BETWEEN "2026-03-01" AND "2026-04-01"

-- filters and limits
FIND NEIGHBORS OF "alice" WHERE type = "WORKS_WITH" AND confidence >= 0.8
FIND 2 HOPS FROM "alice" AT "2026-03-01" WHERE source = "slack" LIMIT 10
WHO WAS CONNECTED TO "alice" ON "2026-03-01" WHERE confidence > 0.95
```

Timestamps are RFC 3339 / ISO 8601. A date-only form is midnight UTC. Full
grammar lives in [docs/query-language.md](./docs/query-language.md).

## Library usage (Rust)

```rust
use chrona_core::{Db, EdgeInput, Ts};

let db = Db::open("demo.chrona")?;

db.write(|w| {
    w.upsert_node("alice", Some("person"))?;
    w.add_edge(EdgeInput {
        from: "alice".into(),
        to: "bob".into(),
        edge_type: "WORKS_WITH".into(),
        valid_from: Ts::parse("2026-01-15")?,
        valid_to: None,
        observed_at: Ts::now(),
        source: "slack".into(),
        confidence: 0.9,
        properties: Default::default(),
    })?;
    Ok(())
})?;

let snap = db.begin_read()?;
let alice = snap.get_node_id("alice")?.unwrap();
for edge in snap.neighbors_as_of(alice, Ts::parse("2026-02-01")?)? {
    println!("{:?}", edge);
}
```

## Library usage (Python)

```python
import chrona

db = chrona.Db("demo.chrona")

with db.write() as w:
    w.upsert_node("alice", node_type="person")
    w.add_edge(
        from_="alice", to="bob", edge_type="WORKS_WITH",
        valid_from="2026-01-15", source="slack", confidence=0.9,
    )

# DSL query
for edge in db.query('FIND NEIGHBORS OF "alice" WHERE confidence >= 0.8'):
    print(edge)

# Time-travel
with db.read() as snap:
    alice = snap.node_id("alice")
    for edge in snap.neighbors_as_of(alice, "2026-02-01"):
        print(edge.to_ext_id, edge.edge_type)
```

Install: `pip install chrona` (once published) or `maturin develop` from
`crates/chrona-py/`.

## Architecture in one picture

```
┌──────────────────────────────────────────────────┐
│  chrona CLI   │   chrona-py (PyO3)  │  (napi)   │
├──────────────────────────────────────────────────┤
│  chrona-query  — DSL: lex → parse → plan → exec  │
├──────────────────────────────────────────────────┤
│  chrona-core   — graph · temporal · provenance   │
│                  event log · snapshot · tables   │
├──────────────────────────────────────────────────┤
│  redb          — single-file MVCC B-tree         │
├──────────────────────────────────────────────────┤
│  database.chrona                                 │
└──────────────────────────────────────────────────┘
```

Deep dive: [ARCHITECTURE.md](./ARCHITECTURE.md) · On-disk format:
[FORMAT.md](./FORMAT.md).

## Project status and roadmap

| Version | Status | Focus |
|---|---|---|
| **1.0** | ✅ released | Engine, CLI, 6 MVP queries + WHERE/LIMIT, JSON I/O, JSONL import, full verify, Python bindings, benchmarks |
| 1.1 | planned | TypeScript (napi-rs) bindings, property-value filters |
| 1.2 | planned | Cypher-compatible subset; `WHERE properties.key = value` |
| 1.3 | planned | Column stats, cost-based planner, second-gen temporal index |
| 1.4 | planned | More connectors (Parquet, Slack, GraphML) |
| 2.0 | goal | Native Chrona page layer replacing redb; distributed story |

Expansion beyond the engine (separate product line): hosted sync, graph
explorer UI, managed cloud. The embedded engine stays open source.

## Performance

Measured on an Apple M-series MacBook (release build, criterion):

| Operation | P50 | Notes |
|---|---|---|
| Cold open (1 k edges) | **~14.6 ms** | open → ready for reads |
| 1-hop traversal | **~2.3 µs** | `neighbors_as_of` on a hot path |
| 2-hop BFS | **~16.4 µs** | deduped BFS with temporal filter |
| Temporal `as_of` | **~2.5 µs** | mid-window `T` on 5 000 versioned edges |
| Diff scan (5 k events) | **~386 µs** | ≈ 77 ms projected for 1 M events |
| Ingest (single txn) | **~37 k edges/s** | durable writes with fsync-per-commit |

Benchmarks live in `crates/chrona-core/benches/` and are runnable via
`cargo bench`. Full methodology: [docs/benchmarks.md](./docs/benchmarks.md).

## Contributing

Contributions welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md). Areas where
help is especially valued:

- Query language extensions (while scope-holding discipline is maintained)
- Benchmark datasets and adversarial workloads
- Docs and tutorials
- Language bindings (Python, Node, Go)

## License

Dual-licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))
- MIT license ([LICENSE-MIT](./LICENSE-MIT))

at your option.

Contributions are licensed under the same terms unless explicitly stated
otherwise.

## Design & philosophy

Chrona is built around a small set of beliefs documented in
[Chrona\_Product\_Thesis.md](./Chrona_Product_Thesis.md) (thesis) and
[ARCHITECTURE.md](./ARCHITECTURE.md) (runtime), including:

- Time is first-class. Every relationship carries validity.
- The event log is the source of truth. The state graph is derivable.
- Edges are immutable. Revisions happen by appending, never overwriting.
- Do one thing well. No distributed story in 0.x.

If those resonate, you'll probably like working on this.
