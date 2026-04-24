# chrona (Python)

Python bindings for [Chrona](https://github.com/chrona-db/chrona), the embedded
temporal graph database.

## Install

Once published to PyPI:

```bash
pip install chrona
```

From source (requires Rust toolchain + Python 3.7+):

```bash
pip install maturin
cd crates/chrona-py
maturin develop --release
```

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

# Run a DSL query
result = db.query('FIND NEIGHBORS OF "alice" WHERE confidence >= 0.8')
for edge in result:
    print(edge.from_ext_id, "-[", edge.edge_type, "]->", edge.to_ext_id)

# Time-travel
with db.read() as snap:
    alice = snap.node_id("alice")
    for edge in snap.neighbors_as_of(alice, "2026-02-01"):
        print(edge)

# JSON
print(db.query_json('WHAT CHANGED BETWEEN "2026-01-01" AND "2026-04-01"'))
```

## API

- `chrona.Db(path)` — open or create a database.
- `db.write()` — context manager yielding a `WriteTxn`.
- `db.read()` — context manager yielding a `Snapshot`.
- `db.query(q)` → `QueryResult` iterable of edges.
- `db.query_json(q)` → JSON string (useful for tracing / logs).
- `WriteTxn.upsert_node(ext_id, node_type=None, properties=None)` → int id.
- `WriteTxn.add_edge(from_, to, edge_type, ...)` → int edge id.
- `WriteTxn.invalidate_edge(id, at)` — shorten an edge's validity window.
- `WriteTxn.supersede_edge(id, ...)` — append a revision.
- `Snapshot.node_id(ext_id)` → Optional[int].
- `Snapshot.neighbors_as_of(id, when)` → list[Edge].
- `Snapshot.reverse_neighbors_as_of(id, when)` → list[Edge].
- `Snapshot.n_hops_as_of(id, hops, when)` → list[Edge].
- `Snapshot.path_as_of(src, dst, when)` → Optional[list[Edge]].
- `Snapshot.events_between(t1, t2)` → list[Event].

All timestamps are strings (RFC 3339 or `YYYY-MM-DD`); strings pass through to
Chrona's parser.

## Licensing

Dual MIT / Apache-2.0, same as upstream.
