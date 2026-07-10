use std::collections::HashMap;
use std::sync::Arc;

use crate::extraction::VectorKind;
use crate::parser;
#[cfg(not(target_arch = "wasm32"))]
use crate::RdsInput;
#[cfg(not(target_arch = "wasm32"))]
use crate::Result;
#[cfg(target_arch = "wasm32")]
use crate::{AsyncCursorConfig, AsyncRdsInput};
use crate::{Attributes, Error, LazyVector, ObjectPath, ParseConfig};

#[cfg(test)]
use crate::RObject;
#[cfg(test)]
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitAction {
    Continue,
    Skip,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkAction {
    Skip,
    StreamAll,
}

#[derive(Debug)]
pub enum StreamingError<E> {
    Parse(Error),
    Visitor(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingProgress {
    /// Bytes consumed from the input cursor so far.
    pub bytes_read: u64,
    /// Total bytes in the input stream when known.
    pub total_bytes: Option<u64>,
    /// Count of objects fully visited.
    pub objects_visited: usize,
}

impl<E> From<Error> for StreamingError<E> {
    fn from(err: Error) -> Self {
        StreamingError::Parse(err)
    }
}

impl<E: std::fmt::Display> std::fmt::Display for StreamingError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamingError::Parse(err) => write!(f, "parse error: {}", err),
            StreamingError::Visitor(err) => write!(f, "visitor error: {}", err),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for StreamingError<E> {}

pub type StreamingResult<T, E> = std::result::Result<T, StreamingError<E>>;

pub trait RdsVisitor {
    type Error;

    fn on_header(&mut self, _format_version: u32) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn on_object_start(
        &mut self,
        path: &ObjectPath,
        obj_type: &str,
    ) -> std::result::Result<VisitAction, Self::Error>;

    /// Called when a REFSXP is encountered.
    ///
    /// `target` is the object path for the referenced object when known.
    fn on_shared_reference(
        &mut self,
        _path: &ObjectPath,
        _target: Option<&ObjectPath>,
    ) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn on_vector_metadata(
        &mut self,
        _path: &ObjectPath,
        _vec_type: VectorKind,
        _len: usize,
    ) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn on_vector_chunk_available(
        &mut self,
        _path: &ObjectPath,
        _span: LazyVector,
    ) -> std::result::Result<ChunkAction, Self::Error> {
        Ok(ChunkAction::Skip)
    }

    fn on_attributes(
        &mut self,
        _path: &ObjectPath,
        _attrs: &Attributes,
    ) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn on_object_end(&mut self, _path: &ObjectPath) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DatasetInfo {
    pub version: Option<u32>,
    pub object_count: usize,
    pub vectors: Vec<VectorMetadata>,
    pub dataframes: Vec<DataFrameMetadata>,
    pub s4_objects: Vec<S4Metadata>,
    pub warnings: Vec<MetadataWarning>,
    pub estimated_memory_bytes: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct VectorMetadata {
    pub path: ObjectPath,
    pub kind: VectorKind,
    pub length: usize,
    pub dimensions: Option<Vec<usize>>,
    pub has_names: bool,
}

#[derive(Debug, Clone)]
pub struct DataFrameMetadata {
    pub path: ObjectPath,
    pub num_rows: usize,
    pub num_cols: usize,
    pub column_names: Vec<Arc<str>>,
    pub column_types: Vec<VectorKind>,
}

#[derive(Debug, Clone)]
pub struct S4Metadata {
    pub path: ObjectPath,
    pub class: Vec<Arc<str>>,
    pub slot_names: Vec<Arc<str>>,
}

#[derive(Debug, Clone)]
pub enum MetadataWarning {
    PartialParse {
        path: ObjectPath,
        reason: String,
    },
    UnsupportedStructure {
        path: ObjectPath,
        structure: String,
    },
    MemoryBudgetExceeded {
        path: ObjectPath,
        limit_bytes: usize,
    },
    CachePressure {
        cache_size_bytes: usize,
        evicted_count: usize,
    },
    VectorLazy {
        path: ObjectPath,
        vector_type: String,
        length: usize,
        threshold: usize,
        byte_len: u64,
    },
}

#[allow(dead_code)]
struct Frame {
    path: ObjectPath,
    obj_type: String,
    class: Option<Vec<Arc<str>>>,
    dims: Option<Vec<usize>>,
    names: Option<Vec<Arc<str>>>,
    row_count: Option<usize>,
    child_count: usize,
    child_names: Vec<Arc<str>>,
    child_kinds: Vec<VectorKind>,
    slot_names: Vec<Arc<str>>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn inspect_metadata_streaming(
    input: &dyn crate::RdsInput,
    config: ParseConfig,
) -> Result<DatasetInfo> {
    let mut visitor = DatasetInfoVisitor::new();
    match crate::traverse_rds_streaming(input, config, &mut visitor) {
        Ok(()) => Ok(visitor.finish()),
        Err(crate::StreamingError::Parse(err)) => Err(err),
        Err(crate::StreamingError::Visitor(_)) => Err(crate::Error::Unsupported(
            "metadata visitor failed".to_string(),
        )),
    }
}

#[allow(dead_code)]
struct DatasetInfoVisitor {
    info: DatasetInfo,
    stack: Vec<Frame>,
    vectors_by_path: HashMap<ObjectPath, VectorMetadata>,
}

#[allow(dead_code)]
impl DatasetInfoVisitor {
    fn new() -> Self {
        Self {
            info: DatasetInfo {
                version: None,
                object_count: 0,
                vectors: Vec::new(),
                dataframes: Vec::new(),
                s4_objects: Vec::new(),
                warnings: Vec::new(),
                estimated_memory_bytes: Some(0),
            },
            stack: Vec::new(),
            vectors_by_path: HashMap::new(),
        }
    }

    fn finish(mut self) -> DatasetInfo {
        self.info.vectors = self.vectors_by_path.values().cloned().collect();
        self.info
    }

    fn current_frame_mut(&mut self) -> Option<&mut Frame> {
        self.stack.last_mut()
    }

    fn is_dataframe(frame: &Frame) -> bool {
        frame
            .class
            .as_ref()
            .map(|class| class.iter().any(|name| name.as_ref() == "data.frame"))
            .unwrap_or(false)
    }
}

impl RdsVisitor for DatasetInfoVisitor {
    type Error = std::convert::Infallible;

    fn on_header(&mut self, format_version: u32) -> std::result::Result<(), Self::Error> {
        self.info.version = Some(format_version);
        Ok(())
    }

    fn on_object_start(
        &mut self,
        path: &ObjectPath,
        obj_type: &str,
    ) -> std::result::Result<VisitAction, Self::Error> {
        self.info.object_count += 1;
        if matches!(obj_type, "Bytecode" | "Altrep" | "ExternalPtr" | "WeakRef") {
            self.info
                .warnings
                .push(MetadataWarning::UnsupportedStructure {
                    path: path.clone(),
                    structure: obj_type.to_string(),
                });
        }
        self.stack.push(Frame {
            path: path.clone(),
            obj_type: obj_type.to_string(),
            class: None,
            dims: None,
            names: None,
            row_count: None,
            child_count: 0,
            child_names: Vec::new(),
            child_kinds: Vec::new(),
            slot_names: Vec::new(),
        });
        Ok(VisitAction::Continue)
    }

    fn on_vector_metadata(
        &mut self,
        path: &ObjectPath,
        vec_type: VectorKind,
        len: usize,
    ) -> std::result::Result<(), Self::Error> {
        if self.stack.len() >= 2 {
            let parent_index = self.stack.len() - 2;
            let parent = &mut self.stack[parent_index];
            parent.child_count += 1;
            if let Some(segment) = path.segments.last() {
                if !segment.starts_with('[') {
                    parent.child_names.push(segment.clone());
                }
            }
            parent.child_kinds.push(vec_type);
        }

        let meta = VectorMetadata {
            path: path.clone(),
            kind: vec_type,
            length: len,
            dimensions: self
                .current_frame_mut()
                .and_then(|frame| frame.dims.clone()),
            has_names: self
                .current_frame_mut()
                .and_then(|frame| frame.names.as_ref().map(|names| !names.is_empty()))
                .unwrap_or(false),
        };
        self.vectors_by_path.insert(path.clone(), meta);

        if let Some(total) = self.info.estimated_memory_bytes.as_mut() {
            let elem_size = match vec_type {
                VectorKind::Integer => 4,
                VectorKind::Real => 8,
                VectorKind::Logical => 4,
                VectorKind::Raw => 1,
                VectorKind::Complex => 16,
                VectorKind::Character => 0,
            };
            if elem_size > 0 {
                let bytes = len.saturating_mul(elem_size);
                *total = total.saturating_add(bytes);
            }
        }
        Ok(())
    }

    fn on_attributes(
        &mut self,
        _path: &ObjectPath,
        attrs: &Attributes,
    ) -> std::result::Result<(), Self::Error> {
        let Some(frame) = self.current_frame_mut() else {
            return Ok(());
        };
        for (key, value) in attrs.iter() {
            let value = value.as_concrete();
            match key.as_ref() {
                "class" => match value {
                    crate::RObject::Character(names) => {
                        // NA class entries are treated as absent.
                        frame.class = Some(names.iter().flatten().cloned().collect());
                    }
                    crate::RObject::Symbol(name) => {
                        frame.class = Some(vec![name]);
                    }
                    crate::RObject::WithAttributes { object, .. } => match object.as_ref() {
                        crate::RObject::Character(names) => {
                            // NA class entries are treated as absent.
                            frame.class = Some(names.iter().flatten().cloned().collect());
                        }
                        crate::RObject::Symbol(name) => {
                            frame.class = Some(vec![name.clone()]);
                        }
                        _ => {}
                    },
                    _ => {}
                },
                "dim" => {
                    if let crate::RObject::Integer(values) = value {
                        frame.dims = Some(values.iter().map(|v| *v as usize).collect());
                    }
                }
                "names" => {
                    if let crate::RObject::Character(values) = value {
                        // Names are positional; an NA name keeps its slot,
                        // rendered with R's NA display convention.
                        frame.names = Some(
                            values
                                .iter()
                                .map(|v| v.clone().unwrap_or_else(|| Arc::from("<NA>")))
                                .collect(),
                        );
                    }
                }
                "row.names" => {
                    if let crate::RObject::Integer(values) = value {
                        if values.len() == 2 && values[0] == i32::MIN {
                            let count = values[1].unsigned_abs() as usize;
                            frame.row_count = Some(count);
                        }
                    } else if let crate::RObject::Character(values) = value {
                        frame.row_count = Some(values.len());
                    }
                }
                _ => {
                    if frame.obj_type == "S4Object"
                        && key.as_ref() != "package"
                        && key.as_ref() != "class"
                    {
                        frame.slot_names.push(key.clone());
                    }
                }
            }
        }

        Ok(())
    }

    fn on_object_end(&mut self, _path: &ObjectPath) -> std::result::Result<(), Self::Error> {
        let Some(frame) = self.stack.pop() else {
            return Ok(());
        };

        if Self::is_dataframe(&frame) {
            let num_cols = frame
                .child_count
                .max(frame.names.as_ref().map_or(0, |n| n.len()));
            let column_names = frame.names.unwrap_or(frame.child_names);
            let num_rows = frame
                .row_count
                .or_else(|| frame.dims.as_ref().and_then(|dims| dims.first().copied()))
                .unwrap_or(0);
            self.info.dataframes.push(DataFrameMetadata {
                path: frame.path.clone(),
                num_rows,
                num_cols,
                column_names,
                column_types: frame.child_kinds,
            });
        }

        if frame.obj_type == "S4Object" {
            let class = frame.class.unwrap_or_default();
            self.info.s4_objects.push(S4Metadata {
                path: frame.path,
                class,
                slot_names: frame.slot_names,
            });
        }

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn traverse_rds_streaming<V: RdsVisitor>(
    input: &dyn RdsInput,
    config: ParseConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    parser::traverse_rds_streaming_with_input(input, config, visitor)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn traverse_rds_streaming_with_progress<V: RdsVisitor>(
    input: &dyn RdsInput,
    config: ParseConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error> {
    parser::traverse_rds_streaming_with_input_progress(input, config, visitor, progress)
}

#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_async<V: RdsVisitor>(
    input: &dyn AsyncRdsInput,
    parse_config: ParseConfig,
    cursor_config: AsyncCursorConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    parser::traverse_rds_streaming_with_async_input(input, parse_config, cursor_config, visitor)
        .await
}

#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_async_with_progress<V: RdsVisitor>(
    input: &dyn AsyncRdsInput,
    parse_config: ParseConfig,
    cursor_config: AsyncCursorConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error> {
    parser::traverse_rds_streaming_with_async_input_progress(
        input,
        parse_config,
        cursor_config,
        visitor,
        progress,
    )
    .await
}

/// Traverse RDS data from a sequential input source (e.g., streaming decompression).
///
/// This is more memory-efficient than `traverse_rds_streaming_async` for compressed files
/// as it doesn't require buffering decompressed data for random access. The input must
/// support only forward reading (no seeking backwards).
///
/// # Example
///
/// ```ignore
/// use rds2rust::{StreamingGzipDecompressor, ParseConfig};
///
/// let blob = get_compressed_blob();
/// let mut decompressor = StreamingGzipDecompressor::new(blob).await?;
/// let mut visitor = MyVisitor::new();
///
/// traverse_rds_streaming_sequential_async(
///     &mut decompressor,
///     ParseConfig::default(),
///     &mut visitor
/// ).await?;
/// ```
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_sequential_async<I, V>(
    input: &mut I,
    parse_config: ParseConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error>
where
    I: crate::AsyncSequentialInput,
    V: RdsVisitor,
{
    use crate::parser::traverse_rds_streaming_with_sequential_input;

    traverse_rds_streaming_with_sequential_input(input, parse_config, visitor).await
}

/// Traverse RDS data from a sequential input source with progress reporting.
///
/// Similar to `traverse_rds_streaming_sequential_async` but provides progress callbacks.
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_sequential_async_with_progress<I, V>(
    input: &mut I,
    parse_config: ParseConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error>
where
    I: crate::AsyncSequentialInput,
    V: RdsVisitor,
{
    use crate::parser::traverse_rds_streaming_with_sequential_input_progress;

    traverse_rds_streaming_with_sequential_input_progress(input, parse_config, visitor, progress)
        .await
}

#[cfg(test)]
struct TraverseState {
    visited_shared: HashSet<usize>,
}

#[cfg(test)]
impl TraverseState {
    fn new() -> Self {
        Self {
            visited_shared: HashSet::new(),
        }
    }
}

#[cfg(test)]
enum TraverseControl {
    Continue,
    Stop,
}

#[cfg(test)]
fn traverse_object<V: RdsVisitor>(
    obj: &RObject,
    path: &mut ObjectPath,
    visitor: &mut V,
    state: &mut TraverseState,
) -> StreamingResult<TraverseControl, V::Error> {
    if let RObject::WithAttributes { object, attributes } = obj {
        if matches!(
            emit_attributes(attributes, path, visitor, state)?,
            TraverseControl::Stop
        ) {
            return Ok(TraverseControl::Stop);
        }
        return traverse_object(object, path, visitor, state);
    }

    if let RObject::Shared(inner) = obj {
        let key = Arc::as_ptr(inner) as usize;
        if !state.visited_shared.insert(key) {
            let action = visitor
                .on_object_start(path, "SharedRef")
                .map_err(StreamingError::Visitor)?;
            if action == VisitAction::Stop {
                return Ok(TraverseControl::Stop);
            }
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
            return Ok(TraverseControl::Continue);
        }
        let inner_obj = inner.read().map_err(|_| {
            StreamingError::Parse(Error::Unsupported(
                "shared object lock poisoned".to_string(),
            ))
        })?;
        return traverse_object(&inner_obj, path, visitor, state);
    }

    let obj_type = object_type_name(obj);
    match visitor
        .on_object_start(path, obj_type)
        .map_err(StreamingError::Visitor)?
    {
        VisitAction::Stop => return Ok(TraverseControl::Stop),
        VisitAction::Skip => {
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
            return Ok(TraverseControl::Continue);
        }
        VisitAction::Continue => {}
    }

    match obj {
        RObject::Null
        | RObject::Symbol(_)
        | RObject::Special { .. }
        | RObject::Builtin { .. }
        | RObject::GlobalEnv
        | RObject::BaseEnv
        | RObject::EmptyEnv
        | RObject::MissingArg
        | RObject::UnboundValue => {}
        RObject::Integer(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Integer, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::Real(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Real, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::Logical(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Logical, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::Character(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Character, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::Raw(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Raw, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::Complex(data) => {
            visitor
                .on_vector_metadata(path, VectorKind::Complex, data.len())
                .map_err(StreamingError::Visitor)?;
            if let Some(span) = data.lazy_span() {
                let _ = visitor
                    .on_vector_chunk_available(path, span)
                    .map_err(StreamingError::Visitor)?;
            }
        }
        RObject::List(values) => {
            for (index, value) in values.iter().enumerate() {
                push_index(path, index);
                if matches!(
                    traverse_object(value, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Pairlist(elements) => {
            for (index, element) in elements.iter().enumerate() {
                let segment = element
                    .tag
                    .as_ref()
                    .map(Arc::clone)
                    .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
                path.push(segment);
                if matches!(
                    traverse_object(&element.value, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Language { function, args } => {
            path.push(Arc::from("function"));
            if matches!(
                traverse_object(function, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            for (index, arg) in args.iter().enumerate() {
                let segment = arg
                    .tag
                    .as_ref()
                    .map(Arc::clone)
                    .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
                path.push(segment);
                if matches!(
                    traverse_object(&arg.value, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Expression(values) => {
            for (index, value) in values.iter().enumerate() {
                push_index(path, index);
                if matches!(
                    traverse_object(value, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => {
            path.push(Arc::from("formals"));
            if matches!(
                traverse_object(formals, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("body"));
            if matches!(
                traverse_object(body, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("environment"));
            if matches!(
                traverse_object(environment, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
        }
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => {
            path.push(Arc::from("enclosing"));
            if matches!(
                traverse_object(enclosing, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("frame"));
            if matches!(
                traverse_object(frame, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("hashtab"));
            if matches!(
                traverse_object(hashtab, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
        }
        RObject::Promise {
            value,
            expression,
            environment,
        } => {
            path.push(Arc::from("value"));
            if matches!(
                traverse_object(value, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("expression"));
            if matches!(
                traverse_object(expression, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("environment"));
            if matches!(
                traverse_object(environment, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
        }
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => {
            path.push(Arc::from("code"));
            if matches!(
                traverse_object(code, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("constants"));
            if matches!(
                traverse_object(constants, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
            path.push(Arc::from("expr"));
            if matches!(
                traverse_object(expr, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
        }
        RObject::DataFrame(data) => {
            for (name, column) in data.columns.iter() {
                path.push(Arc::clone(name));
                if matches!(
                    traverse_object(column, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Factor(data) => {
            path.push(Arc::from("values"));
            visitor
                .on_vector_metadata(path, VectorKind::Integer, data.values.len())
                .map_err(StreamingError::Visitor)?;
            path.pop();
            path.push(Arc::from("levels"));
            visitor
                .on_vector_metadata(path, VectorKind::Character, data.levels.len())
                .map_err(StreamingError::Visitor)?;
            path.pop();
        }
        RObject::S3Object(data) => {
            if matches!(
                emit_attributes(&data.attributes, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.push(Arc::from("base"));
            if matches!(
                traverse_object(&data.base, path, visitor, state)?,
                TraverseControl::Stop
            ) {
                return Ok(TraverseControl::Stop);
            }
            path.pop();
        }
        RObject::S4Object(data) => {
            for (name, slot) in data.slots.iter() {
                path.push(Arc::clone(name));
                if matches!(
                    traverse_object(slot, path, visitor, state)?,
                    TraverseControl::Stop
                ) {
                    return Ok(TraverseControl::Stop);
                }
                path.pop();
            }
        }
        RObject::Namespace(values) => {
            visitor
                .on_vector_metadata(path, VectorKind::Character, values.len())
                .map_err(StreamingError::Visitor)?;
        }
        RObject::Shared(_) | RObject::WithAttributes { .. } => {}
    }

    visitor
        .on_object_end(path)
        .map_err(StreamingError::Visitor)?;
    Ok(TraverseControl::Continue)
}

#[cfg(test)]
fn emit_attributes<V: RdsVisitor>(
    attributes: &Attributes,
    path: &mut ObjectPath,
    visitor: &mut V,
    state: &mut TraverseState,
) -> StreamingResult<TraverseControl, V::Error> {
    if attributes.is_empty() {
        return Ok(TraverseControl::Continue);
    }
    visitor
        .on_attributes(path, attributes)
        .map_err(StreamingError::Visitor)?;
    for (key, value) in attributes.iter() {
        let segment = Arc::from(format!("@{}", key.as_ref()));
        path.push(segment);
        if matches!(
            traverse_object(value, path, visitor, state)?,
            TraverseControl::Stop
        ) {
            return Ok(TraverseControl::Stop);
        }
        path.pop();
    }
    Ok(TraverseControl::Continue)
}

#[cfg(test)]
fn object_type_name(obj: &RObject) -> &'static str {
    match obj {
        RObject::Null => "Null",
        RObject::Integer(_) => "Integer",
        RObject::Real(_) => "Real",
        RObject::Logical(_) => "Logical",
        RObject::Character(_) => "Character",
        RObject::Symbol(_) => "Symbol",
        RObject::Raw(_) => "Raw",
        RObject::Complex(_) => "Complex",
        RObject::List(_) => "List",
        RObject::Pairlist(_) => "Pairlist",
        RObject::Language { .. } => "Language",
        RObject::Expression(_) => "Expression",
        RObject::Closure { .. } => "Closure",
        RObject::Environment { .. } => "Environment",
        RObject::Promise { .. } => "Promise",
        RObject::Special { .. } => "Special",
        RObject::Builtin { .. } => "Builtin",
        RObject::Bytecode { .. } => "Bytecode",
        RObject::DataFrame(_) => "DataFrame",
        RObject::Factor(_) => "Factor",
        RObject::S3Object(_) => "S3Object",
        RObject::S4Object(_) => "S4Object",
        RObject::Namespace(_) => "Namespace",
        RObject::GlobalEnv => "GlobalEnv",
        RObject::BaseEnv => "BaseEnv",
        RObject::EmptyEnv => "EmptyEnv",
        RObject::MissingArg => "MissingArg",
        RObject::UnboundValue => "UnboundValue",
        RObject::Shared(_) => "Shared",
        RObject::WithAttributes { .. } => "WithAttributes",
    }
}

#[cfg(test)]
fn push_index(path: &mut ObjectPath, index: usize) {
    path.push(Arc::from(format!("[{}]", index)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VectorData;
    use std::convert::Infallible;

    #[derive(Default)]
    struct RecordingVisitor {
        events: Vec<String>,
        stop_after: Option<usize>,
    }

    impl RdsVisitor for RecordingVisitor {
        type Error = Infallible;

        fn on_object_start(
            &mut self,
            path: &ObjectPath,
            obj_type: &str,
        ) -> std::result::Result<VisitAction, Self::Error> {
            let path_str = if path.segments.is_empty() {
                "root".to_string()
            } else {
                path.segments
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(".")
            };
            self.events.push(format!("start:{}:{}", obj_type, path_str));
            if let Some(limit) = self.stop_after {
                if self.events.len() >= limit {
                    return Ok(VisitAction::Stop);
                }
            }
            Ok(VisitAction::Continue)
        }

        fn on_object_end(&mut self, path: &ObjectPath) -> std::result::Result<(), Self::Error> {
            let path_str = if path.segments.is_empty() {
                "root".to_string()
            } else {
                path.segments
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>()
                    .join(".")
            };
            self.events.push(format!("end:{}", path_str));
            Ok(())
        }
    }

    #[test]
    fn visitor_stop_terminates_traversal() {
        let obj = RObject::List(vec![
            RObject::Integer(VectorData::Owned(vec![1, 2])),
            RObject::Integer(VectorData::Owned(vec![3, 4])),
        ]);
        let mut visitor = RecordingVisitor {
            stop_after: Some(1),
            ..Default::default()
        };
        let mut state = TraverseState::new();
        let mut path = ObjectPath::new(Vec::new());
        let result = traverse_object(&obj, &mut path, &mut visitor, &mut state);
        assert!(matches!(result, Ok(TraverseControl::Stop)));
        assert_eq!(visitor.events.len(), 1);
    }

    #[test]
    fn visitor_skip_avoids_children() {
        struct SkipListVisitor {
            seen: Vec<String>,
        }

        impl RdsVisitor for SkipListVisitor {
            type Error = Infallible;

            fn on_object_start(
                &mut self,
                path: &ObjectPath,
                obj_type: &str,
            ) -> std::result::Result<VisitAction, Self::Error> {
                let path_str = if path.segments.is_empty() {
                    "root".to_string()
                } else {
                    path.segments
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<_>>()
                        .join(".")
                };
                self.seen.push(format!("{}:{}", obj_type, path_str));
                if obj_type == "List" {
                    return Ok(VisitAction::Skip);
                }
                Ok(VisitAction::Continue)
            }
        }

        let obj = RObject::List(vec![RObject::List(vec![RObject::Integer(
            VectorData::Owned(vec![1]),
        )])]);
        let mut visitor = SkipListVisitor { seen: Vec::new() };
        let mut state = TraverseState::new();
        let mut path = ObjectPath::new(Vec::new());
        let result = traverse_object(&obj, &mut path, &mut visitor, &mut state);
        assert!(matches!(result, Ok(TraverseControl::Continue)));
        assert_eq!(visitor.seen, vec!["List:root".to_string()]);
    }
}
