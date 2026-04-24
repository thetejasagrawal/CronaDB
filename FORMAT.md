# Chrona On-Disk Format

> Status: **Draft v0.1** — pre-code specification. This document defines the byte-level format of a Chrona database. Changes require a format version bump.
>
> Scope: this document pins record layouts, key encoding, and the table catalog in bytes. The runtime architecture is described separately in [ARCHITECTURE.md](./ARCHITECTURE.md).

---

## 1. Overview

A Chrona database is a **single file** on disk, conventionally named `*.chrona`. The file contains:

- A **container**: the physical page manager. In format version 1, the container is [redb](https://github.com/cberner/redb). In future versions it may be a native Chrona page layer. The logical format described below is container-independent.
- A set of **logical tables**, listed in §4. Each is a sorted KV map keyed by a byte string.
- A set of **records**, one of nine defined record types (§3), stored as values in those tables.

**Design goals:**

1. **Byte-deterministic.** Given the same logical input, two independent writers produce byte-identical records. This makes snapshot hashing, golden tests, and cross-implementation verification tractable.
2. **Single-file portable.** A Chrona database copies with `cp`, transfers with `scp`, versions with `git-lfs`, and survives restoration from backups without auxiliary state.
3. **Endianness-stable.** All multi-byte integers are big-endian so that lexicographic byte order matches numeric order. Keys rely on this for range scans.
4. **Forward-compatible.** Unknown event kinds, reserved flag bits, and unknown top-level meta keys are ignored by older readers where safe (§6). Hard-breaking changes require a major version bump.
5. **Zero ambiguity about time.** All timestamps are 64-bit signed nanoseconds since the Unix epoch, UTC, with an explicit on-disk sign-bit flip for correct sort order.

**Non-goals:**

- Minimum possible size. We prefer clarity and alignment over bit-packing. A future format version may introduce column compression for the `edges` table.
- Cross-endian file sharing. We write big-endian only; little-endian hosts convert at read time. This is not a portability restriction — every modern CPU can do big-endian cheaply.
- Backward-writing compatibility (an older writer should not be expected to produce a newer file). Only backward reading.

---

## 2. File Envelope

### 2.1 Container (v1)

In format version 1, the container is redb v2.x. The Chrona file IS a redb file, with Chrona-defined redb table definitions inside it.

This means:

- The file begins with redb's own magic bytes and header. Chrona does not overwrite this.
- Chrona metadata, including the format version, lives in the `meta` logical table (§4.1), not in a Chrona-owned file header.
- Tools that understand redb (e.g. `redb-dump`) can inspect the raw tables, but will see only the key/value byte strings — interpretation requires a Chrona-aware reader.

**Why no custom file header in v1.** Prepending Chrona bytes before redb's header would require a custom page manager. That is v2 work. Instead, the presence of the `meta` table keys `chrona.magic` and `chrona.format_version` is how a reader recognizes a Chrona database inside a redb file. See §2.3.

### 2.2 Container (v2+, reserved)

Format version 2 and later MAY introduce a native page manager with a Chrona file header:

```
Offset  Size  Field
0       4     Magic: 'C' 'H' 'R' 'N' (0x43 0x48 0x52 0x4E)
4       2     Format version (big-endian u16)
6       2     Flags (big-endian u16; bit semantics TBD)
8       8     Page size in bytes (big-endian u64)
16      8     Root page index (big-endian u64)
24      8     WAL head offset (big-endian u64)
32      32    Reserved, zero
64      —     Pages begin here
```

This is reserved and not implemented in v1. Implementations MUST NOT rely on it.

### 2.3 Database Identity

The following keys in the `meta` table (§4.1) identify a Chrona database:

| Key (UTF-8) | Value | Required |
|---|---|---|
| `chrona.magic` | `"CHRN"` (4 bytes ASCII) | Yes |
| `chrona.format_version` | Big-endian u16 | Yes |
| `chrona.writer_version` | UTF-8 semver of writer, e.g. `"0.1.0"` | Yes |
| `chrona.created_at` | `Ts` (see §5.2) | Yes |
| `chrona.required_features` | Big-endian u64 bitmap (see §6.2) | Yes |
| `chrona.optional_features` | Big-endian u64 bitmap | Yes |
| `chrona.node_id_counter` | Big-endian u64 | Yes |
| `chrona.edge_id_counter` | Big-endian u64 | Yes |
| `chrona.event_id_counter` | Big-endian u64 | Yes |
| `chrona.string_id_counter` | Big-endian u32 | Yes |

A valid Chrona file MUST have all of these. A reader that opens a redb file lacking `chrona.magic` MUST refuse with a clear error.

---

## 3. Record Formats

All records are byte strings. Each record begins with a **1-byte version tag** and a **1-byte flags byte** unless otherwise noted. Future fields are added by setting higher flag bits and appending bytes at the record's end.

Multi-byte integers are big-endian throughout unless noted.

### 3.1 NodeRecord

Stored as the value in the `nodes` table. Total size: variable.

```
Offset  Size       Field
0       1          version (= 0x01 in format v1)
1       1          flags (see below)
2       8          created_at: Ts (encoded per §5.2)
10      4          type_id: StringId (big-endian u32)
14      4          ext_id_len: u32 (big-endian)
18      ext_id_len ext_id: UTF-8 bytes
...     4          props_len: u32 (big-endian)
...     props_len  props: CBOR encoding of a CBOR map
```

**Flags byte (NodeRecord):**

| Bit | Meaning |
|---|---|
| 0 | `has_type` — if clear, `type_id` bytes are present but ignored; node is untyped |
| 1-7 | Reserved, MUST be 0 |

### 3.2 EdgeRecord

Stored as the value in the `edges` table. Total size: variable.

```
Offset  Size        Field
0       1           version (= 0x01 in format v1)
1       1           flags (see below)
2       8           from: NodeId (big-endian u64)
10      8           to: NodeId (big-endian u64)
18      4           type_id: StringId (big-endian u32)
22      8           valid_from: Ts (encoded per §5.2)
30      8           valid_to: Ts (encoded per §5.2)          — present iff flags.bit(0)
...     8           observed_at: Ts (encoded per §5.2)
...     4           source_id: StringId (big-endian u32)
...     4           confidence: f32 (big-endian IEEE-754)
...     8           supersedes: EdgeId (big-endian u64)       — present iff flags.bit(1)
...     4           props_len: u32 (big-endian)
...     props_len   props: CBOR encoding of a CBOR map
```

**Flags byte (EdgeRecord):**

| Bit | Meaning |
|---|---|
| 0 | `has_valid_to` — if set, the 8-byte `valid_to` field is present after `valid_from` |
| 1 | `has_supersedes` — if set, the 8-byte `supersedes` field is present after `confidence` |
| 2 | `has_props` — if clear, `props_len` MUST equal 0 and `props` is empty |
| 3-7 | Reserved, MUST be 0 |

**Rationale.** Flag-gated optional fields keep the common case small: an edge with no `valid_to` and no revision predecessor is 40 + props bytes rather than 56.

**Confidence invariant.** `0.0 <= confidence <= 1.0`. NaN and out-of-range values MUST be rejected by the writer. Readers encountering such values MAY clamp or reject at their discretion; the canonical behavior is to reject with a `FormatError`.

### 3.3 EventRecord

Stored as the value in the `events` table.

```
Offset  Size          Field
0       1             version (= 0x01 in format v1)
1       1             flags (reserved, MUST be 0 in v1)
2       1             kind: EventKind (u8; see §3.4)
3       4             payload_len: u32 (big-endian)
7       payload_len   payload: kind-specific bytes (see §3.4)
```

### 3.4 EventKind Payloads

Event kind values are stable. A reader that encounters an unknown kind MUST skip the record (the `payload_len` makes this safe) and continue.

| Kind | Value | Payload format |
|---|---|---|
| `NodeAdded` | 1 | `NodeRecord` bytes (§3.1) |
| `NodeRemoved` | 2 | 8 bytes: `NodeId` (big-endian u64) |
| `EdgeObserved` | 3 | `EdgeRecord` bytes (§3.2) |
| `EdgeInvalidated` | 4 | 16 bytes: `EdgeId` (u64 BE) ‖ `at: Ts` (§5.2) |
| `EdgeSuperseded` | 5 | 8 bytes `old: EdgeId` ‖ `EdgeRecord` bytes for new edge |
| `PropertySet` | 6 | 1 byte `scope` (1=node, 2=edge) ‖ 8 bytes `id` ‖ 4 bytes `key_len` ‖ `key` bytes ‖ 4 bytes `value_len` ‖ `value` bytes (CBOR) |
| Reserved | 7–255 | Undefined in v1; readers skip |

### 3.5 StringRecord

Two tables form a bidirectional interner.

**`strings_fwd`**: key is the UTF-8 bytes of the string; value is:

```
0  4  id: StringId (big-endian u32)
```

**`strings_rev`**: key is `id: StringId` (big-endian u32); value is the UTF-8 bytes of the string (no length prefix — the value is the string).

**Invariant:** for every `(bytes → id)` in `strings_fwd`, there is exactly one `(id → bytes)` in `strings_rev`, and vice versa. `chrona verify` checks this.

### 3.6 ExtIdRecord

Stored in `ext_ids`. Key is UTF-8 bytes of the external id. Value is:

```
0  8  node_id: NodeId (big-endian u64)
```

### 3.7 Adjacency Entry (fwd_adj and rev_adj)

The interesting part of adjacency records is the **key**; the value is small.

**Key for `fwd_adj`** (24 bytes):

```
0   8   from: NodeId (big-endian u64)
8   8   valid_from: Ts (encoded per §5.2)
16  8   edge_id: EdgeId (big-endian u64)
```

**Value for `fwd_adj`** (8 bytes):

```
0  8  to: NodeId (big-endian u64)
```

**Key for `rev_adj`** is symmetric, with `to` replacing `from` and the value carrying `from` instead of `to`.

**Range-scan property.** For a given `from`, scanning keys from `(from, Ts::MIN, 0)` to `(from, T, u64::MAX)` yields all edges whose `valid_from <= T`, in valid_from order. This is the foundation of every temporal neighborhood query.

### 3.8 TemporalIdxEntry

Key in `temporal_idx` (16 bytes):

```
0   8   valid_from: Ts (encoded per §5.2)
8   8   edge_id: EdgeId (big-endian u64)
```

Value: empty (0 bytes).

This index supports global "what changed in `[t1, t2]`" scans without a per-node filter.

### 3.9 SupersedesIdxEntry

Key in `supersedes_idx` (16 bytes):

```
0   8   old_edge_id: EdgeId (big-endian u64)
8   8   new_edge_id: EdgeId (big-endian u64)
```

Value: empty. Supports forward walks of revision chains.

---

## 4. Logical Table Catalog

| Table name | Key encoding | Value encoding | Sort order property |
|---|---|---|---|
| `nodes` | `NodeId` (u64 BE) | `NodeRecord` (§3.1) | Numeric asc |
| `edges` | `EdgeId` (u64 BE) | `EdgeRecord` (§3.2) | Numeric asc |
| `fwd_adj` | `(NodeId, Ts, EdgeId)` (§3.7) | `NodeId` (u64 BE) | Lex = (from, valid_from, edge_id) asc |
| `rev_adj` | `(NodeId, Ts, EdgeId)` | `NodeId` (u64 BE) | Lex = (to, valid_from, edge_id) asc |
| `events` | `(Ts, EventId)` | `EventRecord` (§3.3) | Lex = (timestamp, event_id) asc |
| `temporal_idx` | `(Ts, EdgeId)` | `()` empty | Lex = (valid_from, edge_id) asc |
| `supersedes_idx` | `(EdgeId, EdgeId)` | `()` empty | Lex = (old, new) asc |
| `strings_fwd` | UTF-8 bytes | `StringId` (u32 BE) | Lex (bytewise) |
| `strings_rev` | `StringId` (u32 BE) | UTF-8 bytes | Numeric asc |
| `ext_ids` | UTF-8 bytes | `NodeId` (u64 BE) | Lex (bytewise) |
| `meta` | UTF-8 bytes | Opaque bytes | Lex (bytewise) |

Table names are case-sensitive ASCII. Concrete redb table-name strings prepend a `chrona_` prefix to avoid conflicts with any future redb-layer tables (e.g. `chrona_nodes`, `chrona_edges`).

---

## 5. Encoding Rules

### 5.1 Integers

All multi-byte integers are **big-endian**, two's-complement for signed types, IEEE-754 big-endian for floats. A `u32` is 4 bytes; `u64` is 8 bytes; `i64` is 8 bytes after the sign-bit transform in §5.2.

Varints are NOT used in v1. Fixed-width encoding is chosen for simplicity and to preserve lexicographic-matches-numeric ordering in composite keys.

### 5.2 Timestamps (`Ts`)

A `Ts` value is `i64` nanoseconds since the Unix epoch (1970-01-01 00:00:00 UTC), in UTC, with no leap-second adjustment.

On-disk encoding:

```
encoded = (value ^ 0x8000_0000_0000_0000).to_be_bytes()
```

Effect: the sign bit is flipped before big-endian serialization, so `i64::MIN` encodes to `0x0000_...`, zero encodes to `0x8000_...`, and `i64::MAX` encodes to `0xFFFF_...`. Lexicographic byte comparison therefore matches numeric comparison across the entire range.

Decode is the inverse: `i64::from_be_bytes(bytes) ^ 0x8000_0000_0000_0000`.

**Reserved values:**

- `i64::MIN` (`0x0000_0000_0000_0000` after transform) is the sentinel for "unknown / epoch minimum" in a handful of API contexts but is never used as a sentinel for "open-ended `valid_to`" — that is represented via the `has_valid_to` flag (§3.2).
- `i64::MAX` is a valid timestamp (year 2262) and has no special meaning.

### 5.3 Strings

All strings are UTF-8. Strings are validated at write time; invalid UTF-8 is rejected with `FormatError`. Strings are NOT null-terminated; length is carried either by a length-prefix field or by the containing record's total size.

### 5.4 Floats

`f32` and `f64` are IEEE-754 big-endian. NaN encoding is not canonicalized; writers MUST NOT emit NaN for `confidence`. For property-blob values, NaN is permitted and round-tripped as written.

### 5.5 Properties (CBOR)

User properties on nodes and edges are encoded as a single CBOR `map` (major type 5) with string keys. Both the writer and reader use a canonical CBOR subset:

- Integer keys are permitted but discouraged.
- Indefinite-length items are NOT emitted by Chrona writers (deterministic encoding). Readers accept them.
- Tags above 55799 are reserved for Chrona; user data MUST NOT use them.
- Maximum depth of nested maps/arrays: 16.
- Maximum single property blob size: 1 MiB. Larger values require chunking at the application layer.

**Determinism.** When the writer constructs a property map, keys MUST be emitted in lexicographic byte order (CBOR canonical form). This enables byte-deterministic round-trips and makes content-hashing straightforward.

Rejected alternative: MessagePack. CBOR was selected for IETF standardization (RFC 8949), tag extensibility, and broader cross-language library availability.

### 5.6 Composite Keys

Composite keys concatenate their field byte-encodings with no separator. This works because every field has a fixed byte width (see tables in §3.7, §3.8, §3.9). Variable-width fields are NEVER used as non-terminal components of a composite key.

---

## 6. Versioning

### 6.1 Format Version

`chrona.format_version` is a big-endian u16 stored in the `meta` table. Current value: `1`.

Rules:

- Minor format changes (additive, backward-readable) keep the same major number and bump the minor. Example: adding a new event kind, adding a new reserved flag bit.
- Major format changes (structural, breaking) bump the major and reset the minor. Example: changing key encoding for `fwd_adj`.

In v1, the version is a single u16. If major/minor granularity is needed in the future, the upper byte becomes the major and the lower byte becomes the minor, starting from v2. v1 is treated as major = 0, minor = 1.

### 6.2 Feature Bitmaps

Two u64 bitmaps in `meta`:

- `chrona.required_features`: features the writer used that a reader MUST understand to read the file safely.
- `chrona.optional_features`: features the writer used that a reader MAY ignore.

v1 bitmaps:

| Bit | Feature name | Required? |
|---|---|---|
| 0 | `core.v1` | Required |
| 1 | `events.kinds_1_6` | Required |
| 2 | `cbor.canonical` | Required |
| 3 | `redb.container_v2` | Required |
| 4-63 | Reserved | — |

A writer MUST set every bit whose feature it actually uses. A reader:

1. Reads `required_features`.
2. If any bit set there corresponds to a feature the reader does not implement, refuses to open with a `FormatError::UnsupportedFeature`.
3. Otherwise opens normally. Bits in `optional_features` are informational.

### 6.3 Reserved Bytes

All "reserved" flag bits, record-level padding, and event-kind values outside 1–6 are reserved for future use. Writers MUST zero them; readers MUST ignore them.

### 6.4 Migration

v1 has no migration path defined (no prior version). When v2 ships, Chrona SHALL provide a `chrona migrate --from 1 --to 2` CLI subcommand. Migration is out-of-place: the old file is not touched; a new file is written.

---

## 7. Crash Recovery

Recovery is handled by the container layer (redb in v1). On any `open()` after a crash:

1. redb examines its own WAL and rolls forward committed pages, discards in-progress writes.
2. Chrona's view of the file is the state after the most recent `commit()` that returned successfully.
3. Chrona runs no additional recovery — if redb successfully opens, the database is consistent by invariant (§7.1 of ARCHITECTURE.md).

There is no Chrona-level WAL in format v1. All durability is delegated to the container.

Crash scenarios and expected outcomes:

| Scenario | Result |
|---|---|
| Crash before first `commit()` returned | File is empty (or nonexistent); no data |
| Crash during `commit()` fsync | File contains state up to previous commit; partial commit is discarded |
| Crash after `commit()` returned | File contains state up to that commit |
| Corrupt page (bit-flip detected by checksum) | redb returns `StorageError`; Chrona surfaces it; file is not auto-repaired |

A planned `chrona repair` tool (v2) will attempt event-log replay to reconstruct state after page corruption. This is not in v1.

---

## 8. Verification

The `chrona verify <file>` subcommand performs the following checks, in order, aborting on the first failure:

1. **Container integrity.** Delegates to redb's own consistency check.
2. **Magic and version.** Confirms `chrona.magic`, `chrona.format_version`, and feature bitmaps.
3. **Counters.** Confirms `node_id_counter >= max(NodeId in nodes)` and similar for edges and events.
4. **String interner consistency.** For every `(bytes → id)` in `strings_fwd`, confirms `strings_rev[id] == bytes`, and vice versa. Confirms id range is `[1, string_id_counter]`.
5. **Ext-id uniqueness.** Confirms every `node.ext_id` has exactly one entry in `ext_ids` pointing at `node.id`.
6. **Referential integrity.** For every edge in `edges`:
   - `edges[id].from` and `edges[id].to` exist in `nodes`.
   - `fwd_adj` has a matching entry, and its value equals `edges[id].to`.
   - `rev_adj` has a matching entry, and its value equals `edges[id].from`.
   - If `supersedes.is_some()`, the target edge exists and `supersedes_idx` has the matching entry.
7. **Temporal well-formedness.** For every edge with `valid_to.is_some()`, `valid_from <= valid_to`.
8. **Event log well-formedness.** Events are strictly increasing in `(timestamp, event_id)`. Every `EdgeObserved` event's embedded `EdgeRecord` matches `edges[id]` byte-for-byte.
9. **Adjacency completeness.** Count of `fwd_adj` entries equals count of `edges`. Same for `rev_adj`.
10. **Confidence range.** Every edge's `confidence` is in `[0.0, 1.0]` and not NaN.

Any violation is a writer bug or a storage corruption event, never a legal state.

---

## 9. Worked Examples

### 9.1 A Minimal Edge

Edge from node 7 to node 42, type `"WORKS_WITH"` (stringId=12), observed on 2026-04-24T10:00:00Z (Ts = 1_777_629_600_000_000_000), valid from same instant, open-ended, source `"user"` (stringId=3), confidence 1.0, no revision chain, no user props.

EdgeRecord bytes (total 50 bytes):

```
01                              version
00                              flags (has_valid_to=0, has_supersedes=0, has_props=0)
00 00 00 00 00 00 00 07         from = 7
00 00 00 00 00 00 00 2A         to = 42
00 00 00 0C                     type_id = 12
98 A7 4F 20 6D C5 F8 00         valid_from (Ts encoded, sign-bit flipped)
98 A7 4F 20 6D C5 F8 00         observed_at (same instant)
00 00 00 03                     source_id = 3
3F 80 00 00                     confidence = 1.0f32 BE
00 00 00 00                     props_len = 0
```

Layout width: 1 + 1 + 8 + 8 + 4 + 8 + 8 + 4 + 4 + 4 = **50 bytes**. An edge with both optional fields (`valid_to` and `supersedes`) and a 32-byte property blob would be 50 + 8 + 8 + 32 = 98 bytes.

### 9.2 An Adjacency Key

Forward-adjacency key for the same edge, given `edge_id = 99`:

```
00 00 00 00 00 00 00 07         from = 7
98 A7 4F 20 6D C5 F8 00         valid_from (encoded)
00 00 00 00 00 00 00 63         edge_id = 99
```

24 bytes. A range scan on this table from `(7, MIN, 0)` to `(7, 1_777_629_600_000_000_000, u64::MAX)` returns this entry.

### 9.3 An `EdgeInvalidated` Event

Invalidating edge 99 at Ts = 1_780_307_200_000_000_000:

EventRecord bytes:

```
01                              version
00                              flags
04                              kind = 4 (EdgeInvalidated)
00 00 00 10                     payload_len = 16
00 00 00 00 00 00 00 63         edge_id = 99
98 CF 2C 68 FB 40 00 00         at (Ts encoded)
```

Total: 23 bytes.

---

## 10. Tooling

### 10.1 `chrona dump`

`chrona dump <file>` prints a human-readable description of every record, table by table. Output format is line-oriented for grep-friendliness:

```
nodes[1]: ext_id="alice" type="person" created_at=2026-01-15T12:00:00Z props={role:"eng"}
nodes[2]: ext_id="bob" type="person" created_at=2026-01-15T12:00:01Z props={}
edges[1]: 1 -[WORKS_WITH]-> 2 valid=[2026-01-15..) obs=2026-01-15T12:00:00Z src=manual conf=1.00
events[1@2026-01-15T12:00:00Z]: NodeAdded(1)
events[2@2026-01-15T12:00:01Z]: NodeAdded(2)
events[3@2026-01-15T12:00:02Z]: EdgeObserved(1)
```

### 10.2 `chrona fsck`

Alias for `chrona verify` (§8). Added for muscle-memory reasons.

### 10.3 Format test vectors

The repository will ship a directory `fixtures/format/` containing golden files: hand-crafted minimal databases exercising every record type and every event kind, with expected `chrona dump` output. Any change to the format requires regenerating these fixtures and updating this document.

---

## 11. Deliberate Omissions

The following are intentionally absent from v1:

- **Compression.** The `edges` table could benefit from columnar compression. Deferred to v2.
- **Encryption.** Out of scope. Users needing encryption at rest should run on an encrypted filesystem.
- **Column statistics.** Needed for a cost-based query planner. v1 has no cost model.
- **Bloom filters.** Absent. Index scans are cheap enough on MVP workloads.
- **Multi-version edge records.** The record schema itself is versioned via the leading `version` byte, but schema evolution is additive — there is no in-file concept of "some edges use schema v1, others v2". Upgrades are whole-file via `chrona migrate`.

These omissions are tracked as format-future items and will be revisited when justified by a real workload.

---

## 12. Change Log

- **v0.1 (this draft).** Initial specification. Pinned record formats, key encoding, table catalog, versioning strategy, and recovery model. No code exists yet; this document is expected to change as prototype work surfaces issues. Upon first release, a copy of this document SHALL be committed to `docs/format/v1.md` and treated as immutable; further changes go into subsequent versioned copies.

---

*End of FORMAT.md v0.1. See ARCHITECTURE.md for runtime semantics and layer boundaries.*
