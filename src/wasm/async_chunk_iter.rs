use std::io::Cursor;
use std::marker::PhantomData;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use byteorder::{BigEndian, ReadBytesExt};

#[cfg(target_arch = "wasm32")]
use crate::constants::{CHARSXP, REFSXP};
#[cfg(target_arch = "wasm32")]
use crate::wasm::AsyncRdsInput;
#[cfg(target_arch = "wasm32")]
use crate::{Complex, Error, LazyVector, Logical, Result};

#[derive(Debug, Clone, Copy)]
pub struct AsyncChunkConfig {
    pub max_elements: usize,
    pub max_bytes: usize,
}

impl Default for AsyncChunkConfig {
    fn default() -> Self {
        Self {
            max_elements: 10_000,
            max_bytes: 10 * 1024 * 1024,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub trait AsyncChunkElement: Sized {
    const BYTES: usize;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self>;
}

#[cfg(target_arch = "wasm32")]
impl AsyncChunkElement for i32 {
    const BYTES: usize = 4;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_i32::<BigEndian>()?)
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncChunkElement for f64 {
    const BYTES: usize = 8;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_f64::<BigEndian>()?)
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncChunkElement for Logical {
    const BYTES: usize = 4;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let raw = cursor.read_i32::<BigEndian>()?;
        Ok(match raw {
            1 => Logical::True,
            0 => Logical::False,
            _ => Logical::Na,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncChunkElement for u8 {
    const BYTES: usize = 1;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_u8()?)
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncChunkElement for Complex {
    const BYTES: usize = 16;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;
        Ok(Complex { real, imaginary })
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub struct AsyncFixedLazyChunkIter<'a, T> {
    source: &'a dyn AsyncRdsInput,
    span: LazyVector,
    pos: usize,
    chunk_elements: usize,
    _phantom: PhantomData<T>,
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
impl<'a, T: AsyncChunkElement> AsyncFixedLazyChunkIter<'a, T> {
    pub fn new(source: &'a dyn AsyncRdsInput, span: LazyVector, config: AsyncChunkConfig) -> Self {
        let max_by_bytes = config.max_bytes.checked_div(T::BYTES).unwrap_or(0).max(1);
        let chunk_elements = config.max_elements.min(max_by_bytes).max(1);
        Self {
            source,
            span,
            pos: 0,
            chunk_elements,
            _phantom: PhantomData,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
impl<'a, T> AsyncFixedLazyChunkIter<'a, T>
where
    T: AsyncChunkElement,
{
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<T>>> {
        if self.pos >= self.span.length {
            return Ok(None);
        }
        let remaining = self.span.length - self.pos;
        let take = remaining.min(self.chunk_elements);
        let offset_bytes = self
            .span
            .offset
            .checked_add((self.pos * T::BYTES) as u64)
            .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))?;
        let total_bytes = take * T::BYTES;
        let bytes = self.source.read_at(offset_bytes, total_bytes).await?;
        if bytes.len() != total_bytes {
            return Err(Error::TruncatedLazyPayload {
                expected: self.span.byte_len,
                actual: (self.pos * T::BYTES + bytes.len()) as u64,
            });
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut out = Vec::with_capacity(take);
        for _ in 0..take {
            out.push(T::read_from(&mut cursor)?);
        }
        self.pos += take;
        Ok(Some(out))
    }
}

#[cfg(target_arch = "wasm32")]
struct AsyncSpanReader<'a> {
    input: &'a dyn AsyncRdsInput,
    start: u64,
    end: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_pos: usize,
    buf_len: usize,
    chunk_size: usize,
}

#[cfg(target_arch = "wasm32")]
impl<'a> AsyncSpanReader<'a> {
    fn new(input: &'a dyn AsyncRdsInput, span: LazyVector, chunk_size: usize) -> Result<Self> {
        let end = span
            .offset
            .checked_add(span.byte_len)
            .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))?;
        Ok(Self {
            input,
            start: span.offset,
            end,
            pos: span.offset,
            buf: vec![0u8; chunk_size.max(1)],
            buf_pos: 0,
            buf_len: 0,
            chunk_size: chunk_size.max(1),
        })
    }

    fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    async fn fill(&mut self) -> Result<()> {
        if self.remaining() == 0 {
            return Err(Error::TruncatedLazyPayload {
                expected: self.end - self.start,
                actual: self.pos - self.start,
            });
        }
        let to_read = self.remaining().min(self.chunk_size as u64) as usize;
        let chunk = self.input.read_at(self.pos, to_read).await?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: self.end - self.start,
                actual: self.pos - self.start + chunk.len() as u64,
            });
        }
        self.buf[..to_read].copy_from_slice(&chunk);
        self.buf_pos = 0;
        self.buf_len = to_read;
        self.pos += to_read as u64;
        Ok(())
    }

    async fn read_exact(&mut self, out: &mut [u8]) -> Result<()> {
        let mut written = 0;
        while written < out.len() {
            if self.buf_pos == self.buf_len {
                self.fill().await?;
            }
            let available = self.buf_len - self.buf_pos;
            let needed = out.len() - written;
            let to_copy = available.min(needed);
            out[written..written + to_copy]
                .copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + to_copy]);
            self.buf_pos += to_copy;
            written += to_copy;
        }
        Ok(())
    }

    async fn read_u32(&mut self) -> Result<u32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes).await?;
        Ok(u32::from_be_bytes(bytes))
    }

    async fn read_i32(&mut self) -> Result<i32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes).await?;
        Ok(i32::from_be_bytes(bytes))
    }

    async fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf).await?;
        Ok(buf)
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_charsxp_content_streaming_async(
    reader: &mut AsyncSpanReader<'_>,
    flags: u32,
) -> Result<Option<Arc<str>>> {
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    let length = if use_compact {
        let bytes = reader.read_bytes(3).await?;
        ((bytes[0] as i32) << 16) | ((bytes[1] as i32) << 8) | (bytes[2] as i32)
    } else {
        reader.read_i32().await?
    };

    if length == -1 {
        // NA_character_
        return Ok(None);
    }
    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative CHARSXP length {}",
            length
        )));
    }

    let length = length as usize;
    let bytes = reader.read_bytes(length).await?;
    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok(Some(Arc::from(string.as_str())))
}

#[cfg(target_arch = "wasm32")]
pub struct AsyncLazyCharacterChunkIter<'a> {
    reader: AsyncSpanReader<'a>,
    remaining: usize,
    config: AsyncChunkConfig,
    cache: Vec<Option<Arc<str>>>,
}

#[cfg(target_arch = "wasm32")]
impl<'a> AsyncLazyCharacterChunkIter<'a> {
    pub fn new(
        source: &'a dyn AsyncRdsInput,
        span: LazyVector,
        config: AsyncChunkConfig,
    ) -> Result<Self> {
        let reader = AsyncSpanReader::new(source, span, config.max_bytes.max(1))?;
        Ok(Self {
            reader,
            remaining: span.length,
            config,
            cache: Vec::new(),
        })
    }

    pub async fn next_chunk(&mut self) -> Result<Option<Vec<Option<Arc<str>>>>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while self.remaining > 0 && out.len() < self.config.max_elements {
            let flags = self.reader.read_u32().await?;
            let type_from_0_7 = flags & 0xFF;
            let type_from_8_15 = (flags >> 8) & 0xFF;

            let value = if type_from_0_7 == REFSXP {
                let ref_index = (flags >> 8) as usize;
                if ref_index == 0 || ref_index > self.cache.len() {
                    return Err(Error::InvalidFormat(format!(
                        "Invalid string reference: {} (cache size: {})",
                        ref_index,
                        self.cache.len()
                    )));
                }
                self.cache[ref_index - 1].clone()
            } else if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
                let parsed = parse_charsxp_content_streaming_async(&mut self.reader, flags).await?;
                self.cache.push(parsed.clone());
                parsed
            } else {
                return Err(Error::Unsupported(
                    "non-CHARSXP element in character vector".to_string(),
                ));
            };

            let value_bytes = value.as_deref().map_or(0, str::len);
            if !out.is_empty() && bytes + value_bytes > self.config.max_bytes {
                out.push(value);
                self.remaining -= 1;
                return Ok(Some(out));
            }
            bytes += value_bytes;
            out.push(value);
            self.remaining -= 1;
            if bytes >= self.config.max_bytes {
                break;
            }
        }
        Ok(Some(out))
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    struct TestAsyncInput {
        data: Vec<u8>,
    }

    impl AsyncRdsInput for TestAsyncInput {
        fn read_at<'a>(&'a self, offset: u64, len: usize) -> crate::wasm::AsyncReadFuture<'a> {
            Box::pin(async move {
                let start = offset as usize;
                let end = start.saturating_add(len);
                if end > self.data.len() {
                    return Err(Error::UnexpectedEofDetail {
                        position: start,
                        needed: len,
                        available: self.data.len().saturating_sub(start),
                    });
                }
                Ok(self.data[start..end].to_vec())
            })
        }

        fn len(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
    }

    #[wasm_bindgen_test]
    async fn async_fixed_lazy_chunk_iter_reads_values() {
        let mut data = Vec::new();
        for value in [1i32, 2, 3, 4] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestAsyncInput { data };
        let span = LazyVector {
            length: 4,
            offset: 0,
            byte_len: 16,
        };
        let mut iter = AsyncFixedLazyChunkIter::<i32>::new(
            &input,
            span,
            AsyncChunkConfig {
                max_elements: 2,
                max_bytes: 1024,
            },
        );
        let first = iter.next_chunk().await.unwrap().unwrap();
        let second = iter.next_chunk().await.unwrap().unwrap();
        assert_eq!(first, vec![1, 2]);
        assert_eq!(second, vec![3, 4]);
        assert!(iter.next_chunk().await.unwrap().is_none());
    }

    #[wasm_bindgen_test]
    async fn async_lazy_character_chunk_iter_reads_values() {
        let mut data = Vec::new();
        for value in ["a", "bbb"] {
            data.extend_from_slice(&(CHARSXP as u32).to_be_bytes());
            data.extend_from_slice(&(value.len() as i32).to_be_bytes());
            data.extend_from_slice(value.as_bytes());
        }
        let input = TestAsyncInput { data };
        let span = LazyVector {
            length: 2,
            offset: 0,
            byte_len: span_len(&input),
        };
        let mut iter = AsyncLazyCharacterChunkIter::new(
            &input,
            span,
            AsyncChunkConfig {
                max_elements: 1,
                max_bytes: 16,
            },
        )
        .unwrap();
        let first = iter.next_chunk().await.unwrap().unwrap();
        let second = iter.next_chunk().await.unwrap().unwrap();
        assert_eq!(first, vec![Some(Arc::from("a"))]);
        assert_eq!(second, vec![Some(Arc::from("bbb"))]);
        assert!(iter.next_chunk().await.unwrap().is_none());
    }

    fn span_len(input: &TestAsyncInput) -> u64 {
        input.data.len() as u64
    }
}
