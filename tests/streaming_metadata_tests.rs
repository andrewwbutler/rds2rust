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
