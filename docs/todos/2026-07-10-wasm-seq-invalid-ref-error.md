---
title: wasm sequential STRSXP loop should error on invalid in-vector ref index
priority: p2
status: ready
source: ce-review 2026-07-10 (adversarial, confidence 0.85)
---

The wasm sequential STRSXP element loop degrades an out-of-range intra-vector
REFSXP index to `None` (`string_cache.get(i-1).cloned().unwrap_or(None)`),
while `chunk_iter.rs` and the native `parse_character_vector_full` return
`InvalidFormat` on the same condition. Pre-change behavior fabricated the
string "NA" (equally silent), so this is not a regression, but erroring is
the consistent, fail-fast choice for corrupt streams.

**Fix:** replace the `unwrap_or(None)` fallbacks (parse + skip twins) with an
`InvalidFormat` error matching the native message; add a native + wasm test
with a hand-built stream containing an out-of-range in-vector ref.
