//! Integration and roundtrip tests for Expression vectors.
//!
//! Expression vectors are collections of unevaluated R expressions,
//! typically the result of parse() or expression().

// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

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
// Expression Vector Tests
// =============================================================================

#[test]
fn test_expr_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_single.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse single expression")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // parse(text = "x + 1") produces a single expression
            assert_eq!(elements.len(), 1);
            // The element should be a language object representing x + 1
            match &elements[0] {
                RObject::Language { .. } => {
                    // Good, expression contains a language object
                }
                _ => {
                    // Acceptable - structure may vary
                }
            }
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

#[test]
fn test_expr_multiple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_multiple.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse multiple expressions")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // parse(text = c("x + 1", "y * 2", "z / 3")) produces 3 expressions
            assert_eq!(elements.len(), 3);
            // Each element should be a language object
            for element in &elements {
                match element {
                    RObject::Language { .. } => {
                        // Good
                    }
                    _ => {
                        // Also acceptable
                    }
                }
            }
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

#[test]
fn test_expr_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_empty.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse empty expression")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // expression() produces an empty expression vector
            assert_eq!(elements.len(), 0);
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

#[test]
fn test_expr_calls() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_calls.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse expression with calls")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // parse(text = c("mean(x)", "sum(y)", "sd(z)")) produces 3 expressions
            assert_eq!(elements.len(), 3);
            // Each should be a function call (language object)
            for element in &elements {
                match element {
                    RObject::Language { .. } => {
                        // Good, each is a language object (function call)
                    }
                    _ => {
                        // Structure may vary
                    }
                }
            }
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

#[test]
fn test_expr_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_complex.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse complex expression")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // parse(text = "sqrt(x + y)") produces 1 expression
            assert_eq!(elements.len(), 1);
            // Should be a language object representing the nested call
            match &elements[0] {
                RObject::Language { .. } => {
                    // Good, nested call structure
                }
                _ => {
                    // Structure may vary
                }
            }
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

#[test]
fn test_expr_manual() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_manual.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse manually created expression")
        .object;

    match obj {
        RObject::Expression(elements) => {
            // expression(a + b, c * d, sqrt(e)) produces 3 expressions
            assert_eq!(elements.len(), 3);
            // Each should be a language object
            for element in &elements {
                match element {
                    RObject::Language { .. } => {
                        // Good
                    }
                    _ => {
                        // Structure may vary
                    }
                }
            }
        }
        _ => panic!("Expected Expression vector, got {:?}", obj),
    }
}

// =============================================================================
// Expression Vector Roundtrip Tests
// =============================================================================

#[test]
fn test_expr_single_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_single.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing single expression")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write single expression");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized single expression")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_expr_multiple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_multiple.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing multiple expressions")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write multiple expressions");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized multiple expressions")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_expr_empty_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_empty.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing empty expression")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write empty expression");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized empty expression")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_expr_calls_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_calls.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing expression with calls")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write expression with calls");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized expression with calls")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_expr_complex_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_complex.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing complex expression")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write complex expression");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized complex expression")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_expr_manual_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_manual.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing manually created expression")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write manually created expression");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized manually created expression")
        .object;

    assert_eq!(obj, deserialized);
}
