#![cfg(not(target_arch = "wasm32"))]
//! Streaming traversal tests.

use std::path::{Path, PathBuf};

use rds2rust::{
    read_rds, traverse_rds_streaming, traverse_rds_streaming_with_progress, ObjectPath,
    ParseConfig, RObject, RdsVisitor, StreamingProgress, VectorKind, VisitAction,
};

#[cfg(not(target_arch = "wasm32"))]
use rds2rust::MmapRdsSource;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

#[cfg(not(target_arch = "wasm32"))]
fn read_source(filename: &str) -> MmapRdsSource {
    let path = format!("tests/data/{}", filename);
    MmapRdsSource::from_path(Path::new(&path))
        .unwrap_or_else(|_| panic!("Failed to open test file: {}", path))
}

#[cfg(not(target_arch = "wasm32"))]
fn find_large_fixture() -> Option<(PathBuf, usize)> {
    let dir = Path::new("tests/data");
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name()?.to_string_lossy();
        if !file_name.starts_with("large_int_") || !file_name.ends_with(".rds") {
            continue;
        }
        let len_str = file_name
            .trim_start_matches("large_int_")
            .trim_end_matches(".rds");
        if let Ok(len) = len_str.parse::<usize>() {
            return Some((path, len));
        }
    }
    None
}

#[derive(Default)]
struct CountingVisitor {
    shared_refs: usize,
    shared_targets: usize,
    vector_metadata: usize,
    attrs: usize,
    vector_metadata_before_attrs: Option<bool>,
    skip_root: bool,
}

impl RdsVisitor for CountingVisitor {
    type Error = std::convert::Infallible;

    fn on_object_start(
        &mut self,
        _path: &ObjectPath,
        obj_type: &str,
    ) -> Result<VisitAction, Self::Error> {
        if obj_type == "SharedRef" {
            self.shared_refs += 1;
        }
        if self.skip_root {
            return Ok(VisitAction::Skip);
        }
        Ok(VisitAction::Continue)
    }

    fn on_vector_metadata(
        &mut self,
        _path: &ObjectPath,
        _vec_type: VectorKind,
        _len: usize,
    ) -> Result<(), Self::Error> {
        self.vector_metadata += 1;
        if self.vector_metadata_before_attrs.is_none() {
            self.vector_metadata_before_attrs = Some(true);
        }
        Ok(())
    }

    fn on_shared_reference(
        &mut self,
        _path: &ObjectPath,
        target: Option<&ObjectPath>,
    ) -> Result<(), Self::Error> {
        if target.is_some() {
            self.shared_targets += 1;
        }
        Ok(())
    }

    fn on_attributes(
        &mut self,
        _path: &ObjectPath,
        _attrs: &rds2rust::Attributes,
    ) -> Result<(), Self::Error> {
        self.attrs += 1;
        if self.vector_metadata_before_attrs.is_none() {
            self.vector_metadata_before_attrs = Some(false);
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_emits_shared_refs() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("ref_shared_list.rds");
    let bytes = std::fs::read("tests/data/ref_shared_list.rds")
        .expect("failed to read ref_shared_list.rds");
    let obj = read_rds(&bytes).expect("failed to parse ref_shared_list.rds");
    if !contains_shared(&obj) {
        eprintln!("Skipping test: no shared references detected in ref_shared_list.rds");
        return;
    }
    let mut visitor = CountingVisitor::default();
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("streaming traversal failed");

    assert!(
        visitor.shared_refs > 0,
        "expected SharedRef events in ref_shared_list.rds"
    );
    assert!(
        visitor.shared_targets > 0,
        "expected SharedRef target paths in ref_shared_list.rds"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_skip_root_avoids_metadata() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("list_simple.rds");
    let mut visitor = CountingVisitor {
        skip_root: true,
        ..Default::default()
    };
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("streaming traversal failed");

    assert_eq!(
        visitor.vector_metadata, 0,
        "skip should prevent child metadata callbacks"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_attributes_after_vector_metadata() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("int_with_custom_attrs.rds");
    let mut visitor = CountingVisitor::default();
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("streaming traversal failed");

    assert!(visitor.vector_metadata > 0, "expected vector metadata");
    assert!(visitor.attrs > 0, "expected attributes");
    assert_eq!(
        visitor.vector_metadata_before_attrs,
        Some(true),
        "expected vector metadata before attributes for vectors"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_pairlist_attributes_before_children() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let bytes =
        std::fs::read("tests/data/pairlist_mixed.rds").expect("failed to read pairlist_mixed.rds");
    let obj = read_rds(&bytes).expect("failed to parse pairlist_mixed.rds");
    if !contains_pairlist_with_attrs(&obj) {
        eprintln!("Skipping test: no pairlist attributes detected");
        return;
    }

    let source = read_source("pairlist_mixed.rds");
    let mut visitor = PairlistOrderVisitor::default();
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("streaming traversal failed");
    if visitor.pairlist_attrs_seen {
        assert!(
            !visitor.child_before_attrs,
            "pairlist child visited before attributes"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_progress_reports_bytes() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("list_simple.rds");
    let mut visitor = CountingVisitor::default();
    let mut progress_events: Vec<StreamingProgress> = Vec::new();
    traverse_rds_streaming_with_progress(
        &source,
        ParseConfig::default(),
        &mut visitor,
        &mut |progress| progress_events.push(progress),
    )
    .expect("streaming traversal failed");

    assert!(!progress_events.is_empty(), "expected progress callbacks");
    let last = *progress_events.last().expect("progress event missing");
    assert!(last.bytes_read > 0, "expected non-zero bytes read");
    if let Some(total) = last.total_bytes {
        assert!(
            last.bytes_read <= total,
            "bytes_read should not exceed total_bytes"
        );
    }
}

#[derive(Default)]
struct LargeVectorVisitor {
    lengths: Vec<usize>,
}

impl RdsVisitor for LargeVectorVisitor {
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
        vec_type: VectorKind,
        len: usize,
    ) -> Result<(), Self::Error> {
        if vec_type == VectorKind::Integer {
            self.lengths.push(len);
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_large_vector_fixture() {
    let Some((path, expected_len)) = find_large_fixture() else {
        eprintln!("Skipping test: no large_int_*.rds fixture found");
        return;
    };

    let source = MmapRdsSource::from_path(&path)
        .unwrap_or_else(|_| panic!("Failed to open fixture: {}", path.display()));
    let mut visitor = LargeVectorVisitor::default();
    let mut progress_events: Vec<StreamingProgress> = Vec::new();
    let config = ParseConfig::lazy_metadata().with_max_vector_length(expected_len + 1);

    traverse_rds_streaming_with_progress(&source, config, &mut visitor, &mut |progress| {
        progress_events.push(progress)
    })
    .expect("streaming traversal failed");

    assert!(
        visitor.lengths.contains(&expected_len),
        "expected integer vector length {} in streaming metadata",
        expected_len
    );
    assert!(
        progress_events.iter().any(|evt| evt.bytes_read > 0),
        "expected progress bytes to advance"
    );
}

fn contains_shared(obj: &RObject) -> bool {
    match obj {
        RObject::Shared(_) => true,
        RObject::WithAttributes { object, attributes } => {
            contains_shared(object) || attributes.iter().any(|(_, value)| contains_shared(value))
        }
        RObject::List(values) | RObject::Expression(values) => values.iter().any(contains_shared),
        RObject::Pairlist(elements) => elements.iter().any(|elem| contains_shared(&elem.value)),
        RObject::Language { function, args } => {
            contains_shared(function) || args.iter().any(|elem| contains_shared(&elem.value))
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => contains_shared(formals) || contains_shared(body) || contains_shared(environment),
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => contains_shared(enclosing) || contains_shared(frame) || contains_shared(hashtab),
        RObject::Promise {
            value,
            expression,
            environment,
        } => contains_shared(value) || contains_shared(expression) || contains_shared(environment),
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => contains_shared(code) || contains_shared(constants) || contains_shared(expr),
        RObject::DataFrame(data) => data.columns.values().any(contains_shared),
        RObject::S3Object(data) => {
            contains_shared(&data.base)
                || data
                    .attributes
                    .iter()
                    .any(|(_, value)| contains_shared(value))
        }
        RObject::S4Object(data) => data.slots.values().any(contains_shared),
        _ => false,
    }
}

fn contains_pairlist_with_attrs(obj: &RObject) -> bool {
    match obj {
        RObject::WithAttributes { object, attributes } => {
            (matches!(**object, RObject::Pairlist(_)) && !attributes.is_empty())
                || contains_pairlist_with_attrs(object)
                || attributes
                    .iter()
                    .any(|(_, value)| contains_pairlist_with_attrs(value))
        }
        RObject::Pairlist(elements) => elements
            .iter()
            .any(|elem| contains_pairlist_with_attrs(&elem.value)),
        RObject::List(values) | RObject::Expression(values) => {
            values.iter().any(contains_pairlist_with_attrs)
        }
        RObject::Language { function, args } => {
            contains_pairlist_with_attrs(function)
                || args
                    .iter()
                    .any(|elem| contains_pairlist_with_attrs(&elem.value))
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => {
            contains_pairlist_with_attrs(formals)
                || contains_pairlist_with_attrs(body)
                || contains_pairlist_with_attrs(environment)
        }
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => {
            contains_pairlist_with_attrs(enclosing)
                || contains_pairlist_with_attrs(frame)
                || contains_pairlist_with_attrs(hashtab)
        }
        RObject::Promise {
            value,
            expression,
            environment,
        } => {
            contains_pairlist_with_attrs(value)
                || contains_pairlist_with_attrs(expression)
                || contains_pairlist_with_attrs(environment)
        }
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => {
            contains_pairlist_with_attrs(code)
                || contains_pairlist_with_attrs(constants)
                || contains_pairlist_with_attrs(expr)
        }
        RObject::DataFrame(data) => data.columns.values().any(contains_pairlist_with_attrs),
        RObject::S3Object(data) => {
            contains_pairlist_with_attrs(&data.base)
                || data
                    .attributes
                    .iter()
                    .any(|(_, value)| contains_pairlist_with_attrs(value))
        }
        RObject::S4Object(data) => data.slots.values().any(contains_pairlist_with_attrs),
        _ => false,
    }
}

#[derive(Default)]
struct PairlistOrderVisitor {
    stack: Vec<PairlistFrame>,
    pairlist_attrs_seen: bool,
    child_before_attrs: bool,
}

#[derive(Default)]
struct PairlistFrame {
    is_pairlist: bool,
    attrs_seen: bool,
}

impl RdsVisitor for PairlistOrderVisitor {
    type Error = std::convert::Infallible;

    fn on_object_start(
        &mut self,
        _path: &ObjectPath,
        obj_type: &str,
    ) -> Result<VisitAction, Self::Error> {
        if let Some(parent) = self.stack.last() {
            if parent.is_pairlist && !parent.attrs_seen {
                self.child_before_attrs = true;
            }
        }
        self.stack.push(PairlistFrame {
            is_pairlist: obj_type == "Pairlist",
            attrs_seen: false,
        });
        Ok(VisitAction::Continue)
    }

    fn on_attributes(
        &mut self,
        _path: &ObjectPath,
        _attrs: &rds2rust::Attributes,
    ) -> Result<(), Self::Error> {
        if let Some(frame) = self.stack.last_mut() {
            if frame.is_pairlist {
                frame.attrs_seen = true;
                self.pairlist_attrs_seen = true;
            }
        }
        Ok(())
    }

    fn on_object_end(&mut self, _path: &ObjectPath) -> Result<(), Self::Error> {
        self.stack.pop();
        Ok(())
    }
}
