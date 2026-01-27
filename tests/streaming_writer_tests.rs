#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{
    read_rds_from_path, write_rds, write_rds_atomic, write_rds_streaming,
    write_rds_streaming_with_compression, RObject, VectorData,
};
use std::path::PathBuf;

fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    path.push(format!("rds2rust_{name}_{}_{}", std::process::id(), nanos));
    path
}

fn sample_object() -> RObject {
    RObject::Integer(VectorData::Owned(vec![1, 2, 3, 4]))
}

#[test]
fn streaming_matches_legacy_output() {
    let obj = sample_object();
    let legacy = write_rds(&obj).expect("write_rds");
    let mut streamed = Vec::new();
    write_rds_streaming(&obj, &mut streamed).expect("write_rds_streaming");
    assert_eq!(legacy, streamed);
}

#[test]
fn streaming_with_compression_level_roundtrip() {
    let obj = sample_object();
    let mut streamed = Vec::new();
    write_rds_streaming_with_compression(&obj, &mut streamed, flate2::Compression::new(0))
        .expect("write_rds_streaming_with_compression");
    let parsed = rds2rust::read_rds(&streamed).expect("read_rds").object;
    match parsed.into_concrete() {
        RObject::Integer(vec) => assert_eq!(vec.as_vec(), &vec![1, 2, 3, 4]),
        other => panic!("unexpected object: {:?}", other),
    }
}

#[test]
fn streaming_to_file_roundtrip() {
    let obj = sample_object();
    let path = temp_path("streaming_write.rds");
    {
        let file = std::fs::File::create(&path).expect("create file");
        let writer = std::io::BufWriter::new(file);
        write_rds_streaming(&obj, writer).expect("write_rds_streaming");
    }
    let parsed = read_rds_from_path(&path)
        .expect("read_rds_from_path")
        .object;
    match parsed.into_concrete() {
        RObject::Integer(vec) => assert_eq!(vec.as_vec(), &vec![1, 2, 3, 4]),
        other => panic!("unexpected object: {:?}", other),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn write_rds_atomic_roundtrip() {
    let obj = sample_object();
    let path = temp_path("atomic_write.rds");
    write_rds_atomic(&obj, &path).expect("write_rds_atomic");
    let parsed = read_rds_from_path(&path)
        .expect("read_rds_from_path")
        .object;
    match parsed.into_concrete() {
        RObject::Integer(vec) => assert_eq!(vec.as_vec(), &vec![1, 2, 3, 4]),
        other => panic!("unexpected object: {:?}", other),
    }
    let _ = std::fs::remove_file(&path);
}
