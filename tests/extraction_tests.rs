#![cfg(not(target_arch = "wasm32"))]

use byteorder::{BigEndian, WriteBytesExt};

use std::fs::File;
use std::io::Read;

use rds2rust::{
    convert_object_to_raw_dump, convert_object_to_raw_dump_at_path, expand_dataframe_paths,
    expand_dense_matrix_paths, expand_list_index_paths, expand_object_paths,
    expand_object_paths_for_kind, expand_s4_slot_paths, expand_sparse_matrix_paths,
    extract_complex_vector_streaming, extract_integer_vector_streaming,
    extract_logical_vector_streaming, extract_object_from_path, extract_object_from_path_chunked,
    extract_object_from_path_with_kind, extract_object_from_path_with_kind_chunked,
    extract_object_to_raw_files, extract_object_to_raw_files_with_kind,
    extract_object_to_raw_files_with_kind_and_input_streaming, extract_raw_vector_streaming,
    extract_real_vector_streaming, extract_vectors_from_path_chunked, extract_vectors_streaming,
    extract_vectors_to_raw_files, read_extraction_manifest, read_vector_file_header,
    validate_vector_file_header, write_extraction_manifest, DataFrameData, LazyVector, ObjectKind,
    RObject, S4ObjectData, VectorData,
};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn extract_integer_vector_to_raw_file() {
    let data = [0, 0, 0, 5];
    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: 4,
    };
    let obj = RObject::Integer(VectorData::Lazy(span));
    let dir = tempdir().expect("tempdir");

    let result =
        extract_vectors_to_raw_files(&obj, &data, &[""], Some(4), dir.path()).expect("extract");

    assert!(result.missing.is_empty());
    assert_eq!(result.extracted.len(), 1);

    let manifest_path =
        write_extraction_manifest(dir.path(), &result, "manifest.json").expect("write manifest");

    let file_path = &result.extracted[0].file_path;
    let mut file = File::open(file_path).expect("open output");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("read output");

    assert!(buf.starts_with(b"RDS2VEC1"));
    assert_eq!(buf.len(), 24 + 4);
    assert_eq!(&buf[24..], &data);

    let mut manifest = String::new();
    File::open(manifest_path)
        .expect("open manifest")
        .read_to_string(&mut manifest)
        .expect("read manifest");
    assert!(manifest.contains("\"version\":1"));
    assert!(manifest.contains("\"object_kind\":\"Unknown\""));
    assert!(manifest.contains("\"path\":\"root\""));
    assert!(manifest.contains("\"kind\":\"Integer\""));
}

#[test]
fn read_manifest_and_validate_vector_header() {
    let data = [0, 0, 0, 5];
    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: 4,
    };
    let obj = RObject::Integer(VectorData::Lazy(span));
    let dir = tempdir().expect("tempdir");

    let result =
        extract_vectors_to_raw_files(&obj, &data, &[""], Some(4), dir.path()).expect("extract");
    let manifest_path =
        write_extraction_manifest(dir.path(), &result, "manifest.json").expect("write manifest");
    let manifest = read_extraction_manifest(&manifest_path).expect("read manifest");

    let entry = manifest.vectors.first().expect("vector entry");
    let file_path = dir.path().join(&entry.file);
    let header = read_vector_file_header(&file_path).expect("read header");
    assert_eq!(header.length, 1);
    validate_vector_file_header(&file_path, entry).expect("validate header");
}

#[test]
fn extract_character_vector_to_raw_file() {
    let obj = RObject::Character(VectorData::Owned(vec![Arc::from("hi"), Arc::from("there")]));
    let dir = tempdir().expect("tempdir");

    let result = extract_vectors_to_raw_files(&obj, &[], &[""], None, dir.path()).expect("extract");
    assert!(result.missing.is_empty());
    assert_eq!(result.extracted.len(), 1);

    let file_path = &result.extracted[0].file_path;
    let mut file = File::open(file_path).expect("open output");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("read output");

    let mut expected = Vec::new();
    expected.extend_from_slice(b"RDS2VEC1");
    expected.push(1);
    expected.push(6);
    expected.push(1);
    expected.push(0);
    expected.write_u64::<BigEndian>(2).unwrap();
    expected.write_u32::<BigEndian>(0).unwrap();
    expected.write_i32::<BigEndian>(2).unwrap();
    expected.extend_from_slice(b"hi");
    expected.write_i32::<BigEndian>(5).unwrap();
    expected.extend_from_slice(b"there");

    assert_eq!(buf, expected);
}

#[test]
fn extract_lazy_character_vector_to_raw_file() {
    let mut data = Vec::new();
    data.extend_from_slice(&[0, 0, 0, 9]); // CHARSXP flags
    data.extend_from_slice(&[0, 0, 0, 2]); // length
    data.extend_from_slice(b"hi");

    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: data.len() as u64,
    };
    let obj = RObject::Character(VectorData::Lazy(span));
    let dir = tempdir().expect("tempdir");

    let result = extract_vectors_to_raw_files(&obj, &data, &[""], Some(data.len()), dir.path())
        .expect("extract");
    assert!(result.missing.is_empty());

    let file_path = &result.extracted[0].file_path;
    let mut file = File::open(file_path).expect("open output");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("read output");

    let mut expected = Vec::new();
    expected.extend_from_slice(b"RDS2VEC1");
    expected.push(1);
    expected.push(6);
    expected.push(1);
    expected.push(0);
    expected.write_u64::<BigEndian>(1).unwrap();
    expected.write_u32::<BigEndian>(0).unwrap();
    expected.write_i32::<BigEndian>(2).unwrap();
    expected.extend_from_slice(b"hi");

    assert_eq!(buf, expected);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_vectors_from_path_writes_manifest() {
    let obj = RObject::Integer(VectorData::Owned(vec![1, 2, 3]));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = rds2rust::extract_vectors_from_path(
        &input_path,
        out_dir.clone(),
        &[""],
        Some(1),
        Some("manifest.json"),
    )
    .expect("extract");

    assert_eq!(output.result.extracted.len(), 1);
    let manifest_path = output.manifest_path.expect("manifest path");
    assert!(manifest_path.exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_vectors_from_path_chunked_writes_manifest() {
    let obj = RObject::Integer(VectorData::Owned(vec![1, 2, 3]));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = extract_vectors_from_path_chunked(
        &input_path,
        out_dir.clone(),
        &[""],
        Some(1),
        Some("manifest.json"),
    )
    .expect("extract");

    assert_eq!(output.result.extracted.len(), 1);
    let manifest_path = output.manifest_path.expect("manifest path");
    assert!(manifest_path.exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_object_from_path_expands_and_writes_manifest() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    columns.insert(Arc::from("b"), RObject::Real(VectorData::Owned(vec![1.0])));
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = extract_object_from_path(
        &input_path,
        out_dir.clone(),
        "",
        None,
        Some("manifest.json"),
    )
    .expect("extract");

    assert_eq!(output.paths.len(), 2);
    assert_eq!(output.result.extracted.len(), 2);
    let manifest_path = output.manifest_path.expect("manifest path");
    assert!(manifest_path.exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_object_from_path_chunked_expands_and_writes_manifest() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = extract_object_from_path_chunked(
        &input_path,
        out_dir.clone(),
        "",
        None,
        Some("manifest.json"),
    )
    .expect("extract");

    assert_eq!(output.paths.len(), 1);
    assert_eq!(output.result.extracted.len(), 1);
    let manifest_path = output.manifest_path.expect("manifest path");
    assert!(manifest_path.exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_object_to_raw_files_with_kind_and_input_streaming_writes_manifest() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    columns.insert(Arc::from("b"), RObject::Real(VectorData::Owned(vec![1.0])));
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let source = rds2rust::MmapRdsSource::from_path(&input_path).expect("open source");
    let config = rds2rust::ParseConfig::for_trusted_large_file();
    let parsed = rds2rust::read_rds_with_input(&source, config).expect("parse rds");

    let out_dir = dir.path().join("out");
    let output = extract_object_to_raw_files_with_kind_and_input_streaming(
        &parsed,
        &source,
        "",
        ObjectKind::DataFrame,
        Some(1024),
        &out_dir,
        Some("manifest.json"),
    )
    .expect("extract streaming");

    assert_eq!(output.paths.len(), 2);
    assert_eq!(output.result.extracted.len(), 2);
    let manifest_path = output.manifest_path.expect("manifest path");
    let manifest = read_extraction_manifest(&manifest_path).expect("read manifest");
    assert_eq!(manifest.object_kind, "DataFrame");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_object_from_path_with_kind_chunked_expands() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = extract_object_from_path_with_kind_chunked(
        &input_path,
        out_dir.clone(),
        "",
        ObjectKind::DataFrame,
        None,
        None,
    )
    .expect("extract");

    assert_eq!(output.paths.len(), 1);
    assert_eq!(output.result.extracted.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_object_from_path_with_kind_expands() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));
    let bytes = rds2rust::write_rds(&obj).expect("write rds");

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("input.rds");
    std::fs::write(&input_path, bytes).expect("write input");

    let out_dir = dir.path().join("out");
    let output = extract_object_from_path_with_kind(
        &input_path,
        out_dir.clone(),
        "",
        ObjectKind::DataFrame,
        None,
        None,
    )
    .expect("extract");

    assert_eq!(output.paths.len(), 1);
    assert_eq!(output.result.extracted.len(), 1);
}

#[test]
fn expand_dataframe_paths_root() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    columns.insert(Arc::from("b"), RObject::Real(VectorData::Owned(vec![1.0])));
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let mut paths = expand_dataframe_paths(&obj, "").expect("expand dataframe");
    paths.sort();
    assert_eq!(paths, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn expand_object_paths_dataframe_root() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    columns.insert(Arc::from("b"), RObject::Real(VectorData::Owned(vec![1.0])));
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let mut paths = expand_object_paths(&obj, "").expect("expand object");
    paths.sort();
    assert_eq!(paths, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn expand_object_paths_for_kind_dataframe() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let paths = expand_object_paths_for_kind(&obj, "", ObjectKind::DataFrame).expect("expand kind");
    assert_eq!(paths, vec!["a".to_string()]);
}

#[test]
fn extract_object_to_raw_files_with_kind_mismatch() {
    let obj = RObject::List(vec![
        RObject::Integer(VectorData::Owned(vec![1])),
        RObject::Real(VectorData::Owned(vec![2.0])),
    ]);
    let dir = tempdir().expect("tempdir");

    let err = extract_object_to_raw_files_with_kind(
        &obj,
        &[],
        "",
        ObjectKind::DataFrame,
        None,
        dir.path(),
        None,
    )
    .expect_err("mismatch");

    assert!(format!("{}", err).contains("DataFrame"));
}

#[test]
fn extract_object_to_raw_files_dataframe_manifest() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    columns.insert(Arc::from("b"), RObject::Real(VectorData::Owned(vec![1.0])));
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let dir = tempdir().expect("tempdir");
    let output =
        extract_object_to_raw_files(&obj, &[], "", None, dir.path(), Some("manifest.json"))
            .expect("extract object");

    assert_eq!(output.paths.len(), 2);
    assert_eq!(output.result.extracted.len(), 2);
    assert!(output.manifest_path.expect("manifest path").exists());
}

#[test]
fn extract_object_to_raw_files_with_kind_manifest() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let dir = tempdir().expect("tempdir");
    let output = extract_object_to_raw_files_with_kind(
        &obj,
        &[],
        "",
        ObjectKind::DataFrame,
        None,
        dir.path(),
        Some("manifest.json"),
    )
    .expect("extract object");

    let mut manifest = String::new();
    File::open(output.manifest_path.expect("manifest path"))
        .expect("open manifest")
        .read_to_string(&mut manifest)
        .expect("read manifest");
    assert!(manifest.contains("\"object_kind\":\"DataFrame\""));
}

#[test]
fn extract_object_to_raw_files_with_kind_dense_matrix() {
    let obj = RObject::WithAttributes {
        object: Box::new(RObject::Real(VectorData::Owned(vec![1.0, 2.0, 3.0, 4.0]))),
        attributes: {
            let mut attrs = rds2rust::Attributes::new();
            attrs.insert(
                Arc::from("dim"),
                RObject::Integer(VectorData::Owned(vec![2, 2])),
            );
            attrs
        },
    };

    let dir = tempdir().expect("tempdir");
    let output = extract_object_to_raw_files_with_kind(
        &obj,
        &[],
        "",
        ObjectKind::DenseMatrix,
        None,
        dir.path(),
        Some("manifest.json"),
    )
    .expect("extract object");

    assert_eq!(output.paths.len(), 2);
    assert_eq!(output.result.extracted.len(), 2);
    let mut manifest = String::new();
    File::open(output.manifest_path.expect("manifest path"))
        .expect("open manifest")
        .read_to_string(&mut manifest)
        .expect("read manifest");
    assert!(manifest.contains("\"object_kind\":\"DenseMatrix\""));
}

#[test]
fn extract_vectors_with_quoted_slot_name() {
    let mut slots = indexmap::IndexMap::new();
    slots.insert(
        Arc::from("slot.value"),
        RObject::Real(VectorData::Owned(vec![1.0, 2.0])),
    );
    let s4 = S4ObjectData {
        class: vec![Arc::from("dgCMatrix")],
        package: None,
        slots,
    };
    let obj = RObject::S4Object(Box::new(s4));
    let dir = tempdir().expect("tempdir");

    let result = extract_vectors_to_raw_files(&obj, &[], &["[\"slot.value\"]"], None, dir.path())
        .expect("extract");
    assert_eq!(result.extracted.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_raw_vector_streaming_writes_payload() {
    let bytes = vec![10u8, 20, 30, 40];
    let span = LazyVector {
        length: bytes.len(),
        offset: 0,
        byte_len: bytes.len() as u64,
    };
    let obj = RObject::Raw(VectorData::Lazy(span));

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("raw.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let info =
        extract_raw_vector_streaming(&obj, &source, "", dir.path(), None).expect("stream raw");
    let output = std::fs::read(info.file_path).expect("read output");
    assert!(output.starts_with(b"RDS2VEC1"));
    assert_eq!(&output[24..], bytes.as_slice());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_integer_vector_streaming_writes_payload() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1i32.to_be_bytes());
    bytes.extend_from_slice(&2i32.to_be_bytes());
    let span = LazyVector {
        length: 2,
        offset: 0,
        byte_len: bytes.len() as u64,
    };
    let obj = RObject::Integer(VectorData::Lazy(span));

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("int.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let info = extract_integer_vector_streaming(&obj, &source, "", dir.path(), None)
        .expect("stream integer");
    let output = std::fs::read(info.file_path).expect("read output");
    assert!(output.starts_with(b"RDS2VEC1"));
    assert_eq!(&output[24..], bytes.as_slice());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_logical_vector_streaming_writes_payload() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_be_bytes());
    bytes.extend_from_slice(&1i32.to_be_bytes());
    bytes.extend_from_slice(&i32::MIN.to_be_bytes());
    let span = LazyVector {
        length: 3,
        offset: 0,
        byte_len: bytes.len() as u64,
    };
    let obj = RObject::Logical(VectorData::Lazy(span));

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("logical.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let info = extract_logical_vector_streaming(&obj, &source, "", dir.path(), None)
        .expect("stream logical");
    let output = std::fs::read(info.file_path).expect("read output");
    assert!(output.starts_with(b"RDS2VEC1"));
    assert_eq!(&output[24..], bytes.as_slice());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_vectors_streaming_multiple_paths() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1i32.to_be_bytes());
    bytes.extend_from_slice(&2i32.to_be_bytes());
    bytes.extend_from_slice(&0i32.to_be_bytes());
    bytes.extend_from_slice(&1i32.to_be_bytes());
    let int_span = LazyVector {
        length: 2,
        offset: 0,
        byte_len: 8,
    };
    let logical_span = LazyVector {
        length: 2,
        offset: 8,
        byte_len: 8,
    };
    let obj = RObject::List(vec![
        RObject::Integer(VectorData::Lazy(int_span)),
        RObject::Logical(VectorData::Lazy(logical_span)),
    ]);

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("multi.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let result = extract_vectors_streaming(&obj, &source, &["[0]", "[1]"], dir.path(), None)
        .expect("stream vectors");
    assert_eq!(result.extracted.len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_character_vector_streaming_writes_payload() {
    let mut data = Vec::new();
    data.extend_from_slice(&[0, 0, 0, 9]); // CHARSXP flags
    data.extend_from_slice(&[0, 0, 0, 2]); // length
    data.extend_from_slice(b"hi");

    let span = LazyVector {
        length: 1,
        offset: 0,
        byte_len: data.len() as u64,
    };
    let obj = RObject::Character(VectorData::Lazy(span));
    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("chars.bin");
    std::fs::write(&input_path, &data).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let result =
        extract_vectors_streaming(&obj, &source, &[""], dir.path(), Some(1024)).expect("stream");
    let output = std::fs::read(&result.extracted[0].file_path).expect("read output");

    let mut expected = Vec::new();
    expected.extend_from_slice(b"RDS2VEC1");
    expected.push(1);
    expected.push(6);
    expected.push(1);
    expected.push(0);
    expected.write_u64::<BigEndian>(1).unwrap();
    expected.write_u32::<BigEndian>(0).unwrap();
    expected.write_i32::<BigEndian>(2).unwrap();
    expected.extend_from_slice(b"hi");

    assert_eq!(output, expected);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_real_vector_streaming_writes_payload() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1.25f64.to_be_bytes());
    bytes.extend_from_slice(&(-2.5f64).to_be_bytes());
    let span = LazyVector {
        length: 2,
        offset: 0,
        byte_len: bytes.len() as u64,
    };
    let obj = RObject::Real(VectorData::Lazy(span));

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("real.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let info =
        extract_real_vector_streaming(&obj, &source, "", dir.path(), None).expect("stream real");
    let output = std::fs::read(info.file_path).expect("read output");
    assert!(output.starts_with(b"RDS2VEC1"));
    assert_eq!(&output[24..], bytes.as_slice());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_complex_vector_streaming_writes_payload() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1.0f64.to_be_bytes());
    bytes.extend_from_slice(&(-2.0f64).to_be_bytes());
    bytes.extend_from_slice(&3.5f64.to_be_bytes());
    bytes.extend_from_slice(&4.25f64.to_be_bytes());
    let span = LazyVector {
        length: 2,
        offset: 0,
        byte_len: bytes.len() as u64,
    };
    let obj = RObject::Complex(VectorData::Lazy(span));

    let dir = tempdir().expect("tempdir");
    let input_path = dir.path().join("complex.bin");
    std::fs::write(&input_path, &bytes).expect("write input");
    let source = rds2rust::ChunkedRdsSource::from_path(&input_path).expect("chunked source");

    let info = extract_complex_vector_streaming(&obj, &source, "", dir.path(), None)
        .expect("stream complex");
    let output = std::fs::read(info.file_path).expect("read output");
    assert!(output.starts_with(b"RDS2VEC1"));
    assert_eq!(&output[24..], bytes.as_slice());
}

#[test]
fn convert_object_to_raw_dump_dataframe() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::DataFrame(Box::new(df));

    let dir = tempdir().expect("tempdir");
    let output = convert_object_to_raw_dump(
        &obj,
        &[],
        ObjectKind::DataFrame,
        None,
        dir.path(),
        Some("manifest.json"),
    )
    .expect("convert");

    assert_eq!(output.paths.len(), 1);
    assert_eq!(output.result.extracted.len(), 1);
    assert!(output.manifest_path.expect("manifest path").exists());
}

#[test]
fn convert_object_to_raw_dump_at_path_dataframe() {
    let mut columns = indexmap::IndexMap::new();
    columns.insert(
        Arc::from("a"),
        RObject::Integer(VectorData::Owned(vec![1, 2, 3])),
    );
    let df = DataFrameData {
        columns,
        row_names: Vec::new(),
    };
    let obj = RObject::List(vec![RObject::DataFrame(Box::new(df))]);

    let dir = tempdir().expect("tempdir");
    let output = convert_object_to_raw_dump_at_path(
        &obj,
        &[],
        "[0]",
        ObjectKind::DataFrame,
        None,
        dir.path(),
        None,
    )
    .expect("convert");

    assert_eq!(output.paths.len(), 1);
    assert_eq!(output.result.extracted.len(), 1);
}

#[test]
fn extract_object_to_raw_files_with_kind_sparse_matrix() {
    let mut slots = indexmap::IndexMap::new();
    slots.insert(
        Arc::from("x"),
        RObject::Real(VectorData::Owned(vec![1.0, 2.0])),
    );
    slots.insert(
        Arc::from("i"),
        RObject::Integer(VectorData::Owned(vec![0, 1])),
    );
    slots.insert(
        Arc::from("p"),
        RObject::Integer(VectorData::Owned(vec![0, 2])),
    );
    slots.insert(
        Arc::from("Dim"),
        RObject::Integer(VectorData::Owned(vec![2, 2])),
    );
    let s4 = S4ObjectData {
        class: vec![Arc::from("dgCMatrix")],
        package: None,
        slots,
    };
    let obj = RObject::S4Object(Box::new(s4));

    let dir = tempdir().expect("tempdir");
    let output = extract_object_to_raw_files_with_kind(
        &obj,
        &[],
        "",
        ObjectKind::SparseMatrix,
        None,
        dir.path(),
        Some("manifest.json"),
    )
    .expect("extract object");

    assert_eq!(output.paths.len(), 4);
    assert_eq!(output.result.extracted.len(), 4);
    let mut manifest = String::new();
    File::open(output.manifest_path.expect("manifest path"))
        .expect("open manifest")
        .read_to_string(&mut manifest)
        .expect("read manifest");
    assert!(manifest.contains("\"object_kind\":\"SparseMatrix\""));
}

#[test]
fn expand_s4_slot_paths_root() {
    let mut slots = indexmap::IndexMap::new();
    slots.insert(
        Arc::from("x"),
        RObject::Real(VectorData::Owned(vec![1.0, 2.0])),
    );
    slots.insert(Arc::from("i"), RObject::Integer(VectorData::Owned(vec![1])));
    let s4 = S4ObjectData {
        class: vec![Arc::from("dgCMatrix")],
        package: None,
        slots,
    };
    let obj = RObject::S4Object(Box::new(s4));

    let mut paths = expand_s4_slot_paths(&obj, "").expect("expand s4 slots");
    paths.sort();
    assert_eq!(paths, vec!["i".to_string(), "x".to_string()]);
}

#[test]
fn expand_list_index_paths_root() {
    let obj = RObject::List(vec![
        RObject::Integer(VectorData::Owned(vec![1])),
        RObject::Real(VectorData::Owned(vec![2.0])),
    ]);

    let mut paths = expand_list_index_paths(&obj, "").expect("expand list");
    paths.sort();
    assert_eq!(paths, vec!["[0]".to_string(), "[1]".to_string()]);
}

#[test]
fn expand_sparse_matrix_paths_root() {
    let mut slots = indexmap::IndexMap::new();
    slots.insert(
        Arc::from("x"),
        RObject::Real(VectorData::Owned(vec![1.0, 2.0])),
    );
    slots.insert(
        Arc::from("i"),
        RObject::Integer(VectorData::Owned(vec![0, 1])),
    );
    slots.insert(
        Arc::from("p"),
        RObject::Integer(VectorData::Owned(vec![0, 2])),
    );
    slots.insert(
        Arc::from("Dim"),
        RObject::Integer(VectorData::Owned(vec![2, 2])),
    );
    slots.insert(Arc::from("Dimnames"), RObject::List(vec![]));
    slots.insert(
        Arc::from("other"),
        RObject::Integer(VectorData::Owned(vec![1])),
    );

    let s4 = S4ObjectData {
        class: vec![Arc::from("dgCMatrix")],
        package: None,
        slots,
    };
    let obj = RObject::S4Object(Box::new(s4));

    let paths = expand_sparse_matrix_paths(&obj, "").expect("expand sparse paths");
    assert_eq!(
        paths,
        vec![
            "x".to_string(),
            "i".to_string(),
            "p".to_string(),
            "Dim".to_string(),
            "Dimnames".to_string()
        ]
    );
}

#[test]
fn expand_dense_matrix_paths_root() {
    let obj = RObject::WithAttributes {
        object: Box::new(RObject::Real(VectorData::Owned(vec![1.0, 2.0, 3.0, 4.0]))),
        attributes: {
            let mut attrs = rds2rust::Attributes::new();
            attrs.insert(
                Arc::from("dim"),
                RObject::Integer(VectorData::Owned(vec![2, 2])),
            );
            attrs.insert(Arc::from("dimnames"), RObject::List(vec![]));
            attrs
        },
    };

    let paths = expand_dense_matrix_paths(&obj, "").expect("expand dense matrix");
    assert_eq!(
        paths,
        vec!["".to_string(), "dim".to_string(), "dimnames".to_string()]
    );
}
