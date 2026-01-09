//! Streaming decompression support for gzip-compressed RDS files.
//!
//! This module provides `StreamingGzipDecompressor` which uses the browser's
//! `DecompressionStream` API to decompress gzip data on-the-fly with bounded
//! memory usage.

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Promise, Reflect, Uint8Array};
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::Blob;

#[cfg(target_arch = "wasm32")]
use crate::wasm::r#async::{AsyncReadFuture, AsyncSequentialInput};
#[cfg(target_arch = "wasm32")]
use crate::{Error, Result};

/// JavaScript module for compression detection and utilities.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/wasm/decompress.js")]
extern "C" {
    /// Detect compression format from blob header.
    #[wasm_bindgen(js_name = detectCompression)]
    fn detect_compression_js(blob: &Blob) -> Promise;
    /// Detect gzip member offsets for multi-member gzip files.
    #[wasm_bindgen(js_name = detectGzipMemberOffsets)]
    fn detect_gzip_member_offsets_js(blob: &Blob) -> Promise;
    /// Create a streaming reader for multi-member gzip files.
    #[wasm_bindgen(js_name = streamMultiMemberGzip)]
    fn stream_multi_member_gzip_js(blob: &Blob, offsets: JsValue) -> JsValue;
}

/// Streaming decompressor for gzip-compressed blobs.
///
/// This decompressor uses the browser's `DecompressionStream` API to decompress
/// gzip data on-the-fly. It implements `AsyncSequentialInput` to provide
/// forward-only reading with bounded memory usage.
///
/// # Example
///
/// ```ignore
/// let blob = get_compressed_blob();
/// let mut decompressor = StreamingGzipDecompressor::new(blob).await?;
///
/// // Read decompressed data sequentially
/// let chunk1 = decompressor.read_next(1024).await?;
/// let chunk2 = decompressor.read_next(2048).await?;
/// ```
#[cfg(target_arch = "wasm32")]
pub struct StreamingGzipDecompressor {
    /// ReadableStreamDefaultReader for the decompressed stream
    stream_reader: JsValue,
    /// Buffer for decompressed data
    decompressed_buffer: VecDeque<u8>,
    /// Current read position in the decompressed stream
    position: u64,
    /// Whether we've reached the end of the stream
    finished: bool,
    /// Total decompressed size (unknown until fully decompressed)
    total_size: Option<u64>,
}

#[cfg(target_arch = "wasm32")]
impl StreamingGzipDecompressor {
    /// Create a new streaming decompressor for a gzip-compressed Blob.
    ///
    /// This function:
    /// 1. Verifies the blob is gzip-compressed using `detectCompression`
    /// 2. Creates a `DecompressionStream` with "gzip" format
    /// 3. Pipes the blob stream through the decompressor
    /// 4. Returns a reader ready for sequential reading
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The blob is not gzip-compressed
    /// - The `DecompressionStream` API is not available
    /// - Failed to create or pipe the stream
    pub async fn new(blob: Blob) -> Result<Self> {
        // Verify it's actually gzip compressed
        let compression = JsFuture::from(detect_compression_js(&blob))
            .await
            .map_err(|e| Error::CompressionError(format!("Detection failed: {:?}", e)))?;

        let compression_str = compression
            .as_string()
            .ok_or_else(|| Error::CompressionError("Invalid compression type".into()))?;

        if compression_str != "gzip" {
            return Err(Error::CompressionError(format!(
                "Expected gzip compression, got: {}. Only gzip is supported for streaming decompression.",
                compression_str
            )));
        }

        let offsets_value = JsFuture::from(detect_gzip_member_offsets_js(&blob))
            .await
            .map_err(|e| {
                Error::CompressionError(format!("Failed to detect gzip members: {:?}", e))
            })?;
        let offsets = Array::from(&offsets_value);
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("gzip members detected: {}", offsets.length());
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        if offsets.length() > 1 {
            let decompressed_stream = stream_multi_member_gzip_js(&blob, offsets.into());
            let get_reader_fn = Reflect::get(&decompressed_stream, &JsValue::from_str("getReader"))
                .map_err(|e| Error::CompressionError(format!("Failed to get getReader: {:?}", e)))?
                .dyn_into::<js_sys::Function>()
                .map_err(|_| Error::CompressionError("getReader is not a function".into()))?;
            let reader = get_reader_fn
                .call0(&decompressed_stream)
                .map_err(|e| Error::CompressionError(format!("Failed to get reader: {:?}", e)))?;
            return Ok(Self {
                stream_reader: reader,
                decompressed_buffer: VecDeque::new(),
                position: 0,
                finished: false,
                total_size: None,
            });
        }

        // Check if DecompressionStream is available (window or worker global)
        let global = js_sys::global();
        let has_decompression_stream =
            Reflect::has(&global, &JsValue::from_str("DecompressionStream")).unwrap_or(false);

        if !has_decompression_stream {
            return Err(Error::CompressionError(
                "DecompressionStream API not available. Requires Chrome 89+, Firefox 102+, or Safari 16.4+.".into()
            ));
        }

        // Create DecompressionStream using web_sys bindings
        let decompression_stream_class =
            Reflect::get(&global, &JsValue::from_str("DecompressionStream")).map_err(|e| {
                Error::CompressionError(format!("Failed to get DecompressionStream: {:?}", e))
            })?;

        let decompressor = Reflect::construct(
            &decompression_stream_class
                .dyn_into::<js_sys::Function>()
                .map_err(|_| {
                    Error::CompressionError("DecompressionStream is not a constructor".into())
                })?,
            &js_sys::Array::of1(&JsValue::from_str("gzip")),
        )
        .map_err(|e| {
            Error::CompressionError(format!("Failed to create DecompressionStream: {:?}", e))
        })?;

        // Get blob stream
        let blob_stream = blob.stream();

        // Pipe through decompressor
        let writable =
            Reflect::get(&decompressor, &JsValue::from_str("writable")).map_err(|e| {
                Error::CompressionError(format!("Failed to get writable stream: {:?}", e))
            })?;

        let readable =
            Reflect::get(&decompressor, &JsValue::from_str("readable")).map_err(|e| {
                Error::CompressionError(format!("Failed to get readable stream: {:?}", e))
            })?;

        // Pipe blob stream to decompressor's writable side
        let pipe_through_fn = Reflect::get(&blob_stream, &JsValue::from_str("pipeThrough"))
            .map_err(|e| Error::CompressionError(format!("Failed to get pipeThrough: {:?}", e)))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| Error::CompressionError("pipeThrough is not a function".into()))?;

        let transform = js_sys::Object::new();
        Reflect::set(&transform, &JsValue::from_str("writable"), &writable)
            .map_err(|e| Error::CompressionError(format!("Failed to set writable: {:?}", e)))?;
        Reflect::set(&transform, &JsValue::from_str("readable"), &readable)
            .map_err(|e| Error::CompressionError(format!("Failed to set readable: {:?}", e)))?;

        let decompressed_stream = pipe_through_fn
            .call1(&blob_stream, &transform)
            .map_err(|e| Error::CompressionError(format!("Failed to pipe through: {:?}", e)))?;

        // Get reader from the decompressed stream
        let get_reader_fn = Reflect::get(&decompressed_stream, &JsValue::from_str("getReader"))
            .map_err(|e| Error::CompressionError(format!("Failed to get getReader: {:?}", e)))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| Error::CompressionError("getReader is not a function".into()))?;

        let reader = get_reader_fn
            .call0(&decompressed_stream)
            .map_err(|e| Error::CompressionError(format!("Failed to get reader: {:?}", e)))?;

        Ok(Self {
            stream_reader: reader,
            decompressed_buffer: VecDeque::new(),
            position: 0,
            finished: false,
            total_size: None,
        })
    }

    /// Read next chunk from the decompression stream into the buffer.
    ///
    /// Returns `Ok(true)` if data was added to the buffer, `Ok(false)` if the
    /// stream is finished or no data was available.
    async fn fill_buffer(&mut self) -> Result<bool> {
        if self.finished {
            return Ok(false);
        }

        let read_fn = Reflect::get(&self.stream_reader, &JsValue::from_str("read"))
            .map_err(|e| Error::CompressionError(format!("Failed to get read function: {:?}", e)))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| Error::CompressionError("read is not a function".into()))?;

        loop {
            let read_promise = read_fn
                .call0(&self.stream_reader)
                .map_err(|e| Error::CompressionError(format!("Failed to call read(): {:?}", e)))?
                .dyn_into::<Promise>()
                .map_err(|_| Error::CompressionError("read() did not return a Promise".into()))?;

            let result = JsFuture::from(read_promise)
                .await
                .map_err(|e| Error::CompressionError(format!("Stream read failed: {:?}", e)))?;

            let done = Reflect::get(&result, &JsValue::from_str("done"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if done {
                self.finished = true;
                return Ok(false);
            }

            let value = Reflect::get(&result, &JsValue::from_str("value"))
                .map_err(|e| Error::CompressionError(format!("Failed to get value: {:?}", e)))?;

            if value.is_undefined() || value.is_null() {
                continue;
            }

            let array = Uint8Array::new(&value);
            let chunk = array.to_vec();

            if chunk.is_empty() {
                continue;
            }

            self.decompressed_buffer.extend(chunk);
            return Ok(true);
        }
    }

    /// Returns the number of bytes currently buffered.
    pub fn buffered_bytes(&self) -> usize {
        self.decompressed_buffer.len()
    }

    /// Returns whether the stream has finished.
    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncSequentialInput for StreamingGzipDecompressor {
    fn read_next<'a>(&'a mut self, len: usize) -> AsyncReadFuture<'a> {
        Box::pin(async move {
            // Fill buffer until we have enough data or reach end
            while self.decompressed_buffer.len() < len && !self.finished {
                let _ = self.fill_buffer().await?;
            }

            // Extract requested amount (or all remaining if less available)
            let available = self.decompressed_buffer.len().min(len);
            let mut result = Vec::with_capacity(available);
            for _ in 0..available {
                if let Some(byte) = self.decompressed_buffer.pop_front() {
                    result.push(byte);
                }
            }

            self.position += result.len() as u64;
            Ok(result)
        })
    }

    fn total_size(&self) -> Option<u64> {
        self.total_size
    }

    fn position(&self) -> u64 {
        self.position
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for StreamingGzipDecompressor {
    fn drop(&mut self) {
        // Try to cancel/release the reader if possible
        let _ = Reflect::get(&self.stream_reader, &JsValue::from_str("cancel"));
    }
}
