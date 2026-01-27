//! Integration and roundtrip tests for basic R types.
//!
//! This module tests NULL, Integer, Real, Logical, Character, Raw, and Complex types.

use rds2rust::{read_rds, write_rds, Logical, RObject};
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
// NULL Tests
// =============================================================================

#[test]
fn test_null() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("null.rds");
    let obj = read_rds(&data).expect("Failed to parse NULL").object;

    match obj {
        RObject::Null => {} // Success
        other => panic!("Expected Null, got {:?}", other),
    }
}

#[test]
fn test_null_roundtrip() {
    let obj = RObject::Null;
    let serialized = write_rds(&obj).expect("Failed to write NULL");
    let deserialized = read_rds(&serialized).expect("Failed to read NULL").object;
    assert_eq!(obj, deserialized);
}

// =============================================================================
// Integer Tests
// =============================================================================

#[test]
fn test_integer_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to parse integer").object;

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1);
        }
        other => panic!("Expected Integer, got {:?}", other),
    }
}

#[test]
fn test_integer_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse integer vector")
        .object;

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 10);
            assert_eq!(vec, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        }
        other => panic!("Expected Integer vector, got {:?}", other),
    }
}

#[test]
fn test_integer_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_with_na.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse integer with NA")
        .object;

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 1);
            assert_eq!(vec[1], i32::MIN); // NA_integer_ is i32::MIN
            assert_eq!(vec[2], 3);
        }
        other => panic!("Expected Integer vector with NA, got {:?}", other),
    }
}

#[test]
fn test_integer_roundtrip() {
    let obj = RObject::Integer(vec![1, 2, 3, 4, 5].into());
    let serialized = write_rds(&obj).expect("Failed to write integer vector");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read integer vector")
        .object;
    assert_eq!(obj, deserialized);
}

#[test]
fn test_integer_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing int").object;

    let serialized = write_rds(&obj).expect("Failed to write int");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized int")
        .object;

    assert_eq!(obj, deserialized);
}

// =============================================================================
// Real Tests
// =============================================================================

#[test]
fn test_real_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data).expect("Failed to parse real").object;

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1.5);
        }
        other => panic!("Expected Real, got {:?}", other),
    }
}

#[test]
fn test_real_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse real vector").object;

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], 1.1);
            assert_eq!(vec[1], 2.2);
            assert_eq!(vec[2], 3.3);
            assert_eq!(vec[3], 4.4);
        }
        other => panic!("Expected Real vector, got {:?}", other),
    }
}

#[test]
fn test_real_special() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_special.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse real with special values")
        .object;

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 5);
            assert_eq!(vec[0], 1.5);
            // vec[1] is NA_real_ (a specific NaN bit pattern)
            assert!(vec[1].is_nan());
            assert_eq!(vec[2], f64::INFINITY);
            assert_eq!(vec[3], f64::NEG_INFINITY);
            // vec[4] is NaN
            assert!(vec[4].is_nan());
        }
        other => panic!("Expected Real vector with special values, got {:?}", other),
    }
}

#[test]
fn test_real_roundtrip() {
    let obj = RObject::Real(vec![1.5, 2.5, 3.5].into());
    let serialized = write_rds(&obj).expect("Failed to write real vector");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read real vector")
        .object;
    assert_eq!(obj, deserialized);
}

#[test]
fn test_real_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing real")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write real");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized real")
        .object;

    assert_eq!(obj, deserialized);
}

// =============================================================================
// Logical Tests
// =============================================================================

#[test]
fn test_logical_true() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_true.rds");
    let obj = read_rds(&data).expect("Failed to parse logical").object;

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], Logical::True);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_logical_false() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_false.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse logical FALSE")
        .object;

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], Logical::False);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_logical_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse logical vector")
        .object;

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], Logical::True);
            assert_eq!(vec[1], Logical::False);
            assert_eq!(vec[2], Logical::Na);
            assert_eq!(vec[3], Logical::True);
        }
        other => panic!("Expected Logical vector, got {:?}", other),
    }
}

#[test]
fn test_logical_roundtrip() {
    let obj =
        RObject::Logical(vec![Logical::True, Logical::False, Logical::Na, Logical::True].into());
    let serialized = write_rds(&obj).expect("Failed to write logical vector");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read logical vector")
        .object;
    assert_eq!(obj, deserialized);
}

#[test]
fn test_logical_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing logical")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write logical");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized logical")
        .object;

    assert_eq!(obj, deserialized);
}

// =============================================================================
// Character Tests
// =============================================================================

#[test]
fn test_character_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data).expect("Failed to parse character").object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].as_ref(), "hello");
        }
        other => panic!("Expected Character, got {:?}", other),
    }
}

#[test]
fn test_character_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse character vector")
        .object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0].as_ref(), "foo");
            assert_eq!(vec[1].as_ref(), "bar");
            assert_eq!(vec[2].as_ref(), "baz");
        }
        other => panic!("Expected Character vector, got {:?}", other),
    }
}

#[test]
fn test_character_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_with_na.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse character with NA")
        .object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0].as_ref(), "test");
            assert_eq!(vec[1].as_ref(), "NA"); // NA_character_ is currently parsed as "NA"
            assert_eq!(vec[2].as_ref(), "string");
        }
        other => panic!("Expected Character vector with NA, got {:?}", other),
    }
}

#[test]
fn test_character_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_empty.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse empty character vector")
        .object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 0);
        }
        other => panic!("Expected empty Character vector, got {:?}", other),
    }
}

#[test]
fn test_character_roundtrip() {
    let obj = RObject::Character(vec![Arc::from("hello"), Arc::from("world")].into());
    let serialized = write_rds(&obj).expect("Failed to write character vector");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read character vector")
        .object;
    assert_eq!(obj, deserialized);
}

#[test]
fn test_character_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing character")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write character");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized character")
        .object;

    assert_eq!(obj, deserialized);
}

// =============================================================================
// Raw Tests
// =============================================================================

#[test]
fn test_raw_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("raw_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse raw vector").object;

    match obj {
        RObject::Raw(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 0x01);
            assert_eq!(vec[1], 0x02);
            assert_eq!(vec[2], 0xFF);
        }
        other => panic!("Expected Raw vector, got {:?}", other),
    }
}

#[test]
fn test_raw_roundtrip() {
    let obj = RObject::Raw(vec![0x01, 0x02, 0x03, 0xFF, 0x00].into());
    let serialized = write_rds(&obj).expect("Failed to write raw vector");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read raw vector")
        .object;
    assert_eq!(obj, deserialized);
}

#[test]
fn test_raw_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("raw_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing raw").object;

    let serialized = write_rds(&obj).expect("Failed to write raw");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized raw")
        .object;

    assert_eq!(obj, deserialized);
}

// =============================================================================
// Complex Tests
// =============================================================================

#[test]
fn test_complex_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_single.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse single complex number")
        .object;

    match obj {
        RObject::Complex(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].real, 1.0);
            assert_eq!(vec[0].imaginary, 2.0);
        }
        other => panic!("Expected Complex vector, got {:?}", other),
    }
}

#[test]
fn test_complex_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse complex vector")
        .object;

    match obj {
        RObject::Complex(vec) => {
            assert_eq!(vec.len(), 3);

            // 1+2i
            assert_eq!(vec[0].real, 1.0);
            assert_eq!(vec[0].imaginary, 2.0);

            // 3+4i
            assert_eq!(vec[1].real, 3.0);
            assert_eq!(vec[1].imaginary, 4.0);

            // 5+6i
            assert_eq!(vec[2].real, 5.0);
            assert_eq!(vec[2].imaginary, 6.0);
        }
        other => panic!("Expected Complex vector, got {:?}", other),
    }
}

#[test]
fn test_complex_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_vector.rds");
    let obj = read_rds(&data)
        .expect("Failed to read existing complex")
        .object;

    let serialized = write_rds(&obj).expect("Failed to write complex");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read serialized complex")
        .object;

    assert_eq!(obj, deserialized);
}

/// Test that single-element character vectors roundtrip correctly as STRSXP.
/// They should NOT be converted to symbols.
#[test]
fn test_single_element_character_not_symbol() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    // Create various single-element character vectors in R
    let setup = r#"
        data <- list(
            single_char = "hello",
            char_in_list = list("world"),
            names_attr = setNames(1:3, c("a", "b", "c"))
        )
        saveRDS(data, "/tmp/rds2rust_single_char_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_single_char_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse").object;
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_single_char_regression_out.rds", &output).expect("Failed to write");

    // Verify in R that everything is correct type
    let verify = r#"
        data <- readRDS("/tmp/rds2rust_single_char_regression_out.rds")

        # Check single_char is character, not symbol
        if (!is.character(data$single_char)) {
            cat("FAIL: single_char is not character, got:", typeof(data$single_char), "\n")
            quit(status = 1)
        }
        if (data$single_char != "hello") {
            cat("FAIL: single_char value wrong\n")
            quit(status = 1)
        }

        # Check char_in_list
        if (!is.character(data$char_in_list[[1]])) {
            cat("FAIL: char_in_list[[1]] is not character\n")
            quit(status = 1)
        }

        # Check names_attr - names should be character
        if (!is.character(names(data$names_attr))) {
            cat("FAIL: names_attr names are not character\n")
            quit(status = 1)
        }

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Single-element character verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_single_char_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_single_char_regression_out.rds");
}

/// Test that character vectors in various contexts remain as character vectors.
/// This is a comprehensive test for the symbol/string distinction.
#[test]
fn test_character_vs_symbol_contexts() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    let setup = r#"
        # Various contexts where character vectors should NOT become symbols
        data <- list(
            # Plain character vector
            plain_char = c("a", "b", "c"),

            # Single element character
            single = "single",

            # Character in data frame column
            df = data.frame(x = 1:3, name = c("foo", "bar", "baz"), stringsAsFactors = FALSE),

            # Named vector (names should stay as character)
            named_vec = c(first = 1, second = 2, third = 3),

            # Nested list with characters
            nested = list(inner = list(value = "nested_char"))
        )
        saveRDS(data, "/tmp/rds2rust_char_contexts_regression.rds")
        cat("ok")
    "#;

    let result = run_r_code(setup);
    assert!(result.is_ok(), "Failed to create test data: {:?}", result);

    // Roundtrip through Rust
    let data = fs::read("/tmp/rds2rust_char_contexts_regression.rds").expect("Failed to read");
    let obj = read_rds(&data).expect("Failed to parse").object;
    let output = write_rds(&obj).expect("Failed to serialize");
    fs::write("/tmp/rds2rust_char_contexts_regression_out.rds", &output).expect("Failed to write");

    // Verify everything is correct type
    let verify = r#"
        data <- readRDS("/tmp/rds2rust_char_contexts_regression_out.rds")

        # Plain character vector
        if (!is.character(data$plain_char) || length(data$plain_char) != 3) {
            cat("FAIL: plain_char not correct\n")
            quit(status = 1)
        }

        # Single element
        if (!is.character(data$single) || data$single != "single") {
            cat("FAIL: single not correct, type:", typeof(data$single), "\n")
            quit(status = 1)
        }

        # Data frame column
        if (!is.character(data$df$name)) {
            cat("FAIL: df$name not character\n")
            quit(status = 1)
        }

        # Named vector names
        if (!is.character(names(data$named_vec))) {
            cat("FAIL: named_vec names not character\n")
            quit(status = 1)
        }

        # Nested character
        if (!is.character(data$nested$inner$value) || data$nested$inner$value != "nested_char") {
            cat("FAIL: nested character not correct\n")
            quit(status = 1)
        }

        cat("PASS")
    "#;

    let result = run_r_code(verify);
    assert!(
        result.is_ok() && result.as_ref().unwrap().contains("PASS"),
        "Character contexts verification failed: {:?}",
        result
    );

    // Cleanup
    let _ = fs::remove_file("/tmp/rds2rust_char_contexts_regression.rds");
    let _ = fs::remove_file("/tmp/rds2rust_char_contexts_regression_out.rds");
}

// =============================================================================
// Logical Vector with Attributes Tests
// =============================================================================

#[test]
fn test_logical_vector_with_attributes() {
    use rds2rust::Attributes;

    // Create logical vector with attributes
    let logical_vec = RObject::Logical(vec![Logical::True, Logical::False, Logical::True].into());

    let mut attrs = Attributes::new();
    attrs.insert(
        "test_attr".into(),
        RObject::Character(vec!["value".into()].into()),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap().object;

    // Verify structure matches
    assert_eq!(obj, result);
}

#[test]
fn test_logical_vector_empty_with_attributes() {
    use rds2rust::Attributes;

    // Edge case: empty logical vector with attributes (length=0)
    let logical_vec = RObject::Logical(vec![].into());
    let mut attrs = Attributes::new();
    attrs.insert("empty".into(), RObject::Logical(vec![Logical::True].into()));

    let obj = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs,
    };

    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap().object;
    assert_eq!(obj, result);
}

#[test]
fn test_logical_vector_na_heavy_with_attributes() {
    use rds2rust::Attributes;

    // Edge case: mixture of TRUE/FALSE/NA
    let logical_vec = RObject::Logical(
        vec![
            Logical::True,
            Logical::Na,
            Logical::False,
            Logical::Na,
            Logical::Na,
            Logical::True,
            Logical::False,
            Logical::Na,
        ]
        .into(),
    );

    let mut attrs = Attributes::new();
    attrs.insert("test".into(), RObject::Integer(vec![1].into()));

    let obj = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs,
    };

    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap().object;
    assert_eq!(obj, result);
}

#[test]
fn test_logical_matrix_with_dimnames() {
    use rds2rust::Attributes;

    // Create 2x3 logical matrix (R stores by column)
    let logical_vec = RObject::Logical(
        vec![
            Logical::True,
            Logical::False, // Column 1
            Logical::True,
            Logical::True, // Column 2
            Logical::False,
            Logical::True, // Column 3
        ]
        .into(),
    );

    let mut attrs = Attributes::new();

    // Add dim attribute (rows, cols)
    attrs.insert("dim".into(), RObject::Integer(vec![2, 3].into()));

    // Add dimnames attribute
    let dimnames = RObject::List(vec![
        RObject::Character(vec!["row1".into(), "row2".into()].into()),
        RObject::Character(vec!["col1".into(), "col2".into(), "col3".into()].into()),
    ]);
    attrs.insert("dimnames".into(), dimnames);

    let matrix = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs,
    };

    // Write and read back
    let bytes = write_rds(&matrix).unwrap();
    let result = read_rds(&bytes[..]).unwrap().object;

    assert_eq!(matrix, result);
}

#[test]
fn test_logical_matrix_attribute_order_variation() {
    use rds2rust::Attributes;

    // Test that logical matrices with attributes can be written regardless of attribute insertion order
    // Note: The parser may reorder attributes during read, so we verify each version writes successfully
    let logical_vec = RObject::Logical(vec![Logical::True; 6].into());

    // Version 1: dim then dimnames
    let mut attrs1 = Attributes::new();
    attrs1.insert("dim".into(), RObject::Integer(vec![2, 3].into()));
    attrs1.insert(
        "dimnames".into(),
        RObject::List(vec![
            RObject::Character(vec!["r1".into(), "r2".into()].into()),
            RObject::Character(vec!["c1".into(), "c2".into(), "c3".into()].into()),
        ]),
    );
    let matrix1 = RObject::WithAttributes {
        object: Box::new(logical_vec.clone()),
        attributes: attrs1,
    };

    // Version 2: dimnames then dim (reverse order)
    let mut attrs2 = Attributes::new();
    attrs2.insert(
        "dimnames".into(),
        RObject::List(vec![
            RObject::Character(vec!["r1".into(), "r2".into()].into()),
            RObject::Character(vec!["c1".into(), "c2".into(), "c3".into()].into()),
        ]),
    );
    attrs2.insert("dim".into(), RObject::Integer(vec![2, 3].into()));
    let matrix2 = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs2,
    };

    // Both should write successfully and produce valid RDS
    let bytes1 = write_rds(&matrix1).unwrap();
    let result1 = read_rds(&bytes1[..]).unwrap().object;

    let bytes2 = write_rds(&matrix2).unwrap();
    let result2 = read_rds(&bytes2[..]).unwrap().object;

    // Verify both results are WithAttributes containing Logical vectors
    match (&result1, &result2) {
        (
            RObject::WithAttributes {
                object: obj1,
                attributes: attrs1,
            },
            RObject::WithAttributes {
                object: obj2,
                attributes: attrs2,
            },
        ) => {
            // Both should have the same logical vector
            assert_eq!(obj1, obj2);
            // Both should have dim and dimnames attributes (order may vary)
            assert!(attrs1.get("dim").is_some());
            assert!(attrs1.get("dimnames").is_some());
            assert!(attrs2.get("dim").is_some());
            assert!(attrs2.get("dimnames").is_some());
        }
        _ => panic!("Expected WithAttributes objects"),
    }
}
