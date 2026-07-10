//! Integration and roundtrip tests for Formula objects.
//!
//! Formulas are S3 objects with class="formula" and a Language base.

// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

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

// =============================================================================
// Formula Tests
// =============================================================================

#[test]
fn test_formula_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse simple formula")
        .object;

    // Formulas are language objects with class="formula"
    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0].as_ref(), "formula");

            // The base should be a language object representing y ~ x
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, formula is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

#[test]
fn test_formula_multiple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_multiple.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse formula with multiple predictors")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, y ~ x + z is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

#[test]
fn test_formula_interaction() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_interaction.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse formula with interaction")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, y ~ x * z is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

#[test]
fn test_formula_functions() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_functions.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse formula with functions")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, log(y) ~ sqrt(x) + I(z^2) is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

#[test]
fn test_formula_no_intercept() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_no_intercept.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse formula without intercept")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, y ~ x - 1 is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

#[test]
fn test_formula_one_sided() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_one_sided.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse one-sided formula")
        .object;

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language { .. } => {
                    // Good, ~ x + y is a language object
                }
                _ => panic!("Expected Language object as formula base"),
            }
        }
        _ => panic!("Expected S3Object (formula), got {:?}", obj),
    }
}

// =============================================================================
// Formula Roundtrip Tests
// =============================================================================

#[test]
fn test_formula_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_simple.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing simple formula")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write simple formula");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized simple formula")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_multiple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_multiple.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing formula with multiple predictors")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write formula with multiple predictors");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized formula with multiple predictors")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_interaction_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_interaction.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing formula with interaction")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write formula with interaction");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized formula with interaction")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_functions_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_functions.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing formula with functions")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write formula with functions");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized formula with functions")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_no_intercept_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_no_intercept.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing formula without intercept")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write formula without intercept");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized formula without intercept")
        .object;

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_one_sided_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_one_sided.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing one-sided formula")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write one-sided formula");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized one-sided formula")
        .object;

    assert_eq!(obj, deserialized);
}

/// Test that formulas preserve their structure correctly.
/// Formulas use Language objects internally.
#[test]
fn test_formula_roundtrip_regression() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    let setup = r#"
        # Create formulas with various structures
        formulas <- list(
            simple = y ~ x,
            multiple = y ~ x + z,
            interaction = y ~ x * z
        )
        saveRDS(formulas, "/tmp/rds2rust_formula_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_formula_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse").object;
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_formula_regression_out.rds", &output).expect("Failed to write");

    // Verify formulas work correctly
    let verify = r#"
        formulas <- readRDS("/tmp/rds2rust_formula_regression_out.rds")

        # Check they are still formulas
        if (!inherits(formulas$simple, "formula")) {
            cat("FAIL: simple is not a formula\n")
            quit(status = 1)
        }

        # Check structure is preserved
        if (!identical(deparse(formulas$simple), "y ~ x")) {
            cat("FAIL: simple formula structure wrong:", deparse(formulas$simple), "\n")
            quit(status = 1)
        }

        # Check they can be used in model fitting context
        df <- data.frame(y = 1:10, x = 1:10, z = 1:10)
        tryCatch({
            model <- lm(formulas$simple, data = df)
            if (is.null(model)) {
                cat("FAIL: Could not fit model with formula\n")
                quit(status = 1)
            }
        }, error = function(e) {
            cat("FAIL: Error fitting model:", e$message, "\n")
            quit(status = 1)
        })

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Formula verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_formula_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_formula_regression_out.rds");
}
