# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-07-13

Covers all changes since 0.1.41: PR [#2](https://github.com/andrewwbutler/rds2rust/pull/2)
(PERSISTSXP payloads), PR [#3](https://github.com/andrewwbutler/rds2rust/pull/3)
(reference-table alignment), the PACKAGESXP/namespace/wasm payload fixes, and
the streaming bytecode decoder.

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
- **PACKAGESXP entries** (serialized package environments) now parse as a new
  `RObject::PackageEnv` variant holding the environment name
  (e.g. `"package:stats"`) and occupy one reference slot, matching R's reader.
  Previously `Null` plus stream corruption. The writer emits PACKAGESXP for
  this variant, so package environments now round-trip with their type intact
  (previously they would have been rewritten as namespaces).
- **BASENAMESPACE_SXP entries** now parse as `RObject::Namespace(["base"])`.
  Previously the parser consumed a phantom payload here, swallowing the next
  object in the stream.
- **New `RObject::PackageEnv(Vec<Arc<str>>)` variant** distinguishes package
  environments from namespaces at the type level (both share the same
  OutStringVec wire shape but different SEXP types). Code that matches
  `RObject` exhaustively must add a `PackageEnv` arm.
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
- **`NA_character_` is now a real missing value.** `RObject::Character`
  elements changed from `Arc<str>` to `Option<Arc<str>>` (`None` is NA).
  Previously NA parsed as the literal string `"NA"` — indistinguishable from
  a real `"NA"` — and the writer silently turned missing data into the string
  `"NA"` on roundtrip. This is the largest migration item in 0.2.0; every
  element access chooses an NA policy at compile time:

  ```rust
  // element access
  let s: &Arc<str> = &chars.as_vec()[0];              // before
  let s: Option<&str> = chars.as_vec()[0].as_deref(); // after
  let s = chars.as_vec()[0].as_deref().unwrap_or("NA"); // old behavior, explicit

  // iteration
  for s in vec.as_vec() { use_str(s); }                    // before
  for s in vec.iter_strs().flatten() { use_str(s); }       // after (skip NAs)

  // construction
  RObject::Character(vec![Arc::from("x")].into())          // before
  RObject::Character(VectorData::from_strs(["x"]))         // after (no NAs)
  RObject::Character(vec![Some(Arc::from("x")), None].into()) // with NA

  // test assertions
  assert_eq!(v[0].as_ref(), "x");            // before
  assert_eq!(v[0].as_deref(), Some("x"));    // after
  ```

  Helpers on character vectors: `from_strs`, `get_str(i) -> Option<&str>`,
  `is_na(i)`, `iter_strs()`, and `to_strings_with_na(placeholder)` — the
  explicit escape hatch reproducing the old rendering in one greppable call.
- **Derived plain-string surfaces treat NA as absent**, except where position
  is load-bearing: an NA in a `names` attribute yields an unnamed element in
  place; NA class entries are skipped; `FactorData.levels` and
  `DataFrameData.row_names` became `Vec<Option<Arc<str>>>` to preserve slots
  (factor values index into levels positionally). NA in a symbol-name
  position is now a parse error (R has no NA symbols).
- **The writer emits R's real NA marker** (bare CHARSXP flags + length −1,
  byte-identical to R's own output), so writing NA strings is possible for
  the first time; roundtrips are `identical()` under R's `readRDS`.
- **wasm/JS boundary**: NA strings surface as `null` instead of the string
  `"NA"` in all character extraction and chunked-streaming APIs.

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
- Streaming metadata inspection (`inspect_metadata_streaming`) now decodes the
  bytecode (BCODESXP) wire format correctly instead of walking it as three
  generic objects. The previous traversal relied on a lenient continue-arm and
  byte-skipping; it leaked bytecode internals (the opcode integer vector) into
  the reported vectors and could miss real siblings. Bytecode is still reported
  as an unsupported structure (unchanged warning contract), but the payload is
  now consumed exactly, so objects after a bytecode payload stay aligned and a
  standalone bytecode rep marker (`BCREPREF`/`BCREPDEF`) is now rejected in all
  contexts.

### Changed

- Corrupt streams that previously misparsed silently now fail fast with
  `InvalidFormat` errors: a non-zero names placeholder in an `OutStringVec`
  payload (R errors here too), a non-CHARSXP item type in such a payload, and
  BCREPREF/BCREPDEF bytecode-representation markers encountered outside a
  bytecode payload (previously resolved against the main reference table,
  returning an arbitrary wrong object). Files written by R are unaffected.
- GENERICREFSXP/CLASSREFSXP entries (types 245/246) now fail fast with
  `InvalidFormat` in the sync and streaming parsers. R's own reader errors
  on these unconditionally, so no readable stream contains them; previously
  the parser resolved their index against the main reference table,
  returning an arbitrary wrong object.
- Additional fail-fast hardening for hostile (non-R) streams: an `OutStringVec`
  CHARSXP item carrying the `HAS_ATTR` bit is rejected (R never writes it, and
  the item reader does not consume the promised attribute — so it would
  otherwise desync), and the string-vec/character allocation paths cap the
  eager `Vec::with_capacity` reservation so a hostile element count cannot
  drive a multi-gigabyte allocation before any item is read (the async/wasm
  path had no remaining-stream backstop).
- Invalid in-vector string references (REFSXP inside STRSXP with an
  out-of-range or zero index) now fail fast in every reader: the wasm
  sequential path previously degraded them silently, and three defensive
  extraction readers resolved the 1-based wire index against the cache
  without the off-by-one adjustment. All readers now share the native
  parser's 1-based semantics. R never emits in-vector string references,
  so files written by R are unaffected.

### Developer notes

- `wasm-pack test --node` now works for the whole workspace: native-only test
  files are cfg-gated off wasm32, the wasm test suites run under node instead
  of requiring a browser (globals are looked up on `globalThis`), and two
  previously-never-executed wasm test suites (streaming decompression, lazy
  vectors) now run and pass. Requires Node >= 18 for `CompressionStream`.

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

### Added

- Lazy character vectors can now be materialized (native): previously
  `materialize`/`materialize_path` returned `Unsupported` for character
  columns while every numeric type worked. New
  `materialize_character_vector` / `materialize_character_data` complete the
  per-type API family. Implemented by reusing the hardened
  `read_lazy_character_range` decoder, so NA elements and intra-vector string
  references resolve identically to the eager parser.

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
