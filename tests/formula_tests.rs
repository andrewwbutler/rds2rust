//! Integration and roundtrip tests for Formula objects.
//!
//! Formulas are S3 objects with class="formula" and a Language base.

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
// Formula Tests
// =============================================================================

#[test]
fn test_formula_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple formula");

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
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to parse formula with multiple predictors");

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to parse formula with interaction");

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to parse formula with functions");

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to parse formula without intercept");

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to parse one-sided formula");

    match obj {
        RObject::S3Object(s3_data) => {
            let base = &s3_data.base;
            let class = &s3_data.class;

            assert_eq!(class[0].as_ref(), "formula");
            match base.as_ref() {
                RObject::Language(_) => {
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
    let obj = read_rds(&data).expect("Failed to read existing simple formula");

    let serialized = write_rds(&obj).expect("Failed to write simple formula");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized simple formula");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_multiple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_multiple.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with multiple predictors");

    let serialized = write_rds(&obj).expect("Failed to write formula with multiple predictors");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with multiple predictors");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_interaction_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_interaction.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with interaction");

    let serialized = write_rds(&obj).expect("Failed to write formula with interaction");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with interaction");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_functions_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_functions.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with functions");

    let serialized = write_rds(&obj).expect("Failed to write formula with functions");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with functions");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_no_intercept_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_no_intercept.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula without intercept");

    let serialized = write_rds(&obj).expect("Failed to write formula without intercept");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula without intercept");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_formula_one_sided_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_one_sided.rds");
    let obj = read_rds(&data).expect("Failed to read existing one-sided formula");

    let serialized = write_rds(&obj).expect("Failed to write one-sided formula");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized one-sided formula");

    assert_eq!(obj, deserialized);
}
