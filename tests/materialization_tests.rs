// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{materialize_path, MaterializationContext, RObject, VectorData};
use rds2rust::{Error, LazyVector};

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
