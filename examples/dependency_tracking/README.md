# Example: Dependency tracking across infrastructure changes

Track which services depended on which other services over time. When an
incident hits, ask **"what was depending on X right before things broke?"**
without rebuilding the topology by hand.

This example builds a tiny service-dependency graph, migrates one of the
edges (api-gateway → redis-1 becomes api-gateway → redis-2 on 2026-03-01),
then queries the dependency graph at two different points in time to show
the diff.

## Run it

```bash
cargo run --example dependency_tracking -p chrona-core
```

Expected output (abridged):

```
[2026-01-01] Initial deps recorded.
[2026-03-01] api-gateway migrated from redis-1 to redis-2.

*** Outage detected at 2026-03-15 involving redis-1 ***

Dependencies on redis-1 as of 2026-03-14T23:59:59Z:
  worker (valid since 2026-01-01T00:00:00Z)

Dependencies on redis-1 as of 2026-01-15T00:00:00Z:
  api-gateway (valid since 2026-01-01T00:00:00Z)
  worker      (valid since 2026-01-01T00:00:00Z)

Diff: api-gateway was removed from redis-1's consumers between these dates.
```

## Key ideas demonstrated

- `reverse_neighbors_as_of(id, t)` for "who pointed at X at time t."
- `supersede_edge` to model a clean migration with a single revision.
- Asking the same query at two different times to get a temporal diff for
  free — no application-side bookkeeping.
