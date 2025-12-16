//! Integration and roundtrip tests for Factor types.

use rds2rust::{read_rds, write_rds, Attributes, FactorData, RObject};
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
// Factor Tests
// =============================================================================

#[test]
fn test_factor_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple factor");

    match obj {
        RObject::Factor(factor_data) => {
            let values = &factor_data.values;
            let levels = &factor_data.levels;
            let ordered = factor_data.ordered;

            // Check it's not ordered
            assert!(!ordered);

            // Check levels
            assert_eq!(levels.len(), 3);
            assert_eq!(levels[0].as_ref(), "high");
            assert_eq!(levels[1].as_ref(), "low");
            assert_eq!(levels[2].as_ref(), "medium");

            // Check values (1-based indices into levels)
            assert_eq!(values.len(), 5);
            // R factor: c("low", "high", "medium", "low", "high")
            // levels = c("high", "low", "medium")
            // So: low=2, high=1, medium=3
            assert_eq!(values[0], 2); // "low"
            assert_eq!(values[1], 1); // "high"
            assert_eq!(values[2], 3); // "medium"
            assert_eq!(values[3], 2); // "low"
            assert_eq!(values[4], 1); // "high"
        }
        _ => panic!("Expected Factor, got {:?}", obj),
    }
}

#[test]
fn test_factor_ordered() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_ordered.rds");
    let obj = read_rds(&data).expect("Failed to parse ordered factor");

    match obj {
        RObject::Factor(factor_data) => {
            let values = &factor_data.values;
            let levels = &factor_data.levels;
            let ordered = factor_data.ordered;

            // Check it's ordered
            assert!(ordered);

            // Check levels (in order)
            assert_eq!(levels.len(), 3);
            assert_eq!(levels[0].as_ref(), "low");
            assert_eq!(levels[1].as_ref(), "medium");
            assert_eq!(levels[2].as_ref(), "high");

            // Check values (1-based indices into levels)
            assert_eq!(values.len(), 4);
            // R factor: ordered(c("low", "medium", "high", "low"), levels = c("low", "medium", "high"))
            // So: low=1, medium=2, high=3
            assert_eq!(values[0], 1); // "low"
            assert_eq!(values[1], 2); // "medium"
            assert_eq!(values[2], 3); // "high"
            assert_eq!(values[3], 1); // "low"
        }
        _ => panic!("Expected Factor, got {:?}", obj),
    }
}

// =============================================================================
// Factor Roundtrip Tests
// =============================================================================

#[test]
fn test_factor_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing factor");

    let serialized = write_rds(&obj).expect("Failed to write factor");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized factor");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_factor_ordered_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_ordered.rds");
    let obj = read_rds(&data).expect("Failed to read existing ordered factor");

    let serialized = write_rds(&obj).expect("Failed to write ordered factor");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized ordered factor");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_factor_with_names_roundtrip() {
    let factor = FactorData {
        values: vec![1, 2, 2, 1],
        levels: vec![Arc::from("alpha"), Arc::from("beta")],
        ordered: false,
    };
    let expected_names = vec![
        Arc::from("cell_a"),
        Arc::from("cell_b"),
        Arc::from("cell_c"),
        Arc::from("cell_d"),
    ];

    let mut attrs = Attributes::new();
    attrs.insert(
        Arc::from("names"),
        RObject::Character(expected_names.clone().into()),
    );

    let obj = factor.clone().with_attributes(attrs);

    let serialized = write_rds(&obj).expect("Failed to write factor with names");
    let deserialized = read_rds(&serialized).expect("Failed to read factor with names");

    match deserialized {
        RObject::WithAttributes { object, attributes } => {
            let data = match object.as_ref() {
                RObject::Factor(data) => data,
                other => panic!("Expected Factor inside WithAttributes, got {:?}", other),
            };
            assert_eq!(data.values, factor.values);
            assert_eq!(data.levels, factor.levels);
            assert_eq!(data.ordered, factor.ordered);

            match attributes.get("names") {
                Some(RObject::Character(names)) => assert_eq!(names, &expected_names),
                other => panic!("Expected names attribute, got {:?}", other),
            }
        }
        other => panic!("Expected WithAttributes wrapping Factor, got {:?}", other),
    }
}
