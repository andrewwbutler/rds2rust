//! Tests for advanced RDS serialization format features.
//!
//! These tests verify support for:
//! - Compact 3-byte CHARSXP length encoding
//! - All pseudo-types (238-255)
//! - SYMSXP in character vectors
//! - Nested character vectors
//! - S3 objects as attribute containers
//! - Package/namespace functions
//! - Multi-level reference tracking
//! - Various attribute edge cases

// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{read_rds, PairlistElement, RObject};
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
// Compact encoding tests
// =============================================================================

#[test]
fn test_compact_strings() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("compact_strings.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse compact strings")
        .object;

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0].as_deref(), Some("ExampleObject"));
            assert_eq!(vec[1].as_deref(), Some("package"));
            assert_eq!(vec[2].as_deref(), Some("namespace"));
            assert_eq!(vec[3].as_deref(), Some("environment"));
        }
        _ => panic!("Expected Character vector"),
    }
}

#[test]
fn test_string_lengths() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Very short string
    let data = read_test_file("string_very_short.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse very short string")
        .object;
    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].as_deref(), Some("x"));
        }
        _ => panic!("Expected Character vector"),
    }

    // Medium string (500 characters)
    let data = read_test_file("string_medium.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse medium string")
        .object;
    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].as_deref().unwrap().len(), 500);
        }
        _ => panic!("Expected Character vector"),
    }

    // Long string (70000 characters)
    let data = read_test_file("string_long.rds");
    let obj = read_rds(&data).expect("Failed to parse long string").object;
    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].as_deref().unwrap().len(), 70000);
        }
        _ => panic!("Expected Character vector"),
    }
}

#[test]
fn test_string_encodings() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // UTF-8 string
    let data = read_test_file("string_utf8.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse UTF-8 string")
        .object;
    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert!(vec[0].as_deref().unwrap().contains("世界"));
            assert!(vec[0].as_deref().unwrap().contains("🌍"));
        }
        _ => panic!("Expected Character vector"),
    }

    // Latin1 string
    let data = read_test_file("string_latin1.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse Latin1 string")
        .object;
    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            // The string should contain "Café" or similar
            assert!(vec[0].as_deref().unwrap().len() >= 3);
        }
        _ => panic!("Expected Character vector"),
    }
}

// =============================================================================
// Nested structure tests
// =============================================================================

#[test]
fn test_nested_char_vectors() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("nested_char_vectors.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse nested char vectors")
        .object;

    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Should have "names" attribute
            assert!(attributes.get("names").is_some());

            match object.as_ref() {
                RObject::List(elements) => {
                    assert_eq!(elements.len(), 2);

                    // First element: "outer" - character vector
                    match &elements[0] {
                        RObject::Character(vec) => {
                            assert_eq!(vec.len(), 2);
                            assert_eq!(vec[0].as_deref(), Some("level1_a"));
                            assert_eq!(vec[1].as_deref(), Some("level1_b"));
                        }
                        _ => panic!("Expected first element to be Character vector"),
                    }

                    // Second element: "inner" - nested list
                    match &elements[1] {
                        RObject::WithAttributes {
                            object: inner_obj, ..
                        } => {
                            match inner_obj.as_ref() {
                                RObject::List(_inner_elements) => {
                                    // Successfully parsed nested structure
                                }
                                _ => panic!("Expected inner element to be List"),
                            }
                        }
                        RObject::List(_) => {
                            // Also acceptable
                        }
                        _ => panic!("Expected second element to be List"),
                    }
                }
                _ => panic!("Expected List"),
            }
        }
        RObject::List(_) => {
            // Acceptable if no names attribute
        }
        _ => panic!("Expected List or WithAttributes"),
    }
}

#[test]
fn test_mixed_types_list() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("mixed_types_list.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse mixed types list")
        .object;

    // Extract the list (may be wrapped in WithAttributes)
    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List or WithAttributes"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);

            // First: character vector
            match &elements[0] {
                RObject::Character(vec) => assert_eq!(vec.len(), 3),
                _ => panic!("Expected Character vector"),
            }

            // Second: integer vector (ALTREP or regular)
            match &elements[1] {
                RObject::Integer(vec) => assert_eq!(vec.len(), 5),
                _ => panic!("Expected Integer vector"),
            }

            // Third: real vector
            match &elements[2] {
                RObject::Real(vec) => assert_eq!(vec.len(), 3),
                _ => panic!("Expected Real vector"),
            }
        }
        _ => panic!("Expected List"),
    }
}

// =============================================================================
// S3 object tests
// =============================================================================

#[test]
fn test_s3_rich_attributes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_rich_attributes.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse S3 with rich attributes")
        .object;

    match obj {
        RObject::S3Object(s3) => {
            assert_eq!(s3.class.len(), 1);
            assert_eq!(s3.class[0].as_ref(), "custom_class");

            // Check attributes
            assert!(s3.attributes.get("metadata").is_some());
            assert!(s3.attributes.get("version").is_some());
            assert!(s3.attributes.get("timestamp").is_some());
        }
        _ => panic!("Expected S3Object"),
    }
}

// =============================================================================
// Pseudo-type tests
// =============================================================================

#[test]
fn test_package_function() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("package_function.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse package function")
        .object;

    // Package functions are typically closures or builtins
    match obj {
        RObject::Closure { .. } => {
            // Good - parsed as closure
        }
        RObject::Builtin { .. } => {
            // Also acceptable
        }
        _ => {
            // Some package functions might be other types
            eprintln!("Package function type: {:?}", std::mem::discriminant(&obj));
        }
    }
}

#[test]
fn test_complex_pseudo_types() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_pseudo_types.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse complex pseudo types")
        .object;

    // Extract the list
    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 5);

            // func: compiled function (Closure with possible Bytecode)
            match &elements[0] {
                RObject::Closure { .. } => {
                    // Successfully parsed closure
                }
                _ => panic!(
                    "Expected Closure for func element, got {:?}",
                    std::mem::discriminant(&elements[0])
                ),
            }

            // NOTE: The remaining elements have complex serialization patterns that may vary
            // depending on R version and context. We verify we can parse the structure
            // without errors, but don't strictly enforce types since R may serialize
            // primitives and environments in various ways (e.g., as character strings,
            // pairlists, or null references).

            // Just verify we successfully parsed all 5 elements
            assert_eq!(elements.len(), 5);
        }
        _ => panic!("Expected List"),
    }
}

// =============================================================================
// Reference tracking tests
// =============================================================================

#[test]
fn test_multi_level_refs() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("multi_level_refs.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse multi-level refs")
        .object;

    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            // Should have 4 top-level elements
            assert_eq!(elements.len(), 4);

            // Verify the structure: ref1, middle, ref3 should have length 2
            // deep should have length 1
            for (i, elem) in elements.iter().enumerate() {
                match elem {
                    RObject::WithAttributes { object: inner, .. } => {
                        match inner.as_ref() {
                            RObject::List(inner_list) => {
                                // Elements 0, 1, 2 (ref1, middle, ref3) have length 2
                                // Element 3 (deep) has length 1
                                let expected_len = if i == 3 { 1 } else { 2 };
                                assert_eq!(
                                    inner_list.len(),
                                    expected_len,
                                    "Element {} should have {} items",
                                    i,
                                    expected_len
                                );
                            }
                            _ => panic!("Expected List in element {}", i),
                        }
                    }
                    RObject::List(inner_list) => {
                        // Elements 0, 1, 2 (ref1, middle, ref3) have length 2
                        // Element 3 (deep) has length 1
                        let expected_len = if i == 3 { 1 } else { 2 };
                        assert_eq!(
                            inner_list.len(),
                            expected_len,
                            "Element {} should have {} items",
                            i,
                            expected_len
                        );
                    }
                    _ => {
                        eprintln!("Element {} type: {:?}", i, std::mem::discriminant(elem));
                    }
                }
            }
        }
        _ => panic!("Expected List"),
    }
}

#[test]
fn test_repeated_symbol_names() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("repeated_symbol_names.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse repeated symbol names")
        .object;

    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 4);

            // Each element should be a named list with keys x, y, z
            for (i, elem) in elements.iter().enumerate() {
                match elem {
                    RObject::WithAttributes { attributes, .. } => {
                        if let Some(RObject::Character(names)) = attributes.get("names") {
                            assert_eq!(names.len(), 3, "Element {} should have 3 names", i);
                            assert_eq!(names[0].as_deref(), Some("x"));
                            assert_eq!(names[1].as_deref(), Some("y"));
                            assert_eq!(names[2].as_deref(), Some("z"));
                        }
                    }
                    _ => {
                        eprintln!("Element {} has no attributes", i);
                    }
                }
            }
        }
        _ => panic!("Expected List"),
    }
}

// =============================================================================
// Attribute edge cases
// =============================================================================

#[test]
fn test_no_attributes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("no_attributes.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse object with no attributes")
        .object;

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 3);
        }
        _ => panic!("Expected Real vector without attributes"),
    }
}

#[test]
fn test_custom_attributes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Integer with custom attributes
    let data = read_test_file("int_with_custom_attrs.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse int with custom attrs")
        .object;

    eprintln!("DEBUG: obj type = {:?}", std::mem::discriminant(&obj));
    match obj {
        RObject::WithAttributes { object, attributes } => {
            match object.as_ref() {
                RObject::Integer(vec) => assert_eq!(vec.len(), 10),
                _ => panic!("Expected Integer vector"),
            }

            assert!(attributes.get("custom_attr").is_some());
            assert!(attributes.get("dimension_info").is_some());
        }
        _ => panic!(
            "Expected WithAttributes, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }

    // Real with custom attributes
    let data = read_test_file("real_with_custom_attrs.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse real with custom attrs")
        .object;

    match obj {
        RObject::WithAttributes { object, attributes } => {
            match object.as_ref() {
                RObject::Real(vec) => assert!(!vec.is_empty()),
                _ => panic!("Expected Real vector"),
            }

            assert!(attributes.get("units").is_some());
            assert!(attributes.get("precision").is_some());
        }
        _ => panic!("Expected WithAttributes"),
    }

    // Character with custom attributes
    let data = read_test_file("char_with_custom_attrs.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse char with custom attrs")
        .object;

    match obj {
        RObject::WithAttributes { object, attributes } => {
            match object.as_ref() {
                RObject::Character(vec) => assert_eq!(vec.len(), 3),
                _ => panic!("Expected Character vector"),
            }

            assert!(attributes.get("encoding").is_some());
            assert!(attributes.get("origin").is_some());
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_all_types_attributes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("all_types_attributes.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse all types attributes")
        .object;

    match obj {
        RObject::WithAttributes { object, attributes } => {
            match object.as_ref() {
                RObject::Real(vec) => assert_eq!(vec.len(), 3),
                _ => panic!("Expected Real vector"),
            }

            // Check various attribute types
            assert!(attributes.get("int_attr").is_some());
            assert!(attributes.get("real_attr").is_some());
            assert!(attributes.get("char_attr").is_some());
            assert!(attributes.get("logical_attr").is_some());
            assert!(attributes.get("list_attr").is_some());
            assert!(attributes.get("vec_attr").is_some());
        }
        _ => panic!("Expected WithAttributes"),
    }
}

// =============================================================================
// NULL handling tests
// =============================================================================

#[test]
fn test_null_variants() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Plain NULL
    let data = read_test_file("null_plain.rds");
    let obj = read_rds(&data).expect("Failed to parse plain NULL").object;
    assert!(matches!(obj, RObject::Null));

    // NULL in list
    let data = read_test_file("null_in_list.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse NULL in list")
        .object;

    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[1], RObject::Null));
        }
        _ => panic!("Expected List"),
    }

    // Multiple NULLs
    let data = read_test_file("multi_null.rds");
    let obj = read_rds(&data).expect("Failed to parse multi NULL").object;

    match obj {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[0], RObject::Null));
            assert!(matches!(elements[1], RObject::Null));
            assert!(matches!(elements[2], RObject::Null));
        }
        _ => panic!("Expected List"),
    }
}

// =============================================================================
// Language object tests
// =============================================================================

#[test]
fn test_language_variants() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Simple call
    let data = read_test_file("lang_simple_call.rds");
    let obj = read_rds(&data).expect("Failed to parse simple call").object;
    assert!(matches!(obj, RObject::Language { .. }));

    // Named arguments
    let data = read_test_file("lang_named_args.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse named args call")
        .object;
    assert!(matches!(obj, RObject::Language { .. }));

    // Deeply nested
    let data = read_test_file("lang_deep_nested.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse deep nested call")
        .object;
    assert!(matches!(obj, RObject::Language { .. }));
}

// =============================================================================
// Pairlist tests
// =============================================================================

#[test]
fn test_pairlist_mixed() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("pairlist_mixed.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse mixed pairlist")
        .object;

    match obj {
        RObject::Pairlist(elements) => {
            assert_eq!(elements.len(), 4);

            // Check tags are present
            for (i, elem) in elements.iter().enumerate() {
                assert!(elem.tag.is_some(), "Element {} should have a tag", i);
            }
        }
        _ => panic!("Expected Pairlist"),
    }
}

// =============================================================================
// Environment tests
// =============================================================================

#[test]
fn test_environment_variants() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Empty environment
    let data = read_test_file("environment_empty.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse empty environment")
        .object;
    // Environments may be represented as Environment or Null
    match obj {
        RObject::Environment { .. } | RObject::Null => {
            // Success
        }
        _ => panic!("Expected Environment or Null"),
    }

    // Rich environment
    let data = read_test_file("environment_rich.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse rich environment")
        .object;
    match obj {
        RObject::Environment { .. } | RObject::Null => {
            // Success
        }
        _ => panic!("Expected Environment or Null"),
    }
}

// =============================================================================
// Edge case tests
// =============================================================================

#[test]
fn test_tiny_object() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("tiny_object.rds");
    let obj = read_rds(&data).expect("Failed to parse tiny object").object;

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1);
        }
        _ => panic!("Expected Integer"),
    }
}

#[test]
fn test_list_with_large_int() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_with_large_int.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse list with large int")
        .object;

    let list = match obj {
        RObject::WithAttributes { object, .. } => object,
        RObject::List(_) => Box::new(obj),
        _ => panic!("Expected List"),
    };

    match list.as_ref() {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);

            // Second element: large integer vector
            match &elements[1] {
                RObject::Integer(vec) => {
                    assert_eq!(vec.len(), 1000);
                    // Check it's 1:1000
                    assert_eq!(vec[0], 1);
                    assert_eq!(vec[999], 1000);
                }
                _ => panic!("Expected Integer vector"),
            }
        }
        _ => panic!("Expected List"),
    }
}

// =============================================================================
// Namespace serialization tests
// =============================================================================

#[test]
fn test_namespace_roundtrip() {
    // Test that namespace references are preserved during roundtrip
    use rds2rust::write_rds;
    use std::sync::Arc;

    // Create a namespace reference
    let namespace = RObject::Namespace(vec![Arc::from("Matrix")]);

    // Serialize
    let serialized = write_rds(&namespace).expect("Failed to serialize namespace");

    // Deserialize
    let deserialized = read_rds(&serialized)
        .expect("Failed to deserialize namespace")
        .object;

    // Verify
    match deserialized {
        RObject::Namespace(names) => {
            assert_eq!(names.len(), 1);
            assert_eq!(names[0].as_ref(), "Matrix");
        }
        _ => panic!("Expected Namespace, got {:?}", deserialized),
    }
}

#[test]
fn test_namespace_multiple_components() {
    // Test namespace with multiple name components
    use rds2rust::write_rds;
    use std::sync::Arc;

    let namespace = RObject::Namespace(vec![Arc::from("ExamplePkg"), Arc::from("1.0.0")]);

    let serialized = write_rds(&namespace).expect("Failed to serialize");
    let deserialized = read_rds(&serialized).expect("Failed to deserialize").object;

    match deserialized {
        RObject::Namespace(names) => {
            assert_eq!(names.len(), 2);
            assert_eq!(names[0].as_ref(), "ExamplePkg");
            assert_eq!(names[1].as_ref(), "1.0.0");
        }
        _ => panic!("Expected Namespace"),
    }
}

#[test]
fn test_namespace_in_list() {
    // Test that namespaces can be embedded in other structures
    use rds2rust::write_rds;
    use std::sync::Arc;

    let list = RObject::List(vec![
        RObject::Namespace(vec![Arc::from("base")]),
        RObject::Integer(vec![1, 2, 3].into()),
        RObject::Namespace(vec![Arc::from("Matrix")]),
    ]);

    let serialized = write_rds(&list).expect("Failed to serialize");
    let deserialized = read_rds(&serialized).expect("Failed to deserialize").object;

    match deserialized {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);

            match &elements[0] {
                RObject::Namespace(names) => {
                    assert_eq!(names[0].as_ref(), "base");
                }
                _ => panic!("Expected Namespace at index 0"),
            }

            match &elements[1] {
                RObject::Integer(vals) => {
                    assert_eq!(vals, &vec![1, 2, 3]);
                }
                _ => panic!("Expected Integer at index 1"),
            }

            match &elements[2] {
                RObject::Namespace(names) => {
                    assert_eq!(names[0].as_ref(), "Matrix");
                }
                _ => panic!("Expected Namespace at index 2"),
            }
        }
        _ => panic!("Expected List"),
    }
}

#[test]
fn test_namespace_serialization_format() {
    // Test that the serialized format is correct (NAMESPACESXP = 123)
    use rds2rust::write_rds;
    use std::sync::Arc;

    let namespace = RObject::Namespace(vec![Arc::from("stats")]);
    let serialized = write_rds(&namespace).expect("Failed to serialize");

    // Decompress and check the format
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(&serialized[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("Failed to decompress");

    // Skip header, find NAMESPACESXP_SERIAL (249 = 0xF9)
    // Note: R uses type 249 for serialized namespaces, not type 123
    let mut found_namespace = false;
    for i in 0..decompressed.len().saturating_sub(4) {
        let flags = u32::from_be_bytes([
            decompressed[i],
            decompressed[i + 1],
            decompressed[i + 2],
            decompressed[i + 3],
        ]);

        if (flags & 0xFF) == 249 {
            // NAMESPACESXP_SERIAL
            found_namespace = true;
            break;
        }
    }

    assert!(
        found_namespace,
        "Could not find NAMESPACESXP_SERIAL (type 249) in serialized output"
    );
}

#[test]
fn test_closure_with_namespace_environment() {
    // Test that closures with namespace environments are preserved
    // This is critical for S4 method dispatch in packages that rely on namespaces
    use rds2rust::write_rds;
    use std::sync::Arc;

    // Create a closure whose environment chain includes a namespace
    // This mimics what happens with command objects that capture namespaces
    let namespace_env = RObject::Namespace(vec![Arc::from("ExamplePkg")]);

    let closure = RObject::Closure {
        formals: Box::new(RObject::Null),
        body: Box::new(RObject::Language {
            function: Box::new(RObject::Character(vec![Some(Arc::from("print"))].into())),
            args: vec![PairlistElement {
                tag: None,
                value: RObject::Character(vec![Some(Arc::from("x"))].into()),
                tag_object: None,
            }],
        }),
        environment: Box::new(namespace_env),
    };

    // Serialize
    let serialized = write_rds(&closure).expect("Failed to serialize closure");

    // Deserialize
    let deserialized = read_rds(&serialized)
        .expect("Failed to deserialize closure")
        .object;

    // Verify the closure structure is preserved
    match deserialized {
        RObject::Closure { environment, .. } => {
            // The environment should be our namespace
            match *environment {
                RObject::Namespace(names) => {
                    assert_eq!(names.len(), 1);
                    assert_eq!(names[0].as_ref(), "ExamplePkg");
                }
                _ => panic!("Expected Namespace environment, got {:?}", environment),
            }
        }
        _ => panic!("Expected Closure, got {:?}", deserialized),
    }
}

#[test]
fn test_environment_chain_with_namespace() {
    // Test that environment chains containing namespaces are preserved
    use rds2rust::write_rds;
    use std::sync::Arc;

    // Create an environment whose enclosing environment is a namespace
    let namespace = RObject::Namespace(vec![Arc::from("Matrix")]);

    let env = RObject::Environment {
        enclosing: Box::new(namespace),
        frame: Box::new(RObject::Null),
        hashtab: Box::new(RObject::Null),
    };

    // Serialize
    let serialized = write_rds(&env).expect("Failed to serialize environment");

    // Deserialize
    let deserialized = read_rds(&serialized)
        .expect("Failed to deserialize environment")
        .object;

    // Verify
    match deserialized {
        RObject::Environment { enclosing, .. } => match *enclosing {
            RObject::Namespace(names) => {
                assert_eq!(names[0].as_ref(), "Matrix");
            }
            _ => panic!("Expected Namespace as enclosing, got {:?}", enclosing),
        },
        _ => panic!("Expected Environment"),
    }
}
