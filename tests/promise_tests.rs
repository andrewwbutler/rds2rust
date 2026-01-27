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
fn test_promise_in_env() {
    // Fixed: Avoided Debug print which caused infinite recursion on circular references
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("promise_in_env.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse promise in environment")
        .object;

    // The environment should contain a promise
    // Promises are structured as: value, expression, environment
    fn is_env_like(o: &rds2rust::RObject) -> bool {
        matches!(o, rds2rust::RObject::Environment { .. })
            || matches!(o, rds2rust::RObject::WithAttributes { object, .. } if matches!(object.as_ref(), rds2rust::RObject::Environment { .. }))
    }

    match obj {
        rds2rust::RObject::Environment { .. } => {
            println!("Parsed environment with promise (contains circular reference)");
        }
        rds2rust::RObject::WithAttributes { object, .. } if is_env_like(object.as_ref()) => {
            println!("Parsed env wrapped in attributes");
        }
        rds2rust::RObject::Shared(inner) => {
            let guard = inner.read().unwrap();
            assert!(
                is_env_like(&guard),
                "Shared wrapper should wrap Environment, got {:?}",
                std::mem::discriminant(&*guard)
            );
        }
        _ => panic!(
            "Expected Environment, got type: {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_promise_in_env_roundtrip() {
    // Fixed: Avoided Debug comparison which caused infinite recursion
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let original_data = read_test_file("promise_in_env.rds");
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    // Both should be Environment types
    // Note: Can't use Debug comparison due to circular reference causing infinite recursion
    fn env_like(o: &rds2rust::RObject) -> bool {
        match o {
            rds2rust::RObject::Environment { .. } => true,
            rds2rust::RObject::WithAttributes { object, .. } => env_like(object.as_ref()),
            rds2rust::RObject::Shared(inner) => {
                let guard = inner.read().unwrap();
                env_like(&guard)
            }
            _ => false,
        }
    }

    assert!(env_like(&obj));
    assert!(env_like(&obj2));
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
    let obj = read_rds(&data)
        .expect("Failed to parse special function 'if'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_for() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_for.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse special function 'for'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_while() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_while.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse special function 'while'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_function() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_function.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse special function 'function'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_special_bracket() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("special_bracket.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse special function '['")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_plus() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_plus.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function '+'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

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
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function 'sum'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_c() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_c.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function 'c'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_sqrt() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_sqrt.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function 'sqrt'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_length() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_length.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function 'length'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}

#[test]
fn test_builtin_min() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("builtin_min.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse builtin function 'min'")
        .object;

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
    let obj = read_rds(&original_data)
        .expect("Failed to parse original")
        .object;

    let rewritten_data = write_rds(&obj).expect("Failed to write");
    let obj2 = read_rds(&rewritten_data)
        .expect("Failed to parse rewritten")
        .object;

    assert_eq!(format!("{:?}", obj), format!("{:?}", obj2));
}
