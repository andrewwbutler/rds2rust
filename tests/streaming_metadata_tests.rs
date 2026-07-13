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

    // Byte-alignment proof: the list is list(name=<chr>, func=<compiled fn>,
    // data=<real len 3>) with a `names` attribute. Because the bytecode
    // payload is now consumed exactly, the walk reaches the trailing `data`
    // element (Real, len 3, at `[2]`) and the list's `names` attribute
    // (Character, len 3). The old stumbling traversal misread the payload and
    // never surfaced these — seeing them is direct evidence the decoder
    // consumed the bytecode bytes and nothing more.
    let has_trailing_real = info.vectors.iter().any(|v| {
        matches!(v.kind, rds2rust::VectorKind::Real)
            && v.length == 3
            && v.path.segments.len() == 1
            && v.path.segments[0].as_ref() == "[2]"
    });
    assert!(
        has_trailing_real,
        "expected the trailing real vector `data` (len 3 at [2]) after the \
         bytecode payload; got vectors: {:?}",
        info.vectors
            .iter()
            .map(|v| (&v.path.segments, v.kind, v.length))
            .collect::<Vec<_>>()
    );
}

/// Error path: the streaming BCREPREF/BCREPDEF arm is tightened to reject a
/// bytecode rep marker appearing as a standalone top-level object. Such a
/// marker is only valid inside a BCODESXP constant pool (now fully consumed by
/// the sync decoder), so reaching one at the stream root means a corrupt or
/// misparsed stream — it must error rather than silently continue.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_rejects_top_level_bytecode_rep_marker() {
    // Minimal RDS v3 header (XDR "X\n", format version 3, writer/min R
    // versions, empty native-encoding name), followed by a top-level object
    // whose flags word is BCREPREF (243). BCREPREF is never a valid top-level
    // SEXP, so streaming inspection must reject it.
    const BCREPREF: u32 = 243;
    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend_from_slice(&[0x58, 0x0a]); // "X\n" XDR marker
    bytes.extend_from_slice(&3u32.to_be_bytes()); // serialization format version
    bytes.extend_from_slice(&[0x00, 0x04, 0x03, 0x03]); // writer R version 4.3.3
    bytes.extend_from_slice(&[0x00, 0x03, 0x05, 0x00]); // min reader R version 3.5.0
    bytes.extend_from_slice(&0u32.to_be_bytes()); // native encoding name length 0
    bytes.extend_from_slice(&BCREPREF.to_be_bytes()); // top-level object flags = BCREPREF

    let input = BytesInput { data: bytes };
    let err = inspect_metadata_streaming(&input, ParseConfig::default())
        .expect_err("expected a parse error for a top-level bytecode rep marker, got Ok");
    // Assert the error originates from the tightened BCREPREF/BCREPDEF arm, not
    // an earlier header/format parse failure — otherwise the test could pass
    // for the wrong reason if the synthetic header ever drifted.
    let msg = err.to_string();
    assert!(
        msg.contains("Bytecode representation type"),
        "expected the rejection to come from the bytecode-rep-marker arm, got: {}",
        msg
    );
}

/// Reps path: `bytecode_reps.rds` is a compiled function whose constant pool
/// uses BCREPDEF/BCREPREF (R shares repeated/recursive language objects via
/// rep markers). This is the case the old lenient continue-arm hand-waved:
/// the streaming decoder must consume the rep markers exactly. A clean single
/// Bytecode warning with no desync error proves the reps were consumed
/// correctly by the (delegated) decoder.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_handles_rep_using_bytecode() {
    if !Path::new("tests/data/bytecode_reps.rds").exists() {
        eprintln!("Skipping test: bytecode_reps.rds not generated");
        return;
    }

    let source = read_source("bytecode_reps.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed on rep-using bytecode");

    assert_eq!(info.version, Some(3));
    assert_eq!(
        count_bytecode_warnings(&info),
        1,
        "expected exactly one Bytecode warning for the rep-using compiled function"
    );
}

/// Byte-alignment across a rep-using payload: `bytecode_then_trailing.rds` is
/// `list(compiled = <rep-using compiled fn>, trailing_marker = <int len 5>)`.
/// The trailing integer vector is serialized *after* the bytecode payload, so
/// its continued visibility at `[1]` is direct proof the streaming decoder
/// consumed exactly the bytecode bytes — the core evidence the old stumbling
/// traversal is gone.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn streaming_stays_aligned_after_bytecode_payload() {
    if !Path::new("tests/data/bytecode_then_trailing.rds").exists() {
        eprintln!("Skipping test: bytecode_then_trailing.rds not generated");
        return;
    }

    let source = read_source("bytecode_then_trailing.rds");
    let info = inspect_metadata_streaming(&source, ParseConfig::default())
        .expect("metadata streaming failed on bytecode-then-trailing");

    assert_eq!(count_bytecode_warnings(&info), 1);

    let trailing_marker = info.vectors.iter().find(|v| {
        matches!(v.kind, rds2rust::VectorKind::Integer)
            && v.path.segments.len() == 1
            && v.path.segments[0].as_ref() == "[1]"
    });
    let trailing_marker = trailing_marker.unwrap_or_else(|| {
        panic!(
            "expected the trailing integer marker at [1] after the bytecode \
             payload (desync guard); got vectors: {:?}",
            info.vectors
                .iter()
                .map(|v| (&v.path.segments, v.kind, v.length))
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(
        trailing_marker.length, 5,
        "trailing marker should be the length-5 integer vector, proving the \
         bytecode payload was consumed exactly"
    );
}

/// In-memory `RdsInput` for synthetic-stream tests.
#[cfg(not(target_arch = "wasm32"))]
struct BytesInput {
    data: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl rds2rust::RdsInput for BytesInput {
    fn read_at(&self, offset: u64, len: usize) -> rds2rust::Result<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len).min(self.data.len());
        Ok(self.data[start..end.max(start)].to_vec())
    }

    fn len(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }
}
