# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

Covers all changes since 0.1.41: PR [#2](https://github.com/andrewwbutler/rds2rust/pull/2)
(PERSISTSXP payloads), PR [#3](https://github.com/andrewwbutler/rds2rust/pull/3)
(reference-table alignment), and the PACKAGESXP/namespace/wasm payload fixes.
The version in `Cargo.toml` has not been bumped yet.

### Breaking / behavior changes for downstream consumers

- **`RObject::Shared` wrapping is much narrower.** The parser now assigns
  reference-table slots to exactly the set R's serializer does (symbols,
  environments, external pointers, weak references, and persisted /
  package / namespace entries). Atomic vectors, lists, pairlists, closures,
  and bytecode are **no longer wrapped in `RObject::Shared`** — they come
  back as concrete variants. Code that pattern-matches on `Shared` for those
  types must now handle the concrete variant (or call `as_concrete()` /
  `into_concrete()`, which handle both).
- **PERSISTSXP entries** (written by `serialize(refhook=)`, e.g. R lazy-load
  help databases) now parse as `RObject::Character` holding the ref-hook
  strings (e.g. `"env::1"`). Previously they returned `Null` *and* silently
  corrupted everything after them in the stream.
- **PACKAGESXP entries** (serialized package environments) now parse as
  `RObject::Namespace` holding the environment name (e.g. `"package:stats"`)
  and occupy one reference slot, matching R's reader. Previously `Null` plus
  stream corruption.
- **BASENAMESPACE_SXP entries** now parse as `RObject::Namespace(["base"])`.
  Previously the parser consumed a phantom payload here, swallowing the next
  object in the stream.
- **`RObject::Namespace` is now dual-purpose**: it represents both namespaces
  (`NAMESPACESXP`) and package environments (`PACKAGESXP`). Package
  environments are distinguishable by the `"package:"` prefix in the name.
- **Parsed structures may differ from 0.1.41 wherever the old output was
  silently wrong.** Any file sharing symbols or environments (e.g. `srcref`
  structures from `keep.source = TRUE`, R help databases) previously came
  back with shifted attribute names (e.g. `class` mislabeled or dropped),
  wrong back-reference targets, or spurious cycles. Consumers that had
  worked around such artifacts should re-test against 0.2.0.
- **Writer wire format is now R-conforming.** REFSXP indices are emitted from
  the single object reference table in all contexts, including TAG positions.
  Files written by ≤ 0.1.41 that contained repeated symbols alongside
  environments were non-conforming (R itself could misread them); the 0.2.0
  parser retains a defensive symbol-table fallback so it can still read them,
  but rewriting such files with 0.2.0 is recommended.
- **Error message string change**: `"Negative PERSISTSXP string count"` is now
  `"Negative string vector count"` (the check moved into the shared
  `OutStringVec` consumer). Error messages are not part of the stable API,
  but exact-string matchers will notice.
- **wasm sequential parser** now returns the same `Character` / `Namespace`
  values as the native parser for the pseudo-types above, instead of `Null`.

### Fixed

- Stream desynchronization on PERSISTSXP entries: the ref-hook string payload
  is now consumed, keeping every subsequent object aligned (#2).
- REFSXP reference-table misalignment: back references now resolve to the
  correct objects; previously e.g. `serialize(list(a = as.name("foo"),
  b = as.name("foo")), con)` resolved `b` to the enclosing list, creating a
  spurious cycle (#3).
- Writer leaked one reference index per plain (non-`Shared`) `Symbol`
  occurrence, shifting all later indices for conforming readers including
  R's `readRDS` (#3).
- Streaming environment parser desynced after the first environment (missing
  `locked` field and unconditional trailing attribute item) (#3).
- `NAMESPACESXP_SERIAL` entries now fill their reference-table placeholder so
  later back references resolve to the namespace (#3).
- Stream desynchronization on PACKAGESXP entries (payload never consumed) and
  on BASENAMESPACE_SXP entries (phantom payload consumed).
- `parse_namespace` now uses the shared `OutStringVec` consumer, gaining
  long-vector-marker handling and negative-count rejection.
- The wasm sequential parse and skip paths now consume PERSISTSXP /
  PACKAGESXP / NAMESPACESXP payloads instead of desyncing.

### Changed

- Corrupt streams that previously misparsed silently now fail fast with
  `InvalidFormat` errors: a non-zero names placeholder in an `OutStringVec`
  payload (R errors here too), a non-CHARSXP item type in such a payload, and
  BCREPREF/BCREPDEF bytecode-representation markers encountered outside a
  bytecode payload (previously resolved against the main reference table,
  returning an arbitrary wrong object). Files written by R are unaffected.

### Performance

- `RObject::PartialEq` and the dedup cache no longer deep-clone both operands
  on every comparison; a pathological 147 KB help-database entry went from
  ~9.3 s (post-alignment, pre-fix) to ~35 ms (#3).
- The dedup cache's linear scan was replaced with a fingerprint index plus an
  early-out for never-cached object types. A file with 20,000 distinct small
  character vectors went from ~1.38 s to ~7 ms (release build); typical files
  improve modestly (a real help-DB entry: 5.2 → 4.1 ms).
- Resolving a REFSXP in TAG position no longer deep-clones the referenced
  object (twice); symbols and S4 objects stay concrete, other referents are
  wrapped as shared handles, and tag names are extracted without
  materializing the referent.

### Validation

- All 6,139 Rd help entries across 117 installed packages of an R 4.5.2
  library parse with structure and attribute names matching
  `tools:::fetchRdDB()` exactly; across 1,085,489 srcref attributes every
  srcfile reference resolves to the correct persisted entry (#3, verified
  again with the PACKAGESXP fixes on R 4.3.3).

## [0.1.41] - 2026-02-23

### Added

- xz (lzma) compression support in the reader
  ([#1](https://github.com/andrewwbutler/rds2rust/pull/1)), without wasm
  regressions.

### Fixed

- Restored a missing test fixture; fixed clipping warnings.

Older history was not recorded in a changelog.
