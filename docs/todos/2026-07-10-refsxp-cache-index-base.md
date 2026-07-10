---
title: Align REFSXP in-vector cache index base across defensive readers
priority: p2
status: ready
source: ce-review 2026-07-10 (correctness, confidence 0.92, PRE-EXISTING)
---

Three defensive character-vector readers use the 1-based wire REFSXP index
directly against a 0-based cache (`cache.get(ref_index)`): src/extraction.rs
(two column-cache export loops) and src/wasm/extract.rs `parse_character_vec`.
The native parser and chunk_iter correctly use `ref_index - 1` with
`ref_index == 0` rejected. R never emits in-vector CHARSXP REFSXPs (CHARSXPs
are not ref-tracked by R's serializer), so conforming files are unaffected —
but the paths disagree with each other and would misread any stream that does
use them.

**Fix:** use `ref_index.saturating_sub(1)` with a zero-index rejection in all
three sites, matching chunk_iter.rs; add a synthetic-stream test.
