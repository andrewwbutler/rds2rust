---
title: Extract shared helper for duplicated wasm payload arms
priority: p2
status: ready
source: ce-review 2026-07-10 (maintainability reviewer, confidence 0.90; downgraded P1->P2 in synthesis)
---

The PERSISTSXP/PACKAGESXP/NAMESPACESXP/BASENAMESPACE_SXP payload arms are
byte-identical in `parse_object_sequential_value_async` and
`skip_object_sequential_value_async` (src/parser.rs). Future changes to payload
consumption must be replicated in both or the paths drift — the exact failure
class this branch fixed.

Downgraded from P1 because the two functions already duplicate their entire
match wholesale (pre-existing file convention) and the wasm tests bound the
drift risk for the parse path.

**Fix:** extract a shared async helper, e.g.
`try_parse_pseudo_stringvec_async(sexp_type, ctx, cursor) -> Result<Option<RObject>>`,
called from both matches. Re-verify with the wasm test procedure documented in
docs/todos/2026-07-10-wasm-skip-path-test.md. Consider pairing with that todo
so the skip-path test lands first and guards the refactor.
