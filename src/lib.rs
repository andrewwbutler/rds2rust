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
    estimate_parse_size, extract_vector_chunked, extract_vector_to_js, memory_warning,
    read_rds_async, recommend_decompression_mode, AsyncBufferedCursor, AsyncCursorConfig,
    AsyncParseConfig, AsyncRdsInput, AsyncReadFuture, BlobChunkedSource, CacheConfig, CacheMetrics,
    WasmDecompressedSource, WasmDecompressionMode, WasmDecompressionThresholds,
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

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_placeholder() {
        // Placeholder test - will be replaced with actual tests
        let value = 1;
        assert_eq!(value, 1);
    }
}
