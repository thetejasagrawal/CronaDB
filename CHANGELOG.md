# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-04-24

First stable release. API and on-disk format are locked under SemVer 1.x.

### Added

- **Query language extensions:**
  - `WHERE` filters on `type`, `source`, `confidence`, `valid_from`,
    `valid_to`, and `observed_at`. AND-joined terms.
  - `LIMIT n` clause on all edge-returning queries.
  - Identifier token for forward-compatibility with future query extensions.
- **CLI additions:**
  - `chrona query --json` emits structured JSON results.
  - `chrona stats --json`, `chrona nodes --json`, `chrona edges --json`.
  - `chrona nodes`, `chrona edges` — direct table inspection.
  - `chrona history <edge_id>` — walk a revision chain.
  - `chrona import --format csv|jsonl` with JSONL support.
  - `chrona fsck` as an alias for `verify`.
- **`chrona verify`** now performs full per-FORMAT.md §8 checks:
  meta presence, interner round-trip, referential integrity of nodes and
  edges, temporal well-formedness, adjacency completeness, event-log
  monotonicity, confidence range.
- **Snapshot API additions:**
  - `Snapshot::verify()` — invariant check with a structured report.
  - `Snapshot::all_nodes()`, `Snapshot::all_edges_view()` — iterate everything.
  - `Snapshot::revision_chain(edge_id)` — walk `supersedes` backwards.
- **Python bindings (`chrona` package)** via PyO3 + abi3-py37:
  - `chrona.Db`, `chrona.WriteTxn`, `chrona.Snapshot`, `chrona.Edge`,
    `chrona.Node`.
  - Context-managed read/write transactions.
  - `query`, `query_json` convenience methods.
  - Published as a `cp37-abi3` wheel via `maturin build`.
- **Observability:** tracing spans on `open`, `txn.read`, `txn.write`,
  `commit`. Enabled with `CHRONA_TRACE=info`.
- **Error taxonomy:** `Error::code()` returns a stable `E_*` string;
  `Error::is_user_recoverable()` flag.
- **docs/benchmarks.md** with reproducible numbers.

### Changed

- `Query` enum variants now carry `filter: Filter` and `limit: Option<u32>`
  fields for the four filterable query shapes. `Diff` and `Changed` are
  unchanged.
- `chrona-cli`'s `import` subcommand takes `--file` and optional `--format`
  instead of `--csv`.
- All crate versions bumped to `1.0.0`. The workspace depends on
  `chrona-core = 1.0` and `chrona-query = 1.0`.

### Stability contract (locked at 1.0.0)

- **File format**: version 1. Files written by 1.x will be readable by 1.x
  forever; major-bump files may carry a higher format version.
- **Rust API**: `Db`, `Snapshot`, `WriteTxn`, `Edge`, `Node`, `Ts`, `Props`,
  `PropValue`, `Error`, `VerifyReport` are stable. Additive changes allowed
  in 1.x.
- **CLI grammar**: stable within 1.x. New queries may be added; existing ones
  will not change meaning.
- **Python ABI**: abi3-py37 — the same wheel works on Python 3.7+.
- **Error codes**: the set of `E_*` strings is stable. Adding new ones is
  allowed; renaming or removing requires 2.0.

## [0.1.0] - 2026-04-24 (superseded)

Initial internal release. Superseded by 1.0.0 without public adoption.

See the git history for details.
