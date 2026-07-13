---
date: 2026-07-10
topic: na-character-representation
---

# NA_character_ Representation

## Problem Frame

rds2rust maps R's `NA_character_` to the literal string `"NA"`, making a
missing value indistinguishable from the legal string `"NA"`. Character is the
only vector type with this defect — Logical has an explicit `Na` variant,
Integer uses R's own `i32::MIN` sentinel, and Real carries the NA_real_ NaN
payload through `f64` — and it is the only type that **corrupts on
roundtrip**: the writer never emits R's NA marker, so R → rds2rust → R turns
missing data into the string `"NA"`. For a data-fidelity library this is
silent data corruption on a common value (any character column containing
NAs). Affected users: anyone reading string data out of parsed objects, anyone
writing RDS files, and wasm/JS consumers of the lazy character APIs.

## Requirements

**Representation**
- R1. `RObject::Character` elements are `Option<Arc<str>>`, with `None`
  representing `NA_character_`, across every parse path (sync, streaming,
  wasm sequential, lazy vector materialization).
- R2. A real `"NA"` string remains distinguishable from a missing value
  end-to-end: parse, in-memory comparison, and write.

**Roundtrip fidelity**
- R3. The writer emits R's NA marker (CHARSXP length −1) for `None`; an RDS
  file with NA strings roundtrips through rds2rust such that R's `readRDS`
  sees values `identical()` to the original.
- R4. wasm/lazy character APIs surface NA equivalently (element type change /
  `null` on the JS boundary), not as a fake string.

**Derived plain-string surfaces**
- R5. Surfaces that derive plain strings from character data treat NA as
  *absent*, never fabricating a string — but skipping never applies where
  position is semantically load-bearing: an NA element in a `names` attribute
  yields an unnamed element in place (tag `None`, positions preserved); an NA
  inside a class vector is skipped (class lookup is order-insensitive);
  **factor levels must preserve their slot** — factor values index into
  levels positionally, so an NA level cannot be dropped without corrupting
  every subsequent level lookup (slot representation is a planning decision).
  Derived types (`class: Vec<Arc<str>>`, attribute keys, symbol names) keep
  their non-optional string types.

**Migration and release**
- R6. Construction stays ergonomic for the common no-NA case: conversion
  impls / helper constructors so building a character vector from plain
  strings is a single call, plus accessor helpers (e.g. `Option<&str>`
  element access and an explicit escape hatch that renders `None` as a
  caller-chosen placeholder string — e.g. `"NA"` — reproducing today's
  output in one visible, greppable call). Final helper set and naming are a
  planning decision; the *existence* of these affordances is the requirement.
- R7. Ships in the (unreleased, already-breaking) 0.2.0, with a CHANGELOG
  migration section showing concrete before/after snippets for element
  access, iteration, construction, and test assertions.
- R8. Debug/display output renders NA distinctly (e.g. `<NA>`, matching R's
  own printing), never as the bare string `NA`.

## Success Criteria

- A file containing `c("NA", NA_character_)` parses into two distinguishable
  values, and roundtrips through rds2rust back to R with `identical()` truth.
- Every element-access site (in-crate and downstream) is forced through the
  compiler to choose an NA policy — no silent behavior carryover.
- The real-world help-database regression check still matches R exactly.
- The full native + wasm suites pass with NA-specific fixtures added,
  explicitly exercising the derived surfaces (NA in a `names` attribute, in a
  class vector, and in factor levels), not just plain character columns.

## Scope Boundaries

- No validity-mask / Arrow-style layer — the value is the single source of
  truth.
- No changes to Integer/Real/Logical NA representations (already faithful).
- No attempt to model R's tri-valued equality (`NA == NA` → `NA`); Rust `==`
  stays structural (`None == None` is `true`).
- Derived string surfaces stay non-optional (per R5); this change does not
  make names, class, keys, or symbols optional types. (Factor levels are the
  one carve-out: R5 requires slot preservation for NA levels, and the slot
  representation — possibly optional level entries — is a planning decision.)

## Key Decisions

- **`Option<Arc<str>>` over a bespoke enum or validity mask**: idiomatic,
  compiler-forced loud migration (the failure mode being fixed is *silent*),
  one source of truth, zero memory overhead via niche optimization. The
  Logical-style enum was a close second (crate consistency) but reinvents
  Option; the validity mask was rejected because it lets downstream code
  silently keep the corrupt behavior and carries permanent dual-truth
  coherence costs.
- **Derived surfaces treat NA as absent**: matches R's own behavior for name
  lookups, never fabricates a string, keeps derived types simple.
- **Fold into 0.2.0**: it is unreleased and already breaking (Shared
  narrowing); one downstream migration event instead of two.
- **Writer fidelity is in scope**: the roundtrip corruption is the core
  motivation, not an optional extra.

## Outstanding Questions

### Deferred to Planning

- [Affects R1][Technical] Exact plumbing for the lazy vector machinery
  (`VectorData<T>` becoming `VectorData<Option<Arc<str>>>` for character) and
  the wasm lazy-range decode path — including lock coherence if
  materialization can occur under the Shared/RwLock object graph. Note:
  `materialization.rs` currently returns `Unsupported` for Character
  materialization; planning decides whether 0.2.0 implements it or keeps
  `Unsupported` with the new element type.
- [Affects R1][Needs research] The parser also maps NILSXP in CHARSXP
  position to `"NA"` today (a fifth NA-producing pattern beyond length −1);
  confirm R's semantics and map it to `None` consistently.
- [Affects R2][Technical] Internal coherence surfaces must distinguish `None`
  from `Some("NA")`: the dedup fingerprint and `PartialEq` (mechanical with
  `Option`, but verify), plus any `From`/`Into` conversions or iterator
  adapters that could silently bypass the `Option` (inventory during the
  site enumeration).
- [Affects R5][Technical] Slot representation for an NA factor level
  (positions must be preserved per R5 — e.g. optional level entries or
  explicit NA-level handling in `FactorData`).
- [Affects R4][Technical] JS boundary mapping details (`null` vs `undefined`,
  and whether existing JS-facing conversions need coordinated updates).
- [Affects R6][Technical] Final helper-method set and naming on
  `CharacterVector`.
- [Affects R5][Technical] Site enumeration for derived surfaces (tag/name
  extraction, class conversion, factor level handling) — the behavior is
  decided; the inventory belongs to planning.
- [Affects R3][Resolved 2026-07-10] R's NA wire format verified empirically
  (`serialize(c("NA", NA_character_))` hexdump): NA_character_ is a bare
  CHARSXP flags word `0x00000009` (no encoding-level bits) followed by
  int32 −1, nothing else. Regular strings carry level bits in the flags
  (e.g. `0x00040009` for ASCII), so the writer must emit bare flags for NA.

## Next Steps

→ `/ce:plan` for structured implementation planning
