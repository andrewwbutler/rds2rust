---
title: "feat: materialize lazy character vectors"
type: feat
status: completed
date: 2026-07-12
---

# feat: Materialize lazy character vectors

## Overview

`materialize_vector` returns `Unsupported` for `RObject::Character`, while
Integer/Real/Logical/Raw/Complex all materialize. Character is the only hole,
and it's a common one (string columns, factor source data). Close it by
reusing the already-hardened `read_lazy_character_range` decoder from
`chunk_iter.rs`, mirroring the existing per-type materialization pattern.

## Problem Frame

A lazy character span is variable-length: its bytes are a run of CHARSXP
entries (each a flags word + i32 length + payload, or a length of -1 for NA)
plus intra-vector REFSXP dedup back-references. Unlike the fixed-stride
numeric spans, it can't use `validate_byte_len`/`slice_for_span` element math.
But a decoder for exactly this shape already exists —
`chunk_iter::read_lazy_character_range(source, span, start, count)` — and it
returns `Vec<Option<Arc<str>>>`, precisely the element type
`VectorData<Option<Arc<str>>>` needs (NA handling included). The only
impedance mismatch: that reader takes a `&dyn RdsInput`, while
`MaterializationContext` holds a `&[u8]` (the full decompressed stream). The
character span's `offset` is absolute into that same buffer, so a trivial
slice→`RdsInput` adapter bridges the two.

## Requirements Trace

- R1. `materialize_vector` materializes `Character` lazy spans instead of
  erroring, in-place (`VectorData::Lazy` → `VectorData::Owned`).
- R2. Materialized output is identical (element-for-element, including NA and
  intra-vector references) to what the eager parser produces for the same
  file.
- R3. Public parity: a `materialize_character_vector` /
  `materialize_character_data` pair matches the existing per-type API surface
  (both the `MaterializationContext` methods and the free functions).
- R4. No regressions; the memory-budget path behaves consistently with the
  numeric types (character honors the budget via `check_budget`).

## Scope Boundaries

- Native only. The character range reader (`read_lazy_character_range`,
  `SpanReader`) is already `#[cfg(not(target_arch = "wasm32"))]`; wasm keeps
  its separate async range API and is out of scope here.
- No change to how lazy character spans are *produced* by the parser — only
  how an existing span is materialized.
- Reuse the existing decoder; do not write a second CHARSXP parser.

## Key Technical Decisions

- **Reuse `read_lazy_character_range(source, span, 0, span.length)`** rather
  than reimplementing CHARSXP decoding. It was heavily exercised and hardened
  during the in-vector-ref work (1-based index checks, NA handling), so
  correctness comes for free and there's one decoder to maintain.
- **Add a small slice `RdsInput` adapter** (e.g. `SliceRdsInput<'a>(&'a [u8])`)
  so `MaterializationContext`'s `&[u8]` can drive the range reader. The reader
  addresses bytes by absolute offset, matching how the span's `offset` indexes
  the decompressed buffer. Keep it private to `materialization.rs` unless a
  public need appears.
- **Budget accounting uses `span.byte_len`** (the on-wire size), consistent
  with the numeric materializers which charge `span.byte_len` before decoding.
  Charge once up front via `check_budget`.
- **`materialize_vector` returns `Ok(true)`** for Character on success, like
  the other arms (true = "this was a lazy vector that got materialized").

## Open Questions

### Resolved During Planning

- Which decoder to use → `chunk_iter::read_lazy_character_range` (exists,
  hardened, returns the right element type).
- How to bridge `&[u8]` → `RdsInput` → a private slice adapter.
- NA / intra-vector-ref correctness → inherited from the reused decoder;
  pinned by R2's equivalence test.

### Deferred to Implementation

- Exact placement/visibility of the slice adapter (materialization.rs-private
  vs. a shared test helper) — decide when wiring it; there's a `BytesInput`
  pattern already used in tests that can be promoted if convenient.
- Whether `read_lazy_character_range`'s default `ChunkConfig` needs any tuning
  for the whole-vector (0..length) case — expected no; verify no perf cliff on
  a large fixture.

## Implementation Units

- [x] **Unit 1: Slice RdsInput adapter + character materializer**

**Goal:** `MaterializationContext::materialize_character_vector(span) ->
Vec<Option<Arc<str>>>` and `materialize_character_data(&mut VectorData<...>)`,
plus their free-function counterparts, backed by the reused decoder.

**Requirements:** R1, R3, R4

**Dependencies:** none

**Files:**
- Modify: `src/materialization.rs`
- (Reuse) `src/chunk_iter.rs::read_lazy_character_range`
- Test: `tests/materialization_tests.rs` (extend if it exists; else the new
  cases live in `tests/lazy_parsing_tests.rs` next to the existing lazy tests)

**Approach:**
- Add a private `struct SliceRdsInput<'a>(&'a [u8])` implementing `RdsInput`
  (`read_at` = bounded slice copy from the absolute offset; `len` = slice
  length), mirroring the `BytesInput` used in `tests/na_character_tests.rs`.
- Add `MaterializationContext::materialize_character_vector(span)`: charge
  `check_budget(span.byte_len as usize)`, then
  `read_lazy_character_range(&SliceRdsInput(self.data), span, 0, span.length)`.
- Add `materialize_character_data` (Lazy → Owned) following the numeric
  `_data` methods exactly.
- Replace the `Character(_) => Err(Unsupported)` arm in `materialize_vector`
  with `Character(v) => { ctx.materialize_character_data(v)?; Ok(true) }`.
- Add the two public free functions (`materialize_character_vector`,
  `materialize_character_data`) mirroring the numeric free fns.

**Patterns to follow:**
- The `materialize_real_*` method/free-fn pair (structure, budget call).
- `tests/na_character_tests.rs`'s `BytesInput` for the slice adapter shape.

**Test scenarios:**
- Happy path: a character vector large enough to stay lazy (> default lazy
  threshold, ~50 elements) writes → lazy-parses → materialize → equals the
  eager parse of the same bytes.
- Edge case (NA): the vector includes a `None` element; materialized output
  has `None` at that index, distinct from a real `"NA"` string.
- Edge case (intra-vector ref): a vector with a repeated string that R/rds2rust
  encodes as an in-vector REFSXP; materialized output resolves it to the same
  string (reuse the hand-built stream shape from `tests/invector_ref_tests.rs`).
- Edge case (empty / already-owned): materializing an already-`Owned`
  character vector is a no-op; a zero-length lazy span yields `[]`.
- Error path: a truncated character span surfaces `TruncatedLazyPayload`
  (inherited from `SpanReader`), not a panic.
- Integration: `materialize_path` on a data-frame string column (path like
  `column_name`) materializes it and leaves siblings lazy.
- Budget: with a `with_budget` smaller than `span.byte_len`, materialization
  returns `MemoryBudgetExceeded` (consistent with numeric types).

**Verification:**
- The Character arm no longer returns `Unsupported`; the equivalence test
  (materialize == eager) passes on native.
- Full native suite green; fmt/clippy clean; wasm32 still compiles (adapter is
  native-gated alongside the reader).

## System-Wide Impact

- **Interaction graph:** `materialize_vector` (dispatch),
  `materialize_tokens`/`materialize_path`/`materialize_paths_with_budget`
  (all now reach Character columns), and any caller that materializes a
  parsed-lazy tree.
- **Unchanged invariants:** lazy span *production* in the parser; the numeric
  materializers; the wasm async range API; the public `RObject` shape.
- **API surface parity:** the new free functions complete the
  `materialize_*_vector` / `materialize_*_data` family so Character is no
  longer the odd one out.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Materialized output diverges from eager parse (ref/NA edge cases) | R2 equivalence test asserts element-for-element equality against the eager parser on the same bytes |
| Slice adapter offset mismatch (relative vs. absolute) | The span's `offset` is absolute into the decompressed buffer, which is exactly `self.data`; the reader reads at absolute offsets — verified against the existing `read_lazy_character_range` contract |
| Reversing a documented decision (NA plan scoped this as `Unsupported` follow-up) | Intentional per the 0.2.0 "include everything" call; CHANGELOG notes the newly-supported capability |

## Documentation / Operational Notes

- CHANGELOG 0.2.0: character lazy vectors now materialize (was `Unsupported`);
  the plan's earlier "documented limitation" note is superseded.
- Re-run `Rscript tests/generate_test_data.R` only if a new fixture is added
  (the equivalence tests can build their bytes in-process like the existing
  lazy tests, avoiding new fixtures).

## Sources & References

- Reused decoder: `src/chunk_iter.rs::read_lazy_character_range`
- Pattern: `src/materialization.rs` numeric `materialize_*` pairs
- Span production: `src/parser.rs` character `VectorData::Lazy` sites
- Adapter shape: `tests/na_character_tests.rs::BytesInput`
