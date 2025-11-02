//! Integration and roundtrip tests for basic R types.
//!
//! This module tests NULL, Integer, Real, Logical, Character, Raw, and Complex types.

use rds2rust::{read_rds, write_rds, Logical, RObject};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
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
    let obj = read_rds(&data).expect("Failed to parse NULL");

    match obj {
        RObject::Null => {} // Success
        other => panic!("Expected Null, got {:?}", other),
    }
}

#[test]
fn test_null_roundtrip() {
    let obj = RObject::Null;
    let serialized = write_rds(&obj).expect("Failed to write NULL");
    let deserialized = read_rds(&serialized).expect("Failed to read NULL");
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
    let obj = read_rds(&data).expect("Failed to parse integer");

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
    let obj = read_rds(&data).expect("Failed to parse integer vector");

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
    let obj = read_rds(&data).expect("Failed to parse integer with NA");

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
    let obj = RObject::Integer(vec![1, 2, 3, 4, 5]);
    let serialized = write_rds(&obj).expect("Failed to write integer vector");
    let deserialized = read_rds(&serialized).expect("Failed to read integer vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_integer_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing int");

    let serialized = write_rds(&obj).expect("Failed to write int");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized int");

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
    let obj = read_rds(&data).expect("Failed to parse real");

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
    let obj = read_rds(&data).expect("Failed to parse real vector");

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
    let obj = read_rds(&data).expect("Failed to parse real with special values");

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
    let obj = RObject::Real(vec![1.5, 2.5, 3.5]);
    let serialized = write_rds(&obj).expect("Failed to write real vector");
    let deserialized = read_rds(&serialized).expect("Failed to read real vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_real_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing real");

    let serialized = write_rds(&obj).expect("Failed to write real");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized real");

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
    let obj = read_rds(&data).expect("Failed to parse logical");

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
    let obj = read_rds(&data).expect("Failed to parse logical FALSE");

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
    let obj = read_rds(&data).expect("Failed to parse logical vector");

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
    let obj = RObject::Logical(vec![Logical::True, Logical::False, Logical::Na, Logical::True]);
    let serialized = write_rds(&obj).expect("Failed to write logical vector");
    let deserialized = read_rds(&serialized).expect("Failed to read logical vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_logical_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing logical");

    let serialized = write_rds(&obj).expect("Failed to write logical");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized logical");

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
    let obj = read_rds(&data).expect("Failed to parse character");

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
    let obj = read_rds(&data).expect("Failed to parse character vector");

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
    let obj = read_rds(&data).expect("Failed to parse character with NA");

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
    let obj = read_rds(&data).expect("Failed to parse empty character vector");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 0);
        }
        other => panic!("Expected empty Character vector, got {:?}", other),
    }
}

#[test]
fn test_character_roundtrip() {
    let obj = RObject::Character(vec![Arc::from("hello"), Arc::from("world")]);
    let serialized = write_rds(&obj).expect("Failed to write character vector");
    let deserialized = read_rds(&serialized).expect("Failed to read character vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_character_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing character");

    let serialized = write_rds(&obj).expect("Failed to write character");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized character");

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
    let obj = read_rds(&data).expect("Failed to parse raw vector");

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
    let obj = RObject::Raw(vec![0x01, 0x02, 0x03, 0xFF, 0x00]);
    let serialized = write_rds(&obj).expect("Failed to write raw vector");
    let deserialized = read_rds(&serialized).expect("Failed to read raw vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_raw_roundtrip_existing() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("raw_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing raw");

    let serialized = write_rds(&obj).expect("Failed to write raw");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized raw");

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
    let obj = read_rds(&data).expect("Failed to parse single complex number");

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
    let obj = read_rds(&data).expect("Failed to parse complex vector");

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
    let obj = read_rds(&data).expect("Failed to read existing complex");

    let serialized = write_rds(&obj).expect("Failed to write complex");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized complex");

    assert_eq!(obj, deserialized);
}
