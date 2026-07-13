---
title: "refactor: real streaming bytecode decoder (replace lenient stumbling)"
type: refactor
status: completed
date: 2026-07-12
---

# refactor: Real streaming bytecode decoder

## Overview

The streaming metadata parser does not decode the bytecode (BCODESXP) wire
format. `parse_bytecode_streaming_inner` reads three generic objects it labels
`code` / `constants` / `expr` via `parse_object_streaming`, which is
structurally wrong — the real format is `reps_len` (u32) + `code` + a
count-prefixed, type-code-dispatched `constants` list, with **no** separate
`expr` slot. It only avoids desync because streaming skips bytes
opportunistically and a lenient `in_bytecode_context` arm swallows the
BCREPREF/BCREPDEF markers it can't interpret. This plan replaces that
stumbling traversal with a decoder that consumes exactly the right bytes,
mirroring the already-correct sync decoder, while keeping the existing
"Bytecode unsupported" metadata warning (bytecode isn't meaningful metadata
to surface).

## Problem Frame

Last item before cutting 0.2.0. Its *observed* effect is narrow — it affects
only the streaming metadata-inspection path (`inspect_metadata_streaming`) on
files that contain serialized R bytecode; the sync parser (`read_rds`) already
decodes bytecode correctly via `parse_bytecode` → `parse_bytecode_body` →
`parse_bc_constants` / `parse_bc_lang` with a shared `reps` table. But the
"narrow effect" is partly an artifact of weak coverage, not proven safety:
the streaming shortcut relies on a lenient continue-arm and a byte-skip rather
than actually consuming the payload, and the only existing test
(`metadata_warns_on_bytecode`) asserts *only* that the Bytecode warning fires
— it never checks object counts or that objects *after* the bytecode payload
are still seen. In `bytecode_func.rds` the bytecode sits near the end of the
stream, so a desync after it is currently unobservable. So the self-alignment
of the current traversal is untested; a bytecode shape that doesn't happen to
self-align could corrupt every object after it in the streaming walk and no
test would catch it. Making the streaming path decode the true format removes
that fragility, makes the alignment testable, and lets the lenient
BCREPREF/BCREPDEF arm be tightened.

## Requirements Trace

- R1. The streaming path decodes the true BCODESXP wire format: read
  `reps_len` (u32) first, then `code` (an object), then the constants list
  (count-prefixed, each entry dispatched on a type code, not on flags). No
  invented `expr` slot.
- R2. Constant entries that are `BCODESXP` recurse; `BCREPDEF`/`BCREPREF`/
  `LANGSXP`/`LISTSXP`/`ATTRLANGSXP`/`ATTRLISTSXP` are decoded with a shared
  `reps` table exactly as the sync path does; other type codes go through the
  normal streaming object parser. The cursor lands exactly at the end of the
  bytecode payload.
- R3. The streaming visitor still emits the `Bytecode` "unsupported structure"
  metadata warning (unchanged behavior for `inspect_metadata_streaming`
  consumers).
- R4. With correct consumption in place, the streaming BCREPREF/BCREPDEF
  lenient `in_bytecode_context` continue-arm is no longer load-bearing and is
  tightened (bytecode reps are consumed by the new decoder, never reached as
  loose stream objects); reaching one as a standalone stream object is an
  error in all contexts.
- R5. No regression: `inspect_metadata_streaming` still succeeds and produces
  correct downstream object counts / warnings on real fixtures (single
  compiled function, bytecode-in-list, bytecode-in-S4-slot, and any nested /
  rep-using bytecode), and every object *after* a bytecode payload in the
  stream is still parsed correctly (proving no desync).

## Scope Boundaries

- Streaming metadata path only. The sync parser's bytecode decoding is
  correct and is not changed (it is the template).
- Do **not** surface new bytecode metadata (constant counts, nested langs,
  etc.). The warning contract stays: bytecode is reported as an unsupported
  structure. (Considered and rejected — see Alternatives.)
- Do **not** buffer-and-delegate to the sync decoder (would abandon the
  streaming/bounded-memory property; see Alternatives).
- wasm: the streaming metadata path is native (`#[cfg(not(wasm32))]`); the
  wasm async sequential parser is out of scope. Confirm during implementation
  whether an async streaming twin exists that needs the same treatment (see
  Deferred).

## Context & Research

### Relevant Code and Patterns

- **The template (sync, correct):** `src/parser.rs` — `parse_bytecode`
  (reads `reps_len`, allocates `reps`), `parse_bytecode_body` (`code` +
  `parse_bc_constants`), `parse_bc_constants` (count + per-entry type-code
  dispatch, sets `in_bytecode_context`), `parse_bc_lang` (BCREPDEF stores into
  `reps`, BCREPREF reads from it, LANG/LIST via `parse_bc_lang_struct`), and
  `parse_bc_lang_struct`. This is exactly what the streaming version must
  mirror, adapted to the visitor/skip model.
- **The code to replace:** `src/parser.rs` — `parse_bytecode_streaming` (the
  `in_bytecode_context` wrapper) and `parse_bytecode_streaming_inner` (the
  three-generic-object body). The dispatch site is the `BCODESXP =>
  parse_bytecode_streaming(...)` arm in `parse_object_streaming`.
- **The lenient arm to tighten:** the `BCREPREF | BCREPDEF => { if
  ctx.in_bytecode_context { StreamControl::Continue } else { Err } }` arm in
  `parse_object_streaming`.
- **How the sync path differs from a streaming visitor:** the sync decoder
  *builds* `RObject`s; the streaming path *walks/emits* and mostly skips.
  The new streaming decoder must consume the same bytes but need not
  reconstruct the objects — it can skip/emit rather than build, as long as the
  cursor advances identically. The `reps` table for streaming only needs to
  track byte positions or be a no-op if reps are re-walked rather than shared;
  decide during implementation (see Deferred) — but the *consumption* must
  match the sync decoder byte-for-byte.
- **Streaming object entry point:** `parse_object_streaming` (flags-based)
  vs. the bytecode wire format (type-code-based). The new decoder reads raw
  i32 type codes and dispatches, calling `parse_object_streaming` only for the
  "normal object" fall-through cases.
- **Warning emission:** the `Bytecode` arm of the streaming object-type
  labeling (`... => "Bytecode"`) and the `UnsupportedStructure` warning path
  in `src/streaming.rs` (`DatasetInfoVisitor` / `on_object_start`) — must
  still fire.

### Institutional Learnings

- No `docs/solutions/` entries. Directly relevant prior work in this same
  release: the streaming environment desync fix (`locked` field + trailing
  attrib) and the `in_bytecode_context` flag were both added to keep this path
  from desyncing — this plan removes the remaining reliance on the lenient
  arm. The `parse_bytecode_streaming` wrapper that sets/restores
  `in_bytecode_context` was itself added during the hardening work.

### External References

- None needed. R's bytecode serialization format is fully embodied in the
  working sync decoder in this repo; the sync path is the authoritative
  reference. (serialize.c `ReadBC`/`ReadBCLang`/`ReadBC1` correspond 1:1 to
  `parse_bytecode` / `parse_bc_lang` / `parse_bytecode_body` here.)

## Key Technical Decisions

- **Mirror the sync decoder's consumption exactly, in a streaming style.**
  The new `parse_bytecode_streaming_inner` reads `reps_len`, then `code`, then
  the constants count and each constant by type code, recursing on nested
  bytecode and resolving BCREPDEF/BCREPREF against a reps table — the same
  control flow as `parse_bc_constants`/`parse_bc_lang`, but walking/skipping
  instead of building. Rationale: byte-for-byte consumption parity with the
  proven sync path is the correctness bar, and structural symmetry makes the
  two easy to keep in sync.
- **Keep emitting the Bytecode warning (Option A).** Do not expand the
  metadata contract. Rationale: bytecode internals aren't useful metadata for
  `inspect_metadata_streaming` consumers, and surfacing them would change the
  warning contract and `DatasetInfo` shape for no clear user benefit. Correct
  *traversal* is the goal; correct *reporting* already exists (the warning).
- **Tighten the lenient BCREPREF/BCREPDEF arm.** Once the decoder consumes
  reps internally, no rep marker is ever reached as a loose streaming object,
  so the `in_bytecode_context` special-case can become an unconditional error
  (these types are never valid standalone). Rationale: removes the last
  desync-prone shortcut and matches R (which errors on these out of context).
- **Streaming reps table shape is an implementation detail.** The sync reps
  table stores decoded `RObject`s so BCREPREF can return the shared value.
  Streaming doesn't reconstruct, so it either (a) tracks nothing and re-walks
  the referenced structure by remembering byte spans, or (b) keeps a minimal
  presence table just to validate indices. Decide during implementation; the
  invariant that must hold is cursor-position parity with the sync decoder.

## Open Questions

### Resolved During Planning

- Target behavior → correct traversal, keep the Bytecode warning (Option A;
  user-confirmed). Not surfacing new metadata; not buffer-delegating to sync.
- Authoritative format reference → the in-repo sync decoder (no external
  research needed).
- Is the `expr` slot real? → No. R's BCODESXP is `reps_len` + `code` +
  `constants`; the sync `parse_bytecode_body` sets `expr: Null`. The streaming
  version's third `parse_object_streaming` call for "expr" is spurious and
  must be removed.
- **Is there an async/wasm streaming bytecode twin needing the same fix? → No**
  (verified during review). The async streaming entry
  (`parse_object_streaming_async`) buffers a slice (`as_sync_slice` →
  `RdsCursor::new_slice`) and runs the **sync** streaming parser over it;
  there is no async `parse_bytecode` variant. The wasm path therefore already
  routes bytecode through the sync-correct decoder. This fix is native-only.

### Deferred to Implementation

- **Reps table representation for streaming** (track byte spans vs. minimal
  index-presence vs. none) — pick the simplest that yields byte-position
  parity with the sync decoder; validate against a rep-using fixture.
- **Whether a rep-using bytecode fixture already exists** or must be generated.
  `cmpfun` output for a function with a shared/recursive language object in its
  constant pool is the trigger; `tests/generate_test_data.R` already produces
  `bytecode_func.rds` / `bytecode_in_list.rds`. If none of the existing
  fixtures exercise BCREPDEF/BCREPREF, add one (a compiled function whose
  constant pool contains a self-referential or repeated call).
- **Emit vs. skip within the code/constants walk** — whether the visitor
  should see path segments (`code`, `constants[i]`) or the whole payload is
  walked silently under the single Bytecode warning. Match current observable
  behavior (a single Bytecode warning, correct post-payload object counts)
  unless a test says otherwise.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for
> review, not implementation specification. The implementing agent should
> treat it as context, not code to reproduce.*

Wire format the streaming decoder must consume (identical to what the sync
decoder consumes), contrasted with what the current streaming code wrongly
assumes:

```
BCODESXP payload (after the type flags word):

  ACTUAL (sync decoder, correct):        CURRENT STREAMING (wrong):
  ┌─ reps_len : u32                      ┌─ (reps_len NOT read)
  ├─ code     : one object               ├─ code      : parse_object_streaming
  └─ constants:                          ├─ constants : parse_object_streaming
       count : u32                       └─ expr      : parse_object_streaming
       for each of `count`:                 (expr does not exist on the wire)
         type_code : i32
         ├ BCODESXP        -> recurse (reps shared)
         ├ BCREPDEF        -> idx:u32, inner_type:i32, decode, store reps[idx]
         ├ BCREPREF        -> idx:u32, return reps[idx]
         ├ LANG/LIST/ATTR* -> parse_bc_lang_struct (may nest reps)
         └ otherwise       -> normal object
```

The fix reshapes `parse_bytecode_streaming_inner` to the left column, walking
(not building) each piece so the cursor advances exactly as the sync decoder's
would. The single `Bytecode` unsupported-structure warning is emitted once for
the whole payload, as today.

## Implementation Units

- [x] **Unit 1: Characterize current streaming bytecode behavior**

**Goal:** Lock in the observable behavior that must not regress (warning
emitted, object counts after the payload correct) before changing the decoder,
and expose whether current fixtures exercise reps.

**Requirements:** R3, R5 (guards them)

**Dependencies:** none

**Files:**
- Test: `tests/streaming_metadata_tests.rs` (extend), possibly
  `tests/bytecode_tests.rs`
- Reference: `tests/generate_test_data.R` (existing bytecode fixtures)

**Execution note:** Characterization-first — capture current streaming
metadata output (warnings + object/vector counts) on the existing bytecode
fixtures as assertions, so the refactor is provably behavior-preserving for
the observable surface.

**Approach:**
- Assert current `inspect_metadata_streaming` output on `bytecode_func.rds`
  and `bytecode_in_list.rds`: the Bytecode `UnsupportedStructure` warning
  fires, and the object/vector counts and any post-bytecode siblings match the
  eager parse's structure (use `read_rds` as the oracle for what should follow
  the payload).
- Add an assertion that a non-bytecode object *following* a bytecode payload
  in the same stream is still seen by the visitor (desync guard). If no such
  fixture exists, note it for Unit 3.

**Test scenarios:**
- Happy path: `inspect_metadata_streaming(bytecode_func.rds)` returns Ok, warns
  Bytecode once, object_count matches the eager-parse object count.
- Happy path: `bytecode_in_list.rds` — the list and its non-bytecode siblings
  are all visited; Bytecode warning present.
- Edge/desync: a stream with an object *after* the bytecode payload — that
  trailing object is visited (guards R5). (Fixture may be added in Unit 3.)

**Verification:** New characterization tests pass against the *current* code
(they encode present behavior), giving a regression net for Unit 2.

- [x] **Unit 2: Real streaming bytecode decoder**

**Goal:** Replace `parse_bytecode_streaming_inner` with a decoder that consumes
the true wire format (reps_len + code + type-code-dispatched constants +
reps), and remove the spurious `expr` read. Tighten the lenient
BCREPREF/BCREPDEF streaming arm.

**Requirements:** R1, R2, R3, R4

**Dependencies:** Unit 1

**Files:**
- Modify: `src/parser.rs` (`parse_bytecode_streaming` /
  `parse_bytecode_streaming_inner`, the `BCREPREF | BCREPDEF` arm in
  `parse_object_streaming`, and possibly small streaming twins of
  `parse_bc_constants` / `parse_bc_lang` / `parse_bc_lang_struct`)
- Modify: `src/streaming.rs` only if the warning path needs adjustment (it
  should not — the Bytecode label/warning stays)
- Test: `tests/streaming_metadata_tests.rs`, `tests/bytecode_tests.rs`

**Approach:**
- Read `reps_len` (u32) first; establish the streaming reps table (shape per
  the deferred decision). Then walk `code` (one object) and the constants
  list: read `count`, then for each entry read the i32 type code and dispatch
  — recurse on `BCODESXP`, handle `BCREPDEF`/`BCREPREF`, route LANG/LIST/ATTR*
  through a streaming `parse_bc_lang`-equivalent, and fall through to
  `parse_object_streaming` for normal objects. Drop the third (`expr`) call.
- Keep emitting the single Bytecode `UnsupportedStructure` warning (the
  BCODESXP object-type labeling already drives this; leave it).
- After the decoder consumes reps correctly, change the `BCREPREF | BCREPDEF`
  arm in `parse_object_streaming` to error unconditionally (no more
  `in_bytecode_context` special-case). Confirm `in_bytecode_context` is then
  either removed or retained only if still used elsewhere.

**Patterns to follow:**
- `parse_bytecode` / `parse_bytecode_body` / `parse_bc_constants` /
  `parse_bc_lang` / `parse_bc_lang_struct` (sync) — same control flow, walking
  instead of building.

**Test scenarios:**
- Happy path: all Unit 1 characterization tests still pass (behavior-
  preserving on observable surface) — warning fires, counts match.
- Integration (byte parity / desync): a stream that places a known object
  immediately after a bytecode payload — the trailing object is parsed with
  correct type/value in the streaming walk, proving the decoder consumed
  exactly the bytecode bytes (this is the core proof the stumbling is gone).
- Edge (reps): a bytecode fixture whose constant pool uses BCREPDEF/BCREPREF
  (nested/shared language object) — streaming inspection succeeds, warns
  Bytecode, and the post-payload stream stays aligned.
- Edge (nested bytecode): a constant that is itself BCODESXP — recursion
  consumes it and the walk stays aligned.
- Error path: a standalone BCREPREF/BCREPDEF appearing as a top-level stream
  object (not inside bytecode) → `InvalidFormat` (the tightened arm), matching
  the sync/R behavior.
- Error path: a truncated bytecode payload → a parse error surfaced through
  `inspect_metadata_streaming`, not a panic or silent success.

**Verification:** streaming and eager parses agree on structure for every
bytecode fixture; the trailing-object desync test passes; the lenient arm is
gone; full native suite + `inspect_metadata_streaming` green.

- [x] **Unit 3: Fixtures and cross-parser equivalence**

**Goal:** Ensure a rep-using and a trailing-object fixture exist, and assert
streaming-vs-eager structural equivalence across all bytecode shapes.

**Requirements:** R5

**Dependencies:** Unit 2 (may be co-developed with it)

**Files:**
- Modify: `tests/generate_test_data.R` (add a rep-using compiled-function
  fixture and/or a bytecode-then-trailing-object fixture if not already
  present)
- Test: `tests/streaming_metadata_tests.rs`, `tests/bytecode_tests.rs`

**Approach:**
- If existing fixtures don't exercise BCREPDEF/BCREPREF, add a compiled
  function whose constant pool contains a repeated/self-referential call (R's
  compiler emits reps for shared language structures). Add a fixture with a
  non-bytecode object serialized after a bytecode object if the desync test
  needs one.
- Assert that for each bytecode fixture, the set of objects/vectors the
  streaming visitor reports is consistent with the eager `read_rds` structure
  (same trailing siblings, same warning).

**Test scenarios:**
- Integration: for each of {simple compiled fn, bytecode-in-list,
  bytecode-in-S4-slot (`s4_command_bundle`-style), rep-using bytecode},
  streaming inspection agrees with eager parse on post-payload structure and
  warns Bytecode.
- Edge: rep-using fixture specifically confirms BCREPREF resolution doesn't
  desync (the case the old lenient arm hand-waved).

**Verification:** new fixtures generated by `tests/generate_test_data.R`;
equivalence tests green; `wasm-pack test --node` unaffected (native-only path).

## System-Wide Impact

- **Interaction graph:** `inspect_metadata_streaming` → `traverse_rds_streaming`
  → `parse_object_streaming` (BCODESXP arm) → the new decoder; the
  `DatasetInfoVisitor` warning path in `src/streaming.rs`. The tightened
  `BCREPREF | BCREPDEF` arm affects any streaming walk that encounters those
  types.
- **Error propagation:** truncated/short bytecode now surfaces a real
  `StreamingError::Parse` instead of stumbling; a standalone rep marker errors
  in all contexts.
- **Unchanged invariants:** the sync `read_rds` bytecode decoder; the
  `Bytecode` `UnsupportedStructure` warning contract; `DatasetInfo` shape; the
  wasm build (the async streaming path buffers and delegates to the sync
  decoder, so it is unaffected — no async twin exists).
- **Integration coverage:** the trailing-object-after-bytecode test is the
  cross-layer proof unit-level assertions can't give — it's the direct
  evidence the desync-prone shortcut is gone.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Streaming decoder consumes different bytes than sync (subtle desync) | Byte-position parity is the bar; the trailing-object-after-bytecode test fails loudly on any drift; the sync decoder is the exact template |
| Reps table semantics differ subtly between build (sync) and walk (streaming) | Reps only need index/position validity in streaming; a rep-using fixture pins it; if unsure, mirror the sync structure and just discard built values |
| Existing fixtures don't exercise reps → fix looks done but the hard path is untested | Unit 3 explicitly adds a rep-using fixture; Unit 1 surfaces whether current fixtures cover it |
| Over-scope creep into surfacing bytecode metadata | Scope Boundary + Option-A decision: warning contract unchanged |

## Documentation / Operational Notes

- CHANGELOG 0.2.0: a small note that the streaming metadata parser now decodes
  the bytecode wire format correctly (previously a lenient traversal that
  relied on byte-skipping); the Bytecode-unsupported warning is unchanged. Low
  user impact — `inspect_metadata_streaming` behavior is the same except it no
  longer risks desync on unusual bytecode.
- Re-run `Rscript tests/generate_test_data.R` after adding fixtures (repo
  convention; fixtures are gitignored).
- This is the last pre-cut item; after it lands, the 0.2.0 cut is
  verification → bump `Cargo.toml` → stamp CHANGELOG date → merge to main.

## Alternative Approaches Considered

- **Surface real bytecode metadata** (constant counts, nested closures):
  rejected — expands the warning/`DatasetInfo` contract for no clear consumer
  benefit; the goal is correct traversal, and correct reporting (the warning)
  already exists.
- **Buffer the bytecode span and delegate to the sync `parse_bytecode`:**
  rejected — abandons the streaming/bounded-memory property that is the whole
  point of the streaming path, and doesn't fit the visitor model cleanly.
- **Compute the bytecode payload's byte length and skip exactly that span**
  (instead of a full type-code-dispatch walk): **not viable** (confirmed in
  review). The payload length is not knowable without walking the
  variable-length, recursive, reps-using type-code dispatch — there is no
  length prefix for the whole BCODESXP body. So any correct byte-consumption
  requires the walk; there is no shortcut that skips without decoding.
- **Leave the lenient stumbling as-is and just document it:** rejected here
  because the user chose to complete the fix for 0.2.0; it remains the
  fallback if the effort/risk proves worse than expected during Unit 2.

## Sources & References

- Template (sync decoder): `src/parser.rs` — `parse_bytecode`,
  `parse_bytecode_body`, `parse_bc_constants`, `parse_bc_lang`,
  `parse_bc_lang_struct`
- Code to replace: `src/parser.rs` — `parse_bytecode_streaming`,
  `parse_bytecode_streaming_inner`, the `BCREPREF | BCREPDEF` streaming arm
- Warning path: `src/streaming.rs` — `DatasetInfoVisitor`; the `Bytecode`
  object-type label in `parse_object_streaming`
- Tests/fixtures: `tests/streaming_metadata_tests.rs`,
  `tests/bytecode_tests.rs`, `tests/generate_test_data.R`
  (`bytecode_func`, `bytecode_in_list`)
- Format authority: R `serialize.c` `ReadBC`/`ReadBC1`/`ReadBCLang`
  (1:1 with the sync decoder in this repo)
