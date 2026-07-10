---
title: Add wasm test exercising skip_object_sequential_value_async payload arms
priority: p2
status: ready
source: ce-review 2026-07-10 (testing reviewer, confidence 0.82)
---

`skip_object_sequential_value_async` received PERSISTSXP/PACKAGESXP/NAMESPACESXP/
BASENAMESPACE_SXP payload-consumption arms identical to
`parse_object_sequential_value_async`, but no wasm test drives the skip path —
the four tests in `tests/wasm_payload_tests.rs` only exercise the parse path.
A future refactor of the skip function could break payload consumption without
any test failing.

**Fix:** add a wasm test whose stream routes a pseudo-type entry through the
skip path (e.g. an S4 slot or attribute shape parsed in LazyMetadata mode that
the sequential parser skips), asserting elements after the entry stay aligned.
Run via:

    cargo build --target wasm32-unknown-unknown --test wasm_payload_tests
    WASM_BINDGEN_TEST_ONLY_NODE=1 wasm-bindgen-test-runner \
        target/wasm32-unknown-unknown/debug/deps/wasm_payload_tests-*.wasm
