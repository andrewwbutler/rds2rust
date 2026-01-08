//! High-level streaming API with automatic compression detection.
//!
//! This module provides convenience functions that automatically detect compression
//! format and choose the optimal parsing strategy (sequential for compressed,
//! random-access for uncompressed).

#[cfg(target_arch = "wasm32")]
use js_sys::Promise;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::Blob;

#[cfg(target_arch = "wasm32")]
use crate::streaming::{RdsVisitor, StreamingError};
#[cfg(target_arch = "wasm32")]
use crate::wasm::streaming_decompress::StreamingGzipDecompressor;
#[cfg(target_arch = "wasm32")]
use crate::wasm::{AsyncCursorConfig, BlobChunkedSource, CacheConfig};
#[cfg(target_arch = "wasm32")]
use crate::{ParseConfig, StreamingProgress};

/// JavaScript module for compression detection.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/wasm/decompress.js")]
extern "C" {
    /// Detect compression format from blob header.
    #[wasm_bindgen(js_name = detectCompression)]
    fn detect_compression_js(blob: &Blob) -> Promise;
}

/// Detect compression format of a blob.
///
/// Returns one of: "gzip", "bzip2", "xz", "rds" (uncompressed), or "unknown".
#[cfg(target_arch = "wasm32")]
pub async fn detect_blob_compression(blob: &Blob) -> Result<String, JsValue> {
    let compression = JsFuture::from(detect_compression_js(blob)).await?;

    compression
        .as_string()
        .ok_or_else(|| JsValue::from_str("Invalid compression type"))
}

/// Traverse RDS blob with automatic compression detection and optimal streaming mode.
///
/// This function:
/// 1. Detects the compression format from the blob header
/// 2. For gzip: Uses `StreamingGzipDecompressor` with sequential streaming (memory-efficient)
/// 3. For uncompressed: Uses `BlobChunkedSource` with random-access streaming (cached)
/// 4. For xz/bzip2: Returns an error with instructions
///
/// # Example
///
/// ```ignore
/// use rds2rust::{traverse_rds_blob_streaming, ParseConfig, RdsVisitor};
///
/// let blob = get_rds_blob(); // May be compressed or not
/// let mut visitor = MyVisitor::new();
///
/// traverse_rds_blob_streaming(
///     blob,
///     ParseConfig::default(),
///     &mut visitor
/// ).await?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Blob format is not recognized
/// - Compression format is xz or bzip2 (not supported in streaming mode)
/// - DecompressionStream API is not available (for gzip)
/// - Parse error occurs
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_blob_streaming<V>(
    blob: Blob,
    parse_config: ParseConfig,
    visitor: &mut V,
) -> Result<(), StreamingError<V::Error>>
where
    V: RdsVisitor,
{
    let compression = detect_blob_compression(&blob).await.map_err(|e| {
        StreamingError::Parse(crate::Error::CompressionError(format!(
            "Failed to detect compression: {:?}",
            e
        )))
    })?;

    match compression.as_str() {
        "gzip" => {
            // Use sequential streaming decompressor
            let mut decompressor = StreamingGzipDecompressor::new(blob)
                .await
                .map_err(StreamingError::Parse)?;

            crate::traverse_rds_streaming_sequential_async(&mut decompressor, parse_config, visitor)
                .await
        }
        "rds" => {
            // Uncompressed, use existing random-access streaming
            let cache_config = CacheConfig::default();
            let source = BlobChunkedSource::new(blob, cache_config);
            let cursor_config = AsyncCursorConfig::default();

            crate::traverse_rds_streaming_async(&source, parse_config, cursor_config, visitor).await
        }
        "bzip2" | "xz" => Err(StreamingError::Parse(crate::Error::CompressionError(
            format!(
                "{} compression not supported in streaming mode. \
                 Use decompressBlobIfNeeded() from decompress.js to decompress first, \
                 or use the non-streaming parse API.",
                compression
            ),
        ))),
        "unknown" => Err(StreamingError::Parse(crate::Error::InvalidFormat(
            "Unrecognized file format. Expected gzip-compressed or uncompressed RDS file.".into(),
        ))),
        _ => Err(StreamingError::Parse(crate::Error::InvalidFormat(format!(
            "Unexpected compression format: {}",
            compression
        )))),
    }
}

/// Traverse RDS blob with automatic compression detection and progress reporting.
///
/// Similar to `traverse_rds_blob_streaming` but provides progress callbacks.
///
/// # Example
///
/// ```ignore
/// use rds2rust::{traverse_rds_blob_streaming_with_progress, ParseConfig};
///
/// let blob = get_rds_blob();
/// let mut visitor = MyVisitor::new();
///
/// traverse_rds_blob_streaming_with_progress(
///     blob,
///     ParseConfig::default(),
///     &mut visitor,
///     &mut |progress| {
///         console_log!("Progress: {}/{} bytes", progress.bytes_read, progress.total_bytes.unwrap_or(0));
///     }
/// ).await?;
/// ```
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_blob_streaming_with_progress<V>(
    blob: Blob,
    parse_config: ParseConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> Result<(), StreamingError<V::Error>>
where
    V: RdsVisitor,
{
    let compression = detect_blob_compression(&blob).await.map_err(|e| {
        StreamingError::Parse(crate::Error::CompressionError(format!(
            "Failed to detect compression: {:?}",
            e
        )))
    })?;

    match compression.as_str() {
        "gzip" => {
            // Use sequential streaming decompressor
            let mut decompressor = StreamingGzipDecompressor::new(blob)
                .await
                .map_err(StreamingError::Parse)?;

            crate::traverse_rds_streaming_sequential_async_with_progress(
                &mut decompressor,
                parse_config,
                visitor,
                progress,
            )
            .await
        }
        "rds" => {
            // Uncompressed, use existing random-access streaming
            let cache_config = CacheConfig::default();
            let source = BlobChunkedSource::new(blob, cache_config);
            let cursor_config = AsyncCursorConfig::default();

            crate::traverse_rds_streaming_async_with_progress(
                &source,
                parse_config,
                cursor_config,
                visitor,
                progress,
            )
            .await
        }
        "bzip2" | "xz" => Err(StreamingError::Parse(crate::Error::CompressionError(
            format!(
                "{} compression not supported in streaming mode. \
                 Use decompressBlobIfNeeded() from decompress.js to decompress first.",
                compression
            ),
        ))),
        "unknown" => Err(StreamingError::Parse(crate::Error::InvalidFormat(
            "Unrecognized file format. Expected gzip-compressed or uncompressed RDS file.".into(),
        ))),
        _ => Err(StreamingError::Parse(crate::Error::InvalidFormat(format!(
            "Unexpected compression format: {}",
            compression
        )))),
    }
}

/// Check if streaming decompression is available in the current browser.
///
/// Returns `Ok(())` if the browser supports `DecompressionStream` API,
/// otherwise returns an error with information about browser requirements.
#[cfg(target_arch = "wasm32")]
pub fn check_streaming_decompression_support() -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "No window object available".to_string())?;

    let has_decompression_stream =
        js_sys::Reflect::has(&window, &JsValue::from_str("DecompressionStream")).unwrap_or(false);

    if has_decompression_stream {
        Ok(())
    } else {
        Err("DecompressionStream API not available. \
             Streaming decompression requires Chrome 89+, Firefox 102+, or Safari 16.4+."
            .to_string())
    }
}
