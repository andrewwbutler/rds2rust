use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::{Error, LazyVector, Logical, Result, RObject, VectorData};
use crate::constants::{CHARSXP, REFSXP};

const DEFAULT_STREAM_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorKind {
    Integer,
    Real,
    Logical,
    Raw,
    Complex,
    Character,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Big,
    Little,
}

#[derive(Debug, Clone)]
pub struct ExtractedVectorInfo {
    pub path: String,
    pub file_path: PathBuf,
    pub kind: VectorKind,
    pub length: usize,
    pub elem_size: usize,
    pub endian: Endian,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub extracted: Vec<ExtractedVectorInfo>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    DataFrame,
    DenseMatrix,
    SparseMatrix,
    List,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::DataFrame => "DataFrame",
            ObjectKind::DenseMatrix => "DenseMatrix",
            ObjectKind::SparseMatrix => "SparseMatrix",
            ObjectKind::List => "List",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectExtractionOutput {
    pub paths: Vec<String>,
    pub result: ExtractionResult,
    pub manifest_path: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct ExtractionOutput {
    pub result: ExtractionResult,
    pub manifest_path: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_vectors_from_path<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    paths: &[&str],
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ExtractionOutput> {
    let source = crate::MmapRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_config(source.as_slice(), config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);
    let result = extract_vectors_to_raw_files(&obj, source.as_slice(), paths, budget_bytes, &out_dir)?;

    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest(&out_dir, &result, name)?)
    } else {
        None
    };

    Ok(ExtractionOutput {
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_vectors_from_path_chunked<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    paths: &[&str],
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ExtractionOutput> {
    let source = crate::ChunkedRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_input(&source, config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);
    let result = extract_vectors_to_raw_files_with_input(&obj, &source, paths, budget_bytes, &out_dir)?;

    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest(&out_dir, &result, name)?)
    } else {
        None
    };

    Ok(ExtractionOutput {
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_from_path<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    path: &str,
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    let source = crate::MmapRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_config(source.as_slice(), config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);

    extract_object_to_raw_files(&obj, source.as_slice(), path, budget_bytes, &out_dir, manifest_name)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_from_path_chunked<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    path: &str,
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    let source = crate::ChunkedRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_input(&source, config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);

    extract_object_to_raw_files_with_input(&obj, &source, path, budget_bytes, &out_dir, manifest_name)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_from_path_with_kind<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    path: &str,
    kind: ObjectKind,
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    let source = crate::MmapRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_config(source.as_slice(), config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);

    extract_object_to_raw_files_with_kind(
        &obj,
        source.as_slice(),
        path,
        kind,
        budget_bytes,
        &out_dir,
        manifest_name,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_from_path_with_kind_chunked<P: AsRef<Path>>(
    input_path: P,
    out_dir: PathBuf,
    path: &str,
    kind: ObjectKind,
    budget_mb: Option<usize>,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    let source = crate::ChunkedRdsSource::from_path(input_path.as_ref())?;
    std::fs::create_dir_all(&out_dir)?;
    let config = match budget_mb {
        Some(budget) => crate::ParseConfig::for_constrained_conversion(budget),
        None => crate::ParseConfig::for_trusted_large_file(),
    };
    let obj = crate::read_rds_with_input(&source, config)?;
    let budget_bytes = budget_mb.map(|mb| mb * 1024 * 1024);

    extract_object_to_raw_files_with_kind_and_input(
        &obj,
        &source,
        path,
        kind,
        budget_bytes,
        &out_dir,
        manifest_name,
    )
}

pub fn expand_object_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let expanded = expand_dataframe_paths(obj, path)?;
    if !expanded.is_empty() {
        return Ok(expanded);
    }

    let expanded = expand_sparse_matrix_paths(obj, path)?;
    if !expanded.is_empty() {
        return Ok(expanded);
    }

    let expanded = expand_dense_matrix_paths(obj, path)?;
    if !expanded.is_empty() {
        return Ok(expanded);
    }

    let expanded = expand_list_index_paths(obj, path)?;
    if !expanded.is_empty() {
        return Ok(expanded);
    }

    if path.is_empty() {
        Ok(vec![String::new()])
    } else {
        Ok(vec![path.to_string()])
    }
}

pub fn expand_object_paths_for_kind(
    obj: &RObject,
    path: &str,
    kind: ObjectKind,
) -> Result<Vec<String>> {
    let detected = object_kind_at_path(obj, path)?;
    if detected != Some(kind) {
        return Err(Error::InvalidFormat(format!(
            "object at '{}' is not a {}",
            path,
            kind.as_str()
        )));
    }

    match kind {
        ObjectKind::DataFrame => expand_dataframe_paths(obj, path),
        ObjectKind::SparseMatrix => expand_sparse_matrix_paths(obj, path),
        ObjectKind::DenseMatrix => expand_dense_matrix_paths(obj, path),
        ObjectKind::List => expand_list_index_paths(obj, path),
    }
}

pub fn extract_object_to_raw_files(
    obj: &RObject,
    data: &[u8],
    path: &str,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths(obj, path)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_to_raw_files(obj, data, &path_refs, budget_bytes, out_dir)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest_with_kind(
            out_dir, &result, name, None,
        )?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_to_raw_files_with_input(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths(obj, path)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_to_raw_files_with_input(obj, input, &path_refs, budget_bytes, out_dir)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest(out_dir, &result, name)?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_to_raw_files_with_input_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    chunk_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths(obj, path)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_streaming(obj, input, &path_refs, out_dir, chunk_bytes)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest(out_dir, &result, name)?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}

pub fn extract_object_to_raw_files_with_kind(
    obj: &RObject,
    data: &[u8],
    path: &str,
    kind: ObjectKind,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths_for_kind(obj, path, kind)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_to_raw_files(obj, data, &path_refs, budget_bytes, out_dir)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest_with_kind(
            out_dir,
            &result,
            name,
            Some(kind),
        )?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_to_raw_files_with_kind_and_input(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    kind: ObjectKind,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths_for_kind(obj, path, kind)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_to_raw_files_with_input(obj, input, &path_refs, budget_bytes, out_dir)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest_with_kind(
            out_dir,
            &result,
            name,
            Some(kind),
        )?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_object_to_raw_files_with_kind_and_input_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    kind: ObjectKind,
    chunk_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    std::fs::create_dir_all(out_dir)?;
    let paths = expand_object_paths_for_kind(obj, path, kind)?;
    let path_refs: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    let result = extract_vectors_streaming(obj, input, &path_refs, out_dir, chunk_bytes)?;
    let manifest_path = if let Some(name) = manifest_name {
        Some(write_extraction_manifest_with_kind(
            out_dir,
            &result,
            name,
            Some(kind),
        )?)
    } else {
        None
    };

    Ok(ObjectExtractionOutput {
        paths,
        result,
        manifest_path,
    })
}
pub fn convert_object_to_raw_dump(
    obj: &RObject,
    data: &[u8],
    kind: ObjectKind,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    extract_object_to_raw_files_with_kind(
        obj,
        data,
        "",
        kind,
        budget_bytes,
        out_dir,
        manifest_name,
    )
}

pub fn convert_object_to_raw_dump_at_path(
    obj: &RObject,
    data: &[u8],
    path: &str,
    kind: ObjectKind,
    budget_bytes: Option<usize>,
    out_dir: &Path,
    manifest_name: Option<&str>,
) -> Result<ObjectExtractionOutput> {
    extract_object_to_raw_files_with_kind(
        obj,
        data,
        path,
        kind,
        budget_bytes,
        out_dir,
        manifest_name,
    )
}
pub fn expand_dataframe_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let tokens = parse_path_tokens(path)?;
    match collect_object_info(obj, &tokens)? {
        Some(ObjectInfo::DataFrameColumns(columns)) => Ok(prefix_paths(path, &columns)),
        _ => Ok(Vec::new()),
    }
}

pub fn expand_s4_slot_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let tokens = parse_path_tokens(path)?;
    match collect_object_info(obj, &tokens)? {
        Some(ObjectInfo::S4Slots(slots)) => Ok(prefix_paths(path, &slots)),
        _ => Ok(Vec::new()),
    }
}

pub fn expand_list_index_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let tokens = parse_path_tokens(path)?;
    match collect_object_info(obj, &tokens)? {
        Some(ObjectInfo::ListIndices(indices)) => Ok(prefix_indices(path, indices)),
        _ => Ok(Vec::new()),
    }
}

pub fn expand_sparse_matrix_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let tokens = parse_path_tokens(path)?;
    let Some(ObjectInfo::S4Slots(slots)) = collect_object_info(obj, &tokens)? else {
        return Ok(Vec::new());
    };

    let mut selected = Vec::new();
    for slot in ["x", "i", "p", "Dim", "Dimnames"] {
        if slots.iter().any(|s| s.as_str() == slot) {
            selected.push(slot.to_string());
        }
    }

    Ok(prefix_paths(path, &selected))
}

pub fn expand_dense_matrix_paths(obj: &RObject, path: &str) -> Result<Vec<String>> {
    let tokens = parse_path_tokens(path)?;
    let Some(ObjectInfo::WithAttributes(attribute_keys)) = collect_object_info(obj, &tokens)? else {
        return Ok(Vec::new());
    };

    let mut selected = Vec::new();
    for attr in ["dim", "dimnames"] {
        if attribute_keys.iter().any(|key| key.as_str() == attr) {
            selected.push(attr.to_string());
        }
    }

    let mut paths = Vec::with_capacity(selected.len() + 1);
    paths.push(path.to_string());
    if !selected.is_empty() {
        paths.extend(prefix_paths(path, &selected));
    }
    Ok(paths)
}

pub fn write_extraction_manifest<P: AsRef<Path>>(
    out_dir: P,
    result: &ExtractionResult,
    file_name: &str,
) -> Result<PathBuf> {
    write_extraction_manifest_with_kind(out_dir, result, file_name, None)
}

pub fn write_extraction_manifest_with_kind<P: AsRef<Path>>(
    out_dir: P,
    result: &ExtractionResult,
    file_name: &str,
    object_kind: Option<ObjectKind>,
) -> Result<PathBuf> {
    const MANIFEST_VERSION: u32 = 1;
    let path = out_dir.as_ref().join(file_name);
    let mut file = File::create(&path)?;

    file.write_all(b"{")?;
    file.write_all(format!("\"version\":{},", MANIFEST_VERSION).as_bytes())?;
    let kind = object_kind.map(|kind| kind.as_str()).unwrap_or("Unknown");
    write_json_field(&mut file, "object_kind", kind)?;
    file.write_all(b",")?;
    file.write_all(b"\"vectors\":[")?;

    for (idx, info) in result.extracted.iter().enumerate() {
        if idx > 0 {
            file.write_all(b",")?;
        }
        file.write_all(b"{")?;
        let display_path = if info.path.is_empty() {
            "root"
        } else {
            info.path.as_str()
        };
        write_json_field(&mut file, "path", display_path)?;
        file.write_all(b",")?;
        let file_name = info
            .file_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| info.file_path.to_string_lossy().to_string());
        write_json_field(&mut file, "file", &file_name)?;
        file.write_all(b",")?;
        write_json_field(&mut file, "kind", &format!("{:?}", info.kind))?;
        file.write_all(b",")?;
        file.write_all(format!("\"length\":{}", info.length).as_bytes())?;
        file.write_all(b",")?;
        file.write_all(format!("\"elem_size\":{}", info.elem_size).as_bytes())?;
        file.write_all(b",")?;
        write_json_field(&mut file, "endian", &format!("{:?}", info.endian))?;
        file.write_all(b"}")?;
    }

    file.write_all(b"],")?;
    file.write_all(b"\"missing\":[")?;
    for (idx, missing) in result.missing.iter().enumerate() {
        if idx > 0 {
            file.write_all(b",")?;
        }
        write_json_string(&mut file, missing)?;
    }
    file.write_all(b"]}")?;
    file.write_all(b"\n")?;

    Ok(path)
}

pub fn extract_vectors_to_raw_files(
    obj: &RObject,
    data: &[u8],
    paths: &[&str],
    budget_bytes: Option<usize>,
    out_dir: &Path,
) -> Result<ExtractionResult> {
    extract_vectors_to_raw_files_internal(obj, data, None, paths, budget_bytes, out_dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_vectors_to_raw_files_with_input(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    paths: &[&str],
    budget_bytes: Option<usize>,
    out_dir: &Path,
) -> Result<ExtractionResult> {
    extract_vectors_to_raw_files_internal(obj, &[], Some(input), paths, budget_bytes, out_dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_raw_vector_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractedVectorInfo> {
    std::fs::create_dir_all(out_dir)?;
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing raw vector at '{}'",
            path
        )));
    };

    let file_path = out_dir.join(sanitize_path(path));
    let mut file = File::create(&file_path)?;

    let (kind, length, elem_size) = match target {
        VectorTarget::Raw(vec) => {
            write_header(&mut file, VectorKind::Raw, vec.len(), 1)?;
            file.write_all(vec.as_ref())?;
            (VectorKind::Raw, vec.len(), 1)
        }
        VectorTarget::LazyRaw(span) => {
            write_header(&mut file, VectorKind::Raw, span.length, 1)?;
            stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
            (VectorKind::Raw, span.length, 1)
        }
        _ => {
            return Err(Error::InvalidFormat(format!(
                "object at '{}' is not a raw vector",
                path
            )));
        }
    };

    Ok(ExtractedVectorInfo {
        path: path.to_string(),
        file_path,
        kind,
        length,
        elem_size,
        endian: Endian::Big,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_vectors_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    paths: &[&str],
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractionResult> {
    std::fs::create_dir_all(out_dir)?;
    let mut extracted = Vec::new();
    let mut missing = Vec::new();

    for path in paths {
        let Some(target) = find_vector_at_path(obj, path)? else {
            missing.push((*path).to_string());
            continue;
        };

        let file_path = out_dir.join(sanitize_path(path));
        let mut file = File::create(&file_path)?;

        let (kind, length, elem_size) = match target {
            VectorTarget::Integer(vec) => {
                write_header(&mut file, VectorKind::Integer, vec.len(), 4)?;
                for value in vec.iter() {
                    file.write_i32::<BigEndian>(*value)?;
                }
                (VectorKind::Integer, vec.len(), 4)
            }
            VectorTarget::Real(vec) => {
                write_header(&mut file, VectorKind::Real, vec.len(), 8)?;
                for value in vec.iter() {
                    file.write_f64::<BigEndian>(*value)?;
                }
                (VectorKind::Real, vec.len(), 8)
            }
            VectorTarget::Logical(vec) => {
                write_header(&mut file, VectorKind::Logical, vec.len(), 4)?;
                for value in vec.iter() {
                    let encoded = match value {
                        Logical::False => 0,
                        Logical::True => 1,
                        Logical::Na => i32::MIN,
                    };
                    file.write_i32::<BigEndian>(encoded)?;
                }
                (VectorKind::Logical, vec.len(), 4)
            }
            VectorTarget::Raw(vec) => {
                write_header(&mut file, VectorKind::Raw, vec.len(), 1)?;
                file.write_all(vec.as_ref())?;
                (VectorKind::Raw, vec.len(), 1)
            }
            VectorTarget::Complex(vec) => {
                write_header(&mut file, VectorKind::Complex, vec.len(), 16)?;
                for value in vec.iter() {
                    file.write_f64::<BigEndian>(value.real)?;
                    file.write_f64::<BigEndian>(value.imaginary)?;
                }
                (VectorKind::Complex, vec.len(), 16)
            }
            VectorTarget::Character(vec) => {
                write_header(&mut file, VectorKind::Character, vec.len(), 0)?;
                for value in vec.iter() {
                    let bytes = value.as_bytes();
                    file.write_i32::<BigEndian>(bytes.len() as i32)?;
                    file.write_all(bytes)?;
                }
                (VectorKind::Character, vec.len(), 0)
            }
            VectorTarget::LazyInteger(span) => {
                write_header(&mut file, VectorKind::Integer, span.length, 4)?;
                stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
                (VectorKind::Integer, span.length, 4)
            }
            VectorTarget::LazyReal(span) => {
                write_header(&mut file, VectorKind::Real, span.length, 8)?;
                stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
                (VectorKind::Real, span.length, 8)
            }
            VectorTarget::LazyLogical(span) => {
                write_header(&mut file, VectorKind::Logical, span.length, 4)?;
                stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
                (VectorKind::Logical, span.length, 4)
            }
            VectorTarget::LazyRaw(span) => {
                write_header(&mut file, VectorKind::Raw, span.length, 1)?;
                stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
                (VectorKind::Raw, span.length, 1)
            }
            VectorTarget::LazyComplex(span) => {
                write_header(&mut file, VectorKind::Complex, span.length, 16)?;
                stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
                (VectorKind::Complex, span.length, 16)
            }
            VectorTarget::LazyCharacter(span) => {
                write_lazy_character_vector_streaming(input, span, &mut file, budget_bytes)?;
                (VectorKind::Character, span.length, 0)
            }
        };

        extracted.push(ExtractedVectorInfo {
            path: (*path).to_string(),
            file_path,
            kind,
            length,
            elem_size,
            endian: Endian::Big,
        });
    }

    Ok(ExtractionResult { extracted, missing })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_integer_vector_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractedVectorInfo> {
    std::fs::create_dir_all(out_dir)?;
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing integer vector at '{}'",
            path
        )));
    };

    let file_path = out_dir.join(sanitize_path(path));
    let mut file = File::create(&file_path)?;

    let (kind, length, elem_size) = match target {
        VectorTarget::Integer(vec) => {
            write_header(&mut file, VectorKind::Integer, vec.len(), 4)?;
            for value in vec.iter() {
                file.write_i32::<BigEndian>(*value)?;
            }
            (VectorKind::Integer, vec.len(), 4)
        }
        VectorTarget::LazyInteger(span) => {
            write_header(&mut file, VectorKind::Integer, span.length, 4)?;
            stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
            (VectorKind::Integer, span.length, 4)
        }
        _ => {
            return Err(Error::InvalidFormat(format!(
                "object at '{}' is not an integer vector",
                path
            )));
        }
    };

    Ok(ExtractedVectorInfo {
        path: path.to_string(),
        file_path,
        kind,
        length,
        elem_size,
        endian: Endian::Big,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_logical_vector_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractedVectorInfo> {
    std::fs::create_dir_all(out_dir)?;
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing logical vector at '{}'",
            path
        )));
    };

    let file_path = out_dir.join(sanitize_path(path));
    let mut file = File::create(&file_path)?;

    let (kind, length, elem_size) = match target {
        VectorTarget::Logical(vec) => {
            write_header(&mut file, VectorKind::Logical, vec.len(), 4)?;
            for value in vec.iter() {
                let encoded = match value {
                    Logical::False => 0,
                    Logical::True => 1,
                    Logical::Na => i32::MIN,
                };
                file.write_i32::<BigEndian>(encoded)?;
            }
            (VectorKind::Logical, vec.len(), 4)
        }
        VectorTarget::LazyLogical(span) => {
            write_header(&mut file, VectorKind::Logical, span.length, 4)?;
            stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
            (VectorKind::Logical, span.length, 4)
        }
        _ => {
            return Err(Error::InvalidFormat(format!(
                "object at '{}' is not a logical vector",
                path
            )));
        }
    };

    Ok(ExtractedVectorInfo {
        path: path.to_string(),
        file_path,
        kind,
        length,
        elem_size,
        endian: Endian::Big,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_real_vector_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractedVectorInfo> {
    std::fs::create_dir_all(out_dir)?;
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing real vector at '{}'",
            path
        )));
    };

    let file_path = out_dir.join(sanitize_path(path));
    let mut file = File::create(&file_path)?;

    let (kind, length, elem_size) = match target {
        VectorTarget::Real(vec) => {
            write_header(&mut file, VectorKind::Real, vec.len(), 8)?;
            for value in vec.iter() {
                file.write_f64::<BigEndian>(*value)?;
            }
            (VectorKind::Real, vec.len(), 8)
        }
        VectorTarget::LazyReal(span) => {
            write_header(&mut file, VectorKind::Real, span.length, 8)?;
            stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
            (VectorKind::Real, span.length, 8)
        }
        _ => {
            return Err(Error::InvalidFormat(format!(
                "object at '{}' is not a real vector",
                path
            )));
        }
    };

    Ok(ExtractedVectorInfo {
        path: path.to_string(),
        file_path,
        kind,
        length,
        elem_size,
        endian: Endian::Big,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn extract_complex_vector_streaming(
    obj: &RObject,
    input: &dyn crate::RdsInput,
    path: &str,
    out_dir: &Path,
    budget_bytes: Option<usize>,
) -> Result<ExtractedVectorInfo> {
    std::fs::create_dir_all(out_dir)?;
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing complex vector at '{}'",
            path
        )));
    };

    let file_path = out_dir.join(sanitize_path(path));
    let mut file = File::create(&file_path)?;

    let (kind, length, elem_size) = match target {
        VectorTarget::Complex(vec) => {
            write_header(&mut file, VectorKind::Complex, vec.len(), 16)?;
            for value in vec.iter() {
                file.write_f64::<BigEndian>(value.real)?;
                file.write_f64::<BigEndian>(value.imaginary)?;
            }
            (VectorKind::Complex, vec.len(), 16)
        }
        VectorTarget::LazyComplex(span) => {
            write_header(&mut file, VectorKind::Complex, span.length, 16)?;
            stream_span_bytes_from_input(input, span, &mut file, budget_bytes)?;
            (VectorKind::Complex, span.length, 16)
        }
        _ => {
            return Err(Error::InvalidFormat(format!(
                "object at '{}' is not a complex vector",
                path
            )));
        }
    };

    Ok(ExtractedVectorInfo {
        path: path.to_string(),
        file_path,
        kind,
        length,
        elem_size,
        endian: Endian::Big,
    })
}

fn extract_vectors_to_raw_files_internal(
    obj: &RObject,
    data: &[u8],
    #[cfg(not(target_arch = "wasm32"))] input: Option<&dyn crate::RdsInput>,
    #[cfg(target_arch = "wasm32")] _input: Option<&dyn std::any::Any>,
    paths: &[&str],
    budget_bytes: Option<usize>,
    out_dir: &Path,
) -> Result<ExtractionResult> {
    let mut remaining_budget = budget_bytes;
    let mut extracted = Vec::new();
    let mut missing = Vec::new();

    for path in paths {
        let Some(target) = find_vector_at_path(obj, path)? else {
            missing.push((*path).to_string());
            continue;
        };

        let file_path = out_dir.join(sanitize_path(path));
        let mut file = File::create(&file_path)?;

        let (kind, length, elem_size) = match target {
            VectorTarget::Integer(vec) => {
                write_header(&mut file, VectorKind::Integer, vec.len(), 4)?;
                for value in vec.iter() {
                    file.write_i32::<BigEndian>(*value)?;
                }
                (VectorKind::Integer, vec.len(), 4)
            }
            VectorTarget::Real(vec) => {
                write_header(&mut file, VectorKind::Real, vec.len(), 8)?;
                for value in vec.iter() {
                    file.write_f64::<BigEndian>(*value)?;
                }
                (VectorKind::Real, vec.len(), 8)
            }
            VectorTarget::Logical(vec) => {
                write_header(&mut file, VectorKind::Logical, vec.len(), 4)?;
                for value in vec.iter() {
                    let encoded = match value {
                        Logical::False => 0,
                        Logical::True => 1,
                        Logical::Na => i32::MIN,
                    };
                    file.write_i32::<BigEndian>(encoded)?;
                }
                (VectorKind::Logical, vec.len(), 4)
            }
            VectorTarget::Raw(vec) => {
                write_header(&mut file, VectorKind::Raw, vec.len(), 1)?;
                file.write_all(vec.as_ref())?;
                (VectorKind::Raw, vec.len(), 1)
            }
            VectorTarget::Complex(vec) => {
                write_header(&mut file, VectorKind::Complex, vec.len(), 16)?;
                for value in vec.iter() {
                    file.write_f64::<BigEndian>(value.real)?;
                    file.write_f64::<BigEndian>(value.imaginary)?;
                }
                (VectorKind::Complex, vec.len(), 16)
            }
            VectorTarget::Character(vec) => {
                write_header(&mut file, VectorKind::Character, vec.len(), 0)?;
                for value in vec.iter() {
                    let bytes = value.as_bytes();
                    file.write_i32::<BigEndian>(bytes.len() as i32)?;
                    file.write_all(bytes)?;
                }
                (VectorKind::Character, vec.len(), 0)
            }
            VectorTarget::LazyInteger(span) => {
                let input_used = input.is_some();
                write_header(&mut file, VectorKind::Integer, span.length, 4)?;
                stream_span_bytes_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Integer, span.length, 4)
            }
            VectorTarget::LazyReal(span) => {
                let input_used = input.is_some();
                write_header(&mut file, VectorKind::Real, span.length, 8)?;
                stream_span_bytes_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Real, span.length, 8)
            }
            VectorTarget::LazyLogical(span) => {
                let input_used = input.is_some();
                write_header(&mut file, VectorKind::Logical, span.length, 4)?;
                stream_span_bytes_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Logical, span.length, 4)
            }
            VectorTarget::LazyRaw(span) => {
                let input_used = input.is_some();
                write_header(&mut file, VectorKind::Raw, span.length, 1)?;
                stream_span_bytes_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Raw, span.length, 1)
            }
            VectorTarget::LazyComplex(span) => {
                let input_used = input.is_some();
                write_header(&mut file, VectorKind::Complex, span.length, 16)?;
                stream_span_bytes_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Complex, span.length, 16)
            }
            VectorTarget::LazyCharacter(span) => {
                let input_used = input.is_some();
                write_lazy_character_with_optional_input(
                    data,
                    input,
                    span,
                    &mut file,
                    remaining_budget,
                )?;
                if !input_used {
                    charge_budget(&mut remaining_budget, span.byte_len as usize)?;
                }
                (VectorKind::Character, span.length, 0)
            }
        };

        extracted.push(ExtractedVectorInfo {
            path: (*path).to_string(),
            file_path,
            kind,
            length,
            elem_size,
            endian: Endian::Big,
        });
    }

    Ok(ExtractionResult { extracted, missing })
}

fn stream_span_bytes_with_optional_input(
    data: &[u8],
    #[cfg(not(target_arch = "wasm32"))] input: Option<&dyn crate::RdsInput>,
    #[cfg(target_arch = "wasm32")] _input: Option<&dyn std::any::Any>,
    span: LazyVector,
    file: &mut File,
    budget_bytes: Option<usize>,
) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(input) = input {
        return stream_span_bytes_from_input(input, span, file, budget_bytes);
    }
    stream_span_bytes_from_slice(data, span, file, budget_bytes)
}

fn stream_span_bytes_from_slice(
    data: &[u8],
    span: LazyVector,
    file: &mut File,
    budget_bytes: Option<usize>,
) -> Result<()> {
    let chunk_size = resolve_stream_chunk_size(budget_bytes)?;
    let start = span.offset as usize;
    let end = span
        .offset
        .checked_add(span.byte_len)
        .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))? as usize;
    if end > data.len() {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: data.len().saturating_sub(start) as u64,
        });
    }
    let slice = &data[start..end];
    stream_bytes_from_slice(slice, file, chunk_size)
}

#[cfg(not(target_arch = "wasm32"))]
fn stream_span_bytes_from_input(
    input: &dyn crate::RdsInput,
    span: LazyVector,
    file: &mut File,
    budget_bytes: Option<usize>,
) -> Result<()> {
    let mut remaining = span.byte_len as usize;
    let mut offset = span.offset;
    let chunk_size = resolve_stream_chunk_size(budget_bytes)?;

    while remaining > 0 {
        let to_read = remaining.min(chunk_size);
        let chunk = input.read_at(offset, to_read)?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: span.byte_len,
                actual: (span.byte_len as usize - remaining + chunk.len()) as u64,
            });
        }
        file.write_all(&chunk)?;
        remaining -= to_read;
        offset += to_read as u64;
    }

    Ok(())
}

fn stream_bytes_from_slice(data: &[u8], file: &mut File, chunk_size: usize) -> Result<()> {
    let chunk_size = chunk_size.max(1);

    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + chunk_size).min(data.len());
        file.write_all(&data[offset..end])?;
        offset = end;
    }
    Ok(())
}

fn resolve_stream_chunk_size(budget_bytes: Option<usize>) -> Result<usize> {
    if let Some(budget) = budget_bytes {
        if budget == 0 {
            return Err(Error::MemoryBudgetExceeded {
                needed: 1,
                available: 0,
            });
        }
        Ok(DEFAULT_STREAM_CHUNK_BYTES.min(budget))
    } else {
        Ok(DEFAULT_STREAM_CHUNK_BYTES)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_lazy_character_with_optional_input(
    data: &[u8],
    input: Option<&dyn crate::RdsInput>,
    span: LazyVector,
    file: &mut File,
    budget_bytes: Option<usize>,
) -> Result<()> {
    if let Some(input) = input {
        write_lazy_character_vector_streaming(input, span, file, budget_bytes)
    } else {
        write_lazy_character_vector(data, span, file)
    }
}

#[cfg(target_arch = "wasm32")]
fn write_lazy_character_with_optional_input(
    data: &[u8],
    _input: Option<&dyn std::any::Any>,
    span: LazyVector,
    file: &mut File,
    _budget_bytes: Option<usize>,
) -> Result<()> {
    write_lazy_character_vector(data, span, file)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_lazy_character_vector_streaming(
    input: &dyn crate::RdsInput,
    span: LazyVector,
    file: &mut File,
    budget_bytes: Option<usize>,
) -> Result<()> {
    let chunk_size = resolve_stream_chunk_size(budget_bytes)?;
    let mut reader = SpanReader::new(input, span, chunk_size)?;
    write_header(file, VectorKind::Character, span.length, 0)?;
    let mut cache: Vec<Arc<str>> = Vec::new();

    for _ in 0..span.length {
        let flags = reader.read_u32()?;
        let type_from_0_7 = flags & 0xFF;
        let type_from_8_15 = (flags >> 8) & 0xFF;

        if type_from_0_7 == REFSXP {
            let ref_index = (flags >> 8) as usize;
            let value = cache.get(ref_index).ok_or_else(|| {
                Error::InvalidFormat(format!("invalid REFSXP index {}", ref_index))
            })?;
            write_string_record(file, value)?;
            continue;
        }

        if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
            let value = parse_charsxp_content_streaming(&mut reader, flags)?;
            cache.push(value.clone());
            write_string_record(file, &value)?;
            continue;
        }

        return Err(Error::Unsupported(
            "non-CHARSXP element in character vector".to_string(),
        ));
    }

    Ok(())
}

fn charge_budget(remaining: &mut Option<usize>, bytes: usize) -> Result<()> {
    if let Some(remaining) = remaining {
        if bytes > *remaining {
            return Err(Error::MemoryBudgetExceeded {
                needed: bytes,
                available: *remaining,
            });
        }
        *remaining -= bytes;
    }
    Ok(())
}

fn write_json_field(file: &mut File, key: &str, value: &str) -> Result<()> {
    write_json_string(file, key)?;
    file.write_all(b":")?;
    write_json_string(file, value)?;
    Ok(())
}

fn write_json_string(file: &mut File, value: &str) -> Result<()> {
    file.write_all(b"\"")?;
    for ch in value.chars() {
        match ch {
            '"' => file.write_all(b"\\\"")?,
            '\\' => file.write_all(b"\\\\")?,
            '\n' => file.write_all(b"\\n")?,
            '\r' => file.write_all(b"\\r")?,
            '\t' => file.write_all(b"\\t")?,
            _ => {
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                file.write_all(encoded.as_bytes())?;
            }
        }
    }
    file.write_all(b"\"")?;
    Ok(())
}

fn slice_for_span<'a>(data: &'a [u8], span: LazyVector) -> Result<&'a [u8]> {
    let start = span.offset as usize;
    let end = span
        .offset
        .checked_add(span.byte_len)
        .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))? as usize;

    if start > data.len() {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: 0,
        });
    }

    let available = data.len() - start;
    if end > data.len() {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: available as u64,
        });
    }

    Ok(&data[start..end])
}

fn write_lazy_character_vector(data: &[u8], span: LazyVector, file: &mut File) -> Result<()> {
    write_header(file, VectorKind::Character, span.length, 0)?;
    let slice = slice_for_span(data, span)?;
    let mut cursor = std::io::Cursor::new(slice);
    let mut cache: Vec<Arc<str>> = Vec::new();

    for _ in 0..span.length {
        let flags = cursor.read_u32::<BigEndian>()?;
        let type_from_0_7 = flags & 0xFF;
        let type_from_8_15 = (flags >> 8) & 0xFF;

        if type_from_0_7 == REFSXP {
            let ref_index = (flags >> 8) as usize;
            let value = cache.get(ref_index).ok_or_else(|| {
                Error::InvalidFormat(format!("invalid REFSXP index {}", ref_index))
            })?;
            write_string_record(file, value)?;
            continue;
        }

        if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
            let value = parse_charsxp_content_raw(&mut cursor, flags)?;
            cache.push(value.clone());
            write_string_record(file, &value)?;
            continue;
        }

        return Err(Error::Unsupported(
            "non-CHARSXP element in character vector".to_string(),
        ));
    }

    Ok(())
}

fn write_string_record(file: &mut File, value: &Arc<str>) -> Result<()> {
    let bytes = value.as_bytes();
    file.write_i32::<BigEndian>(bytes.len() as i32)?;
    file.write_all(bytes)?;
    Ok(())
}

fn parse_charsxp_content_raw(
    cursor: &mut std::io::Cursor<&[u8]>,
    flags: u32,
) -> Result<Arc<str>> {
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    let length = if use_compact {
        let mut bytes_3 = [0u8; 3];
        cursor.read_exact(&mut bytes_3)?;
        ((bytes_3[0] as i32) << 16) | ((bytes_3[1] as i32) << 8) | (bytes_3[2] as i32)
    } else {
        cursor.read_i32::<BigEndian>()?
    };

    if length == -1 {
        return Ok(Arc::from("NA"));
    }
    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative CHARSXP length {}",
            length
        )));
    }

    let length = length as usize;
    let mut bytes = vec![0u8; length];
    cursor.read_exact(&mut bytes)?;

    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };

    Ok(Arc::from(string))
}

#[cfg(not(target_arch = "wasm32"))]
struct SpanReader<'a> {
    input: &'a dyn crate::RdsInput,
    start: u64,
    end: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_pos: usize,
    buf_len: usize,
    chunk_size: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> SpanReader<'a> {
    fn new(input: &'a dyn crate::RdsInput, span: LazyVector, chunk_size: usize) -> Result<Self> {
        let end = span
            .offset
            .checked_add(span.byte_len)
            .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))?;
        Ok(Self {
            input,
            start: span.offset,
            end,
            pos: span.offset,
            buf: vec![0u8; chunk_size.max(1)],
            buf_pos: 0,
            buf_len: 0,
            chunk_size: chunk_size.max(1),
        })
    }

    fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    fn fill(&mut self) -> Result<()> {
        if self.remaining() == 0 {
            return Err(Error::TruncatedLazyPayload {
                expected: self.end - self.start,
                actual: self.pos - self.start,
            });
        }
        let to_read = self
            .remaining()
            .min(self.chunk_size as u64) as usize;
        let chunk = self.input.read_at(self.pos, to_read)?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: self.end - self.start,
                actual: self.pos - self.start + chunk.len() as u64,
            });
        }
        self.buf[..to_read].copy_from_slice(&chunk);
        self.buf_pos = 0;
        self.buf_len = to_read;
        self.pos += to_read as u64;
        Ok(())
    }

    fn read_exact(&mut self, out: &mut [u8]) -> Result<()> {
        let mut written = 0;
        while written < out.len() {
            if self.buf_pos == self.buf_len {
                self.fill()?;
            }
            let available = self.buf_len - self.buf_pos;
            let needed = out.len() - written;
            let to_copy = available.min(needed);
            out[written..written + to_copy]
                .copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + to_copy]);
            self.buf_pos += to_copy;
            written += to_copy;
        }
        Ok(())
    }

    fn read_u32(&mut self) -> Result<u32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_i32(&mut self) -> Result<i32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes)?;
        Ok(i32::from_be_bytes(bytes))
    }

    fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_charsxp_content_streaming(reader: &mut SpanReader<'_>, flags: u32) -> Result<Arc<str>> {
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    let length = if use_compact {
        let bytes = reader.read_bytes(3)?;
        ((bytes[0] as i32) << 16) | ((bytes[1] as i32) << 8) | (bytes[2] as i32)
    } else {
        reader.read_i32()?
    };

    if length == -1 {
        return Ok(Arc::from("NA"));
    }
    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative CHARSXP length {}",
            length
        )));
    }

    let length = length as usize;
    let bytes = reader.read_bytes(length)?;
    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok(Arc::from(string))
}

fn write_header(file: &mut File, kind: VectorKind, length: usize, elem_size: usize) -> Result<()> {
    const MAGIC: &[u8; 8] = b"RDS2VEC1";
    file.write_all(MAGIC)?;
    file.write_u8(1)?; // version
    file.write_u8(kind_to_tag(kind))?;
    file.write_u8(endian_to_tag(Endian::Big))?;
    file.write_u8(0)?; // reserved
    file.write_u64::<BigEndian>(length as u64)?;
    file.write_u32::<BigEndian>(elem_size as u32)?;
    Ok(())
}

fn kind_to_tag(kind: VectorKind) -> u8 {
    match kind {
        VectorKind::Integer => 1,
        VectorKind::Real => 2,
        VectorKind::Logical => 3,
        VectorKind::Raw => 4,
        VectorKind::Complex => 5,
        VectorKind::Character => 6,
    }
}

fn endian_to_tag(endian: Endian) -> u8 {
    match endian {
        Endian::Big => 1,
        Endian::Little => 2,
    }
}

fn sanitize_path(path: &str) -> String {
    let trimmed = if path.is_empty() { "root" } else { path };
    let mut sanitized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    sanitized.push_str(".rdsvec");
    sanitized
}

#[derive(Debug)]
enum ObjectInfo {
    DataFrameColumns(Vec<String>),
    S4Slots(Vec<String>),
    ListIndices(Vec<usize>),
    WithAttributes(Vec<String>),
}

#[derive(Debug)]
enum PathToken {
    Field(String),
    Index(usize),
}

#[derive(Debug)]
enum VectorTarget<'a> {
    Integer(Cow<'a, [i32]>),
    Real(Cow<'a, [f64]>),
    Logical(Cow<'a, [Logical]>),
    Raw(Cow<'a, [u8]>),
    Complex(Cow<'a, [crate::Complex]>),
    Character(Cow<'a, [std::sync::Arc<str>]>),
    LazyInteger(LazyVector),
    LazyReal(LazyVector),
    LazyLogical(LazyVector),
    LazyRaw(LazyVector),
    LazyComplex(LazyVector),
    LazyCharacter(LazyVector),
}

fn find_vector_at_path<'a>(obj: &'a RObject, path: &str) -> Result<Option<VectorTarget<'a>>> {
    let tokens = parse_path_tokens(path)?;
    find_vector_tokens(obj, &tokens)
}

fn parse_path_tokens(path: &str) -> Result<Vec<PathToken>> {
    let mut tokens = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
            }
            b'[' => {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if i >= bytes.len() || bytes[i] != quote {
                        return Err(Error::InvalidFormat(format!(
                            "unterminated quoted field in '{}'",
                            path
                        )));
                    }
                    let field = path[start..i].to_string();
                    i += 1;
                    if i >= bytes.len() || bytes[i] != b']' {
                        return Err(Error::InvalidFormat(format!(
                            "invalid quoted field in '{}'",
                            path
                        )));
                    }
                    tokens.push(PathToken::Field(field));
                    i += 1;
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if start == i || i >= bytes.len() || bytes[i] != b']' {
                        return Err(Error::InvalidFormat(format!(
                            "invalid path index in '{}'",
                            path
                        )));
                    }
                    let index: usize = path[start..i]
                        .parse()
                        .map_err(|_| Error::InvalidFormat(format!("invalid index in '{}'", path)))?;
                    tokens.push(PathToken::Index(index));
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                let field = path[start..i].to_string();
                if field.is_empty() {
                    return Err(Error::InvalidFormat(format!("invalid path '{}'", path)));
                }
                tokens.push(PathToken::Field(field));
            }
        }
    }

    Ok(tokens)
}

fn collect_object_info(obj: &RObject, tokens: &[PathToken]) -> Result<Option<ObjectInfo>> {
    use RObject::*;

    if tokens.is_empty() {
        return Ok(match obj {
            DataFrame(df) => Some(ObjectInfo::DataFrameColumns(
                df.columns.keys().map(|k| k.to_string()).collect(),
            )),
            S4Object(s4) => Some(ObjectInfo::S4Slots(
                s4.slots.keys().map(|k| k.to_string()).collect(),
            )),
            List(items) | Expression(items) => {
                Some(ObjectInfo::ListIndices((0..items.len()).collect()))
            }
            WithAttributes { attributes, .. } => Some(ObjectInfo::WithAttributes(
                attributes.attrs.iter().map(|(k, _)| k.to_string()).collect(),
            )),
            Shared(inner) => {
                let inner = inner.read().unwrap();
                collect_object_info(&inner, tokens)?
            }
            _ => None,
        });
    }

    match &tokens[0] {
        PathToken::Field(name) => match obj {
            DataFrame(df) => match df.columns.get(name.as_str()) {
                Some(col) => collect_object_info(col, &tokens[1..]),
                None => Ok(None),
            },
            S4Object(s4) => match s4.slots.get(name.as_str()) {
                Some(slot) => collect_object_info(slot, &tokens[1..]),
                None => Ok(None),
            },
            S3Object(s3) => {
                if name == "base" {
                    collect_object_info(&s3.base, &tokens[1..])
                } else {
                    Ok(None)
                }
            }
            Closure {
                formals,
                body,
                environment,
            } => match name.as_str() {
                "formals" => collect_object_info(formals, &tokens[1..]),
                "body" => collect_object_info(body, &tokens[1..]),
                "environment" => collect_object_info(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Environment {
                enclosing,
                frame,
                hashtab,
            } => match name.as_str() {
                "enclosing" => collect_object_info(enclosing, &tokens[1..]),
                "frame" => collect_object_info(frame, &tokens[1..]),
                "hashtab" => collect_object_info(hashtab, &tokens[1..]),
                _ => Ok(None),
            },
            Promise {
                value,
                expression,
                environment,
            } => match name.as_str() {
                "value" => collect_object_info(value, &tokens[1..]),
                "expression" => collect_object_info(expression, &tokens[1..]),
                "environment" => collect_object_info(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Bytecode {
                code,
                constants,
                expr,
            } => match name.as_str() {
                "code" => collect_object_info(code, &tokens[1..]),
                "constants" => collect_object_info(constants, &tokens[1..]),
                "expr" => collect_object_info(expr, &tokens[1..]),
                _ => Ok(None),
            },
            Language { function, args } => match name.as_str() {
                "function" => collect_object_info(function, &tokens[1..]),
                "args" => collect_object_info_pairlist(args, &tokens[1..]),
                _ => Ok(None),
            },
            Pairlist(_) => Ok(None),
            WithAttributes { object, attributes } => {
                if let Some(index) = list_name_index(attributes, name) {
                    if let List(items) | Expression(items) = object.as_ref() {
                        if let Some(item) = items.get(index) {
                            return collect_object_info(item, &tokens[1..]);
                        }
                    }
                }
                collect_object_info(object, tokens)
            }
            Shared(inner) => {
                let inner = inner.read().unwrap();
                collect_object_info(&inner, tokens)
            }
            _ => Ok(None),
        },
        PathToken::Index(index) => match obj {
            List(items) | Expression(items) => match items.get(*index) {
                Some(item) => collect_object_info(item, &tokens[1..]),
                None => Ok(None),
            },
            Pairlist(elements) => collect_object_info_pairlist_index(elements, *index, &tokens[1..]),
            _ => Ok(None),
        },
    }
}

fn object_kind_at_path(obj: &RObject, path: &str) -> Result<Option<ObjectKind>> {
    let tokens = parse_path_tokens(path)?;
    let Some(info) = collect_object_info(obj, &tokens)? else {
        return Ok(None);
    };

    let kind = match info {
        ObjectInfo::DataFrameColumns(_) => Some(ObjectKind::DataFrame),
        ObjectInfo::ListIndices(_) => Some(ObjectKind::List),
        ObjectInfo::S4Slots(slots) => {
            if is_sparse_matrix_slots(&slots) {
                Some(ObjectKind::SparseMatrix)
            } else {
                None
            }
        }
        ObjectInfo::WithAttributes(keys) => {
            if keys.iter().any(|key| key.as_str() == "dim") {
                Some(ObjectKind::DenseMatrix)
            } else {
                None
            }
        }
    };

    Ok(kind)
}

fn list_name_index(attributes: &crate::Attributes, name: &str) -> Option<usize> {
    let obj = attributes.get("names")?;
    list_name_index_from_obj(obj, name)
}

fn list_name_index_from_obj(obj: &RObject, name: &str) -> Option<usize> {
    match obj {
        RObject::Character(VectorData::Owned(values)) => values
            .iter()
            .position(|value| value.as_ref() == name),
        RObject::WithAttributes { object, .. } => list_name_index_from_obj(object, name),
        RObject::Shared(inner) => {
            let inner = inner.read().ok()?;
            list_name_index_from_obj(&inner, name)
        }
        _ => None,
    }
}

fn is_sparse_matrix_slots(slots: &[String]) -> bool {
    let mut has_x = false;
    let mut has_i = false;
    let mut has_p = false;
    let mut has_dim = false;
    for slot in slots {
        match slot.as_str() {
            "x" => has_x = true,
            "i" => has_i = true,
            "p" => has_p = true,
            "Dim" => has_dim = true,
            _ => {}
        }
    }
    has_x && has_i && has_p && has_dim
}

fn collect_object_info_pairlist(
    elements: &[crate::PairlistElement],
    tokens: &[PathToken],
) -> Result<Option<ObjectInfo>> {
    if tokens.is_empty() {
        return Ok(None);
    }
    match &tokens[0] {
        PathToken::Index(index) => collect_object_info_pairlist_index(elements, *index, &tokens[1..]),
        _ => Ok(None),
    }
}

fn collect_object_info_pairlist_index(
    elements: &[crate::PairlistElement],
    index: usize,
    tokens: &[PathToken],
) -> Result<Option<ObjectInfo>> {
    let elem = match elements.get(index) {
        Some(elem) => elem,
        None => return Ok(None),
    };

    if tokens.is_empty() {
        return Ok(None);
    }

    match &tokens[0] {
        PathToken::Field(name) => match name.as_str() {
            "value" => collect_object_info(&elem.value, &tokens[1..]),
            "tag_object" => match elem.tag_object.as_ref() {
                Some(tag) => collect_object_info(tag, &tokens[1..]),
                None => Ok(None),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn prefix_paths(prefix: &str, names: &[String]) -> Vec<String> {
    if prefix.is_empty() {
        return names.to_vec();
    }
    names
        .iter()
        .map(|name| format!("{}.{}", prefix, name))
        .collect()
}

fn prefix_indices(prefix: &str, indices: Vec<usize>) -> Vec<String> {
    let mut paths = Vec::with_capacity(indices.len());
    for index in indices {
        if prefix.is_empty() {
            paths.push(format!("[{}]", index));
        } else {
            paths.push(format!("{}[{}]", prefix, index));
        }
    }
    paths
}

fn find_vector_tokens<'a>(obj: &'a RObject, tokens: &[PathToken]) -> Result<Option<VectorTarget<'a>>> {
    use RObject::*;

    if tokens.is_empty() {
        if let WithAttributes { object, .. } = obj {
            return find_vector_tokens(object, tokens);
        }
        return Ok(find_vector_here(obj));
    }

    match &tokens[0] {
        PathToken::Field(name) => match obj {
            DataFrame(df) => match df.columns.get(name.as_str()) {
                Some(col) => find_vector_tokens(col, &tokens[1..]),
                None => Ok(None),
            },
            S4Object(s4) => match s4.slots.get(name.as_str()) {
                Some(slot) => find_vector_tokens(slot, &tokens[1..]),
                None => Ok(None),
            },
            S3Object(s3) => {
                if name == "base" {
                    find_vector_tokens(&s3.base, &tokens[1..])
                } else {
                    Ok(None)
                }
            }
            Closure {
                formals,
                body,
                environment,
            } => match name.as_str() {
                "formals" => find_vector_tokens(formals, &tokens[1..]),
                "body" => find_vector_tokens(body, &tokens[1..]),
                "environment" => find_vector_tokens(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Environment {
                enclosing,
                frame,
                hashtab,
            } => match name.as_str() {
                "enclosing" => find_vector_tokens(enclosing, &tokens[1..]),
                "frame" => find_vector_tokens(frame, &tokens[1..]),
                "hashtab" => find_vector_tokens(hashtab, &tokens[1..]),
                _ => Ok(None),
            },
            Promise {
                value,
                expression,
                environment,
            } => match name.as_str() {
                "value" => find_vector_tokens(value, &tokens[1..]),
                "expression" => find_vector_tokens(expression, &tokens[1..]),
                "environment" => find_vector_tokens(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Bytecode {
                code,
                constants,
                expr,
            } => match name.as_str() {
                "code" => find_vector_tokens(code, &tokens[1..]),
                "constants" => find_vector_tokens(constants, &tokens[1..]),
                "expr" => find_vector_tokens(expr, &tokens[1..]),
                _ => Ok(None),
            },
            Language { function, args } => match name.as_str() {
                "function" => find_vector_tokens(function, &tokens[1..]),
                "args" => find_pairlist_elements(args, &tokens[1..]),
                _ => Ok(None),
            },
            Pairlist(_) => Ok(None),
            WithAttributes { object, attributes } => match attributes.get(name.as_str()) {
                Some(attr) => find_vector_tokens(attr, &tokens[1..]),
                None => {
                    if let Some(index) = list_name_index(attributes, name) {
                        if let RObject::List(items) | RObject::Expression(items) = object.as_ref()
                        {
                            if let Some(item) = items.get(index) {
                                return find_vector_tokens(item, &tokens[1..]);
                            }
                        }
                    }
                    find_vector_tokens(object, tokens)
                }
            },
            Shared(inner) => {
                let inner = inner.read().unwrap();
                find_vector_tokens_owned(&inner, tokens)
            }
            _ => Ok(None),
        },
        PathToken::Index(index) => match obj {
            List(items) | Expression(items) => match items.get(*index) {
                Some(item) => find_vector_tokens(item, &tokens[1..]),
                None => Ok(None),
            },
            Pairlist(elements) => find_pairlist_index(elements, *index, &tokens[1..]),
            _ => Ok(None),
        },
    }
}

fn find_vector_tokens_owned<'a>(obj: &RObject, tokens: &[PathToken]) -> Result<Option<VectorTarget<'a>>> {
    use RObject::*;

    if tokens.is_empty() {
        if let WithAttributes { object, .. } = obj {
            return find_vector_tokens_owned(object, tokens);
        }
        return Ok(find_vector_here_owned(obj));
    }

    match &tokens[0] {
        PathToken::Field(name) => match obj {
            DataFrame(df) => match df.columns.get(name.as_str()) {
                Some(col) => find_vector_tokens_owned(col, &tokens[1..]),
                None => Ok(None),
            },
            S4Object(s4) => match s4.slots.get(name.as_str()) {
                Some(slot) => find_vector_tokens_owned(slot, &tokens[1..]),
                None => Ok(None),
            },
            S3Object(s3) => {
                if name == "base" {
                    find_vector_tokens_owned(&s3.base, &tokens[1..])
                } else {
                    Ok(None)
                }
            }
            Closure {
                formals,
                body,
                environment,
            } => match name.as_str() {
                "formals" => find_vector_tokens_owned(formals, &tokens[1..]),
                "body" => find_vector_tokens_owned(body, &tokens[1..]),
                "environment" => find_vector_tokens_owned(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Environment {
                enclosing,
                frame,
                hashtab,
            } => match name.as_str() {
                "enclosing" => find_vector_tokens_owned(enclosing, &tokens[1..]),
                "frame" => find_vector_tokens_owned(frame, &tokens[1..]),
                "hashtab" => find_vector_tokens_owned(hashtab, &tokens[1..]),
                _ => Ok(None),
            },
            Promise {
                value,
                expression,
                environment,
            } => match name.as_str() {
                "value" => find_vector_tokens_owned(value, &tokens[1..]),
                "expression" => find_vector_tokens_owned(expression, &tokens[1..]),
                "environment" => find_vector_tokens_owned(environment, &tokens[1..]),
                _ => Ok(None),
            },
            Bytecode {
                code,
                constants,
                expr,
            } => match name.as_str() {
                "code" => find_vector_tokens_owned(code, &tokens[1..]),
                "constants" => find_vector_tokens_owned(constants, &tokens[1..]),
                "expr" => find_vector_tokens_owned(expr, &tokens[1..]),
                _ => Ok(None),
            },
            Language { function, args } => match name.as_str() {
                "function" => find_vector_tokens_owned(function, &tokens[1..]),
                "args" => find_pairlist_elements_owned(args, &tokens[1..]),
                _ => Ok(None),
            },
            Pairlist(_) => Ok(None),
            WithAttributes { object, attributes } => match attributes.get(name.as_str()) {
                Some(attr) => find_vector_tokens_owned(attr, &tokens[1..]),
                None => {
                    if let Some(index) = list_name_index(attributes, name) {
                        if let RObject::List(items) | RObject::Expression(items) = object.as_ref()
                        {
                            if let Some(item) = items.get(index) {
                                return find_vector_tokens_owned(item, &tokens[1..]);
                            }
                        }
                    }
                    find_vector_tokens_owned(object, tokens)
                }
            },
            Shared(inner) => {
                let inner = inner.read().unwrap();
                find_vector_tokens_owned(&inner, tokens)
            }
            _ => Ok(None),
        },
        PathToken::Index(index) => match obj {
            List(items) | Expression(items) => match items.get(*index) {
                Some(item) => find_vector_tokens_owned(item, &tokens[1..]),
                None => Ok(None),
            },
            Pairlist(elements) => find_pairlist_index_owned(elements, *index, &tokens[1..]),
            _ => Ok(None),
        },
    }
}

fn find_pairlist_elements<'a>(
    elements: &'a [crate::PairlistElement],
    tokens: &[PathToken],
) -> Result<Option<VectorTarget<'a>>> {
    if tokens.is_empty() {
        return Ok(None);
    }
    match &tokens[0] {
        PathToken::Index(index) => find_pairlist_index(elements, *index, &tokens[1..]),
        _ => Ok(None),
    }
}

fn find_pairlist_elements_owned<'a>(
    elements: &[crate::PairlistElement],
    tokens: &[PathToken],
) -> Result<Option<VectorTarget<'a>>> {
    if tokens.is_empty() {
        return Ok(None);
    }
    match &tokens[0] {
        PathToken::Index(index) => find_pairlist_index_owned(elements, *index, &tokens[1..]),
        _ => Ok(None),
    }
}

fn find_pairlist_index<'a>(
    elements: &'a [crate::PairlistElement],
    index: usize,
    tokens: &[PathToken],
) -> Result<Option<VectorTarget<'a>>> {
    let elem = match elements.get(index) {
        Some(elem) => elem,
        None => return Ok(None),
    };

    if tokens.is_empty() {
        return Ok(None);
    }

    match &tokens[0] {
        PathToken::Field(name) => match name.as_str() {
            "value" => find_vector_tokens(&elem.value, &tokens[1..]),
            "tag_object" => match elem.tag_object.as_ref() {
                Some(tag) => find_vector_tokens(tag, &tokens[1..]),
                None => Ok(None),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn find_pairlist_index_owned<'a>(
    elements: &[crate::PairlistElement],
    index: usize,
    tokens: &[PathToken],
) -> Result<Option<VectorTarget<'a>>> {
    let elem = match elements.get(index) {
        Some(elem) => elem,
        None => return Ok(None),
    };

    if tokens.is_empty() {
        return Ok(None);
    }

    match &tokens[0] {
        PathToken::Field(name) => match name.as_str() {
            "value" => find_vector_tokens_owned(&elem.value, &tokens[1..]),
            "tag_object" => match elem.tag_object.as_ref() {
                Some(tag) => find_vector_tokens_owned(tag, &tokens[1..]),
                None => Ok(None),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn find_vector_here<'a>(obj: &'a RObject) -> Option<VectorTarget<'a>> {
    match obj {
        RObject::Integer(VectorData::Owned(vec)) => Some(VectorTarget::Integer(Cow::Borrowed(vec))),
        RObject::Real(VectorData::Owned(vec)) => Some(VectorTarget::Real(Cow::Borrowed(vec))),
        RObject::Logical(VectorData::Owned(vec)) => Some(VectorTarget::Logical(Cow::Borrowed(vec))),
        RObject::Raw(VectorData::Owned(vec)) => Some(VectorTarget::Raw(Cow::Borrowed(vec))),
        RObject::Complex(VectorData::Owned(vec)) => Some(VectorTarget::Complex(Cow::Borrowed(vec))),
        RObject::Character(VectorData::Owned(vec)) => {
            Some(VectorTarget::Character(Cow::Borrowed(vec)))
        }
        RObject::Integer(VectorData::Lazy(span)) => Some(VectorTarget::LazyInteger(*span)),
        RObject::Real(VectorData::Lazy(span)) => Some(VectorTarget::LazyReal(*span)),
        RObject::Logical(VectorData::Lazy(span)) => Some(VectorTarget::LazyLogical(*span)),
        RObject::Raw(VectorData::Lazy(span)) => Some(VectorTarget::LazyRaw(*span)),
        RObject::Complex(VectorData::Lazy(span)) => Some(VectorTarget::LazyComplex(*span)),
        RObject::Character(VectorData::Lazy(span)) => Some(VectorTarget::LazyCharacter(*span)),
        _ => None,
    }
}

fn find_vector_here_owned<'a>(obj: &RObject) -> Option<VectorTarget<'a>> {
    match obj {
        RObject::Integer(VectorData::Owned(vec)) => {
            Some(VectorTarget::Integer(Cow::Owned(vec.clone())))
        }
        RObject::Real(VectorData::Owned(vec)) => Some(VectorTarget::Real(Cow::Owned(vec.clone()))),
        RObject::Logical(VectorData::Owned(vec)) => {
            Some(VectorTarget::Logical(Cow::Owned(vec.clone())))
        }
        RObject::Raw(VectorData::Owned(vec)) => Some(VectorTarget::Raw(Cow::Owned(vec.clone()))),
        RObject::Complex(VectorData::Owned(vec)) => {
            Some(VectorTarget::Complex(Cow::Owned(vec.clone())))
        }
        RObject::Character(VectorData::Owned(vec)) => {
            Some(VectorTarget::Character(Cow::Owned(vec.clone())))
        }
        RObject::Integer(VectorData::Lazy(span)) => Some(VectorTarget::LazyInteger(*span)),
        RObject::Real(VectorData::Lazy(span)) => Some(VectorTarget::LazyReal(*span)),
        RObject::Logical(VectorData::Lazy(span)) => Some(VectorTarget::LazyLogical(*span)),
        RObject::Raw(VectorData::Lazy(span)) => Some(VectorTarget::LazyRaw(*span)),
        RObject::Complex(VectorData::Lazy(span)) => Some(VectorTarget::LazyComplex(*span)),
        RObject::Character(VectorData::Lazy(span)) => Some(VectorTarget::LazyCharacter(*span)),
        _ => None,
    }
}
