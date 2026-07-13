---
title: "fix: defensive hardening for string-vec payloads and out-of-context bytecode reps"
type: fix
status: completed
date: 2026-07-10
---

# fix: Defensive hardening for string-vec payloads and out-of-context bytecode reps

## Overview

The deferred "defensive hardening" bundle from the PR #2/#3 reviews: make corrupt
or nonconforming streams fail fast with `InvalidFormat` errors instead of
misparsing silently. Three items, all verified against R's `serialize.c`:

1. **Names placeholder validation** in `parse_string_vec` / `parse_string_vec_async`:
   R's `InStringVec` errors on a non-zero placeholder; we currently ignore it.
2. **CHARSXP item type check** in the same functions: each item's flags word must
   carry the CHARSXP type (low byte, or bits 8–15 for the compact variant this
   codebase supports); currently any type is fed to `parse_charsxp_content`,
   turning corrupt streams into garbage strings.
3. **BCREPREF/BCREPDEF outside bytecode context**: R's `ReadItem` has no case for
   243/244 (they only occur inside bytecode payloads, which `parse_bc_lang`
   handles with a proper reps table). The `parse_object` arm at parser.rs:4557
   treats `flags >> 8` as a main-ref-table index — wrong wire format AND wrong
   table — and the streaming parser silently `Continue`s on 244 (line 5267)
   while erroring on 243. Both should error like R does.

## Scope Boundaries

- The wasm sequential `_ => RObject::Null` fallback for 243/244 stays as-is
  (generic unknown-type handling; no ref-table misuse there).
- The `GENERICREFSXP | CLASSREFSXP` arm (same ref-table-misuse shape, R also
  errors on these) is left unchanged — not in the deferred bundle; note for later.
- No changes to conforming-stream behavior: all existing fixtures must parse
  identically.

## Implementation Units

- [x] **Unit 1: String-vec payload validation (sync + wasm async)**

**Files:** `src/parser.rs`, `tests/packagesxp_tests.rs`, `tests/wasm_payload_tests.rs`

**Approach:** In `parse_string_vec` (parser.rs:8963) and `parse_string_vec_async`
(parser.rs:~4110): error on non-zero names placeholder (mirror R's message);
after reading each item's flags word, resolve the type like the parsers do
(low byte, falling back to bits 8–15 when the low byte is 0) and reject
non-CHARSXP with `InvalidFormat`.

**Test scenarios:**
- Error path (native): hand-built PACKAGESXP stream with placeholder = 1 → `Err`.
- Error path (native): hand-built PACKAGESXP stream with an INTSXP (13) item
  flags word → `Err`.
- Error path (wasm): copy `PACKAGESXP_STREAM`, set the placeholder to non-zero,
  expect parse failure through the async path (pins the mirror).
- Happy path: all existing fixture tests (persistsxp, packagesxp, namespace)
  unchanged — regression net for both the standard and compact flag forms.

- [x] **Unit 2: Error on out-of-context BCREPREF/BCREPDEF** *(execution finding: the streaming parser's old `244 => Continue` was load-bearing — `parse_bytecode_streaming` routes bytecode internals through the generic dispatch, which cannot decode the bytecode wire format, and the metadata traversal relies on lenient stumbling to emit its "Bytecode unsupported" warning. Resolution: rep markers are tolerated only inside bytecode context — `parse_bytecode_streaming` now sets `ctx.in_bytecode_context` — and error outside it; the sync `parse_object` arm errors unconditionally, validated by the full suite.)*

**Files:** `src/parser.rs`, `tests/packagesxp_tests.rs`

**Approach:** Replace the `parse_object` arm with an `InvalidFormat` error
(mentioning bytecode context); remove `| 244` from the streaming Continue arm
so 243/244 both hit the existing `UnknownSexpType` error. Before/while doing
this, confirm no existing fixture routes through either arm (the full suite is
the detector — any hit turns a silent misparse into a loud failure).

**Test scenarios:**
- Error path: hand-built streams with top-level type 243 and 244 → `Err`, and
  specifically not a `Shared` object (the old misuse).
- Happy path: full native suite (includes bytecode-containing fixtures, which
  exercise the legitimate `parse_bc_lang` reps path).

## Verification

- New tests fail on the pre-change parser, pass after.
- Full native suite + all 5 wasm tests pass; fmt/clippy clean; wasm32 compile clean.
- CHANGELOG 0.2.0 section gains a note: corrupt string-vec payloads and
  out-of-context bytecode reps now error instead of silently misparsing.
