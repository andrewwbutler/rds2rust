//! Integration and roundtrip tests for objects with attributes.
//!
//! This includes named vectors and matrices.

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
                    assert_eq!(names_vec[0], "a");
                    assert_eq!(names_vec[1], "b");
                    assert_eq!(names_vec[2], "c");
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
                    assert_eq!(names_vec[0], "x");
                    assert_eq!(names_vec[1], "y");
                    assert_eq!(names_vec[2], "z");
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
