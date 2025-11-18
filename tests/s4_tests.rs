//! Integration and roundtrip tests for S4 objects.

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
        RObject::S4Object(s4_data) => {
            let class = &s4_data.class;
            let slots = &s4_data.slots;

            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0].as_ref(), "Animal");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check species slot
            match slots.get(&Arc::from("species")) {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 1);
                    assert_eq!(species[0].as_ref(), "Tiger");
                }
                _ => panic!("Expected 'species' slot with character value"),
            }

            // Check age slot
            match slots.get(&Arc::from("age")) {
                Some(RObject::Real(age)) => {
                    assert_eq!(age.len(), 1);
                    assert_eq!(age[0], 5.0);
                }
                _ => panic!("Expected 'age' slot with numeric value"),
            }

            // Check habitat slot
            match slots.get(&Arc::from("habitat")) {
                Some(RObject::Character(habitat)) => {
                    assert_eq!(habitat.len(), 1);
                    assert_eq!(habitat[0].as_ref(), "Rainforest");
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
        RObject::S4Object(s4_data) => {
            let class = &s4_data.class;
            let slots = &s4_data.slots;

            // Check the class (should show inheritance)
            assert!(class.len() >= 1);
            assert_eq!(class[0].as_ref(), "Bird");

            // Check slots from both parent and child classes
            assert!(slots.len() >= 5);

            // Parent class slots (from Animal)
            assert!(slots.get(&Arc::from("species")).is_some());
            assert!(slots.get(&Arc::from("age")).is_some());
            assert!(slots.get(&Arc::from("habitat")).is_some());

            // Child class slots (from Bird)
            match slots.get(&Arc::from("wingspan")) {
                Some(RObject::Real(wingspan)) => {
                    assert_eq!(wingspan.len(), 1);
                    assert_eq!(wingspan[0], 1.2);
                }
                _ => panic!("Expected 'wingspan' slot"),
            }

            match slots.get(&Arc::from("can_fly")) {
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
        RObject::S4Object(s4_data) => {
            let class = &s4_data.class;
            let slots = &s4_data.slots;

            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0].as_ref(), "Aquarium");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check temperatures slot (numeric vector)
            match slots.get(&Arc::from("temperatures")) {
                Some(RObject::Real(temps)) => {
                    assert_eq!(temps.len(), 3);
                    assert_eq!(temps[0], 24.5);
                    assert_eq!(temps[1], 25.0);
                    assert_eq!(temps[2], 24.8);
                }
                _ => panic!("Expected 'temperatures' slot with numeric vector"),
            }

            // Check fish_species slot (character vector)
            match slots.get(&Arc::from("fish_species")) {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 3);
                    assert_eq!(species[0].as_ref(), "clownfish");
                    assert_eq!(species[1].as_ref(), "tang");
                    assert_eq!(species[2].as_ref(), "angelfish");
                }
                _ => panic!("Expected 'fish_species' slot with character vector"),
            }

            // Check saltwater slot (logical)
            match slots.get(&Arc::from("saltwater")) {
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
    let deserialized =
        read_rds(&serialized).expect("Failed to read serialized S4 inheritance object");

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

// =============================================================================
// S4 Object as Attribute Container Tests
// =============================================================================

#[test]
fn test_s4_as_attribute_container() {
    // This test verifies the fix for S4 objects used as attribute containers
    // The bug was that parse_attributes() would return empty attributes when
    // encountering an S4Object, causing the class field to be lost.

    use rds2rust::S4ObjectData;
    use std::collections::HashMap;

    // Create an S4 object with a class and slots
    let mut slots = HashMap::new();
    slots.insert(Arc::from("data"), RObject::Integer(vec![1, 2, 3]));
    slots.insert(
        Arc::from("metadata"),
        RObject::Character(vec![Arc::from("test")]),
    );

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("ComplexObject"), Arc::from("BaseClass")],
        package: None,
        slots,
    }));

    // Serialize and deserialize
    let serialized = write_rds(&s4_obj).expect("Failed to serialize S4 object");
    let deserialized = read_rds(&serialized).expect("Failed to deserialize S4 object");

    // Verify the object structure is preserved
    match deserialized {
        RObject::S4Object(s4_data) => {
            // Most importantly, verify the class field is not empty!
            assert!(
                !s4_data.class.is_empty(),
                "S4 object class should not be empty"
            );
            assert_eq!(s4_data.class.len(), 2, "Should have 2 classes");
            assert_eq!(s4_data.class[0].as_ref(), "ComplexObject");
            assert_eq!(s4_data.class[1].as_ref(), "BaseClass");

            // Verify slots are preserved
            assert_eq!(s4_data.slots.len(), 2, "Should have 2 slots");

            match s4_data.slots.get(&Arc::from("data")) {
                Some(RObject::Integer(vals)) => {
                    assert_eq!(vals, &vec![1, 2, 3]);
                }
                _ => panic!("Expected 'data' slot with integer values"),
            }

            match s4_data.slots.get(&Arc::from("metadata")) {
                Some(RObject::Character(vals)) => {
                    assert_eq!(vals.len(), 1);
                    assert_eq!(vals[0].as_ref(), "test");
                }
                _ => panic!("Expected 'metadata' slot with character value"),
            }
        }
        _ => panic!("Expected S4Object after deserialization"),
    }
}

#[test]
fn test_s4_nested_as_attribute() {
    // Test S4 objects that contain other S4 objects in their slots
    use rds2rust::S4ObjectData;
    use std::collections::HashMap;

    // Create inner S4 object
    let mut inner_slots = HashMap::new();
    inner_slots.insert(Arc::from("value"), RObject::Real(vec![3.14]));

    let inner_s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("InnerClass")],
        package: None,
        slots: inner_slots,
    }));

    // Create outer S4 object containing the inner one
    let mut outer_slots = HashMap::new();
    outer_slots.insert(Arc::from("inner"), inner_s4);
    outer_slots.insert(
        Arc::from("name"),
        RObject::Character(vec![Arc::from("outer")]),
    );

    let outer_s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("OuterClass")],
        package: None,
        slots: outer_slots,
    }));

    // Serialize and deserialize
    let serialized = write_rds(&outer_s4).expect("Failed to serialize nested S4 object");
    let deserialized = read_rds(&serialized).expect("Failed to deserialize nested S4 object");

    // Verify the structure
    match deserialized {
        RObject::S4Object(outer_data) => {
            // Verify outer class
            assert_eq!(outer_data.class.len(), 1);
            assert_eq!(outer_data.class[0].as_ref(), "OuterClass");
            assert_eq!(outer_data.slots.len(), 2);

            // Verify the nested S4 object
            match outer_data.slots.get(&Arc::from("inner")) {
                Some(RObject::S4Object(inner_data)) => {
                    // Critical: verify the inner S4 object's class is preserved
                    assert!(
                        !inner_data.class.is_empty(),
                        "Inner S4 object class should not be empty"
                    );
                    assert_eq!(inner_data.class.len(), 1);
                    assert_eq!(inner_data.class[0].as_ref(), "InnerClass");

                    // Verify inner slots
                    match inner_data.slots.get(&Arc::from("value")) {
                        Some(RObject::Real(vals)) => {
                            assert_eq!(vals.len(), 1);
                            assert_eq!(vals[0], 3.14);
                        }
                        _ => panic!("Expected 'value' slot in inner S4 object"),
                    }
                }
                _ => panic!("Expected nested S4Object in 'inner' slot"),
            }
        }
        _ => panic!("Expected outer S4Object after deserialization"),
    }
}

#[test]
fn test_s4_flags_correctly_set() {
    // Test that S4 objects have correct serialization flags for R method dispatch
    // This ensures isS4() returns TRUE and slot accessors work in R
    use rds2rust::S4ObjectData;
    use std::collections::HashMap;

    // Create an S4 object similar to a Matrix dgCMatrix
    let mut slots = HashMap::new();
    slots.insert(Arc::from("Dim"), RObject::Integer(vec![3, 3]));
    slots.insert(
        Arc::from("Dimnames"),
        RObject::List(vec![RObject::Null, RObject::Null]),
    );
    slots.insert(Arc::from("x"), RObject::Real(vec![1.0, 2.0, 3.0]));
    slots.insert(Arc::from("i"), RObject::Integer(vec![0, 1, 2]));
    slots.insert(Arc::from("p"), RObject::Integer(vec![0, 1, 2, 3]));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("dgCMatrix")],
        package: Some(Arc::from("Matrix")),
        slots,
    }));

    // Serialize the object
    let serialized = write_rds(&s4_obj).expect("Failed to serialize S4 object");

    // Decompress and check the flags
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(&serialized[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("Failed to decompress");

    // Skip header (14 bytes for "X\n" format + version info + encoding)
    // The S4 object flags should be at byte 28 (0x1c)
    // Expected flags: 0x00010319
    //   - Type: 0x19 (25 = S4SXP)
    //   - Bit 8: IS_OBJECT_BIT (0x100)
    //   - Bit 9: HAS_ATTR_BIT (0x200)
    //   - Bits 12+: S4_LEVELS (0x10000) - bit 4 in gp field indicates S4

    // Find the S4SXP flags (type 25 = 0x19)
    let mut found_s4_flags = false;
    for i in 0..decompressed.len().saturating_sub(4) {
        let flags = u32::from_be_bytes([
            decompressed[i],
            decompressed[i + 1],
            decompressed[i + 2],
            decompressed[i + 3],
        ]);

        // Check if this looks like S4SXP flags
        if (flags & 0xFF) == 25 {
            // S4SXP type
            // Verify IS_OBJECT_BIT is set (bit 8)
            let is_object = (flags & 0x100) != 0;
            // Verify HAS_ATTR_BIT is set (bit 9)
            let has_attr = (flags & 0x200) != 0;
            // Verify S4_LEVELS is set (0x10000)
            let has_s4_levels = (flags & 0x10000) != 0;

            if has_attr {
                assert!(
                    is_object,
                    "S4 object must have IS_OBJECT_BIT set (bit 8). Flags: 0x{:08x}",
                    flags
                );
                assert!(
                    has_s4_levels,
                    "S4 object must have S4_LEVELS set (0x10000). Flags: 0x{:08x}",
                    flags
                );
                found_s4_flags = true;
                break;
            }
        }
    }

    assert!(
        found_s4_flags,
        "Could not find S4SXP flags in serialized output"
    );

    // Also verify roundtrip works
    let deserialized = read_rds(&serialized).expect("Failed to deserialize S4 object");
    match deserialized {
        RObject::S4Object(s4_data) => {
            assert_eq!(s4_data.class[0].as_ref(), "dgCMatrix");
            assert_eq!(s4_data.package.as_ref().map(|p| p.as_ref()), Some("Matrix"));
            assert!(s4_data.slots.contains_key(&Arc::from("Dim")));
        }
        _ => panic!("Expected S4Object after deserialization"),
    }
}

#[test]
fn test_s4_package_attribute_preserved() {
    // Test that the package attribute is correctly preserved during roundtrip
    // This is essential for R's method dispatch to find the correct methods
    use rds2rust::S4ObjectData;
    use std::collections::HashMap;

    let test_cases = vec![
        ("Matrix", "dgCMatrix"),
        ("SeuratObject", "Assay"),
        ("methods", "signature"),
        (".GlobalEnv", "UserClass"),
    ];

    for (package, class_name) in test_cases {
        let mut slots = HashMap::new();
        slots.insert(Arc::from("data"), RObject::Integer(vec![1, 2, 3]));

        let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
            class: vec![Arc::from(class_name)],
            package: Some(Arc::from(package)),
            slots,
        }));

        let serialized = write_rds(&s4_obj).expect("Failed to serialize");
        let deserialized = read_rds(&serialized).expect("Failed to deserialize");

        match deserialized {
            RObject::S4Object(s4_data) => {
                assert_eq!(
                    s4_data.class[0].as_ref(),
                    class_name,
                    "Class name mismatch for package {}",
                    package
                );
                assert_eq!(
                    s4_data.package.as_ref().map(|p| p.as_ref()),
                    Some(package),
                    "Package attribute not preserved for class {}",
                    class_name
                );
            }
            _ => panic!("Expected S4Object for class {}", class_name),
        }
    }
}

#[test]
fn test_s4_default_package_fallback() {
    // Test that S4 objects without a package get .GlobalEnv as default
    use rds2rust::S4ObjectData;
    use std::collections::HashMap;

    let mut slots = HashMap::new();
    slots.insert(Arc::from("value"), RObject::Real(vec![42.0]));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("MyCustomClass")],
        package: None, // No package specified
        slots,
    }));

    let serialized = write_rds(&s4_obj).expect("Failed to serialize");
    let deserialized = read_rds(&serialized).expect("Failed to deserialize");

    match deserialized {
        RObject::S4Object(s4_data) => {
            assert_eq!(s4_data.class[0].as_ref(), "MyCustomClass");
            // When no package is specified, it should default to .GlobalEnv
            assert_eq!(
                s4_data.package.as_ref().map(|p| p.as_ref()),
                Some(".GlobalEnv"),
                "Default package should be .GlobalEnv"
            );
        }
        _ => panic!("Expected S4Object"),
    }
}
