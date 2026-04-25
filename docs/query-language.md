# Chrona Query Language (1.0)

This document describes the DSL supported by `chrona query` in 1.x.

Design goals:

- Readable to a non-DB-expert.
- Six query shapes, covering every example from the canonical workload set.
- Zero ambiguity about time — all times are RFC 3339, UTC-enforced.
- Explicit, AND-joined `WHERE` filters; no surprise predicate pushdown.

---

## Grammar

```
query        := neighbor_q | hop_q | path_q | whoat_q | diff_q | changed_q

neighbor_q   := FIND NEIGHBORS OF STRING time_clause? where_clause? limit_clause?
hop_q        := FIND INT HOPS FROM STRING time_clause? where_clause? limit_clause?
path_q       := SHOW PATH FROM STRING TO STRING time_clause? where_clause? limit_clause?
whoat_q      := WHO WAS CONNECTED TO STRING ON STRING where_clause? limit_clause?
diff_q       := DIFF GRAPH BETWEEN STRING AND STRING (FOR NODE STRING)?
changed_q    := WHAT CHANGED BETWEEN STRING AND STRING (FOR NODE STRING)?

time_clause  := (AT STRING) | (BEFORE STRING) | (AFTER STRING)
where_clause := WHERE predicate (AND predicate)*
limit_clause := LIMIT INT

predicate    := IDENT op value
op           := = | != | < | <= | > | >=
value        := STRING | NUMBER

STRING       := " ( [^"\\] | \" | \\ | \n | \t )* "
INT          := [0-9]+
NUMBER       := [0-9]+ ( \. [0-9]+ )?
IDENT        := [a-zA-Z_][a-zA-Z0-9_]*
```

Keywords are case-insensitive. Every string literal must be double-quoted.
Bare identifiers are only valid as the left-hand side of a `WHERE` predicate;
a stray identifier in any other position is rejected at parse time.

## Semantics

- `FIND NEIGHBORS OF "x"` — every outgoing edge from `x` live right now.
- `FIND NEIGHBORS OF "x" AT "t"` — every outgoing edge from `x` live at `t`.
- `FIND n HOPS FROM "x" [AT|BEFORE|AFTER "t"]` — BFS from `x` to depth `n`, time-filtered.
- `SHOW PATH FROM "a" TO "b" [time]` — shortest live path, BFS.
- `WHO WAS CONNECTED TO "x" ON "t"` — union of forward and reverse neighbors at `t`.
- `DIFF GRAPH BETWEEN "t1" AND "t2" [FOR NODE "x"]` — structured summary of events in `[t1, t2]`.
- `WHAT CHANGED BETWEEN "t1" AND "t2" [FOR NODE "x"]` — same as above; semantics-level alias today.

The `FOR NODE` filter in `DIFF` / `WHAT CHANGED` is parsed but currently
ignored by the executor — it will narrow the event scan once a per-node event
index lands in 1.3.

## WHERE predicates

`WHERE` is supported on `FIND NEIGHBORS`, `FIND n HOPS`, `SHOW PATH`, and
`WHO WAS CONNECTED`. Predicates are AND-joined and reference the following
edge fields:

| Field | Type | Operators |
|---|---|---|
| `type` (alias `edge_type`) | string | all six (string compares are lexicographic) |
| `source` | string | all six (lexicographic) |
| `confidence` | number | all six |
| `valid_from` | RFC 3339 string | all six (compared as instants) |
| `valid_to` | RFC 3339 string | all six (compared as instants; missing `valid_to` is treated as `+∞`) |
| `observed_at` | RFC 3339 string | all six (compared as instants) |

The six operators are `=`, `!=`, `<`, `<=`, `>`, `>=`.

Property-level predicates (`properties.foo = "bar"`) are not yet supported;
see "Planned extensions" below.

## LIMIT

`LIMIT n` truncates the edge stream after `n` results. It applies to
`FIND NEIGHBORS`, `FIND n HOPS`, `SHOW PATH`, and `WHO WAS CONNECTED`.
`DIFF GRAPH` and `WHAT CHANGED` ignore `LIMIT` because they return summary
counts rather than streams.

## Timestamps

All timestamps are parsed as RFC 3339 or the shorter `YYYY-MM-DD` form, which
is interpreted as UTC midnight.

- `"2026-03-01"` — 2026-03-01 00:00:00 UTC.
- `"2026-03-01T12:34:56Z"` — the same date at 12:34:56 UTC.
- `"2026-03-01T12:34:56+01:00"` — converted to UTC on parse.

Negative timestamps (pre-1970) are legal but require the full RFC 3339 form.

## Examples

```
FIND NEIGHBORS OF "alice"
FIND NEIGHBORS OF "alice" WHERE type = "WORKS_WITH" AND confidence >= 0.8
FIND 2 HOPS FROM "company_123" AT "2026-02-01" WHERE source = "slack" LIMIT 10
SHOW PATH FROM "incident_7" TO "vendor_2" BEFORE "2026-03-10"
WHO WAS CONNECTED TO "Acme" ON "2026-03-01" WHERE confidence > 0.95
DIFF GRAPH BETWEEN "2026-01-01" AND "2026-04-01" FOR NODE "server_9"
WHAT CHANGED BETWEEN "2026-03-01" AND "2026-04-01"
```

## Error reporting

Every parser error carries the offending token's name and a short phrase
explaining what was expected. Example:

```
$ chrona query demo.chrona 'FIND NEIGHBORS "x"'
chrona: query error: expected OF, got STRING
```

## Planned extensions (post-1.0)

- `WHERE properties.key = value` predicates (1.2).
- `TYPE "X"` shorthand for `WHERE type = "X"` (1.2).
- A Cypher-compatible subset: `MATCH (a)-[:REL]->(b) WHERE ...` (1.2).

These are deliberately **not** in 1.0.
