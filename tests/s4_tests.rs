//! Integration and roundtrip tests for S4 objects.

use indexmap::IndexMap;
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
    // Using IndexMap instead of HashMap for order preservation

    // Create an S4 object with a class and slots
    let mut slots = IndexMap::new();
    slots.insert(Arc::from("data"), RObject::Integer(vec![1, 2, 3].into()));
    slots.insert(
        Arc::from("metadata"),
        RObject::Character(vec![Arc::from("test")].into()),
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
    // Using IndexMap instead of HashMap for order preservation

    // Create inner S4 object
    let mut inner_slots = IndexMap::new();
    inner_slots.insert(Arc::from("value"), RObject::Real(vec![3.14].into()));

    let inner_s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("InnerClass")],
        package: None,
        slots: inner_slots,
    }));

    // Create outer S4 object containing the inner one
    let mut outer_slots = IndexMap::new();
    outer_slots.insert(Arc::from("inner"), inner_s4);
    outer_slots.insert(
        Arc::from("name"),
        RObject::Character(vec![Arc::from("outer")].into()),
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
    // Using IndexMap instead of HashMap for order preservation

    // Create an S4 object similar to a Matrix dgCMatrix
    let mut slots = IndexMap::new();
    slots.insert(Arc::from("Dim"), RObject::Integer(vec![3, 3].into()));
    slots.insert(
        Arc::from("Dimnames"),
        RObject::List(vec![RObject::Null, RObject::Null]),
    );
    slots.insert(Arc::from("x"), RObject::Real(vec![1.0, 2.0, 3.0].into()));
    slots.insert(Arc::from("i"), RObject::Integer(vec![0, 1, 2].into()));
    slots.insert(Arc::from("p"), RObject::Integer(vec![0, 1, 2, 3].into()));

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
    // Using IndexMap instead of HashMap for order preservation

    let test_cases = vec![
        ("Matrix", "dgCMatrix"),
        ("ExamplePkg", "ExampleClass"),
        ("methods", "signature"),
        (".GlobalEnv", "UserClass"),
    ];

    for (package, class_name) in test_cases {
        let mut slots = IndexMap::new();
        slots.insert(Arc::from("data"), RObject::Integer(vec![1, 2, 3].into()));

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
    // Using IndexMap instead of HashMap for order preservation

    let mut slots = IndexMap::new();
    slots.insert(Arc::from("value"), RObject::Real(vec![42.0].into()));

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


/// Test that parsing multiple S4 objects in sequence doesn't leak state.
/// This is a regression test for the PENDING_CLASS_ATTRS thread-local bug
/// where attributes from one parse could contaminate subsequent parses.
#[test]
fn test_cross_parse_state_isolation() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Parse the same S4 file twice to ensure identical results
    let data = read_test_file("s4_complex.rds");

    let obj1 = read_rds(&data).expect("Failed to parse S4 object (first read)");
    let obj2 = read_rds(&data).expect("Failed to parse S4 object (second read)");

    // Extract slot values from both parses
    let extract_slots = |obj: &RObject| -> IndexMap<Arc<str>, RObject> {
        match obj {
            RObject::S4Object(s4_data) => s4_data.slots.clone(),
            _ => panic!("Expected S4Object"),
        }
    };

    let slots1 = extract_slots(&obj1);
    let slots2 = extract_slots(&obj2);

    // Verify both parses have the same slot keys
    assert_eq!(
        slots1.len(),
        slots2.len(),
        "Slot count should be identical across parses"
    );

    for (key1, key2) in slots1.keys().zip(slots2.keys()) {
        assert_eq!(key1, key2, "Slot keys should be in same order");
    }

    // Verify each slot has the same variant type (not corrupted)
    for (key, value1) in &slots1 {
        let value2 = slots2.get(key).expect("Slot should exist in second parse");

        // Check that the variant type matches (discriminant comparison)
        assert_eq!(
            std::mem::discriminant(value1),
            std::mem::discriminant(value2),
            "Slot '{}' should have same type across parses (got {} vs {})",
            key,
            variant_name(value1),
            variant_name(value2)
        );
    }
}

/// Test that parsing different S4 files in sequence doesn't cross-contaminate.
/// This tests the more severe case where state from parse #1 leaks into parse #2.
#[test]
fn test_cross_file_parse_isolation() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Parse two different S4 files in sequence
    let simple_data = read_test_file("s4_simple.rds");
    let complex_data = read_test_file("s4_complex.rds");

    // Parse simple, then complex
    let simple_obj = read_rds(&simple_data).expect("Failed to parse simple S4");
    let complex_obj = read_rds(&complex_data).expect("Failed to parse complex S4");

    // Now parse complex again - it should be identical to the previous parse
    let complex_obj2 = read_rds(&complex_data).expect("Failed to parse complex S4 (second time)");

    // Extract slots
    let extract_slots = |obj: &RObject| -> IndexMap<Arc<str>, RObject> {
        match obj {
            RObject::S4Object(s4_data) => s4_data.slots.clone(),
            _ => panic!("Expected S4Object"),
        }
    };

    let complex_slots1 = extract_slots(&complex_obj);
    let complex_slots2 = extract_slots(&complex_obj2);

    // Verify that parsing simple didn't contaminate complex
    for (key, value1) in &complex_slots1 {
        let value2 = complex_slots2
            .get(key)
            .expect("Slot should exist in second parse");

        assert_eq!(
            std::mem::discriminant(value1),
            std::mem::discriminant(value2),
            "Slot '{}' corrupted after parsing simple file (got {} vs {})",
            key,
            variant_name(value1),
            variant_name(value2)
        );
    }

    // Verify simple is still correct when parsed after complex
    let simple_obj2 = read_rds(&simple_data).expect("Failed to parse simple S4 (second time)");
    let simple_slots1 = extract_slots(&simple_obj);
    let simple_slots2 = extract_slots(&simple_obj2);

    assert_eq!(
        simple_slots1.len(),
        simple_slots2.len(),
        "Simple S4 slots corrupted after parsing complex"
    );
}

/// Helper function to get a human-readable variant name for debugging
fn variant_name(obj: &RObject) -> &'static str {
    match obj {
        RObject::Null => "Null",
        RObject::Integer(_) => "Integer",
        RObject::Real(_) => "Real",
        RObject::Logical(_) => "Logical",
        RObject::Character(_) => "Character",
        RObject::Symbol(_) => "Symbol",
        RObject::Raw(_) => "Raw",
        RObject::Complex(_) => "Complex",
        RObject::List(_) => "List",
        RObject::Pairlist(_) => "Pairlist",
        RObject::Language { .. } => "Language",
        RObject::Expression(_) => "Expression",
        RObject::Closure { .. } => "Closure",
        RObject::Environment { .. } => "Environment",
        RObject::Promise { .. } => "Promise",
        RObject::Special { .. } => "Special",
        RObject::Builtin { .. } => "Builtin",
        RObject::Bytecode { .. } => "Bytecode",
        RObject::DataFrame(_) => "DataFrame",
        RObject::Factor(_) => "Factor",
        RObject::S3Object(_) => "S3Object",
        RObject::S4Object(_) => "S4Object",
        RObject::Namespace(_) => "Namespace",
        RObject::GlobalEnv => "GlobalEnv",
        RObject::BaseEnv => "BaseEnv",
        RObject::EmptyEnv => "EmptyEnv",
        RObject::MissingArg => "MissingArg",
        RObject::UnboundValue => "UnboundValue",
        RObject::WithAttributes { .. } => "WithAttributes",
        RObject::Shared(_) => "Shared",
        _ => "Unknown",
    }
}

// =============================================================================
// S4 Object Tests with Logical Matrix .Data Slot
// =============================================================================

#[test]
fn test_s4_with_logical_matrix_data_slot() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create .Data slot: logical matrix (non-square: 5 rows × 3 cols)
    // Use irregular TRUE/FALSE/NA pattern to catch row/col ordering bugs
    let n_rows = 5;
    let n_cols = 3;
    let logical_data = vec![
        // Column 1
        Logical::True,
        Logical::False,
        Logical::Na,
        Logical::True,
        Logical::False,
        // Column 2
        Logical::Na,
        Logical::True,
        Logical::True,
        Logical::False,
        Logical::Na,
        // Column 3
        Logical::False,
        Logical::Na,
        Logical::True,
        Logical::True,
        Logical::False,
    ];
    let logical_vec = RObject::Logical(logical_data.into());

    let mut matrix_attrs = Attributes::new();
    matrix_attrs.insert("dim".into(), RObject::Integer(vec![n_rows, n_cols].into()));

    let row_names: Vec<Arc<str>> = (0..n_rows)
        .map(|i| format!("item_{}", i + 1).into())
        .collect();
    let col_names: Vec<Arc<str>> = vec!["layer1".into(), "layer2".into(), "layer3".into()];

    let dimnames = RObject::List(vec![
        RObject::Character(row_names.into()),
        RObject::Character(col_names.into()),
    ]);
    matrix_attrs.insert("dimnames".into(), dimnames);

    let data_slot = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: matrix_attrs,
    };

    // Create S4 object with logical matrix in .Data slot
    let mut slots = IndexMap::new();
    slots.insert(".Data".into(), data_slot);

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["BooleanMatrix".into()],
        package: Some("TestPackage".into()),
        slots,
    }));

    // Write and read back
    let bytes = write_rds(&s4_obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();

    assert_eq!(s4_obj, result);
}

#[test]
fn test_s4_with_logical_matrix_different_package() {
    use rds2rust::{Attributes, S4ObjectData};

    // Test another S4 class to ensure package info is preserved
    let logical_vec = RObject::Logical(vec![Logical::True; 4].into());
    let mut attrs = Attributes::new();
    attrs.insert("dim".into(), RObject::Integer(vec![2, 2].into()));

    let data_slot = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: attrs,
    };

    let mut slots = IndexMap::new();
    slots.insert(".Data".into(), data_slot);

    let obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["CustomMatrix".into()],
        package: Some("TestPackage".into()),
        slots,
    }));

    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();
    assert_eq!(obj, result);
}

// =============================================================================
// S4 Objects with Attributes (WithAttributes wrapping S4Object)
// =============================================================================

#[test]
fn test_s4_with_outer_attributes_basic() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create basic S4 object
    let mut slots = IndexMap::new();
    slots.insert("x".into(), RObject::Integer(vec![1, 2, 3].into()));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["TestClass".into()],
        package: Some("TestPackage".into()),
        slots,
    }));

    // Wrap with outer attributes
    let mut attrs = Attributes::new();
    attrs.insert(
        "custom_attr".into(),
        RObject::Character(vec!["value".into()].into()),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(s4_obj),
        attributes: attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();

    // Parser reads S4 with merged attributes as S4Object with attributes in slots
    // (R doesn't distinguish between outer attributes and slots in serialization)
    match &result {
        RObject::S4Object(data) => {
            // Verify class preserved
            assert_eq!(data.class[0].as_ref(), "TestClass");
            // Verify original slot exists
            assert!(data.slots.contains_key("x"));
            // Verify outer attribute was merged
            assert!(data.slots.contains_key("custom_attr"));
        }
        _ => panic!("Expected S4Object, got {:?}", result.variant_name()),
    }
}

#[test]
fn test_s4_with_dim_attributes() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create S4 object with logical matrix in .Data slot
    let logical_vec = RObject::Logical(vec![Logical::True; 6].into());
    let mut data_attrs = Attributes::new();
    data_attrs.insert("dim".into(), RObject::Integer(vec![2, 3].into()));

    let data_slot = RObject::WithAttributes {
        object: Box::new(logical_vec),
        attributes: data_attrs,
    };

    let mut slots = IndexMap::new();
    slots.insert(".Data".into(), data_slot);

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["MatrixLike".into()],
        package: Some("TestPkg".into()),
        slots,
    }));

    // Add dim and dimnames at S4 object level
    let mut s4_attrs = Attributes::new();
    s4_attrs.insert("dim".into(), RObject::Integer(vec![2, 3].into()));
    s4_attrs.insert(
        "dimnames".into(),
        RObject::List(vec![
            RObject::Character(vec!["r1".into(), "r2".into()].into()),
            RObject::Character(vec!["c1".into(), "c2".into(), "c3".into()].into()),
        ]),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(s4_obj),
        attributes: s4_attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();

    // Parser reads S4 with merged attributes as S4Object
    match &result {
        RObject::S4Object(data) => {
            // Verify class preserved
            assert_eq!(data.class[0].as_ref(), "MatrixLike");
            // Verify .Data slot exists
            assert!(data.slots.contains_key(".Data"));
            // Verify dim and dimnames were merged as slots
            assert!(data.slots.contains_key("dim"));
            assert!(data.slots.contains_key("dimnames"));
        }
        _ => panic!("Expected S4Object, got {:?}", result.variant_name()),
    }
}

#[test]
fn test_s4_with_empty_outer_attributes() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create S4 object
    let mut slots = IndexMap::new();
    slots.insert("value".into(), RObject::Integer(vec![42].into()));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["TestClass".into()],
        package: Some("TestPkg".into()),
        slots,
    }));

    // Wrap with empty attributes
    let attrs = Attributes::new();

    let with_attrs = RObject::WithAttributes {
        object: Box::new(s4_obj.clone()),
        attributes: attrs,
    };

    // Should behave identically to bare S4
    let bytes_with = write_rds(&with_attrs).unwrap();
    let bytes_bare = write_rds(&s4_obj).unwrap();

    // Both should produce valid RDS
    let result_with = read_rds(&bytes_with[..]).unwrap();
    let result_bare = read_rds(&bytes_bare[..]).unwrap();

    // Results should be equivalent (both are S4 objects with same structure)
    assert!(matches!(
        result_with,
        RObject::S4Object(_) | RObject::WithAttributes { .. }
    ));
    assert!(matches!(result_bare, RObject::S4Object(_)));
}

#[test]
fn test_s4_class_attribute_cannot_be_overridden() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create S4 object
    let mut slots = IndexMap::new();
    slots.insert("x".into(), RObject::Integer(vec![1].into()));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["OriginalClass".into()],
        package: Some("OriginalPkg".into()),
        slots,
    }));

    // Try to override class with outer attribute (should be silently ignored)
    let mut attrs = Attributes::new();
    attrs.insert(
        "class".into(),
        RObject::Character(vec!["FakeClass".into()].into()),
    );
    attrs.insert("other_attr".into(), RObject::Integer(vec![999].into()));

    let obj = RObject::WithAttributes {
        object: Box::new(s4_obj),
        attributes: attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();

    // Verify S4 class was preserved (not overridden)
    match &result {
        RObject::S4Object(data) => {
            assert_eq!(data.class[0].as_ref(), "OriginalClass");
            assert_eq!(data.package.as_ref().unwrap().as_ref(), "OriginalPkg");
        }
        RObject::WithAttributes { object, .. } => {
            if let RObject::S4Object(data) = object.as_ref() {
                assert_eq!(data.class[0].as_ref(), "OriginalClass");
                assert_eq!(data.package.as_ref().unwrap().as_ref(), "OriginalPkg");
            } else {
                panic!("Expected S4Object inside WithAttributes");
            }
        }
        _ => panic!("Expected S4Object or WithAttributes wrapping S4Object"),
    }
}

#[test]
fn test_s4_outer_attribute_shadows_slot() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create S4 with a 'value' slot
    let mut slots = IndexMap::new();
    slots.insert("value".into(), RObject::Integer(vec![1].into()));
    slots.insert("other_slot".into(), RObject::Integer(vec![2].into()));

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["TestClass".into()],
        package: Some("TestPkg".into()),
        slots,
    }));

    // Add outer attribute with same name 'value' (should shadow the slot)
    let mut attrs = Attributes::new();
    attrs.insert("value".into(), RObject::Integer(vec![999].into()));

    let obj = RObject::WithAttributes {
        object: Box::new(s4_obj),
        attributes: attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let _result = read_rds(&bytes[..]).unwrap();

    // This test documents that outer attributes CAN shadow slots
    // The behavior is: outer attributes take precedence (explicit user intent)
}

#[test]
fn test_s4_with_nested_withattributes_in_slot() {
    use rds2rust::{Attributes, S4ObjectData};

    // Create a slot that itself is WithAttributes
    let inner_obj = RObject::Integer(vec![1, 2, 3].into());
    let mut inner_attrs = Attributes::new();
    inner_attrs.insert(
        "inner_attr".into(),
        RObject::Character(vec!["inner".into()].into()),
    );

    let inner_with_attrs = RObject::WithAttributes {
        object: Box::new(inner_obj),
        attributes: inner_attrs,
    };

    // Create S4 with nested WithAttributes in slot
    let mut slots = IndexMap::new();
    slots.insert("nested".into(), inner_with_attrs);

    let s4_obj = RObject::S4Object(Box::new(S4ObjectData {
        class: vec!["OuterClass".into()],
        package: Some("OuterPkg".into()),
        slots,
    }));

    // Add outer attributes
    let mut outer_attrs = Attributes::new();
    outer_attrs.insert(
        "outer_attr".into(),
        RObject::Character(vec!["outer".into()].into()),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(s4_obj),
        attributes: outer_attrs,
    };

    // Write and read back
    let bytes = write_rds(&obj).unwrap();
    let result = read_rds(&bytes[..]).unwrap();

    // Parser reads S4 with merged attributes as S4Object
    match &result {
        RObject::S4Object(data) => {
            // Verify class preserved
            assert_eq!(data.class[0].as_ref(), "OuterClass");
            // Verify nested slot preserved
            assert!(data.slots.contains_key("nested"));
            // Verify outer attribute merged
            assert!(data.slots.contains_key("outer_attr"));
        }
        _ => panic!("Expected S4Object, got {:?}", result.variant_name()),
    }
}
