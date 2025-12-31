#[cfg(not(target_arch = "wasm32"))]
fn find_rds_extract() -> std::path::PathBuf {
    std::env::var("CARGO_BIN_EXE_rds-extract")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir =
                std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
            let target = manifest_dir.join("target").join("debug");
            let exe_name = format!("rds-extract{}", std::env::consts::EXE_SUFFIX);
            let exe_path = target.join(exe_name);
            if exe_path.exists() {
                return exe_path;
            }

            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let status = std::process::Command::new(cargo)
                .arg("build")
                .arg("--bin")
                .arg("rds-extract")
                .arg("--quiet")
                .current_dir(&manifest_dir)
                .status()
                .expect("build rds-extract");
            if !status.success() {
                panic!("failed to build rds-extract for CLI integration test");
            }
            exe_path
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn run_extract_convert(
    fixture: &std::path::Path,
    kind: &str,
    object_path: &str,
    streaming: Option<bool>,
    chunked: bool,
    chunk_size_mb: Option<usize>,
) -> (tempfile::TempDir, rds2rust::Manifest) {
    use std::process::Command;

    assert!(fixture.exists(), "fixture missing: {:?}", fixture);
    let exe = find_rds_extract();
    let dir = tempfile::tempdir().expect("tempdir");
    let out_dir = dir.path();

    let mut cmd = Command::new(exe);
    cmd.arg("convert")
        .arg(fixture)
        .arg(out_dir)
        .arg("--object-kind")
        .arg(kind)
        .arg("--object-path")
        .arg(object_path);
    if chunked {
        cmd.arg("--chunked");
    }
    match streaming {
        Some(true) => {
            cmd.arg("--streaming");
        }
        Some(false) => {
            cmd.arg("--no-streaming");
        }
        None => {}
    }
    if let Some(size) = chunk_size_mb {
        cmd.arg("--chunk-size-mb").arg(size.to_string());
    }

    let output = cmd.output().expect("run rds-extract");
    if !output.status.success() {
        panic!(
            "rds-extract failed: {}\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let manifest_path = out_dir.join("manifest.json");
    let manifest = rds2rust::read_extraction_manifest(&manifest_path).expect("parse manifest");
    (dir, manifest)
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_manifest_outputs(dir: &tempfile::TempDir, manifest: &rds2rust::Manifest) {
    let out_dir = dir.path();
    for entry in &manifest.vectors {
        let file_path = out_dir.join(&entry.file);
        rds2rust::validate_vector_file_header(&file_path, entry)
            .expect("validate vector header");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_chunked_writes_manifest() {
    let fixture = std::path::Path::new("tests/data/dataframe_simple.rds");
    let (dir, manifest) = run_extract_convert(fixture, "dataframe", "", Some(false), true, None);
    assert_eq!(manifest.object_kind, "DataFrame");
    validate_manifest_outputs(&dir, &manifest);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_streaming_chunked_writes_manifest() {
    let fixture = std::path::Path::new("tests/data/dataframe_simple.rds");
    let (dir, manifest) = run_extract_convert(fixture, "dataframe", "", Some(true), true, Some(1));
    assert_eq!(manifest.object_kind, "DataFrame");
    validate_manifest_outputs(&dir, &manifest);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_streaming_chunked_sparse_matrix() {
    let fixture = std::path::Path::new("tests/data/sparse_dimnames.rds");
    let (dir, manifest) = run_extract_convert(fixture, "sparse-matrix", "", Some(true), true, Some(1));
    assert_eq!(manifest.object_kind, "SparseMatrix");
    validate_manifest_outputs(&dir, &manifest);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_streaming_chunked_dense_matrix() {
    let fixture = std::path::Path::new("tests/data/matrix_real.rds");
    let (dir, manifest) = run_extract_convert(fixture, "dense-matrix", "", Some(true), true, Some(1));
    assert_eq!(manifest.object_kind, "DenseMatrix");
    validate_manifest_outputs(&dir, &manifest);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_streaming_chunked_dense_matrix_dimnames() {
    let fixture = std::path::Path::new("tests/data/matrix_dimnames.rds");
    let (dir, manifest) = run_extract_convert(fixture, "dense-matrix", "", Some(true), true, Some(1));
    assert_eq!(manifest.object_kind, "DenseMatrix");
    validate_manifest_outputs(&dir, &manifest);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cli_convert_no_streaming_chunked_sparse_matrix() {
    let fixture = std::path::Path::new("tests/data/sparse_dimnames.rds");
    let (dir, manifest) = run_extract_convert(fixture, "sparse-matrix", "", Some(false), true, None);
    assert_eq!(manifest.object_kind, "SparseMatrix");
    validate_manifest_outputs(&dir, &manifest);
}
