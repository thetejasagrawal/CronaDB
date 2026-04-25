<p align="center">
  <img src="https://raw.githubusercontent.com/thetejasagrawal/CronaDB/main/Assets/CronaDB_banner.png" alt="CronaDB" width="520">
</p>

<p align="center"><b>Python bindings for CronaDB — SQLite for graphs that change over time.</b></p>

[CronaDB](https://github.com/thetejasagrawal/CronaDB) is an embedded temporal
graph database. One file. No server. Time-travel built in. This package is
the official PyO3 wrapper.

## Install

```bash
pip install chrona            # once published to PyPI
```

From source (requires Rust 1.75+ and Python 3.7+):

```bash
pip install maturin
git clone https://github.com/thetejasagrawal/CronaDB
cd CronaDB/crates/chrona-py
maturin develop --release
```

Wheels are `abi3-py37` — one wheel works on Python 3.7+.

## Quickstart

```python
import chrona

db = chrona.Db("demo.chrona")

with db.write() as txn:
    txn.upsert_node("alice", node_type="person")
    txn.upsert_node("bob",   node_type="person")
    txn.add_edge(
        from_="alice",
        to="bob",
        edge_type="WORKS_WITH",
        valid_from="2026-01-15",
        source="slack",
        confidence=0.9,
    )

# DSL query
for edge in db.query('FIND NEIGHBORS OF "alice" WHERE confidence >= 0.8'):
    print(edge.from_ext_id, "-[", edge.edge_type, "]->", edge.to_ext_id)

# Time-travel via the read API
with db.read() as snap:
    alice = snap.node_id("alice")
    for edge in snap.neighbors_as_of(alice, "2026-02-01"):
        print(edge)

# JSON output (handy for tracing / logs)
print(db.query_json('WHAT CHANGED BETWEEN "2026-01-01" AND "2026-04-01"'))
```

## API

| Call | Returns | Notes |
|---|---|---|
| `chrona.Db(path)` | `Db` | open or create |
| `db.write()` | context manager → `WriteTxn` | one writer at a time |
| `db.read()` | context manager → `Snapshot` | snapshot isolation |
| `db.query(q)` | `QueryResult` | iterable of `Edge` |
| `db.query_json(q)` | `str` | JSON string |
| `WriteTxn.upsert_node(ext_id, node_type=None, properties=None)` | `int` | returns node id |
| `WriteTxn.add_edge(from_, to, edge_type, ...)` | `int` | returns edge id |
| `WriteTxn.invalidate_edge(id, at)` | `None` | shorten validity window |
| `WriteTxn.supersede_edge(id, ...)` | `int` | append a revision |
| `Snapshot.node_id(ext_id)` | `Optional[int]` | |
| `Snapshot.neighbors_as_of(id, when)` | `list[Edge]` | forward edges |
| `Snapshot.reverse_neighbors_as_of(id, when)` | `list[Edge]` | inbound edges |
| `Snapshot.n_hops_as_of(id, hops, when)` | `list[Edge]` | BFS, deduped |
| `Snapshot.path_as_of(src, dst, when)` | `Optional[list[Edge]]` | shortest path |
| `Snapshot.events_between(t1, t2)` | `list[Event]` | event log range scan |

All timestamps are strings (RFC 3339 or `YYYY-MM-DD`) and pass through to
CronaDB's parser.

## Documentation

- [Main project README](https://github.com/thetejasagrawal/CronaDB)
- [Query language](https://github.com/thetejasagrawal/CronaDB/blob/main/docs/query-language.md)
- [Architecture](https://github.com/thetejasagrawal/CronaDB/blob/main/ARCHITECTURE.md)
- [Benchmarks](https://github.com/thetejasagrawal/CronaDB/blob/main/docs/benchmarks.md)

## License

Dual MIT / Apache-2.0, same as upstream.
