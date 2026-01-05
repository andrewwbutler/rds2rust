#![cfg(not(target_arch = "wasm32"))]

use std::io::Write;

#[cfg(not(target_arch = "wasm32"))]
use rds2rust::{ChunkedRdsSource, RdsInput};

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn chunked_source_reads_ranges() {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    file.write_all(b"abcdef").expect("write");
    file.flush().expect("flush");

    let source = ChunkedRdsSource::from_path(file.path()).expect("chunked source");
    assert_eq!(source.len(), Some(6));
    let chunk = source.read_at(1, 3).expect("read");
    assert_eq!(chunk, b"bcd");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn chunked_source_cache_metrics() {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    file.write_all(b"abcdef").expect("write");
    file.flush().expect("flush");

    let source = ChunkedRdsSource::from_path(file.path()).expect("chunked source");
    let _ = source.read_at(1, 3).expect("read");
    let metrics = source.cache_metrics();
    assert_eq!(metrics.hits, 0);
    assert_eq!(metrics.misses, 1);
    assert!(metrics.bytes_read >= 6);

    let _ = source.read_at(2, 2).expect("read again");
    let metrics = source.cache_metrics();
    assert_eq!(metrics.hits, 1);
    assert_eq!(metrics.misses, 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn read_rds_from_chunked_source() {
    let obj = rds2rust::RObject::Integer(rds2rust::VectorData::Owned(vec![1, 2, 3]));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), bytes).expect("write file");

    let parsed = rds2rust::read_rds_from_path_chunked(file.path()).expect("read chunked");
    match parsed {
        rds2rust::RObject::Integer(vec) => {
            assert_eq!(vec.as_vec(), &vec![1, 2, 3]);
        }
        other => panic!("unexpected object: {:?}", other),
    }
}
