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

#[test]
fn test_character_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse character vector");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], "foo");
            assert_eq!(vec[1], "bar");
            assert_eq!(vec[2], "baz");
        }
        other => panic!("Expected Character vector, got {:?}", other),
    }
}

#[test]
fn test_character_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_with_na.rds");
    let obj = read_rds(&data).expect("Failed to parse character with NA");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], "test");
            assert_eq!(vec[1], "NA"); // NA_character_ is currently parsed as "NA"
            assert_eq!(vec[2], "string");
        }
        other => panic!("Expected Character vector with NA, got {:?}", other),
    }
}

#[test]
fn test_character_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_empty.rds");
    let obj = read_rds(&data).expect("Failed to parse empty character vector");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 0);
        }
        other => panic!("Expected empty Character vector, got {:?}", other),
    }
}

#[test]
fn test_logical_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse logical vector");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], rds2rust::Logical::True);
            assert_eq!(vec[1], rds2rust::Logical::False);
            assert_eq!(vec[2], rds2rust::Logical::Na);
            assert_eq!(vec[3], rds2rust::Logical::True);
        }
        other => panic!("Expected Logical vector, got {:?}", other),
    }
}

#[test]
fn test_logical_false() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_false.rds");
    let obj = read_rds(&data).expect("Failed to parse logical FALSE");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], rds2rust::Logical::False);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_real_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse real vector");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], 1.1);
            assert_eq!(vec[1], 2.2);
            assert_eq!(vec[2], 3.3);
            assert_eq!(vec[3], 4.4);
        }
        other => panic!("Expected Real vector, got {:?}", other),
    }
}

#[test]
fn test_real_special() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_special.rds");
    let obj = read_rds(&data).expect("Failed to parse real with special values");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 5);
            assert_eq!(vec[0], 1.5);
            // vec[1] is NA_real_ (a specific NaN bit pattern)
            assert!(vec[1].is_nan());
            assert_eq!(vec[2], f64::INFINITY);
            assert_eq!(vec[3], f64::NEG_INFINITY);
            // vec[4] is NaN
            assert!(vec[4].is_nan());
        }
        other => panic!("Expected Real vector with special values, got {:?}", other),
    }
}

#[test]
fn test_integer_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_with_na.rds");
    let obj = read_rds(&data).expect("Failed to parse integer with NA");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 1);
            assert_eq!(vec[1], i32::MIN); // NA_integer_ is i32::MIN
            assert_eq!(vec[2], 3);
        }
        other => panic!("Expected Integer vector with NA, got {:?}", other),
    }
}

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
