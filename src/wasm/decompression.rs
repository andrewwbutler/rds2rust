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
