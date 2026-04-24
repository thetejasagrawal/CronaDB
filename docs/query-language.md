# Chrona Query Language (MVP v0.1)

This document describes the DSL supported by `chrona query` in v0.1.

Design goals:

- Readable to a non-DB-expert.
- Six query shapes, covering every example from the product thesis §13.
- Zero ambiguity about time — all times are RFC 3339, UTC-enforced.
- No filter expressions in v0.1; they arrive in 0.2 with property predicates.

---

## Grammar

```
query       := neighbor_q | hop_q | path_q | whoat_q | diff_q | changed_q

neighbor_q  := FIND NEIGHBORS OF STRING time_clause?
hop_q       := FIND INT HOPS FROM STRING time_clause?
path_q      := SHOW PATH FROM STRING TO STRING time_clause?
whoat_q     := WHO WAS CONNECTED TO STRING ON STRING
diff_q      := DIFF GRAPH BETWEEN STRING AND STRING (FOR NODE STRING)?
changed_q   := WHAT CHANGED BETWEEN STRING AND STRING (FOR NODE STRING)?

time_clause := (AT STRING) | (BEFORE STRING) | (AFTER STRING)

STRING      := " ( [^"\\] | \" | \\ | \n | \t )* "
INT         := [0-9]+
```

Keywords are case-insensitive. Every string literal must be double-quoted.
Unquoted identifiers are rejected at lex time — this prevents accidental shell
globs from becoming queries.

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
index lands in 0.2.

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
FIND 2 HOPS FROM "company_123" AT "2026-02-01"
SHOW PATH FROM "incident_7" TO "vendor_2" BEFORE "2026-03-10"
WHO WAS CONNECTED TO "Acme" ON "2026-03-01"
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

## Planned extensions (0.2+)

- `WHERE` clauses on edge type, source, confidence, and property values.
- `LIMIT n` on neighbor/hop/path results.
- `TYPE "X"` shorthand for `WHERE edge_type = "X"`.
- A Cypher subset: `MATCH (a)-[:REL]->(b) WHERE ...`.

These are deliberately **not** in 0.1.
