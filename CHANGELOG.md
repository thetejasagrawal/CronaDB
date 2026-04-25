# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`chrona tui <path>`** — interactive terminal UI built on `ratatui`. Pick a
  node from the left pane, see its neighborhood in the right pane, and
  scroll the time cursor (`+/-` for days, `[`/`]` for weeks) to watch the
  graph change in place. Includes an inline query box (`:` to focus, `Enter`
  to run) and a help overlay (`?`).
- **`chrona demo <path>`** — seeds a small startup-org graph spanning four
  months of 2026 with a reorganization, a job change, and a project launch.
  After seeding, prints a curated list of suggested queries to copy-paste.
  Supports `--tui` to launch directly into the TUI.
- Color and `[ ok ]` / `[fail]` / `[info]` tags for `chrona init`, `chrona
  demo`, and `chrona verify` output. Color is suppressed when stdout is not
  a terminal or when `NO_COLOR` is set.
- Project banner and icon (`Assets/CronaDB_banner.png`,
  `Assets/CronaDB_logo_icon.png`); README leads with the banner.
- Launch-ready README rewrite: navigation strip, "use it for" section with
  concrete scenarios, head-to-head comparison table (vs Neo4j, SQLite,
  DuckDB, Datomic), FAQ, and explicit star/discussion CTAs.
- Per-example READMEs under `examples/agent_memory/` and
  `examples/dependency_tracking/` so the GitHub directory view shows what
  each demo proves out.
- `CITATION.cff` so users can cite CronaDB from a paper or post.
- Polished `chrona-py/README.md` (now the PyPI landing page) with banner,
  tabular API reference, and links back to the main repo's design docs.
- Richer Python class docstrings on `Edge`, `Node`, `Snapshot`, `WriteTxn`
  (`help(chrona.Edge)` is now actually useful).
- GitHub PR template, issue templates (bug, feature, docs), and a private
  security advisory link in the issue chooser.
- Dependabot configuration for Cargo and GitHub Actions, grouped weekly.
- `python-release.yml` workflow to build abi3-py37 wheels and publish to PyPI
  via trusted publishing on tag push.

### Fixed

- `chrona nodes` no longer silently swallows string-resolution errors.
  A failed `resolve_string` now surfaces as `<unresolved string {id}: {err}>`
  instead of being printed as `null`, so corrupt-string-table bugs are
  immediately visible.

### Changed

- All repository URLs now point at
  `https://github.com/thetejasagrawal/CronaDB`.
- README no longer publishes a forward-looking roadmap; future versions
  ship when they ship.
- `release.yml` now installs the cross-compilation linker for the
  `aarch64-unknown-linux-gnu` target so the release matrix succeeds on
  `ubuntu-latest`.
- Fixed three broken intra-doc links in `chrona-core` (`Snapshot::verify`,
  `VerifyReport`) so `cargo doc -D warnings` is clean and the CI docs job
  passes.
- Documentation pass: removed dangling references to a thesis document that
  was never published, refreshed `docs/query-language.md` to describe the
  shipped 1.0 grammar (WHERE, LIMIT) instead of pre-1.0 plans, and replaced
  placeholder contact addresses in `SECURITY.md` and `CODE_OF_CONDUCT.md`
  with real channels.

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
