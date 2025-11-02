//! Integration and roundtrip tests for List and Pairlist types.

use rds2rust::{read_rds, write_rds, RObject};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn read_test_file(filename: &str) -> Vec<u8> {
    let path = format!("tests/data/{}", filename);
    fs::read(&path).unwrap_or_else(|_| panic!("Failed to read test file: {}", path))
}

// =============================================================================
// List Tests
// =============================================================================

#[test]
fn test_list_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple list");

    match obj {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);
            // Each element should be an integer vector with one element
            match &elements[0] {
                RObject::Integer(v) => assert_eq!(v, &vec![1]),
                other => panic!("Expected Integer, got {:?}", other),
            }
            match &elements[1] {
                RObject::Integer(v) => assert_eq!(v, &vec![2]),
                other => panic!("Expected Integer, got {:?}", other),
            }
            match &elements[2] {
                RObject::Integer(v) => assert_eq!(v, &vec![3]),
                other => panic!("Expected Integer, got {:?}", other),
            }
        }
        other => panic!("Expected List, got {:?}", other),
    }
}

#[test]
fn test_list_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_empty.rds");
    let obj = read_rds(&data).expect("Failed to parse empty list");

    match obj {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 0);
        }
        other => panic!("Expected empty List, got {:?}", other),
    }
}

#[test]
fn test_list_roundtrip() {
    let obj = RObject::List(vec![
        RObject::Integer(vec![1, 2, 3]),
        RObject::Character(vec![Arc::from("test")]),
        RObject::Real(vec![4.5]),
    ]);
    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read list");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_list_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing list");

    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized list");

    assert_eq!(obj, deserialized);
}
