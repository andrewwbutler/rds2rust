//! Integration tests for namespace serialization.
//!
//! These tests verify that:
//! 1. Namespace references are correctly serialized (type 249)
//! 2. R can read the files and auto-load namespaces
//! 3. S4 method dispatch works after loading

use rds2rust::{read_rds, write_rds, RObject};
use std::fs;
use std::process::Command;
use std::sync::Arc;

/// Helper to run R code and check the result
fn run_r_code(code: &str) -> Result<String, String> {
    let output = Command::new("Rscript")
        .arg("-e")
        .arg(code)
        .output()
        .map_err(|e| format!("Failed to run Rscript: {}", e))?;

    // Combine stdout and stderr (R prints some messages to stderr even on success)
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}{}", stdout, stderr);

    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

/// Check if R and Matrix package are available
fn r_and_matrix_available() -> bool {
    run_r_code("library(Matrix); cat('ok')").map(|s| s.contains("ok")).unwrap_or(false)
}

#[test]
fn test_namespace_triggers_package_loading() {
    if !r_and_matrix_available() {
        eprintln!("Skipping test: R or Matrix package not available");
        return;
    }

    // Create a namespace reference
    let namespace = RObject::Namespace(vec![Arc::from("Matrix")]);

    // Serialize
    let serialized = write_rds(&namespace).expect("Failed to serialize");
    let tmp_file = "/tmp/rds2rust_ns_test.rds";
    fs::write(tmp_file, &serialized).expect("Failed to write file");

    // Read in R and check if Matrix namespace is loaded
    let r_code = format!(
        r#"
        # Record namespaces before loading
        before <- loadedNamespaces()

        # Read the file
        obj <- readRDS("{}")

        # Record namespaces after loading
        after <- loadedNamespaces()

        # Check if Matrix was loaded
        if ("Matrix" %in% after && !("Matrix" %in% before)) {{
            cat("PASS: Matrix namespace was auto-loaded\n")
        }} else if ("Matrix" %in% after) {{
            cat("PASS: Matrix namespace is loaded (was already loaded)\n")
        }} else {{
            cat("FAIL: Matrix namespace was not loaded\n")
            quit(status = 1)
        }}
        "#,
        tmp_file
    );

    let result = run_r_code(&r_code);
    assert!(result.is_ok(), "R script failed: {:?}", result.err());
    assert!(
        result.unwrap().contains("PASS"),
        "Namespace was not loaded by R"
    );

    // Cleanup
    let _ = fs::remove_file(tmp_file);
}

#[test]
fn test_closure_with_namespace_env_loads_package() {
    if !r_and_matrix_available() {
        eprintln!("Skipping test: R or Matrix package not available");
        return;
    }

    // First, create a closure in R that's defined in the Matrix namespace
    let setup_code = r#"
        library(Matrix)
        f <- evalq(function(x) sparseMatrix(i=1, j=1, x=x), envir = asNamespace("Matrix"))
        saveRDS(f, "/tmp/rds2rust_closure_test_input.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup_code);
    assert!(
        result.is_ok() && result.unwrap().contains("ok"),
        "Failed to create test closure in R"
    );

    // Read, roundtrip through Rust, write back
    let data = fs::read("/tmp/rds2rust_closure_test_input.rds").expect("Failed to read input");
    let obj = read_rds(&data).expect("Failed to parse");
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_closure_test_output.rds", &output).expect("Failed to write output");

    // Read in fresh R session and verify namespace loads
    let verify_code = r#"
        # Start fresh - unload Matrix if loaded
        if ("Matrix" %in% loadedNamespaces()) {
            # Can't easily unload, so just note it
            cat("NOTE: Matrix already loaded\n")
        }

        # Read the roundtripped file
        f <- readRDS("/tmp/rds2rust_closure_test_output.rds")

        # Check if Matrix namespace is now loaded
        if ("Matrix" %in% loadedNamespaces()) {
            cat("PASS: Matrix namespace loaded\n")
        } else {
            cat("FAIL: Matrix namespace not loaded\n")
            quit(status = 1)
        }

        # Try to use the function
        tryCatch({
            result <- f(5.0)
            cat("PASS: Function executed successfully\n")
        }, error = function(e) {
            cat("FAIL: Function execution failed:", e$message, "\n")
            quit(status = 1)
        })
    "#;

    let result = run_r_code(verify_code);
    assert!(result.is_ok(), "R verification failed: {:?}", result.err());
    let output = result.unwrap();
    assert!(
        output.contains("PASS: Matrix namespace loaded"),
        "Matrix namespace not loaded: {}",
        output
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_closure_test_input.rds");
    let _ = fs::remove_file("/tmp/rds2rust_closure_test_output.rds");
}

#[test]
fn test_s4_object_method_dispatch() {
    if !r_and_matrix_available() {
        eprintln!("Skipping test: R or Matrix package not available");
        return;
    }

    // Create a Matrix object in R
    let setup_code = r#"
        library(Matrix)
        m <- sparseMatrix(i=c(1,2,3), j=c(1,2,3), x=c(1.0, 2.0, 3.0), dims=c(3, 3))
        saveRDS(m, "/tmp/rds2rust_s4_test_input.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup_code);
    assert!(
        result.is_ok() && result.unwrap().contains("ok"),
        "Failed to create test Matrix in R"
    );

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_s4_test_input.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse");
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_s4_test_output.rds", &output).expect("Failed to write");

    // Verify S4 method dispatch works
    let verify_code = r#"
        # Load Matrix package first - R doesn't auto-load package methods
        # from S4 object class info (the package attribute is just metadata)
        library(Matrix)

        m <- readRDS("/tmp/rds2rust_s4_test_output.rds")

        # Check basic properties
        if (!isS4(m)) {
            cat("FAIL: Object is not S4\n")
            quit(status = 1)
        }
        cat("PASS: Object is S4\n")

        # Check class
        if (class(m) != "dgCMatrix") {
            cat("FAIL: Wrong class:", class(m), "\n")
            quit(status = 1)
        }
        cat("PASS: Class is dgCMatrix\n")

        # Check dim() method dispatch
        d <- dim(m)
        if (is.null(d) || !all(d == c(3, 3))) {
            cat("FAIL: dim() returned wrong value:", d, "\n")
            quit(status = 1)
        }
        cat("PASS: dim() returns c(3, 3)\n")

        # Check nrow/ncol
        if (nrow(m) != 3 || ncol(m) != 3) {
            cat("FAIL: nrow/ncol wrong\n")
            quit(status = 1)
        }
        cat("PASS: nrow() and ncol() work\n")

        # Check sum (uses S4 method)
        if (sum(m) != 6) {
            cat("FAIL: sum() returned wrong value\n")
            quit(status = 1)
        }
        cat("PASS: sum() returns 6\n")

        # Check slot access
        if (length(m@x) != 3) {
            cat("FAIL: Slot access failed\n")
            quit(status = 1)
        }
        cat("PASS: Slot access works\n")
    "#;

    let result = run_r_code(verify_code);
    assert!(result.is_ok(), "R script failed: {:?}", result.err());
    let output = result.unwrap();

    // Check all tests passed (the output may contain "Loading required package" which is fine)
    let pass_count = output.matches("PASS:").count();
    let fail_count = output.matches("FAIL:").count();
    assert_eq!(
        fail_count, 0,
        "Some S4 tests failed. Output:\n{}",
        output
    );
    assert!(
        pass_count >= 6,
        "Not all S4 tests passed (expected 6, got {}). Output:\n{}",
        pass_count,
        output
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_s4_test_input.rds");
    let _ = fs::remove_file("/tmp/rds2rust_s4_test_output.rds");
}

#[test]
fn test_multiple_namespace_references() {
    if !r_and_matrix_available() {
        eprintln!("Skipping test: R or Matrix package not available");
        return;
    }

    // Create a list with multiple namespace references (same namespace)
    // This tests that reference tracking works correctly
    let namespace = RObject::Namespace(vec![Arc::from("Matrix")]);
    let list = RObject::List(vec![
        namespace.clone(),
        RObject::Integer(vec![1, 2, 3]),
        namespace.clone(), // Second reference should use REFSXP
    ]);

    let serialized = write_rds(&list).expect("Failed to serialize");
    let tmp_file = "/tmp/rds2rust_multi_ns_test.rds";
    fs::write(tmp_file, &serialized).expect("Failed to write");

    // Verify R can read it
    let verify_code = format!(
        r#"
        obj <- readRDS("{}")

        if (!is.list(obj) || length(obj) != 3) {{
            cat("FAIL: Wrong structure\n")
            quit(status = 1)
        }}

        if ("Matrix" %in% loadedNamespaces()) {{
            cat("PASS: Matrix namespace loaded\n")
        }} else {{
            cat("FAIL: Matrix namespace not loaded\n")
            quit(status = 1)
        }}

        # Check the integer vector is intact
        if (!identical(obj[[2]], c(1L, 2L, 3L))) {{
            cat("FAIL: Integer vector corrupted\n")
            quit(status = 1)
        }}
        cat("PASS: List structure intact\n")
        "#,
        tmp_file
    );

    let result = run_r_code(&verify_code);
    assert!(result.is_ok(), "R verification failed: {:?}", result.err());
    assert!(
        result.unwrap().contains("PASS: List structure intact"),
        "Test failed"
    );

    // Cleanup
    let _ = fs::remove_file(tmp_file);
}

#[test]
fn test_different_namespaces_in_list() {
    // Test that multiple different namespaces can be in the same object
    if !r_and_matrix_available() {
        eprintln!("Skipping test: R or Matrix package not available");
        return;
    }

    // Create a list with two different namespaces
    let list = RObject::List(vec![
        RObject::Namespace(vec![Arc::from("Matrix")]),
        RObject::Integer(vec![42]),
        RObject::Namespace(vec![Arc::from("stats")]),
    ]);

    let serialized = write_rds(&list).expect("Failed to serialize");
    let tmp_file = "/tmp/rds2rust_diff_ns_test.rds";
    fs::write(tmp_file, &serialized).expect("Failed to write");

    // Verify R can read it and both namespaces are loaded
    let verify_code = format!(
        r#"
        obj <- readRDS("{}")

        if (!is.list(obj) || length(obj) != 3) {{
            cat("FAIL: Wrong structure\n")
            quit(status = 1)
        }}

        # Check both namespaces are loaded
        if (!("Matrix" %in% loadedNamespaces())) {{
            cat("FAIL: Matrix namespace not loaded\n")
            quit(status = 1)
        }}
        cat("PASS: Matrix namespace loaded\n")

        if (!("stats" %in% loadedNamespaces())) {{
            cat("FAIL: stats namespace not loaded\n")
            quit(status = 1)
        }}
        cat("PASS: stats namespace loaded\n")

        # Check the integer is intact
        if (!identical(obj[[2]], 42L)) {{
            cat("FAIL: Integer corrupted\n")
            quit(status = 1)
        }}
        cat("PASS: List structure intact\n")
        "#,
        tmp_file
    );

    let result = run_r_code(&verify_code);
    assert!(result.is_ok(), "R verification failed: {:?}", result.err());
    let output = result.unwrap();
    assert!(
        output.contains("PASS: List structure intact"),
        "Test failed: {}",
        output
    );

    // Cleanup
    let _ = fs::remove_file(tmp_file);
}
