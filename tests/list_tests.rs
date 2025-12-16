//! Integration and roundtrip tests for List and Pairlist types.

use rds2rust::{read_rds, write_rds, RObject};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

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

// =============================================================================
// List Tests
// =============================================================================

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

#[test]
fn test_list_roundtrip() {
    let obj = RObject::List(vec![
        RObject::Integer(vec![1, 2, 3].into()),
        RObject::Character(vec![Arc::from("test")].into()),
        RObject::Real(vec![4.5].into()),
    ]);
    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read list");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_list_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing list");

    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized list");

    assert_eq!(obj, deserialized);
}

/// Test that named lists preserve their names as character vectors.
/// This was broken when single-element Character was incorrectly converted to SYMSXP.
#[test]
fn test_named_list_names_preserved() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    // Create a named list in R
    let setup = r#"
        x <- list(alpha = 1, beta = 2, gamma = "test")
        saveRDS(x, "/tmp/rds2rust_named_list_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_named_list_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse");
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_named_list_regression_out.rds", &output).expect("Failed to write");

    // Verify in R
    let verify = r#"
        x <- readRDS("/tmp/rds2rust_named_list_regression_out.rds")

        # Check names are preserved
        if (!identical(names(x), c("alpha", "beta", "gamma"))) {
            cat("FAIL: names not preserved\n")
            cat("Got:", paste(names(x), collapse=", "), "\n")
            quit(status = 1)
        }

        # Check values are correct
        if (x$alpha != 1 || x$beta != 2 || x$gamma != "test") {
            cat("FAIL: values incorrect\n")
            quit(status = 1)
        }

        # Check that names() returns a character vector, not symbols
        if (!is.character(names(x))) {
            cat("FAIL: names are not character vector\n")
            quit(status = 1)
        }

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Named list verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_named_list_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_named_list_regression_out.rds");
}
