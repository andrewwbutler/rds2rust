//! Data frames and factors written by rds2rust must come back from R as real
//! S3 objects. Both used to be written without R's OBJECT bit, so R saw the
//! class attribute but skipped internal-generic dispatch: `nrow()` on a
//! written data frame returned NULL, and `is.object()` was FALSE for both.
//! Data frames also lost R's compact automatic row names.

// Native-only test file: excluded from wasm32 so `wasm-pack test`
// (which builds every test target) can compile the workspace.
#![cfg(not(target_arch = "wasm32"))]

use indexmap::IndexMap;
use rds2rust::{read_rds, write_rds, DataFrameData, FactorData, RObject};
use std::fs;
use std::io::Read;
use std::process::Command;
use std::sync::Arc;

const IS_OBJECT_BIT: u32 = 1 << 8;
const HAS_ATTR_BIT: u32 = 1 << 9;

fn r_available() -> bool {
    Command::new("R")
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_r_code(code: &str) -> Result<String, String> {
    let output = Command::new("R")
        .args(["--vanilla", "--slave", "-e", code])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn gunzip(written: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(written)
        .read_to_end(&mut bytes)
        .expect("writer output is gzip");
    bytes
}

/// The flags word of the top-level object in a written stream.
fn top_level_flags(written: &[u8]) -> u32 {
    let bytes = gunzip(written);
    // "X\n", then version, writer version, minimum version (3 words).
    let mut offset = 2 + 12;
    // Version 3 streams add the native encoding: its length, then its bytes.
    if bytes[2..6] == [0, 0, 0, 3] {
        let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4 + len as usize;
    }
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn names(names: &[&str]) -> Vec<Option<Arc<str>>> {
    names.iter().map(|name| Some(Arc::from(*name))).collect()
}

fn data_frame(row_names: Vec<Option<Arc<str>>>) -> RObject {
    let mut columns = IndexMap::new();
    columns.insert(Arc::from("a"), RObject::Integer(vec![1, 2, 3].into()));
    RObject::DataFrame(Box::new(DataFrameData { columns, row_names }))
}

#[test]
fn data_frames_and_factors_are_written_with_the_object_bit() {
    let factor = RObject::Factor(Box::new(FactorData {
        values: vec![1, 2, 1],
        levels: names(&["a", "b"]),
        ordered: false,
    }));
    for (label, obj) in [
        ("data frame", data_frame(names(&["1", "2", "3"]))),
        ("factor", factor),
    ] {
        let flags = top_level_flags(&write_rds(&obj).expect("write"));
        assert_eq!(flags & IS_OBJECT_BIT, IS_OBJECT_BIT, "{label}: {flags:#x}");
        assert_eq!(flags & HAS_ATTR_BIT, HAS_ATTR_BIT, "{label}: {flags:#x}");
    }
}

#[test]
fn automatic_row_names_are_written_in_compact_form() {
    // INTSXP of length 2 holding NA_integer_ and -3: R's c(NA_integer_, -3L).
    let compact: Vec<u8> = [0x0d_u32, 2, 0x8000_0000, (-3_i32) as u32]
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .collect();
    let has_compact = |obj: &RObject| {
        gunzip(&write_rds(obj).expect("write"))
            .windows(compact.len())
            .any(|window| window == compact)
    };
    assert!(has_compact(&data_frame(names(&["1", "2", "3"]))));
    // Anything other than exactly "1".."n" stays explicit character names.
    for explicit in [&["a", "b", "c"], &["1", "3", "2"], &["01", "2", "3"]] {
        assert!(!has_compact(&data_frame(names(explicit))), "{explicit:?}");
    }
}

/// R reads what rds2rust wrote back as identical() to what R saved, and
/// dispatches data.frame and factor methods on it.
#[test]
fn data_frames_and_factors_roundtrip_through_r() {
    if !r_available() {
        eprintln!("Skipping test: R not available");
        return;
    }

    let dir = std::env::temp_dir().join(format!("rds2rust_class_roundtrip_{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    let r_dir = dir.display().to_string().replace('\\', "/");

    // Each object is saved by R, read and written by rds2rust, then compared.
    let cases = [
        ("df", "data.frame(a = 1:3, b = c('x', 'y', 'z'))"),
        ("df_named", "data.frame(a = 1:2, row.names = c('r1', 'r2'))"),
        ("df_zero_rows", "data.frame(a = integer(0))"),
        ("df_empty", "data.frame()"),
        ("factor", "factor(c('a', 'b', 'a'))"),
        (
            "ordered",
            "factor(c('lo', 'hi'), levels = c('lo', 'hi'), ordered = TRUE)",
        ),
    ];
    let save = cases
        .iter()
        .map(|(name, expr)| {
            format!("saveRDS({expr}, '{r_dir}/{name}.rds', version = 2, compress = FALSE)")
        })
        .collect::<Vec<_>>()
        .join("; ");
    run_r_code(&save).expect("R failed to save the originals");

    for (name, _) in cases {
        let original = fs::read(dir.join(format!("{name}.rds"))).expect("read original");
        let obj = read_rds(&original).expect("parse original").object;
        let written = write_rds(&obj).expect("write");
        fs::write(dir.join(format!("{name}_rt.rds")), written).expect("write roundtrip");
    }

    let check = cases
        .iter()
        .map(|(name, _)| {
            format!(
                "o <- readRDS('{r_dir}/{name}.rds'); r <- readRDS('{r_dir}/{name}_rt.rds'); \
                 if (!identical(o, r)) stop('{name}: not identical'); \
                 if (!is.object(r)) stop('{name}: OBJECT bit missing'); \
                 if (is.data.frame(r) && !identical(nrow(r), nrow(o))) stop('{name}: nrow')"
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let result = run_r_code(&format!("{check}; cat('PASS')"));
    let _ = fs::remove_dir_all(&dir);
    match result {
        Ok(stdout) => assert!(stdout.contains("PASS"), "R verification failed: {stdout}"),
        Err(err) => panic!("R rejected a roundtripped object: {err}"),
    }
}
