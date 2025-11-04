//! Tests for ALTREP (Alternative Representations) support.
//!
//! ALTREP was introduced in R 3.5.0 for efficient memory representations.
//! These tests verify that we can parse and materialize ALTREP objects.

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

#[test]
fn test_altrep_intseq() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_intseq.rds");
    let obj = read_rds(&data).expect("Failed to parse ALTREP integer sequence");

    // ALTREP compact_intseq should be materialized to a regular integer vector
    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1, "First element should be 1");
            assert_eq!(vec[999], 1000, "Last element should be 1000");

            // Verify it's a proper sequence
            for (i, &val) in vec.iter().enumerate() {
                assert_eq!(val, (i + 1) as i32, "Element {} should be {}", i, i + 1);
            }
        }
        _ => panic!("Expected Integer vector, got {:?}", std::mem::discriminant(&obj)),
    }
}

#[test]
fn test_altrep_realseq() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_realseq.rds");
    let obj = read_rds(&data).expect("Failed to parse ALTREP real sequence");

    // ALTREP realseq might be materialized to integer or real vector
    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1.0, "First element should be 1.0");
            assert_eq!(vec[999], 1000.0, "Last element should be 1000.0");
        }
        RObject::Integer(vec) => {
            // Some ALTREP realseq are materialized as integers if stride is 1
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1, "First element should be 1");
            assert_eq!(vec[999], 1000, "Last element should be 1000");
        }
        _ => panic!("Expected Real or Integer vector, got {:?}", std::mem::discriminant(&obj)),
    }
}

#[test]
fn test_altrep_in_list() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_in_list.rds");
    let obj = read_rds(&data).expect("Failed to parse list containing ALTREP");

    // The object should be a list with attributes
    let (list, attrs) = match obj {
        RObject::WithAttributes { object, attributes } => (object, attributes),
        _ => panic!("Expected WithAttributes, got {:?}", std::mem::discriminant(&obj)),
    };

    // Check the list has correct names
    if let Some(RObject::Character(names)) = attrs.get("names") {
        assert_eq!(names.len(), 3);
        assert_eq!(names[0].as_ref(), "seq");
        assert_eq!(names[1].as_ref(), "data");
        assert_eq!(names[2].as_ref(), "another_seq");
    } else {
        panic!("Expected 'names' attribute");
    }

    // Check the list elements
    if let RObject::List(elements) = list.as_ref() {
        assert_eq!(elements.len(), 3);

        // First element: ALTREP sequence 1:100
        match &elements[0] {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 100);
                assert_eq!(vec[0], 1);
                assert_eq!(vec[99], 100);
            }
            _ => panic!("Expected first element to be Integer vector"),
        }

        // Second element: regular real vector
        match &elements[1] {
            RObject::Real(vec) => {
                assert_eq!(vec.len(), 3);
                assert_eq!(vec[0], 1.5);
                assert_eq!(vec[1], 2.5);
                assert_eq!(vec[2], 3.5);
            }
            _ => panic!("Expected second element to be Real vector"),
        }

        // Third element: ALTREP sequence 50:150
        match &elements[2] {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 101);
                assert_eq!(vec[0], 50);
                assert_eq!(vec[100], 150);
            }
            _ => panic!("Expected third element to be Integer vector"),
        }
    } else {
        panic!("Expected List");
    }
}

#[test]
fn test_regular_int_no_altrep() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("regular_int.rds");
    let obj = read_rds(&data).expect("Failed to parse regular integer vector");

    // Regular integer vector (not ALTREP) should parse normally
    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 5);
            assert_eq!(vec[0], 1);
            assert_eq!(vec[1], 2);
            assert_eq!(vec[2], 3);
            assert_eq!(vec[3], 4);
            assert_eq!(vec[4], 5);
        }
        _ => panic!("Expected Integer vector, got {:?}", std::mem::discriminant(&obj)),
    }
}
