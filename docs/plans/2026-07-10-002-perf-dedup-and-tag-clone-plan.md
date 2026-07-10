---
title: "perf: hash-indexed dedup cache and clone-free TAG REFSXP resolution"
type: refactor
status: completed
date: 2026-07-10
---

# perf: Hash-indexed dedup cache and clone-free TAG REFSXP resolution

## Overview

The deferred performance bundle from the PR #3 review:

1. **`DedupTable` linear scan** — `deduplicate()` runs at the tail of every
   sync `parse_object` call and compares the object against *every* cache
   entry. Files with many distinct small cacheable objects (e.g. thousands of
   unique length-1 character vectors) degrade to O(n²) comparisons.
2. **TAG REFSXP deep clone** — resolving a REFSXP in TAG position deep-clones
   the referenced object (`obj.read().unwrap().clone()`), then
   `extract_tag_name(tag_obj.clone())` clones it *again*. Cheap for symbols
   (the common case) but wasteful for large shared referents (environments),
   and the second clone is pure waste for every tag.

## Key Technical Decisions

- **Dedup early-out for non-cacheable objects.** A non-cacheable object can
  never equal a cached entry: `==` requires the same variant, and for the
  length-gated vector variants equality implies equal length, hence equal
  cacheability; for variant-constant cacheability (List/DataFrame/S3/S4/Env/
  Closure are never cached) the variant is simply absent from the cache. So
  `deduplicate` can return immediately without scanning. This alone removes
  the scan for all large/composite objects.
- **Fingerprint index, not full structural hashing.** `HashMap<u64, Vec<u32>>`
  mapping fingerprint → cache indices, with full `==` only on bucket
  candidates. The fingerprint hashes the discriminant plus a *bounded,
  equality-stable projection* per variant (full contents for the small
  cacheable vectors, name for symbols, length-bounded prefix for factors,
  discriminant-only for composite catch-all variants like WithAttributes /
  Pairlist / Language). Any deterministic projection of equality-implied
  fields keeps the required invariant `a == b ⇒ fp(a) == fp(b)`;
  discriminant-only buckets degrade gracefully to a per-variant scan.
- **wasm rollback contract preserved.** `checkpoint()` stays `(len, hits,
  misses)`; `rollback()` truncates the cache and prunes index entries ≥ len.
- **TAG REFSXP: Shared-wrap instead of deep clone, with two concrete
  exceptions.** Symbols stay concrete (cheap clone, overwhelmingly common)
  and S4 objects stay concrete because `parse_attributes` pattern-matches
  `RObject::S4Object` on `tag_object` directly (the `__tag_s4_object__`
  special case). Everything else becomes `RObject::Shared(arc)` — an Arc
  clone.
- **`extract_tag_name` takes `&RObject`.** Kills the unconditional
  `tag_obj.clone()` at every call site (17 of them) and reads through Shared
  without materializing the referent.

## Scope Boundaries

- No change to what gets cached (`should_cache_for_dedup` untouched, including
  its pre-existing permissive `_ => true` arm).
- No public API changes; `PairlistElement.tag_object` may now hold
  `RObject::Shared` for non-symbol/non-S4 REFSXP tags (same object identity,
  different wrapper — consumers in types.rs handle generic RObject values).
- Measured with release-build benchmarks bracketing the change; synthetic
  dedup-heavy fixtures live in the scratch area, not the repo.

## Implementation Units

- [x] **Unit 0: Baseline benchmarks** *(release build, best-of-3: distinct 1376.9 ms, repeat 7.3 ms, acf 5.2 ms)*

**Approach:** Temporary release-build harness timing `read_rds` on: (a) 20k
distinct length-1 character vectors (worst case for the linear scan), (b) 20k
elements from a 50-value pool (hit-heavy case), (c) the real `stats::acf`
help-DB entry (regression guard). Record numbers before any change.

**Test scenarios:** Test expectation: none — measurement only.

- [x] **Unit 1: Dedup early-out + fingerprint index** *(distinct: 1376.9 -> 7.5 ms)*

**Files:** `src/parser.rs` (DedupTable, new `dedup_fingerprint`)

**Test scenarios:**
- Happy path: full native suite unchanged (dedup behavior is
  correctness-neutral; hits return equal-content clones exactly as before).
- Integration: wasm tests unchanged; wasm32 compile clean (rollback pruning).
- Perf: case (a) improves by an order of magnitude or more; (b) and (c) do
  not regress beyond noise.

- [x] **Unit 2: Clone-free TAG REFSXP resolution** *(final: distinct 7.3 ms, repeat 5.2 ms, acf 4.1 ms; real-world attrs still match R)*

**Files:** `src/parser.rs` (`extract_tag_name` signature + 17 call sites;
REFSXP tag branches in `parse_pairlist_element`, the streaming tag sites, and
the wasm sequential tag sites)

**Test scenarios:**
- Happy path: full native suite — specifically the S4, attribute, and
  refsxp-alignment tests (tag-heavy paths).
- Integration: help-DB real-world check still matches R (attributes are
  tag-driven).
- Edge: S4-in-TAG special case preserved (S4 referents stay concrete).

## Verification

- Full native suite + wasm tests pass; fmt/clippy/wasm32-check clean.
- Before/after benchmark numbers recorded in the commit message and CHANGELOG.
- Real-world `stats::acf` entry parses identically to R.
