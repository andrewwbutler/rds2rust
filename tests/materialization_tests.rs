// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{materialize_path, MaterializationContext, RObject, VectorData};
use rds2rust::{Error, LazyVector};

use flate2::read::GzDecoder;
use std::io::Read;
use std::sync::Arc;

/// write_rds gzips by default; lazy spans reference offsets in the
/// uncompressed stream, so tests operate on the gunzipped bytes.
fn gunzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .expect("gunzip failed");
    out
}

/// Serialize `obj`, return the decompressed stream plus the top-level lazy
/// character span parsed from it.
fn lazy_char_stream(obj: &RObject) -> (Vec<u8>, LazyVector) {
    let compressed = rds2rust::write_rds(obj).expect("write failed");
    let raw = gunzip(&compressed);
    let lazy = rds2rust::read_rds_lazy(&compressed)
        .expect("lazy parse failed")
        .object;
    let span = match lazy {
        RObject::Character(VectorData::Lazy(span)) => span,
        other => panic!("expected lazy character vector, got {:?}", other),
    };
    (raw, span)
}

/// Ground truth: the eager-parsed elements for the same object.
fn eager_char_elems(obj: &RObject) -> Vec<Option<Arc<str>>> {
    let eager = rds2rust::read_rds(&rds2rust::write_rds(obj).expect("write failed"))
        .expect("eager parse failed")
        .object;
    match eager {
        RObject::Character(VectorData::Owned(v)) => v,
        other => panic!("expected owned character vector, got {:?}", other),
    }
}

/// A character vector long enough to stay lazy under the default threshold.
fn big_char_vec(with_na_at: Option<usize>) -> RObject {
    let mut v: Vec<Option<Arc<str>>> = (0..50)
        .map(|i| Some(Arc::from(format!("value_{}", i).as_str())))
        .collect();
    if let Some(i) = with_na_at {
        v[i] = None;
    }
    RObject::Character(v.into())
}

#[test]
fn materialize_path_root_integer() {
    let data = [0, 0, 0, 5];
    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: 4,
    };
    let mut obj = RObject::Integer(VectorData::Lazy(span));
    let mut ctx = MaterializationContext::with_budget(&data, 4);

    let changed = materialize_path(&mut obj, "", &mut ctx).unwrap();
    assert!(changed);

    match obj {
        RObject::Integer(VectorData::Owned(values)) => {
            assert_eq!(values, vec![5]);
        }
        _ => panic!("expected owned integer vector"),
    }
}

#[test]
fn materialize_path_budget_exhausted() {
    let data = [0, 0, 0, 5];
    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: 4,
    };
    let mut obj = RObject::Integer(VectorData::Lazy(span));
    let mut ctx = MaterializationContext::with_budget(&data, 2);

    let err = materialize_path(&mut obj, "", &mut ctx).unwrap_err();
    assert!(matches!(err, Error::MemoryBudgetExceeded { .. }));
}

// ---- Character materialization -------------------------------------------

#[test]
fn materialize_character_matches_eager() {
    let obj = big_char_vec(None);
    let expected = eager_char_elems(&obj);
    let (raw, span) = lazy_char_stream(&obj);

    let mut ctx = MaterializationContext::new(&raw);
    let values = ctx.materialize_character_vector(span).unwrap();
    assert_eq!(values, expected);
    assert_eq!(values[0].as_deref(), Some("value_0"));
    assert_eq!(values.len(), 50);
}

#[test]
fn materialize_character_preserves_na() {
    // NA at index 7; also include a real "NA" string to prove distinctness.
    let mut v: Vec<Option<Arc<str>>> = (0..50)
        .map(|i| Some(Arc::from(format!("v{}", i).as_str())))
        .collect();
    v[7] = None;
    v[8] = Some(Arc::from("NA"));
    let obj = RObject::Character(v.into());

    let expected = eager_char_elems(&obj);
    let (raw, span) = lazy_char_stream(&obj);

    let values = materialize_char(&raw, span);
    assert_eq!(values, expected);
    assert_eq!(values[7], None, "NA_character_ stays None");
    assert_eq!(
        values[8].as_deref(),
        Some("NA"),
        "the string \"NA\" is not NA"
    );
}

#[test]
fn materialize_character_resolves_intra_vector_refs() {
    // Repeated strings let the writer dedup via in-vector REFSXP; the
    // materialized output must resolve them to the same string as the
    // eager parse.
    let mut v: Vec<Option<Arc<str>>> = Vec::new();
    for i in 0..20 {
        v.push(Some(Arc::from(format!("uniq_{}", i).as_str())));
    }
    for _ in 0..20 {
        v.push(Some(Arc::from("repeated"))); // 20 copies -> dedup candidates
    }
    let obj = RObject::Character(v.into());

    let expected = eager_char_elems(&obj);
    let (raw, span) = lazy_char_stream(&obj);

    let values = materialize_char(&raw, span);
    assert_eq!(values, expected);
    assert_eq!(values[39].as_deref(), Some("repeated"));
}

#[test]
fn materialize_character_data_in_place() {
    let obj = big_char_vec(Some(3));
    let expected = eager_char_elems(&obj);
    let (raw, span) = lazy_char_stream(&obj);

    let mut vector: VectorData<Option<Arc<str>>> = VectorData::Lazy(span);
    let mut ctx = MaterializationContext::new(&raw);
    ctx.materialize_character_data(&mut vector).unwrap();

    match vector {
        VectorData::Owned(values) => assert_eq!(values, expected),
        VectorData::Lazy(_) => panic!("still lazy after materialization"),
    }
}

#[test]
fn materialize_character_via_dispatch() {
    // materialize_vector (through materialize_path root) now handles Character.
    let obj = big_char_vec(Some(10));
    let expected = eager_char_elems(&obj);
    let (raw, span) = lazy_char_stream(&obj);

    let mut lazy_obj = RObject::Character(VectorData::Lazy(span));
    let mut ctx = MaterializationContext::new(&raw);
    let changed = materialize_path(&mut lazy_obj, "", &mut ctx).unwrap();
    assert!(changed, "Character must report as materialized");

    match lazy_obj {
        RObject::Character(VectorData::Owned(values)) => assert_eq!(values, expected),
        other => panic!("expected owned character vector, got {:?}", other),
    }
}

#[test]
fn materialize_character_already_owned_is_noop() {
    let raw: Vec<u8> = Vec::new();
    let mut vector: VectorData<Option<Arc<str>>> = vec![Some(Arc::from("x"))].into();
    let mut ctx = MaterializationContext::new(&raw);
    ctx.materialize_character_data(&mut vector).unwrap();
    match vector {
        VectorData::Owned(v) => assert_eq!(v, vec![Some(Arc::from("x"))]),
        VectorData::Lazy(_) => panic!("owned vector became lazy"),
    }
}

#[test]
fn materialize_character_budget_exceeded() {
    let obj = big_char_vec(None);
    let (raw, span) = lazy_char_stream(&obj);

    // Budget smaller than the span's on-wire size.
    let mut ctx = MaterializationContext::with_budget(&raw, (span.byte_len as usize) - 1);
    let err = ctx.materialize_character_vector(span).unwrap_err();
    assert!(matches!(err, Error::MemoryBudgetExceeded { .. }));
}

#[test]
fn materialize_character_truncated_errors() {
    let obj = big_char_vec(None);
    let (raw, span) = lazy_char_stream(&obj);

    // Chop the buffer mid-span so the reader runs out of bytes.
    let truncated = &raw[..(span.offset as usize) + 4];
    let mut ctx = MaterializationContext::new(truncated);
    let err = ctx.materialize_character_vector(span).unwrap_err();
    assert!(
        matches!(err, Error::TruncatedLazyPayload { .. }),
        "expected TruncatedLazyPayload, got {:?}",
        err
    );
}

/// Helper: materialize a character span from a decompressed buffer.
fn materialize_char(raw: &[u8], span: LazyVector) -> Vec<Option<Arc<str>>> {
    let mut ctx = MaterializationContext::new(raw);
    ctx.materialize_character_vector(span).unwrap()
}
