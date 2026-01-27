//! Integration tests for the Attributes API.
//!
//! These tests verify that users can construct RObject::WithAttributes
//! programmatically using the exported Attributes struct.

use rds2rust::{read_rds, write_rds, Attributes, RObject};
use std::sync::Arc;

// =============================================================================
// Named List Construction Tests
// =============================================================================

#[test]
fn test_construct_named_list() {
    // Create a list with three elements
    let list = RObject::List(vec![
        RObject::Real(vec![1.0].into()),
        RObject::Real(vec![2.0].into()),
        RObject::Real(vec![3.0].into()),
    ]);

    // Create names attribute
    let names = RObject::Character(
        vec![Arc::from("first"), Arc::from("second"), Arc::from("third")].into(),
    );

    // Construct attributes using the public API
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), names);

    // Create the named list
    let named_list = RObject::WithAttributes {
        object: Box::new(list),
        attributes: attrs,
    };

    // Verify the structure
    match named_list {
        RObject::WithAttributes { object, attributes } => {
            // Check the list content
            match *object {
                RObject::List(ref items) => {
                    assert_eq!(items.len(), 3);
                }
                _ => panic!("Expected List"),
            }

            // Check the names attribute
            let names_attr = attributes.get("names");
            assert!(names_attr.is_some(), "Expected 'names' attribute");

            match names_attr.unwrap() {
                RObject::Character(ref names_vec) => {
                    assert_eq!(names_vec.len(), 3);
                    assert_eq!(names_vec[0].as_ref(), "first");
                    assert_eq!(names_vec[1].as_ref(), "second");
                    assert_eq!(names_vec[2].as_ref(), "third");
                }
                _ => panic!("Expected Character vector for names"),
            }
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_named_list_roundtrip() {
    // Create a named list
    let list = RObject::List(vec![
        RObject::Integer(vec![10, 20].into()),
        RObject::Character(vec![Arc::from("hello"), Arc::from("world")].into()),
    ]);

    let names = RObject::Character(vec![Arc::from("numbers"), Arc::from("words")].into());

    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), names);

    let named_list = RObject::WithAttributes {
        object: Box::new(list),
        attributes: attrs,
    };

    // Write and read back
    let serialized = write_rds(&named_list).expect("Failed to write named list");
    let deserialized = read_rds(&serialized)
        .expect("Failed to read named list")
        .object;

    // Verify it matches
    assert_eq!(named_list, deserialized);
}

// =============================================================================
// Named Vector Construction Tests
// =============================================================================

#[test]
fn test_construct_named_integer_vector() {
    // Create an integer vector
    let vec = RObject::Integer(vec![100, 200, 300].into());

    // Create names
    let names = RObject::Character(vec![Arc::from("a"), Arc::from("b"), Arc::from("c")].into());

    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), names);

    let named_vec = RObject::WithAttributes {
        object: Box::new(vec),
        attributes: attrs,
    };

    // Verify structure
    match named_vec {
        RObject::WithAttributes { object, attributes } => {
            match *object {
                RObject::Integer(ref v) => {
                    assert_eq!(v, &vec![100, 200, 300]);
                }
                _ => panic!("Expected Integer vector"),
            }

            assert_eq!(attributes.iter().count(), 1);
            assert!(attributes.get("names").is_some());
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_named_vector_roundtrip() {
    let vec = RObject::Real(vec![1.5, 2.5, 3.5].into());
    let names = RObject::Character(vec![Arc::from("x"), Arc::from("y"), Arc::from("z")].into());

    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), names);

    let named_vec = RObject::WithAttributes {
        object: Box::new(vec),
        attributes: attrs,
    };

    let serialized = write_rds(&named_vec).expect("Failed to write");
    let deserialized = read_rds(&serialized).expect("Failed to read").object;

    assert_eq!(named_vec, deserialized);
}

// =============================================================================
// Multiple Attributes Tests
// =============================================================================

#[test]
fn test_object_with_multiple_attributes() {
    // Create a vector with multiple custom attributes
    let vec = RObject::Integer(vec![1, 2, 3, 4, 5, 6].into());

    let mut attrs = Attributes::new();

    // Add names attribute
    attrs.insert(
        Arc::from("names"),
        RObject::Character(
            vec![
                Arc::from("a"),
                Arc::from("b"),
                Arc::from("c"),
                Arc::from("d"),
                Arc::from("e"),
                Arc::from("f"),
            ]
            .into(),
        ),
    );

    // Add dim attribute to make it a 2x3 matrix
    attrs.insert(Arc::from("dim"), RObject::Integer(vec![2, 3].into()));

    // Add a custom attribute
    attrs.insert(
        Arc::from("description"),
        RObject::Character(vec![Arc::from("test matrix")].into()),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(vec),
        attributes: attrs,
    };

    // Verify all attributes are present
    match obj {
        RObject::WithAttributes {
            object: _,
            attributes,
        } => {
            assert_eq!(attributes.iter().count(), 3);
            assert!(attributes.get("names").is_some());
            assert!(attributes.get("dim").is_some());
            assert!(attributes.get("description").is_some());
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_multiple_attributes_roundtrip() {
    let vec = RObject::Real(vec![1.0, 2.0, 3.0, 4.0].into());

    let mut attrs = Attributes::new();
    attrs.insert(
        Arc::from("names"),
        RObject::Character(
            vec![
                Arc::from("w"),
                Arc::from("x"),
                Arc::from("y"),
                Arc::from("z"),
            ]
            .into(),
        ),
    );
    attrs.insert(Arc::from("version"), RObject::Integer(vec![1].into()));
    attrs.insert(
        Arc::from("author"),
        RObject::Character(vec![Arc::from("test")].into()),
    );

    let obj = RObject::WithAttributes {
        object: Box::new(vec.clone()),
        attributes: attrs,
    };

    let serialized = write_rds(&obj).expect("Failed to write");
    let deserialized = read_rds(&serialized).expect("Failed to read").object;

    // Verify the object and attributes separately since attribute order may vary
    match deserialized {
        RObject::WithAttributes { object, attributes } => {
            match *object {
                RObject::Real(ref v) => assert_eq!(*v, vec![1.0, 2.0, 3.0, 4.0]),
                _ => panic!("Expected Real"),
            }
            assert_eq!(attributes.iter().count(), 3);

            // Verify each attribute exists with correct value
            match attributes.get("names").unwrap() {
                RObject::Character(ref v) => {
                    assert_eq!(
                        v,
                        &vec![
                            Arc::from("w"),
                            Arc::from("x"),
                            Arc::from("y"),
                            Arc::from("z")
                        ]
                    );
                }
                _ => panic!("Expected Character for names"),
            }
            match attributes.get("version").unwrap() {
                RObject::Integer(ref v) => assert_eq!(v, &vec![1]),
                _ => panic!("Expected Integer for version"),
            }
            match attributes.get("author").unwrap() {
                RObject::Character(ref v) => assert_eq!(v, &vec![Arc::from("test")]),
                _ => panic!("Expected Character for author"),
            }
        }
        _ => panic!("Expected WithAttributes"),
    }
}

// =============================================================================
// Attributes Helper Methods Tests
// =============================================================================

#[test]
fn test_attributes_new() {
    let attrs = Attributes::new();
    assert!(attrs.is_empty());
    assert_eq!(attrs.iter().count(), 0);
}

#[test]
fn test_attributes_insert_and_get() {
    let mut attrs = Attributes::new();

    attrs.insert(Arc::from("key1"), RObject::Integer(vec![1].into()));
    attrs.insert(
        Arc::from("key2"),
        RObject::Character(vec![Arc::from("value")].into()),
    );

    assert!(!attrs.is_empty());
    assert_eq!(attrs.iter().count(), 2);

    let val1 = attrs.get("key1");
    assert!(val1.is_some());
    match val1.unwrap() {
        RObject::Integer(ref v) => assert_eq!(v, &vec![1]),
        _ => panic!("Expected Integer"),
    }

    let val2 = attrs.get("key2");
    assert!(val2.is_some());
}

#[test]
fn test_attributes_update_existing_key() {
    let mut attrs = Attributes::new();

    attrs.insert(Arc::from("key"), RObject::Integer(vec![1].into()));
    assert_eq!(attrs.iter().count(), 1);

    // Update the same key
    attrs.insert(Arc::from("key"), RObject::Integer(vec![2].into()));
    assert_eq!(attrs.iter().count(), 1); // Should still be 1, not 2

    match attrs.get("key").unwrap() {
        RObject::Integer(ref v) => assert_eq!(v, &vec![2]),
        _ => panic!("Expected Integer with value 2"),
    }
}

#[test]
fn test_attributes_iter() {
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("a"), RObject::Integer(vec![1].into()));
    attrs.insert(Arc::from("b"), RObject::Integer(vec![2].into()));
    attrs.insert(Arc::from("c"), RObject::Integer(vec![3].into()));

    let keys: Vec<&str> = attrs.iter().map(|(k, _)| k.as_ref()).collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"a"));
    assert!(keys.contains(&"b"));
    assert!(keys.contains(&"c"));
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_attributes_construction() {
    // Test that we can construct a WithAttributes with empty attributes
    // (though this is not the recommended approach - just use the plain object)
    let vec = RObject::Integer(vec![1, 2, 3].into());
    let attrs = Attributes::new();

    let obj = RObject::WithAttributes {
        object: Box::new(vec),
        attributes: attrs,
    };

    // Verify structure
    match obj {
        RObject::WithAttributes { object, attributes } => {
            match *object {
                RObject::Integer(ref v) => assert_eq!(v, &vec![1, 2, 3]),
                _ => panic!("Expected Integer"),
            }
            assert!(attributes.is_empty());
        }
        _ => panic!("Expected WithAttributes"),
    }
}

#[test]
fn test_nested_with_attributes() {
    // Create a list that contains objects with attributes
    let named_vec = {
        let vec = RObject::Integer(vec![1, 2].into());
        let mut attrs = Attributes::new();
        attrs.insert(
            Arc::from("names"),
            RObject::Character(vec![Arc::from("x"), Arc::from("y")].into()),
        );
        RObject::WithAttributes {
            object: Box::new(vec),
            attributes: attrs,
        }
    };

    let list = RObject::List(vec![named_vec, RObject::Null]);

    // Add names to the list itself
    let mut list_attrs = Attributes::new();
    list_attrs.insert(
        Arc::from("names"),
        RObject::Character(vec![Arc::from("item1"), Arc::from("item2")].into()),
    );

    let named_list = RObject::WithAttributes {
        object: Box::new(list),
        attributes: list_attrs,
    };

    // Roundtrip test
    let serialized = write_rds(&named_list).expect("Failed to write");
    let deserialized = read_rds(&serialized).expect("Failed to read").object;

    assert_eq!(named_list, deserialized);
}
