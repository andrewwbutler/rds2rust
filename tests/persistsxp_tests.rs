//! Tests for PERSISTSXP entries written by `serialize(refhook=)`.
//!
//! R's lazy-load databases (e.g. `help/<pkg>.rdb`) persist srcfile
//! environments as "env::N" strings via a ref hook. The PERSISTSXP payload
//! must be consumed during parsing, otherwise the cursor desynchronizes and
//! every object after the first persisted one is corrupted.

use rds2rust::{
    read_rds, traverse_rds_streaming, ObjectPath, ParseConfig, RObject, RdsVisitor, VisitAction,
};
use std::path::Path;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn read_test_file(filename: &str) -> Vec<u8> {
    let path = format!("tests/data/{}", filename);
    std::fs::read(&path).unwrap_or_else(|_| panic!("Failed to read test file: {}", path))
}

/// Unwrap attribute wrappers and shared references down to the plain object.
fn unwrap_value(obj: &RObject) -> RObject {
    match obj {
        RObject::WithAttributes { object, .. } => unwrap_value(object),
        RObject::Shared(shared) => {
            let inner = shared.read().expect("shared object lock poisoned");
            unwrap_value(&inner)
        }
        other => other.clone(),
    }
}

fn attr_of<'a>(obj: &'a RObject, name: &str) -> Option<&'a RObject> {
    if let RObject::WithAttributes { attributes, object } = obj {
        if let Some(value) = attributes.get(name) {
            return Some(value);
        }
        return attr_of(object, name);
    }
    None
}

fn character_values(obj: &RObject) -> Vec<String> {
    match unwrap_value(obj) {
        RObject::Character(values) => values.to_strings_with_na("<NA>"),
        other => panic!("expected character vector, got {:?}", other),
    }
}

#[test]
fn test_persistsxp_stream_alignment() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // list(first = env, second = env, after = "still-aligned") serialized
    // with refhook = function(e) "env::1"
    let data = read_test_file("persistsxp.rds");
    let obj = read_rds(&data)
        .expect("failed to parse persistsxp.rds")
        .object;

    let names = attr_of(&obj, "names").expect("list names should survive");
    assert_eq!(character_values(names), ["first", "second", "after"]);

    let items = match unwrap_value(&obj) {
        RObject::List(items) => items,
        other => panic!("expected list, got {:?}", other),
    };
    assert_eq!(items.len(), 3);

    // The persisted environment surfaces as the ref-hook strings.
    assert_eq!(character_values(&items[0]), ["env::1"]);

    // The ref hook takes precedence over back-references, so the second
    // occurrence of the same environment is another PERSISTSXP entry whose
    // payload must be consumed as well.
    assert_eq!(character_values(&items[1]), ["env::1"]);

    // The critical assertion: objects after the persisted entry parse
    // correctly, proving the PERSISTSXP payload was consumed.
    assert_eq!(character_values(&items[2]), ["still-aligned"]);
}

#[test]
fn test_persistsxp_in_attributes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // structure(list("payload"), srcenv = env, tail_attr = "tail"), the same
    // shape as srcref/srcfile attributes on Rd objects in help databases.
    let data = read_test_file("persistsxp_attr.rds");
    let obj = read_rds(&data)
        .expect("failed to parse persistsxp_attr.rds")
        .object;

    let srcenv = attr_of(&obj, "srcenv").expect("srcenv attribute should survive");
    assert_eq!(character_values(srcenv), ["env::1"]);

    let tail_attr = attr_of(&obj, "tail_attr").expect("tail_attr attribute should survive");
    assert_eq!(character_values(tail_attr), ["tail"]);

    let items = match unwrap_value(&obj) {
        RObject::List(items) => items,
        other => panic!("expected list, got {:?}", other),
    };
    assert_eq!(items.len(), 1);
    assert_eq!(character_values(&items[0]), ["payload"]);
}

#[test]
fn test_persistsxp_multiple_strings() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // list(env = env, after = "multi-aligned") serialized with
    // refhook = function(e) c("env", "extra", "names")
    let data = read_test_file("persistsxp_multi.rds");
    let obj = read_rds(&data)
        .expect("failed to parse persistsxp_multi.rds")
        .object;

    let items = match unwrap_value(&obj) {
        RObject::List(items) => items,
        other => panic!("expected list, got {:?}", other),
    };
    assert_eq!(items.len(), 2);
    assert_eq!(character_values(&items[0]), ["env", "extra", "names"]);
    assert_eq!(character_values(&items[1]), ["multi-aligned"]);
}

#[derive(Default)]
struct CollectingVisitor {
    vector_metadata: Vec<(rds2rust::VectorKind, usize)>,
    attr_keys: Vec<String>,
}

impl RdsVisitor for CollectingVisitor {
    type Error = std::convert::Infallible;

    fn on_object_start(
        &mut self,
        _path: &ObjectPath,
        _obj_type: &str,
    ) -> Result<VisitAction, Self::Error> {
        Ok(VisitAction::Continue)
    }

    fn on_vector_metadata(
        &mut self,
        _path: &ObjectPath,
        vec_type: rds2rust::VectorKind,
        len: usize,
    ) -> Result<(), Self::Error> {
        self.vector_metadata.push((vec_type, len));
        Ok(())
    }

    fn on_attributes(
        &mut self,
        _path: &ObjectPath,
        attrs: &rds2rust::Attributes,
    ) -> Result<(), Self::Error> {
        self.attr_keys
            .extend(attrs.iter().map(|(key, _)| key.to_string()));
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_persistsxp_streaming_alignment() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = rds2rust::MmapRdsSource::from_path(Path::new("tests/data/persistsxp.rds"))
        .expect("failed to open persistsxp.rds");
    let mut visitor = CollectingVisitor::default();
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("streaming traversal must consume PERSISTSXP payloads");

    // With the PERSISTSXP payload consumed, the traversal sees the trailing
    // "still-aligned" element (length 1) and the list names (length 3).
    // A desynchronized stream misreads both and drops the names attribute.
    assert_eq!(
        visitor.vector_metadata,
        [
            (rds2rust::VectorKind::Character, 1),
            (rds2rust::VectorKind::Character, 3),
        ]
    );
    assert_eq!(visitor.attr_keys, ["names"]);
}
