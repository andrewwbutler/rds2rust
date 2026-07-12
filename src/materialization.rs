use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;
use std::sync::Arc;

use crate::{Complex, Error, LazyVector, Logical, RObject, Result, VectorData};

/// Adapts an in-memory decompressed stream to the `RdsInput` trait so the
/// lazy character range reader (`chunk_iter::read_lazy_character_range`) can
/// decode variable-length CHARSXP spans. Character spans record absolute
/// offsets into this same buffer.
#[cfg(not(target_arch = "wasm32"))]
struct SliceRdsInput<'a>(&'a [u8]);

#[cfg(not(target_arch = "wasm32"))]
impl crate::RdsInput for SliceRdsInput<'_> {
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = offset as usize;
        if start > self.0.len() {
            return Ok(Vec::new());
        }
        let end = start.saturating_add(len).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }

    fn len(&self) -> Option<u64> {
        Some(self.0.len() as u64)
    }
}

#[derive(Debug, PartialEq)]
enum PathToken {
    Field(String),
    Index(usize),
}

pub struct MaterializationContext<'a> {
    data: &'a [u8],
    remaining_budget: Option<usize>,
}

impl<'a> MaterializationContext<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            remaining_budget: None,
        }
    }

    pub fn with_budget(data: &'a [u8], budget_bytes: usize) -> Self {
        Self {
            data,
            remaining_budget: Some(budget_bytes),
        }
    }

    pub fn remaining_budget(&self) -> Option<usize> {
        self.remaining_budget
    }

    fn check_budget(&mut self, bytes_needed: usize) -> Result<()> {
        if let Some(remaining) = &mut self.remaining_budget {
            if bytes_needed > *remaining {
                return Err(Error::MemoryBudgetExceeded {
                    needed: bytes_needed,
                    available: *remaining,
                });
            }
            *remaining -= bytes_needed;
        }
        Ok(())
    }

    pub fn materialize_integer_vector(&mut self, span: LazyVector) -> Result<Vec<i32>> {
        validate_byte_len(span, std::mem::size_of::<i32>())?;
        self.check_budget(span.byte_len as usize)?;
        let mut cursor = Cursor::new(slice_for_span(self.data, span)?);
        let mut vec = Vec::with_capacity(span.length);
        for _ in 0..span.length {
            vec.push(cursor.read_i32::<BigEndian>()?);
        }
        Ok(vec)
    }

    pub fn materialize_real_vector(&mut self, span: LazyVector) -> Result<Vec<f64>> {
        validate_byte_len(span, std::mem::size_of::<f64>())?;
        self.check_budget(span.byte_len as usize)?;
        let mut cursor = Cursor::new(slice_for_span(self.data, span)?);
        let mut vec = Vec::with_capacity(span.length);
        for _ in 0..span.length {
            vec.push(cursor.read_f64::<BigEndian>()?);
        }
        Ok(vec)
    }

    pub fn materialize_logical_vector(&mut self, span: LazyVector) -> Result<Vec<Logical>> {
        validate_byte_len(span, std::mem::size_of::<i32>())?;
        self.check_budget(span.byte_len as usize)?;
        let mut cursor = Cursor::new(slice_for_span(self.data, span)?);
        let mut vec = Vec::with_capacity(span.length);
        for _ in 0..span.length {
            let val = cursor.read_i32::<BigEndian>()?;
            let logical = match val {
                0 => Logical::False,
                1 => Logical::True,
                i32::MIN => Logical::Na,
                _ => Logical::Na,
            };
            vec.push(logical);
        }
        Ok(vec)
    }

    pub fn materialize_raw_vector(&mut self, span: LazyVector) -> Result<Vec<u8>> {
        validate_byte_len(span, 1)?;
        self.check_budget(span.byte_len as usize)?;
        let slice = slice_for_span(self.data, span)?;
        Ok(slice.to_vec())
    }

    pub fn materialize_complex_vector(&mut self, span: LazyVector) -> Result<Vec<Complex>> {
        validate_byte_len(span, std::mem::size_of::<Complex>())?;
        self.check_budget(span.byte_len as usize)?;
        let mut cursor = Cursor::new(slice_for_span(self.data, span)?);
        let mut vec = Vec::with_capacity(span.length);
        for _ in 0..span.length {
            let real = cursor.read_f64::<BigEndian>()?;
            let imaginary = cursor.read_f64::<BigEndian>()?;
            vec.push(Complex { real, imaginary });
        }
        Ok(vec)
    }

    /// Materialize a lazy character span. Unlike the numeric materializers,
    /// character spans are variable-length (CHARSXP entries + intra-vector
    /// REFSXP dedup), so this delegates to the shared range decoder rather
    /// than doing fixed-stride element math. `None` elements are
    /// `NA_character_`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn materialize_character_vector(
        &mut self,
        span: LazyVector,
    ) -> Result<Vec<Option<Arc<str>>>> {
        self.check_budget(span.byte_len as usize)?;
        let input = SliceRdsInput(self.data);
        crate::chunk_iter::read_lazy_character_range(&input, span, 0, span.length)
    }

    pub fn materialize_integer_data(&mut self, vector: &mut VectorData<i32>) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_integer_vector(span)?);
        }
        Ok(())
    }

    pub fn materialize_real_data(&mut self, vector: &mut VectorData<f64>) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_real_vector(span)?);
        }
        Ok(())
    }

    pub fn materialize_logical_data(&mut self, vector: &mut VectorData<Logical>) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_logical_vector(span)?);
        }
        Ok(())
    }

    pub fn materialize_raw_data(&mut self, vector: &mut VectorData<u8>) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_raw_vector(span)?);
        }
        Ok(())
    }

    pub fn materialize_complex_data(&mut self, vector: &mut VectorData<Complex>) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_complex_vector(span)?);
        }
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn materialize_character_data(
        &mut self,
        vector: &mut VectorData<Option<Arc<str>>>,
    ) -> Result<()> {
        if let VectorData::Lazy(span) = *vector {
            *vector = VectorData::Owned(self.materialize_character_vector(span)?);
        }
        Ok(())
    }
}

pub fn materialize_path(
    obj: &mut RObject,
    path: &str,
    ctx: &mut MaterializationContext<'_>,
) -> Result<bool> {
    let tokens = parse_path_tokens(path)?;
    materialize_tokens(obj, &tokens, ctx)
}

pub fn materialize_paths_with_budget(
    obj: &mut RObject,
    data: &[u8],
    paths: &[&str],
    budget_bytes: Option<usize>,
) -> Result<Vec<String>> {
    let mut ctx = match budget_bytes {
        Some(budget) => MaterializationContext::with_budget(data, budget),
        None => MaterializationContext::new(data),
    };

    let mut missing = Vec::new();
    for path in paths {
        let changed = materialize_path(obj, path, &mut ctx)?;
        if !changed {
            missing.push((*path).to_string());
        }
    }

    Ok(missing)
}

fn slice_for_span(data: &[u8], span: LazyVector) -> Result<&[u8]> {
    let start = span.offset as usize;
    let end = span
        .offset
        .checked_add(span.byte_len)
        .ok_or_else(|| Error::InvalidFormat("lazy span overflow".to_string()))?
        as usize;

    if start > data.len() {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: 0,
        });
    }

    let available = data.len() - start;
    if end > data.len() {
        return Err(Error::TruncatedLazyPayload {
            expected: span.byte_len,
            actual: available as u64,
        });
    }

    Ok(&data[start..end])
}

fn validate_byte_len(span: LazyVector, elem_size: usize) -> Result<()> {
    let expected = span
        .length
        .checked_mul(elem_size)
        .ok_or_else(|| Error::InvalidFormat("lazy span length overflow".to_string()))?;
    if span.byte_len != expected as u64 {
        return Err(Error::InvalidFormat(format!(
            "lazy span byte_len mismatch: expected {}, got {}",
            expected, span.byte_len
        )));
    }
    Ok(())
}

fn parse_path_tokens(path: &str) -> Result<Vec<PathToken>> {
    let mut tokens = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
            }
            b'[' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if start == i || i >= bytes.len() || bytes[i] != b']' {
                    return Err(Error::InvalidFormat(format!(
                        "invalid path index in '{}'",
                        path
                    )));
                }
                let index: usize = path[start..i]
                    .parse()
                    .map_err(|_| Error::InvalidFormat(format!("invalid index in '{}'", path)))?;
                tokens.push(PathToken::Index(index));
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                let field = path[start..i].to_string();
                if field.is_empty() {
                    return Err(Error::InvalidFormat(format!("invalid path '{}'", path)));
                }
                tokens.push(PathToken::Field(field));
            }
        }
    }

    Ok(tokens)
}

fn materialize_tokens(
    obj: &mut RObject,
    tokens: &[PathToken],
    ctx: &mut MaterializationContext<'_>,
) -> Result<bool> {
    use RObject::*;

    if tokens.is_empty() {
        return materialize_vector(obj, ctx);
    }

    match &tokens[0] {
        PathToken::Field(name) => match obj {
            DataFrame(df) => match df.columns.get_mut(name.as_str()) {
                Some(col) => materialize_tokens(col, &tokens[1..], ctx),
                None => Ok(false),
            },
            S4Object(s4) => match s4.slots.get_mut(name.as_str()) {
                Some(slot) => materialize_tokens(slot, &tokens[1..], ctx),
                None => Ok(false),
            },
            S3Object(s3) => {
                if name == "base" {
                    materialize_tokens(&mut s3.base, &tokens[1..], ctx)
                } else {
                    Ok(false)
                }
            }
            Closure {
                formals,
                body,
                environment,
            } => match name.as_str() {
                "formals" => materialize_tokens(formals, &tokens[1..], ctx),
                "body" => materialize_tokens(body, &tokens[1..], ctx),
                "environment" => materialize_tokens(environment, &tokens[1..], ctx),
                _ => Ok(false),
            },
            Environment {
                enclosing,
                frame,
                hashtab,
            } => match name.as_str() {
                "enclosing" => materialize_tokens(enclosing, &tokens[1..], ctx),
                "frame" => materialize_tokens(frame, &tokens[1..], ctx),
                "hashtab" => materialize_tokens(hashtab, &tokens[1..], ctx),
                _ => Ok(false),
            },
            Promise {
                value,
                expression,
                environment,
            } => match name.as_str() {
                "value" => materialize_tokens(value, &tokens[1..], ctx),
                "expression" => materialize_tokens(expression, &tokens[1..], ctx),
                "environment" => materialize_tokens(environment, &tokens[1..], ctx),
                _ => Ok(false),
            },
            Bytecode {
                code,
                constants,
                expr,
            } => match name.as_str() {
                "code" => materialize_tokens(code, &tokens[1..], ctx),
                "constants" => materialize_tokens(constants, &tokens[1..], ctx),
                "expr" => materialize_tokens(expr, &tokens[1..], ctx),
                _ => Ok(false),
            },
            Language { function, args } => match name.as_str() {
                "function" => materialize_tokens(function, &tokens[1..], ctx),
                "args" => materialize_pairlist_elements(args, &tokens[1..], ctx),
                _ => Ok(false),
            },
            Pairlist(_) => Ok(false),
            WithAttributes { object, .. } => materialize_tokens(object, tokens, ctx),
            Shared(inner) => {
                let mut inner = inner.write().unwrap();
                materialize_tokens(&mut inner, tokens, ctx)
            }
            _ => Ok(false),
        },
        PathToken::Index(index) => match obj {
            List(items) | Expression(items) => match items.get_mut(*index) {
                Some(item) => materialize_tokens(item, &tokens[1..], ctx),
                None => Ok(false),
            },
            Pairlist(elements) => materialize_pairlist_index(elements, *index, &tokens[1..], ctx),
            _ => Ok(false),
        },
    }
}

fn materialize_pairlist_elements(
    elements: &mut [crate::PairlistElement],
    tokens: &[PathToken],
    ctx: &mut MaterializationContext<'_>,
) -> Result<bool> {
    if tokens.is_empty() {
        return Ok(false);
    }
    match &tokens[0] {
        PathToken::Index(index) => materialize_pairlist_index(elements, *index, &tokens[1..], ctx),
        _ => Ok(false),
    }
}

fn materialize_pairlist_index(
    elements: &mut [crate::PairlistElement],
    index: usize,
    tokens: &[PathToken],
    ctx: &mut MaterializationContext<'_>,
) -> Result<bool> {
    let elem = match elements.get_mut(index) {
        Some(elem) => elem,
        None => return Ok(false),
    };

    if tokens.is_empty() {
        return Ok(false);
    }

    match &tokens[0] {
        PathToken::Field(name) => match name.as_str() {
            "value" => materialize_tokens(&mut elem.value, &tokens[1..], ctx),
            "tag_object" => match elem.tag_object.as_mut() {
                Some(tag) => materialize_tokens(tag, &tokens[1..], ctx),
                None => Ok(false),
            },
            _ => Ok(false),
        },
        _ => Ok(false),
    }
}

fn materialize_vector(obj: &mut RObject, ctx: &mut MaterializationContext<'_>) -> Result<bool> {
    use RObject::*;

    match obj {
        Integer(v) => {
            ctx.materialize_integer_data(v)?;
            Ok(true)
        }
        Real(v) => {
            ctx.materialize_real_data(v)?;
            Ok(true)
        }
        Logical(v) => {
            ctx.materialize_logical_data(v)?;
            Ok(true)
        }
        Raw(v) => {
            ctx.materialize_raw_data(v)?;
            Ok(true)
        }
        Complex(v) => {
            ctx.materialize_complex_data(v)?;
            Ok(true)
        }
        Character(v) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                ctx.materialize_character_data(v)?;
                Ok(true)
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = v;
                Err(Error::Unsupported(
                    "materialize character vectors is native-only".to_string(),
                ))
            }
        }
        _ => Ok(false),
    }
}

pub fn materialize_integer_vector(data: &[u8], span: LazyVector) -> Result<Vec<i32>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_integer_vector(span)
}

pub fn materialize_real_vector(data: &[u8], span: LazyVector) -> Result<Vec<f64>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_real_vector(span)
}

pub fn materialize_logical_vector(data: &[u8], span: LazyVector) -> Result<Vec<Logical>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_logical_vector(span)
}

pub fn materialize_raw_vector(data: &[u8], span: LazyVector) -> Result<Vec<u8>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_raw_vector(span)
}

pub fn materialize_complex_vector(data: &[u8], span: LazyVector) -> Result<Vec<Complex>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_complex_vector(span)
}

pub fn materialize_integer_data(data: &[u8], vector: &mut VectorData<i32>) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_integer_data(vector)
}

pub fn materialize_real_data(data: &[u8], vector: &mut VectorData<f64>) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_real_data(vector)
}

pub fn materialize_logical_data(data: &[u8], vector: &mut VectorData<Logical>) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_logical_data(vector)
}

pub fn materialize_raw_data(data: &[u8], vector: &mut VectorData<u8>) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_raw_data(vector)
}

pub fn materialize_complex_data(data: &[u8], vector: &mut VectorData<Complex>) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_complex_data(vector)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn materialize_character_vector(
    data: &[u8],
    span: LazyVector,
) -> Result<Vec<Option<Arc<str>>>> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_character_vector(span)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn materialize_character_data(
    data: &[u8],
    vector: &mut VectorData<Option<Arc<str>>>,
) -> Result<()> {
    let mut ctx = MaterializationContext::new(data);
    ctx.materialize_character_data(vector)
}
