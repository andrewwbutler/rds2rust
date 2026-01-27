//! Integration and roundtrip tests for DataFrame types.

use rds2rust::{read_rds, write_rds, Logical, RObject};
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
// DataFrame Tests
// =============================================================================

#[test]
fn test_dataframe_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse simple data frame")
        .object;

    match obj {
        RObject::DataFrame(data) => {
            let columns = &data.columns;
            let row_names = &data.row_names;

            // Check that we have 3 columns
            assert_eq!(columns.len(), 3);

            // Check column "x" (integers 1, 2, 3)
            match columns.get(&Arc::from("x")) {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec, &vec![1, 2, 3]);
                }
                _ => panic!("Expected integer column 'x'"),
            }

            // Check column "y" (characters "a", "b", "c")
            match columns.get(&Arc::from("y")) {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0].as_ref(), "a");
                    assert_eq!(vec[1].as_ref(), "b");
                    assert_eq!(vec[2].as_ref(), "c");
                }
                _ => panic!("Expected character column 'y'"),
            }

            // Check column "z" (logicals TRUE, FALSE, TRUE)
            match columns.get(&Arc::from("z")) {
                Some(RObject::Logical(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], Logical::True);
                    assert_eq!(vec[1], Logical::False);
                    assert_eq!(vec[2], Logical::True);
                }
                _ => panic!("Expected logical column 'z'"),
            }

            // Check row names (default: "1", "2", "3")
            assert_eq!(row_names.len(), 3);
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}

#[test]
fn test_dataframe_mixed() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_mixed.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse mixed data frame")
        .object;

    match obj {
        RObject::DataFrame(data) => {
            let columns = &data.columns;

            // Check that we have 4 columns
            assert_eq!(columns.len(), 4);

            // Verify int_col
            match columns.get(&Arc::from("int_col")) {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec, &vec![1, 2, 3, 4]);
                }
                _ => panic!("Expected integer column 'int_col'"),
            }

            // Verify real_col
            match columns.get(&Arc::from("real_col")) {
                Some(RObject::Real(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0], 1.1);
                    assert_eq!(vec[3], 4.4);
                }
                _ => panic!("Expected real column 'real_col'"),
            }

            // Verify char_col
            match columns.get(&Arc::from("char_col")) {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0].as_ref(), "foo");
                    assert_eq!(vec[3].as_ref(), "qux");
                }
                _ => panic!("Expected character column 'char_col'"),
            }

            // Verify logical_col
            match columns.get(&Arc::from("logical_col")) {
                Some(RObject::Logical(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0], Logical::True);
                    assert_eq!(vec[1], Logical::False);
                }
                _ => panic!("Expected logical column 'logical_col'"),
            }
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}

#[test]
fn test_dataframe_rownames() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_rownames.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse data frame with row names")
        .object;

    match obj {
        RObject::DataFrame(data) => {
            let columns = &data.columns;
            let row_names = &data.row_names;

            // Check columns
            assert_eq!(columns.len(), 2);

            // Check row names
            assert_eq!(row_names.len(), 3);
            assert_eq!(row_names[0].as_ref(), "person1");
            assert_eq!(row_names[1].as_ref(), "person2");
            assert_eq!(row_names[2].as_ref(), "person3");

            // Verify data
            match columns.get(&Arc::from("name")) {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec[0].as_ref(), "Alice");
                    assert_eq!(vec[1].as_ref(), "Bob");
                    assert_eq!(vec[2].as_ref(), "Charlie");
                }
                _ => panic!("Expected character column 'name'"),
            }

            match columns.get(&Arc::from("age")) {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec, &vec![25, 30, 35]);
                }
                _ => panic!("Expected integer column 'age'"),
            }
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}

// =============================================================================
// DataFrame Roundtrip Tests
// =============================================================================

#[test]
fn test_dataframe_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing dataframe")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write dataframe");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized dataframe")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_dataframe_mixed_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_mixed.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing mixed dataframe")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write mixed dataframe");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized mixed dataframe")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_dataframe_rownames_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_rownames.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing dataframe with rownames")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write dataframe with rownames");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized dataframe with rownames")
        .object;

    assert_eq!(obj, deserialized);
}
