# Security Policy

## Supported versions

Chrona is pre-1.0. Only the latest minor version receives security fixes.

| Version | Supported |
|---|---|
| 0.1.x | ✅ |
| < 0.1 | ❌ |

## Reporting a vulnerability

**Do not file public issues for security vulnerabilities.**

Email: `security@chrona.dev` (will be configured before first release).

Please include:

- A clear description of the issue.
- Reproduction steps or a proof-of-concept.
- The affected version(s).
- Your suggested severity (low / medium / high / critical).

We will:

1. Acknowledge receipt within 72 hours.
2. Investigate and provide an initial assessment within 7 days.
3. Work on a fix privately; coordinate disclosure with you.
4. Credit you in the release notes unless you prefer otherwise.

## Scope

In scope:

- Data corruption bugs in `chrona-core`.
- Memory-safety issues (should be rare — we minimize `unsafe`).
- Query parser DoS (unbounded memory or CPU from untrusted input).
- Cryptographic issues (once we add encryption).

Out of scope (but still welcome as regular bugs):

- Performance issues that are not DoS-grade.
- Feature requests.
- Issues requiring a malicious binary with write access to the database file.

## Threat model (v0.1)

Chrona 0.1 is an embedded library. The threat model assumes:

- The process opening the database is trusted.
- The database file is not shared across trust boundaries without additional
  protection (e.g. filesystem-level encryption).
- Query strings may come from untrusted input; parser DoS counts as a security
  issue.
- Network access is not part of 0.x; the library does no I/O other than the
  local file.

This threat model tightens as we grow — 1.0 will document a more formal model.
