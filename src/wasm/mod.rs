#[cfg(target_arch = "wasm32")]
mod r#async;
#[cfg(target_arch = "wasm32")]
mod async_parse;
#[cfg(target_arch = "wasm32")]
mod blob_source;
#[cfg(target_arch = "wasm32")]
mod decompression;
#[cfg(target_arch = "wasm32")]
mod extract;
#[cfg(target_arch = "wasm32")]
mod streaming_api;
#[cfg(target_arch = "wasm32")]
mod streaming_decompress;
#[cfg(target_arch = "wasm32")]
mod write;

#[cfg(target_arch = "wasm32")]
pub use async_parse::{read_rds_async, AsyncParseConfig};
#[cfg(target_arch = "wasm32")]
pub use blob_source::{BlobChunkedSource, CacheConfig, CacheMetrics};
#[cfg(target_arch = "wasm32")]
pub use decompression::{
    decompress_blob_for_random_access, decompress_blob_if_needed, memory_warning,
    read_rds_from_blob, recommend_decompression_mode, WasmDecompressedSource,
    WasmDecompressionMode, WasmDecompressionThresholds,
};
#[cfg(target_arch = "wasm32")]
pub use extract::{
    extract_vector_chunked, extract_vector_to_js, read_lazy_character_vector,
    read_lazy_vector_range,
};
#[cfg(target_arch = "wasm32")]
pub use r#async::{
    estimate_parse_size, AsyncBufferedCursor, AsyncCursor, AsyncCursorConfig, AsyncRdsInput,
    AsyncReadFuture, AsyncSequentialInput, SequentialAdapter, SequentialCursor,
};
#[cfg(target_arch = "wasm32")]
pub use streaming_api::{
    check_streaming_decompression_support, detect_blob_compression, traverse_rds_blob_streaming,
    traverse_rds_blob_streaming_with_progress,
};
#[cfg(target_arch = "wasm32")]
pub use streaming_decompress::StreamingGzipDecompressor;
#[cfg(target_arch = "wasm32")]
pub use write::{
    recommended_chunk_size_mb, write_rds_with_callback, write_rds_with_callback_and_compression,
    write_rds_with_progress, write_rds_with_progress_and_compression,
};
