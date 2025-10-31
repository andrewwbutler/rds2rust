// Integration tests for rds2rust
//
// Note: These tests require test RDS files to be generated.
// Run: Rscript tests/generate_test_data.R

use rds2rust::{read_rds, RObject};
use std::fs;
use std::path::Path;

// Helper function to read test data file
fn read_test_file(filename: &str) -> Vec<u8> {
    let path = Path::new("tests/data").join(filename);
    fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "Failed to read test file: {}. Did you run 'Rscript tests/generate_test_data.R'?",
            path.display()
        )
    })
}

// Helper to check if test data exists
fn test_data_exists() -> bool {
    Path::new("tests/data/null.rds").exists()
}

#[test]
fn test_null() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("null.rds");
    let obj = read_rds(&data).expect("Failed to parse NULL");

    match obj {
        RObject::Null => {} // Success
        other => panic!("Expected Null, got {:?}", other),
    }
}

#[test]
fn test_integer_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to parse integer");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1);
        }
        other => panic!("Expected Integer, got {:?}", other),
    }
}

#[test]
fn test_integer_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse integer vector");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 10);
            assert_eq!(vec, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        }
        other => panic!("Expected Integer vector, got {:?}", other),
    }
}

#[test]
fn test_real_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data).expect("Failed to parse real");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1.5);
        }
        other => panic!("Expected Real, got {:?}", other),
    }
}

#[test]
fn test_logical_true() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_true.rds");
    let obj = read_rds(&data).expect("Failed to parse logical");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], rds2rust::Logical::True);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_character_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data).expect("Failed to parse character");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], "hello");
        }
        other => panic!("Expected Character, got {:?}", other),
    }
}
