use std::path::PathBuf;

use rds2rust::{ChunkedRdsSource, ParseConfig};

fn print_usage() {
    eprintln!("Usage: rds-cache-profile <input.rds> [object-path]");
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(2);
    }

    let input = PathBuf::from(args.remove(0));
    let object_path = args.pop().unwrap_or_default();

    let source = ChunkedRdsSource::from_path(&input).expect("open chunked source");
    let obj = rds2rust::read_rds_with_input(&source, ParseConfig::for_trusted_large_file())
        .expect("parse rds");
    let paths = rds2rust::expand_object_paths(&obj, &object_path).expect("expand paths");
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();

    let out_dir = tempfile::tempdir().expect("tempdir");
    let _result = rds2rust::extract_vectors_streaming(
        &obj,
        &source,
        &path_refs,
        out_dir.path(),
        Some(4 * 1024 * 1024),
    )
    .expect("extract");

    let metrics = source.cache_metrics();
    println!(
        "cache hits={} misses={} prefetches={} bytes_read={}",
        metrics.hits, metrics.misses, metrics.prefetches, metrics.bytes_read
    );
}
