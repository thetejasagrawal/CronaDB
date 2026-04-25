# Example: Agent memory

An LLM-style agent learns about the world from noisy sources, revises its
beliefs over time, and occasionally has to invalidate one entirely. CronaDB's
temporal model is a natural fit: every belief is an edge with a confidence,
a source, and a window of validity, and revisions are first-class.

This example walks through three steps:

1. **t0** — agent infers `alice WORKS_AT AcmeCorp` from an email
   (`confidence=0.65`, `source="email_analysis"`).
2. **t1** — LinkedIn confirms it; the previous edge is *superseded* by a
   higher-confidence revision (`0.95`, `linkedin`).
3. **t2** — Alice tweets that she's joined BetaInc; the AcmeCorp edge is
   superseded again, this time by a different target node.

Then it queries the belief state at three different times and walks the
revision chain backwards, showing how the agent's view of the world looked
at each moment.

## Run it

```bash
cargo run --example agent_memory -p chrona-core
```

Expected output (abridged):

```
At t0 + 1 day:
  alice -[WORKS_AT]-> AcmeCorp   (src=email_analysis, conf=0.65)

At t1 + 1 day:
  alice -[WORKS_AT]-> AcmeCorp   (src=linkedin, conf=0.95)

At t2 + 1 day:
  alice -[WORKS_AT]-> BetaInc    (src=twitter, conf=0.99)

Revision chain for e3 (latest):
  3 -> BetaInc  (WORKS_AT) src=twitter      conf=0.99 valid_from=2026-03-15T10:00:00Z
  2 -> AcmeCorp (WORKS_AT) src=linkedin     conf=0.95 valid_from=2026-02-01T10:00:00Z
  1 -> AcmeCorp (WORKS_AT) src=email_analysis conf=0.65 valid_from=2026-01-15T10:00:00Z
```

## Key ideas demonstrated

- `supersede_edge` for appending revisions rather than overwriting.
- `neighbors_as_of(id, t)` for time-travel reads.
- Walking `Edge::supersedes` to reconstruct provenance.
