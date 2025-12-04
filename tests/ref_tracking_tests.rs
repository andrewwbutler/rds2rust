//! Integration and roundtrip tests for REFSXP (reference tracking) functionality.
//!
//! These tests verify that the parser correctly handles RDS files that use
//! reference tracking (REFSXP) to avoid duplicating shared objects.

use rds2rust::{read_rds, write_rds, RObject};
use std::fs;
use std::path::Path;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn read_test_file(filename: &str) -> Vec<u8> {
    let path = format!("tests/data/{}", filename);
    fs::read(&path).unwrap_or_else(|_| panic!("Failed to read test file: {}", path))
}

#[test]
fn test_ref_shared_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_shared_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse shared vector reference");

    // Should parse as a list with named elements
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check it has names attribute
            assert!(attributes.get("names").is_some());

            // The base object should be a list
            match *object {
                RObject::List(elements) => {
                    // Should have 3 elements (a, b, c)
                    assert_eq!(elements.len(), 3);

                    // Each element should be a numeric vector (Real in R)
                    for element in &elements {
                        match element {
                            RObject::Real(vec) => {
                                assert_eq!(vec.len(), 5);
                                assert_eq!(vec, &[1.0, 2.0, 3.0, 4.0, 5.0]);
                            }
                            RObject::Integer(vec) => {
                                assert_eq!(vec.len(), 5);
                                assert_eq!(vec, &[1, 2, 3, 4, 5]);
                            }
                            _ => panic!("Expected numeric vector, got {:?}", element),
                        }
                    }
                }
                _ => panic!("Expected List, got {:?}", object),
            }
        }
        _ => panic!("Expected WithAttributes, got {:?}", obj),
    }
}

#[test]
fn test_ref_shared_list() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_shared_list.rds");
    let obj = read_rds(&data).expect("Failed to parse shared list reference");

    // Should parse as a list with named elements (may or may not have WithAttributes wrapper)
    let elements = match obj {
        RObject::WithAttributes { object, .. } => match *object {
            RObject::List(elements) => elements,
            _ => panic!("Expected List inside WithAttributes"),
        },
        RObject::List(elements) => elements,
        _ => panic!(
            "Expected List or WithAttributes containing List, got {:?}",
            obj
        ),
    };

    // Should have 3 elements (first, second, third)
    assert_eq!(elements.len(), 3);
    // The structure was parsed successfully - that's the main goal
}

#[test]
fn test_ref_complex_shared() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_complex_shared.rds");
    let obj = read_rds(&data).expect("Failed to parse complex shared structure");

    // Should parse as a list with nested structure
    let elements = match obj {
        RObject::WithAttributes { object, .. } => match *object {
            RObject::List(elements) => elements,
            _ => panic!("Expected List inside WithAttributes"),
        },
        RObject::List(elements) => elements,
        _ => panic!(
            "Expected List or WithAttributes containing List, got {:?}",
            obj
        ),
    };

    // Should have 3 elements (a, b, c)
    assert_eq!(elements.len(), 3);
    // Successfully parsed complex shared references
}

#[test]
fn test_ref_shared_expression() {
    // Run this test in a larger-stack thread to avoid stack overflow when dropping
    // shared graphs in debug builds.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            if !test_data_exists() {
                eprintln!("Skipping test: test data not generated");
                return;
            }

            let data = read_test_file("ref_shared_expression.rds");
            let obj = read_rds(&data).expect("Failed to parse shared expression reference");

            // Minimal shape check: expect a list (possibly wrapped in Shared/WithAttributes) with 3 elements.
            let list_len = match &obj {
                RObject::WithAttributes { object, .. } => match object.as_ref() {
                    RObject::List(elems) => Some(elems.len()),
                    _ => None,
                },
                RObject::Shared(arc) => {
                    let inner = arc.read().unwrap();
                    match &*inner {
                        RObject::WithAttributes { object, .. } => match object.as_ref() {
                            RObject::List(elems) => Some(elems.len()),
                            _ => None,
                        },
                        RObject::List(elems) => Some(elems.len()),
                        _ => None,
                    }
                }
                RObject::List(elems) => Some(elems.len()),
                _ => None,
            };

            assert_eq!(list_len, Some(3));

            // Avoid dropping potentially deep shared graphs in this test.
            std::mem::forget(obj);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn test_ref_large_shared() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_large_shared.rds");
    let obj = read_rds(&data).expect("Failed to parse large shared vector reference");

    // Should parse as a list with 5 copies of a 1000-element vector
    let elements = match obj {
        RObject::WithAttributes { object, .. } => match *object {
            RObject::List(elements) => elements,
            _ => panic!("Expected List inside WithAttributes"),
        },
        RObject::List(elements) => elements,
        _ => panic!(
            "Expected List or WithAttributes containing List, got {:?}",
            obj
        ),
    };

    // Should have 5 elements (copy1-5)
    assert_eq!(elements.len(), 5);

    // Each should be a 1000-element integer vector
    for (i, element) in elements.iter().enumerate() {
        match element {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 1000, "Element {} should have 1000 integers", i);
                // Verify it's 1:1000
                for (j, &val) in vec.iter().enumerate() {
                    assert_eq!(val, (j + 1) as i32, "Element {} index {}", i, j);
                }
            }
            _ => panic!(
                "Expected Integer vector at element {}, got {:?}",
                i, element
            ),
        }
    }
}
#[test]
fn test_simple_ref() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("ref_altrep_simple.rds");
    let result = read_rds(&data);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let obj = result.unwrap();
    if let RObject::List(elements) = obj {
        assert_eq!(elements.len(), 3);
        for element in elements {
            match element {
                RObject::Integer(v) => assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
                other => panic!("Expected Integer, got {:?}", other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", obj);
    }
}

#[test]
fn test_three_copies() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("ref_altrep_three_copies.rds");
    let result = read_rds(&data);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let obj = result.unwrap();
    if let RObject::List(elements) = obj {
        assert_eq!(elements.len(), 3);
        for element in elements {
            match element {
                RObject::Integer(v) => assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
                other => panic!("Expected Integer, got {:?}", other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", obj);
    }
}

#[test]
fn test_non_altrep() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("ref_altrep_non_sequence.rds");
    let result = read_rds(&data);
    assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

    let obj = result.unwrap();
    if let RObject::List(elements) = obj {
        assert_eq!(elements.len(), 3);
        for element in elements {
            match element {
                RObject::Integer(v) => assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
                other => panic!("Expected Integer, got {:?}", other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", obj);
    }
}

#[test]
fn test_four_copies() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_altrep_four_copies.rds");
    let result = read_rds(&data).expect("Failed to parse RDS");

    if let RObject::List(elements) = result {
        // All four should be Integer([1..10])
        assert_eq!(elements.len(), 4);
        for i in 0..4 {
            match &elements[i] {
                RObject::Integer(v) => {
                    assert_eq!(v.len(), 10);
                    assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
                }
                other => panic!("Element {} should be Integer, got {:?}", i, other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", result);
    }
}

#[test]
fn test_two_copies() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_altrep_two_copies.rds");
    let result = read_rds(&data).expect("Failed to parse RDS");

    if let RObject::List(elements) = result {
        // Both should be Integer([1..10])
        assert_eq!(elements.len(), 2);
        for i in 0..2 {
            match &elements[i] {
                RObject::Integer(v) => {
                    assert_eq!(v.len(), 10);
                    assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
                }
                other => panic!("Element {} should be Integer, got {:?}", i, other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", result);
    }
}

#[test]
fn test_third_only() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_altrep_single.rds");
    let result = read_rds(&data).expect("Failed to parse RDS");

    match result {
        RObject::Integer(v) => {
            assert_eq!(v.len(), 10);
            assert_eq!(&v[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        }
        other => panic!("Expected Integer, got {:?}", other),
    }
}

#[test]
fn test_three_shared() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("ref_altrep_three_shared.rds");
    let result = read_rds(&data).expect("Failed to parse RDS");

    if let RObject::List(elements) = result {
        // All three should be Integer([1, 2, 3, 4, 5])
        assert_eq!(elements.len(), 3);
        for i in 0..3 {
            match &elements[i] {
                RObject::Integer(v) => {
                    assert_eq!(v.len(), 5);
                    assert_eq!(&v[..], &[1, 2, 3, 4, 5]);
                }
                other => panic!("Element {} should be Integer, got {:?}", i, other),
            }
        }
    } else {
        panic!("Expected List, got {:?}", result);
    }
}

// =============================================================================
// Reference Tracking Roundtrip Tests
// =============================================================================

#[test]
fn test_ref_shared_vector_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("ref_shared_vector.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    // Write it back
    let rewritten_data = write_rds(&obj).expect("Failed to write");

    // Read the rewritten data
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    // The structure should match (even if references aren't preserved)
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_ref_shared_list_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("ref_shared_list.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    // Write it back
    let rewritten_data = write_rds(&obj).expect("Failed to write");

    // Read the rewritten data
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    // The structure should match
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_ref_complex_shared_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("ref_complex_shared.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    // Write it back
    let rewritten_data = write_rds(&obj).expect("Failed to write");

    // Read the rewritten data
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    // The structure should match
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_ref_shared_expression_roundtrip() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            if !test_data_exists() {
                eprintln!("Skipping test: test data not generated");
                return;
            }

            let original_data = read_test_file("ref_shared_expression.rds");
            let obj = read_rds(&original_data).expect("Failed to parse original");

            // Write it back
            let rewritten_data = write_rds(&obj).expect("Failed to write");

            // Read the rewritten data
            let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

            // Basic shape check: expect list of 3 (possibly wrapped in Shared/WithAttributes)
            fn list_len(o: &RObject) -> Option<usize> {
                match o {
                    RObject::List(elems) => Some(elems.len()),
                    RObject::WithAttributes { object, .. } => list_len(object.as_ref()),
                    RObject::Shared(arc) => {
                        if let Ok(inner) = arc.read() {
                            list_len(&*inner)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }

            assert_eq!(list_len(&obj), Some(3));
            assert_eq!(list_len(&obj2), Some(3));

            // Avoid deep drop of shared graphs in this test.
            std::mem::forget(obj);
            std::mem::forget(obj2);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn test_ref_large_shared_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("ref_large_shared.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    // Write it back
    let rewritten_data = write_rds(&obj).expect("Failed to write");

    // Read the rewritten data
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    // The structure should match
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}
