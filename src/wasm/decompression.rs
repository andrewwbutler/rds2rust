#[cfg(target_arch = "wasm32")]
use js_sys::Promise;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::Blob;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub enum WasmDecompressedSource {
    InMemory(Vec<u8>),
    Blob(Blob),
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmDecompressionMode {
    InMemory,
    Blob,
    Streaming,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct WasmDecompressionThresholds {
    pub in_memory_bytes: u64,
    pub blob_bytes: u64,
}

#[cfg(target_arch = "wasm32")]
impl Default for WasmDecompressionThresholds {
    fn default() -> Self {
        Self {
            in_memory_bytes: 500 * 1024 * 1024,
            blob_bytes: 10 * 1024 * 1024 * 1024,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn recommend_decompression_mode(
    file_size: u64,
    thresholds: WasmDecompressionThresholds,
) -> WasmDecompressionMode {
    if file_size < thresholds.in_memory_bytes {
        WasmDecompressionMode::InMemory
    } else if file_size <= thresholds.blob_bytes {
        WasmDecompressionMode::Blob
    } else {
        WasmDecompressionMode::Streaming
    }
}

#[cfg(target_arch = "wasm32")]
pub fn memory_warning(file_size: u64, device_memory_gb: Option<f64>) -> Option<String> {
    let memory_gb = device_memory_gb.unwrap_or(4.0);
    let file_gb = file_size as f64 / (1024.0 * 1024.0 * 1024.0);
    if file_gb > memory_gb * 2.0 {
        Some(format!(
            "File size {:.1}GB is large for {:.1}GB RAM. Consider streaming mode.",
            file_gb, memory_gb
        ))
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/wasm/decompress.js")]
extern "C" {
    #[wasm_bindgen(js_name = decompressBlobIfNeeded)]
    fn decompress_blob_if_needed_js(blob: Blob, options: JsValue) -> Promise;
    #[wasm_bindgen(js_name = decompressBlobForRandomAccess)]
    fn decompress_blob_for_random_access_js(blob: Blob, options: JsValue) -> Promise;
}

#[cfg(target_arch = "wasm32")]
pub async fn decompress_blob_if_needed(
    blob: Blob,
    options: Option<JsValue>,
) -> crate::Result<Blob> {
    let opts = options.unwrap_or(JsValue::UNDEFINED);
    let promise = decompress_blob_if_needed_js(blob, opts);
    let value = JsFuture::from(promise)
        .await
        .map_err(|err| crate::Error::Unsupported(format!("decompression error: {:?}", err)))?;
    value
        .dyn_into::<Blob>()
        .map_err(|err| crate::Error::Unsupported(format!("decompression error: {:?}", err)))
}

#[cfg(target_arch = "wasm32")]
pub async fn decompress_blob_for_random_access(
    blob: Blob,
    options: Option<JsValue>,
) -> crate::Result<Blob> {
    #[cfg(target_arch = "wasm32")]
    {
        let size = blob.size() as u64;
        let msg = format!(
            "[rds2rust] decompress_blob_for_random_access blob_size={} bytes",
            size
        );
        web_sys::console::log_1(&JsValue::from_str(&msg));
        if let Some(opts) = options.as_ref() {
            if !opts.is_undefined() {
                web_sys::console::log_1(&JsValue::from_str(
                    "[rds2rust] decompress_blob_for_random_access options provided",
                ));
            }
        }
    }
    let opts = options.unwrap_or(JsValue::UNDEFINED);
    let promise = decompress_blob_for_random_access_js(blob, opts);
    let value = JsFuture::from(promise)
        .await
        .map_err(|err| crate::Error::Unsupported(format!("decompression error: {:?}", err)))?;
    value
        .dyn_into::<Blob>()
        .map_err(|err| crate::Error::Unsupported(format!("decompression error: {:?}", err)))
}

#[cfg(target_arch = "wasm32")]
pub async fn read_rds_from_blob(
    blob: Blob,
    parse_config: crate::ParseConfig,
    async_config: crate::AsyncParseConfig,
    cache_config: crate::CacheConfig,
    options: Option<JsValue>,
) -> crate::Result<crate::ParseResult> {
    let decompressed = decompress_blob_if_needed(blob, options).await?;

    // DEBUG: Log blob size as seen by Rust
    #[cfg(target_arch = "wasm32")]
    {
        let blob_size = decompressed.size() as u64;
        let msg = format!(
            "[RUST DEBUG] Decompressed blob size from Rust: {} bytes ({:.2} GB)",
            blob_size,
            blob_size as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        web_sys::console::log_1(&JsValue::from_str(&msg));
    }

    let source = crate::BlobChunkedSource::new(decompressed, cache_config);
    crate::read_rds_async(&source, parse_config, async_config).await
}
