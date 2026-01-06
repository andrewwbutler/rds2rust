//! A Rust library for reading and writing R RDS files without requiring an R runtime.
//!
//! This library provides functionality to serialize and deserialize R objects to/from
//! the RDS binary format.

use std::sync::Arc;

mod constants;
mod error;
mod extraction;
mod manifest;
mod materialization;
mod parser;
#[cfg(not(target_arch = "wasm32"))]
mod source;
mod types;
#[cfg(target_arch = "wasm32")]
mod wasm;
mod writer;

pub use error::{Error, Result};
pub use extraction::{
    convert_object_to_raw_dump, convert_object_to_raw_dump_at_path, expand_dataframe_paths,
    expand_dense_matrix_paths, expand_list_index_paths, expand_object_paths,
    expand_object_paths_for_kind, expand_s4_slot_paths, expand_sparse_matrix_paths,
    extract_object_to_raw_files, extract_object_to_raw_files_with_kind,
    extract_vectors_to_raw_files, write_extraction_manifest, write_extraction_manifest_with_kind,
    Endian, ExtractedVectorInfo, ExtractionResult, ObjectExtractionOutput, ObjectKind, VectorKind,
};
#[cfg(not(target_arch = "wasm32"))]
pub use extraction::{
    extract_complex_vector_streaming, extract_integer_vector_streaming,
    extract_logical_vector_streaming, extract_object_from_path, extract_object_from_path_chunked,
    extract_object_from_path_with_kind, extract_object_from_path_with_kind_chunked,
    extract_object_to_raw_files_with_input_streaming,
    extract_object_to_raw_files_with_kind_and_input_streaming, extract_raw_vector_streaming,
    extract_real_vector_streaming, extract_vectors_from_path, extract_vectors_from_path_chunked,
    extract_vectors_streaming, ExtractionOutput,
};
pub use manifest::{
    read_extraction_manifest, read_vector_file_header, validate_vector_file_header, Manifest,
    ManifestVector, VectorFileHeader,
};
pub use materialization::{
    materialize_complex_data, materialize_complex_vector, materialize_integer_data,
    materialize_integer_vector, materialize_logical_data, materialize_logical_vector,
    materialize_path, materialize_paths_with_budget, materialize_raw_data, materialize_raw_vector,
    materialize_real_data, materialize_real_vector, MaterializationContext,
};
#[cfg(not(target_arch = "wasm32"))]
pub use source::{ChunkedCacheMetrics, ChunkedRdsSource, MmapRdsSource, RdsInput};
pub use types::{
    Attributes, Complex, DataFrameData, FactorData, LazyVector, Logical, PairlistElement, RObject,
    S3ObjectData, S4ObjectData, VectorData,
};
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    decompress_blob_if_needed, estimate_parse_size, extract_vector_chunked, extract_vector_to_js,
    memory_warning, read_rds_async, read_rds_from_blob, recommended_chunk_size_mb,
    recommend_decompression_mode, write_rds_with_callback,
    write_rds_with_callback_and_compression, write_rds_with_progress,
    write_rds_with_progress_and_compression, AsyncBufferedCursor, AsyncCursorConfig,
    AsyncParseConfig, AsyncRdsInput, AsyncReadFuture, BlobChunkedSource, CacheConfig,
    CacheMetrics, WasmDecompressedSource, WasmDecompressionMode, WasmDecompressionThresholds,
};

/// Parsing mode for RDS files.
///
/// Determines whether to fully parse all data or parse only metadata
/// for lightweight file inspection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ParseMode {
    /// Fully parse all data (current behavior, default).
    ///
    /// All vectors and matrices are loaded into memory.
    #[default]
    Full,

    /// Parse structure only, skip large allocations.
    ///
    /// Vectors/matrices are represented as metadata (type, length, dimensions)
    /// without allocating the actual data. This enables:
    /// - Fast file inspection
    /// - Handling files larger than available RAM
    /// - Metadata extraction without memory overhead
    LazyMetadata,

    /// Parse structure with selective loading (advanced).
    ///
    /// Caller specifies which paths to fully load.
    /// Note: Paths use structured segments to avoid parsing ambiguity.
    Selective {
        /// Paths to fully load (all others remain lazy)
        paths: Vec<ObjectPath>,
    },
}

/// Structured path for selective loading.
///
/// Uses interned string segments to avoid parsing ambiguity and
/// reduce memory allocations when matching paths during parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectPath {
    /// Path segments (e.g., ["data", "matrices", "values"])
    pub segments: Vec<Arc<str>>,
}

impl ObjectPath {
    /// Create a new object path from segments.
    pub fn new(segments: Vec<Arc<str>>) -> Self {
        Self { segments }
    }

    /// Create a new object path from string segments.
    pub fn from_strings<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            segments: segments
                .into_iter()
                .map(|s| Arc::from(s.as_ref()))
                .collect(),
        }
    }

    /// Check if this path matches or is a prefix of another path.
    pub fn matches(&self, other: &[Arc<str>]) -> bool {
        if self.segments.len() > other.len() {
            return false;
        }
        self.segments.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

/// Configuration for parsing RDS files.
///
/// Allows customization of memory allocation limits to handle large files
/// or enforce stricter safety constraints.
///
/// # Safety Guardrails
///
/// Note: `mode` does NOT override safety guardrails (`max_vector_length`, `max_allocation_bytes`).
/// These limits are enforced even in `LazyMetadata` mode to protect against corrupt headers.
#[derive(Debug, Clone)]
pub struct ParseConfig {
    /// Maximum number of elements allowed in a vector (default: 50,000,000)
    pub max_vector_length: usize,
    /// Maximum bytes that can be allocated for a single vector (default: 128 MB)
    pub max_allocation_bytes: usize,
    /// Parsing mode (default: Full)
    pub mode: ParseMode,
    /// In lazy mode, vectors smaller than this are always loaded (default: 10 elements)
    ///
    /// Small vectors are typically metadata (dimensions, names, etc.) and should
    /// be loaded even in lazy mode for efficient structure inspection.
    pub lazy_threshold: usize,

    /// In lazy mode, bytecode constants smaller than this are always loaded (default: 1000 elements)
    ///
    /// Bytecode parsing requires extracting tag/symbol names from character vectors.
    /// This threshold is higher than `lazy_threshold` because:
    /// - Tags/symbols are almost always small (class names, function names, etc.)
    /// - Loading a 1000-element character vector is cheap (typically <50 KB)
    /// - If a tag exceeds this threshold, it will be skipped gracefully (placeholder used)
    /// - This prevents failures when parsing S4 objects with embedded command history
    pub bytecode_lazy_threshold: usize,

    /// Optional hard cap on total materialized bytes during conversion/materialization.
    ///
    /// None means no budget enforcement at this layer.
    pub memory_budget_bytes: Option<usize>,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            max_vector_length: 50_000_000,
            max_allocation_bytes: 128 * 1024 * 1024, // 128 MB
            mode: ParseMode::default(),
            lazy_threshold: 10, // Load vectors with <= 10 elements even in lazy mode
            bytecode_lazy_threshold: 1000, // Load bytecode constants with <= 1000 elements
            memory_budget_bytes: None,
        }
    }
}

impl ParseConfig {
    fn clamp_to_usize(bytes: u64) -> usize {
        bytes.min(usize::MAX as u64) as usize
    }
    /// Create a new ParseConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum vector length.
    pub fn with_max_vector_length(mut self, max: usize) -> Self {
        self.max_vector_length = max;
        self
    }

    /// Set the maximum allocation bytes.
    pub fn with_max_allocation_bytes(mut self, max: usize) -> Self {
        self.max_allocation_bytes = max;
        self
    }

    /// Set the parsing mode.
    pub fn with_mode(mut self, mode: ParseMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the lazy threshold (vectors smaller than this are always loaded in lazy mode).
    pub fn with_lazy_threshold(mut self, threshold: usize) -> Self {
        self.lazy_threshold = threshold;
        self
    }

    /// Set the bytecode lazy threshold (bytecode constants smaller than this are always loaded).
    pub fn with_bytecode_lazy_threshold(mut self, threshold: usize) -> Self {
        self.bytecode_lazy_threshold = threshold;
        self
    }

    /// Set a memory budget for materialization/conversion.
    pub fn with_memory_budget_bytes(mut self, budget: Option<usize>) -> Self {
        self.memory_budget_bytes = budget;
        self
    }

    /// Create a config for lazy metadata parsing.
    ///
    /// This mode parses only structure and metadata without allocating
    /// vector/matrix data, enabling:
    /// - Fast file inspection
    /// - Handling files larger than available RAM
    /// - Metadata extraction with minimal memory overhead
    ///
    /// # Note
    ///
    /// Safety guardrails (`max_vector_length`, `max_allocation_bytes`) are
    /// still enforced to protect against corrupt headers.
    pub fn lazy_metadata() -> Self {
        Self {
            mode: ParseMode::LazyMetadata,
            ..Default::default()
        }
    }

    /// Create a config suitable for large scientific datasets (e.g., genomics).
    ///
    /// Sets higher limits:
    /// - max_vector_length: 500,000,000 (500M elements)
    /// - max_allocation_bytes: 2 GB
    pub fn large_data() -> Self {
        Self {
            max_vector_length: 500_000_000,
            max_allocation_bytes: Self::clamp_to_usize(2_u64 * 1024 * 1024 * 1024), // 2 GB
            mode: ParseMode::default(),
            lazy_threshold: 100,
            bytecode_lazy_threshold: 1000,
            memory_budget_bytes: None,
        }
    }

    /// Create a config with unlimited size (use with caution).
    ///
    /// Only use this when you trust the input files and have sufficient memory.
    pub fn unlimited() -> Self {
        Self {
            max_vector_length: usize::MAX,
            max_allocation_bytes: usize::MAX,
            mode: ParseMode::default(),
            lazy_threshold: 100,
            bytecode_lazy_threshold: 1000,
            memory_budget_bytes: None,
        }
    }

    /// Create a config for trusted, large files.
    ///
    /// Keeps guardrails but raises limits and stays in lazy metadata mode.
    pub fn for_trusted_large_file() -> Self {
        Self {
            max_vector_length: 1_000_000_000,
            max_allocation_bytes: Self::clamp_to_usize(4_u64 * 1024 * 1024 * 1024), // 4 GB
            mode: ParseMode::LazyMetadata,
            lazy_threshold: 100,
            bytecode_lazy_threshold: 10_000,
            memory_budget_bytes: None,
        }
    }

    /// Create a config for inspection-only parsing.
    ///
    /// Forces all vectors lazy to avoid allocations.
    pub fn for_inspection_only() -> Self {
        Self {
            mode: ParseMode::LazyMetadata,
            lazy_threshold: 0,
            bytecode_lazy_threshold: 0,
            memory_budget_bytes: None,
            ..Default::default()
        }
    }

    /// Create a config suitable for constrained conversions.
    ///
    /// Use with explicit materialization budgeting in higher-level code.
    pub fn for_constrained_conversion(budget_mb: usize) -> Self {
        Self {
            max_vector_length: 1_000_000_000,
            max_allocation_bytes: Self::clamp_to_usize(4_u64 * 1024 * 1024 * 1024), // 4 GB
            mode: ParseMode::LazyMetadata,
            lazy_threshold: 100,
            bytecode_lazy_threshold: 1000,
            memory_budget_bytes: Some(budget_mb * 1024 * 1024),
        }
    }
}

/// Read an RDS file from a byte slice with default safety limits.
///
/// For large files, consider using [`read_rds_with_config`] with [`ParseConfig::large_data()`].
/// For lazy parsing (metadata only), use [`read_rds_lazy`].
pub fn read_rds(data: &[u8]) -> Result<RObject> {
    read_rds_with_config(data, ParseConfig::default())
}

/// Read an RDS file in lazy mode (metadata only, no vector data allocation).
///
/// This function parses only the structure and metadata without allocating
/// vector/matrix data, enabling:
/// - Fast file inspection (structure, column names, dimensions)
/// - Handling files larger than available RAM
/// - Metadata extraction with minimal memory overhead
///
/// # Examples
///
/// ```
/// use rds2rust::{read_rds_lazy, RObject};
///
/// # fn example(data: &[u8]) -> rds2rust::Result<()> {
/// // Parse metadata only
/// let obj = read_rds_lazy(data)?;
///
/// // Check if object is fully loaded
/// assert!(!obj.is_fully_loaded());
///
/// // Get lazy vector spans for inspection
/// let spans = obj.lazy_spans();
/// for (path, lazy_vec) in spans {
///     println!("{}: {} elements at offset {}", path, lazy_vec.length, lazy_vec.offset);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Note
///
/// Safety guardrails (`max_vector_length`, `max_allocation_bytes`) are
/// still enforced to protect against corrupt headers.
///
/// Attempting to write a lazy object will fail - materialize it first
/// or re-parse in full mode.
pub fn read_rds_lazy(data: &[u8]) -> Result<RObject> {
    read_rds_with_config(data, ParseConfig::lazy_metadata())
}

/// Read an RDS file from a byte slice with custom configuration.
///
/// # Examples
///
/// ```
/// use rds2rust::{read_rds_with_config, ParseConfig};
///
/// // For large scientific datasets
/// let config = ParseConfig::large_data();
/// // let obj = read_rds_with_config(&data, config)?;
///
/// // For custom limits
/// let config = ParseConfig::new()
///     .with_max_allocation_bytes(512 * 1024 * 1024); // 512 MB
/// // let obj = read_rds_with_config(&data, config)?;
/// ```
pub fn read_rds_with_config(data: &[u8], config: ParseConfig) -> Result<RObject> {
    let obj = parser::parse_rds_with_config(data, config)?;
    Ok(unwrap_top_level_shared(obj))
}

/// Read an RDS file from an input source with custom configuration.
///
/// This uses `RdsInput` to support chunked backing stores.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_with_input(input: &dyn RdsInput, config: ParseConfig) -> Result<RObject> {
    let obj = parser::parse_rds_with_input(input, config)?;
    Ok(unwrap_top_level_shared(obj))
}

/// Read an RDS file from an input source in lazy metadata mode.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_lazy_with_input(input: &dyn RdsInput) -> Result<RObject> {
    read_rds_with_input(input, ParseConfig::lazy_metadata())
}

/// Read an RDS file from a file path with custom configuration.
///
/// This uses a temp file + mmap for gzip-compressed inputs to avoid
/// holding the full decompressed payload in memory.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_from_path_with_config<P: AsRef<std::path::Path>>(
    path: P,
    config: ParseConfig,
) -> Result<RObject> {
    let source = MmapRdsSource::from_path(path.as_ref())?;
    read_rds_with_config(source.as_slice(), config)
}

/// Read an RDS file from a file path with custom configuration, backed by chunked reads.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_from_path_chunked_with_config<P: AsRef<std::path::Path>>(
    path: P,
    config: ParseConfig,
) -> Result<RObject> {
    let source = ChunkedRdsSource::from_path(path.as_ref())?;
    read_rds_with_input(&source, config)
}

/// Read an RDS file from a file path with default configuration.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_from_path<P: AsRef<std::path::Path>>(path: P) -> Result<RObject> {
    read_rds_from_path_with_config(path, ParseConfig::default())
}

/// Read an RDS file from a file path with default configuration, backed by chunked reads.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_from_path_chunked<P: AsRef<std::path::Path>>(path: P) -> Result<RObject> {
    read_rds_from_path_chunked_with_config(path, ParseConfig::default())
}

/// Read an RDS file from a file path in lazy metadata mode, backed by chunked reads.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_lazy_from_path_chunked<P: AsRef<std::path::Path>>(path: P) -> Result<RObject> {
    let source = ChunkedRdsSource::from_path(path.as_ref())?;
    read_rds_lazy_with_input(&source)
}

/// Read an RDS file from a file path in lazy metadata mode.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_rds_lazy_from_path<P: AsRef<std::path::Path>>(path: P) -> Result<RObject> {
    read_rds_from_path_with_config(path, ParseConfig::lazy_metadata())
}

/// Unwrap Shared wrappers added by the parser for reference tracking.
///
/// The parser wraps all tracked objects in RObject::Shared to maintain Arc consistency
/// for REFSXP references. At the API boundary, we recursively unwrap Shared objects
/// that only have one strong reference (not actually shared via REFSXP).
///
/// Objects with multiple references (actual shared references from REFSXP) are kept as Shared.
fn unwrap_top_level_shared(obj: RObject) -> RObject {
    unwrap_shared_recursive(obj)
}

fn unwrap_shared_recursive(obj: RObject) -> RObject {
    match obj {
        RObject::Shared(arc) => {
            let strong_count = Arc::strong_count(&arc);
            if strong_count == 1 {
                // Only one reference - this is just for tracking, unwrap it
                match Arc::try_unwrap(arc) {
                    Ok(rwlock) => {
                        let inner = rwlock.into_inner().unwrap();
                        // Recursively unwrap the inner object
                        unwrap_shared_recursive(inner)
                    }
                    Err(arc) => {
                        // Shouldn't happen if strong_count was 1, but handle gracefully
                        RObject::Shared(arc)
                    }
                }
            } else {
                // Multiple references - this is a real shared reference, keep it
                RObject::Shared(arc)
            }
        }
        // Recursively unwrap container types
        RObject::List(elements) => {
            RObject::List(elements.into_iter().map(unwrap_shared_recursive).collect())
        }
        RObject::Pairlist(elements) => RObject::Pairlist(
            elements
                .into_iter()
                .map(|elem| PairlistElement {
                    tag: elem.tag,
                    value: unwrap_shared_recursive(elem.value),
                    tag_object: elem
                        .tag_object
                        .map(|obj| Box::new(unwrap_shared_recursive(*obj))),
                })
                .collect(),
        ),
        RObject::Language { function, args } => RObject::Language {
            function: Box::new(unwrap_shared_recursive(*function)),
            args: args
                .into_iter()
                .map(|elem| PairlistElement {
                    tag: elem.tag,
                    value: unwrap_shared_recursive(elem.value),
                    tag_object: elem
                        .tag_object
                        .map(|obj| Box::new(unwrap_shared_recursive(*obj))),
                })
                .collect(),
        },
        RObject::Expression(elements) => {
            RObject::Expression(elements.into_iter().map(unwrap_shared_recursive).collect())
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => RObject::Closure {
            formals: Box::new(unwrap_shared_recursive(*formals)),
            body: Box::new(unwrap_shared_recursive(*body)),
            environment: Box::new(unwrap_shared_recursive(*environment)),
        },
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => RObject::Environment {
            enclosing: Box::new(unwrap_shared_recursive(*enclosing)),
            frame: Box::new(unwrap_shared_recursive(*frame)),
            hashtab: Box::new(unwrap_shared_recursive(*hashtab)),
        },
        RObject::Promise {
            value,
            expression,
            environment,
        } => RObject::Promise {
            value: Box::new(unwrap_shared_recursive(*value)),
            expression: Box::new(unwrap_shared_recursive(*expression)),
            environment: Box::new(unwrap_shared_recursive(*environment)),
        },
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => RObject::Bytecode {
            code: Box::new(unwrap_shared_recursive(*code)),
            constants: Box::new(unwrap_shared_recursive(*constants)),
            expr: Box::new(unwrap_shared_recursive(*expr)),
        },
        RObject::DataFrame(mut df_data) => {
            // Unwrap RObjects in the columns
            for (_, value) in df_data.columns.iter_mut() {
                *value = unwrap_shared_recursive(std::mem::replace(value, RObject::Null));
            }
            RObject::DataFrame(df_data)
        }
        RObject::S3Object(mut s3_data) => {
            s3_data.base = Box::new(unwrap_shared_recursive(*s3_data.base));
            s3_data.attributes = unwrap_attributes(s3_data.attributes);
            RObject::S3Object(s3_data)
        }
        RObject::S4Object(mut s4_data) => {
            // Unwrap RObjects in the slots
            for (_, value) in s4_data.slots.iter_mut() {
                *value = unwrap_shared_recursive(std::mem::replace(value, RObject::Null));
            }
            RObject::S4Object(s4_data)
        }
        RObject::WithAttributes { object, attributes } => RObject::WithAttributes {
            object: Box::new(unwrap_shared_recursive(*object)),
            attributes: unwrap_attributes(attributes),
        },
        // Other types don't contain nested RObjects or don't need unwrapping
        other => other,
    }
}

/// Helper to recursively unwrap Shared objects in attributes
fn unwrap_attributes(mut attrs: Attributes) -> Attributes {
    for (_, value) in attrs.attrs.iter_mut() {
        *value = Box::new(unwrap_shared_recursive(*std::mem::replace(
            value,
            Box::new(RObject::Null),
        )));
    }
    attrs
}

/// Write an RObject to RDS format.
/// Returns gzip-compressed RDS data.
pub fn write_rds(obj: &RObject) -> Result<Vec<u8>> {
    writer::write_rds(obj)
}

/// Streaming writer APIs.
///
/// Use these for large outputs to avoid buffering the entire file in memory.
///
/// # Examples
///
/// ```rust
/// use rds2rust::{write_rds_streaming, write_rds_atomic, RObject, VectorData};
/// use std::fs::File;
/// use std::io::BufWriter;
///
/// let obj = RObject::Integer(VectorData::Owned(vec![1, 2, 3]));
///
/// // Stream to a file (gzip compressed)
/// let file = File::create("output.rds")?;
/// write_rds_streaming(&obj, BufWriter::new(file))?;
///
/// // Atomic write helper (native only)
/// write_rds_atomic(&obj, "output.rds")?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Write an RObject to a streaming sink (gzip compressed).
pub fn write_rds_streaming<W: std::io::Write>(obj: &RObject, sink: W) -> Result<()> {
    writer::write_rds_streaming(obj, sink)
}

/// Write an RObject to a streaming sink with an explicit compression level.
pub fn write_rds_streaming_with_compression<W: std::io::Write>(
    obj: &RObject,
    sink: W,
    compression: flate2::Compression,
) -> Result<()> {
    writer::write_rds_streaming_with_compression(obj, sink, compression)
}

/// Write an RObject to disk atomically (native only).
#[cfg(not(target_arch = "wasm32"))]
pub fn write_rds_atomic<P: AsRef<std::path::Path>>(obj: &RObject, path: P) -> Result<()> {
    writer::write_rds_atomic(obj, path)
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen::JsCast;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_placeholder() {
        let value = 1;
        assert_eq!(value, 1);
    }

    #[cfg(target_arch = "wasm32")]
    fn has_decompression_stream() -> bool {
        let global = js_sys::global();
        let value = js_sys::Reflect::get(&global, &wasm_bindgen::JsValue::from_str("DecompressionStream"))
            .unwrap_or(wasm_bindgen::JsValue::UNDEFINED);
        !value.is_undefined()
    }

    #[cfg(target_arch = "wasm32")]
    fn blob_from_bytes(bytes: &[u8]) -> web_sys::Blob {
        let array = js_sys::Array::new();
        let view = js_sys::Uint8Array::from(bytes);
        array.push(&view.buffer());
        web_sys::Blob::new_with_u8_array_sequence(&array)
            .expect("blob from bytes")
    }

    #[cfg(target_arch = "wasm32")]
    fn ungzip(bytes: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).expect("ungzip");
        out
    }

    #[cfg(target_arch = "wasm32")]
    fn js_options(pairs: &[(&str, wasm_bindgen::JsValue)]) -> wasm_bindgen::JsValue {
        let obj = js_sys::Object::new();
        for (key, value) in pairs {
            js_sys::Reflect::set(&obj, &wasm_bindgen::JsValue::from_str(key), value)
                .expect("set option");
        }
        wasm_bindgen::JsValue::from(obj)
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_gzip_blob() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![1, 2, 3]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let blob = blob_from_bytes(&gzip_bytes);
        let parsed = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            None,
        )
        .await
        .expect("parse gzip")
        .into_concrete();
        match parsed {
            crate::RObject::Integer(vec) => {
                assert_eq!(vec.as_vec(), &vec![1, 2, 3]);
            }
            other => panic!("unexpected object: {:?}", other),
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_uncompressed_blob() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![4, 5]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let raw_bytes = ungzip(&gzip_bytes);
        let blob = blob_from_bytes(&raw_bytes);
        let parsed = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            None,
        )
        .await
        .expect("parse uncompressed")
        .into_concrete();
        match parsed {
            crate::RObject::Integer(vec) => {
                assert_eq!(vec.as_vec(), &vec![4, 5]);
            }
            other => panic!("unexpected object: {:?}", other),
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_unsupported_format() {
        if !has_decompression_stream() {
            return;
        }
        let bytes = vec![0x42, 0x5a, 0x00, 0x00];
        let blob = blob_from_bytes(&bytes);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            None,
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("bzip2"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_extension_mismatch() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![7]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let raw_bytes = ungzip(&gzip_bytes);
        let blob = blob_from_bytes(&raw_bytes);
        let options = js_options(&[(
            "filename",
            wasm_bindgen::JsValue::from_str("sample.rds.gz"),
        )]);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            Some(options),
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains(".gz"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_ratio_exceeded() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![1, 2, 3, 4]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let blob = blob_from_bytes(&gzip_bytes);
        let options = js_options(&[("maxRatio", wasm_bindgen::JsValue::from_f64(0.1))]);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            Some(options),
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("Compression ratio"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_budget_precheck() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![9, 10]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let blob = blob_from_bytes(&gzip_bytes);
        let options = js_options(&[("budgetBytes", wasm_bindgen::JsValue::from_f64(1.0))]);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            Some(options),
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("budget"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_corrupt_gzip() {
        if !has_decompression_stream() {
            return;
        }
        let bytes = vec![0x1f, 0x8b, 0x08, 0x00, 0x00];
        let blob = blob_from_bytes(&bytes);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            None,
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("decompression error"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test(async)]
    async fn wasm_read_rds_timeout() {
        if !has_decompression_stream() {
            return;
        }
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![11, 12]));
        let gzip_bytes = crate::write_rds(&obj).expect("write rds");
        let blob = blob_from_bytes(&gzip_bytes);
        let options = js_options(&[
            ("timeoutMs", wasm_bindgen::JsValue::from_f64(1.0)),
            ("testDelayMs", wasm_bindgen::JsValue::from_f64(50.0)),
        ]);
        let result = crate::read_rds_from_blob(
            blob,
            crate::ParseConfig::default(),
            crate::AsyncParseConfig::default(),
            crate::CacheConfig::default(),
            Some(options),
        )
        .await;
        let err = result.expect_err("expected error");
        assert!(err.to_string().to_lowercase().contains("timeout"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn wasm_write_rds_with_callback_roundtrip() {
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![5, 6, 7]));
        let chunks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let chunks_clone = chunks.clone();

        let callback = wasm_bindgen::closure::Closure::wrap(Box::new(
            move |chunk: js_sys::Uint8Array| {
                let mut buf = vec![0u8; chunk.length() as usize];
                chunk.copy_to(&mut buf);
                chunks_clone.borrow_mut().push(buf);
            },
        ) as Box<dyn FnMut(js_sys::Uint8Array)>);

        let callback_fn: js_sys::Function =
            callback.as_ref().unchecked_ref::<js_sys::Function>().clone();
        crate::write_rds_with_callback(&obj, callback_fn, Some(1))
            .expect("write_rds_with_callback");
        callback.forget();

        let bytes: Vec<u8> = chunks.borrow().iter().flatten().copied().collect();
        let parsed = crate::read_rds(&bytes).expect("read_rds");
        match parsed.into_concrete() {
            crate::RObject::Integer(vec) => assert_eq!(vec.as_vec(), &vec![5, 6, 7]),
            other => panic!("unexpected object: {:?}", other),
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn wasm_write_rds_with_progress_reports_bytes() {
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![8, 9, 10, 11]));
        let chunks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let chunks_clone = chunks.clone();
        let progress = std::rc::Rc::new(std::cell::Cell::new(0f64));
        let progress_clone = progress.clone();

        let on_chunk = wasm_bindgen::closure::Closure::wrap(Box::new(
            move |chunk: js_sys::Uint8Array| {
                let mut buf = vec![0u8; chunk.length() as usize];
                chunk.copy_to(&mut buf);
                chunks_clone.borrow_mut().push(buf);
            },
        ) as Box<dyn FnMut(js_sys::Uint8Array)>);

        let on_progress = wasm_bindgen::closure::Closure::wrap(Box::new(move |bytes: f64| {
            progress_clone.set(bytes);
        }) as Box<dyn FnMut(f64)>);

        let on_chunk_fn: js_sys::Function =
            on_chunk.as_ref().unchecked_ref::<js_sys::Function>().clone();
        let on_progress_fn: js_sys::Function =
            on_progress.as_ref().unchecked_ref::<js_sys::Function>().clone();
        crate::write_rds_with_progress(&obj, on_chunk_fn, on_progress_fn, Some(1))
            .expect("write_rds_with_progress");
        on_chunk.forget();
        on_progress.forget();

        assert!(progress.get() > 0.0);
        let bytes: Vec<u8> = chunks.borrow().iter().flatten().copied().collect();
        let parsed = crate::read_rds(&bytes).expect("read_rds");
        match parsed.into_concrete() {
            crate::RObject::Integer(vec) => assert_eq!(vec.as_vec(), &vec![8, 9, 10, 11]),
            other => panic!("unexpected object: {:?}", other),
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn wasm_write_rds_callback_rejects_zero_chunk_size() {
        let obj = crate::RObject::Integer(crate::VectorData::Owned(vec![1]));
        let callback = wasm_bindgen::closure::Closure::wrap(Box::new(
            |_chunk: js_sys::Uint8Array| {},
        ) as Box<dyn FnMut(js_sys::Uint8Array)>);

        let callback_fn: js_sys::Function =
            callback.as_ref().unchecked_ref::<js_sys::Function>().clone();
        let result = crate::write_rds_with_callback(&obj, callback_fn, Some(0));
        callback.forget();
        let err = result.expect_err("expected error");
        assert!(err.to_string().contains("chunk_size_mb"));
    }
}
