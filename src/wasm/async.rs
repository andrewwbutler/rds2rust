#[cfg(target_arch = "wasm32")]
use std::future::Future;
#[cfg(target_arch = "wasm32")]
use std::pin::Pin;

#[cfg(target_arch = "wasm32")]
use byteorder::{BigEndian, ReadBytesExt};
#[cfg(target_arch = "wasm32")]
use js_sys::Reflect;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
use crate::constants::{
    BASEENV_SXP, EMPTYENV_SXP, GLOBALENV_SXP, INTSXP, LGLSXP, NILSXP, RAWSXP, REALSXP, STRSXP,
    SYMSXP, VECSXP,
};
#[cfg(target_arch = "wasm32")]
use crate::{Error, Result};

#[cfg(target_arch = "wasm32")]
pub type AsyncReadFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type AsyncEnsureFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

#[cfg(target_arch = "wasm32")]
pub trait AsyncRdsInput {
    fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a>;
    fn len(&self) -> Option<u64>;
}

/// Trait for sequential-only input sources (e.g., streaming decompression).
/// Unlike `AsyncRdsInput`, this trait only supports forward-only reading,
/// which enables bounded memory usage for compressed streams.
#[cfg(target_arch = "wasm32")]
pub trait AsyncSequentialInput {
    /// Read the next `len` bytes from the current position.
    /// Returns fewer bytes if end of stream is reached.
    /// Reading is strictly forward-only - cannot seek backwards.
    fn read_next<'a>(&'a mut self, len: usize) -> AsyncReadFuture<'a>;

    /// Returns total size if known (None for compressed streams where
    /// decompressed size is unknown until fully decompressed).
    fn total_size(&self) -> Option<u64>;

    /// Returns current read position in the decompressed stream.
    fn position(&self) -> u64;
}

#[cfg(target_arch = "wasm32")]
pub trait AsyncCursor {
    fn ensure_available<'a>(&'a mut self, needed: usize) -> AsyncEnsureFuture<'a>;
    fn as_sync_slice(&self, len: usize) -> Result<&[u8]>;
    fn advance(&mut self, bytes: u64) -> Result<()>;
    fn position(&self) -> u64;
    fn total_len(&self) -> Option<u64>;
    fn buffer_size(&self) -> usize;
    fn max_buffer_size(&self) -> usize;
    fn peek_u32(&self) -> Result<u32>;
}

/// Adapter that implements `AsyncSequentialInput` from any `AsyncRdsInput`.
/// This allows using random-access sources in sequential-only contexts.
#[cfg(target_arch = "wasm32")]
pub struct SequentialAdapter<T: AsyncRdsInput> {
    inner: T,
    position: u64,
}

#[cfg(target_arch = "wasm32")]
impl<T: AsyncRdsInput> SequentialAdapter<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, position: 0 }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: AsyncRdsInput> AsyncSequentialInput for SequentialAdapter<T> {
    fn read_next<'a>(&'a mut self, len: usize) -> AsyncReadFuture<'a> {
        Box::pin(async move {
            let data = self.inner.read_at(self.position, len).await?;
            self.position += data.len() as u64;
            Ok(data)
        })
    }

    fn total_size(&self) -> Option<u64> {
        self.inner.len()
    }

    fn position(&self) -> u64 {
        self.position
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct AsyncCursorConfig {
    pub buffer_size: usize,
    pub max_buffer_size: usize,
}

#[cfg(target_arch = "wasm32")]
impl Default for AsyncCursorConfig {
    fn default() -> Self {
        Self {
            buffer_size: 64 * 1024 * 1024,
            max_buffer_size: 128 * 1024 * 1024,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn recommended_buffer_config() -> AsyncCursorConfig {
    let device_memory_gb: f64 = web_sys::window()
        .and_then(|w| {
            let nav = w.navigator();
            Reflect::get(&nav, &JsValue::from_str("deviceMemory"))
                .ok()
                .and_then(|value| value.as_f64())
        })
        .unwrap_or(4.0);

    let buffer_mb = (device_memory_gb * 32.0_f64).min(128.0_f64).max(16.0_f64) as usize;
    let max_buffer_mb = (buffer_mb * 2).min(256);

    AsyncCursorConfig {
        buffer_size: buffer_mb * 1024 * 1024,
        max_buffer_size: max_buffer_mb * 1024 * 1024,
    }
}

#[cfg(target_arch = "wasm32")]
pub struct AsyncBufferedCursor<'a> {
    source: &'a dyn AsyncRdsInput,
    buffer: Vec<u8>,
    buffer_offset: u64,
    position: u64,
    buffer_size: usize,
    max_buffer_size: usize,
}

#[cfg(target_arch = "wasm32")]
impl<'a> AsyncBufferedCursor<'a> {
    pub async fn new(source: &'a dyn AsyncRdsInput, config: AsyncCursorConfig) -> Result<Self> {
        let initial = if let Some(total_len) = source.len() {
            let remaining = total_len.saturating_sub(0) as usize;
            std::cmp::min(config.buffer_size, remaining)
        } else {
            config.buffer_size
        };
        let mut cursor = Self {
            source,
            buffer: Vec::new(),
            buffer_offset: 0,
            position: 0,
            buffer_size: config.buffer_size,
            max_buffer_size: config.max_buffer_size,
        };
        cursor.refill(initial).await?;
        Ok(cursor)
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn total_len(&self) -> Option<u64> {
        self.source.len()
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_size
    }

    pub fn advance(&mut self, bytes: u64) -> Result<()> {
        let new_pos = self.position.saturating_add(bytes);
        if let Some(len) = self.source.len() {
            if new_pos > len {
                return Err(Error::UnexpectedEof);
            }
        }
        self.position = new_pos;
        Ok(())
    }

    pub async fn ensure_available(&mut self, needed: usize) -> Result<()> {
        if needed > self.max_buffer_size {
            return Err(Error::InvalidFormat(format!(
                "async buffer request {} exceeds max {}",
                needed, self.max_buffer_size
            )));
        }

        if let Some(total_len) = self.source.len() {
            let remaining = total_len.saturating_sub(self.position) as usize;
            if needed > remaining {
                return Err(Error::UnexpectedEofDetail {
                    position: self.position as usize,
                    needed,
                    available: remaining,
                });
            }
        }

        if self.available_in_buffer() >= needed {
            return Ok(());
        }

        let mut request = std::cmp::max(needed, self.buffer_size);
        if let Some(total_len) = self.source.len() {
            let remaining = total_len.saturating_sub(self.position) as usize;
            request = request.min(remaining);
        }
        if request == 0 {
            return Err(Error::UnexpectedEof);
        }
        self.refill(request).await?;
        if self.available_in_buffer() < needed {
            return Err(Error::UnexpectedEof);
        }
        Ok(())
    }

    pub fn as_sync_slice(&self, len: usize) -> Result<&[u8]> {
        if self.available_in_buffer() < len {
            return Err(Error::UnexpectedEof);
        }
        let start = (self.position - self.buffer_offset) as usize;
        Ok(&self.buffer[start..start + len])
    }

    pub fn peek_u32(&self) -> Result<u32> {
        let slice = self.as_sync_slice(4)?;
        let mut cursor = std::io::Cursor::new(slice);
        Ok(cursor.read_u32::<BigEndian>()?)
    }

    fn available_in_buffer(&self) -> usize {
        let start = (self.position - self.buffer_offset) as usize;
        self.buffer.len().saturating_sub(start)
    }

    async fn refill(&mut self, len: usize) -> Result<()> {
        let request = if let Some(total_len) = self.source.len() {
            let remaining = total_len.saturating_sub(self.position) as usize;
            std::cmp::min(len, remaining)
        } else {
            len
        };
        if request == 0 {
            return Err(Error::UnexpectedEof);
        }
        let bytes = self.source.read_at(self.position, request).await?;
        self.buffer_offset = self.position;
        self.buffer = bytes;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a> AsyncCursor for AsyncBufferedCursor<'a> {
    fn ensure_available<'b>(&'b mut self, needed: usize) -> AsyncEnsureFuture<'b> {
        Box::pin(async move { AsyncBufferedCursor::ensure_available(self, needed).await })
    }

    fn as_sync_slice(&self, len: usize) -> Result<&[u8]> {
        AsyncBufferedCursor::as_sync_slice(self, len)
    }

    fn advance(&mut self, bytes: u64) -> Result<()> {
        AsyncBufferedCursor::advance(self, bytes)
    }

    fn position(&self) -> u64 {
        AsyncBufferedCursor::position(self)
    }

    fn total_len(&self) -> Option<u64> {
        AsyncBufferedCursor::total_len(self)
    }

    fn buffer_size(&self) -> usize {
        AsyncBufferedCursor::buffer_size(self)
    }

    fn max_buffer_size(&self) -> usize {
        AsyncBufferedCursor::max_buffer_size(self)
    }

    fn peek_u32(&self) -> Result<u32> {
        AsyncBufferedCursor::peek_u32(self)
    }
}

#[cfg(target_arch = "wasm32")]
pub fn estimate_parse_size(cursor: &AsyncBufferedCursor<'_>) -> Result<usize> {
    if cursor.available_in_buffer() < 8 {
        return Err(Error::UnexpectedEof);
    }

    let slice = cursor.as_sync_slice(8)?;
    let mut temp = std::io::Cursor::new(slice);
    let flags = temp.read_u32::<BigEndian>()?;
    let length = temp.read_i32::<BigEndian>()?.max(0) as usize;

    let sexp_type = flags & 0xFF;
    let estimate = match sexp_type {
        NILSXP | SYMSXP | EMPTYENV_SXP | GLOBALENV_SXP | BASEENV_SXP => 4,
        INTSXP | LGLSXP => 4 + 4 + length.saturating_mul(4) + 1024,
        REALSXP => 4 + 4 + length.saturating_mul(8) + 1024,
        RAWSXP => 4 + 4 + length + 1024,
        STRSXP => 4 + 4 + length.saturating_mul(100),
        VECSXP => 4 + 4 + length.saturating_mul(10 * 1024),
        _ => 1024 * 1024,
    };

    Ok(estimate)
}

/// Cursor adapter that provides buffered reading over a sequential input.
///
/// This cursor is similar to `AsyncBufferedCursor` but works with `AsyncSequentialInput`
/// sources that only support forward reading. It maintains a bounded buffer and
/// automatically drops consumed bytes to prevent memory growth.
///
/// # Memory Management
///
/// The cursor enforces monotonic reads - attempting to read at an offset lower than
/// previously consumed data will result in an error. This ensures bounded memory usage.
#[cfg(target_arch = "wasm32")]
pub struct SequentialCursor<'a, I: AsyncSequentialInput> {
    input: &'a mut I,
    buffer: std::collections::VecDeque<u8>,
    buffer_start_pos: u64,
    position: u64,
    buffer_size: usize,
    max_buffer_size: usize,
}

#[cfg(target_arch = "wasm32")]
impl<'a, I: AsyncSequentialInput> SequentialCursor<'a, I> {
    /// Create a new sequential cursor with default configuration.
    pub async fn new(input: &'a mut I) -> Result<Self> {
        let config = recommended_buffer_config();
        Self::with_config(input, config).await
    }

    /// Create a new sequential cursor with specific buffer configuration.
    pub async fn with_config(input: &'a mut I, config: AsyncCursorConfig) -> Result<Self> {
        let mut cursor = Self {
            input,
            buffer: std::collections::VecDeque::new(),
            buffer_start_pos: 0,
            position: 0,
            buffer_size: config.buffer_size,
            max_buffer_size: config.max_buffer_size,
        };

        // Pre-fill buffer with initial data
        cursor.ensure_available(config.buffer_size).await?;
        Ok(cursor)
    }

    /// Returns the current read position.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Returns the total size if known (None for sequential streams).
    pub fn total_len(&self) -> Option<u64> {
        self.input.total_size()
    }

    /// Returns the buffer size configuration.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Returns the maximum buffer size configuration.
    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_size
    }

    /// Advance the read position by the specified number of bytes.
    pub fn advance(&mut self, bytes: u64) -> Result<()> {
        let new_pos = self.position.saturating_add(bytes);
        if let Some(len) = self.input.total_size() {
            if new_pos > len {
                return Err(Error::UnexpectedEof);
            }
        }
        self.position = new_pos;
        Ok(())
    }

    /// Ensure that at least `needed` bytes are available in the buffer at the current position.
    pub async fn ensure_available(&mut self, needed: usize) -> Result<()> {
        if needed > self.max_buffer_size {
            return Err(Error::InvalidFormat(format!(
                "sequential buffer request {} exceeds max {}",
                needed, self.max_buffer_size
            )));
        }

        // Check monotonic access - cannot read backwards
        if self.position < self.buffer_start_pos {
            return Err(Error::ParseError(
                "Sequential input does not support seeking backwards".into(),
            ));
        }

        let offset_in_buffer = (self.position - self.buffer_start_pos) as usize;

        // Check if we already have enough data
        if offset_in_buffer + needed <= self.buffer.len() {
            // Drop consumed bytes to keep memory bounded
            if offset_in_buffer > 0 {
                self.buffer.drain(..offset_in_buffer);
                self.buffer_start_pos = self.position;
            }
            self.buffer.make_contiguous();
            return Ok(());
        }

        // Need to read more data
        let buffer_end = self.buffer_start_pos + self.buffer.len() as u64;
        let needed_end = self.position + needed as u64;
        let to_read = (needed_end - buffer_end) as usize;

        // Read in chunks if needed
        let mut remaining = to_read;
        while remaining > 0 {
            let chunk_size = remaining.min(self.buffer_size);
            let chunk = self.input.read_next(chunk_size).await?;

            if chunk.is_empty() {
                // Reached end of stream
                if offset_in_buffer + needed > self.buffer.len() {
                    return Err(Error::UnexpectedEofDetail {
                        position: self.position as usize,
                        needed,
                        available: self.buffer.len().saturating_sub(offset_in_buffer),
                    });
                }
                break;
            }

            self.buffer.extend(chunk.iter());
            remaining = remaining.saturating_sub(chunk.len());
        }

        // Drop consumed bytes
        let offset_in_buffer = (self.position - self.buffer_start_pos) as usize;
        if offset_in_buffer > 0 {
            self.buffer.drain(..offset_in_buffer);
            self.buffer_start_pos = self.position;
        }

        self.buffer.make_contiguous();

        // Verify we have enough data now
        if needed > self.buffer.len() {
            return Err(Error::UnexpectedEofDetail {
                position: self.position as usize,
                needed,
                available: self.buffer.len(),
            });
        }

        Ok(())
    }

    /// Get a slice of bytes at the current position without advancing.
    pub fn as_sync_slice(&self, len: usize) -> Result<&[u8]> {
        let offset = (self.position - self.buffer_start_pos) as usize;

        if offset + len > self.buffer.len() {
            return Err(Error::UnexpectedEof);
        }

        // ensure_available() always makes the buffer contiguous
        let (slice1, slice2) = self.buffer.as_slices();
        if !slice2.is_empty() {
            return Err(Error::ParseError(
                "Buffer fragmentation - call ensure_available before as_sync_slice".into(),
            ));
        }
        Ok(&slice1[offset..offset + len])
    }

    /// Peek a u32 value at the current position without advancing.
    pub fn peek_u32(&self) -> Result<u32> {
        let slice = self.as_sync_slice(4)?;
        let mut cursor = std::io::Cursor::new(slice);
        Ok(cursor.read_u32::<BigEndian>()?)
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, I: AsyncSequentialInput> AsyncCursor for SequentialCursor<'a, I> {
    fn ensure_available<'b>(&'b mut self, needed: usize) -> AsyncEnsureFuture<'b> {
        Box::pin(async move { SequentialCursor::ensure_available(self, needed).await })
    }

    fn as_sync_slice(&self, len: usize) -> Result<&[u8]> {
        SequentialCursor::as_sync_slice(self, len)
    }

    fn advance(&mut self, bytes: u64) -> Result<()> {
        SequentialCursor::advance(self, bytes)
    }

    fn position(&self) -> u64 {
        SequentialCursor::position(self)
    }

    fn total_len(&self) -> Option<u64> {
        SequentialCursor::total_len(self)
    }

    fn buffer_size(&self) -> usize {
        SequentialCursor::buffer_size(self)
    }

    fn max_buffer_size(&self) -> usize {
        SequentialCursor::max_buffer_size(self)
    }

    fn peek_u32(&self) -> Result<u32> {
        SequentialCursor::peek_u32(self)
    }
}
