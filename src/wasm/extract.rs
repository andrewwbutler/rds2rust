#[cfg(target_arch = "wasm32")]
use std::io::Read;

#[cfg(target_arch = "wasm32")]
use byteorder::{BigEndian, ReadBytesExt};
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Float64Array, Function, Int32Array, Uint8Array};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
use crate::constants::{CHARSXP, REFSXP};
#[cfg(target_arch = "wasm32")]
use crate::extraction::find_vector_at_path;
#[cfg(target_arch = "wasm32")]
use crate::extraction::VectorTarget;
#[cfg(target_arch = "wasm32")]
use crate::wasm::{AsyncChunkConfig, AsyncLazyCharacterChunkIter, AsyncRdsInput};
#[cfg(target_arch = "wasm32")]
use crate::{Complex, Error, LazyVector, Logical, RObject, Result};

#[cfg(target_arch = "wasm32")]
pub async fn extract_vector_to_js(
    obj: &RObject,
    input: &dyn AsyncRdsInput,
    path: &str,
) -> Result<JsValue> {
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing vector at path '{}'",
            path
        )));
    };

    match target {
        VectorTarget::Integer(vec) => Ok(Int32Array::from(vec.as_ref()).into()),
        VectorTarget::Real(vec) => Ok(Float64Array::from(vec.as_ref()).into()),
        VectorTarget::Logical(vec) => {
            let data: Vec<i32> = vec
                .as_ref()
                .iter()
                .map(|val| match val {
                    Logical::False => 0,
                    Logical::True => 1,
                    Logical::Na => i32::MIN,
                })
                .collect();
            Ok(Int32Array::from(data.as_slice()).into())
        }
        VectorTarget::Raw(vec) => Ok(Uint8Array::from(vec.as_ref()).into()),
        VectorTarget::Complex(vec) => {
            let mut data = Vec::with_capacity(vec.len() * 2);
            for val in vec.as_ref() {
                data.push(val.real);
                data.push(val.imaginary);
            }
            Ok(Float64Array::from(data.as_slice()).into())
        }
        VectorTarget::Character(vec) => Ok(strings_to_js(vec.as_ref())),
        VectorTarget::LazyInteger(span) => {
            let bytes = read_span_bytes(input, span).await?;
            let vec = parse_i32_vec(&bytes, span.length)?;
            Ok(Int32Array::from(vec.as_slice()).into())
        }
        VectorTarget::LazyReal(span) => {
            let bytes = read_span_bytes(input, span).await?;
            let vec = parse_f64_vec(&bytes, span.length)?;
            Ok(Float64Array::from(vec.as_slice()).into())
        }
        VectorTarget::LazyLogical(span) => {
            let bytes = read_span_bytes(input, span).await?;
            let vec = parse_i32_vec(&bytes, span.length)?;
            Ok(Int32Array::from(vec.as_slice()).into())
        }
        VectorTarget::LazyRaw(span) => {
            let bytes = read_span_bytes(input, span).await?;
            Ok(Uint8Array::from(bytes.as_slice()).into())
        }
        VectorTarget::LazyComplex(span) => {
            let bytes = read_span_bytes(input, span).await?;
            let vec = parse_complex_vec(&bytes, span.length)?;
            let mut data = Vec::with_capacity(vec.len() * 2);
            for val in &vec {
                data.push(val.real);
                data.push(val.imaginary);
            }
            Ok(Float64Array::from(data.as_slice()).into())
        }
        VectorTarget::LazyCharacter(span) => {
            let bytes = read_span_bytes(input, span).await?;
            let values = parse_character_vec(&bytes, span.length)?;
            Ok(strings_to_js(&values))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_character_vector(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
) -> Result<Vec<Option<std::sync::Arc<str>>>> {
    read_lazy_character_range_async(input, span, 0, span.length).await
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_character_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Option<std::sync::Arc<str>>>> {
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

    let mut iter = AsyncLazyCharacterChunkIter::new(input, span, AsyncChunkConfig::default())?;
    let mut skipped = 0usize;
    let mut out = Vec::with_capacity(count);

    while out.len() < count {
        let chunk = match iter.next_chunk().await? {
            Some(chunk) => chunk,
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

#[cfg(target_arch = "wasm32")]
pub async fn extract_vector_chunked(
    obj: &RObject,
    input: &dyn AsyncRdsInput,
    path: &str,
    chunk_size: usize,
    callback: &Function,
) -> Result<()> {
    let Some(target) = find_vector_at_path(obj, path)? else {
        return Err(Error::InvalidFormat(format!(
            "missing vector at path '{}'",
            path
        )));
    };

    match target {
        VectorTarget::Integer(vec) => {
            emit_i32_chunks(vec.as_ref(), chunk_size, callback)?;
        }
        VectorTarget::Real(vec) => {
            emit_f64_chunks(vec.as_ref(), chunk_size, callback)?;
        }
        VectorTarget::Logical(vec) => {
            let data: Vec<i32> = vec
                .as_ref()
                .iter()
                .map(|val| match val {
                    Logical::False => 0,
                    Logical::True => 1,
                    Logical::Na => i32::MIN,
                })
                .collect();
            emit_i32_chunks(&data, chunk_size, callback)?;
        }
        VectorTarget::Raw(vec) => {
            emit_u8_chunks(vec.as_ref(), chunk_size, callback)?;
        }
        VectorTarget::Complex(vec) => {
            emit_complex_chunks(vec.as_ref(), chunk_size, callback)?;
        }
        VectorTarget::Character(vec) => {
            emit_string_chunks(vec.as_ref(), chunk_size, callback)?;
        }
        VectorTarget::LazyInteger(span) => {
            stream_numeric_chunks(input, span, 4, chunk_size, callback, parse_i32_chunk).await?;
        }
        VectorTarget::LazyReal(span) => {
            stream_numeric_chunks(input, span, 8, chunk_size, callback, parse_f64_chunk).await?;
        }
        VectorTarget::LazyLogical(span) => {
            stream_numeric_chunks(input, span, 4, chunk_size, callback, parse_i32_chunk).await?;
        }
        VectorTarget::LazyRaw(span) => {
            stream_raw_chunks(input, span, chunk_size, callback).await?;
        }
        VectorTarget::LazyComplex(span) => {
            stream_complex_chunks(input, span, chunk_size, callback).await?;
        }
        VectorTarget::LazyCharacter(span) => {
            stream_character_chunks(input, span, chunk_size, callback).await?;
        }
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn strings_to_js(values: &[Option<std::sync::Arc<str>>]) -> JsValue {
    let arr = Array::new();
    for value in values {
        match value {
            Some(value) => arr.push(&JsValue::from_str(value.as_ref())),
            // NA_character_ surfaces as null on the JS boundary.
            None => arr.push(&JsValue::NULL),
        };
    }
    arr.into()
}

#[cfg(target_arch = "wasm32")]
async fn read_span_bytes(input: &dyn AsyncRdsInput, span: LazyVector) -> Result<Vec<u8>> {
    let mut remaining = span.byte_len as usize;
    let mut offset = span.offset;
    let mut out = Vec::with_capacity(remaining);
    while remaining > 0 {
        let to_read = remaining.min(4 * 1024 * 1024);
        let chunk = input.read_at(offset, to_read).await?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: span.byte_len,
                actual: out.len() as u64 + chunk.len() as u64,
            });
        }
        out.extend_from_slice(&chunk);
        remaining -= to_read;
        offset += to_read as u64;
    }
    Ok(out)
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_vector_range(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    elem_size: usize,
    start_index: usize,
    count: usize,
) -> Result<Vec<u8>> {
    if start_index > span.length {
        return Err(Error::InvalidFormat(format!(
            "lazy vector range start {} exceeds length {}",
            start_index, span.length
        )));
    }
    let max_count = span.length.saturating_sub(start_index);
    if count > max_count {
        return Err(Error::InvalidFormat(format!(
            "lazy vector range count {} exceeds available {}",
            count, max_count
        )));
    }
    if count == 0 {
        return Ok(Vec::new());
    }

    let byte_len = count
        .checked_mul(elem_size)
        .ok_or_else(|| Error::InvalidFormat("lazy vector byte length overflow".to_string()))?;
    let mut remaining = byte_len;
    let mut offset = span.offset + (start_index * elem_size) as u64;
    let mut out = Vec::with_capacity(byte_len);

    while remaining > 0 {
        let to_read = remaining.min(4 * 1024 * 1024);
        let chunk = input.read_at(offset, to_read).await?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: byte_len as u64,
                actual: out.len() as u64 + chunk.len() as u64,
            });
        }
        out.extend_from_slice(&chunk);
        remaining -= to_read;
        offset += to_read as u64;
    }

    Ok(out)
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_integer_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<i32>> {
    let bytes = read_lazy_vector_range(input, span, 4, start, count).await?;
    parse_i32_vec(&bytes, count)
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_real_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<f64>> {
    let bytes = read_lazy_vector_range(input, span, 8, start, count).await?;
    parse_f64_vec(&bytes, count)
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_logical_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Logical>> {
    let bytes = read_lazy_vector_range(input, span, 4, start, count).await?;
    let values = parse_i32_vec(&bytes, count)?;
    Ok(values.into_iter().map(Logical::from).collect())
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_raw_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<u8>> {
    read_lazy_vector_range(input, span, 1, start, count).await
}

#[cfg(target_arch = "wasm32")]
pub async fn read_lazy_complex_range_async(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    start: usize,
    count: usize,
) -> Result<Vec<Complex>> {
    let bytes = read_lazy_vector_range(input, span, 16, start, count).await?;
    parse_complex_vec(&bytes, count)
}

#[cfg(target_arch = "wasm32")]
fn parse_i32_vec(bytes: &[u8], length: usize) -> Result<Vec<i32>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        vec.push(cursor.read_i32::<BigEndian>()?);
    }
    Ok(vec)
}

#[cfg(target_arch = "wasm32")]
fn parse_f64_vec(bytes: &[u8], length: usize) -> Result<Vec<f64>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        vec.push(cursor.read_f64::<BigEndian>()?);
    }
    Ok(vec)
}

#[cfg(target_arch = "wasm32")]
fn parse_complex_vec(bytes: &[u8], length: usize) -> Result<Vec<Complex>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;
        vec.push(Complex { real, imaginary });
    }
    Ok(vec)
}

#[cfg(target_arch = "wasm32")]
fn parse_character_vec(bytes: &[u8], length: usize) -> Result<Vec<Option<std::sync::Arc<str>>>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut values = Vec::with_capacity(length);
    let mut cache: Vec<Option<std::sync::Arc<str>>> = Vec::new();

    for _ in 0..length {
        let flags = cursor.read_u32::<BigEndian>()?;
        let type_from_0_7 = flags & 0xFF;
        let type_from_8_15 = (flags >> 8) & 0xFF;

        if type_from_0_7 == REFSXP {
            let ref_index = (flags >> 8) as usize;
            // Wire REFSXP indices are 1-based (0 is invalid), matching the
            // native parser and chunk iterators.
            let value = ref_index
                .checked_sub(1)
                .and_then(|i| cache.get(i))
                .ok_or_else(|| {
                    Error::InvalidFormat(format!("invalid REFSXP index {}", ref_index))
                })?;
            values.push(value.clone());
            continue;
        }

        if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
            let value = parse_charsxp_content_from_cursor(&mut cursor, flags)?;
            cache.push(value.clone());
            values.push(value);
            continue;
        }

        return Err(Error::Unsupported(
            "non-CHARSXP element in character vector".to_string(),
        ));
    }

    Ok(values)
}

#[cfg(target_arch = "wasm32")]
fn parse_charsxp_content_from_cursor(
    cursor: &mut std::io::Cursor<&[u8]>,
    flags: u32,
) -> Result<Option<std::sync::Arc<str>>> {
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    let length = if use_compact {
        let mut bytes = [0u8; 3];
        cursor.read_exact(&mut bytes)?;
        ((bytes[0] as i32) << 16) | ((bytes[1] as i32) << 8) | (bytes[2] as i32)
    } else {
        cursor.read_i32::<BigEndian>()?
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

    let mut bytes = vec![0u8; length as usize];
    cursor.read_exact(&mut bytes)?;
    let string: String = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok(Some(std::sync::Arc::from(string.as_str())))
}

#[cfg(target_arch = "wasm32")]
fn emit_i32_chunks(data: &[i32], chunk_size: usize, callback: &Function) -> Result<()> {
    let elem_size = 4usize;
    let elems_per_chunk = (chunk_size / elem_size).max(1);
    let total = data.len();
    let mut offset = 0usize;

    while offset < total {
        let end = (offset + elems_per_chunk).min(total);
        let slice = &data[offset..end];
        let js_chunk = Int32Array::from(slice).into();
        let progress = JsValue::from_f64(end as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &js_chunk, &progress)
            .map_err(map_js_error)?;
        offset = end;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn emit_f64_chunks(data: &[f64], chunk_size: usize, callback: &Function) -> Result<()> {
    let elem_size = 8usize;
    let elems_per_chunk = (chunk_size / elem_size).max(1);
    let total = data.len();
    let mut offset = 0usize;

    while offset < total {
        let end = (offset + elems_per_chunk).min(total);
        let slice = &data[offset..end];
        let js_chunk = Float64Array::from(slice).into();
        let progress = JsValue::from_f64(end as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &js_chunk, &progress)
            .map_err(map_js_error)?;
        offset = end;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn emit_u8_chunks(data: &[u8], chunk_size: usize, callback: &Function) -> Result<()> {
    let elems_per_chunk = chunk_size.max(1);
    let total = data.len();
    let mut offset = 0usize;

    while offset < total {
        let end = (offset + elems_per_chunk).min(total);
        let slice = &data[offset..end];
        let js_chunk = Uint8Array::from(slice).into();
        let progress = JsValue::from_f64(end as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &js_chunk, &progress)
            .map_err(map_js_error)?;
        offset = end;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn emit_complex_chunks(data: &[Complex], chunk_size: usize, callback: &Function) -> Result<()> {
    let elem_size = 16usize;
    let elems_per_chunk = (chunk_size / elem_size).max(1);
    let total = data.len();
    let mut offset = 0usize;

    while offset < total {
        let end = (offset + elems_per_chunk).min(total);
        let slice = &data[offset..end];
        let mut packed = Vec::with_capacity(slice.len() * 2);
        for value in slice {
            packed.push(value.real);
            packed.push(value.imaginary);
        }
        let js_chunk = Float64Array::from(packed.as_slice()).into();
        let progress = JsValue::from_f64(end as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &js_chunk, &progress)
            .map_err(map_js_error)?;
        offset = end;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn emit_string_chunks(
    data: &[Option<std::sync::Arc<str>>],
    chunk_size: usize,
    callback: &Function,
) -> Result<()> {
    let bytes_per_chunk = chunk_size.max(1);
    let mut offset = 0usize;
    let total = data.len();

    while offset < total {
        let arr = Array::new();
        let mut bytes = 0usize;
        let mut end = offset;
        while end < total {
            let value = data[end].as_deref();
            let next = bytes + value.map_or(0, str::len);
            if end > offset && next > bytes_per_chunk {
                break;
            }
            bytes = next;
            match value {
                Some(value) => arr.push(&JsValue::from_str(value)),
                // NA_character_ surfaces as null on the JS boundary.
                None => arr.push(&JsValue::NULL),
            };
            end += 1;
        }
        let progress = JsValue::from_f64(end as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &arr.into(), &progress)
            .map_err(map_js_error)?;
        offset = end;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn stream_numeric_chunks<T>(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    elem_size: usize,
    chunk_size: usize,
    callback: &Function,
    parse: fn(&[u8]) -> Result<T>,
) -> Result<()>
where
    T: Into<JsValue>,
{
    let total_bytes = span.byte_len as usize;
    let mut remaining = total_bytes;
    let mut offset = span.offset;
    let mut carry: Vec<u8> = Vec::new();
    let mut processed = 0usize;

    while remaining > 0 {
        let to_read = remaining.min(chunk_size.max(elem_size));
        let chunk = input.read_at(offset, to_read).await?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: span.byte_len,
                actual: processed as u64 + chunk.len() as u64,
            });
        }
        let mut buf = Vec::with_capacity(carry.len() + chunk.len());
        buf.extend_from_slice(&carry);
        buf.extend_from_slice(&chunk);

        let remainder = buf.len() % elem_size;
        let split = buf.len() - remainder;
        carry = buf[split..].to_vec();

        if split > 0 {
            let parsed = parse(&buf[..split])?;
            let progress = JsValue::from_f64((processed + split) as f64 / total_bytes as f64);
            callback
                .call2(&JsValue::NULL, &parsed.into(), &progress)
                .map_err(map_js_error)?;
        }

        processed += to_read;
        remaining -= to_read;
        offset += to_read as u64;
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn stream_raw_chunks(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    chunk_size: usize,
    callback: &Function,
) -> Result<()> {
    let total_bytes = span.byte_len as usize;
    let mut remaining = total_bytes;
    let mut offset = span.offset;
    let mut processed = 0usize;

    while remaining > 0 {
        let to_read = remaining.min(chunk_size.max(1));
        let chunk = input.read_at(offset, to_read).await?;
        if chunk.len() != to_read {
            return Err(Error::TruncatedLazyPayload {
                expected: span.byte_len,
                actual: processed as u64 + chunk.len() as u64,
            });
        }
        let js_chunk = Uint8Array::from(chunk.as_slice()).into();
        let progress = JsValue::from_f64((processed + to_read) as f64 / total_bytes as f64);
        callback
            .call2(&JsValue::NULL, &js_chunk, &progress)
            .map_err(map_js_error)?;
        processed += to_read;
        remaining -= to_read;
        offset += to_read as u64;
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn stream_complex_chunks(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    chunk_size: usize,
    callback: &Function,
) -> Result<()> {
    stream_numeric_chunks(input, span, 16, chunk_size, callback, parse_complex_chunk).await
}

#[cfg(target_arch = "wasm32")]
async fn stream_character_chunks(
    input: &dyn AsyncRdsInput,
    span: LazyVector,
    chunk_size: usize,
    callback: &Function,
) -> Result<()> {
    let mut iter = AsyncLazyCharacterChunkIter::new(
        input,
        span,
        AsyncChunkConfig {
            max_elements: usize::MAX,
            max_bytes: chunk_size.max(1),
        },
    )?;
    let total = span.length;
    let mut processed = 0usize;
    while let Some(chunk) = iter.next_chunk().await? {
        processed += chunk.len();
        let arr = Array::new();
        for value in &chunk {
            match value {
                Some(value) => arr.push(&JsValue::from_str(value.as_ref())),
                // NA_character_ surfaces as null on the JS boundary.
                None => arr.push(&JsValue::NULL),
            };
        }
        let progress = JsValue::from_f64(processed as f64 / total as f64);
        callback
            .call2(&JsValue::NULL, &arr.into(), &progress)
            .map_err(map_js_error)?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn parse_i32_chunk(bytes: &[u8]) -> Result<JsValue> {
    let count = bytes.len() / 4;
    let mut cursor = std::io::Cursor::new(bytes);
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(cursor.read_i32::<BigEndian>()?);
    }
    Ok(Int32Array::from(values.as_slice()).into())
}

#[cfg(target_arch = "wasm32")]
fn parse_f64_chunk(bytes: &[u8]) -> Result<JsValue> {
    let count = bytes.len() / 8;
    let mut cursor = std::io::Cursor::new(bytes);
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(cursor.read_f64::<BigEndian>()?);
    }
    Ok(Float64Array::from(values.as_slice()).into())
}

#[cfg(target_arch = "wasm32")]
fn parse_complex_chunk(bytes: &[u8]) -> Result<JsValue> {
    let count = bytes.len() / 16;
    let mut cursor = std::io::Cursor::new(bytes);
    let mut packed = Vec::with_capacity(count * 2);
    for _ in 0..count {
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;
        packed.push(real);
        packed.push(imaginary);
    }
    Ok(Float64Array::from(packed.as_slice()).into())
}

#[cfg(target_arch = "wasm32")]
fn map_js_error(err: JsValue) -> Error {
    Error::Unsupported(format!("wasm callback error: {:?}", err))
}
