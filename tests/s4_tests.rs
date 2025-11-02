//! Integration and roundtrip tests for S4 objects.

use rds2rust::{read_rds, write_rds, Logical, RObject};
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
// S4 Object Tests
// =============================================================================

#[test]
fn test_s4_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple S4 object");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "Animal");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check species slot
            match slots.get("species") {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 1);
                    assert_eq!(species[0], "Tiger");
                }
                _ => panic!("Expected 'species' slot with character value"),
            }

            // Check age slot
            match slots.get("age") {
                Some(RObject::Real(age)) => {
                    assert_eq!(age.len(), 1);
                    assert_eq!(age[0], 5.0);
                }
                _ => panic!("Expected 'age' slot with numeric value"),
            }

            // Check habitat slot
            match slots.get("habitat") {
                Some(RObject::Character(habitat)) => {
                    assert_eq!(habitat.len(), 1);
                    assert_eq!(habitat[0], "Rainforest");
                }
                _ => panic!("Expected 'habitat' slot with character value"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

#[test]
fn test_s4_inheritance() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_inheritance.rds");
    let obj = read_rds(&data).expect("Failed to parse S4 object with inheritance");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class (should show inheritance)
            assert!(class.len() >= 1);
            assert_eq!(class[0], "Bird");

            // Check slots from both parent and child classes
            assert!(slots.len() >= 5);

            // Parent class slots (from Animal)
            assert!(slots.get("species").is_some());
            assert!(slots.get("age").is_some());
            assert!(slots.get("habitat").is_some());

            // Child class slots (from Bird)
            match slots.get("wingspan") {
                Some(RObject::Real(wingspan)) => {
                    assert_eq!(wingspan.len(), 1);
                    assert_eq!(wingspan[0], 1.2);
                }
                _ => panic!("Expected 'wingspan' slot"),
            }

            match slots.get("can_fly") {
                Some(RObject::Logical(can_fly)) => {
                    assert_eq!(can_fly.len(), 1);
                    assert_eq!(can_fly[0], Logical::True);
                }
                _ => panic!("Expected 'can_fly' slot"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

#[test]
fn test_s4_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_complex.rds");
    let obj = read_rds(&data).expect("Failed to parse complex S4 object");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "Aquarium");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check temperatures slot (numeric vector)
            match slots.get("temperatures") {
                Some(RObject::Real(temps)) => {
                    assert_eq!(temps.len(), 3);
                    assert_eq!(temps[0], 24.5);
                    assert_eq!(temps[1], 25.0);
                    assert_eq!(temps[2], 24.8);
                }
                _ => panic!("Expected 'temperatures' slot with numeric vector"),
            }

            // Check fish_species slot (character vector)
            match slots.get("fish_species") {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 3);
                    assert_eq!(species, &vec!["clownfish", "tang", "angelfish"]);
                }
                _ => panic!("Expected 'fish_species' slot with character vector"),
            }

            // Check saltwater slot (logical)
            match slots.get("saltwater") {
                Some(RObject::Logical(saltwater)) => {
                    assert_eq!(saltwater.len(), 1);
                    assert_eq!(saltwater[0], Logical::True);
                }
                _ => panic!("Expected 'saltwater' slot with logical value"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

// =============================================================================
// S4 Object Roundtrip Tests
// =============================================================================

#[test]
fn test_s4_simple_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 object");

    let serialized = write_rds(&obj).expect("Failed to write S4 object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_s4_inheritance_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_inheritance.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 inheritance object");

    let serialized = write_rds(&obj).expect("Failed to write S4 inheritance object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 inheritance object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_s4_complex_roundtrip() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_complex.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 complex object");

    let serialized = write_rds(&obj).expect("Failed to write S4 complex object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 complex object");

    assert_eq!(obj, deserialized);
}
