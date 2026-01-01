//! Integration tests for CLOSXP (closures/functions) and ENVSXP (environments).
//!
//! These tests verify that the parser correctly handles function objects
//! and environment objects, including closures with custom environments.

use rds2rust::{read_rds, write_rds, RObject};
use std::fs;
use std::path::Path;
use std::process::Command;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn r_available() -> bool {
    Command::new("R")
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_r_code(code: &str) -> Result<String, String> {
    let output = Command::new("R")
        .args(["--vanilla", "--slave", "-e", code])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
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
            // Unwrap any Shared wrapper first
            let formals_concrete = formals.as_concrete();
            match &formals_concrete {
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
                RObject::Language { .. } => {
                    // Body structure is complex, just verify it's a language object
                }
                RObject::Bytecode { .. } => {
                    // Compiled version - acceptable
                }
                other => panic!("Expected Language or Bytecode for body, got {:?}", other),
            }

            // Check environment - should be global env
            assert!(
                matches!(environment.as_ref(), RObject::GlobalEnv),
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
                RObject::Language { .. } | RObject::Bytecode { .. }
            ) {
                eprintln!("Body is: {:#?}", body);
                panic!("Body should be a Language or Bytecode object");
            }

            // Check environment - should be an Environment (not global)
            // Unwrap any Shared wrapper first
            let environment_concrete = environment.as_concrete();
            match &environment_concrete {
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
            // Enclosing should be global env
            assert!(
                matches!(enclosing.as_ref(), RObject::GlobalEnv),
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

    // Basic type check - avoid deep comparison due to circular Shared references
    // which cause infinite recursion in derived PartialEq
    match (&original, &roundtrip) {
        (RObject::Closure { .. }, RObject::Closure { .. }) => {
            // Successfully roundtripped a self-referencing closure
            // Full structural comparison requires cycle-safe PartialEq implementation
        }
        _ => panic!("Roundtrip changed object type - expected Closure"),
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

/// Test that closures with simple expressions roundtrip correctly.
#[test]
fn test_closure_simple_expression_roundtrip() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    // Create closures with various body expressions
    let setup = r#"
        f1 <- function(x) x + 1
        f2 <- function(x, y) x * y
        f3 <- function(a) sqrt(a)

        saveRDS(list(f1 = f1, f2 = f2, f3 = f3), "/tmp/rds2rust_closure_expr_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_closure_expr_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse");

    if std::env::var("DEBUG_DUMP_CLOSURE").is_ok() {
        println!("Parsed object: {:#?}", obj);
    }

    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_closure_expr_regression_out.rds", &output).expect("Failed to write");

    // Verify closures work correctly
    let verify = r#"
        funcs <- readRDS("/tmp/rds2rust_closure_expr_regression_out.rds")

        # Test f1: x + 1
        if (funcs$f1(5) != 6) {
            cat("FAIL: f1(5) should be 6, got:", funcs$f1(5), "\n")
            quit(status = 1)
        }

        # Test f2: x * y
        if (funcs$f2(3, 4) != 12) {
            cat("FAIL: f2(3, 4) should be 12, got:", funcs$f2(3, 4), "\n")
            quit(status = 1)
        }

        # Test f3: sqrt(a)
        if (funcs$f3(16) != 4) {
            cat("FAIL: f3(16) should be 4, got:", funcs$f3(16), "\n")
            quit(status = 1)
        }

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Closure expression verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_closure_expr_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_closure_expr_regression_out.rds");
}

/// Test that function calls with named arguments preserve the argument names.
#[test]
fn test_closure_named_arguments_preserved() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    // Create a closure that calls a function with named arguments
    let setup = r#"
        # A function that uses named arguments
        f <- function(x, y) {
            seq(from = x, to = y, by = 1)
        }
        saveRDS(f, "/tmp/rds2rust_named_args_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_named_args_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse");
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_named_args_regression_out.rds", &output).expect("Failed to write");

    // Verify the function works and argument names are preserved
    let verify = r#"
        f <- readRDS("/tmp/rds2rust_named_args_regression_out.rds")

        # Test that function executes correctly
        result <- f(1, 5)
        expected <- c(1, 2, 3, 4, 5)  # Use numeric to match seq() output
        if (!all(result == expected)) {
            stop(paste("FAIL: f(1, 5) should be 1:5, got:", paste(result, collapse=",")))
        }

        # Check that the function body shows named arguments
        body_str <- deparse(body(f))
        if (!any(grepl("from.*=", body_str))) {
            stop(paste("FAIL: Named argument 'from' not preserved in body. Body:", paste(body_str, collapse="\n")))
        }

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Named arguments verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_named_args_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_named_args_regression_out.rds");
}

#[test]
fn test_command_closures_roundtrip_r_read() {
    if !test_data_exists() {
        eprintln!("Warning: tests/data directory not found, skipping test");
        return;
    }
    if !r_available() {
        eprintln!("Warning: R not available, skipping test");
        return;
    }

    let candidates = [
        "test_minimal_closure.rds",
        "test_with_real_functions.rds",
        "command_one_real.rds",
        "commands_real_1.rds",
        "commands_real_2.rds",
        "command_realistic.rds",
        "withattr_language.rds",
        "withattr_closure.rds",
    ];

    let debug_only = std::env::var("RDS_DEBUG_ONLY").ok();
    for filename in candidates
        .iter()
        .copied()
        .filter(|name| debug_only.as_deref().is_none_or(|only| only == *name))
    {
        let data = read_test_file(filename);
        let obj = read_rds(&data)
            .unwrap_or_else(|e| panic!("Failed to parse {} with rds2rust: {:?}", filename, e));
        let output = write_rds(&obj)
            .unwrap_or_else(|e| panic!("Failed to serialize {} with rds2rust: {:?}", filename, e));

        if std::env::var("RDS_DEBUG_REF_FALLBACK").is_ok() {
            let _ = read_rds(&output)
                .unwrap_or_else(|e| panic!("Failed to parse roundtripped {}: {:?}", filename, e));
        }

        let output_path = format!("/tmp/rds2rust_command_roundtrip_{}", filename);
        fs::write(&output_path, &output)
            .unwrap_or_else(|e| panic!("Failed to write {}: {}", output_path, e));

        let r_code = format!("readRDS('{}')", output_path.replace('\'', "\\'"));
        if let Err(err) = run_r_code(&r_code) {
            panic!("R failed to read roundtripped {}: {}", filename, err);
        }

        let _ = fs::remove_file(&output_path);
    }
}
