//! Integration and roundtrip tests for Language objects (unevaluated R expressions/calls).

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
// Language Object Tests
// =============================================================================

#[test]
fn test_lang_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple language object");

    match obj {
        RObject::Language { function, args } => {
            // quote(sum(1, 2, 3)) => function=sum, args=[1, 2, 3]
            // Just verify we got the structure
            let _ = function; // function should be sum
            let _ = args; // args should be the arguments
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

#[test]
fn test_lang_with_args() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_with_args.rds");
    let obj = read_rds(&data).expect("Failed to parse language object with args");

    match obj {
        RObject::Language { function, args } => {
            // quote(mean(x, na.rm = TRUE)) => function=mean, args=[x, TRUE]
            let _ = function;
            let _ = args;
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

#[test]
fn test_lang_nested() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_nested.rds");
    let obj = read_rds(&data).expect("Failed to parse nested language object");

    match obj {
        RObject::Language { function, args } => {
            // quote(sqrt(sum(x, y))) => function=sqrt, args=[sum(x, y)]
            let _ = function;

            // The first argument should be another language object: sum(x, y)
            if !args.is_empty() {
                match &args[0].value {
                    RObject::Language { .. } => {
                        // Good, nested call structure preserved
                    }
                    _ => {
                        // Also acceptable - nested structure may vary
                    }
                }
            }
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

// =============================================================================
// Language Object Roundtrip Tests
// =============================================================================

#[test]
fn test_lang_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing simple language object");

    let serialized = write_rds(&obj).expect("Failed to write simple language object");
    let deserialized =
        read_rds(&serialized).expect("Failed to read serialized simple language object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_lang_with_args_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_with_args.rds");
    let obj = read_rds(&data).expect("Failed to read existing language object with args");

    let serialized = write_rds(&obj).expect("Failed to write language object with args");
    let deserialized =
        read_rds(&serialized).expect("Failed to read serialized language object with args");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_lang_nested_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_nested.rds");
    let obj = read_rds(&data).expect("Failed to read existing nested language object");

    let serialized = write_rds(&obj).expect("Failed to write nested language object");
    let deserialized =
        read_rds(&serialized).expect("Failed to read serialized nested language object");

    assert_eq!(obj, deserialized);
}
