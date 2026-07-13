//! In-vector string reference (REFSXP inside STRSXP) handling.
//!
//! Wire indices are 1-based (0 is invalid); every reader must resolve them
//! against the per-vector string cache identically, and invalid indices must
//! fail fast rather than silently degrading. R's own serializer never emits
//! these (CHARSXPs are not ref-tracked), so streams are hand-built.

// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{read_rds, RObject, VectorData};
use std::sync::Arc;

/// Hand-built version-2 XDR stream builder.
struct StreamBuilder(Vec<u8>);

impl StreamBuilder {
    fn new() -> Self {
        let mut data = Vec::new();
        data.extend_from_slice(b"X\n"); // XDR format marker
        data.extend_from_slice(&2i32.to_be_bytes()); // serialization version 2
        data.extend_from_slice(&0x040303i32.to_be_bytes()); // writer R version
        data.extend_from_slice(&0x020300i32.to_be_bytes()); // min reader R version
        Self(data)
    }

    fn word(mut self, w: i32) -> Self {
        self.0.extend_from_slice(&w.to_be_bytes());
        self
    }

    fn charsxp(mut self, s: &str) -> Self {
        self.0.extend_from_slice(&9i32.to_be_bytes()); // bare CHARSXP flags
        self.0.extend_from_slice(&(s.len() as i32).to_be_bytes());
        self.0.extend_from_slice(s.as_bytes());
        self
    }

    fn refsxp(self, index: u32) -> Self {
        self.word(((index as i32) << 8) | 0xFF)
    }

    fn build(self) -> Vec<u8> {
        self.0
    }
}

fn strsxp(len: u32) -> StreamBuilder {
    StreamBuilder::new().word(0x10).word(len as i32)
}

/// Ground truth: index 1 refers to the FIRST cached string (1-based).
#[test]
fn test_invector_ref_resolves_one_based() {
    // STRSXP ["a", "b", ref@1] -> ["a", "b", "a"]
    let data = strsxp(3).charsxp("a").charsxp("b").refsxp(1).build();
    let obj = read_rds(&data).expect("failed to parse").object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.get_str(0), Some("a"));
            assert_eq!(vec.get_str(1), Some("b"));
            assert_eq!(
                vec.get_str(2),
                Some("a"),
                "ref@1 must resolve to the first string"
            );
        }
        other => panic!("expected character vector, got {:?}", other),
    }
}

/// An out-of-range in-vector reference is a format error, not silent data.
#[test]
fn test_invector_ref_invalid_index_rejected() {
    // STRSXP ["a", ref@5] with only one cached string
    let data = strsxp(2).charsxp("a").refsxp(5).build();
    let err = format!(
        "{:?}",
        read_rds(&data).expect_err("out-of-range in-vector ref must be rejected")
    );
    assert!(
        err.contains("string reference") || err.contains("REFSXP"),
        "unexpected error: {}",
        err
    );

    // Index 0 is invalid on the wire.
    let data = strsxp(2).charsxp("a").refsxp(0).build();
    assert!(
        read_rds(&data).is_err(),
        "in-vector ref index 0 must be rejected"
    );
}

/// In-memory input source for lazy range reads.
struct BytesInput {
    data: Vec<u8>,
}

impl rds2rust::RdsInput for BytesInput {
    fn read_at(&self, offset: u64, len: usize) -> rds2rust::Result<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len).min(self.data.len());
        Ok(self.data[start..end.max(start)].to_vec())
    }

    fn len(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }
}

/// The lazy-range reader resolves in-vector references with the same 1-based
/// semantics as the eager parser, including across an NA element.
#[test]
fn test_lazy_range_invector_refs_one_based() {
    // 12 elements (> default lazy threshold 10):
    // s0..s4, NA, then refs to entries 1..=6 (the sixth cached entry is NA).
    let mut b = strsxp(12);
    for i in 0..5 {
        b = b.charsxp(&format!("s{}", i));
    }
    b = b.word(9).word(-1); // NA element (bare CHARSXP flags + length -1)
    for i in 1..=6 {
        b = b.refsxp(i);
    }
    let data = b.build();

    // Eager parse is the ground truth.
    let eager = read_rds(&data).expect("eager parse failed").object;
    let expected: Vec<Option<Arc<str>>> = match eager {
        RObject::Character(v) => v.into_vec(),
        other => panic!("expected character vector, got {:?}", other),
    };
    assert_eq!(expected.len(), 12);
    assert_eq!(expected[5], None);
    assert_eq!(expected[6].as_deref(), Some("s0"), "ref@1 -> first string");
    assert_eq!(expected[11], None, "ref@6 -> the cached NA");

    // Lazy parse yields a span; range-read must agree with the eager parse.
    let lazy = rds2rust::read_rds_lazy(&data)
        .expect("lazy parse failed")
        .object;
    let span = match lazy {
        RObject::Character(VectorData::Lazy(span)) => span,
        other => panic!("expected lazy character vector, got {:?}", other),
    };
    let input = BytesInput { data };
    let chunk =
        rds2rust::read_lazy_character_range(&input, span, 0, 12).expect("lazy range read failed");
    assert_eq!(chunk, expected, "lazy range read must match eager parse");
}
