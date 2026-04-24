# Contributing to Chrona

Thanks for considering a contribution. Chrona is pre-1.0; the project is small,
opinionated, and moves fast. That means your contributions can have outsized
impact, and it also means we're careful about scope.

## Quick start

```bash
git clone https://github.com/chrona-db/chrona
cd chrona
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

If all four pass locally, you're ready to work.

## What we want

- **Bug fixes** with a regression test.
- **Performance improvements** with a benchmark that shows the delta.
- **Documentation improvements** — always welcome.
- **Query language extensions** that fit the MVP grammar; propose in an issue first.
- **New test cases**, especially adversarial ones for the temporal layer.

## What we don't want (yet)

- New top-level features without prior discussion.
- Breaking changes to the on-disk format. See [FORMAT.md](./FORMAT.md) for
  the stability contract.
- Dependencies with incompatible licenses. See `deny.toml`.
- "While I was here, I reformatted everything." Keep diffs focused.

## Process

1. **Open an issue first** for anything larger than a one-line fix. This is the
   cheapest way to know whether we'll accept the work.
2. **Fork and branch.** Feature branches named `feature/short-name` or
   `fix/short-name`.
3. **Write tests.** No PR merges without either a new test, a benchmark, or a
   compelling reason neither applies.
4. **Run the full check locally:**
   ```bash
   cargo fmt --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all
   ```
5. **Open the PR.** Explain *why* the change is right, not just what it does.
6. **CI must be green.** No force-merges on red CI.

## Code style

- `rustfmt` is enforced. Run `cargo fmt` before committing.
- Clippy warnings are errors. Run `cargo clippy --all-targets` before committing.
- Prefer small, named functions over long ones. Target: < 60 lines per function.
- No `unwrap()` in non-test code. Use `?` or a typed error. `expect("reason")` is
  allowed where the invariant is obvious and documented.
- Module-level docs (`//!`) are required at the top of every `mod.rs` and every
  public crate root.
- Doc comments on every public item. Examples encouraged.

## Commit and PR hygiene

- Present-tense, imperative commit subjects: `Add temporal index scan`.
- One logical change per commit. Squash noise before merge.
- PRs link to any related issue.
- PRs describe the *user-visible* effect first, the implementation second.

## Licensing

By contributing, you agree that your contributions will be licensed under the
project's dual MIT / Apache-2.0 license. Large contributions may be asked to
sign a CLA; this is decided case-by-case before 1.0.

## Security

Do not file public issues for security vulnerabilities. See
[SECURITY.md](./SECURITY.md).

## Code of conduct

See [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).

## Design docs

Before writing code, read:

1. [Chrona_Product_Thesis.md](./Chrona_Product_Thesis.md) — why this exists.
2. [ARCHITECTURE.md](./ARCHITECTURE.md) — how it's shaped.
3. [FORMAT.md](./FORMAT.md) — what the bytes look like.

If your change contradicts any of these, say so in the PR and propose an edit
to the relevant doc. We take those documents seriously and change them
deliberately.
