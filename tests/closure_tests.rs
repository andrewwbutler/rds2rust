//! Integration tests for CLOSXP (closures/functions) and ENVSXP (environments).
//!
//! These tests verify that the parser correctly handles function objects
//! and environment objects, including closures with custom environments.

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

#[test]
fn test_simple_function() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("closure_simple.rds");
    let result = read_rds(&data);
    assert!(
        result.is_ok(),
        "Failed to parse simple function: {:?}",
        result.err()
    );

    let obj = result.unwrap();

    // Verify it's a Closure
    match &obj {
        RObject::Closure {
            formals,
            body,
            environment,
        } => {
            // Check formals - should be a pairlist with x and y (y=10)
            match formals.as_ref() {
                RObject::Pairlist(elements) => {
                    assert_eq!(elements.len(), 2, "Should have 2 parameters");

                    // First parameter: x (no default)
                    assert_eq!(
                        elements[0].tag.as_deref(),
                        Some("x"),
                        "First param should be 'x'"
                    );

                    // Second parameter: y = 10
                    assert_eq!(
                        elements[1].tag.as_deref(),
                        Some("y"),
                        "Second param should be 'y'"
                    );
                    match &elements[1].value {
                        RObject::Real(vec) => {
                            assert_eq!(vec.len(), 1, "Default value should be a scalar");
                            assert_eq!(vec[0], 10.0, "Default value should be 10");
                        }
                        other => panic!("Expected Real default value, got {:?}", other),
                    }
                }
                other => panic!("Expected Pairlist for formals, got {:?}", other),
            }

            // Check body - allow Language or Bytecode (R may compile functions automatically)
            match body.as_ref() {
                RObject::Language(_) => {
                    // Body structure is complex, just verify it's a language object
                }
                RObject::Bytecode { .. } => {
                    // Compiled version - acceptable
                }
                other => panic!("Expected Language or Bytecode for body, got {:?}", other),
            }

            // Check environment - should be global env (NULL)
            assert!(
                matches!(environment.as_ref(), RObject::Null),
                "Simple function should have global environment"
            );
        }
        other => panic!("Expected Closure, got {:?}", other),
    }
}

#[test]
fn test_closure_with_environment() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("closure_with_env.rds");
    let result = read_rds(&data);
    assert!(
        result.is_ok(),
        "Failed to parse closure with environment: {:?}",
        result.err()
    );

    let obj = result.unwrap();

    // Verify it's a Closure
    match &obj {
        RObject::Closure {
            formals,
            body,
            environment,
        } => {
            // Check formals - should be NULL (no parameters)
            match formals.as_ref() {
                RObject::Null => {
                    // Expected
                }
                RObject::Pairlist(elements) if elements.is_empty() => {
                    // Also acceptable
                }
                other => panic!(
                    "Expected Null or empty Pairlist for formals, got {:?}",
                    other
                ),
            }

            // Check body - accept Language or Bytecode
            if !matches!(
                body.as_ref(),
                RObject::Language(_) | RObject::Bytecode { .. }
            ) {
                eprintln!("Body is: {:#?}", body);
                panic!("Body should be a Language or Bytecode object");
            }

            // Check environment - should be an Environment (not global)
            match environment.as_ref() {
                RObject::Environment {
                    enclosing: _,
                    frame,
                    hashtab: _,
                } => {
                    // Verify structure exists
                    // enclosing should not be NULL (it's a closure environment)
                    // frame might contain bindings
                    // hashtab might be NULL or a vector

                    // Basic validation - just ensure types are reasonable
                    assert!(
                        !matches!(frame.as_ref(), RObject::Null),
                        "Frame should not be NULL for closure environment"
                    );
                }
                other => panic!("Expected Environment, got {:?}", other),
            }
        }
        other => panic!("Expected Closure, got {:?}", other),
    }
}

#[test]
fn test_simple_environment() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("environment_simple.rds");
    let result = read_rds(&data);
    assert!(
        result.is_ok(),
        "Failed to parse simple environment: {:?}",
        result.err()
    );

    let obj = result.unwrap();

    // Verify it's an Environment
    match &obj {
        RObject::Environment {
            enclosing,
            frame: _,
            hashtab,
        } => {
            // Enclosing should be global env (NULL)
            assert!(
                matches!(enclosing.as_ref(), RObject::Null),
                "Simple environment should have global as parent"
            );

            // Frame should be a pairlist (might be empty)
            // Hashtab should be a VECSXP or NULL

            // Basic validation
            match hashtab.as_ref() {
                RObject::List(_) => {
                    // Hash table as vector - good
                }
                RObject::Null => {
                    // No hash table - also acceptable for small envs
                }
                other => panic!("Expected List or Null for hashtab, got {:?}", other),
            }
        }
        other => panic!("Expected Environment, got {:?}", other),
    }
}

#[test]
fn test_simple_function_roundtrip() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("closure_simple.rds");
    let original = read_rds(&data).unwrap();

    // Write it back
    let rds_bytes = write_rds(&original).unwrap();

    // Read it back
    let roundtrip = read_rds(&rds_bytes).unwrap();

    // Compare (basic structure comparison)
    match (&original, &roundtrip) {
        (
            RObject::Closure {
                formals: f1,
                body: b1,
                environment: e1,
            },
            RObject::Closure {
                formals: f2,
                body: b2,
                environment: e2,
            },
        ) => {
            assert_eq!(f1, f2, "Formals should match after roundtrip");
            assert_eq!(b1, b2, "Body should match after roundtrip");
            assert_eq!(e1, e2, "Environment should match after roundtrip");
        }
        _ => panic!("Objects don't match types after roundtrip"),
    }
}

#[test]
fn test_environment_roundtrip() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }

    let data = read_test_file("environment_simple.rds");
    let original = read_rds(&data).unwrap();

    // Write it back
    let rds_bytes = write_rds(&original).unwrap();

    // Read it back
    let roundtrip = read_rds(&rds_bytes).unwrap();

    // Compare (basic structure comparison)
    match (&original, &roundtrip) {
        (
            RObject::Environment {
                enclosing: e1,
                frame: f1,
                hashtab: h1,
            },
            RObject::Environment {
                enclosing: e2,
                frame: f2,
                hashtab: h2,
            },
        ) => {
            assert_eq!(e1, e2, "Enclosing should match after roundtrip");
            assert_eq!(f1, f2, "Frame should match after roundtrip");
            assert_eq!(h1, h2, "Hashtab should match after roundtrip");
        }
        _ => panic!("Objects don't match types after roundtrip"),
    }
}
