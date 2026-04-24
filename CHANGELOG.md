# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial implementation of `chrona-core`: storage layer on top of redb, graph
  model with nodes and edges, temporal layer with bitemporal edges, event log,
  and provenance metadata.
- `chrona-query` crate: DSL lexer, parser, AST, and executor covering the six
  MVP query shapes from the product thesis.
- `chrona-cli` binary with `init`, `import`, `query`, `stats`, `verify`, and
  `repl` subcommands.
- Dual MIT / Apache-2.0 licensing.
- GitHub Actions CI (build, test, fmt, clippy, deny).
- Benchmark suite (criterion) covering cold start, traversal, temporal slice,
  diff, and append-heavy ingest.
- Example application: agent memory.
- Architecture and on-disk format specifications.

### Stability contract

- File format version: **1**. Files created by 0.1.x readers will be readable
  by 1.x readers. Breaking format changes require a major version bump.
- Public Rust API: **unstable**. Breaking changes allowed between 0.x minor
  versions.
- CLI grammar: **stable within 0.x**. New queries may be added; existing ones
  will not change meaning without a major bump.
