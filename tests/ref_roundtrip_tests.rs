//! Roundtrip tests for reference tracking functionality.
//!
//! These tests verify that RDS files with reference tracking (REFSXP)
//! can be read and written back successfully.

use rds2rust::{read_rds, write_rds};
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
fn test_roundtrip_ref_shared_vector() {
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
fn test_roundtrip_ref_shared_list() {
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
fn test_roundtrip_ref_complex_shared() {
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
fn test_roundtrip_ref_shared_expression() {
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

    // The structure should match
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_roundtrip_ref_large_shared() {
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
