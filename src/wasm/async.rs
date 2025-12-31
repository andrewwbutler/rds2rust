#[cfg(target_arch = "wasm32")]
use std::future::Future;
#[cfg(target_arch = "wasm32")]
use std::pin::Pin;

#[cfg(target_arch = "wasm32")]
use byteorder::{BigEndian, ReadBytesExt};

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
pub trait AsyncRdsInput {
    fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a>;
    fn len(&self) -> Option<u64>;
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
    pub async fn new(
        source: &'a dyn AsyncRdsInput,
        config: AsyncCursorConfig,
    ) -> Result<Self> {
        let mut cursor = Self {
            source,
            buffer: Vec::new(),
            buffer_offset: 0,
            position: 0,
            buffer_size: config.buffer_size,
            max_buffer_size: config.max_buffer_size,
        };
        cursor.refill(config.buffer_size).await?;
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

        if self.available_in_buffer() >= needed {
            return Ok(());
        }

        let request = std::cmp::max(needed, self.buffer_size);
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
        let bytes = self.source.read_at(self.position, len).await?;
        self.buffer_offset = self.position;
        self.buffer = bytes;
        Ok(())
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
