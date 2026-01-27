//! Integration and roundtrip tests for S3 objects.

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
// S3 Object Tests
// =============================================================================

#[test]
fn test_s3_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse simple S3 object")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;
            let attributes = &s3_data.attributes;

            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0].as_ref(), "my_custom_class");

            // Check the base object is a list
            match base.as_ref() {
                RObject::List(elements) => {
                    assert_eq!(elements.len(), 3);
                }
                _ => panic!("Expected List as base object"),
            }

            // Should have a names attribute for the list
            assert!(attributes.get("names").is_some());
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

#[test]
fn test_s3_multi_class() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_multi_class.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse S3 object with multiple classes")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            // Check multiple classes (inheritance)
            assert_eq!(class.len(), 2);
            assert_eq!(class[0].as_ref(), "special_class");
            assert_eq!(class[1].as_ref(), "base_class");

            // Check the base object
            match base.as_ref() {
                RObject::List(elements) => {
                    assert_eq!(elements.len(), 2);
                }
                _ => panic!("Expected List as base object"),
            }
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

#[test]
fn test_s3_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse S3 object on vector")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;
            let attributes = &s3_data.attributes;

            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0].as_ref(), "custom_vector");

            // Check the base object is a vector
            match base.as_ref() {
                RObject::Real(vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 10.0);
                    assert_eq!(vec[1], 20.0);
                    assert_eq!(vec[2], 30.0);
                }
                _ => panic!("Expected Real vector as base object"),
            }

            // Check for the description attribute
            match attributes.get("description") {
                Some(RObject::Character(desc)) => {
                    assert_eq!(desc.len(), 1);
                    assert_eq!(desc[0].as_ref(), "A custom vector class");
                }
                _ => panic!("Expected description attribute"),
            }
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

// =============================================================================
// S3 Object Roundtrip Tests
// =============================================================================

#[test]
fn test_s3_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing S3 object")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write S3 object");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized S3 object")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_s3_multi_class_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_multi_class.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing S3 multi-class object")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write S3 multi-class object");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized S3 multi-class object")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_s3_vector_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing S3 vector object")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write S3 vector object");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized S3 vector object")
        .object;

    assert_eq!(obj, deserialized);
}
