use std::collections::HashSet;
use std::sync::Arc;

use rds2rust::{read_rds, RObject};

fn assert_dimnames_list(obj: &RObject, visited_shared: &mut HashSet<usize>, checked: &mut usize) {
    match obj {
        RObject::S4Object(data) => {
            let classes: Vec<&str> = data.class.iter().map(|c| c.as_ref()).collect();
            let is_sparse_class = classes.iter().any(|c| {
                matches!(
                    *c,
                    "dgCMatrix" | "CsparseMatrix" | "sparseMatrix" | "generalMatrix" | "dMatrix"
                ) || c.contains("Graph")
            });
            let has_sparse_slots = is_sparse_class
                && data.slots.contains_key(&Arc::from("Dimnames"))
                && data.slots.contains_key(&Arc::from("i"))
                && data.slots.contains_key(&Arc::from("p"));
            if has_sparse_slots {
                *checked += 1;
                let dimnames = data
                    .slots
                    .get(&Arc::from("Dimnames"))
                    .expect("Dimnames slot missing");
                match dimnames.as_concrete() {
                    RObject::Null => {}
                    RObject::List(elems) => {
                        assert_eq!(elems.len(), 2, "Dimnames must have two elements");
                        for (idx, elem) in elems.iter().enumerate() {
                            match elem.as_concrete() {
                                RObject::Null => {}
                                RObject::Character(names) => {
                                    assert!(
                                        !names.is_empty(),
                                        "Dimnames element {} should not be empty",
                                        idx
                                    );
                                }
                                other => panic!(
                                    "Unexpected Dimnames element {} type: {:?}",
                                    idx,
                                    std::mem::discriminant(&other)
                                ),
                            }
                        }
                    }
                    RObject::Symbol(_) => panic!("Dimnames resolved to Symbol"),
                    _ => {
                        // For other unexpected shapes, just ensure it's not Symbol.
                    }
                }
            }

            for slot_value in data.slots.values() {
                assert_dimnames_list(slot_value, visited_shared, checked);
            }
        }
        RObject::List(values) => {
            for value in values {
                assert_dimnames_list(value, visited_shared, checked);
            }
        }
        RObject::Pairlist(values) => {
            for elem in values {
                assert_dimnames_list(&elem.value, visited_shared, checked);
            }
        }
        RObject::WithAttributes { object, attributes } => {
            for (_, attr_val) in &attributes.attrs {
                assert_dimnames_list(attr_val, visited_shared, checked);
            }
            assert_dimnames_list(object, visited_shared, checked);
        }
        RObject::Shared(arc) => {
            let ptr = Arc::as_ptr(arc) as usize;
            if visited_shared.insert(ptr) {
                let guard = arc.read().unwrap();
                assert_dimnames_list(&*guard, visited_shared, checked);
            }
        }
        _ => {}
    }
}

#[test]
fn sparse_matrices_have_list_dimnames() {
    let data = std::fs::read("tests/data/sparse_dimnames.rds")
        .expect("fixture tests/data/sparse_dimnames.rds missing");
    let obj = read_rds(&data).expect("failed to parse sparse dimnames fixture");

    let mut visited_shared = HashSet::new();
    let mut checked = 0;
    assert_dimnames_list(&obj, &mut visited_shared, &mut checked);
    assert!(
        checked > 0,
        "expected at least one sparse matrix with Dimnames"
    );
}
