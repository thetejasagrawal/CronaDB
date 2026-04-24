# Chrona Architecture

> Status: **Draft v0.1** — pre-code specification. Changes here require a version bump once a release exists.
>
> Scope: this document describes the runtime architecture of Chrona. It pins layer boundaries, data model, consistency semantics, query lifecycle, and invariants. Bytes-on-disk are specified separately in [FORMAT.md](./FORMAT.md).

---

## 1. Goals and Non-Goals

### 1.1 Goals

1. **Embedded, single-file, zero-config.** One binary, one `.chrona` file, in-process execution. Opening an existing database must be sub-100 ms cold.
2. **Time is first-class.** Every edge carries bitemporal validity and observation metadata. Queries can ask "what was true at time T" without user-level modeling.
3. **Provenance is first-class.** Source, confidence, and revision lineage are part of the edge record, not an afterthought tacked on via property bags.
4. **State graph *and* event graph.** The state graph is a materialized view over an append-only event log. Both views are queryable.
5. **Predictable latency.** No GC pauses. Hot traversals stay in cache. Single-writer MVCC keeps reads lock-free.
6. **Honest durability.** Default: fsync-per-commit, WAL-backed, survives `kill -9` at any point without data loss below the last committed transaction.

### 1.2 Non-Goals (in MVP)

- Distributed execution, replication, sharding, consensus.
- Multi-writer concurrency. A single writer is sufficient for the target workloads.
- Full Cypher. A deliberately small DSL ships in MVP; a Cypher subset comes later.
- A visual graph explorer. CLI and bindings only.
- Schema enforcement at the storage layer. Properties are opaque bytes to the core; typing is a bindings-layer concern.
- An index zoo. Four indexes ship; the rest wait.
- Large analytics (multi-million-node global PageRank, etc.). Chrona optimizes for bounded neighborhood traversal and temporal diffs, not global graph compute.

### 1.3 Design Principles

- **The event log is the source of truth.** The state graph is derivable. If the two ever disagree, the event log wins.
- **Edges are immutable once written.** "Deleting" or "updating" an edge means appending a new event (`EdgeInvalidated`, `EdgeSuperseded`) that changes how derivations interpret the original record. This eliminates an entire category of race conditions and makes time-travel free.
- **Hot path stays in `u64`.** External string IDs are resolved to `u64` once at query-plan time. The execution engine never sees a string during traversal.
- **Prefer sequential I/O.** Adjacency is stored so that a 1-hop expansion is a single range scan. The temporal index is sorted so that "as of T" is a bounded lookup, not a full scan.
- **Commit to formats early; defer physical layout.** The logical record format (FORMAT.md) is part of the API contract from v1. The physical page layer (redb in v1) can change without breaking it.

---

## 2. System Overview

```
┌────────────────────────────────────────────────────────────┐
│  chrona CLI   │   Python (PyO3)   │   Node.js (napi-rs)   │
│  chrona-cli   │   chrona-py       │   chrona-node          │
└───────┬───────┴─────────┬─────────┴───────────┬────────────┘
        │                 │                     │
        └─────────────────┴─────────────────────┘
                          │
                          ▼
        ┌─────────────────────────────────────────┐
        │   chrona-query                          │
        │   DSL lexer → parser → AST → plan       │
        │   → execution (operator pipeline)       │
        └──────────────────┬──────────────────────┘
                           │
                           ▼
        ┌─────────────────────────────────────────┐
        │   chrona-core                           │
        │                                         │
        │   ┌───────────────────────────────────┐ │
        │   │ Graph semantics                   │ │
        │   │   nodes · edges · neighborhoods   │ │
        │   │   traversal · path · diff         │ │
        │   └──────────────┬────────────────────┘ │
        │                  │                      │
        │   ┌──────────────▼────────────────────┐ │
        │   │ Temporal layer                    │ │
        │   │   event log · as-of filter        │ │
        │   │   interval scan · revision chain  │ │
        │   └──────────────┬────────────────────┘ │
        │                  │                      │
        │   ┌──────────────▼────────────────────┐ │
        │   │ Storage engine                    │ │
        │   │   tables · keys · string interner │ │
        │   │   snapshot · WAL · recovery       │ │
        │   └──────────────┬────────────────────┘ │
        │                  │                      │
        │   ┌──────────────▼────────────────────┐ │
        │   │ Page layer (redb in v1)           │ │
        │   │   B-tree pages · MVCC · fsync     │ │
        │   └───────────────────────────────────┘ │
        └─────────────────────────────────────────┘
                           │
                           ▼
                   ┌───────────────┐
                   │  database.chrona   │ (single file)
                   └───────────────┘
```

Each layer has a single inbound dependency on the layer below and a single outbound interface to the layer above. The boundaries are enforced by crate structure (see §10).

---

## 3. Data Model

### 3.1 Core Types

| Type | Rust | On-disk | Semantics |
|---|---|---|---|
| `NodeId` | `u64` | BE u64 | Internal id, assigned monotonically. `0` reserved. |
| `EdgeId` | `u64` | BE u64 | Internal id, assigned monotonically. `0` reserved. |
| `EventId` | `u64` | BE u64 | Internal id, monotonically increasing per file. `0` reserved. |
| `StringId` | `u32` | BE u32 | Interned id for type names, source names, property keys. |
| `Ts` | `i64` | BE i64, sign-bit flipped | Nanoseconds since Unix epoch, UTC. See §5.2. |
| `Confidence` | `f32` | BE f32 | `0.0..=1.0`. Values outside this range are rejected at the API boundary. |
| `ExtId` | `String` | UTF-8 bytes | User-facing opaque identifier. Unique per node within a database. |

### 3.2 Node

A node is the minimum unit of identity.

```
Node {
    id:        NodeId,
    ext_id:    ExtId,          // user-facing opaque string
    type_id:   StringId,       // interned node type name; 0 = untyped
    created_at: Ts,            // when this node first appeared in events
    props:     Cbor,           // user properties, opaque to core
}
```

Nodes are **not** temporal in v1. A node exists or it does not. Node properties can be overwritten. Node-level temporal history can be modeled by the user via `ObservedAt` property conventions; first-class node versioning is a post-MVP feature.

### 3.3 Edge

An edge carries the full temporal + provenance payload.

```
Edge {
    id:          EdgeId,
    from:        NodeId,
    to:          NodeId,
    type_id:     StringId,      // interned edge type name, e.g. "WORKS_WITH"
    valid_from:  Ts,            // inclusive
    valid_to:    Option<Ts>,    // exclusive; None = open-ended / still valid
    observed_at: Ts,            // when this fact was recorded
    source_id:   StringId,      // e.g. "slack_import", "user_entry", "model_v3"
    confidence:  Confidence,
    supersedes:  Option<EdgeId>,// previous edge in revision chain
    props:       Cbor,          // user properties
}
```

**Validity semantics.** An edge is *live at time T* if and only if:

```
valid_from <= T AND (valid_to IS NONE OR valid_to > T)
```

This is the half-open interval `[valid_from, valid_to)`, standard bitemporal convention.

**Immutability.** An edge record is write-once. There is no in-place update. The `supersedes` field and the `EdgeInvalidated` / `EdgeSuperseded` event kinds handle revisions.

### 3.4 Event

Every write to the database produces one or more event records.

```
Event {
    id:        EventId,
    timestamp: Ts,              // wall-clock at write time; not valid_from
    kind:      EventKind,
    payload:   Bytes,           // kind-specific, see §3.5
}
```

### 3.5 Event Kinds (v1)

| Kind | Value | Payload | Effect on state graph |
|---|---|---|---|
| `NodeAdded` | 1 | `NodeRecord` | Insert node |
| `NodeRemoved` | 2 | `NodeId` | Remove node (and its edges are cascaded to invalidated) |
| `EdgeObserved` | 3 | `EdgeRecord` | Insert edge |
| `EdgeInvalidated` | 4 | `{ edge_id: EdgeId, at: Ts }` | Set `valid_to = at` on the target edge |
| `EdgeSuperseded` | 5 | `{ old: EdgeId, new: EdgeRecord }` | Insert new, link `new.supersedes = old`, set `old.valid_to = new.valid_from` |
| `PropertySet` | 6 | `{ scope, id, key, value }` | Update node/edge properties (node only in v1) |

Future kinds are reserved in the range 7..=255. Readers must tolerate unknown event kinds by ignoring them (see §7.3).

### 3.6 The Two Views

The same underlying data exposes two distinct query surfaces:

**State graph** answers "what is true (or was true at T)?":
- `neighbors(node)`, `neighbors_as_of(node, t)`
- `path(a, b)`, `path_before(a, b, t)`
- `edges_between(a, b)`, `edges_between_as_of(a, b, t)`

**Event graph** answers "what changed?":
- `events_between(t1, t2)`
- `diff(t1, t2)` — a summary of added, invalidated, superseded
- `history_of_edge(edge_id)` — walk `supersedes` chain
- `events_touching(node, t1, t2)`

Both views read from the same storage. No duplicate state.

---

## 4. Storage Engine

### 4.1 Tables

Chrona stores everything in a fixed set of logical tables. Keys and values are byte-strings; the exact encoding is pinned in [FORMAT.md](./FORMAT.md).

| Table | Key | Value | Purpose |
|---|---|---|---|
| `nodes` | `NodeId` | `NodeRecord` | Canonical node store |
| `edges` | `EdgeId` | `EdgeRecord` | Canonical edge store |
| `fwd_adj` | `(NodeId from, Ts valid_from, EdgeId)` | `NodeId to` | Forward adjacency, time-ordered |
| `rev_adj` | `(NodeId to, Ts valid_from, EdgeId)` | `NodeId from` | Reverse adjacency, time-ordered |
| `events` | `(Ts, EventId)` | `EventRecord` | Append-only event log, time-ordered |
| `temporal_idx` | `(Ts valid_from, EdgeId)` | `()` | Global edge-by-valid_from index |
| `strings_fwd` | `[u8]` | `StringId` | String → id |
| `strings_rev` | `StringId` | `[u8]` | Id → string |
| `ext_ids` | `[u8] ext_id` | `NodeId` | External id → internal id |
| `supersedes_idx` | `(EdgeId old, EdgeId new)` | `()` | Revision chain navigation |
| `meta` | `[u8] key` | `[u8] value` | Database metadata (schema version, flags, counters) |

### 4.2 Why These Indexes and Not Others

- **`fwd_adj` / `rev_adj` keyed by `(node, valid_from, edge_id)`.** A range scan from `(n, MIN, 0)` to `(n, T, MAX)` returns every edge from `n` that was valid *at or before* T; filtering by `valid_to` on the returned records is cheap and O(output). This is the backbone of all neighborhood queries.
- **`temporal_idx` keyed by `(valid_from, edge_id)`.** Powers "what changed in this interval" without touching any node-keyed structure.
- **`supersedes_idx`.** Small, sparse; lets revision-chain walks avoid scanning `edges`.
- **No edge-type index in v1.** Type filtering is applied post-range-scan. Added in v2 if profiling shows it matters.
- **No property index in v1.** Full scan for filtered queries. Added in v2.

### 4.3 String Interning

Type names (node type, edge type), source names, and property keys are interned. A `StringId` is 32 bits; ids are assigned monotonically and never reused. The `strings_fwd` / `strings_rev` tables are append-only with no deletion in v1.

Interning is mandatory for type/source/property-key fields. Arbitrary user property *values* are not interned — they live in the CBOR blob.

### 4.4 Snapshot and MVCC

The page layer (redb in v1) provides:

- Copy-on-write B-tree pages.
- Reader snapshots: `begin_read()` captures the current root; all subsequent reads through that handle see a consistent view, even as writers commit new versions.
- Single writer lock: `begin_write()` acquires an exclusive lock on the file. Only one writer exists at a time.
- Commit atomicity: a commit flips a single root pointer in the file header. Either all changes are visible or none are.

Chrona inherits all three semantics. This gives us **snapshot isolation** for readers, **serializable** writes, and **no reader-writer blocking**.

### 4.5 WAL and Durability

Writes flow as follows within a single transaction:

1. `begin_write()` acquires the writer lock, opens a write transaction.
2. Caller issues logical writes (add edge, invalidate edge, etc.).
3. Each logical write produces one or more events, appended to the `events` table, and corresponding derived-table updates (`edges`, `fwd_adj`, `rev_adj`, etc.).
4. On `commit()`, the page layer writes dirty pages, flushes a WAL record, fsyncs, and atomically updates the root pointer.
5. The writer lock is released.

On crash, recovery is automatic on next `open()`: the page layer replays or discards the in-progress WAL, leaving the database at the last committed transaction.

A pragma `chrona_sync_mode` can be set to `normal` (default, fsync per commit) or `off` (no fsync — for bulk-import workloads where the caller will re-import on failure).

---

## 5. Temporal Layer

### 5.1 As-Of Queries

Given a node `n` and a time `T`, `neighbors_as_of(n, T)`:

1. Range-scan `fwd_adj` from `(n, Ts::MIN, 0)` to `(n, T, u64::MAX)`.
2. For each `(from, valid_from, edge_id) → to`, fetch the edge record from `edges`.
3. Filter to those with `valid_to.is_none() || valid_to > T`.
4. Yield `(edge_id, to, edge_metadata)`.

Complexity: O(E_in_range). For a node with most edges concentrated in a recent time window, queries at recent T are nearly O(output).

### 5.2 Timestamp Encoding

`Ts` is `i64` nanoseconds since Unix epoch. On disk, we write `i64::from_be_bytes` after **flipping the sign bit** (`x ^ 0x8000_0000_0000_0000`). This ensures lexicographic byte order matches numeric order across negative and positive timestamps.

`Ts::MIN = i64::MIN`. `Ts::MAX = i64::MAX`. Valid range covers roughly 1677 CE – 2262 CE, matching Arrow / Parquet conventions.

**Open-ended `valid_to`** is represented in the API as `Option<Ts> = None`. On disk, the edge record sets a flag byte and omits the `valid_to` field entirely (see FORMAT.md §3.3). The sentinel `i64::MAX` is **not** used for this purpose, to leave room for true Ts::MAX values.

### 5.3 Revision Chains

When an edge is superseded:

1. Writer appends `EdgeSuperseded { old, new }` event.
2. `old.valid_to` is set to `new.valid_from` in the `edges` record.
3. `new.supersedes = Some(old.id)` is recorded.
4. `supersedes_idx` is updated.

Walking the chain: repeatedly look up `edge.supersedes` until `None`. Forward walks (finding successors) use `supersedes_idx` keyed on `(old, new)`.

A revision chain is a linked list, not a tree. Branching revisions (two observers updating the same edge concurrently) are not supported in v1; the single-writer model precludes them.

### 5.4 Event Log Scans

`events_between(t1, t2)` is a direct range scan on `(Ts, EventId)` keys. O(output). This is the primitive underneath every `WHAT CHANGED BETWEEN...` and `DIFF GRAPH BETWEEN...` query.

---

## 6. Query Engine

### 6.1 Pipeline

```
query string
    │
    ▼
┌───────────┐
│   Lexer   │   produces token stream
└─────┬─────┘
      ▼
┌───────────┐
│  Parser   │   produces AST
└─────┬─────┘
      ▼
┌───────────┐
│ Resolver  │   resolves ext_id → NodeId, source names → StringId,
│           │   parses ISO-8601 timestamps
└─────┬─────┘
      ▼
┌───────────┐
│  Planner  │   picks operators; no cost-based planning in v1
└─────┬─────┘
      ▼
┌───────────┐
│ Executor  │   iterator pipeline; pull-based
└─────┬─────┘
      ▼
  result stream
```

### 6.2 Operators (v1)

- `ScanFwdAdj(from: NodeId, as_of: Option<Ts>)` → Iterator<EdgeRef>
- `ScanRevAdj(to: NodeId, as_of: Option<Ts>)` → Iterator<EdgeRef>
- `Expand(hops: u8)` — BFS driver composed of ScanFwdAdj + deduplication
- `BidirectionalBFS(src, dst, as_of)` — for path queries
- `ScanEvents(t1, t2, filter)` — range scan over `events`
- `ResolveEdge(edge_id)` — lookup in `edges`
- `ResolveNode(node_id)` — lookup in `nodes`
- `Filter(pred)` — applied post-scan
- `Project(cols)` — shape the output
- `Materialize(format)` — Arrow batch, JSON row, or row iterator

Operators pull rows from their inputs. Back-pressure is natural; memory is bounded by the operator with the largest internal state (BFS frontier). No operator in v1 buffers the full result set.

### 6.3 The DSL (minimum viable grammar)

```
query       := neighbor_q | hop_q | path_q | whoat_q | diff_q | changed_q
neighbor_q  := "FIND" "NEIGHBORS" "OF" ext_id                                      time_clause?
hop_q       := "FIND" INT "HOPS" "FROM" ext_id                                     time_clause?
path_q      := "SHOW" "PATH" "FROM" ext_id "TO" ext_id                             time_clause?
whoat_q     := "WHO" "WAS" "CONNECTED" "TO" ext_id "ON" iso_date
diff_q      := "DIFF" "GRAPH" "BETWEEN" iso_date "AND" iso_date ("FOR" "NODE" ext_id)?
changed_q   := "WHAT" "CHANGED" "BETWEEN" iso_date "AND" iso_date ("FOR" "NODE" ext_id)?
time_clause := ("AT" iso_date) | ("BEFORE" iso_date) | ("AFTER" iso_date)
ext_id      := STRING_LITERAL
iso_date    := STRING_LITERAL   // parsed as RFC 3339
```

Filter clauses (`WHERE type = "X"`, `WHERE source = "slack"`, `WHERE confidence > 0.5`) are deferred to post-MVP. They're easy to add; keeping the MVP grammar minimal protects the ship date.

The full grammar and query reference live in `docs/query-language.md` (to be written alongside M5).

---

## 7. Consistency and Recovery

### 7.1 Invariants (must hold at every commit boundary)

1. Every `EdgeRecord` in `edges` has exactly one entry in `fwd_adj` and one in `rev_adj`.
2. Every live edge referenced from `fwd_adj` or `rev_adj` has a corresponding record in `edges`.
3. `edge.valid_from <= edge.valid_to` whenever `valid_to.is_some()`.
4. Every `NodeId` referenced by any edge exists in `nodes`.
5. The `events` table is append-only; no existing event record is ever modified.
6. The state graph at any snapshot equals the fold of all events up to that snapshot, applied in event-id order.
7. Every `StringId` in any record has a corresponding entry in `strings_rev`.

These are checked by `chrona verify` (CLI subcommand). A failure indicates a bug, not data corruption — the format does not permit divergence at commit time.

### 7.2 Crash Recovery

Handled by the page layer. The database file after any crash equals the state at the most recent successful commit. There is no "recovering…" phase visible to the user; `open()` is constant-time regardless of prior crash state.

### 7.3 Forward Compatibility

A reader on format version `N` opening a file written at version `M`:

- `M == N` → read normally.
- `M < N` → read normally (all older records are still understood).
- `M > N` → the `meta.required_features` bitmap is consulted. If the reader supports all required features, read normally, ignoring unknown event kinds and record flags. Otherwise, refuse to open and emit a clear upgrade message.

This is specified in detail in [FORMAT.md §6](./FORMAT.md#6-versioning).

---

## 8. Error Taxonomy

| Category | Examples | User-recoverable? |
|---|---|---|
| `StorageError` | Disk full, I/O error, checksum mismatch, file truncated | Usually not — caller should surface and abort |
| `FormatError` | Unsupported file version, missing required feature | Not at runtime — operator must upgrade |
| `QueryError` | Parse error, unresolved ext_id, malformed timestamp | Yes — fix the query |
| `SchemaError` | Invalid confidence out of `[0, 1]`, supersedes chain loop | Yes — fix the write |
| `ConflictError` | Writer already active (on non-blocking open) | Yes — retry |
| `InternalError` | Invariant violation | No — bug; reported as such |

All errors carry a stable error code (`E0001..E0999`) for bindings to pattern-match on without parsing messages.

---

## 9. Concurrency Model Summary

| Operation | Blocking behavior |
|---|---|
| `open(path)` | Non-blocking. Multiple readers can open the same file. |
| `begin_read()` | Non-blocking. Captures a snapshot. |
| `begin_write()` | Blocks if another writer is active. Optional `try_begin_write()` variant. |
| Read through a snapshot | Never blocks. |
| Commit a write | Acquires a brief exclusive fsync window. |
| Close | Waits for outstanding handles on that connection to drop. |

Cross-process access: a single host process holds the writer; other processes may open read-only snapshots via a file-lock protocol (inherited from the page layer). Cross-process multi-writer is **not** supported.

---

## 10. Crate Boundaries

```
chrona-core/
    ├── storage/        // tables, keys, snapshots, WAL, recovery
    ├── graph/          // nodes, edges, adjacency, interning
    ├── temporal/       // event log, interval scans, revision chains
    ├── provenance/     // source, confidence, revision helpers
    └── lib.rs          // public API: Db, Txn, Snapshot, Iter, Error

chrona-query/
    ├── lexer.rs
    ├── parser.rs
    ├── ast.rs
    ├── resolver.rs
    ├── plan.rs
    ├── exec.rs         // operator pipeline
    └── lib.rs          // public API: Query, QueryResult

chrona-cli/
    └── src/main.rs     // init, import, query, stats, verify, repl

chrona-py/              // PyO3 wrapper
chrona-node/            // napi-rs wrapper
```

Dependencies flow strictly downward. `chrona-query` depends on `chrona-core`. Nothing depends on `chrona-cli`. Bindings depend on both core and query.

No cycles. No cross-layer imports. This is enforced by `cargo-deny` and a CI lint.

---

## 11. Observability

Every internal operation emits a structured tracing span. The root spans are:

- `chrona.open`, `chrona.close`
- `chrona.txn.read`, `chrona.txn.write`
- `chrona.query.lex`, `.parse`, `.resolve`, `.plan`, `.exec`
- `chrona.commit`

Per-operator spans live inside `chrona.query.exec`. Tracing is off by default; enabled via `CHRONA_TRACE=1` or the `Db::with_tracing(subscriber)` constructor.

A built-in `chrona stats <file>` command prints table-level row counts, segment sizes, and the oldest/newest event timestamps. This is the primary diagnostic surface.

---

## 12. What's Pluggable and What's Not

### Pluggable (may change between minor versions)

- Page layer (redb in v1; native Chrona storage in a future version).
- Property encoding (CBOR in v1; typed columns may layer on top later).
- Query parser (alternative frontends, e.g. a Cypher subset, plug in below the AST).
- Index types beyond the four core ones.

### Fixed (stable from v1; breaking changes require major version)

- Core types: `NodeId`, `EdgeId`, `Ts`, `Confidence`, `StringId`.
- Logical table catalog (§4.1).
- Event kinds 1–6 and their payload meanings.
- Temporal validity semantics (half-open `[valid_from, valid_to)`).
- External-id opacity (they're bytes; Chrona does not interpret them).
- The six query shapes in the MVP DSL.

---

## 13. Glossary

- **Adjacency**: The set of edges from or to a given node. Forward adjacency = outgoing; reverse = incoming.
- **As-of query**: A query evaluated at a specific historical timestamp `T`, returning the state of the graph at that moment.
- **Bitemporal**: A record carrying two time axes: validity (when the fact was true) and observation (when we recorded it). Chrona edges are bitemporal.
- **Event log**: The append-only sequence of `Event` records that fully captures every change to the database.
- **Live edge at T**: An edge whose `[valid_from, valid_to)` interval contains `T`.
- **Materialized view**: The state graph is a materialized view over the event log — derived but stored for fast reads.
- **Provenance**: Metadata about where a fact came from: source, confidence, and revision lineage.
- **Revision chain**: A linked list of edges connected via `supersedes`, representing successive observations of the same relationship.
- **Snapshot**: A read-only, consistent view of the database at a specific commit boundary.
- **State graph**: The graph of currently-live nodes and edges, optionally evaluated as-of a timestamp.
- **Supersedes**: When edge A is replaced by edge B. A's `valid_to` is set to B's `valid_from`; B's `supersedes = Some(A.id)`.

---

## 14. Open Questions (to resolve before M1)

1. **WAL granularity.** redb's commit-level WAL, or a finer event-level WAL? redb's is sufficient for MVP durability; revisit if ingest throughput disappoints.
2. **Property encoding.** CBOR is proposed. Final decision requires a micro-benchmark against MessagePack and a bespoke length-prefixed TLV format. Target: < 200 ns per edge round-trip for typical property sets.
3. **EventId allocation.** Monotonic per-file counter stored in `meta`, or hybrid logical clock? Per-file counter is simpler and sufficient until cross-file sync becomes a product concern.
4. **Interning eviction.** Never? Generational compaction? Decision deferred to post-MVP; the `strings_*` tables are append-only in v1.
5. **Node temporality.** Defer. Node deletion via `NodeRemoved` suffices for MVP.

These are tracked as ADRs (Architecture Decision Records) under `docs/adr/`.

---

## Appendix A: Query Lifecycle Walkthrough

Example: `FIND 2 HOPS FROM "company_123" AT "2026-02-01"`

1. **Lex.** Tokens: `FIND`, `INT(2)`, `HOPS`, `FROM`, `STRING("company_123")`, `AT`, `STRING("2026-02-01")`.
2. **Parse.** AST: `HopQuery { hops: 2, from: "company_123", time: Some(At("2026-02-01")) }`.
3. **Resolve.** Query engine opens a read snapshot. Resolves `"company_123"` via `ext_ids` → `NodeId(4782)`. Parses `"2026-02-01"` as RFC 3339 midnight UTC → `Ts(1_769_817_600_000_000_000)`.
4. **Plan.** Pipeline: `ScanFwdAdj(4782, as_of=Ts)` → `Expand(hops=2)` → `Dedup` → `ResolveNode` → `Materialize(Arrow)`.
5. **Execute.** Range-scan on `fwd_adj`, time-filter, collect frontier, re-scan for hop 2, dedup, project node records.
6. **Return.** Arrow RecordBatch streamed to the caller.
7. **Close.** Read snapshot dropped when the result iterator is dropped.

Total I/O: two range scans on `fwd_adj`, one lookup per unique node in the frontier on `nodes`. No writes.

---

*End of ARCHITECTURE.md v0.1. See FORMAT.md for byte-level record layouts and KV key encoding.*
