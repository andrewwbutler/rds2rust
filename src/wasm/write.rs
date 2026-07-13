#[cfg(target_arch = "wasm32")]
use crate::error::{Error, Result};
#[cfg(target_arch = "wasm32")]
use crate::{write_rds_streaming, write_rds_streaming_with_compression, RObject};
#[cfg(target_arch = "wasm32")]
use flate2::Compression;
#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Uint8Array};
#[cfg(target_arch = "wasm32")]
use std::io::Write;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
const DEFAULT_CHUNK_SIZE_MB: usize = 4;
#[cfg(target_arch = "wasm32")]
const PROGRESS_INTERVAL_BYTES: usize = 10 * 1024 * 1024;

#[cfg(target_arch = "wasm32")]
pub struct CallbackWriter {
    callback: Function,
    buffer: Vec<u8>,
    chunk_size: usize,
    bytes_written: usize,
}

#[cfg(target_arch = "wasm32")]
impl CallbackWriter {
    pub fn new(callback: Function, chunk_size: usize) -> Result<Self> {
        if chunk_size == 0 {
            return Err(Error::InvalidFormat(
                "chunk_size must be greater than 0".to_string(),
            ));
        }
        Ok(Self {
            callback,
            buffer: Vec::with_capacity(chunk_size),
            chunk_size,
            bytes_written: 0,
        })
    }

    fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let array = Uint8Array::from(&self.buffer[..]);
        self.callback
            .call1(&JsValue::NULL, &array)
            .map_err(map_callback_error)?;

        self.bytes_written += self.buffer.len();
        self.buffer.clear();
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.flush_buffer()
    }
}

#[cfg(target_arch = "wasm32")]
impl Write for CallbackWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut offset = 0;

        while offset < buf.len() {
            let remaining = self.chunk_size - self.buffer.len();
            let to_copy = remaining.min(buf.len() - offset);
            self.buffer
                .extend_from_slice(&buf[offset..offset + to_copy]);
            offset += to_copy;

            if self.buffer.len() >= self.chunk_size {
                self.flush_buffer().map_err(std::io::Error::other)?;
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_buffer().map_err(std::io::Error::other)
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for CallbackWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        if let Err(err) = self.flush_buffer() {
            web_sys::console::warn_1(&format!("CallbackWriter drop failed: {}", err).into());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub struct ProgressWriter {
    inner: CallbackWriter,
    progress_callback: Function,
    bytes_written: usize,
    next_report_at: usize,
}

#[cfg(target_arch = "wasm32")]
impl ProgressWriter {
    pub fn new(callback: Function, progress_callback: Function, chunk_size: usize) -> Result<Self> {
        Ok(Self {
            inner: CallbackWriter::new(callback, chunk_size)?,
            progress_callback,
            bytes_written: 0,
            next_report_at: PROGRESS_INTERVAL_BYTES,
        })
    }

    fn report_progress(&self) -> Result<()> {
        self.progress_callback
            .call1(
                &JsValue::NULL,
                &JsValue::from_f64(self.bytes_written as f64),
            )
            .map_err(map_callback_error)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.inner.flush_buffer()?;
        self.report_progress()?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl Write for ProgressWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes_written = self.inner.bytes_written + self.inner.buffer.len();

        if self.bytes_written >= self.next_report_at {
            self.next_report_at += PROGRESS_INTERVAL_BYTES;
            self.report_progress().map_err(std::io::Error::other)?;
        }

        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()?;
        self.bytes_written = self.inner.bytes_written + self.inner.buffer.len();
        self.report_progress().map_err(std::io::Error::other)
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for ProgressWriter {
    fn drop(&mut self) {
        if self.inner.buffer.is_empty() {
            return;
        }
        if let Err(err) = self.inner.flush_buffer() {
            web_sys::console::warn_1(&format!("ProgressWriter drop failed: {}", err).into());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn write_rds_with_callback(
    obj: &RObject,
    callback: Function,
    chunk_size_mb: Option<usize>,
) -> Result<()> {
    let chunk_size = chunk_size_mb_to_bytes(chunk_size_mb)?;
    let mut writer = CallbackWriter::new(callback, chunk_size)?;
    write_rds_streaming(obj, &mut writer)?;
    writer.finish()?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn write_rds_with_callback_and_compression(
    obj: &RObject,
    callback: Function,
    chunk_size_mb: Option<usize>,
    compression_level: u32,
) -> Result<()> {
    let chunk_size = chunk_size_mb_to_bytes(chunk_size_mb)?;
    let mut writer = CallbackWriter::new(callback, chunk_size)?;
    let compression = Compression::new(compression_level.min(9));
    write_rds_streaming_with_compression(obj, &mut writer, compression)?;
    writer.finish()?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn write_rds_with_progress(
    obj: &RObject,
    on_chunk: Function,
    on_progress: Function,
    chunk_size_mb: Option<usize>,
) -> Result<()> {
    let chunk_size = chunk_size_mb_to_bytes(chunk_size_mb)?;
    let mut writer = ProgressWriter::new(on_chunk, on_progress, chunk_size)?;
    write_rds_streaming(obj, &mut writer)?;
    writer.finish()?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn write_rds_with_progress_and_compression(
    obj: &RObject,
    on_chunk: Function,
    on_progress: Function,
    chunk_size_mb: Option<usize>,
    compression_level: u32,
) -> Result<()> {
    let chunk_size = chunk_size_mb_to_bytes(chunk_size_mb)?;
    let mut writer = ProgressWriter::new(on_chunk, on_progress, chunk_size)?;
    let compression = Compression::new(compression_level.min(9));
    write_rds_streaming_with_compression(obj, &mut writer, compression)?;
    writer.finish()?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn recommended_chunk_size_mb() -> usize {
    let memory_gb = web_sys::window().and_then(|w| {
        let navigator = w.navigator();
        js_sys::Reflect::get(&navigator, &JsValue::from_str("deviceMemory"))
            .ok()
            .and_then(|value| value.as_f64())
    });

    match memory_gb {
        Some(memory) if memory <= 2.0 => 1,
        Some(memory) if memory <= 4.0 => 4,
        Some(memory) if memory <= 8.0 => 8,
        Some(_) => 16,
        None => 4,
    }
}

#[cfg(target_arch = "wasm32")]
fn map_callback_error(err: JsValue) -> Error {
    let message = err.as_string().unwrap_or_else(|| format!("{:?}", err));
    Error::CallbackFailed(message)
}

#[cfg(target_arch = "wasm32")]
fn chunk_size_mb_to_bytes(chunk_size_mb: Option<usize>) -> Result<usize> {
    let mb = chunk_size_mb.unwrap_or(DEFAULT_CHUNK_SIZE_MB);
    if mb == 0 {
        return Err(Error::InvalidFormat(
            "chunk_size_mb must be greater than 0".to_string(),
        ));
    }
    Ok(mb * 1024 * 1024)
}
