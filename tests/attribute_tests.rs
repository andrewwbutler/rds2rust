//! Integration and roundtrip tests for objects with attributes.
//!
//! This includes named vectors and matrices.

use rds2rust::{read_rds, RObject};
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
// Named Vector Tests
// =============================================================================

#[test]
fn test_int_named() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_named.rds");
    let obj = read_rds(&data).expect("Failed to parse named integer vector");

    // Named vectors have a "names" attribute
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // The object should be an integer vector with 3 elements
            match *object {
                RObject::Integer(ref vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 1);
                    assert_eq!(vec[1], 2);
                    assert_eq!(vec[2], 3);
                }
                _ => panic!("Expected Integer vector"),
            }

            // Check that we have a "names" attribute
            let names = attributes.get("names");
            assert!(names.is_some(), "Expected 'names' attribute");

            // The names should be a character vector ["a", "b", "c"]
            match names.unwrap() {
                RObject::Character(names_vec) => {
                    assert_eq!(names_vec.len(), 3);
                    assert_eq!(names_vec[0].as_ref(), "a");
                    assert_eq!(names_vec[1].as_ref(), "b");
                    assert_eq!(names_vec[2].as_ref(), "c");
                }
                _ => panic!("Expected Character vector for names"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

#[test]
fn test_real_named() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_named.rds");
    let obj = read_rds(&data).expect("Failed to parse named real vector");

    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check the vector values
            match *object {
                RObject::Real(ref vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 1.5);
                    assert_eq!(vec[1], 2.5);
                    assert_eq!(vec[2], 3.5);
                }
                _ => panic!("Expected Real vector"),
            }

            // Check the names attribute
            let names = attributes.get("names");
            assert!(names.is_some(), "Expected 'names' attribute");

            match names.unwrap() {
                RObject::Character(names_vec) => {
                    assert_eq!(names_vec.len(), 3);
                    assert_eq!(names_vec[0].as_ref(), "x");
                    assert_eq!(names_vec[1].as_ref(), "y");
                    assert_eq!(names_vec[2].as_ref(), "z");
                }
                _ => panic!("Expected Character vector for names"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

// =============================================================================
// Matrix Tests
// =============================================================================

#[test]
fn test_matrix_int() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("matrix_int.rds");
    let obj = read_rds(&data).expect("Failed to parse integer matrix");

    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Matrix is stored as a vector in column-major order
            match *object {
                RObject::Integer(ref vec) => {
                    assert_eq!(vec.len(), 6);
                    // Matrix is 2x3, column-major: [1,2] [3,4] [5,6]
                    assert_eq!(vec, &vec![1, 2, 3, 4, 5, 6]);
                }
                _ => panic!("Expected Integer vector"),
            }

            // Check the dim attribute
            let dim = attributes.get("dim");
            assert!(dim.is_some(), "Expected 'dim' attribute");

            match dim.unwrap() {
                RObject::Integer(dim_vec) => {
                    assert_eq!(dim_vec.len(), 2);
                    assert_eq!(dim_vec[0], 2); // nrow
                    assert_eq!(dim_vec[1], 3); // ncol
                }
                _ => panic!("Expected Integer vector for dim"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

// =============================================================================
// Symbol Table Test (Regression Test)
// =============================================================================

/// Test that attribute names are correctly resolved from the symbol table.
/// This is a regression test for a bug where REFSXP in TAG positions
/// were incorrectly looked up in the ref_table instead of the symbol_table,
/// causing attribute names to be replaced with incorrect values from elsewhere
/// in the file (e.g., showing "Cell_1" instead of "names").
///
/// When R serializes pairlist TAGs (like attribute names), REFSXP references
/// use indices into a symbol table (the N-th symbol parsed), not the regular
/// ref_table (the N-th object parsed).
#[test]
fn test_symbol_table_attribute_names() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("symbol_table_test.rds");
    let obj = read_rds(&data).expect("Failed to parse symbol table test");

    // The object should be a list with attributes (the "names" attribute)
    let (outer_list, outer_attrs) = match obj {
        RObject::WithAttributes { object, attributes } => (object, attributes),
        _ => panic!(
            "Expected outer list with attributes, got {:?}",
            std::mem::discriminant(&obj)
        ),
    };

    // CRITICAL: The outer list should have a "names" attribute
    // This tests that REFSXP(N) in TAG positions correctly looks up symbol N
    assert!(
        outer_attrs.get("names").is_some(),
        "Outer list should have 'names' attribute. Available attributes: {:?}",
        outer_attrs
            .attrs
            .iter()
            .map(|(k, _)| k.as_ref())
            .collect::<Vec<_>>()
    );

    // The "names" attribute should be ["first", "second", "third"]
    if let RObject::Character(names) = outer_attrs.get("names").unwrap() {
        assert_eq!(names.len(), 3, "Expected 3 names");
        assert_eq!(names[0].as_ref(), "first");
        assert_eq!(names[1].as_ref(), "second");
        assert_eq!(names[2].as_ref(), "third");
    } else {
        panic!("Expected 'names' attribute to be Character vector");
    }

    // Check the inner lists also have correct "names" attributes
    if let RObject::List(elements) = outer_list.as_ref() {
        assert_eq!(elements.len(), 3, "Expected 3 elements in outer list");

        // Check first inner list: should have names ["a", "b", "c"]
        if let RObject::WithAttributes {
            attributes: inner_attrs,
            ..
        } = &elements[0]
        {
            let inner_names = inner_attrs
                .get("names")
                .expect("First inner list should have 'names' attribute");
            if let RObject::Character(names) = inner_names {
                assert_eq!(names.len(), 3);
                assert_eq!(
                    names[0].as_ref(),
                    "a",
                    "First inner list names[0] should be 'a', not data from elsewhere"
                );
                assert_eq!(names[1].as_ref(), "b");
                assert_eq!(names[2].as_ref(), "c");
            } else {
                panic!("Expected inner names to be Character vector");
            }
        } else {
            panic!("Expected first element to have attributes");
        }

        // Check second inner list: should have names ["x", "y", "z"]
        if let RObject::WithAttributes {
            attributes: inner_attrs,
            ..
        } = &elements[1]
        {
            let inner_names = inner_attrs
                .get("names")
                .expect("Second inner list should have 'names' attribute");
            if let RObject::Character(names) = inner_names {
                assert_eq!(names.len(), 3);
                assert_eq!(
                    names[0].as_ref(),
                    "x",
                    "Second inner list names[0] should be 'x', not data from elsewhere"
                );
                assert_eq!(names[1].as_ref(), "y");
                assert_eq!(names[2].as_ref(), "z");
            } else {
                panic!("Expected inner names to be Character vector");
            }
        } else {
            panic!("Expected second element to have attributes");
        }

        // Check third inner list: should have names ["p", "q", "r"]
        if let RObject::WithAttributes {
            attributes: inner_attrs,
            ..
        } = &elements[2]
        {
            let inner_names = inner_attrs
                .get("names")
                .expect("Third inner list should have 'names' attribute");
            if let RObject::Character(names) = inner_names {
                assert_eq!(names.len(), 3);
                assert_eq!(
                    names[0].as_ref(),
                    "p",
                    "Third inner list names[0] should be 'p', not data from elsewhere"
                );
                assert_eq!(names[1].as_ref(), "q");
                assert_eq!(names[2].as_ref(), "r");
            } else {
                panic!("Expected inner names to be Character vector");
            }
        } else {
            panic!("Expected third element to have attributes");
        }
    } else {
        panic!("Expected outer object to be a List");
    }
}

// Regression: tolerate attribute parsing when attributes consume the final bytes.
#[test]
fn test_attr_at_eof_fixture() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }
    let path = Path::new("tests/data/attr_at_eof.rds");
    if !path.exists() {
        eprintln!("Skipping test: attr_at_eof.rds not generated");
        return;
    }

    let data = read_test_file("attr_at_eof.rds");
    let result = read_rds(&data);
    assert!(
        result.is_ok(),
        "Expected attr_at_eof.rds to parse successfully, got {:?}",
        result.err()
    );
}
