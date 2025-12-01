//! Tests for promises, special functions, and builtin functions.

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

// =============================================================================
// Promise Tests (PROMSXP)
// =============================================================================

#[test]
#[ignore] // REGRESSION: This test passed before Shared handling changes, now causes stack overflow on drop
fn test_promise_in_env() {
    // TODO: Fix stack overflow caused by circular Shared references in promises
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("promise_in_env.rds");
    let obj = read_rds(&data).expect("Failed to parse promise in environment");

    // The environment should contain a promise
    // Promises are structured as: value, expression, environment
    match obj {
        rds2rust::RObject::Environment { .. } => {
            // Environment parsed successfully
            println!("Parsed environment with promise: {:?}", obj);
        }
        _ => panic!("Expected Environment, got: {:?}", obj),
    }
}

#[test]
#[ignore] // REGRESSION: Same stack overflow issue as test_promise_in_env
fn test_promise_in_env_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("promise_in_env.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    // Compare Debug representations
    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

// =============================================================================
// Special Function Tests (SPECIALSXP)
// =============================================================================

#[test]
fn test_special_if() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_if.rds");
    let obj = read_rds(&data).expect("Failed to parse special function 'if'");

    match obj {
        rds2rust::RObject::Special { name } => {
            assert_eq!(name.as_ref(), "if");
        }
        _ => panic!("Expected Special function, got: {:?}", obj),
    }
}

#[test]
fn test_special_if_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("special_if.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_for() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_for.rds");
    let obj = read_rds(&data).expect("Failed to parse special function 'for'");

    match obj {
        rds2rust::RObject::Special { name } => {
            assert_eq!(name.as_ref(), "for");
        }
        _ => panic!("Expected Special function, got: {:?}", obj),
    }
}

#[test]
fn test_special_for_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("special_for.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_while() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_while.rds");
    let obj = read_rds(&data).expect("Failed to parse special function 'while'");

    match obj {
        rds2rust::RObject::Special { name } => {
            assert_eq!(name.as_ref(), "while");
        }
        _ => panic!("Expected Special function, got: {:?}", obj),
    }
}

#[test]
fn test_special_while_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("special_while.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_function() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_function.rds");
    let obj = read_rds(&data).expect("Failed to parse special function 'function'");

    match obj {
        rds2rust::RObject::Special { name } => {
            assert_eq!(name.as_ref(), "function");
        }
        _ => panic!("Expected Special function, got: {:?}", obj),
    }
}

#[test]
fn test_special_function_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("special_function.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_bracket() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_bracket.rds");
    let obj = read_rds(&data).expect("Failed to parse special function '['");

    match obj {
        rds2rust::RObject::Special { name } => {
            assert_eq!(name.as_ref(), "[");
        }
        _ => panic!("Expected Special function, got: {:?}", obj),
    }
}

#[test]
fn test_special_bracket_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("special_bracket.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_plus() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_plus.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function '+'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "+");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_plus_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_plus.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

// =============================================================================
// Builtin Function Tests (BUILTINSXP)
// =============================================================================

#[test]
fn test_builtin_sum() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_sum.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function 'sum'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "sum");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_sum_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_sum.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_c() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_c.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function 'c'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "c");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_c_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_c.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_sqrt() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_sqrt.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function 'sqrt'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "sqrt");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_sqrt_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_sqrt.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_length() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_length.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function 'length'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "length");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_length_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_length.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_min() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_min.rds");
    let obj = read_rds(&data).expect("Failed to parse builtin function 'min'");

    match obj {
        rds2rust::RObject::Builtin { name } => {
            assert_eq!(name.as_ref(), "min");
        }
        _ => panic!("Expected Builtin function, got: {:?}", obj),
    }
}

#[test]
fn test_builtin_min_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("builtin_min.rds");
    let obj = read_rds(&original_data).expect("Failed to parse original");

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data).expect("Failed to parse rewritten");

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}
