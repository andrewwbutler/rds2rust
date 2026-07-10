#![cfg(not(target_arch = "wasm32"))]
#![cfg(not(target_arch = "wasm32"))]

//! Tests for column order preservation and Symbol handling.
//!
//! These tests ensure that:
//! 1. DataFrame column order is preserved through read/write cycles
//! 2. S4 object slot order is preserved through read/write cycles
//! 3. Symbol variant correctly handles R's special NULL marker

use indexmap::IndexMap;
use rds2rust::{DataFrameData, RObject, S4ObjectData};
use std::sync::Arc;

// =============================================================================
// DataFrame Column Order Tests
// =============================================================================

#[test]
fn test_dataframe_column_order_preservation() {
    // Create a DataFrame with specific column order
    let mut columns = IndexMap::new();
    columns.insert(
        Arc::from("vst.mean"),
        RObject::Real(vec![1.0, 2.0, 3.0].into()),
    );
    columns.insert(
        Arc::from("vst.variance"),
        RObject::Real(vec![0.5, 1.5, 2.5].into()),
    );
    columns.insert(
        Arc::from("vst.variance.expected"),
        RObject::Real(vec![0.3, 1.3, 2.3].into()),
    );
    columns.insert(
        Arc::from("vst.variance.standardized"),
        RObject::Real(vec![0.8, 1.8, 2.8].into()),
    );
    columns.insert(
        Arc::from("vst.variable"),
        RObject::Logical(
            vec![
                rds2rust::Logical::True,
                rds2rust::Logical::False,
                rds2rust::Logical::True,
            ]
            .into(),
        ),
    );

    let row_names = vec![
        Some(Arc::from("1")),
        Some(Arc::from("2")),
        Some(Arc::from("3")),
    ];

    let df = RObject::DataFrame(Box::new(DataFrameData {
        columns: columns.clone(),
        row_names,
    }));

    // Write and read back
    let serialized = rds2rust::write_rds(&df).expect("Failed to write dataframe");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read dataframe")
        .object;

    // Verify column order is preserved
    match deserialized {
        RObject::DataFrame(data) => {
            let col_names: Vec<_> = data.columns.keys().map(|k| k.as_ref()).collect();
            assert_eq!(col_names.len(), 5);
            assert_eq!(col_names[0], "vst.mean");
            assert_eq!(col_names[1], "vst.variance");
            assert_eq!(col_names[2], "vst.variance.expected");
            assert_eq!(col_names[3], "vst.variance.standardized");
            assert_eq!(col_names[4], "vst.variable");
        }
        _ => panic!("Expected DataFrame after deserialization"),
    }
}

#[test]
fn test_dataframe_column_order_with_many_columns() {
    // Test with many columns to ensure order is preserved even with larger DataFrames
    let mut columns = IndexMap::new();

    // Create columns in specific alphabetically-scrambled order
    let column_names = vec![
        "zebra", "alpha", "mike", "charlie", "delta", "echo", "foxtrot", "bravo", "hotel", "golf",
    ];

    for name in &column_names {
        columns.insert(Arc::from(*name), RObject::Integer(vec![1, 2, 3].into()));
    }

    let row_names = vec![
        Some(Arc::from("1")),
        Some(Arc::from("2")),
        Some(Arc::from("3")),
    ];

    let df = RObject::DataFrame(Box::new(DataFrameData {
        columns: columns.clone(),
        row_names,
    }));

    // Write and read back
    let serialized = rds2rust::write_rds(&df).expect("Failed to write dataframe");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read dataframe")
        .object;

    // Verify exact column order is preserved
    match deserialized {
        RObject::DataFrame(data) => {
            let col_names: Vec<_> = data.columns.keys().map(|k| k.as_ref()).collect();
            assert_eq!(col_names.len(), column_names.len());
            for (i, expected_name) in column_names.iter().enumerate() {
                assert_eq!(
                    col_names[i], *expected_name,
                    "Column order mismatch at index {}: expected '{}', got '{}'",
                    i, expected_name, col_names[i]
                );
            }
        }
        _ => panic!("Expected DataFrame after deserialization"),
    }
}

// =============================================================================
// S4 Object Slot Order Tests
// =============================================================================

#[test]
fn test_s4_slot_order_preservation() {
    // Create an S4 object with specific slot order
    let mut slots = IndexMap::new();
    slots.insert(Arc::from("counts"), RObject::Integer(vec![1, 2, 3].into()));
    slots.insert(Arc::from("data"), RObject::Real(vec![1.0, 2.0, 3.0].into()));
    slots.insert(Arc::from("slot.value"), RObject::List(vec![]));
    slots.insert(
        Arc::from("slot.orig"),
        RObject::Character(vec![Some(Arc::from("payload"))].into()),
    );
    slots.insert(Arc::from("meta.features"), RObject::List(vec![]));
    slots.insert(Arc::from("misc"), RObject::List(vec![]));

    let s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("ExampleClass")],
        package: Some(Arc::from("ExamplePkg")),
        slots: slots.clone(),
    }));

    // Write and read back
    let serialized = rds2rust::write_rds(&s4).expect("Failed to write S4 object");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read S4 object")
        .object;

    // Verify slot order is preserved
    match deserialized {
        RObject::S4Object(data) => {
            let slot_names: Vec<_> = data.slots.keys().map(|k| k.as_ref()).collect();
            assert_eq!(slot_names.len(), 6);
            assert_eq!(slot_names[0], "counts");
            assert_eq!(slot_names[1], "data");
            assert_eq!(slot_names[2], "slot.value");
            assert_eq!(slot_names[3], "slot.orig");
            assert_eq!(slot_names[4], "meta.features");
            assert_eq!(slot_names[5], "misc");
        }
        _ => panic!("Expected S4Object after deserialization"),
    }
}

// =============================================================================
// Symbol Variant Tests for NULL Markers
// =============================================================================

#[test]
fn test_symbol_null_marker() {
    // Test that the special NULL marker is represented as Symbol variant
    let null_marker = RObject::Symbol(Arc::from("\x01NULL\x01"));

    // Write and read back
    let serialized = rds2rust::write_rds(&null_marker).expect("Failed to write Symbol");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read Symbol")
        .object;

    // Verify it's still a Symbol
    match deserialized {
        RObject::Symbol(name) => {
            assert_eq!(name.as_ref(), "\x01NULL\x01");
        }
        _ => panic!(
            "Expected Symbol after deserialization, got {:?}",
            deserialized
        ),
    }
}

#[test]
fn test_symbol_in_s4_slot() {
    // Test Symbol variant in an S4 object slot (like slot.orig)
    let mut slots = IndexMap::new();
    slots.insert(
        Arc::from("slot.orig"),
        RObject::Symbol(Arc::from("\x01NULL\x01")),
    );
    slots.insert(Arc::from("data"), RObject::Integer(vec![1, 2, 3].into()));

    let s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("ExampleClass")],
        package: Some(Arc::from("ExamplePkg")),
        slots: slots.clone(),
    }));

    // Write and read back
    let serialized = rds2rust::write_rds(&s4).expect("Failed to write S4 object with Symbol");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read S4 object with Symbol")
        .object;

    // Verify Symbol is preserved in slot
    match deserialized {
        RObject::S4Object(data) => match data.slots.get(&Arc::from("slot.orig")) {
            Some(RObject::Symbol(name)) => {
                assert_eq!(name.as_ref(), "\x01NULL\x01");
            }
            Some(other) => panic!("Expected Symbol in slot.orig slot, got {:?}", other),
            None => panic!("Missing slot.orig slot"),
        },
        _ => panic!("Expected S4Object after deserialization"),
    }
}

#[test]
fn test_symbol_vs_character_distinction() {
    // Verify that regular character vectors don't get confused with Symbols
    let char_vec = RObject::Character(vec![Some(Arc::from("regular_string"))].into());
    let symbol = RObject::Symbol(Arc::from("\x01NULL\x01"));

    // Write and read both
    let serialized_char = rds2rust::write_rds(&char_vec).expect("Failed to write Character");
    let serialized_symbol = rds2rust::write_rds(&symbol).expect("Failed to write Symbol");

    let deserialized_char = rds2rust::read_rds(&serialized_char)
        .expect("Failed to read Character")
        .object;
    let deserialized_symbol = rds2rust::read_rds(&serialized_symbol)
        .expect("Failed to read Symbol")
        .object;

    // Verify types are correct
    match deserialized_char {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].as_deref(), Some("regular_string"));
        }
        _ => panic!("Expected Character after deserialization"),
    }

    match deserialized_symbol {
        RObject::Symbol(name) => {
            assert_eq!(name.as_ref(), "\x01NULL\x01");
        }
        _ => panic!("Expected Symbol after deserialization"),
    }
}

// =============================================================================
// Combined Tests
// =============================================================================

#[test]
fn test_dataframe_with_ordered_columns_as_s4_slot() {
    // Test a realistic scenario: data.frame as S4 slot with preserved column order
    let mut df_columns = IndexMap::new();
    df_columns.insert(
        Arc::from("gene_name"),
        RObject::Character(vec![Some(Arc::from("GENE1")), Some(Arc::from("GENE2"))].into()),
    );
    df_columns.insert(Arc::from("mean_expr"), RObject::Real(vec![5.2, 3.1].into()));
    df_columns.insert(Arc::from("variance"), RObject::Real(vec![1.5, 0.8].into()));

    let df = RObject::DataFrame(Box::new(DataFrameData {
        columns: df_columns,
        row_names: vec![Some(Arc::from("1")), Some(Arc::from("2"))],
    }));

    // Wrap DataFrame in S4 object
    let mut slots = IndexMap::new();
    slots.insert(Arc::from("meta.features"), df);
    slots.insert(
        Arc::from("slot.orig"),
        RObject::Symbol(Arc::from("\x01NULL\x01")),
    );

    let s4 = RObject::S4Object(Box::new(S4ObjectData {
        class: vec![Arc::from("TestClass")],
        package: Some(Arc::from("TestPackage")),
        slots,
    }));

    // Write and read back
    let serialized = rds2rust::write_rds(&s4).expect("Failed to write S4 with DataFrame");
    let deserialized = rds2rust::read_rds(&serialized)
        .expect("Failed to read S4 with DataFrame")
        .object;

    // Verify both slot order and DataFrame column order are preserved
    match deserialized {
        RObject::S4Object(data) => {
            // Check slot order
            let slot_names: Vec<_> = data.slots.keys().map(|k| k.as_ref()).collect();
            assert_eq!(slot_names[0], "meta.features");
            assert_eq!(slot_names[1], "slot.orig");

            // Check DataFrame column order
            match data.slots.get(&Arc::from("meta.features")) {
                Some(RObject::DataFrame(df_data)) => {
                    let col_names: Vec<_> = df_data.columns.keys().map(|k| k.as_ref()).collect();
                    assert_eq!(col_names.len(), 3);
                    assert_eq!(col_names[0], "gene_name");
                    assert_eq!(col_names[1], "mean_expr");
                    assert_eq!(col_names[2], "variance");
                }
                _ => panic!("Expected DataFrame in meta.features slot"),
            }

            // Check Symbol in slot.orig
            match data.slots.get(&Arc::from("slot.orig")) {
                Some(RObject::Symbol(name)) => {
                    assert_eq!(name.as_ref(), "\x01NULL\x01");
                }
                _ => panic!("Expected Symbol in slot.orig slot"),
            }
        }
        _ => panic!("Expected S4Object after deserialization"),
    }
}
