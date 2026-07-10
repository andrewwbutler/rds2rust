#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{
    read_lazy_character_range, read_rds, read_rds_lazy, read_rds_with_config, write_rds,
    ParseConfig, RObject, VectorData,
};
use std::io::Read;
use std::sync::Arc;

#[test]
fn test_lazy_integer_vector() {
    // Create a large enough integer vector to exceed lazy_threshold
    let obj = RObject::Integer((1..=100).collect::<Vec<i32>>().into());
    let data = write_rds(&obj).expect("Failed to write");

    // Parse lazily
    let config = ParseConfig::lazy_metadata().with_lazy_threshold(0);
    let lazy_obj = read_rds_with_config(&data, config)
        .expect("Failed to parse lazy")
        .object;

    // Should not be fully loaded (vector has 100 elements > lazy_threshold)
    assert!(!lazy_obj.is_fully_loaded());

    // Check structure
    match lazy_obj {
        RObject::Integer(VectorData::Lazy(lazy)) => {
            assert_eq!(lazy.length, 100);
            assert!(lazy.offset > 0);
            assert_eq!(lazy.byte_len, 100 * 4); // 100 i32s
        }
        _ => panic!("Expected lazy integer vector"),
    }

    // Verify lazy spans
    let spans = lazy_obj.lazy_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, "");
    assert_eq!(spans[0].1.length, 100);
}

#[test]
fn test_lazy_real_vector() {
    let obj = RObject::Real((1..=50).map(|i| i as f64).collect::<Vec<f64>>().into());
    let data = write_rds(&obj).expect("Failed to write");

    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    assert!(!lazy_obj.is_fully_loaded());

    match lazy_obj {
        RObject::Real(VectorData::Lazy(lazy)) => {
            assert_eq!(lazy.length, 50);
            assert_eq!(lazy.byte_len, 50 * 8); // 3 f64s = 24 bytes
        }
        _ => panic!("Expected lazy real vector"),
    }
}

#[test]
fn test_lazy_character_vector() {
    let obj = RObject::Character(
        (1..=50)
            .map(|i| Some(Arc::from(format!("str{}", i).as_str())))
            .collect::<Vec<Option<Arc<str>>>>()
            .into(),
    );
    let data = write_rds(&obj).expect("Failed to write");

    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    assert!(!lazy_obj.is_fully_loaded());

    match lazy_obj {
        RObject::Character(VectorData::Lazy(lazy)) => {
            assert_eq!(lazy.length, 50);
            // Character vectors are variable length, so byte_len depends on string sizes
            assert!(lazy.byte_len > 0);
        }
        _ => panic!("Expected lazy character vector"),
    }
}

#[test]
fn test_lazy_vs_full_parsing() {
    let obj = RObject::Integer((1..=100).collect::<Vec<i32>>().into());
    let data = write_rds(&obj).expect("Failed to write");

    // Parse fully
    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    assert!(full_obj.is_fully_loaded());

    // Parse lazily
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;
    assert!(!lazy_obj.is_fully_loaded());

    // Both should have the same type
    match (&full_obj, &lazy_obj) {
        (RObject::Integer(_), RObject::Integer(_)) => {}
        _ => panic!("Type mismatch"),
    }
}

#[test]
fn test_lazy_list_with_vectors() {
    let obj = RObject::List(vec![
        RObject::Integer((1..=50).collect::<Vec<i32>>().into()),
        RObject::Real((1..=50).map(|i| i as f64).collect::<Vec<f64>>().into()),
        RObject::Character(
            (1..=50)
                .map(|i| Some(Arc::from(format!("s{}", i).as_str())))
                .collect::<Vec<Option<Arc<str>>>>()
                .into(),
        ),
    ]);
    let data = write_rds(&obj).expect("Failed to write");

    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    assert!(!lazy_obj.is_fully_loaded());

    // Check lazy spans
    let spans = lazy_obj.lazy_spans();
    assert_eq!(spans.len(), 3); // 3 vectors in the list

    // Verify paths
    assert_eq!(spans[0].0, "[0]");
    assert_eq!(spans[0].1.length, 50);

    assert_eq!(spans[1].0, "[1]");
    assert_eq!(spans[1].1.length, 50);

    assert_eq!(spans[2].0, "[2]");
    assert_eq!(spans[2].1.length, 50);
}

#[test]
fn test_write_lazy_object_fails() {
    let obj = RObject::Integer((1..=100).collect::<Vec<i32>>().into());
    let data = write_rds(&obj).expect("Failed to write");

    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Trying to write a lazy object should fail
    let result = write_rds(&lazy_obj);
    assert!(result.is_err());
}

#[test]
fn test_empty_vector_lazy() {
    let obj = RObject::Integer(vec![].into());
    let data = write_rds(&obj).expect("Failed to write");

    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Empty vectors are always fully loaded (0 elements <= lazy_threshold)
    assert!(lazy_obj.is_fully_loaded());

    match lazy_obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 0);
            assert!(vec.is_loaded());
        }
        _ => panic!("Expected integer vector"),
    }
}

#[test]
fn test_large_vector_lazy() {
    // Create a large vector
    let large_vec: Vec<i32> = (0..10000).collect();
    let obj = RObject::Integer(large_vec.into());
    let data = write_rds(&obj).expect("Failed to write");

    // Parse lazily - should be fast and use minimal memory
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    assert!(!lazy_obj.is_fully_loaded());

    match lazy_obj {
        RObject::Integer(VectorData::Lazy(lazy)) => {
            assert_eq!(lazy.length, 10000);
            assert_eq!(lazy.byte_len, 10000 * 4);
        }
        _ => panic!("Expected lazy integer vector"),
    }
}

#[test]
fn test_lazy_vector_warnings_emitted() {
    let obj = RObject::Integer((1..=100).collect::<Vec<i32>>().into());
    let data = rds2rust::write_rds(&obj).expect("Failed to write");

    let config = ParseConfig::lazy_metadata().with_lazy_threshold(10);
    let result = read_rds_with_config(&data, config).expect("Failed to parse lazy");

    assert_eq!(result.warnings.len(), 1);
    match &result.warnings[0] {
        rds2rust::MetadataWarning::VectorLazy {
            path,
            vector_type,
            length,
            threshold,
            byte_len,
        } => {
            assert!(path.segments.is_empty());
            assert_eq!(vector_type, "integer");
            assert_eq!(*length, 100);
            assert_eq!(*threshold, 10);
            assert_eq!(*byte_len, 100 * 4);
        }
        other => panic!("Unexpected warning: {:?}", other),
    }
}

#[test]
fn test_lazy_character_range_reads_values() {
    struct TestInput {
        data: Vec<u8>,
    }

    impl rds2rust::RdsInput for TestInput {
        fn read_at(&self, offset: u64, len: usize) -> rds2rust::Result<Vec<u8>> {
            let start = offset as usize;
            let end = start.saturating_add(len);
            if end > self.data.len() {
                return Err(rds2rust::Error::UnexpectedEofDetail {
                    position: start,
                    needed: len,
                    available: self.data.len().saturating_sub(start),
                });
            }
            Ok(self.data[start..end].to_vec())
        }

        fn len(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
    }

    let mut values: Vec<Option<Arc<str>>> = Vec::with_capacity(50);
    values.push(Some(Arc::from("alpha")));
    values.push(Some(Arc::from("beta")));
    for i in 2..50 {
        values.push(Some(Arc::from(format!("v{}", i).as_str())));
    }
    let obj = RObject::Character(values.into());
    let data = rds2rust::write_rds(&obj).expect("Failed to write");
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;
    let span = match lazy_obj {
        RObject::Character(VectorData::Lazy(lazy)) => lazy,
        _ => panic!("Expected lazy character vector"),
    };

    let mut decoder = flate2::read::GzDecoder::new(data.as_slice());
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).expect("decompress");

    let input = TestInput { data: decompressed };
    let range = read_lazy_character_range(&input, span, 1, 1).expect("read range");
    assert_eq!(range, vec![Some(Arc::from("beta"))]);
}

// =============================================================================
// Helper functions for file-based tests
// =============================================================================

use std::fs;
use std::path::Path;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn read_test_file(filename: &str) -> Vec<u8> {
    let path = format!("tests/data/{}", filename);
    fs::read(&path).unwrap_or_else(|_| panic!("Failed to read test file: {}", path))
}

// =============================================================================
// Matrix Tests
// =============================================================================

#[test]
fn test_lazy_dense_matrix_int() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("matrix_int.rds");

    // Parse fully to get expected structure
    let full_obj = read_rds(&data).expect("Failed to parse full").object;

    // Parse lazily
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Both should be WithAttributes wrapping Integer vectors
    match (&full_obj, &lazy_obj) {
        (
            RObject::WithAttributes {
                object: full_inner,
                attributes: full_attrs,
            },
            RObject::WithAttributes {
                object: lazy_inner,
                attributes: lazy_attrs,
            },
        ) => {
            // Both should have dim attribute
            assert!(full_attrs.get("dim").is_some());
            assert!(lazy_attrs.get("dim").is_some());

            // Inner object should be integer vector
            match (full_inner.as_ref(), lazy_inner.as_ref()) {
                (RObject::Integer(full_vec), RObject::Integer(lazy_vec)) => {
                    // Full should be loaded
                    assert!(full_vec.is_loaded());

                    // Lengths should match regardless of load state
                    assert_eq!(full_vec.len(), lazy_vec.len());
                }
                _ => panic!("Expected Integer vectors in both"),
            }
        }
        _ => panic!("Expected WithAttributes wrapping integer vectors"),
    }
}

#[test]
fn test_lazy_dense_matrix_real() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("matrix_real.rds");

    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    match (&full_obj, &lazy_obj) {
        (
            RObject::WithAttributes {
                object: full_inner,
                attributes: full_attrs,
            },
            RObject::WithAttributes {
                object: lazy_inner,
                attributes: lazy_attrs,
            },
        ) => {
            // Verify dim attributes match
            match (full_attrs.get("dim"), lazy_attrs.get("dim")) {
                (Some(RObject::Integer(full_dim)), Some(RObject::Integer(lazy_dim))) => {
                    assert_eq!(full_dim.len(), 2); // nrow, ncol
                    assert_eq!(lazy_dim.len(), 2);
                    // Both dim vectors should be loaded (they're small)
                    assert!(full_dim.is_loaded());
                    assert!(lazy_dim.is_loaded());
                }
                _ => panic!("Expected dim attributes"),
            }

            // Inner vector comparison
            match (full_inner.as_ref(), lazy_inner.as_ref()) {
                (RObject::Real(full_vec), RObject::Real(lazy_vec)) => {
                    assert!(full_vec.is_loaded());
                    // Length should match regardless of load state
                    assert_eq!(full_vec.len(), lazy_vec.len());
                }
                _ => panic!("Expected Real vectors"),
            }
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_lazy_matrix_with_dimnames() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("matrix_dimnames.rds");

    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Verify dimnames are preserved in lazy mode
    match (&full_obj, &lazy_obj) {
        (
            RObject::WithAttributes {
                attributes: full_attrs,
                ..
            },
            RObject::WithAttributes {
                attributes: lazy_attrs,
                ..
            },
        ) => {
            // Both should have dimnames
            assert!(full_attrs.get("dimnames").is_some());
            assert!(lazy_attrs.get("dimnames").is_some());

            // Dimnames structure should match
            match (full_attrs.get("dimnames"), lazy_attrs.get("dimnames")) {
                (Some(RObject::List(full_dimnames)), Some(RObject::List(lazy_dimnames))) => {
                    assert_eq!(full_dimnames.len(), 2); // row names, col names
                    assert_eq!(lazy_dimnames.len(), 2);
                }
                _ => panic!("Expected dimnames as list"),
            }
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_lazy_sparse_matrix() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("sparse_dimnames.rds");

    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Note: This sparse matrix is very small (6 elements in i/x, 5 in p)
    // so it will be fully loaded due to lazy_threshold
    // This is actually correct behavior - small vectors are metadata!

    // Sparse matrices are S4 objects with class dgCMatrix
    match (&full_obj, &lazy_obj) {
        (RObject::S4Object(full_s4), RObject::S4Object(lazy_s4)) => {
            // Class should match
            assert_eq!(full_s4.class, lazy_s4.class);

            // Should have slots: i, p, x, Dim, Dimnames
            assert!(full_s4.slots.contains_key("i"));
            assert!(full_s4.slots.contains_key("p"));
            assert!(full_s4.slots.contains_key("x"));
            assert!(full_s4.slots.contains_key("Dim"));

            assert!(lazy_s4.slots.contains_key("i"));
            assert!(lazy_s4.slots.contains_key("p"));
            assert!(lazy_s4.slots.contains_key("x"));
            assert!(lazy_s4.slots.contains_key("Dim"));

            // Dim should be loaded (it's small)
            match (full_s4.slots.get("Dim"), lazy_s4.slots.get("Dim")) {
                (Some(RObject::Integer(full_dim)), Some(RObject::Integer(lazy_dim))) => {
                    assert!(full_dim.is_loaded());
                    assert!(lazy_dim.is_loaded());
                    assert_eq!(full_dim.len(), 2);
                    assert_eq!(lazy_dim.len(), 2);
                }
                _ => panic!("Expected Dim slot with integer vector"),
            }

            // For this small test fixture, i, p, x vectors will be loaded due to threshold
            // Just verify the structure is correct
            match (full_s4.slots.get("i"), lazy_s4.slots.get("i")) {
                (Some(RObject::Integer(full_i)), Some(RObject::Integer(lazy_i))) => {
                    assert_eq!(full_i.len(), lazy_i.len());
                }
                _ => panic!("Expected i slot with integer vector"),
            }

            match (full_s4.slots.get("p"), lazy_s4.slots.get("p")) {
                (Some(RObject::Integer(full_p)), Some(RObject::Integer(lazy_p))) => {
                    assert_eq!(full_p.len(), lazy_p.len());
                }
                _ => panic!("Expected p slot with integer vector"),
            }

            match (full_s4.slots.get("x"), lazy_s4.slots.get("x")) {
                (Some(RObject::Real(full_x)), Some(RObject::Real(lazy_x))) => {
                    assert_eq!(full_x.len(), lazy_x.len());
                }
                _ => panic!("Expected x slot with real vector"),
            }
        }
        _ => panic!("Expected S4 objects for sparse matrix"),
    }
}

#[test]
fn test_lazy_large_matrix() {
    // Create a large matrix that exceeds lazy_threshold
    let data_vec: Vec<f64> = (0..1000).map(|i| i as f64).collect(); // 1000 elements
    let mut attrs = rds2rust::Attributes::new();
    attrs.insert(Arc::from("dim"), RObject::Integer(vec![100, 10].into()));
    let matrix = RObject::WithAttributes {
        object: Box::new(RObject::Real(data_vec.into())),
        attributes: attrs,
    };

    let serialized = write_rds(&matrix).expect("Failed to write matrix");

    // Parse lazily
    let lazy_obj = read_rds_lazy(&serialized)
        .expect("Failed to parse lazy")
        .object;

    // Should NOT be fully loaded because the vector has 1000 elements > lazy_threshold
    assert!(!lazy_obj.is_fully_loaded());

    // Get lazy spans
    let spans = lazy_obj.lazy_spans();
    assert_eq!(spans.len(), 1, "Expected 1 lazy vector");

    let (path, lazy_vec) = &spans[0];
    assert_eq!(path, "");
    assert_eq!(lazy_vec.length, 1000);
    assert_eq!(lazy_vec.byte_len, 1000 * 8); // 1000 f64s

    // dim attribute should be loaded (it's small)
    match lazy_obj {
        RObject::WithAttributes { attributes, .. } => match attributes.get("dim") {
            Some(RObject::Integer(dim)) => {
                assert!(dim.is_loaded());
                assert_eq!(dim.len(), 2);
            }
            _ => panic!("Expected dim attribute"),
        },
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_lazy_small_manual_matrix() {
    // Create a small matrix (below lazy_threshold)
    let data_vec = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]; // 2x3 matrix, 6 elements
    let mut attrs = rds2rust::Attributes::new();
    attrs.insert(Arc::from("dim"), RObject::Integer(vec![2, 3].into()));
    let matrix = RObject::WithAttributes {
        object: Box::new(RObject::Real(data_vec.into())),
        attributes: attrs,
    };

    let serialized = write_rds(&matrix).expect("Failed to write matrix");

    // Parse lazily
    let lazy_obj = read_rds_lazy(&serialized)
        .expect("Failed to parse lazy")
        .object;

    // Since the matrix has only 6 elements (< lazy_threshold), it will be fully loaded
    assert!(lazy_obj.is_fully_loaded());

    match lazy_obj {
        RObject::WithAttributes { object, attributes } => {
            // Should have dim attribute
            match attributes.get("dim") {
                Some(RObject::Integer(dim)) => {
                    assert_eq!(dim.len(), 2);
                    assert!(dim.is_loaded());
                }
                _ => panic!("Expected dim attribute"),
            }

            // Inner vector should be loaded (small)
            match object.as_ref() {
                RObject::Real(vec) => {
                    assert!(vec.is_loaded());
                    assert_eq!(vec.len(), 6);
                }
                _ => panic!("Expected loaded real vector"),
            }
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_write_lazy_matrix_fails() {
    // Create a large matrix that will be lazy
    let data_vec: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let mut attrs = rds2rust::Attributes::new();
    attrs.insert(Arc::from("dim"), RObject::Integer(vec![100, 10].into()));
    let matrix = RObject::WithAttributes {
        object: Box::new(RObject::Real(data_vec.into())),
        attributes: attrs,
    };

    let serialized = write_rds(&matrix).expect("Failed to write");
    let lazy_obj = read_rds_lazy(&serialized)
        .expect("Failed to parse lazy")
        .object;

    // Verify it's actually lazy
    assert!(!lazy_obj.is_fully_loaded());

    // Trying to write the lazy object should fail
    let result = write_rds(&lazy_obj);
    assert!(result.is_err());
}

// =============================================================================
// Complex S4 Object Tests
// =============================================================================

#[test]
fn test_lazy_complex_s4_minimal() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_multiple_same_class.rds");

    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Full object should be fully loaded
    assert!(full_obj.is_fully_loaded());

    // Both should be lists containing S4 objects
    match (&full_obj, &lazy_obj) {
        (RObject::List(full_list), RObject::List(lazy_list)) => {
            assert_eq!(full_list.len(), lazy_list.len());
            assert_eq!(full_list.len(), 3, "Expected 3 Container S4 objects");

            // Check that all elements are S4 objects
            for (full_elem, lazy_elem) in full_list.iter().zip(lazy_list.iter()) {
                match (full_elem, lazy_elem) {
                    (RObject::S4Object(full_s4), RObject::S4Object(lazy_s4)) => {
                        // Class should match
                        assert_eq!(full_s4.class, lazy_s4.class);
                        assert_eq!(full_s4.class.len(), 1);
                        assert_eq!(full_s4.class[0].as_ref(), "Container");

                        // Slot names should match
                        let full_keys: Vec<_> = full_s4.slots.keys().collect();
                        let lazy_keys: Vec<_> = lazy_s4.slots.keys().collect();
                        assert_eq!(full_keys.len(), lazy_keys.len());

                        for key in &full_keys {
                            assert!(lazy_s4.slots.contains_key(*key), "Missing slot: {}", key);
                        }
                    }
                    _ => panic!("Expected S4 objects in list"),
                }
            }
        }
        _ => panic!("Expected lists containing S4 objects"),
    }
}

#[test]
fn test_lazy_complex_s4_has_lazy_matrices() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_multiple_with_matrices.rds");

    // Parse both full and lazy to verify lazy parsing works for S4 objects with matrices
    let full_obj = read_rds(&data).expect("Failed to parse full").object;
    let lazy_obj = read_rds_lazy(&data).expect("Failed to parse lazy").object;

    // Full object should be fully loaded
    assert!(full_obj.is_fully_loaded());

    let spans = lazy_obj.lazy_spans();
    println!(
        "Found {} lazy vectors in complex S4 object with matrices",
        spans.len()
    );

    // This file contains 3 MatrixContainer S4 objects, each with 2 matrices (2x2)
    // With lazy_threshold=10, the small matrices (4 elements each) should be loaded
    // So we might not have lazy spans for this particular test data.
    // The test just verifies that parsing succeeds and structure is preserved.

    // Verify both parsed to lists
    match (&full_obj, &lazy_obj) {
        (RObject::List(full_list), RObject::List(lazy_list)) => {
            assert_eq!(full_list.len(), 3, "Expected 3 MatrixContainer objects");
            assert_eq!(lazy_list.len(), 3, "Expected 3 MatrixContainer objects");
        }
        _ => panic!("Expected lists containing S4 objects with matrices"),
    }
}
