---
title: "fix: Consume PACKAGESXP payloads and close wasm parser payload gaps"
type: fix
status: completed
date: 2026-07-09
---

# fix: Consume PACKAGESXP payloads and close wasm parser payload gaps

> **Superseded (2026-07-12):** this plan surfaced PACKAGESXP as `RObject::Namespace`
> with a `"package:"` name-prefix convention. That was later replaced by the
> dedicated `RObject::PackageEnv` variant — see the CHANGELOG 0.2.0 section.
> The design notes below describe the intermediate approach, not current behavior.

## Overview

PRs #2 and #3 (merged 2026-07-09) fixed stream desynchronization for PERSISTSXP payloads and reference-table misalignment, but each disclosed known gaps. This plan closes the two correctness gaps selected for this iteration:

1. **PACKAGESXP payloads are still not consumed.** The sync parser returns `Null` without reading the `OutStringVec` payload, and post-#3 the entry *also* occupies a reference-table slot — so any RDS file containing a package environment both desyncs the cursor and leaves a dangling `Null` ref slot. This is the exact same bug class PR #2 fixed for PERSISTSXP.
2. **The wasm/async sequential parsers never consume string-vector payloads** for PERSISTSXP, PACKAGESXP, or NAMESPACESXP — they fall through to `_ => RObject::Null`. Disclosed as out of scope in PR #2; PR #3 updated the wasm ref-tracking rules but not the payload consumption.

## Problem Frame

R's `serialize.c` writes three pseudo-types with an `OutStringVec` payload (an i32 names placeholder, a `WriteLENGTH`-encoded count, then one inline CHARSXP per string): PERSISTSXP (247), PACKAGESXP (248), and NAMESPACESXP (249). On the read side, R consumes the payload and calls `AddReadRef`, so each entry occupies exactly one reference slot. A reader that skips the payload desyncs silently — everything after the entry parses as garbage with no error.

The native parser now handles PERSISTSXP (PR #2) and NAMESPACESXP (`parse_namespace`), but not PACKAGESXP. The wasm sequential paths handle none of the three. Verified this session: R serializes `as.environment("package:stats")` as `0xf8` (PACKAGESXP) + string payload, and `src/parser.rs` line ~4521 returns `Null` without consuming it.

## Requirements Trace

- R1. RDS files containing PACKAGESXP entries parse without desync in the sync and streaming native parsers; objects after the entry are structurally correct.
- R2. A PACKAGESXP entry occupies exactly one reference-table slot and later REFSXP back-references resolve to the package object (matching R's `AddReadRef`).
- R3. The wasm/async sequential parse and skip paths consume PERSISTSXP, PACKAGESXP, and NAMESPACESXP payloads and stay stream-aligned, producing the same structure as the native parser.
- R4. No regressions: full native suite passes; the real-world help-database spot check (Rd entries vs `tools:::fetchRdDB`) still matches R.

## Scope Boundaries

Explicitly **out of scope** (noted for a future session — see also the deferred-gaps memory note):

- Defensive hardening: validating the `OutStringVec` names placeholder (R errors on non-zero), type-checking CHARSXP item flags, and stopping the out-of-bytecode-context `BCREPREF | BCREPDEF` arm in `parse_object` from misusing the main ref table.
- Performance follow-ups: hash-indexed dedup cache (currently a linear scan), avoiding the TAG REFSXP deep clone for non-symbol referents.
- `NA_character_` representation (`RObject::Character` currently maps NA to the literal `"NA"`) — a public API design change needing its own brainstorm.

## Context & Research

### Relevant Code and Patterns

- `src/parser.rs` — `parse_persistsxp_strings` (from PR #2): the model implementation of an `OutStringVec` consumer, including the long-vector `-1` marker, negative-count rejection, and `guard_allocation`. The PACKAGESXP fix should generalize/reuse this rather than duplicate it.
- `src/parser.rs` — `parse_namespace` (~line 8099): consumes the same wire shape but *less* rigorously (plain u32 length, no long-vector marker, no negative-count check, routes CHARSXPs through full `parse_object`). Should be unified onto the shared helper.
- `src/parser.rs` — the `NAMESPACESXP_SERIAL | BASENAMESPACE_SXP` arm (~line 4501): the pattern for filling the ref-table placeholder and returning `RObject::Shared` — PACKAGESXP should mirror it.
- `src/parser.rs` — the streaming `PERSISTSXP` arm (~line 5163, from PR #2): the pattern for payload consumption in `parse_object_streaming`; PACKAGESXP currently sits in the consume-nothing `Continue` arm list (~line 5159).
- `src/parser.rs` — `parse_charsxp_content_async` (~line 4074): the pattern for mirroring a sync consumer as a wasm async fn over `AsyncCursor`.
- `tests/persistsxp_tests.rs` + the PERSISTSXP section of `tests/generate_test_data.R`: the template for fixture-backed alignment tests (skip when `tests/data` absent; fixtures are gitignored and regenerated locally).
- `tests/wasm_lazy_vector_tests.rs`: the pattern for wasm tests — synthetic in-memory byte streams via a local `AsyncRdsInput` impl, `wasm_bindgen_test`. Do **not** use `include_bytes!` on fixtures (gitignored; would break builds).
- `should_track_reference` (~line 763): already includes PACKAGESXP/NAMESPACESXP (from PR #3) — no change needed there; the missing half is payload consumption and placeholder fill.

### Institutional Learnings

- No `docs/solutions/` directory exists. The relevant institutional knowledge is this session's verified review of PRs #2/#3 (empirically validated against R 4.3.3 and R's `serialize.c` semantics).

### External References

- R `src/main/serialize.c`: `OutStringVec` / `InStringVec`, `HashAdd` / `AddReadRef` — the authoritative wire-format reference, already consulted and verified this session. Note R's *reader* uses a plain `InInteger` for the count (no long-vector marker), so handling the marker is strictly more permissive than R — matching what PR #2 already does.
- R behavior verified locally: `serialize(as.environment("package:stats"), NULL)` emits `0xf8` + placeholder + length 1 + CHARSXP `"package:stats"` (with a harmless "may not be available when loading" warning — suppress in the fixture script).

## Key Technical Decisions

- **Generalize `parse_persistsxp_strings` into a shared `OutStringVec` consumer** used by PERSISTSXP, PACKAGESXP, and `parse_namespace`: one implementation of the wire shape instead of three. Rationale: `parse_namespace`'s hand-rolled version lacks the long-vector and negative-count handling; divergence is how the PACKAGESXP gap survived two fixes.
- **Surface PACKAGESXP as `RObject::Namespace(names)`** (the existing variant used for NAMESPACESXP). Rationale: both are "an environment identified by name strings, resolved at load time"; R's package-env names carry the `package:` prefix so consumers can distinguish them; reusing the variant avoids adding a new public enum variant (which would break exhaustive matches downstream). Alternative considered: a character vector like PERSISTSXP — rejected because PACKAGESXP has an existing semantic sibling and, unlike PERSISTSXP hook strings, the payload is always an environment name.
- **Fill the ref-table placeholder and return `RObject::Shared`**, mirroring the NAMESPACESXP arms. Rationale: R `HashAdd`s package envs *before* writing the payload, so a second occurrence of the same package env is a REFSXP back-reference — the placeholder must resolve to the package object, not `Null`.
- **wasm: mirror the shared consumer as an async fn** rather than trying to abstract over sync/async cursors. Rationale: matches the codebase's existing sync/async duplication pattern (`parse_charsxp_content` / `parse_charsxp_content_async`); an abstraction over both cursor types would be a bigger refactor than the bug warrants.
- **wasm tests use small synthetic streams embedded as byte literals** (with the generating R one-liner in a comment), following `tests/wasm_lazy_vector_tests.rs`. Rationale: fixtures are gitignored; wasm tests can't read the filesystem; a PACKAGESXP stream is ~40 bytes.

## Open Questions

### Resolved During Planning

- Does PACKAGESXP need ref tracking changes? — No; PR #3 already added it to `should_track_reference`. Only payload consumption and placeholder fill are missing.
- Can wasm changes be tested for real? — Yes; `wasm-pack`, node, and the wasm32 target are installed, and three test files already use `wasm_bindgen_test`.
- Is a second occurrence of a package env a REFSXP or a repeated PACKAGESXP? — REFSXP (unlike PERSISTSXP, where the hook takes precedence): R checks `HashGet` before the hook-less package path. Tests must assert shared identity, not repeated payloads.

### Deferred to Implementation

- The exact set of wasm async functions needing arms (`parse_object_async`, `parse_object_streaming_async`, `parse_object_sequential_value_async`, `skip_object_sequential_value_async`) — enumerate by tracing `_ =>` fallthroughs when editing; they may not all dispatch pseudo-types the same way.
- Whether `wasm-pack test` currently passes on this repo at all (existing tests are configured `run_in_browser`; a node-friendly configuration may be needed, or run headless browser tests). If the harness proves broken for pre-existing reasons, fall back to `cargo check --target wasm32-unknown-unknown` plus native-mirror unit tests of the shared consumer, and surface the harness state to the user rather than fixing it silently.
- Whether `parse_namespace`'s switch to the shared helper changes behavior on any existing fixture (it shouldn't — the helper is strictly more careful — but the full suite is the net).

## Implementation Units

- [x] **Unit 1: Shared OutStringVec consumer + native PACKAGESXP fix (sync + streaming)**

**Goal:** PACKAGESXP payloads are consumed in both native parsers; the entry fills its ref slot with a `Namespace` object; fixtures and tests prove alignment and back-reference resolution.

**Requirements:** R1, R2, R4

**Dependencies:** None

**Files:**
- Modify: `src/parser.rs`
- Modify: `tests/generate_test_data.R`
- Test: `tests/packagesxp_tests.rs` (new)

**Approach:**
- Rename/generalize `parse_persistsxp_strings` to a neutral name (e.g., an `OutStringVec` consumer); keep behavior identical. PERSISTSXP call sites unchanged in behavior.
- Replace the sync `PACKAGESXP => RObject::Null` arm: consume the payload, build `RObject::Namespace(names)`, fill the ref-table placeholder, return `RObject::Shared` (mirror the `NAMESPACESXP_SERIAL` arm's shape).
- Move PACKAGESXP out of the streaming parser's consume-nothing arm list into its own payload-consuming arm (mirror the streaming PERSISTSXP arm from PR #2).
- Add fixtures to `tests/generate_test_data.R` (wrap `serialize()` calls in `suppressWarnings`): a bare package env; a list with the same package env twice plus a trailing string and a shared symbol after it; a package env inside an attribute.

**Patterns to follow:**
- `tests/persistsxp_tests.rs` for test structure (data-dir skip guard, `unwrap`/`attr_of` helpers, alignment assertions).
- The `NAMESPACESXP_SERIAL | BASENAMESPACE_SXP` arm for placeholder fill semantics.

**Test scenarios:**
- Happy path: `serialize(as.environment("package:stats"))` parses as `Namespace(["package:stats"])`.
- Happy path (alignment): `list(p = pkg_env, after = "still-aligned")` — the element after the package env parses correctly.
- Integration (ref slots): `list(p = pkg_env, p2 = pkg_env, s = as.name("x"), s2 = as.name("x"), tail = "ok")` — `p2` resolves to the same shared object as `p` (REFSXP back-reference), and the symbol back-reference `s2` after it resolves correctly (proves the slot count matches R's writer).
- Integration (attributes): a package env inside an attribute, with a trailing attribute intact (the PERSISTSXP-in-attributes shape).
- Integration (streaming): streaming traversal of the alignment fixture sees the trailing element's vector metadata and the `names` attribute (mirror `test_persistsxp_streaming_alignment`).
- Edge case: synthetic stream with a negative (non-`-1`) count is rejected with `InvalidFormat`, not a panic or silent desync (hand-built bytes; the shared helper already rejects — this pins the behavior for PACKAGESXP).

**Verification:**
- All new tests fail on the pre-change parser and pass after (swap-check like the PR reviews did).
- Full native suite passes after regenerating fixtures.

- [x] **Unit 2: Unify `parse_namespace` onto the shared consumer** *(execution finding: also fixed BASENAMESPACE_SXP consuming a phantom payload — R writes it with no payload; sync + streaming arms split accordingly, with fixtures/tests)*

**Goal:** One implementation of the `OutStringVec` wire shape; `parse_namespace` gains long-vector and negative-count handling for free.

**Requirements:** R4 (plus hardening spillover for R1's wire shape)

**Dependencies:** Unit 1

**Files:**
- Modify: `src/parser.rs`

**Approach:**
- Replace `parse_namespace`'s hand-rolled placeholder/length/loop with a call to the shared consumer; keep returning `RObject::Namespace(names)`.
- Behavior note: the current loop routes each CHARSXP through full `parse_object` and silently drops non-Character results; the shared helper reads flags + `parse_charsxp_content` directly. Valid R streams only ever contain inline CHARSXPs here, so this is equivalent for conforming files — rely on the existing namespace-touching tests (`package_function.rds`, S4 command bundles) as the regression net.

**Test scenarios:**
- Test expectation: none beyond the existing suite — this unit is a refactor to shared code with no intended behavior change on conforming streams; existing namespace fixtures are the net. If any existing test's behavior changes, stop and surface it rather than adapting the test.

**Verification:**
- Full native suite passes unchanged.

- [x] **Unit 3: wasm/async payload consumption for PERSISTSXP, PACKAGESXP, NAMESPACESXP** *(execution findings: `parse_object_async` and `parse_object_streaming_async` delegate to the sync parsers and inherit Units 1–2, so only the two sequential fns needed arms; `wasm-pack test` is broken repo-wide for a pre-existing reason — it builds all test targets and native-only test files don't compile on wasm32 — so tests run via `cargo build --target wasm32-unknown-unknown --test wasm_payload_tests` + `wasm-bindgen-test-runner` under node, all 4 fail-before/pass-after)*

**Goal:** The wasm sequential parse/skip paths consume all three `OutStringVec` payloads and stay stream-aligned, closing the gap disclosed in PR #2.

**Requirements:** R3

**Dependencies:** Unit 1 (shared consumer exists and its semantics are pinned by native tests)

**Files:**
- Modify: `src/parser.rs` (wasm-cfg'd async functions)
- Test: `tests/wasm_payload_tests.rs` (new, `#[cfg(target_arch = "wasm32")]`)

**Approach:**
- Add an async mirror of the shared consumer over `AsyncCursor` (pattern: `parse_charsxp_content_async`).
- Trace each wasm async parse/skip function's dispatch for types 247/248/249 (most fall through to `_ => RObject::Null` today) and add arms that consume the payload and produce the same `RObject` as the native parser (Character for PERSISTSXP, Namespace for PACKAGESXP/NAMESPACESXP), filling ref placeholders where the function tracks refs.
- Tests embed small synthetic streams as byte literals captured from R (document the generating R expression in a comment above each literal).

**Execution note:** Verify the wasm test harness state first (`wasm-pack test` on an existing wasm test) before writing new tests; if it is broken for pre-existing reasons, do not fix the harness in this unit — fall back to `cargo check --target wasm32-unknown-unknown` plus native unit tests of the shared consumer, and report the harness state.

**Test scenarios:**
- Happy path: the PERSISTSXP alignment stream (`list(first = env, second = env, after = "still-aligned")` with a ref hook) parses via the wasm sequential path with the trailing element intact.
- Happy path: the PACKAGESXP alignment stream parses via the wasm sequential path with the trailing element intact.
- Happy path: a bare `getNamespace`-style NAMESPACESXP stream parses as a Namespace without desync.
- Integration (skip path): the sequential *skip* function skips over a persisted/package entry without desyncing the elements after it.

**Verification:**
- `cargo check --target wasm32-unknown-unknown` clean.
- New wasm tests pass under `wasm-pack test` (or the documented fallback applies, reported explicitly).
- Native suite unaffected.

## System-Wide Impact

- **API surface parity:** PACKAGESXP output changes from `Null` (misparsed) to `Namespace(names)` — an observable behavior change, but the old value was produced from a desynced stream, so nothing downstream could have depended on it correctly. Note it in the changelog. The wasm paths gain parity with native for all three pseudo-types.
- **Interaction graph:** All changes are inside `src/parser.rs` dispatch arms plus one shared helper; no writer, types, or public-API signature changes.
- **Error propagation:** The shared consumer returns `InvalidFormat` on malformed counts — same error style both parsers already use; the streaming parser wraps it in `StreamingError::Parse` like the PERSISTSXP arm does.
- **Unchanged invariants:** `should_track_reference`'s tracked set (from PR #3) is untouched. The writer is untouched. `parse_persistsxp_strings` call sites keep identical behavior — only the name generalizes.
- **Integration coverage:** The ref-slot fixture in Unit 1 (package env + symbol back-references) is the cross-layer scenario unit tests alone would miss — it proves slot alignment against R's writer, not just payload consumption.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Fixture fragility: `package:stats` warning or environment availability differs across R versions | `stats` is a base-attached package present in every R install; wrap in `suppressWarnings`; fixture pattern matches how PR #2/#3 fixtures already depend on local R |
| wasm test harness may not currently run (configured `run_in_browser`; never exercised in this session) | Explicit fallback defined in Unit 3's execution note; harness state is verified before new tests are written and reported either way |
| `parse_namespace` unification changes behavior on nonconforming streams | Shared helper is strictly stricter; full suite as net; Unit 2 instructs stopping and surfacing any test change instead of adapting tests |
| PACKAGESXP entries are rare in the wild, so real-world validation is thin | The bug class is identical to PERSISTSXP (already validated on 6,139 real entries); synthetic + R-generated fixtures pin the wire format; R's own reader is the reference |

## Documentation / Operational Notes

- Add a changelog/release-note line: PACKAGESXP entries now parse as `Namespace` values instead of silently corrupting the stream; wasm parsers now consume PERSISTSXP/PACKAGESXP/NAMESPACESXP payloads.
- `RDS_FORMAT.md` mentions pseudo-types; update its PACKAGESXP row/section if it documents current handling.
- Remind: contributors must re-run `Rscript tests/generate_test_data.R` after pulling (fixtures are gitignored).

## Sources & References

- Related PRs: #2 (PERSISTSXP payload fix), #3 (ref-table alignment) — both merged 2026-07-09; their "Notes / known limitations" sections are the origin of this plan's scope.
- Related code: `src/parser.rs` (`parse_persistsxp_strings`, `parse_namespace`, PACKAGESXP arms), `tests/persistsxp_tests.rs`, `tests/wasm_lazy_vector_tests.rs`, `tests/generate_test_data.R`.
- External: R `src/main/serialize.c` (`OutStringVec`/`InStringVec`, `HashAdd`/`AddReadRef`).
