# Chrona

**SQLite for graphs that change over time.**

Chrona is an open-source embedded temporal graph database. One file, no server,
time-travel built in. Store changing relationships with full validity windows,
provenance, and event history — then query not just what is true now, but what
was true before and what changed.

[![CI](https://github.com/chrona-db/chrona/actions/workflows/ci.yml/badge.svg)](https://github.com/chrona-db/chrona/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

> ⚠️ **Status: pre-1.0.** APIs and file format may change before 0.1.0 is
> published. Follow the [changelog](./CHANGELOG.md) and [architecture
> decisions](./ARCHITECTURE.md) for stability guarantees.

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

## Query language (MVP)

Six query shapes in v0.1:

```sql
FIND NEIGHBORS OF "alice"
FIND 2 HOPS FROM "alice" AT "2026-03-01"
SHOW PATH FROM "alice" TO "dan" BEFORE "2026-04-01"
WHO WAS CONNECTED TO "alice" ON "2026-03-01"
WHAT CHANGED BETWEEN "2026-03-01" AND "2026-04-01"
DIFF GRAPH BETWEEN "2026-03-01" AND "2026-04-01"
```

Timestamps are RFC 3339 / ISO 8601. A date-only form is midnight UTC. Full
grammar lives in [docs/query-language.md](./docs/query-language.md).

## Library usage

```rust
use chrona_core::{Db, EdgeInput, Ts};

let db = Db::open("demo.chrona")?;

let mut txn = db.begin_write()?;
let alice = txn.upsert_node("alice", Some("person"))?;
let bob = txn.upsert_node("bob", Some("person"))?;

txn.add_edge(EdgeInput {
    from: "alice",
    to: "bob",
    edge_type: "WORKS_WITH",
    valid_from: Ts::parse("2026-01-15")?,
    valid_to: None,
    observed_at: Ts::now(),
    source: "slack",
    confidence: 0.9,
    properties: Default::default(),
})?;
txn.commit()?;

let snap = db.begin_read()?;
for edge in snap.neighbors_as_of(alice, Ts::parse("2026-02-01")?)? {
    println!("{:?}", edge?);
}
```

## Architecture in one picture

```
┌──────────────────────────────────────────────────┐
│  chrona CLI   │  (future) PyO3   │  napi-rs     │
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
| **0.1** | 🚧 building | Embedded engine, CLI, all six MVP queries, tests, bench suite |
| 0.2 | planned | Python (PyO3) and TypeScript (napi-rs) bindings |
| 0.3 | planned | Cypher-compatible subset; property filters |
| 0.4 | planned | Column stats, cost-based planner, second-gen temporal index |
| 0.5 | planned | Import connectors (CSV, JSON, Parquet, Slack) |
| 1.0 | goal | API-stable, format-stable, production SLAs documented |

Post-1.0 expansion (separate product line): hosted sync, graph explorer UI,
managed cloud. The embedded engine stays open source.

## Performance targets

Measured on a 1 M-edge synthetic dataset, laptop SSD, single thread:

| Operation | Target | Status |
|---|---|---|
| Cold open 1 GB file | < 100 ms | ✅ measured |
| 1-hop traversal P50 | < 1 ms | ✅ measured |
| 2-hop AS-OF P99 | < 50 ms | ✅ measured |
| Diff over 1 M events | < 500 ms | ✅ measured |
| Append-only ingest | > 100 k edges/s | ✅ measured |

Benchmarks live in `benches/` and are runnable via `cargo bench`.

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
