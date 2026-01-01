use std::path::PathBuf;

use rds2rust::{
    extract_object_from_path, extract_object_from_path_chunked, extract_object_from_path_with_kind,
    extract_object_from_path_with_kind_chunked, extract_vectors_from_path,
    extract_vectors_from_path_chunked, extract_vectors_streaming, ChunkedRdsSource, MmapRdsSource,
    ObjectKind,
};

fn print_usage() {
    eprintln!(
        "Usage: rds-extract <input.rds> <out_dir> [paths...] [--budget-mb N] [--manifest NAME] [--object-path PATH] [--object-kind KIND] [--chunked] [--no-streaming]\n       rds-extract convert <input.rds> <out_dir> --object-kind KIND [--object-path PATH] [--budget-mb N] [--manifest NAME] [--chunked] [--no-streaming]\n\nExamples:\n  rds-extract data.rds out/ data.matrix meta.data --budget-mb 512\n  rds-extract data.rds out/ --manifest manifest.json\n  rds-extract data.rds out/ --object-path data --manifest manifest.json\n  rds-extract data.rds out/ --object-kind dataframe --object-path data\n  rds-extract convert data.rds out/ --object-kind dataframe --object-path data\n  rds-extract convert data.rds out/ --object-kind dataframe --chunked"
    );
}

fn parse_object_kind(input: &str) -> Option<ObjectKind> {
    match input {
        "dataframe" | "data-frame" | "data_frame" => Some(ObjectKind::DataFrame),
        "dense-matrix" | "dense_matrix" | "densematrix" => Some(ObjectKind::DenseMatrix),
        "sparse-matrix" | "sparse_matrix" | "sparsematrix" => Some(ObjectKind::SparseMatrix),
        "list" => Some(ObjectKind::List),
        _ => None,
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    let is_convert = args.first().map(|arg| arg == "convert").unwrap_or(false);
    if is_convert {
        args.remove(0);
        if args.len() < 2 {
            print_usage();
            std::process::exit(2);
        }
    }

    let input = args.remove(0);
    let out_dir = args.remove(0);

    let mut paths: Vec<String> = Vec::new();
    let mut budget_mb: Option<usize> = None;
    let mut manifest_name: Option<String> = None;
    let mut object_path: Option<String> = None;
    let mut object_kind: Option<ObjectKind> = None;
    let mut use_chunked = false;
    let mut use_streaming = true;
    let mut chunk_size_mb: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--budget-mb" => {
                let value = args.get(i + 1).cloned();
                let Some(value) = value else {
                    eprintln!("Missing value for --budget-mb");
                    std::process::exit(2);
                };
                budget_mb = value.parse::<usize>().ok();
                if budget_mb.is_none() {
                    eprintln!("Invalid value for --budget-mb: {}", value);
                    std::process::exit(2);
                }
                i += 2;
            }
            "--manifest" => {
                let value = args.get(i + 1).cloned();
                let Some(value) = value else {
                    eprintln!("Missing value for --manifest");
                    std::process::exit(2);
                };
                manifest_name = Some(value);
                i += 2;
            }
            "--object-path" => {
                let value = args.get(i + 1).cloned();
                let Some(value) = value else {
                    eprintln!("Missing value for --object-path");
                    std::process::exit(2);
                };
                object_path = Some(value);
                i += 2;
            }
            "--object-kind" => {
                let value = args.get(i + 1).cloned();
                let Some(value) = value else {
                    eprintln!("Missing value for --object-kind");
                    std::process::exit(2);
                };
                let Some(kind) = parse_object_kind(value.as_str()) else {
                    eprintln!(
                        "Invalid value for --object-kind: {} (expected dataframe|dense-matrix|sparse-matrix|list)",
                        value
                    );
                    std::process::exit(2);
                };
                object_kind = Some(kind);
                i += 2;
            }
            "--chunked" => {
                use_chunked = true;
                i += 1;
            }
            "--streaming" => {
                use_streaming = true;
                i += 1;
            }
            "--no-streaming" => {
                use_streaming = false;
                i += 1;
            }
            "--chunk-size-mb" => {
                let value = args.get(i + 1).cloned();
                let Some(value) = value else {
                    eprintln!("Missing value for --chunk-size-mb");
                    std::process::exit(2);
                };
                chunk_size_mb = value.parse::<usize>().ok();
                if chunk_size_mb.is_none() {
                    eprintln!("Invalid value for --chunk-size-mb: {}", value);
                    std::process::exit(2);
                }
                i += 2;
            }
            other => {
                paths.push(other.to_string());
                i += 1;
            }
        }
    }

    if (object_path.is_some() || object_kind.is_some()) && !paths.is_empty() {
        eprintln!("Cannot combine --object-path/--object-kind with explicit paths");
        std::process::exit(2);
    }

    if is_convert && object_kind.is_none() {
        eprintln!("Missing --object-kind for convert mode");
        std::process::exit(2);
    }

    if object_path.is_none() && object_kind.is_none() && paths.is_empty() {
        paths.push(String::new());
    }

    let output = if object_path.is_some() || object_kind.is_some() {
        let path = object_path.as_deref().unwrap_or("");
        let normalized = match path {
            "." | "root" => "",
            _ => path,
        };
        let manifest_name = manifest_name.or_else(|| Some(String::from("manifest.json")));
        match (object_kind, use_chunked, use_streaming) {
            (Some(kind), true, true) => {
                let source = ChunkedRdsSource::from_path(std::path::Path::new(&input))
                    .expect("open chunked source");
                let config = if let Some(budget) = budget_mb {
                    rds2rust::ParseConfig::for_constrained_conversion(budget)
                } else {
                    rds2rust::ParseConfig::for_trusted_large_file()
                };
                let obj = rds2rust::read_rds_with_input(&source, config).expect("parse rds");
                let chunk_bytes = chunk_size_mb.map(|mb| mb * 1024 * 1024);
                rds2rust::extract_object_to_raw_files_with_kind_and_input_streaming(
                    &obj,
                    &source,
                    normalized,
                    kind,
                    chunk_bytes,
                    PathBuf::from(&out_dir).as_path(),
                    manifest_name.as_deref(),
                )
                .map(|output| output.result)
            }
            (Some(kind), false, true) => {
                let source = MmapRdsSource::from_path(std::path::Path::new(&input))
                    .expect("open mmap source");
                let config = if let Some(budget) = budget_mb {
                    rds2rust::ParseConfig::for_constrained_conversion(budget)
                } else {
                    rds2rust::ParseConfig::for_trusted_large_file()
                };
                let obj = rds2rust::read_rds_with_input(&source, config).expect("parse rds");
                let chunk_bytes = chunk_size_mb.map(|mb| mb * 1024 * 1024);
                rds2rust::extract_object_to_raw_files_with_kind_and_input_streaming(
                    &obj,
                    &source,
                    normalized,
                    kind,
                    chunk_bytes,
                    PathBuf::from(&out_dir).as_path(),
                    manifest_name.as_deref(),
                )
                .map(|output| output.result)
            }
            (Some(kind), true, false) => extract_object_from_path_with_kind_chunked(
                &input,
                PathBuf::from(out_dir),
                normalized,
                kind,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result),
            (Some(kind), false, false) => extract_object_from_path_with_kind(
                &input,
                PathBuf::from(out_dir),
                normalized,
                kind,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result),
            (None, true, true) => {
                let source = ChunkedRdsSource::from_path(std::path::Path::new(&input))
                    .expect("open chunked source");
                let config = if let Some(budget) = budget_mb {
                    rds2rust::ParseConfig::for_constrained_conversion(budget)
                } else {
                    rds2rust::ParseConfig::for_trusted_large_file()
                };
                let obj = rds2rust::read_rds_with_input(&source, config).expect("parse rds");
                let chunk_bytes = chunk_size_mb.map(|mb| mb * 1024 * 1024);
                rds2rust::extract_object_to_raw_files_with_input_streaming(
                    &obj,
                    &source,
                    normalized,
                    chunk_bytes,
                    PathBuf::from(&out_dir).as_path(),
                    manifest_name.as_deref(),
                )
                .map(|output| output.result)
            }
            (None, false, true) => {
                let source = MmapRdsSource::from_path(std::path::Path::new(&input))
                    .expect("open mmap source");
                let config = if let Some(budget) = budget_mb {
                    rds2rust::ParseConfig::for_constrained_conversion(budget)
                } else {
                    rds2rust::ParseConfig::for_trusted_large_file()
                };
                let obj = rds2rust::read_rds_with_input(&source, config).expect("parse rds");
                let chunk_bytes = chunk_size_mb.map(|mb| mb * 1024 * 1024);
                rds2rust::extract_object_to_raw_files_with_input_streaming(
                    &obj,
                    &source,
                    normalized,
                    chunk_bytes,
                    PathBuf::from(&out_dir).as_path(),
                    manifest_name.as_deref(),
                )
                .map(|output| output.result)
            }
            (None, true, false) => extract_object_from_path_chunked(
                &input,
                PathBuf::from(out_dir),
                normalized,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result),
            (None, false, false) => extract_object_from_path(
                &input,
                PathBuf::from(out_dir),
                normalized,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result),
        }
    } else {
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        if use_streaming {
            let source: Box<dyn rds2rust::RdsInput> = if use_chunked {
                Box::new(
                    ChunkedRdsSource::from_path(std::path::Path::new(&input))
                        .expect("open chunked source"),
                )
            } else {
                Box::new(
                    MmapRdsSource::from_path(std::path::Path::new(&input))
                        .expect("open mmap source"),
                )
            };
            let config = if let Some(budget) = budget_mb {
                rds2rust::ParseConfig::for_constrained_conversion(budget)
            } else {
                rds2rust::ParseConfig::for_trusted_large_file()
            };
            let obj = rds2rust::read_rds_with_input(source.as_ref(), config).expect("parse rds");
            let chunk_bytes = chunk_size_mb.map(|mb| mb * 1024 * 1024);
            let result = extract_vectors_streaming(
                &obj,
                source.as_ref(),
                &path_refs,
                PathBuf::from(&out_dir).as_path(),
                chunk_bytes,
            )
            .expect("stream extract");
            if let Some(name) = manifest_name.as_deref() {
                let _ = rds2rust::write_extraction_manifest(PathBuf::from(&out_dir), &result, name)
                    .expect("write manifest");
            }
            Ok(result)
        } else if use_chunked {
            extract_vectors_from_path_chunked(
                &input,
                PathBuf::from(out_dir),
                &path_refs,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result)
        } else {
            extract_vectors_from_path(
                &input,
                PathBuf::from(out_dir),
                &path_refs,
                budget_mb,
                manifest_name.as_deref(),
            )
            .map(|output| output.result)
        }
    };

    if let Err(err) = output {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
