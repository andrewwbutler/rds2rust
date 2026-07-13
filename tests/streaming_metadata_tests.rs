#![cfg(not(target_arch = "wasm32"))]
//! Streaming metadata extraction tests.

use std::path::Path;

use rds2rust::{inspect_metadata_streaming, MetadataWarning, ParseConfig};

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
#[test]
fn metadata_includes_version_and_vectors() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("list_simple.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed");
    assert!(info.version.is_some());
    assert!(!info.vectors.is_empty());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn metadata_detects_dataframe() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("dataframe_simple.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed");
    assert!(!info.dataframes.is_empty());
    let df = &info.dataframes[0];
    assert!(df.num_cols > 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn metadata_detects_s4() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("s4_complex.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed");
    assert!(!info.s4_objects.is_empty());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn metadata_warns_on_altrep() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("altrep_intseq.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed");
    assert!(
        info.warnings.iter().any(|warning| matches!(
            warning,
            MetadataWarning::UnsupportedStructure { structure, .. } if structure == "Altrep"
        )),
        "expected Altrep warning in streaming metadata"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn metadata_warns_on_bytecode() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("bytecode_func.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed");
    assert!(
        info.warnings.iter().any(|warning| matches!(
            warning,
            MetadataWarning::UnsupportedStructure { structure, .. } if structure == "Bytecode"
        )),
        "expected Bytecode warning in streaming metadata"
    );
}

/// Characterization: the observable invariants of streaming inspection on
/// bytecode fixtures that must survive the streaming-bytecode-decoder refactor.
/// These deliberately assert *behavior that must not regress* (no desync, the
/// Bytecode warning fires, non-bytecode siblings after the payload stay
/// visible) rather than the exact object/vector counts — the pre-refactor
/// traversal leaks bytecode internals (e.g. the opcode integer vector at
/// `.../code/[0]`) into the reported vectors, and correcting that is the point
/// of the refactor, so those leaked counts are expected to change.
#[cfg(not(target_arch = "wasm32"))]
fn count_bytecode_warnings(info: &rds2rust::DatasetInfo) -> usize {
    info.warnings
        .iter()
        .filter(|w| {
            matches!(
                w,
                MetadataWarning::UnsupportedStructure { structure, .. } if structure == "Bytecode"
            )
        })
        .count()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn characterize_bytecode_func_streaming() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("bytecode_func.rds");
    // Must not desync / error while walking the bytecode payload.
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed on bytecode_func.rds");

    assert_eq!(info.version, Some(3));
    // Exactly one Bytecode warning for the single compiled function.
    assert_eq!(
        count_bytecode_warnings(&info),
        1,
        "expected exactly one Bytecode warning"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn characterize_bytecode_in_list_streaming() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let source = read_source("bytecode_in_list.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed on bytecode_in_list.rds");

    assert_eq!(info.version, Some(3));
    assert_eq!(
        count_bytecode_warnings(&info),
        1,
        "expected exactly one Bytecode warning"
    );

    // Desync guard: the list holds a non-bytecode sibling (a length-1
    // character vector at `[0]`) alongside the compiled function. If the
    // bytecode walk consumed the wrong number of bytes, this sibling would be
    // missed or the stream would desync. Its continued visibility proves the
    // walk stayed aligned across the bytecode payload.
    let has_top_level_char_sibling = info.vectors.iter().any(|v| {
        matches!(v.kind, rds2rust::VectorKind::Character)
            && v.path.segments.len() == 1
            && v.path.segments[0].as_ref() == "[0]"
    });
    assert!(
        has_top_level_char_sibling,
        "expected the non-bytecode character sibling at [0] to stay visible \
         (desync guard); got vectors: {:?}",
        info.vectors
            .iter()
            .map(|v| (&v.path.segments, v.kind, v.length))
            .collect::<Vec<_>>()
    );
}
