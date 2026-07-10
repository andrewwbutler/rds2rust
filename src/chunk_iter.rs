#[cfg(not(target_arch = "wasm32"))]
use std::io::Cursor;
#[cfg(not(target_arch = "wasm32"))]
use std::marker::PhantomData;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use byteorder::{BigEndian, ReadBytesExt};

#[cfg(not(target_arch = "wasm32"))]
use crate::constants::{CHARSXP, REFSXP};
#[cfg(not(target_arch = "wasm32"))]
use crate::{Complex, Error, LazyVector, Logical, RdsInput, Result, VectorData};

#[derive(Debug, Clone, Copy)]
pub struct ChunkConfig {
    pub max_elements: usize,
    pub max_bytes: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            max_elements: 10_000,
            max_bytes: 10 * 1024 * 1024,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub trait ChunkElement: Sized {
    const BYTES: usize;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self>;
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkElement for i32 {
    const BYTES: usize = 4;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_i32::<BigEndian>()?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkElement for f64 {
    const BYTES: usize = 8;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_f64::<BigEndian>()?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkElement for Logical {
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

#[cfg(not(target_arch = "wasm32"))]
impl ChunkElement for u8 {
    const BYTES: usize = 1;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(cursor.read_u8()?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ChunkElement for Complex {
    const BYTES: usize = 16;
    fn read_from(cursor: &mut Cursor<&[u8]>) -> Result<Self> {
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;
        Ok(Complex { real, imaginary })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub enum VectorChunkIter<'a, T> {
    Owned(OwnedChunkIter<'a, T>),
    Lazy(FixedLazyChunkIter<'a, T>),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct OwnedChunkIter<'a, T> {
    data: &'a [T],
    pos: usize,
    max_elements: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T> OwnedChunkIter<'a, T> {
    fn new(data: &'a [T], max_elements: usize) -> Self {
        Self {
            data,
            pos: 0,
            max_elements: max_elements.max(1),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T: Clone> Iterator for OwnedChunkIter<'a, T> {
    type Item = Result<Vec<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let end = (self.pos + self.max_elements).min(self.data.len());
        let chunk = self.data[self.pos..end].to_vec();
        self.pos = end;
        Some(Ok(chunk))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct FixedLazyChunkIter<'a, T> {
    source: &'a dyn RdsInput,
    span: LazyVector,
    pos: usize,
    chunk_elements: usize,
    _phantom: PhantomData<T>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T> FixedLazyChunkIter<'a, T> {
    fn new(source: &'a dyn RdsInput, span: LazyVector, chunk_elements: usize) -> Self {
        Self {
            source,
            span,
            pos: 0,
            chunk_elements: chunk_elements.max(1),
            _phantom: PhantomData,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T> Iterator for FixedLazyChunkIter<'a, T>
where
    T: ChunkElement,
{
    type Item = Result<Vec<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.span.length {
            return None;
        }
        let remaining = self.span.length - self.pos;
        let take = remaining.min(self.chunk_elements);
        let offset_bytes = self
            .span
            .offset
            .checked_add((self.pos * T::BYTES) as u64)
            .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()));
        let offset_bytes = match offset_bytes {
            Ok(offset) => offset,
            Err(err) => return Some(Err(err)),
        };
        let total_bytes = take * T::BYTES;
        let bytes = match self.source.read_at(offset_bytes, total_bytes) {
            Ok(bytes) => bytes,
            Err(err) => return Some(Err(err)),
        };
        if bytes.len() != total_bytes {
            return Some(Err(Error::TruncatedLazyPayload {
                expected: self.span.byte_len,
                actual: (self.pos * T::BYTES + bytes.len()) as u64,
            }));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut out = Vec::with_capacity(take);
        for _ in 0..take {
            match T::read_from(&mut cursor) {
                Ok(value) => out.push(value),
                Err(err) => return Some(Err(err)),
            }
        }
        self.pos += take;
        Some(Ok(out))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T> VectorChunkIter<'a, T>
where
    T: ChunkElement + Clone,
{
    fn from_owned(data: &'a [T], config: ChunkConfig) -> Self {
        let max_by_bytes = config.max_bytes.checked_div(T::BYTES).unwrap_or(0).max(1);
        let max_elements = config.max_elements.min(max_by_bytes).max(1);
        VectorChunkIter::Owned(OwnedChunkIter::new(data, max_elements))
    }

    fn from_lazy(source: &'a dyn RdsInput, span: LazyVector, config: ChunkConfig) -> Self {
        let max_by_bytes = config.max_bytes.checked_div(T::BYTES).unwrap_or(0).max(1);
        let max_elements = config.max_elements.min(max_by_bytes).max(1);
        VectorChunkIter::Lazy(FixedLazyChunkIter::new(source, span, max_elements))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, T: ChunkElement + Clone> Iterator for VectorChunkIter<'a, T> {
    type Item = Result<Vec<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            VectorChunkIter::Owned(iter) => iter.next(),
            VectorChunkIter::Lazy(iter) => iter.next(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub enum CharacterChunkIter<'a> {
    Owned(OwnedCharacterChunkIter<'a>),
    Lazy(LazyCharacterChunkIter<'a>),
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> Iterator for CharacterChunkIter<'a> {
    type Item = Result<Vec<Option<Arc<str>>>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            CharacterChunkIter::Owned(iter) => iter.next(),
            CharacterChunkIter::Lazy(iter) => iter.next(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct OwnedCharacterChunkIter<'a> {
    data: &'a [Option<Arc<str>>],
    pos: usize,
    config: ChunkConfig,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> OwnedCharacterChunkIter<'a> {
    fn new(data: &'a [Option<Arc<str>>], config: ChunkConfig) -> Self {
        Self {
            data,
            pos: 0,
            config,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> Iterator for OwnedCharacterChunkIter<'a> {
    type Item = Result<Vec<Option<Arc<str>>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while self.pos < self.data.len() && out.len() < self.config.max_elements {
            let value = self.data[self.pos].clone();
            let value_bytes = value.as_deref().map_or(0, str::len);
            if !out.is_empty() && bytes + value_bytes > self.config.max_bytes {
                break;
            }
            bytes += value_bytes;
            out.push(value);
            self.pos += 1;
            if bytes >= self.config.max_bytes {
                break;
            }
        }
        Some(Ok(out))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct LazyCharacterChunkIter<'a> {
    reader: SpanReader<'a>,
    remaining: usize,
    config: ChunkConfig,
    cache: Vec<Option<Arc<str>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> LazyCharacterChunkIter<'a> {
    fn new(source: &'a dyn RdsInput, span: LazyVector, config: ChunkConfig) -> Result<Self> {
        let reader = SpanReader::new(source, span, config.max_bytes.max(1))?;
        Ok(Self {
            reader,
            remaining: span.length,
            config,
            cache: Vec::new(),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> Iterator for LazyCharacterChunkIter<'a> {
    type Item = Result<Vec<Option<Arc<str>>>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while self.remaining > 0 && out.len() < self.config.max_elements {
            let flags = match self.reader.read_u32() {
                Ok(flags) => flags,
                Err(err) => return Some(Err(err)),
            };
            let type_from_0_7 = flags & 0xFF;
            let type_from_8_15 = (flags >> 8) & 0xFF;

            let value = if type_from_0_7 == REFSXP {
                let ref_index = (flags >> 8) as usize;
                if ref_index == 0 || ref_index > self.cache.len() {
                    return Some(Err(Error::InvalidFormat(format!(
                        "Invalid string reference: {} (cache size: {})",
                        ref_index,
                        self.cache.len()
                    ))));
                }
                self.cache[ref_index - 1].clone()
            } else if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
                let parsed = match parse_charsxp_content_streaming(&mut self.reader, flags) {
                    Ok(value) => value,
                    Err(err) => return Some(Err(err)),
                };
                self.cache.push(parsed.clone());
                parsed
            } else {
                return Some(Err(Error::Unsupported(
                    "non-CHARSXP element in character vector".to_string(),
                )));
            };

            let value_bytes = value.as_deref().map_or(0, str::len);
            if !out.is_empty() && bytes + value_bytes > self.config.max_bytes {
                // Put it back? Not possible with streaming, so yield oversized element alone.
                out.push(value);
                self.remaining -= 1;
                return Some(Ok(out));
            }
            bytes += value_bytes;
            out.push(value);
            self.remaining -= 1;
            if bytes >= self.config.max_bytes {
                break;
            }
        }
        Some(Ok(out))
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct SpanReader<'a> {
    input: &'a dyn RdsInput,
    start: u64,
    end: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_pos: usize,
    buf_len: usize,
    chunk_size: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> SpanReader<'a> {
    fn new(input: &'a dyn RdsInput, span: LazyVector, chunk_size: usize) -> Result<Self> {
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

    fn fill(&mut self) -> Result<()> {
        if self.remaining() == 0 {
            return Err(Error::TruncatedLazyPayload {
                expected: self.end - self.start,
                actual: self.pos - self.start,
            });
        }
        let to_read = self.remaining().min(self.chunk_size as u64) as usize;
        let chunk = self.input.read_at(self.pos, to_read)?;
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

    fn read_exact(&mut self, out: &mut [u8]) -> Result<()> {
        let mut written = 0;
        while written < out.len() {
            if self.buf_pos == self.buf_len {
                self.fill()?;
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

    fn read_u32(&mut self) -> Result<u32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_i32(&mut self) -> Result<i32> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes)?;
        Ok(i32::from_be_bytes(bytes))
    }

    fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_charsxp_content_streaming(
    reader: &mut SpanReader<'_>,
    flags: u32,
) -> Result<Option<Arc<str>>> {
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    let length = if use_compact {
        let bytes = reader.read_bytes(3)?;
        ((bytes[0] as i32) << 16) | ((bytes[1] as i32) << 8) | (bytes[2] as i32)
    } else {
        reader.read_i32()?
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
    let bytes = reader.read_bytes(length)?;
    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok(Some(Arc::from(string.as_str())))
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<i32> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<VectorChunkIter<'a, i32>> {
        match self {
            VectorData::Owned(vec) => Ok(VectorChunkIter::from_owned(vec, config)),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(VectorChunkIter::from_lazy(source, *span, config))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<f64> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<VectorChunkIter<'a, f64>> {
        match self {
            VectorData::Owned(vec) => Ok(VectorChunkIter::from_owned(vec, config)),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(VectorChunkIter::from_lazy(source, *span, config))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<Logical> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<VectorChunkIter<'a, Logical>> {
        match self {
            VectorData::Owned(vec) => Ok(VectorChunkIter::from_owned(vec, config)),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(VectorChunkIter::from_lazy(source, *span, config))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<u8> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<VectorChunkIter<'a, u8>> {
        match self {
            VectorData::Owned(vec) => Ok(VectorChunkIter::from_owned(vec, config)),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(VectorChunkIter::from_lazy(source, *span, config))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<Complex> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<VectorChunkIter<'a, Complex>> {
        match self {
            VectorData::Owned(vec) => Ok(VectorChunkIter::from_owned(vec, config)),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(VectorChunkIter::from_lazy(source, *span, config))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VectorData<Option<Arc<str>>> {
    pub fn iter_chunks<'a>(
        &'a self,
        source: Option<&'a dyn RdsInput>,
        config: ChunkConfig,
    ) -> Result<CharacterChunkIter<'a>> {
        match self {
            VectorData::Owned(vec) => Ok(CharacterChunkIter::Owned(OwnedCharacterChunkIter::new(
                vec, config,
            ))),
            VectorData::Lazy(span) => {
                let source = source.ok_or_else(|| {
                    Error::InvalidFormat("lazy vector requires input source".to_string())
                })?;
                Ok(CharacterChunkIter::Lazy(LazyCharacterChunkIter::new(
                    source, *span, config,
                )?))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_lazy_fixed_range<T: ChunkElement>(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<T>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| Error::InvalidFormat("lazy range overflow".to_string()))?;
    if end > span.length {
        return Err(Error::InvalidFormat(
            "lazy range exceeds vector length".to_string(),
        ));
    }

    let offset_bytes = span
        .offset
        .checked_add((start * T::BYTES) as u64)
        .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))?;
    let total_bytes = count
        .checked_mul(T::BYTES)
        .ok_or_else(|| Error::InvalidFormat("lazy range overflow".to_string()))?;
    let bytes = source.read_at(offset_bytes, total_bytes)?;
    if bytes.len() != total_bytes {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: (start * T::BYTES + bytes.len()) as u64,
        });
    }

    let mut cursor = Cursor::new(bytes.as_slice());
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(T::read_from(&mut cursor)?);
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_integer_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<i32>> {
    read_lazy_fixed_range::<i32>(source, span, start, count)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_real_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<f64>> {
    read_lazy_fixed_range::<f64>(source, span, start, count)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_logical_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Logical>> {
    read_lazy_fixed_range::<Logical>(source, span, start, count)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_raw_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<u8>> {
    read_lazy_fixed_range::<u8>(source, span, start, count)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_complex_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Complex>> {
    read_lazy_fixed_range::<Complex>(source, span, start, count)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_lazy_character_range(
    source: &dyn RdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Option<Arc<str>>>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| Error::InvalidFormat("lazy range overflow".to_string()))?;
    if end > span.length {
        return Err(Error::InvalidFormat(
            "lazy range exceeds vector length".to_string(),
        ));
    }

    let mut iter = LazyCharacterChunkIter::new(source, span, ChunkConfig::default())?;
    let mut skipped = 0usize;
    let mut out = Vec::with_capacity(count);

    while out.len() < count {
        let chunk = match iter.next() {
            Some(Ok(chunk)) => chunk,
            Some(Err(err)) => return Err(err),
            None => {
                return Err(Error::InvalidFormat(
                    "lazy character range exceeded available data".to_string(),
                ))
            }
        };

        if skipped < start {
            if skipped + chunk.len() <= start {
                skipped += chunk.len();
                continue;
            }
            let offset = start - skipped;
            let take = (chunk.len() - offset).min(count - out.len());
            out.extend(chunk[offset..offset + take].iter().cloned());
            skipped = start;
        } else {
            let take = chunk.len().min(count - out.len());
            out.extend(chunk[..take].iter().cloned());
        }
    }

    Ok(out)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    struct TestInput {
        data: Vec<u8>,
    }

    impl RdsInput for TestInput {
        fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
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
        }

        fn len(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
    }

    #[test]
    fn iter_chunks_lazy_integer_reads_in_chunks() {
        let mut data = Vec::new();
        for value in [1i32, 2, 3, 4] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestInput { data };
        let span = LazyVector {
            length: 4,
            offset: 0,
            byte_len: 16,
        };
        let vector: VectorData<i32> = VectorData::Lazy(span);
        let config = ChunkConfig {
            max_elements: 2,
            max_bytes: 1024,
        };
        let mut iter = vector.iter_chunks(Some(&input), config).unwrap();
        let first = iter.next().unwrap().unwrap();
        let second = iter.next().unwrap().unwrap();
        assert_eq!(first, vec![1, 2]);
        assert_eq!(second, vec![3, 4]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_chunks_owned_character_respects_byte_cap() {
        let values: Vec<Option<Arc<str>>> = vec![
            Some(Arc::from("alpha")),
            Some(Arc::from("beta")),
            Some(Arc::from("g")),
        ];
        let vector: VectorData<Option<Arc<str>>> = VectorData::Owned(values);
        let config = ChunkConfig {
            max_elements: 10,
            max_bytes: 7,
        };
        let mut iter = vector.iter_chunks(None, config).unwrap();
        let first = iter.next().unwrap().unwrap();
        let second = iter.next().unwrap().unwrap();
        assert_eq!(first, vec![Some(Arc::from("alpha"))]);
        assert_eq!(second, vec![Some(Arc::from("beta")), Some(Arc::from("g"))]);
    }

    #[test]
    fn read_lazy_integer_range_reads_slice() {
        let mut data = Vec::new();
        for value in [1i32, 2, 3, 4] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestInput { data };
        let span = LazyVector {
            length: 4,
            offset: 0,
            byte_len: 16,
        };
        let range = read_lazy_integer_range(&input, span, 1, 2).unwrap();
        assert_eq!(range, vec![2, 3]);
    }

    #[test]
    fn read_lazy_integer_range_rejects_out_of_bounds() {
        let mut data = Vec::new();
        for value in [1i32, 2, 3, 4] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestInput { data };
        let span = LazyVector {
            length: 4,
            offset: 0,
            byte_len: 16,
        };
        let result = read_lazy_integer_range(&input, span, 3, 2);
        assert!(result.is_err());
    }
}
