//! Parser for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::streaming::{
    RdsVisitor, StreamingError, StreamingProgress, StreamingResult, VisitAction,
};
use crate::types::{
    Attributes, Complex, DataFrameData, FactorData, LazyVector, Logical, PairlistElement, RObject,
    S3ObjectData, S4ObjectData,
};
use byteorder::{BigEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, RwLock};
#[cfg(not(target_arch = "wasm32"))]
use xz2::read::XzDecoder;

use crate::extraction::VectorKind;
use crate::types::VectorData;
#[cfg(target_arch = "wasm32")]
use crate::wasm::{AsyncBufferedCursor, AsyncCursor, AsyncCursorConfig, AsyncRdsInput};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

macro_rules! debug_log {
    ($($arg:tt)*) => {{
        #[cfg(feature = "debug-logging")]
        {
            eprintln!($($arg)*);
        }
        #[cfg(not(feature = "debug-logging"))]
        {
            let _ = format_args!($($arg)*);
        }
    }};
}

#[cfg(target_arch = "wasm32")]
fn sequential_debug_enabled() -> bool {
    #[cfg(feature = "debug-logging")]
    {
        let global = js_sys::global();
        if let Ok(value) =
            js_sys::Reflect::get(&global, &JsValue::from_str("SCONVERT_STREAMING_DEBUG"))
        {
            return value.as_bool().unwrap_or(false);
        }
        return false;
    }
    #[cfg(not(feature = "debug-logging"))]
    {
        false
    }
}

#[cfg(target_arch = "wasm32")]
fn sequential_list_debug_enabled() -> bool {
    #[cfg(feature = "debug-logging")]
    {
        let global = js_sys::global();
        if let Ok(value) =
            js_sys::Reflect::get(&global, &JsValue::from_str("SCONVERT_STREAMING_DEBUG_LIST"))
        {
            return value.as_bool().unwrap_or(false);
        }
        return false;
    }
    #[cfg(not(feature = "debug-logging"))]
    {
        false
    }
}

struct RdsCursor<'a> {
    position: u64,
    len: u64,
    inner: RdsCursorInner<'a>,
}

enum RdsCursorInner<'a> {
    Slice(&'a [u8]),
    #[cfg(not(target_arch = "wasm32"))]
    Input(&'a dyn crate::RdsInput),
}

impl<'a> RdsCursor<'a> {
    fn new_slice(data: &'a [u8]) -> Self {
        Self {
            position: 0,
            len: data.len() as u64,
            inner: RdsCursorInner::Slice(data),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn new_input(input: &'a dyn crate::RdsInput) -> Result<Self> {
        let len = input.len().ok_or_else(|| {
            Error::InvalidFormat("input length is required for parsing".to_string())
        })?;
        Ok(Self {
            position: 0,
            len,
            inner: RdsCursorInner::Input(input),
        })
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn set_position(&mut self, pos: u64) {
        self.position = pos.min(self.len);
    }

    fn len(&self) -> u64 {
        self.len
    }
}

impl Read for RdsCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.len {
            return Ok(0);
        }
        let remaining = (self.len - self.position) as usize;
        let to_read = remaining.min(buf.len());
        match &self.inner {
            RdsCursorInner::Slice(data) => {
                let start = self.position as usize;
                let end = start + to_read;
                buf[..to_read].copy_from_slice(&data[start..end]);
            }
            #[cfg(not(target_arch = "wasm32"))]
            RdsCursorInner::Input(input) => {
                let chunk = input
                    .read_at(self.position, to_read)
                    .map_err(std::io::Error::other)?;
                if chunk.len() != to_read {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "short read from input",
                    ));
                }
                buf[..to_read].copy_from_slice(&chunk);
            }
        }
        self.position += to_read as u64;
        Ok(to_read)
    }
}

impl Seek for RdsCursor<'_> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::End(offset) => self.len as i128 + offset as i128,
            SeekFrom::Current(offset) => self.position as i128 + offset as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }
        self.position = (next as u64).min(self.len);
        Ok(self.position)
    }
}

fn debug_enabled() -> bool {
    #[cfg(feature = "debug-logging")]
    {
        std::env::var("RDS_DEBUG").is_ok()
    }
    #[cfg(not(feature = "debug-logging"))]
    {
        false
    }
}

fn ensure_bytes_available(cursor: &RdsCursor<'_>, needed: usize, context: &str) -> Result<()> {
    let pos = cursor.position() as usize;
    let total = cursor.len() as usize;
    let available = total.saturating_sub(pos);
    if available < needed {
        if debug_enabled() {
            debug_log!(
                "[ENSURE_BYTES] EOF at pos={}, needed={}, available={}, ctx={}",
                pos,
                needed,
                available,
                context
            );
        }
        return Err(Error::UnexpectedEofDetail {
            position: pos,
            needed,
            available,
        });
    }
    Ok(())
}

// Default limits for backward compatibility
const MAX_VECTOR_LENGTH: usize = 50_000_000;
#[allow(dead_code)]
const MAX_ALLOCATION_BYTES: usize = 128 * 1024 * 1024;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_FORCE_MATERIALIZE_VECTOR_LEN: usize = 100_000;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_FORCE_MATERIALIZE_STRING_LEN: usize = 10_000;

/// Parser context holding configuration and state
struct ParserContext {
    max_vector_length: usize,
    max_allocation_bytes: usize,
    mode: crate::ParseMode,
    lazy_threshold: usize,
    bytecode_lazy_threshold: usize,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    s4_slot_policy: crate::S4SlotPolicy,
    /// True when parsing bytecode constants (use bytecode_lazy_threshold)
    in_bytecode_context: bool,

    // Parse state (previously thread-local)
    pending_class_attrs: Option<Attributes>,
    parsing_attributes: bool,
    parsing_closure_body: bool,
    parsing_s4_tag: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    parsing_pairlist_root: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    suppress_ref_tracking: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    stop_streaming: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    force_materialize_vector: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    lenient_skip_vectors: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    streaming_parse_mode: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    async_parse_mode: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    s4_slot_overrides: IndexMap<Arc<str>, RObject>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
struct ParserContextSnapshot {
    pending_class_attrs: Option<Attributes>,
    parsing_attributes: bool,
    parsing_closure_body: bool,
    parsing_s4_tag: bool,
    parsing_pairlist_root: bool,
    suppress_ref_tracking: bool,
    in_bytecode_context: bool,
    stop_streaming: bool,
    force_materialize_vector: bool,
    lenient_skip_vectors: bool,
    streaming_parse_mode: bool,
    async_parse_mode: bool,
    s4_slot_overrides: IndexMap<Arc<str>, RObject>,
}

impl ParserContext {
    fn from_config(config: crate::ParseConfig) -> Self {
        Self {
            max_vector_length: config.max_vector_length,
            max_allocation_bytes: config.max_allocation_bytes,
            lazy_threshold: config.lazy_threshold,
            bytecode_lazy_threshold: config.bytecode_lazy_threshold,
            mode: config.mode,
            s4_slot_policy: config.s4_slot_policy,
            in_bytecode_context: false,
            // Initialize parse state to clean values
            pending_class_attrs: None,
            parsing_attributes: false,
            parsing_closure_body: false,
            parsing_s4_tag: false,
            parsing_pairlist_root: false,
            suppress_ref_tracking: false,
            stop_streaming: false,
            force_materialize_vector: false,
            lenient_skip_vectors: false,
            streaming_parse_mode: false,
            async_parse_mode: false,
            s4_slot_overrides: IndexMap::new(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn snapshot(&self) -> ParserContextSnapshot {
        ParserContextSnapshot {
            pending_class_attrs: self.pending_class_attrs.clone(),
            parsing_attributes: self.parsing_attributes,
            parsing_closure_body: self.parsing_closure_body,
            parsing_s4_tag: self.parsing_s4_tag,
            parsing_pairlist_root: self.parsing_pairlist_root,
            suppress_ref_tracking: self.suppress_ref_tracking,
            in_bytecode_context: self.in_bytecode_context,
            stop_streaming: self.stop_streaming,
            force_materialize_vector: self.force_materialize_vector,
            lenient_skip_vectors: self.lenient_skip_vectors,
            streaming_parse_mode: self.streaming_parse_mode,
            async_parse_mode: self.async_parse_mode,
            s4_slot_overrides: self.s4_slot_overrides.clone(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn restore(&mut self, snapshot: ParserContextSnapshot) {
        self.pending_class_attrs = snapshot.pending_class_attrs;
        self.parsing_attributes = snapshot.parsing_attributes;
        self.parsing_closure_body = snapshot.parsing_closure_body;
        self.parsing_s4_tag = snapshot.parsing_s4_tag;
        self.parsing_pairlist_root = snapshot.parsing_pairlist_root;
        self.suppress_ref_tracking = snapshot.suppress_ref_tracking;
        self.in_bytecode_context = snapshot.in_bytecode_context;
        self.stop_streaming = snapshot.stop_streaming;
        self.force_materialize_vector = snapshot.force_materialize_vector;
        self.lenient_skip_vectors = snapshot.lenient_skip_vectors;
        self.streaming_parse_mode = snapshot.streaming_parse_mode;
        self.async_parse_mode = snapshot.async_parse_mode;
        self.s4_slot_overrides = snapshot.s4_slot_overrides;
    }

    /// Get the effective lazy threshold based on current context
    #[inline]
    fn effective_lazy_threshold(&self) -> usize {
        if self.in_bytecode_context {
            self.bytecode_lazy_threshold
        } else {
            self.lazy_threshold
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn tag_in_list(tag: &Option<Arc<str>>, list: &[Arc<str>]) -> bool {
    if let Some(tag) = tag.as_ref() {
        list.iter().any(|entry| entry == tag)
    } else {
        false
    }
}

#[cfg(target_arch = "wasm32")]
fn should_skip_s4_slot(ctx: &ParserContext, tag: &Option<Arc<str>>) -> bool {
    if ctx.mode != crate::ParseMode::LazyMetadata {
        return false;
    }
    tag_in_list(tag, &ctx.s4_slot_policy.skip_slots)
}

#[cfg(target_arch = "wasm32")]
fn should_keep_s4_slot(ctx: &ParserContext, tag: &Option<Arc<str>>) -> bool {
    if ctx.mode != crate::ParseMode::LazyMetadata {
        return true;
    }
    if tag_in_list(tag, &ctx.s4_slot_policy.skip_slots) {
        return false;
    }
    if ctx.s4_slot_policy.keep_slots.is_empty() {
        return true;
    }
    tag_in_list(tag, &ctx.s4_slot_policy.keep_slots)
}

#[cfg(target_arch = "wasm32")]
fn should_capture_s4_slot(ctx: &ParserContext, tag: &Option<Arc<str>>) -> bool {
    if ctx.mode != crate::ParseMode::LazyMetadata {
        return false;
    }
    tag_in_list(tag, &ctx.s4_slot_policy.capture_slots)
}

#[cfg(target_arch = "wasm32")]
fn should_capture_any_slot(ctx: &ParserContext, tag: &Option<Arc<str>>) -> bool {
    if ctx.mode != crate::ParseMode::LazyMetadata {
        return false;
    }
    if !ctx.parsing_pairlist_root {
        return false;
    }
    tag_in_list(tag, &ctx.s4_slot_policy.capture_any_slots)
}

#[cfg(target_arch = "wasm32")]
fn should_materialize_vector_tag(ctx: &ParserContext, tag: &Option<Arc<str>>) -> bool {
    if ctx.mode != crate::ParseMode::LazyMetadata {
        return false;
    }
    tag_in_list(tag, &ctx.s4_slot_policy.materialize_vector_tags)
}

#[cfg(target_arch = "wasm32")]
fn log_large_alloc(_ctx: &ParserContext, _kind: &str, _length: usize) {}

fn guard_allocation(
    ctx: &mut ParserContext,
    length: usize,
    elem_size: usize,
    cursor: &RdsCursor<'_>,
    context: &str,
) -> Result<()> {
    guard_allocation_common(ctx, length, elem_size, context)?;

    // Skip cursor length check in streaming parse mode, where cursor is a limited slice
    // of the full data, not representing the complete file size
    #[cfg(target_arch = "wasm32")]
    if ctx.streaming_parse_mode {
        return Ok(());
    }

    let needed = length * elem_size;
    let remaining = (cursor.len() as usize).saturating_sub(cursor.position() as usize);
    if needed > remaining.saturating_add(16) {
        return Err(Error::InvalidFormat(format!(
            "Length {} ({} bytes) exceeds remaining {} bytes while parsing {}",
            length, needed, remaining, context
        )));
    }

    Ok(())
}

fn guard_allocation_common(
    ctx: &mut ParserContext,
    length: usize,
    elem_size: usize,
    context: &str,
) -> Result<()> {
    let allow_lazy = matches!(ctx.mode, crate::ParseMode::LazyMetadata)
        && length > ctx.effective_lazy_threshold()
        && !ctx.force_materialize_vector;
    let max_vector_length = if allow_lazy {
        usize::MAX
    } else {
        ctx.max_vector_length
    };

    if length > max_vector_length {
        return Err(Error::InvalidFormat(format!(
            "Length {} exceeds safe limit {} while parsing {}",
            length, max_vector_length, context
        )));
    }

    let needed = length.checked_mul(elem_size).ok_or_else(|| {
        Error::InvalidFormat(format!("Length overflow while parsing {}", context))
    })?;

    if !allow_lazy && needed > ctx.max_allocation_bytes {
        return Err(Error::InvalidFormat(format!(
            "Allocation of {} bytes exceeds cap while parsing {}",
            needed, context
        )));
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn guard_skip_allocation<C: AsyncCursor>(
    length: usize,
    elem_size: usize,
    cursor: &C,
    context: &str,
) -> Result<()> {
    let needed = length.checked_mul(elem_size).ok_or_else(|| {
        Error::InvalidFormat(format!("Length overflow while parsing {}", context))
    })?;
    if let Some(total) = cursor.total_len() {
        let position = cursor.position();
        let remaining = total.saturating_sub(position);
        let needed_u64 = u64::try_from(needed).unwrap_or(u64::MAX);

        if needed_u64 > remaining.saturating_add(16) {
            return Err(Error::InvalidFormat(format!(
                "Length {} ({} bytes) exceeds remaining {} bytes while parsing {}",
                length, needed, remaining, context
            )));
        }
    }
    Ok(())
}

/// Reference table for tracking objects during deserialization.
/// R's serialization uses reference tracking to handle shared and circular references.
/// Each object that might be referenced later gets assigned a sequential index (1, 2, 3, ...).
/// When a REFSXP is encountered, it contains an index to retrieve the previously seen object.
struct RefTable {
    /// Map from reference index to the actual object (Arc to allow cheap sharing)
    objects: HashMap<u32, Arc<RwLock<RObject>>>,
    /// Next reference index to assign
    next_index: u32,
}

impl RefTable {
    fn new() -> Self {
        RefTable {
            objects: HashMap::new(),
            next_index: 1, // R uses 1-based indexing for references
        }
    }

    /// Add an object to the reference table and return its index
    fn add(&mut self, obj: RObject) -> u32 {
        let index = self.next_index;
        let arc = Arc::new(RwLock::new(obj));
        if std::env::var("RDS_DEBUG_REF_ORDER").is_ok() {
            let name = arc.read().unwrap().variant_name();
            debug_log!("[PARSE_REF] idx={} type={}", index, name);
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let name = arc.read().unwrap().variant_name();
            let msg = format!("seq ref add idx={} type={}", index, name);
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        // WORKAROUND: Conditional branch prevents LLVM backend optimization bug
        // See: RDS2RUST_PHASE1_FINDINGS.md and RDS2RUST_PHASE2_ADVANCED_TESTS.md
        if true {
            let _ = index; // Force compiler to preserve Arc reference ordering
        }
        self.objects.insert(index, arc);
        self.next_index += 1;
        index
    }

    /// Update an existing reference with a new object
    fn update(&mut self, index: u32, obj: RObject) {
        if let Some(existing) = self.objects.get(&index) {
            let mut guard = existing.write().unwrap();
            // WORKAROUND: Conditional branch prevents LLVM backend optimization bug
            if true {
                let _ = index; // Force compiler to preserve Arc reference ordering
            }
            *guard = obj;
            if std::env::var("RDS_DEBUG_REF_ORDER").is_ok() {
                let name = guard.variant_name();
                debug_log!("[PARSE_REF_UPDATE] idx={} type={}", index, name);
            }
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() {
                let name = guard.variant_name();
                let msg = format!("seq ref update idx={} type={}", index, name);
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            return;
        }
        // WORKAROUND: Conditional branch prevents LLVM backend optimization bug
        if true {
            let _ = index; // Force compiler to preserve Arc reference ordering
        }
        self.objects.insert(index, Arc::new(RwLock::new(obj)));
    }

    /// Get an object by its reference index
    fn get(&self, index: u32) -> Option<Arc<RwLock<RObject>>> {
        let result = self.objects.get(&index).cloned();
        // WORKAROUND: Conditional branch prevents LLVM backend optimization bug
        if let Some(ref arc) = result {
            let _ = arc; // Force compiler to preserve Arc reference ordering
        }
        result
    }

    #[cfg(target_arch = "wasm32")]
    fn checkpoint(&self) -> u32 {
        self.next_index
    }

    #[cfg(target_arch = "wasm32")]
    fn rollback(&mut self, checkpoint: u32) {
        if checkpoint < self.next_index {
            for idx in checkpoint..self.next_index {
                self.objects.remove(&idx);
            }
        }
        self.next_index = checkpoint;
    }
}

/// Symbol table for tracking symbols during deserialization.
/// When REFSXP appears in TAG positions (e.g., pairlist tags for attributes),
/// the reference index refers to the N-th symbol parsed, NOT the N-th object in RefTable.
/// This matches R's serialization format which uses a separate symbol table.
struct SymbolTable {
    /// List of symbols in the order they were parsed
    symbols: Vec<RObject>,
}

impl SymbolTable {
    fn new() -> Self {
        SymbolTable {
            symbols: Vec::new(),
        }
    }

    /// Add a symbol to the table and return its 1-based index
    fn add(&mut self, symbol: RObject) -> u32 {
        self.symbols.push(symbol);
        self.symbols.len() as u32 // 1-based index
    }

    /// Retrieve a symbol by its 1-based index (used for TAG REFSXP lookups).
    fn get(&self, index: u32) -> Option<&RObject> {
        if index == 0 {
            return None;
        }
        self.symbols.get((index - 1) as usize)
    }

    fn len(&self) -> usize {
        self.symbols.len()
    }

    #[cfg(target_arch = "wasm32")]
    fn checkpoint(&self) -> usize {
        self.symbols.len()
    }

    #[cfg(target_arch = "wasm32")]
    fn rollback(&mut self, checkpoint: usize) {
        self.symbols.truncate(checkpoint);
    }
}

/// Deduplication table for memory-efficient object sharing.
/// Tracks previously seen objects to avoid duplicating identical data.
/// Uses Arc-based sharing for efficient cloning of deduplicated objects.
struct DedupTable {
    /// Cache of previously seen objects wrapped in Arc for cheap cloning
    /// We use a Vec for simple linear search since most RDS files have a small
    /// number of unique repeated objects (e.g., class names, common vectors)
    cache: Vec<Arc<RObject>>,
    /// Statistics for monitoring deduplication effectiveness
    hits: usize,
    misses: usize,
}

impl DedupTable {
    fn new() -> Self {
        DedupTable {
            cache: Vec::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Try to deduplicate an object.
    /// Returns Some(existing) if an identical object exists in the cache,
    /// otherwise adds the object to the cache and returns None.
    fn deduplicate(&mut self, obj: &RObject) -> Option<RObject> {
        let obj_concrete = obj.as_concrete();
        // Check if we've seen this object before
        for cached in &self.cache {
            if cached.as_ref().as_concrete() == obj_concrete {
                // Found a match! Return a clone (cheap Arc clone for strings, actual clone for others)
                self.hits += 1;
                return Some((**cached).clone());
            }
        }

        // New unique object - add to cache
        self.misses += 1;

        // Only cache if it's likely to be repeated and not too large
        if should_cache_for_dedup(&obj_concrete) {
            self.cache.push(Arc::new(obj_concrete.clone()));
        }

        None
    }

    #[cfg(target_arch = "wasm32")]
    fn checkpoint(&self) -> (usize, usize, usize) {
        (self.cache.len(), self.hits, self.misses)
    }

    #[cfg(target_arch = "wasm32")]
    fn rollback(&mut self, checkpoint: (usize, usize, usize)) {
        let (len, hits, misses) = checkpoint;
        self.cache.truncate(len);
        self.hits = hits;
        self.misses = misses;
    }

    /// Get deduplication statistics (for debugging/profiling)
    #[allow(dead_code)]
    fn stats(&self) -> (usize, usize, f64) {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
        (self.hits, self.misses, hit_rate)
    }
}

/// Determine if an object should be cached for deduplication.
/// We only cache objects that are likely to be repeated and not too expensive to store.
fn should_cache_for_dedup(obj: &RObject) -> bool {
    match obj {
        // Cache small vectors (likely column names, factor levels, etc.)
        RObject::Character(vec) if vec.len() <= 100 => true,
        RObject::Integer(vec) if vec.len() <= 50 => true,
        RObject::Real(vec) if vec.len() <= 50 => true,
        RObject::Logical(vec) if vec.len() <= 50 => true,

        // Cache NULL, factors, and simple objects
        RObject::Null => true,
        RObject::Factor(_) => true,

        // Don't cache large or complex objects
        RObject::DataFrame(_) => false,
        RObject::S3Object(_) => false,
        RObject::S4Object(_) => false,
        RObject::List(_) => false,
        RObject::Environment { .. } => false,
        RObject::Closure { .. } => false,

        // Cache other small types
        _ => true,
    }
}

/// Determine if an object should be tracked in the reference table.
///
/// R's serializer assigns reference indices to most heap objects. Track the
/// same set R does: structured objects always, atomic vectors only when they
/// carry attributes, and skip singleton markers that never receive reference
/// indices.
fn should_track_reference(sexp_type: u32, has_attr: bool) -> bool {
    match sexp_type {
        // Singletons that never receive reference indices
        NILSXP | GLOBALENV_SXP | BASEENV_SXP | EMPTYENV_SXP => false,

        // Symbols, environments, promises, language objects, lists, S4, etc.
        // are always reference-tracked.
        SYMSXP | ENVSXP | NAMESPACESXP | NAMESPACESXP_SERIAL => true,
        PROMSXP | LANGSXP | LISTSXP | VECSXP | EXPRSXP => true,
        CLOSXP | BCODESXP | EXTPTRSXP | WEAKREFSXP | S4SXP => true,
        GENERICREFSXP | CLASSREFSXP => true,

        // Atomic vectors should be reference-tracked as well so REFSXP indices
        // stay aligned with R's serializer (which can share even plain vectors
        // like dimnames). Attributes are not required for tracking.
        LGLSXP | INTSXP | REALSXP | CPLXSXP | STRSXP | RAWSXP => true,

        // CHARSXP uses per-vector string caches, not the global reference table.
        CHARSXP => false,

        // Builtins/specials and other values default to non-tracked unless
        // they carry attributes.
        SPECIALSXP | BUILTINSXP => has_attr,

        // REFSXP is a reference to an existing object, not a new object, so it shouldn't be tracked
        REFSXP => false,

        // Fallback: track other/unknown types to avoid dropping reference slots.
        _ => true,
    }
}

/// Parse an RDS file from a byte slice.
/// Parse an RDS file with custom configuration
pub fn parse_rds_with_config(
    data: &[u8],
    config: crate::ParseConfig,
) -> Result<crate::ParseResult> {
    let mut ctx = ParserContext::from_config(config);
    let object = parse_rds_internal(data, &mut ctx)?;
    let warnings = build_lazy_vector_warnings(&object, &ctx);
    Ok(crate::ParseResult { object, warnings })
}

/// Parse an RDS file with default configuration
#[allow(dead_code)]
pub(crate) fn parse_rds(data: &[u8]) -> Result<crate::ParseResult> {
    parse_rds_with_config(data, crate::ParseConfig::default())
}

fn parse_rds_internal(data: &[u8], ctx: &mut ParserContext) -> Result<RObject> {
    // Note: Parse state is now automatically clean because ctx is freshly initialized
    // in parse_rds_with_config(). No need to reset - each parse gets fresh state.

    // Check if the file is gzip or xz compressed
    // gzip starts with 0x1f 0x8b
    // xz starts with 0xfd 0x37 0x7a 0x58 0x5a 0x00
    let is_xz = data.len() >= 6
        && data[0] == 0xfd
        && data[1] == 0x37
        && data[2] == 0x7a
        && data[3] == 0x58
        && data[4] == 0x5a
        && data[5] == 0x00;

    let decompressed_data = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        // Decompress gzip
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        decompressed
    } else if is_xz {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Decompress xz using the xz2 crate
            let mut decoder = XzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            decompressed
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err(Error::CompressionError(
                "XZ compressed RDS is not supported on wasm32 targets".to_string(),
            ));
        }
    } else {
        data.to_vec()
    };

    let mut cursor = RdsCursor::new_slice(decompressed_data.as_slice());
    parse_rds_internal_cursor(&mut cursor, ctx)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_rds_with_input(
    input: &dyn crate::RdsInput,
    config: crate::ParseConfig,
) -> Result<crate::ParseResult> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = RdsCursor::new_input(input)?;
    let object = parse_rds_internal_cursor(&mut cursor, &mut ctx)?;
    let warnings = build_lazy_vector_warnings(&object, &ctx);
    Ok(crate::ParseResult { object, warnings })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn traverse_rds_streaming_with_input<V: RdsVisitor>(
    input: &dyn crate::RdsInput,
    config: crate::ParseConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = RdsCursor::new_input(input)?;
    traverse_rds_internal_cursor(&mut cursor, &mut ctx, visitor, None)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn traverse_rds_streaming_with_input_progress<V: RdsVisitor>(
    input: &dyn crate::RdsInput,
    config: crate::ParseConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = RdsCursor::new_input(input)?;
    traverse_rds_internal_cursor(&mut cursor, &mut ctx, visitor, Some(progress))
}

fn parse_rds_internal_cursor(
    cursor: &mut RdsCursor<'_>,
    ctx: &mut ParserContext,
) -> Result<RObject> {
    // Parse header
    let format_version = parse_header(cursor)?;

    // Format version 3 includes native encoding information in the header
    if format_version >= 3 {
        // Read the encoding string length and the encoding string itself
        ensure_bytes_available(cursor, 4, "parse_rds:enc_len")?;
        let enc_len = cursor.read_u32::<BigEndian>()? as usize;
        guard_allocation(ctx, enc_len, 1, cursor, "header encoding")?;
        let mut enc_bytes = vec![0u8; enc_len];
        ensure_bytes_available(cursor, enc_len, "parse_rds:enc_bytes")?;
        cursor.read_exact(&mut enc_bytes)?;
        // We now have the encoding (e.g., "UTF-8"), but we'll ignore it for now
    }

    // Create reference table for tracking shared objects
    let mut ref_table = RefTable::new();

    // Create symbol table for tracking symbols in parse order
    let mut symbol_table = SymbolTable::new();

    // Create deduplication table for memory-efficient object sharing
    let mut dedup_table = DedupTable::new();

    // Parse the actual object
    if debug_enabled() {
        debug_log!(
            "[PARSE_RDS] About to parse root object at pos={}",
            cursor.position()
        );
    }
    let result = parse_object(
        ctx,
        cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
    );
    if debug_enabled() {
        match &result {
            Ok(_) => debug_log!(
                "[PARSE_RDS] Root object parsed successfully, pos={}",
                cursor.position()
            ),
            Err(e) => debug_log!(
                "[PARSE_RDS] Root object parse FAILED: {} (pos={}, remaining={})",
                e,
                cursor.position(),
                cursor.len().saturating_sub(cursor.position())
            ),
        }
    }
    result
}

#[cfg(not(target_arch = "wasm32"))]
fn traverse_rds_internal_cursor<V: RdsVisitor>(
    cursor: &mut RdsCursor<'_>,
    ctx: &mut ParserContext,
    visitor: &mut V,
    progress: Option<&mut dyn FnMut(StreamingProgress)>,
) -> StreamingResult<(), V::Error> {
    let format_version = parse_header(cursor)?;
    visitor
        .on_header(format_version)
        .map_err(StreamingError::Visitor)?;

    if format_version >= 3 {
        ensure_bytes_available(cursor, 4, "parse_rds:enc_len")?;
        let enc_len = cursor
            .read_u32::<BigEndian>()
            .map_err(|e| StreamingError::Parse(Error::Io(e)))? as usize;
        guard_allocation(ctx, enc_len, 1, cursor, "header encoding")?;
        let mut enc_bytes = vec![0u8; enc_len];
        ensure_bytes_available(cursor, enc_len, "parse_rds:enc_bytes")?;
        cursor
            .read_exact(&mut enc_bytes)
            .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
    }

    let mut ref_table = RefTable::new();
    let mut symbol_table = SymbolTable::new();
    let mut dedup_table = DedupTable::new();
    let mut ref_paths = StreamingRefTable::new();
    let mut path = crate::ObjectPath::new(Vec::new());
    let mut progress_state = StreamingProgressState::new(Some(cursor.len()), progress);

    match parse_object_streaming(
        ctx,
        cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
        &mut ref_paths,
        &mut progress_state,
        visitor,
        &mut path,
        true,
    )? {
        StreamControl::Stop => Ok(()),
        StreamControl::Continue => Ok(()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn parse_rds_with_async_input(
    input: &dyn AsyncRdsInput,
    config: crate::ParseConfig,
    cursor_config: AsyncCursorConfig,
) -> Result<crate::ParseResult> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = AsyncBufferedCursor::new(input, cursor_config).await?;
    let object = parse_rds_internal_async(&mut cursor, &mut ctx).await?;
    let warnings = build_lazy_vector_warnings(&object, &ctx);
    Ok(crate::ParseResult { object, warnings })
}

fn build_lazy_vector_warnings(
    object: &RObject,
    ctx: &ParserContext,
) -> Vec<crate::MetadataWarning> {
    if !matches!(ctx.mode, crate::ParseMode::LazyMetadata) {
        return Vec::new();
    }

    let threshold = ctx.effective_lazy_threshold();
    object
        .lazy_vector_infos()
        .into_iter()
        .map(
            |(path, vector_type, span)| crate::MetadataWarning::VectorLazy {
                path,
                vector_type: vector_type.to_string(),
                length: span.length,
                threshold,
                byte_len: span.byte_len,
            },
        )
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_with_async_input<V: RdsVisitor>(
    input: &dyn AsyncRdsInput,
    config: crate::ParseConfig,
    cursor_config: AsyncCursorConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = AsyncBufferedCursor::new(input, cursor_config)
        .await
        .map_err(StreamingError::Parse)?;
    traverse_rds_internal_async_streaming(&mut cursor, &mut ctx, visitor, None).await
}

#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_with_async_input_progress<V: RdsVisitor>(
    input: &dyn AsyncRdsInput,
    config: crate::ParseConfig,
    cursor_config: AsyncCursorConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error> {
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = AsyncBufferedCursor::new(input, cursor_config)
        .await
        .map_err(StreamingError::Parse)?;
    traverse_rds_internal_async_streaming(&mut cursor, &mut ctx, visitor, Some(progress)).await
}

#[cfg(target_arch = "wasm32")]
async fn parse_rds_internal_async(
    cursor: &mut AsyncBufferedCursor<'_>,
    ctx: &mut ParserContext,
) -> Result<RObject> {
    let format_version = parse_with_sync_cursor_retry(cursor, 14, 14, |c| parse_header(c)).await?;

    if format_version >= 3 {
        let enc_len = read_u32_async(cursor).await? as usize;
        guard_allocation_common(ctx, enc_len, 1, "header encoding")?;
        let _enc_bytes = read_bytes_async(cursor, enc_len).await?;
    }

    let mut ref_table = RefTable::new();
    let mut symbol_table = SymbolTable::new();
    let mut dedup_table = DedupTable::new();

    if ctx.mode == crate::ParseMode::LazyMetadata {
        return parse_object_sequential_value_async(
            ctx,
            cursor,
            &mut ref_table,
            &mut symbol_table,
            &mut dedup_table,
        )
        .await;
    }

    parse_object_async(
        ctx,
        cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
    )
    .await
}

/// Traverse RDS from a sequential input source (for streaming decompression).
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_with_sequential_input<I, V>(
    input: &mut I,
    config: crate::ParseConfig,
    visitor: &mut V,
) -> StreamingResult<(), V::Error>
where
    I: crate::AsyncSequentialInput,
    V: RdsVisitor,
{
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = crate::SequentialCursor::new(input)
        .await
        .map_err(StreamingError::Parse)?;
    traverse_rds_internal_sequential_streaming(&mut cursor, &mut ctx, visitor, None).await
}

/// Traverse RDS from a sequential input source with progress reporting.
#[cfg(target_arch = "wasm32")]
pub async fn traverse_rds_streaming_with_sequential_input_progress<I, V>(
    input: &mut I,
    config: crate::ParseConfig,
    visitor: &mut V,
    progress: &mut dyn FnMut(StreamingProgress),
) -> StreamingResult<(), V::Error>
where
    I: crate::AsyncSequentialInput,
    V: RdsVisitor,
{
    let mut ctx = ParserContext::from_config(config);
    let mut cursor = crate::SequentialCursor::new(input)
        .await
        .map_err(StreamingError::Parse)?;
    traverse_rds_internal_sequential_streaming(&mut cursor, &mut ctx, visitor, Some(progress)).await
}

#[cfg(target_arch = "wasm32")]
async fn traverse_rds_internal_sequential_streaming<I, V>(
    cursor: &mut crate::SequentialCursor<'_, I>,
    ctx: &mut ParserContext,
    visitor: &mut V,
    progress: Option<&mut dyn FnMut(StreamingProgress)>,
) -> StreamingResult<(), V::Error>
where
    I: crate::AsyncSequentialInput,
    V: RdsVisitor,
{
    let format_version = parse_with_sync_cursor_retry(cursor, 14, 14, |c| parse_header(c)).await?;
    visitor
        .on_header(format_version)
        .map_err(StreamingError::Visitor)?;

    if format_version >= 3 {
        let enc_len = read_u32_async(cursor).await? as usize;
        guard_allocation_common(ctx, enc_len, 1, "header encoding")?;
        let _enc_bytes = read_bytes_async(cursor, enc_len).await?;
    }

    let mut ref_table = RefTable::new();
    let mut symbol_table = SymbolTable::new();
    let mut dedup_table = DedupTable::new();
    let mut ref_paths = StreamingRefTable::new();
    let mut path = crate::ObjectPath::new(Vec::new());
    let mut progress_state = StreamingProgressState::new(cursor.total_len(), progress);

    let _ = parse_object_streaming_async(
        ctx,
        cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
        &mut ref_paths,
        &mut progress_state,
        visitor,
        &mut path,
        true,
    )
    .await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn traverse_rds_internal_async_streaming<V: RdsVisitor>(
    cursor: &mut AsyncBufferedCursor<'_>,
    ctx: &mut ParserContext,
    visitor: &mut V,
    progress: Option<&mut dyn FnMut(StreamingProgress)>,
) -> StreamingResult<(), V::Error> {
    let format_version = parse_with_sync_cursor_retry(cursor, 14, 14, |c| parse_header(c)).await?;
    visitor
        .on_header(format_version)
        .map_err(StreamingError::Visitor)?;

    if format_version >= 3 {
        let enc_len = read_u32_async(cursor).await? as usize;
        guard_allocation_common(ctx, enc_len, 1, "header encoding")?;
        let _enc_bytes = read_bytes_async(cursor, enc_len).await?;
    }

    let mut ref_table = RefTable::new();
    let mut symbol_table = SymbolTable::new();
    let mut dedup_table = DedupTable::new();
    let mut ref_paths = StreamingRefTable::new();
    let mut path = crate::ObjectPath::new(Vec::new());
    let mut progress_state = StreamingProgressState::new(cursor.total_len(), progress);

    let _ = parse_object_streaming_async(
        ctx,
        cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
        &mut ref_paths,
        &mut progress_state,
        visitor,
        &mut path,
        true,
    )
    .await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn calculate_lazy_vector_bytes(obj: &RObject) -> u64 {
    use crate::types::VectorData;
    match obj {
        RObject::Integer(VectorData::Lazy(v)) => v.byte_len,
        RObject::Real(VectorData::Lazy(v)) => v.byte_len,
        RObject::Logical(VectorData::Lazy(v)) => v.byte_len,
        RObject::Complex(VectorData::Lazy(v)) => v.byte_len,
        RObject::Raw(VectorData::Lazy(v)) => v.byte_len,
        _ => 0,
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_object_async(
    ctx: &mut ParserContext,
    cursor: &mut AsyncBufferedCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    cursor.ensure_available(8).await?;
    let estimate = crate::estimate_parse_size(cursor).unwrap_or(cursor.buffer_size());
    let total_len = cursor
        .total_len()
        .ok_or_else(|| Error::InvalidFormat("async cursor requires length".to_string()))?;
    let remaining = (total_len - cursor.position()) as usize;
    let max_size = std::cmp::min(cursor.max_buffer_size(), remaining);
    let mut size = estimate.clamp(4, max_size.max(4));

    loop {
        cursor.ensure_available(size).await?;
        let slice = cursor.as_sync_slice(size)?;
        let mut sync_cursor = RdsCursor::new_slice(slice);

        // Set streaming mode BEFORE taking snapshot so it persists across retries
        ctx.streaming_parse_mode = true;
        ctx.async_parse_mode = true;

        let ctx_snapshot = ctx.snapshot();
        let ref_checkpoint = ref_table.checkpoint();
        let symbol_checkpoint = symbol_table.checkpoint();
        let dedup_checkpoint = dedup_table.checkpoint();

        let result = parse_object(ctx, &mut sync_cursor, ref_table, symbol_table, dedup_table);

        match result {
            Ok(value) => {
                let sync_pos = sync_cursor.position();
                cursor.advance(sync_pos)?;

                // In WASM streaming mode, if the value contains lazy vectors, we need to skip past them
                // since the sync cursor didn't advance past the vector data
                #[cfg(target_arch = "wasm32")]
                {
                    let bytes_to_skip = calculate_lazy_vector_bytes(&value);
                    if bytes_to_skip > 0 {
                        let msg = format!("Skipping lazy vector: sync_pos={}, bytes_to_skip={}, cursor_pos_before={}, cursor_pos_after={}",
                            sync_pos, bytes_to_skip, cursor.position(), cursor.position() + bytes_to_skip as u64);
                        web_sys::console::debug_1(&wasm_bindgen::JsValue::from_str(&msg));
                        cursor.skip_bytes(bytes_to_skip as usize).await?;
                    }
                }

                return Ok(value);
            }
            Err(Error::UnexpectedEof) | Err(Error::UnexpectedEofDetail { .. }) => {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                if size >= max_size {
                    return result;
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(Error::Io(ref err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                #[cfg(target_arch = "wasm32")]
                {
                    let msg = format!(
                        "IO UnexpectedEof caught, retrying with larger buffer: {} -> {}",
                        size,
                        size.saturating_mul(2).min(max_size)
                    );
                    web_sys::console::debug_1(&wasm_bindgen::JsValue::from_str(&msg));
                }
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                if size >= max_size {
                    return result;
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(Error::InvalidFormat(message))
                if message.contains("exceeds remaining") && size < max_size =>
            {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_object_streaming_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    progress: &mut StreamingProgressState<'_>,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    emit: bool,
) -> StreamingResult<StreamControl, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    if ctx.stop_streaming {
        return Ok(StreamControl::Stop);
    }
    cursor
        .ensure_available(4)
        .await
        .map_err(StreamingError::Parse)?;
    let flags = cursor.peek_u32().map_err(StreamingError::Parse)?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
    if sexp_type == ALTREP_SXP && cursor.total_len().is_none() {
        return std::pin::Pin::from(Box::new(parse_altrep_streaming_sequential_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            emit,
            flags,
        )))
        .await;
    }
    if matches!(sexp_type, INTSXP | REALSXP | LGLSXP | RAWSXP | CPLXSXP) {
        match std::pin::Pin::from(Box::new(try_parse_large_vector_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            emit,
        )))
        .await
        {
            Ok(Some(control)) => return Ok(control),
            Ok(None) => {}
            Err(StreamingError::Parse(Error::StreamingStop)) => {
                return Ok(StreamControl::Stop);
            }
            Err(err) => return Err(err),
        }
    }
    cursor
        .ensure_available(8)
        .await
        .map_err(StreamingError::Parse)?;
    let estimate = estimate_parse_size_from_cursor(cursor).unwrap_or(cursor.buffer_size());
    let max_size = match cursor.total_len() {
        Some(total_len) => {
            let remaining = total_len.saturating_sub(cursor.position()) as usize;
            std::cmp::min(cursor.max_buffer_size(), remaining)
        }
        None => cursor.max_buffer_size(),
    };
    let mut size = estimate.clamp(4, max_size.max(4));

    loop {
        cursor
            .ensure_available(size)
            .await
            .map_err(StreamingError::Parse)?;
        let slice = cursor.as_sync_slice(size).map_err(StreamingError::Parse)?;
        let mut sync_cursor = RdsCursor::new_slice(slice);

        // Set streaming mode BEFORE taking snapshot so it persists across retries
        ctx.streaming_parse_mode = true;
        ctx.async_parse_mode = false;

        let ctx_snapshot = ctx.snapshot();
        let ref_checkpoint = ref_table.checkpoint();
        let symbol_checkpoint = symbol_table.checkpoint();
        let dedup_checkpoint = dedup_table.checkpoint();

        progress.base_offset = cursor.position();
        let result = parse_object_streaming(
            ctx,
            &mut sync_cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            emit,
        );

        match result {
            Ok(control) => {
                cursor
                    .advance(sync_cursor.position())
                    .map_err(StreamingError::Parse)?;
                return Ok(control);
            }
            Err(StreamingError::Visitor(err)) => return Err(StreamingError::Visitor(err)),
            Err(StreamingError::Parse(Error::UnexpectedEof))
            | Err(StreamingError::Parse(Error::UnexpectedEofDetail { .. })) => {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                if size >= max_size {
                    let obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                        ctx,
                        cursor,
                        ref_table,
                        symbol_table,
                        dedup_table,
                    )))
                    .await
                    .map_err(StreamingError::Parse)?;
                    if emit {
                        emit_parsed_object_streaming(&obj, path, visitor)?;
                    }
                    progress.report_object(cursor.position());
                    return Ok(StreamControl::Continue);
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
            }
            Err(StreamingError::Parse(Error::Io(ref err)))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                if size >= max_size {
                    return result;
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
            }
            Err(StreamingError::Parse(Error::InvalidFormat(ref message)))
                if message.contains("exceeds remaining") && cursor.total_len().is_none() =>
            {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                if size < max_size {
                    size = std::cmp::min(max_size, size.saturating_mul(2));
                    continue;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let msg = format!("sequential fallback triggered: {}", message);
                    web_sys::console::debug_1(&JsValue::from_str(&msg));
                }
                match std::pin::Pin::from(Box::new(try_parse_large_vector_streaming_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    ref_paths,
                    progress,
                    visitor,
                    path,
                    emit,
                )))
                .await
                {
                    Ok(Some(control)) => return Ok(control),
                    Ok(None) => {}
                    Err(StreamingError::Parse(Error::StreamingStop)) => {
                        return Ok(StreamControl::Stop);
                    }
                    Err(err) => return Err(err),
                }
                return result;
            }
            Err(StreamingError::Parse(Error::InvalidFormat(ref message)))
                if message.contains("exceeds remaining") && size < max_size =>
            {
                ctx.restore(ctx_snapshot);
                ref_table.rollback(ref_checkpoint);
                symbol_table.rollback(symbol_checkpoint);
                dedup_table.rollback(dedup_checkpoint);
                size = std::cmp::min(max_size, size.saturating_mul(2));
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_altrep_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    progress: &mut StreamingProgressState<'_>,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    emit: bool,
    flags: u32,
) -> StreamingResult<StreamControl, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    let _ = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)?;
    let has_attr = (flags & HAS_ATTR_BIT) != 0;

    let mut emit_children = true;
    if emit {
        match visitor
            .on_object_start(path, sexp_type_name(ALTREP_SXP))
            .map_err(StreamingError::Visitor)?
        {
            VisitAction::Stop => {
                progress.report_object(cursor.position());
                return Ok(StreamControl::Stop);
            }
            VisitAction::Skip => emit_children = false,
            VisitAction::Continue => {}
        }
    }

    let _ = parse_object_streaming_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
    .await?;
    let _ = parse_object_streaming_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
    .await?;
    if has_attr {
        let prev = ctx.parsing_attributes;
        ctx.parsing_attributes = true;
        let _ = parse_object_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            false,
        )
        .await?;
        ctx.parsing_attributes = prev;
    }

    if emit && emit_children {
        visitor
            .on_object_end(path)
            .map_err(StreamingError::Visitor)?;
    }

    progress.report_object(cursor.position());
    Ok(StreamControl::Continue)
}

#[cfg(target_arch = "wasm32")]
async fn try_parse_large_vector_streaming_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    progress: &mut StreamingProgressState<'_>,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    emit: bool,
) -> StreamingResult<Option<StreamControl>, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    cursor
        .ensure_available(1)
        .await
        .map_err(StreamingError::Parse)?;
    let first_byte = cursor.as_sync_slice(1).map_err(StreamingError::Parse)?[0];
    if first_byte >= 240 {
        cursor.advance(1).map_err(StreamingError::Parse)?;
        if emit {
            visitor
                .on_object_start(path, "Null")
                .map_err(StreamingError::Visitor)?;
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }
        progress.report_object(cursor.position());
        return Ok(Some(StreamControl::Continue));
    }

    cursor
        .ensure_available(4)
        .await
        .map_err(StreamingError::Parse)?;
    let flags = cursor.peek_u32().map_err(StreamingError::Parse)?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() {
        let msg = format!(
            "sequential fallback sexp_type={} pos={} emit={}",
            sexp_type,
            cursor.position(),
            emit
        );
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }

    if sexp_type == S4SXP {
        let flags = read_u32_async(cursor)
            .await
            .map_err(StreamingError::Parse)?;
        let has_attr = (flags & HAS_ATTR_BIT) != 0;
        let raw_has_tag = (flags & HAS_TAG_BIT) != 0;
        let has_tag = raw_has_tag;
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!(
                "sequential fallback S4 flags=0x{:08x} has_attr={} has_tag={}",
                flags, has_attr, has_tag
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
            let source = if raw_has_tag {
                "tag"
            } else if has_attr {
                "attr"
            } else {
                "none"
            };
            let msg = format!("sequential fallback S4 attrs source={}", source);
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }

        let mut emit_children = true;
        if emit {
            match visitor
                .on_object_start(path, sexp_type_name(S4SXP))
                .map_err(StreamingError::Visitor)?
            {
                VisitAction::Stop => {
                    progress.report_object(cursor.position());
                    return Ok(Some(StreamControl::Stop));
                }
                VisitAction::Skip => emit_children = false,
                VisitAction::Continue => {}
            }
        }

        let mut attrs = Attributes::new();
        if has_tag {
            let prev = ctx.parsing_s4_tag;
            let prev_pairlist_root = ctx.parsing_pairlist_root;
            ctx.parsing_s4_tag = true;
            ctx.parsing_pairlist_root = true;
            let pairlist = std::pin::Pin::from(Box::new(parse_pairlist_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                true,
            )))
            .await
            .map_err(StreamingError::Parse)?;
            attrs = parse_attributes(RObject::Pairlist(pairlist), ctx)
                .map_err(StreamingError::Parse)?;
            ctx.parsing_s4_tag = prev;
            ctx.parsing_pairlist_root = prev_pairlist_root;
        }

        if has_attr {
            let prev_attrs = ctx.parsing_attributes;
            let prev_s4 = ctx.parsing_s4_tag;
            let prev_pairlist_root = ctx.parsing_pairlist_root;
            ctx.parsing_attributes = true;
            ctx.parsing_s4_tag = true;
            ctx.parsing_pairlist_root = true;
            let attr_obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await
            .map_err(StreamingError::Parse)?;
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() {
                let mut msg = format!(
                    "sequential fallback S4 attr_obj type={}",
                    object_type_name(&attr_obj)
                );
                if let RObject::Pairlist(ref elems) = attr_obj {
                    let tags: Vec<_> = elems
                        .iter()
                        .take(8)
                        .map(|e| e.tag.as_deref().unwrap_or("None"))
                        .collect();
                    msg.push_str(&format!(" pairlist_len={} tags={:?}", elems.len(), tags));
                } else if let RObject::Shared(ref shared) = attr_obj {
                    let inner = shared.read().unwrap();
                    msg.push_str(&format!(" shared_inner={}", object_type_name(&inner)));
                    if let RObject::Pairlist(ref elems) = *inner {
                        let tags: Vec<_> = elems
                            .iter()
                            .take(8)
                            .map(|e| e.tag.as_deref().unwrap_or("None"))
                            .collect();
                        msg.push_str(&format!(" pairlist_len={} tags={:?}", elems.len(), tags));
                    }
                }
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            let extra = parse_attributes(attr_obj, ctx).map_err(StreamingError::Parse)?;
            for (k, v) in extra.attrs.into_iter() {
                if !attrs.attrs.iter().any(|(ek, _)| ek == &k) {
                    attrs.insert(k, *v);
                }
            }
            ctx.parsing_attributes = prev_attrs;
            ctx.parsing_s4_tag = prev_s4;
            ctx.parsing_pairlist_root = prev_pairlist_root;
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let keys: Vec<_> = attrs.attrs.iter().map(|(k, _)| k.as_ref()).collect();
            let msg = format!(
                "sequential fallback S4 attrs keys={:?} has_tag={} has_attr={} raw_has_tag={} flags=0x{:08x}",
                keys, has_tag, has_attr, raw_has_tag, flags
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }

        #[cfg(target_arch = "wasm32")]
        if ctx.mode == crate::ParseMode::LazyMetadata && !ctx.s4_slot_overrides.is_empty() {
            for (key, value) in ctx.s4_slot_overrides.iter() {
                if attrs.get(key.as_ref()).is_none() {
                    attrs.insert(key.clone(), value.clone());
                }
            }
        }

        if emit {
            visitor
                .on_attributes(path, &attrs)
                .map_err(StreamingError::Visitor)?;
            if emit_children {
                emit_attribute_values_streaming(&attrs, path, visitor)?;
            }
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }

        progress.report_object(cursor.position());
        if ctx.stop_streaming {
            return Ok(Some(StreamControl::Stop));
        }
        return Ok(Some(StreamControl::Continue));
    }

    if matches!(sexp_type, LISTSXP | ATTRLISTSXP) {
        let flags = read_u32_async(cursor)
            .await
            .map_err(StreamingError::Parse)?;
        let has_tag = (flags & HAS_TAG_BIT) != 0;
        let has_attr = (flags & HAS_ATTR_BIT) != 0;

        let mut emit_children = true;
        if emit {
            match visitor
                .on_object_start(path, sexp_type_name(sexp_type))
                .map_err(StreamingError::Visitor)?
            {
                VisitAction::Stop => {
                    progress.report_object(cursor.position());
                    return Ok(Some(StreamControl::Stop));
                }
                VisitAction::Skip => emit_children = false,
                VisitAction::Continue => {}
            }
        }

        if has_attr {
            let prev = ctx.parsing_attributes;
            ctx.parsing_attributes = true;
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
            ctx.parsing_attributes = prev;
        }

        let control = parse_pairlist_streaming_sequential_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            has_tag,
            emit && emit_children,
            visitor,
            path,
            progress,
        )
        .await?;

        if emit && emit_children {
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }

        progress.report_object(cursor.position());
        return Ok(Some(control));
    }

    if matches!(sexp_type, LANGSXP | ATTRLANGSXP) {
        let flags = read_u32_async(cursor)
            .await
            .map_err(StreamingError::Parse)?;
        let has_tag = (flags & HAS_TAG_BIT) != 0;
        let has_attr = (flags & HAS_ATTR_BIT) != 0;

        let mut emit_children = true;
        if emit {
            match visitor
                .on_object_start(path, sexp_type_name(sexp_type))
                .map_err(StreamingError::Visitor)?
            {
                VisitAction::Stop => {
                    progress.report_object(cursor.position());
                    return Ok(Some(StreamControl::Stop));
                }
                VisitAction::Skip => emit_children = false,
                VisitAction::Continue => {}
            }
        }

        if has_attr {
            let prev = ctx.parsing_attributes;
            ctx.parsing_attributes = true;
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
            ctx.parsing_attributes = prev;
        }

        let control = parse_language_streaming_sequential_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            has_tag,
            emit && emit_children,
            visitor,
            path,
            progress,
        )
        .await?;

        if emit && emit_children {
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }

        progress.report_object(cursor.position());
        return Ok(Some(control));
    }

    if matches!(sexp_type, VECSXP | EXPRSXP) {
        let flags = read_u32_async(cursor)
            .await
            .map_err(StreamingError::Parse)?;
        let has_attr = (flags & HAS_ATTR_BIT) != 0;

        let mut emit_children = true;
        if emit {
            match visitor
                .on_object_start(path, sexp_type_name(sexp_type))
                .map_err(StreamingError::Visitor)?
            {
                VisitAction::Stop => {
                    progress.report_object(cursor.position());
                    return Ok(Some(StreamControl::Stop));
                }
                VisitAction::Skip => emit_children = false,
                VisitAction::Continue => {}
            }
        }

        let control = parse_list_streaming_sequential_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit && emit_children,
            visitor,
            path,
            progress,
        )
        .await?;

        if has_attr {
            let prev = ctx.parsing_attributes;
            ctx.parsing_attributes = true;
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
            ctx.parsing_attributes = prev;
        }

        if emit && emit_children {
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }

        progress.report_object(cursor.position());
        return Ok(Some(control));
    }

    let (kind, elem_size) = match sexp_type {
        INTSXP => (VectorKind::Integer, std::mem::size_of::<i32>()),
        REALSXP => (VectorKind::Real, std::mem::size_of::<f64>()),
        LGLSXP => (VectorKind::Logical, 4),
        RAWSXP => (VectorKind::Raw, std::mem::size_of::<u8>()),
        CPLXSXP => (VectorKind::Complex, std::mem::size_of::<Complex>()),
        _ => {
            #[cfg(target_arch = "wasm32")]
            {
                let msg = format!("sequential skip unsupported sexp_type={}", sexp_type);
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            return Ok(None);
        }
    };

    let flags = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)?;
    let has_attr = (flags & HAS_ATTR_BIT) != 0;

    let mut emit_children = true;
    if emit {
        match visitor
            .on_object_start(path, sexp_type_name(sexp_type))
            .map_err(StreamingError::Visitor)?
        {
            VisitAction::Stop => {
                progress.report_object(cursor.position());
                return Ok(Some(StreamControl::Stop));
            }
            VisitAction::Skip => emit_children = false,
            VisitAction::Continue => {}
        }
    }

    let length = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)? as usize;
    guard_allocation_common(ctx, length, elem_size, "vector")?;
    let offset = cursor.position();
    let byte_len = length
        .checked_mul(elem_size)
        .ok_or_else(|| Error::InvalidFormat("vector byte length overflow".to_string()))?;
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!(
            "sequential skip vector type={} len={} bytes={} offset={}",
            sexp_type, length, byte_len, offset
        );
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }

    if emit && emit_children {
        visitor
            .on_vector_metadata(path, kind, length)
            .map_err(StreamingError::Visitor)?;
        let span = LazyVector {
            length,
            offset,
            byte_len: byte_len as u64,
        };
        let _ = visitor
            .on_vector_chunk_available(path, span)
            .map_err(StreamingError::Visitor)?;
    }

    cursor
        .skip_bytes(byte_len)
        .await
        .map_err(StreamingError::Parse)?;

    if has_attr {
        let prev = ctx.parsing_attributes;
        ctx.parsing_attributes = true;
        let attr_obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await
        .map_err(StreamingError::Parse)?;
        ctx.parsing_attributes = prev;
        let attrs = parse_attributes(attr_obj, ctx).map_err(StreamingError::Parse)?;
        if emit {
            visitor
                .on_attributes(path, &attrs)
                .map_err(StreamingError::Visitor)?;
            if emit_children {
                emit_attribute_values_streaming(&attrs, path, visitor)?;
            }
        }
    }

    if emit {
        visitor
            .on_object_end(path)
            .map_err(StreamingError::Visitor)?;
    }

    progress.report_object(cursor.position());
    Ok(Some(StreamControl::Continue))
}

#[cfg(target_arch = "wasm32")]
async fn parse_with_sync_cursor_retry<T, F, C>(
    cursor: &mut C,
    initial: usize,
    max_size: usize,
    mut f: F,
) -> Result<T>
where
    F: FnMut(&mut RdsCursor<'_>) -> Result<T>,
    C: AsyncCursor,
{
    let mut size = initial.max(4);
    let max_size = max_size.max(4);

    loop {
        cursor.ensure_available(size).await?;
        let slice = cursor.as_sync_slice(size)?;
        let mut sync_cursor = RdsCursor::new_slice(slice);
        let result = f(&mut sync_cursor);

        match result {
            Ok(value) => {
                cursor.advance(sync_cursor.position())?;
                return Ok(value);
            }
            Err(Error::UnexpectedEof) | Err(Error::UnexpectedEofDetail { .. }) => {
                if size >= max_size {
                    return result;
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(Error::Io(ref err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                if size >= max_size {
                    return result;
                }
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(Error::InvalidFormat(message))
                if message.contains("exceeds remaining") && size < max_size =>
            {
                size = std::cmp::min(max_size, size.saturating_mul(2));
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn read_u32_async<C: AsyncCursor>(cursor: &mut C) -> Result<u32> {
    cursor.ensure_available(4).await?;
    let slice = cursor.as_sync_slice(4)?;
    let mut reader = std::io::Cursor::new(slice);
    let value = reader.read_u32::<BigEndian>()?;
    cursor.advance(4)?;
    Ok(value)
}

#[cfg(target_arch = "wasm32")]
async fn read_bytes_async<C: AsyncCursor>(cursor: &mut C, len: usize) -> Result<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    cursor.ensure_available(len).await?;
    let slice = cursor.as_sync_slice(len)?;
    let bytes = slice.to_vec();
    cursor.advance(len as u64)?;
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
async fn read_i32_async<C: AsyncCursor>(cursor: &mut C) -> Result<i32> {
    let bytes = read_bytes_async(cursor, 4).await?;
    let mut reader = std::io::Cursor::new(bytes);
    Ok(reader.read_i32::<BigEndian>()?)
}

#[cfg(target_arch = "wasm32")]
async fn read_f64_async<C: AsyncCursor>(cursor: &mut C) -> Result<f64> {
    let bytes = read_bytes_async(cursor, 8).await?;
    let mut reader = std::io::Cursor::new(bytes);
    Ok(reader.read_f64::<BigEndian>()?)
}

#[cfg(target_arch = "wasm32")]
async fn parse_object_sequential_value_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    cursor.ensure_available(1).await?;
    let first_byte = cursor.as_sync_slice(1)?[0];
    if first_byte >= 240 {
        cursor.advance(1)?;
        return Ok(RObject::Null);
    }

    let flags = read_u32_async(cursor).await?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };

    let has_attr = if sexp_type == REFSXP {
        false
    } else {
        (flags & HAS_ATTR_BIT) != 0
    };
    let has_tag = if sexp_type == REFSXP {
        false
    } else {
        (flags & HAS_TAG_BIT) != 0
    };

    if sexp_type == REFSXP {
        let ref_index = flags >> 8;
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() && ctx.parsing_s4_tag {
            let mut msg = format!("seq ref REFSXP idx={}", ref_index);
            if let Some(obj) = ref_table.get(ref_index) {
                let inner = obj.read().unwrap();
                msg.push_str(&format!(
                    " entry_type={:?}",
                    std::mem::discriminant(&*inner)
                ));
            } else {
                msg.push_str(" entry_type=missing");
            }
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        if let Some(obj) = ref_table.get(ref_index) {
            return Ok(RObject::Shared(obj));
        }
        return Err(Error::InvalidFormat(format!(
            "Invalid reference index: {}",
            ref_index
        )));
    }

    let track_reference = should_track_reference(sexp_type, has_attr);
    let ref_index = if track_reference && sexp_type != CLOSXP && !ctx.suppress_ref_tracking {
        let idx = ref_table.add(RObject::Null);
        Some(idx)
    } else {
        None
    };

    let early_attributes = if has_attr && matches!(sexp_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP)
    {
        let attr_obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        Some(parse_attributes(attr_obj, ctx)?)
    } else {
        None
    };

    if sexp_type == ALTREP_SXP {
        let class_info = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        let state = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        let attributes_obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;

        let native_obj = convert_altrep_to_native(ctx, class_info, state)?;
        let final_obj = if !matches!(attributes_obj, RObject::Null) {
            let attrs = parse_attributes(attributes_obj, ctx)?;
            if !attrs.is_empty() {
                RObject::WithAttributes {
                    object: Box::new(native_obj),
                    attributes: attrs,
                }
            } else {
                native_obj
            }
        } else {
            native_obj
        };

        if let Some(index) = ref_index {
            ref_table.update(index, final_obj);
            return Ok(RObject::Shared(ref_table.get(index).ok_or_else(|| {
                Error::InvalidFormat(format!("Missing ref idx {}", index))
            })?));
        }

        return Ok(final_obj);
    }

    let mut obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        SYMSXP => {
            let name_flags = read_u32_async(cursor).await?;
            let name_type_from_0_7 = name_flags & 0xFF;
            let name_type_from_8_15 = (name_flags >> 8) & 0xFF;
            let name = if name_type_from_0_7 == REFSXP {
                let ref_index = name_flags >> 8;
                if let Some(obj) = ref_table.get(ref_index) {
                    extract_tag_name(obj.read().unwrap().clone()).unwrap_or_else(|| Arc::from("NA"))
                } else {
                    Arc::from("NA")
                }
            } else if name_type_from_0_7 == CHARSXP || name_type_from_8_15 == CHARSXP {
                Arc::from(
                    parse_charsxp_content_async(ctx, cursor, name_flags)
                        .await?
                        .as_str(),
                )
            } else {
                Arc::from("NA")
            };
            let symbol = RObject::Symbol(name);
            symbol_table.add(symbol.clone());
            symbol
        }
        CHARSXP => {
            let string = parse_charsxp_content_async(ctx, cursor, flags).await?;
            RObject::Character(vec![Arc::from(string.as_str())].into())
        }
        STRSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, 1, "character vector")?;

            let should_skip = matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold();
            if should_skip {
                let offset = cursor.position();
                let start_pos = cursor.position();
                for _ in 0..length {
                    let elem_flags = read_u32_async(cursor).await?;
                    let elem_type = elem_flags & 0xFF;
                    let elem_type_alt = (elem_flags >> 8) & 0xFF;
                    if elem_type == REFSXP {
                        continue;
                    }
                    if elem_type == SYMSXP {
                        let name_flags = read_u32_async(cursor).await?;
                        let name_type = name_flags & 0xFF;
                        let name_type_alt = (name_flags >> 8) & 0xFF;
                        if name_type == REFSXP {
                            continue;
                        }
                        if name_type == CHARSXP || name_type_alt == CHARSXP {
                            let str_len = read_i32_async(cursor).await?;
                            if str_len >= 0 {
                                cursor.skip_bytes(str_len as usize).await?;
                            }
                            continue;
                        }
                        return Err(Error::InvalidFormat(
                            "non-CHARSXP element in character vector".to_string(),
                        ));
                    }
                    if elem_type == CHARSXP || elem_type_alt == CHARSXP {
                        let str_len = read_i32_async(cursor).await?;
                        if str_len >= 0 {
                            cursor.skip_bytes(str_len as usize).await?;
                        }
                        continue;
                    }
                    return Err(Error::InvalidFormat(
                        "non-CHARSXP element in character vector".to_string(),
                    ));
                }

                return Ok(RObject::Character(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: cursor.position().saturating_sub(start_pos),
                })));
            }

            #[cfg(target_arch = "wasm32")]
            log_large_alloc(ctx, "strsxp", length);
            let mut vec = Vec::with_capacity(length);
            let mut string_cache: Vec<Arc<str>> = Vec::new();
            for _ in 0..length {
                let elem_flags = read_u32_async(cursor).await?;
                let elem_type = elem_flags & 0xFF;
                let elem_type_alt = (elem_flags >> 8) & 0xFF;
                let value = if elem_type == REFSXP {
                    let ref_index = (elem_flags >> 8) as usize;
                    string_cache
                        .get(ref_index.saturating_sub(1))
                        .cloned()
                        .unwrap_or_else(|| Arc::from("NA"))
                } else if elem_type == SYMSXP {
                    let name_flags = read_u32_async(cursor).await?;
                    let name_type = name_flags & 0xFF;
                    let name_type_alt = (name_flags >> 8) & 0xFF;
                    if name_type == REFSXP {
                        let ref_index = (name_flags >> 8) as usize;
                        string_cache
                            .get(ref_index.saturating_sub(1))
                            .cloned()
                            .unwrap_or_else(|| Arc::from("NA"))
                    } else if name_type == CHARSXP || name_type_alt == CHARSXP {
                        Arc::from(
                            parse_charsxp_content_async(ctx, cursor, name_flags)
                                .await?
                                .as_str(),
                        )
                    } else {
                        Arc::from("NA")
                    }
                } else if elem_type == CHARSXP || elem_type_alt == CHARSXP {
                    Arc::from(
                        parse_charsxp_content_async(ctx, cursor, elem_flags)
                            .await?
                            .as_str(),
                    )
                } else {
                    Arc::from("NA")
                };
                string_cache.push(value.clone());
                vec.push(value);
            }
            RObject::Character(vec.into())
        }
        INTSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, std::mem::size_of::<i32>(), "int vector")?;
            let offset = cursor.position();
            let byte_len = length * std::mem::size_of::<i32>();
            if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
            {
                cursor.skip_bytes(byte_len).await?;
                RObject::Integer(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: byte_len as u64,
                }))
            } else {
                let bytes = read_bytes_async(cursor, byte_len).await?;
                let mut values = Vec::with_capacity(length);
                for chunk in bytes.chunks_exact(4) {
                    let mut reader = std::io::Cursor::new(chunk);
                    values.push(reader.read_i32::<BigEndian>()?);
                }
                RObject::Integer(values.into())
            }
        }
        REALSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, std::mem::size_of::<f64>(), "real vector")?;
            let offset = cursor.position();
            let byte_len = length * std::mem::size_of::<f64>();
            if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
            {
                cursor.skip_bytes(byte_len).await?;
                RObject::Real(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: byte_len as u64,
                }))
            } else {
                let bytes = read_bytes_async(cursor, byte_len).await?;
                let mut values = Vec::with_capacity(length);
                for chunk in bytes.chunks_exact(8) {
                    let mut reader = std::io::Cursor::new(chunk);
                    values.push(reader.read_f64::<BigEndian>()?);
                }
                RObject::Real(values.into())
            }
        }
        LGLSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, 4, "logical vector")?;
            let offset = cursor.position();
            let byte_len = length * 4;
            if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
            {
                cursor.skip_bytes(byte_len).await?;
                RObject::Logical(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: byte_len as u64,
                }))
            } else {
                let bytes = read_bytes_async(cursor, byte_len).await?;
                let mut values = Vec::with_capacity(length);
                for chunk in bytes.chunks_exact(4) {
                    let mut reader = std::io::Cursor::new(chunk);
                    let value = reader.read_i32::<BigEndian>()?;
                    let logical = match value {
                        1 => Logical::True,
                        0 => Logical::False,
                        i32::MIN => Logical::Na,
                        _ => Logical::Na,
                    };
                    values.push(logical);
                }
                RObject::Logical(values.into())
            }
        }
        RAWSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, 1, "raw vector")?;
            let offset = cursor.position();
            let byte_len = length;
            if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
            {
                cursor.skip_bytes(byte_len).await?;
                RObject::Raw(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: byte_len as u64,
                }))
            } else {
                let bytes = read_bytes_async(cursor, byte_len).await?;
                RObject::Raw(bytes.into())
            }
        }
        CPLXSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(
                ctx,
                length,
                std::mem::size_of::<Complex>(),
                "complex vector",
            )?;
            let offset = cursor.position();
            let byte_len = length * std::mem::size_of::<Complex>();
            if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
            {
                cursor.skip_bytes(byte_len).await?;
                RObject::Complex(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: byte_len as u64,
                }))
            } else {
                let bytes = read_bytes_async(cursor, byte_len).await?;
                let mut values = Vec::with_capacity(length);
                for chunk in bytes.chunks_exact(16) {
                    let mut reader = std::io::Cursor::new(chunk);
                    let real = reader.read_f64::<BigEndian>()?;
                    let imaginary = reader.read_f64::<BigEndian>()?;
                    values.push(Complex { real, imaginary });
                }
                RObject::Complex(values.into())
            }
        }
        VECSXP | EXPRSXP => {
            let start_pos = cursor.position();
            let length = read_u32_async(cursor).await? as usize;
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() {
                let msg = format!(
                    "seq vecsxp start pos={} length={} parsing_s4_tag={}",
                    start_pos, length, ctx.parsing_s4_tag
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() && ctx.parsing_s4_tag {
                let msg = format!("seq vecsxp start pos={} length={}", start_pos, length);
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            #[cfg(target_arch = "wasm32")]
            if sequential_list_debug_enabled() {
                let msg = format!(
                    "seq vecsxp start pos={} length={} parsing_s4_tag={}",
                    start_pos, length, ctx.parsing_s4_tag
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            guard_allocation_common(ctx, length, 1, "list")?;

            let allow_force_materialize =
                ctx.force_materialize_vector && length <= MAX_FORCE_MATERIALIZE_VECTOR_LEN;
            let list_obj = if matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                && length > ctx.effective_lazy_threshold()
                && !allow_force_materialize
            {
                for _ in 0..length {
                    let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                        ctx,
                        cursor,
                        ref_table,
                        symbol_table,
                        dedup_table,
                    )))
                    .await?;
                }

                if sexp_type == VECSXP {
                    RObject::List(Vec::new())
                } else {
                    RObject::Expression(Vec::new())
                }
            } else {
                #[cfg(target_arch = "wasm32")]
                log_large_alloc(ctx, "vecsxp", length);
                let mut values = Vec::with_capacity(length);
                for idx in 0..length {
                    let value = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                        ctx,
                        cursor,
                        ref_table,
                        symbol_table,
                        dedup_table,
                    )))
                    .await?;
                    #[cfg(target_arch = "wasm32")]
                    if sequential_list_debug_enabled() && idx < 3 {
                        let msg = format!(
                            "seq vecsxp elem idx={} type={:?} pos={}",
                            idx,
                            std::mem::discriminant(&value),
                            cursor.position()
                        );
                        web_sys::console::debug_1(&JsValue::from_str(&msg));
                    }
                    values.push(value);
                }
                if sexp_type == VECSXP {
                    RObject::List(values)
                } else {
                    RObject::Expression(values)
                }
            };
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() && ctx.parsing_s4_tag {
                let end_pos = cursor.position();
                let msg = format!(
                    "seq vecsxp end pos={} delta={}",
                    end_pos,
                    end_pos.saturating_sub(start_pos)
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            #[cfg(target_arch = "wasm32")]
            if sequential_list_debug_enabled() {
                let end_pos = cursor.position();
                let msg = format!(
                    "seq vecsxp end pos={} delta={}",
                    end_pos,
                    end_pos.saturating_sub(start_pos)
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            list_obj
        }
        LISTSXP | ATTRLISTSXP | LANGSXP | ATTRLANGSXP => {
            if ctx.lenient_skip_vectors {
                std::pin::Pin::from(Box::new(skip_pairlist_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    has_tag,
                )))
                .await?;
                if sexp_type == LANGSXP || sexp_type == ATTRLANGSXP {
                    RObject::Language {
                        function: Box::new(RObject::Null),
                        args: Vec::new(),
                    }
                } else {
                    RObject::Pairlist(Vec::new())
                }
            } else {
                let list = std::pin::Pin::from(Box::new(parse_pairlist_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    has_tag,
                )))
                .await?;
                if sexp_type == LANGSXP || sexp_type == ATTRLANGSXP {
                    let (function, args) = if list.is_empty() {
                        (RObject::Null, Vec::new())
                    } else {
                        (list[0].value.clone(), list[1..].to_vec())
                    };
                    RObject::Language {
                        function: Box::new(function),
                        args,
                    }
                } else {
                    RObject::Pairlist(list)
                }
            }
        }
        S4SXP => RObject::Null,
        _ => RObject::Null,
    };

    let mut attributes = if sexp_type == S4SXP && has_tag {
        let prev = ctx.parsing_s4_tag;
        let prev_pairlist_root = ctx.parsing_pairlist_root;
        ctx.parsing_s4_tag = true;
        ctx.parsing_pairlist_root = true;
        let pairlist = std::pin::Pin::from(Box::new(parse_pairlist_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            true,
        )))
        .await?;
        ctx.parsing_s4_tag = prev;
        ctx.parsing_pairlist_root = prev_pairlist_root;
        parse_attributes(RObject::Pairlist(pairlist), ctx)?
    } else if let Some(attrs) = early_attributes {
        attrs
    } else if has_attr {
        let prev = ctx.parsing_s4_tag;
        let prev_pairlist_root = ctx.parsing_pairlist_root;
        if sexp_type == S4SXP {
            ctx.parsing_s4_tag = true;
            ctx.parsing_pairlist_root = true;
        }
        let attr_obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() && sexp_type == S4SXP {
            let mut msg = format!("seq value S4 attr_obj type={}", object_type_name(&attr_obj));
            if let RObject::Pairlist(ref elems) = attr_obj {
                let tags: Vec<_> = elems
                    .iter()
                    .take(8)
                    .map(|e| e.tag.as_deref().unwrap_or("None"))
                    .collect();
                msg.push_str(&format!(" pairlist_len={} tags={:?}", elems.len(), tags));
            } else if let RObject::Shared(ref shared) = attr_obj {
                let inner = shared.read().unwrap();
                msg.push_str(&format!(" shared_inner={}", object_type_name(&inner)));
                if let RObject::Pairlist(ref elems) = *inner {
                    let tags: Vec<_> = elems
                        .iter()
                        .take(8)
                        .map(|e| e.tag.as_deref().unwrap_or("None"))
                        .collect();
                    msg.push_str(&format!(" pairlist_len={} tags={:?}", elems.len(), tags));
                }
            }
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        if sexp_type == S4SXP {
            ctx.parsing_s4_tag = prev;
            ctx.parsing_pairlist_root = prev_pairlist_root;
        }
        parse_attributes(attr_obj, ctx)?
    } else {
        Attributes::new()
    };

    if sexp_type == S4SXP && !attributes.is_empty() {
        #[cfg(target_arch = "wasm32")]
        if ctx.mode == crate::ParseMode::LazyMetadata && !ctx.s4_slot_overrides.is_empty() {
            for (key, value) in ctx.s4_slot_overrides.iter() {
                if attributes.get(key.as_ref()).is_none() {
                    attributes.insert(key.clone(), value.clone());
                }
            }
        }
        obj = convert_to_s4_object(attributes);
    } else if has_attr && !attributes.is_empty() {
        obj = RObject::WithAttributes {
            object: Box::new(obj),
            attributes,
        };
    }

    if let Some(index) = ref_index {
        ref_table.update(index, obj.clone());
        obj = RObject::Shared(
            ref_table
                .get(index)
                .ok_or_else(|| Error::InvalidFormat(format!("Missing ref idx {}", index)))?,
        );
    }

    Ok(obj)
}

#[cfg(target_arch = "wasm32")]
async fn parse_tag_sequential_value_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<(Option<Arc<str>>, Option<RObject>)> {
    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() {
        let msg = format!("seq tag parse start pos={}", cursor.position());
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }
    let flags = read_u32_async(cursor).await?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() {
        let msg = format!(
            "seq tag flags=0x{:08x} type0_7={} type8_15={} sexp_type={}",
            flags, type_from_0_7, type_from_8_15, sexp_type
        );
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }

    if sexp_type == REFSXP {
        let ref_index = flags >> 8;
        let tag_obj = if let Some(sym) = symbol_table.get(ref_index) {
            sym.clone()
        } else if let Some(obj) = ref_table.get(ref_index) {
            obj.read()
                .map_err(|_| Error::Unsupported("shared object lock poisoned".to_string()))?
                .clone()
        } else {
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() {
                let msg = format!(
                    "seq tag REFSXP missing index={} symbol_table_len={} ref_table_len={}",
                    ref_index,
                    symbol_table.len(),
                    ref_table.next_index - 1
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            return Err(Error::InvalidFormat(format!(
                "Invalid TAG REFSXP index {} (symbol table size={}, ref table size={})",
                ref_index,
                symbol_table.len(),
                ref_table.next_index - 1
            )));
        };
        return Ok((extract_tag_name(tag_obj.clone()), Some(tag_obj)));
    }

    if sexp_type == SYMSXP {
        let ref_index = if ctx.suppress_ref_tracking {
            None
        } else {
            Some(ref_table.add(RObject::Null))
        };
        let name_flags = read_u32_async(cursor).await?;
        let name_type_from_0_7 = name_flags & 0xFF;
        let name_type_from_8_15 = (name_flags >> 8) & 0xFF;
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!(
                "seq tag SYMSXP name flags=0x{:08x} type0_7={} type8_15={}",
                name_flags, name_type_from_0_7, name_type_from_8_15
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        let name = if name_type_from_0_7 == REFSXP {
            let ref_index = name_flags >> 8;
            if let Some(sym) = symbol_table.get(ref_index) {
                extract_tag_name(sym.clone()).unwrap_or_else(|| Arc::from("NA"))
            } else if let Some(obj) = ref_table.get(ref_index) {
                extract_tag_name(obj.read().unwrap().clone()).unwrap_or_else(|| Arc::from("NA"))
            } else {
                Arc::from("NA")
            }
        } else if name_type_from_0_7 == CHARSXP || name_type_from_8_15 == CHARSXP {
            Arc::from(
                parse_charsxp_content_async(ctx, cursor, name_flags)
                    .await?
                    .as_str(),
            )
        } else {
            Arc::from("NA")
        };
        let symbol = RObject::Symbol(name.clone());
        symbol_table.add(symbol.clone());
        if let Some(index) = ref_index {
            ref_table.update(index, symbol.clone());
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!("seq tag SYMSXP resolved='{}'", name);
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        return Ok((Some(name), Some(symbol)));
    }

    if sexp_type == CHARSXP {
        let name = parse_charsxp_content_async(ctx, cursor, flags).await?;
        let obj = RObject::Character(vec![Arc::from(name.as_str())].into());
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!("seq tag CHARSXP resolved='{}'", name);
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        return Ok((extract_tag_name(obj.clone()), Some(obj)));
    }

    let obj = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
    )))
    .await?;
    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() {
        let msg = format!(
            "seq tag fallback obj type={:?}",
            std::mem::discriminant(&obj)
        );
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }
    Ok((extract_tag_name(obj.clone()), Some(obj)))
}

#[cfg(target_arch = "wasm32")]
async fn parse_pairlist_sequential_value_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    has_tag: bool,
) -> Result<Vec<PairlistElement>> {
    let pairlist_ref_index = if ctx.parsing_pairlist_root && !ctx.suppress_ref_tracking {
        Some(ref_table.add(RObject::Null))
    } else {
        None
    };
    let mut elements = Vec::new();
    let mut has_tag_current = has_tag;
    let force_s4_tag = ctx.parsing_s4_tag;
    #[cfg(target_arch = "wasm32")]
    let stop_tags = ctx.s4_slot_policy.stop_after_slots.clone();
    #[cfg(target_arch = "wasm32")]
    let mut stop_seen = vec![false; stop_tags.len()];
    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() && force_s4_tag {
        let msg = format!("seq pairlist S4 tag parse start pos={}", cursor.position());
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }

    loop {
        let (tag, tag_obj) = if has_tag_current || force_s4_tag {
            std::pin::Pin::from(Box::new(parse_tag_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?
        } else {
            (None, None)
        };
        #[cfg(target_arch = "wasm32")]
        if force_s4_tag
            && ctx.parsing_pairlist_root
            && ctx.mode == crate::ParseMode::LazyMetadata
            && !stop_tags.is_empty()
        {
            if let Some(tag_name) = tag.as_deref() {
                if let Some(index) = stop_tags
                    .iter()
                    .position(|entry| entry.as_ref() == tag_name)
                {
                    stop_seen[index] = true;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let skip_slot = ctx.mode == crate::ParseMode::LazyMetadata
            && force_s4_tag
            && ctx.parsing_pairlist_root
            && should_skip_s4_slot(ctx, &tag);
        #[cfg(target_arch = "wasm32")]
        let keep_slot = if skip_slot {
            false
        } else if ctx.mode == crate::ParseMode::LazyMetadata
            && force_s4_tag
            && ctx.parsing_pairlist_root
        {
            should_keep_s4_slot(ctx, &tag)
        } else {
            true
        };
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!(
                "seq pairlist element tag={:?} pos={}",
                tag,
                cursor.position()
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() && tag.is_none() {
            if let Some(ref obj) = tag_obj {
                let msg = format!(
                    "seq pairlist tag is None; tag_obj type={}",
                    object_type_name(obj)
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
                if let RObject::Shared(ref shared) = obj {
                    let inner = shared.read().unwrap();
                    let msg = format!(
                        "seq pairlist tag_obj shared_inner={}",
                        object_type_name(&inner)
                    );
                    web_sys::console::debug_1(&JsValue::from_str(&msg));
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        if ctx.parsing_pairlist_root && ctx.mode == crate::ParseMode::LazyMetadata && skip_slot {
            if sequential_debug_enabled() {
                let msg = format!("seq pairlist skipping S4 slot tag={:?}", tag);
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() && force_s4_tag {
            let pos = cursor.position();
            let mut msg = format!("seq pairlist S4 value peek pos={}", pos);
            if cursor.ensure_available(1).await.is_ok() {
                if let Ok(slice) = cursor.as_sync_slice(1) {
                    let first_byte = slice[0];
                    msg.push_str(&format!(" first_byte=0x{:02x}", first_byte));
                    if first_byte < 240 && cursor.ensure_available(4).await.is_ok() {
                        if let Ok(flags_slice) = cursor.as_sync_slice(4) {
                            let mut reader = std::io::Cursor::new(flags_slice);
                            if let Ok(flags) = reader.read_u32::<BigEndian>() {
                                let type_from_8_15 = (flags >> 8) & 0xFF;
                                let type_from_0_7 = flags & 0xFF;
                                let sexp_type = if type_from_0_7 == REFSXP
                                    || (2..=S4SXP).contains(&type_from_0_7)
                                    || type_from_0_7 == 1
                                {
                                    type_from_0_7
                                } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
                                    type_from_8_15
                                } else {
                                    type_from_0_7
                                };
                                let has_attr = (flags & HAS_ATTR_BIT) != 0;
                                let has_tag_val = (flags & HAS_TAG_BIT) != 0;
                                msg.push_str(&format!(
                                    " flags=0x{:08x} sexp_type={} has_attr={} has_tag={}",
                                    flags, sexp_type, has_attr, has_tag_val
                                ));
                            }
                        }
                    }
                }
            }
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        let prev_materialize = ctx.force_materialize_vector;
        let prev_lenient_skip = ctx.lenient_skip_vectors;
        if should_materialize_vector_tag(ctx, &tag) {
            ctx.force_materialize_vector = true;
        }
        if skip_slot {
            ctx.lenient_skip_vectors = true;
        }
        let value_start = cursor.position();
        let prev_s4_tag = ctx.parsing_s4_tag;
        let prev_pairlist_root = ctx.parsing_pairlist_root;
        if force_s4_tag {
            ctx.parsing_s4_tag = false;
        }
        if force_s4_tag && prev_pairlist_root {
            ctx.parsing_pairlist_root = false;
        }
        let value = if ctx.mode == crate::ParseMode::LazyMetadata && skip_slot {
            std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?
        } else if force_s4_tag && ctx.mode == crate::ParseMode::LazyMetadata {
            if keep_slot {
                std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                )))
                .await?
            } else {
                std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                )))
                .await?
            }
        } else {
            std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?
        };
        if force_s4_tag {
            ctx.parsing_s4_tag = prev_s4_tag;
        }
        if force_s4_tag && prev_pairlist_root {
            ctx.parsing_pairlist_root = prev_pairlist_root;
        }
        ctx.force_materialize_vector = prev_materialize;
        ctx.lenient_skip_vectors = prev_lenient_skip;
        #[cfg(target_arch = "wasm32")]
        if ctx.mode == crate::ParseMode::LazyMetadata {
            let capture_any = should_capture_any_slot(ctx, &tag);
            let capture_slot =
                force_s4_tag && ctx.parsing_pairlist_root && should_capture_s4_slot(ctx, &tag);
            if capture_any || capture_slot {
                if let Some(name) = tag.as_deref() {
                    ctx.s4_slot_overrides
                        .entry(Arc::from(name))
                        .or_insert_with(|| value.clone());
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let debug_tag_name = tag.as_deref().unwrap_or("None").to_string();
        #[cfg(target_arch = "wasm32")]
        let debug_value_type = std::mem::discriminant(&value);
        if keep_slot {
            elements.push(PairlistElement {
                tag,
                value,
                tag_object: tag_obj.map(Box::new),
            });
        } else {
            // Drop the element entirely when it is explicitly skipped or not required.
        }
        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() && force_s4_tag {
            let value_end = cursor.position();
            let value_delta = value_end.saturating_sub(value_start);
            let msg = format!(
                "seq pairlist S4 tag element tag='{}' value_type={:?} pos={} delta={}",
                debug_tag_name, debug_value_type, value_end, value_delta
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        #[cfg(target_arch = "wasm32")]
        if force_s4_tag
            && ctx.parsing_pairlist_root
            && ctx.mode == crate::ParseMode::LazyMetadata
            && !stop_tags.is_empty()
            && stop_seen.iter().all(|seen| *seen)
        {
            if sequential_debug_enabled() {
                let msg = "seq pairlist S4 root slots complete; stopping early";
                web_sys::console::debug_1(&JsValue::from_str(msg));
            }
            ctx.stop_streaming = true;
            break;
        }
        cursor.ensure_available(1).await?;
        let first_byte = cursor.as_sync_slice(1)?[0];
        if first_byte >= 240 {
            let _ = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            break;
        }
        cursor.ensure_available(4).await?;
        let slice = cursor.as_sync_slice(4)?;
        let mut reader = std::io::Cursor::new(slice);
        let flags = reader.read_u32::<BigEndian>()?;
        let type_from_8_15 = (flags >> 8) & 0xFF;
        let type_from_0_7 = flags & 0xFF;
        let next_type = if type_from_0_7 == REFSXP
            || (2..=S4SXP).contains(&type_from_0_7)
            || type_from_0_7 == 1
        {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
        let has_tag_next = (flags & HAS_TAG_BIT) != 0;
        let is_tagless_s4_next =
            force_s4_tag && ctx.parsing_pairlist_root && matches!(next_type, SYMSXP | CHARSXP);
        let continues_pairlist = has_tag_next
            || matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP)
            || is_tagless_s4_next;

        #[cfg(target_arch = "wasm32")]
        if sequential_debug_enabled() {
            let msg = format!(
                "seq pairlist next flags=0x{:08x} next_type={} has_tag_next={} continues={}",
                flags, next_type, has_tag_next, continues_pairlist
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }

        if next_type == REFSXP {
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() && force_s4_tag {
                let ref_index = flags >> 8;
                let mut msg = format!("seq pairlist next REFSXP idx={}", ref_index);
                if let Some(obj) = ref_table.get(ref_index) {
                    let inner = obj.read().unwrap();
                    msg.push_str(&format!(
                        " entry_type={:?}",
                        std::mem::discriminant(&*inner)
                    ));
                } else {
                    msg.push_str(" entry_type=missing");
                }
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            let referenced = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            match referenced.into_concrete() {
                RObject::Null => break,
                RObject::Pairlist(mut tail) => {
                    elements.append(&mut tail);
                    break;
                }
                RObject::Shared(inner) => {
                    let inner_obj = inner.read().unwrap().clone();
                    if let RObject::Pairlist(mut tail) = inner_obj {
                        elements.append(&mut tail);
                    }
                    break;
                }
                _ => break,
            }
        } else if continues_pairlist {
            if !is_tagless_s4_next {
                // Consume the CDR flags so the next loop iteration reads the tag/value.
                let _ = read_u32_async(cursor).await?;
            }
            has_tag_current = has_tag_next || is_tagless_s4_next;
            continue;
        } else {
            let referenced = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            match referenced.into_concrete() {
                RObject::Null => break,
                RObject::Pairlist(mut tail) => {
                    elements.append(&mut tail);
                    break;
                }
                RObject::Shared(inner) => {
                    let inner_obj = inner.read().unwrap().clone();
                    if let RObject::Pairlist(mut tail) = inner_obj {
                        elements.append(&mut tail);
                    }
                    break;
                }
                other => {
                    elements.push(PairlistElement {
                        tag: None,
                        value: other,
                        tag_object: None,
                    });
                    break;
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    if sequential_debug_enabled() && force_s4_tag {
        let tags: Vec<_> = elements
            .iter()
            .map(|elem| elem.tag.as_deref().unwrap_or("None"))
            .collect();
        let msg = format!("seq pairlist S4 tag parse end tags={:?}", tags);
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }
    if let Some(ref_index) = pairlist_ref_index {
        ref_table.update(ref_index, RObject::Pairlist(elements.clone()));
    }
    Ok(elements)
}

#[cfg(target_arch = "wasm32")]
async fn skip_pairlist_sequential_value_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    has_tag: bool,
) -> Result<()> {
    let mut has_tag_current = has_tag;
    loop {
        let mut tag_name: Option<Arc<str>> = None;
        if has_tag_current {
            let (tag, _tag_obj) = std::pin::Pin::from(Box::new(parse_tag_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            tag_name = tag;
        }

        if ctx.mode == crate::ParseMode::LazyMetadata {
            if should_capture_any_slot(ctx, &tag_name) {
                let prev_materialize = ctx.force_materialize_vector;
                let prev_mode = ctx.mode.clone();
                ctx.force_materialize_vector = true;
                ctx.mode = crate::ParseMode::Full;
                let value = std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                )))
                .await?;
                ctx.mode = prev_mode;
                ctx.force_materialize_vector = prev_materialize;
                if let Some(name) = tag_name.as_deref() {
                    ctx.s4_slot_overrides
                        .entry(Arc::from(name))
                        .or_insert_with(|| value.clone());
                }
            } else {
                let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                )))
                .await?;
            }
        } else {
            let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
        }

        cursor.ensure_available(1).await?;
        let first_byte = cursor.as_sync_slice(1)?[0];
        if first_byte >= 240 {
            let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            break;
        }

        cursor.ensure_available(4).await?;
        let slice = cursor.as_sync_slice(4)?;
        let mut reader = std::io::Cursor::new(slice);
        let flags = reader.read_u32::<BigEndian>()?;
        let type_from_8_15 = (flags >> 8) & 0xFF;
        let type_from_0_7 = flags & 0xFF;
        let next_type = if type_from_0_7 == REFSXP
            || (2..=S4SXP).contains(&type_from_0_7)
            || type_from_0_7 == 1
        {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
        let has_tag_next = (flags & HAS_TAG_BIT) != 0;
        let continues_pairlist =
            has_tag_next || matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP);

        if next_type == REFSXP {
            let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            break;
        } else if continues_pairlist {
            let _ = read_u32_async(cursor).await?;
            has_tag_current = has_tag_next;
            continue;
        } else {
            let _ = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?;
            break;
        }
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn skip_object_sequential_value_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    cursor.ensure_available(1).await?;
    let first_byte = cursor.as_sync_slice(1)?[0];
    if first_byte >= 240 {
        cursor.advance(1)?;
        return Ok(RObject::Null);
    }

    let flags = read_u32_async(cursor).await?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };

    if sexp_type == REFSXP {
        let ref_index = flags >> 8;
        if let Some(obj) = ref_table.get(ref_index) {
            return Ok(RObject::Shared(obj));
        }
        return Err(Error::InvalidFormat(format!(
            "Invalid reference index: {}",
            ref_index
        )));
    }

    let has_attr = (flags & HAS_ATTR_BIT) != 0;
    let has_tag = (flags & HAS_TAG_BIT) != 0;
    let track_reference = should_track_reference(sexp_type, has_attr);
    let ref_index = if track_reference && sexp_type != CLOSXP && !ctx.suppress_ref_tracking {
        let idx = ref_table.add(RObject::Null);
        Some(idx)
    } else {
        None
    };

    let early_attributes = if has_attr && matches!(sexp_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP)
    {
        let attr_obj = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        Some(parse_attributes(attr_obj, ctx)?)
    } else {
        None
    };

    if sexp_type == ALTREP_SXP {
        let class_info = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        let state = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;
        let attributes_obj = std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )))
        .await?;

        let native_obj = convert_altrep_to_native(ctx, class_info, state)?;
        let final_obj = if !matches!(attributes_obj, RObject::Null) {
            let attrs = parse_attributes(attributes_obj, ctx)?;
            if !attrs.is_empty() {
                RObject::WithAttributes {
                    object: Box::new(native_obj),
                    attributes: attrs,
                }
            } else {
                native_obj
            }
        } else {
            native_obj
        };

        if let Some(index) = ref_index {
            ref_table.update(index, final_obj.clone());
            return Ok(RObject::Shared(ref_table.get(index).ok_or_else(|| {
                Error::InvalidFormat(format!("Missing ref idx {}", index))
            })?));
        }

        return Ok(final_obj);
    }

    let mut obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        SYMSXP => {
            let name_flags = read_u32_async(cursor).await?;
            let name_type_from_0_7 = name_flags & 0xFF;
            let name_type_from_8_15 = (name_flags >> 8) & 0xFF;
            let name = if name_type_from_0_7 == REFSXP {
                let ref_index = name_flags >> 8;
                if let Some(sym) = symbol_table.get(ref_index) {
                    extract_tag_name(sym.clone()).unwrap_or_else(|| Arc::from("NA"))
                } else if let Some(obj) = ref_table.get(ref_index) {
                    extract_tag_name(obj.read().unwrap().clone()).unwrap_or_else(|| Arc::from("NA"))
                } else {
                    Arc::from("NA")
                }
            } else if name_type_from_0_7 == CHARSXP || name_type_from_8_15 == CHARSXP {
                Arc::from(
                    parse_charsxp_content_async(ctx, cursor, name_flags)
                        .await?
                        .as_str(),
                )
            } else {
                Arc::from("NA")
            };
            let symbol = RObject::Symbol(name);
            symbol_table.add(symbol.clone());
            symbol
        }
        CHARSXP => {
            let string = parse_charsxp_content_async(ctx, cursor, flags).await?;
            RObject::Character(vec![Arc::from(string.as_str())].into())
        }
        STRSXP => {
            let length = read_u32_async(cursor).await? as usize;
            guard_allocation_common(ctx, length, 1, "character vector")?;

            let allow_force_materialize_strings =
                ctx.force_materialize_vector && length <= MAX_FORCE_MATERIALIZE_STRING_LEN;
            #[cfg(target_arch = "wasm32")]
            if length >= 10_000 {
                let should_skip = matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                    && length > ctx.effective_lazy_threshold();
                let msg = format!(
                    "seq strsxp len={} lazy_thresh={} allow_force={} should_skip={}",
                    length,
                    ctx.effective_lazy_threshold(),
                    allow_force_materialize_strings,
                    should_skip
                );
                web_sys::console::warn_1(&JsValue::from_str(&msg));
            }
            if ctx.lenient_skip_vectors
                || (matches!(ctx.mode, crate::ParseMode::LazyMetadata)
                    && length > ctx.effective_lazy_threshold())
            {
                let offset = cursor.position();
                let start_pos = cursor.position();
                for _ in 0..length {
                    let elem_flags = read_u32_async(cursor).await?;
                    let elem_type = elem_flags & 0xFF;
                    let elem_type_alt = (elem_flags >> 8) & 0xFF;
                    if elem_type == REFSXP {
                        continue;
                    }
                    if elem_type == SYMSXP {
                        let name_flags = read_u32_async(cursor).await?;
                        let name_type = name_flags & 0xFF;
                        let name_type_alt = (name_flags >> 8) & 0xFF;
                        if name_type == REFSXP {
                            continue;
                        }
                        if name_type == CHARSXP || name_type_alt == CHARSXP {
                            let str_len = read_i32_async(cursor).await?;
                            if str_len >= 0 {
                                cursor.skip_bytes(str_len as usize).await?;
                            }
                            continue;
                        }
                        return Err(Error::InvalidFormat(
                            "non-CHARSXP element in character vector".to_string(),
                        ));
                    }
                    if elem_type == CHARSXP || elem_type_alt == CHARSXP {
                        let str_len = read_i32_async(cursor).await?;
                        if str_len >= 0 {
                            cursor.skip_bytes(str_len as usize).await?;
                        }
                        continue;
                    }
                    return Err(Error::InvalidFormat(
                        "non-CHARSXP element in character vector".to_string(),
                    ));
                }

                return Ok(RObject::Character(VectorData::Lazy(LazyVector {
                    length,
                    offset,
                    byte_len: cursor.position().saturating_sub(start_pos),
                })));
            }

            let mut vec = Vec::with_capacity(length);
            let mut string_cache: Vec<Arc<str>> = Vec::new();
            for _ in 0..length {
                let elem_flags = read_u32_async(cursor).await?;
                let elem_type = elem_flags & 0xFF;
                let elem_type_alt = (elem_flags >> 8) & 0xFF;
                let value = if elem_type == REFSXP {
                    let ref_index = (elem_flags >> 8) as usize;
                    string_cache
                        .get(ref_index.saturating_sub(1))
                        .cloned()
                        .unwrap_or_else(|| Arc::from("NA"))
                } else if elem_type == SYMSXP {
                    let name_flags = read_u32_async(cursor).await?;
                    let name_type = name_flags & 0xFF;
                    let name_type_alt = (name_flags >> 8) & 0xFF;
                    if name_type == REFSXP {
                        let ref_index = (name_flags >> 8) as usize;
                        string_cache
                            .get(ref_index.saturating_sub(1))
                            .cloned()
                            .unwrap_or_else(|| Arc::from("NA"))
                    } else if name_type == CHARSXP || name_type_alt == CHARSXP {
                        Arc::from(
                            parse_charsxp_content_async(ctx, cursor, name_flags)
                                .await?
                                .as_str(),
                        )
                    } else {
                        Arc::from("NA")
                    }
                } else if elem_type == CHARSXP || elem_type_alt == CHARSXP {
                    Arc::from(
                        parse_charsxp_content_async(ctx, cursor, elem_flags)
                            .await?
                            .as_str(),
                    )
                } else {
                    Arc::from("NA")
                };
                string_cache.push(value.clone());
                vec.push(value);
            }
            RObject::Character(vec.into())
        }
        INTSXP => {
            let length = read_u32_async(cursor).await? as usize;
            if ctx.lenient_skip_vectors {
                guard_skip_allocation(length, std::mem::size_of::<i32>(), cursor, "int vector")?;
            } else {
                guard_allocation_common(ctx, length, std::mem::size_of::<i32>(), "int vector")?;
            }
            let byte_len = length * std::mem::size_of::<i32>();
            if ctx.force_materialize_vector && length <= MAX_FORCE_MATERIALIZE_VECTOR_LEN {
                #[cfg(target_arch = "wasm32")]
                log_large_alloc(ctx, "intsxp", length);
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(read_i32_async(cursor).await?);
                }
                RObject::Integer(VectorData::Owned(values))
            } else {
                cursor.skip_bytes(byte_len).await?;
                RObject::Integer(VectorData::Lazy(LazyVector {
                    length,
                    offset: cursor.position().saturating_sub(byte_len as u64),
                    byte_len: byte_len as u64,
                }))
            }
        }
        REALSXP => {
            let length = read_u32_async(cursor).await? as usize;
            if ctx.lenient_skip_vectors {
                guard_skip_allocation(length, std::mem::size_of::<f64>(), cursor, "real vector")?;
            } else {
                guard_allocation_common(ctx, length, std::mem::size_of::<f64>(), "real vector")?;
            }
            let byte_len = length * std::mem::size_of::<f64>();
            if ctx.force_materialize_vector && length <= MAX_FORCE_MATERIALIZE_VECTOR_LEN {
                #[cfg(target_arch = "wasm32")]
                log_large_alloc(ctx, "realsxp", length);
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(read_f64_async(cursor).await?);
                }
                RObject::Real(VectorData::Owned(values))
            } else {
                cursor.skip_bytes(byte_len).await?;
                RObject::Real(VectorData::Lazy(LazyVector {
                    length,
                    offset: cursor.position().saturating_sub(byte_len as u64),
                    byte_len: byte_len as u64,
                }))
            }
        }
        LGLSXP => {
            let length = read_u32_async(cursor).await? as usize;
            if ctx.lenient_skip_vectors {
                guard_skip_allocation(length, 4, cursor, "logical vector")?;
            } else {
                guard_allocation_common(ctx, length, 4, "logical vector")?;
            }
            let byte_len = length * 4;
            if ctx.force_materialize_vector && length <= MAX_FORCE_MATERIALIZE_VECTOR_LEN {
                #[cfg(target_arch = "wasm32")]
                log_large_alloc(ctx, "lglsxp", length);
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(read_i32_async(cursor).await?.into());
                }
                RObject::Logical(VectorData::Owned(values))
            } else {
                cursor.skip_bytes(byte_len).await?;
                RObject::Logical(VectorData::Lazy(LazyVector {
                    length,
                    offset: cursor.position().saturating_sub(byte_len as u64),
                    byte_len: byte_len as u64,
                }))
            }
        }
        RAWSXP => {
            let length = read_u32_async(cursor).await? as usize;
            if ctx.lenient_skip_vectors {
                guard_skip_allocation(length, 1, cursor, "raw vector")?;
            } else {
                guard_allocation_common(ctx, length, 1, "raw vector")?;
            }
            cursor.skip_bytes(length).await?;
            RObject::Raw(VectorData::Lazy(LazyVector {
                length,
                offset: cursor.position().saturating_sub(length as u64),
                byte_len: length as u64,
            }))
        }
        CPLXSXP => {
            let length = read_u32_async(cursor).await? as usize;
            if ctx.lenient_skip_vectors {
                guard_skip_allocation(
                    length,
                    std::mem::size_of::<Complex>(),
                    cursor,
                    "complex vector",
                )?;
            } else {
                guard_allocation_common(
                    ctx,
                    length,
                    std::mem::size_of::<Complex>(),
                    "complex vector",
                )?;
            }
            let byte_len = length * std::mem::size_of::<Complex>();
            cursor.skip_bytes(byte_len).await?;
            RObject::Complex(VectorData::Lazy(LazyVector {
                length,
                offset: cursor.position().saturating_sub(byte_len as u64),
                byte_len: byte_len as u64,
            }))
        }
        VECSXP | EXPRSXP => {
            let start_pos = cursor.position();
            let length = read_u32_async(cursor).await? as usize;
            #[cfg(target_arch = "wasm32")]
            if sequential_debug_enabled() {
                let msg = format!(
                    "seq skip vecsxp start pos={} length={} parsing_s4_tag={}",
                    start_pos, length, ctx.parsing_s4_tag
                );
                web_sys::console::debug_1(&JsValue::from_str(&msg));
            }
            if !ctx.lenient_skip_vectors {
                guard_allocation_common(ctx, length, 1, "list")?;
            }
            let mut values = if ctx.lenient_skip_vectors {
                None
            } else {
                Some(Vec::with_capacity(length))
            };
            for idx in 0..length {
                let value_start = cursor.position();
                let value = if ctx.mode == crate::ParseMode::LazyMetadata {
                    std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                        ctx,
                        cursor,
                        ref_table,
                        symbol_table,
                        dedup_table,
                    )))
                    .await?
                } else {
                    std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                        ctx,
                        cursor,
                        ref_table,
                        symbol_table,
                        dedup_table,
                    )))
                    .await?
                };
                #[cfg(target_arch = "wasm32")]
                if sequential_list_debug_enabled() && idx < 3 {
                    let value_end = cursor.position();
                    let msg = format!(
                        "seq skip vecsxp elem idx={} type={:?} pos={} delta={}",
                        idx,
                        std::mem::discriminant(&value),
                        value_end,
                        value_end.saturating_sub(value_start)
                    );
                    web_sys::console::debug_1(&JsValue::from_str(&msg));
                }
                if let Some(ref mut vec) = values {
                    vec.push(value);
                }
            }
            if sexp_type == VECSXP {
                RObject::List(values.unwrap_or_default())
            } else {
                RObject::Expression(values.unwrap_or_default())
            }
        }
        LISTSXP | ATTRLISTSXP | LANGSXP | ATTRLANGSXP => {
            let list = std::pin::Pin::from(Box::new(parse_pairlist_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                has_tag,
            )))
            .await?;
            if sexp_type == LANGSXP || sexp_type == ATTRLANGSXP {
                let (function, args) = if list.is_empty() {
                    (RObject::Null, Vec::new())
                } else {
                    (list[0].value.clone(), list[1..].to_vec())
                };
                RObject::Language {
                    function: Box::new(function),
                    args,
                }
            } else {
                RObject::Pairlist(list)
            }
        }
        S4SXP => RObject::Null,
        _ => RObject::Null,
    };

    let attributes = if sexp_type == S4SXP && has_tag {
        let prev = ctx.parsing_s4_tag;
        let prev_pairlist_root = ctx.parsing_pairlist_root;
        ctx.parsing_s4_tag = true;
        ctx.parsing_pairlist_root = true;
        let pairlist = std::pin::Pin::from(Box::new(parse_pairlist_sequential_value_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            true,
        )))
        .await?;
        ctx.parsing_s4_tag = prev;
        ctx.parsing_pairlist_root = prev_pairlist_root;
        parse_attributes(RObject::Pairlist(pairlist), ctx)?
    } else if let Some(attrs) = early_attributes {
        attrs
    } else if has_attr {
        let prev = ctx.parsing_s4_tag;
        let prev_pairlist_root = ctx.parsing_pairlist_root;
        if sexp_type == S4SXP {
            ctx.parsing_s4_tag = true;
            ctx.parsing_pairlist_root = true;
        }
        let attr_obj = if sexp_type == S4SXP {
            std::pin::Pin::from(Box::new(parse_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?
        } else {
            std::pin::Pin::from(Box::new(skip_object_sequential_value_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
            )))
            .await?
        };
        if sexp_type == S4SXP {
            ctx.parsing_s4_tag = prev;
            ctx.parsing_pairlist_root = prev_pairlist_root;
        }
        parse_attributes(attr_obj, ctx)?
    } else {
        Attributes::new()
    };

    if sexp_type == S4SXP && !attributes.is_empty() {
        obj = convert_to_s4_object(attributes);
    } else if has_attr && !attributes.is_empty() {
        obj = RObject::WithAttributes {
            object: Box::new(obj),
            attributes,
        };
    }

    if let Some(index) = ref_index {
        ref_table.update(index, obj.clone());
        obj = RObject::Shared(
            ref_table
                .get(index)
                .ok_or_else(|| Error::InvalidFormat(format!("Missing ref idx {}", index)))?,
        );
    }

    Ok(obj)
}
#[cfg(target_arch = "wasm32")]
async fn parse_charsxp_content_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    flags: u32,
) -> Result<String> {
    let compact_flag = (flags >> 24) & 0xFF;
    let use_compact = compact_flag > 0;

    let length = if use_compact {
        let bytes_3 = read_bytes_async(cursor, 3).await?;
        ((bytes_3[0] as i32) << 16) | ((bytes_3[1] as i32) << 8) | (bytes_3[2] as i32)
    } else {
        read_i32_async(cursor).await?
    };

    if length == -1 {
        return Ok(String::from("NA"));
    }
    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative CHARSXP length {}",
            length
        )));
    }

    let length = length as usize;
    guard_allocation_common(ctx, length, 1, "charsxp content")?;
    let bytes = read_bytes_async(cursor, length).await?;

    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };

    Ok(string)
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
async fn parse_charsxp_async<C: AsyncCursor>(
    ctx: &mut ParserContext,
    cursor: &mut C,
) -> Result<String> {
    let flags = read_u32_async(cursor).await?;
    let type_from_0_7 = flags & 0xFF;
    let type_from_8_15 = (flags >> 8) & 0xFF;

    if type_from_8_15 == CHARSXP || type_from_0_7 == CHARSXP {
        return parse_charsxp_content_async(ctx, cursor, flags).await;
    }

    if type_from_0_7 == NILSXP || type_from_0_7 == NILVALUE_SXP {
        return Ok(String::from("NA"));
    }

    if type_from_0_7 == REFSXP {
        return Err(Error::InvalidFormat(format!(
            "REFSXP in CHARSXP context requires caller to handle reference (ref={})",
            flags >> 8
        )));
    }

    Err(Error::InvalidFormat(format!(
        "Expected CHARSXP ({}), got {} (flags: 0x{:08x})",
        CHARSXP, type_from_0_7, flags
    )))
}

#[cfg(target_arch = "wasm32")]
fn estimate_parse_size_from_cursor<C: AsyncCursor>(cursor: &C) -> Result<usize> {
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
/// Parse the RDS file header.
fn parse_header(cursor: &mut RdsCursor<'_>) -> Result<u32> {
    // RDS files start with specific magic bytes
    let mut magic = [0u8; 2];
    ensure_bytes_available(cursor, 2, "parse_header:magic")?;
    cursor
        .read_exact(&mut magic)
        .map_err(|_| Error::UnexpectedEof)?;

    // Check for RDS format identifier
    // Format is typically 'X\n' for XDR format (big-endian)
    if magic[0] != b'X' {
        return Err(Error::InvalidFormat(format!(
            "Expected 'X' magic byte, got {:?}",
            magic[0]
        )));
    }

    // Read format version
    ensure_bytes_available(cursor, 4, "parse_header:format_version")?;
    let format_version = cursor.read_u32::<BigEndian>()?;

    // Read R version that wrote the file
    ensure_bytes_available(cursor, 4, "parse_header:writer_version")?;
    let _writer_version = cursor.read_u32::<BigEndian>()?;

    // Read minimum R version needed to read
    ensure_bytes_available(cursor, 4, "parse_header:min_reader_version")?;
    let _min_reader_version = cursor.read_u32::<BigEndian>()?;

    Ok(format_version)
}

/// Parse an R object from the stream.
fn parse_object(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Peek at the first byte to check for packaged/pseudo types
    let pos = cursor.position();
    if debug_enabled() {
        let stream_len = cursor.len();
        if pos + 16 >= stream_len {
            debug_log!(
                "[PARSE_OBJECT] Near EOF: pos={}, total={}, remaining={}",
                pos,
                stream_len,
                stream_len.saturating_sub(pos)
            );
        }
    }
    ensure_bytes_available(cursor, 1, "parse_object:first_byte")?;
    let first_byte = match cursor.read_u8() {
        Ok(b) => b,
        Err(e) => {
            // Check if we're at EOF - this is expected in some cases
            let stream_len = cursor.len();
            if pos >= stream_len {
                return Err(Error::UnexpectedEofDetail {
                    position: pos as usize,
                    needed: 1,
                    available: 0,
                });
            }
            return Err(Error::Io(e));
        }
    };
    cursor.set_position(pos); // Reset position

    // Check if this is a packaged type (single byte encoding)
    // These include NILVALUE_SXP (254/0xFE), GLOBALENV_SXP (253/0xFD), etc.
    if first_byte >= 240 {
        // This is likely a packaged type - read as single byte
        let _packed_type = cursor.read_u8()?;
        // For now, treat all packaged types as NULL
        // GLOBALENV_SXP, UNBOUNDVALUE_SXP, MISSINGARG_SXP are all treated as NULL
        // as they represent singleton environments/values that don't carry data
        return Ok(RObject::Null);
    }

    // Read the flags as a big-endian u32
    ensure_bytes_available(cursor, 4, "parse_object:flags")?;
    let flags = cursor.read_u32::<BigEndian>()?;
    if debug_enabled() {
        debug_log!(
            "[PARSE_OBJECT] Flags=0x{:08x} at pos={}, first_byte={}",
            flags,
            cursor.position(),
            first_byte
        );
    }

    // Extract the SEXP type from the flags.
    // The type can appear in different bit positions:
    // - Bits 0-7: Standard position for most types (REALSXP=14, STRSXP=16, INTSXP=13, etc.)
    // - Bits 8-15: Alternative position when bits 0-7 are 0 or contain non-type data
    //
    // The heuristic:
    // - REFSXP (255) is ALWAYS in bits 0-7 - check this first
    // - If bits 0-7 contain a valid standard type (>= 2 and <= 25), use it
    // - Otherwise, if bits 0-7 are 0-1 (NILSXP/SYMSXP) and bits 8-15 are >= 2, use bits 8-15
    // - For SYMSXP (1) specifically, it's in bits 0-7
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type = if type_from_0_7 == REFSXP {
        // REFSXP is always in bits 0-7, and bits 8-15 contain the reference index
        type_from_0_7
    } else if (2..=S4SXP).contains(&type_from_0_7) {
        // Standard types (LISTSXP=2 through S4SXP=25) in their normal position
        type_from_0_7
    } else if type_from_0_7 == 1 {
        // SYMSXP is always in bits 0-7
        type_from_0_7
    } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
        // If bits 0-7 are 0 (NILSXP) but bits 8-15 have a valid type, use bits 8-15
        // This handles cases like flags 0x00040200 where type 2 (LISTSXP) is in bits 8-15
        type_from_8_15
    } else {
        // Fall back to bits 0-7 (handles NILSXP=0 and special markers)
        type_from_0_7
    };

    // Check for attribute and tag flags
    // Note: Due to XDR encoding, these bits might be in their documented positions
    // or shifted depending on the type
    // IMPORTANT: For REFSXP, bits 8-15 contain the reference index, NOT attribute/tag flags
    let has_attr = if sexp_type == REFSXP {
        false // REFSXP uses bits 8-15 for reference index
    } else {
        (flags & HAS_ATTR_BIT) != 0
    };
    let has_tag = if sexp_type == REFSXP {
        false // REFSXP uses bits 8-15 for reference index
    } else {
        (flags & HAS_TAG_BIT) != 0
    };

    // For pairlists, language objects, and closures, attributes come BEFORE the data
    // (From R's serialize.c: LISTSXP/LANGSXP have ATTRIB before CAR/CDR,
    //  CLOSXP has ATTRIB before CLOENV/FORMALS/BODY)
    // Parse them early if present
    // Note: For CLOSXP, R uses HAS_TAG_BIT to indicate attributes (not HAS_ATTR_BIT)
    // IMPORTANT: For S4SXP, when HAS_TAG_BIT is set, the TAG contains the attributes pairlist!
    let early_attributes = if has_attr
        && (sexp_type == LISTSXP
            || sexp_type == LANGSXP
            || sexp_type == CLOSXP
            || sexp_type == PROMSXP)
    {
        let prev = ctx.parsing_attributes;
        ctx.parsing_attributes = true;
        let obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        ctx.parsing_attributes = prev;
        Some(parse_attributes(obj, ctx)?)
    } else if sexp_type == S4SXP && has_tag {
        // S4 objects with HAS_TAG_BIT store their attributes in the TAG
        // The TAG is a pairlist where each element has a tag (slot name) and value (slot data).
        // Parse with has_tag=true to extract the tag names correctly.

        // Set S4 tag parsing flag (will be cleared after this block)
        ctx.parsing_s4_tag = true;
        let pairlist = parse_pairlist(ctx, cursor, true, ref_table, symbol_table, dedup_table)?;

        let attrs = if let RObject::Pairlist(list) = pairlist {
            parse_attributes(RObject::Pairlist(list), ctx)?
        } else {
            parse_attributes(pairlist, ctx)?
        };
        ctx.parsing_s4_tag = false;
        Some(attrs)
    } else {
        None
    };

    // Add a placeholder to the reference table early for objects that should be tracked
    // This is crucial for circular references - the object must be in the table
    // before we parse its contents/attributes
    let track_reference = should_track_reference(sexp_type, has_attr);
    let ref_index = if track_reference && sexp_type != CLOSXP {
        // Add a NULL placeholder for now
        let idx = ref_table.add(RObject::Null);
        Some(idx)
    } else {
        None
    };

    // Parse the object based on type
    let mut obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        UNBOUNDVALUE_SXP => RObject::UnboundValue, // Unbound value marker
        EMPTYENV_SXP => RObject::EmptyEnv,         // Empty environment (root of env tree)
        BASEENV_SXP => RObject::BaseEnv,           // Base environment
        GLOBALENV_SXP => RObject::GlobalEnv,       // Global environment
        SYMSXP => parse_symbol(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        INTSXP => parse_integer_vector(ctx, cursor)?,
        REALSXP => parse_real_vector(ctx, cursor)?,
        CPLXSXP => parse_complex_vector(ctx, cursor)?,
        LGLSXP => parse_logical_vector(ctx, cursor)?,
        STRSXP => parse_character_vector(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        RAWSXP => parse_raw_vector(ctx, cursor)?,
        S4SXP => parse_s4_object(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        VECSXP => parse_list(ctx, cursor, ref_table, symbol_table, dedup_table, has_attr)?,
        EXPRSXP => parse_expression(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        BCODESXP => parse_bytecode(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        EXTPTRSXP => {
            // External pointer - typically cannot be serialized meaningfully
            // R usually replaces these with NULL on deserialization
            // Skip the external pointer data and return NULL
            if std::env::var("RDS_WARN_EXTPTR").is_ok() || debug_enabled() {
                debug_log!("Warning: External pointer (EXTPTRSXP) encountered - returning NULL");
            }
            RObject::Null
        }
        WEAKREFSXP => {
            // Weak reference - similar to external pointers
            // These typically cannot be meaningfully deserialized
            if std::env::var("RDS_WARN_EXTPTR").is_ok() || debug_enabled() {
                debug_log!("Warning: Weak reference (WEAKREFSXP) encountered - returning NULL");
            }
            RObject::Null
        }
        LISTSXP => parse_pairlist(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        LANGSXP => parse_language(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        CHARSXP => {
            // Sometimes CHARSXP appears standalone (like for encoding markers)
            let string = parse_charsxp_content(ctx, cursor, flags)?;
            // Return as a single-element character vector for now
            RObject::Character(vec![Arc::from(string.as_str())].into())
        }
        CLOSXP => parse_closure(
            ctx,
            cursor,
            has_tag,
            track_reference,
            ref_table,
            symbol_table,
            dedup_table,
        )?,
        ENVSXP => parse_environment(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        PROMSXP => parse_promise(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        SPECIALSXP => parse_special(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        BUILTINSXP => parse_builtin(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        REFSXP => {
            // Reference to a previously seen object
            // The reference index occupies the upper 24 bits (bits 8-31)
            // of the flags word; use the full width to support large graphs.
            let ref_index_val = flags >> 8;

            let in_closure_body = ctx.parsing_closure_body;

            // Prefer the symbol table for closure bodies so parameter references resolve
            // to symbols even when ref_table indices clash with earlier placeholders.
            let mut resolved = if in_closure_body {
                if let Some(sym) = symbol_table.get(ref_index_val) {
                    sym.clone()
                } else if let Some(obj) = ref_table.get(ref_index_val) {
                    if std::env::var("RDS_DEBUG_REF_FALLBACK").is_ok() {
                        let obj_type = obj.read().unwrap().variant_name();
                        debug_log!(
                            "[CLOSURE_REF_FALLBACK] idx={} sym_table={} ref_table={} type={}",
                            ref_index_val,
                            symbol_table.len(),
                            ref_table.next_index - 1,
                            obj_type
                        );
                    }
                    RObject::Shared(obj)
                } else {
                    return Err(Error::InvalidFormat(format!(
                        "Invalid reference index: {}",
                        ref_index_val
                    )));
                }
            } else {
                match ref_table.get(ref_index_val) {
                    Some(obj) => RObject::Shared(obj),
                    None => {
                        return Err(Error::InvalidFormat(format!(
                            "Invalid reference index: {}",
                            ref_index_val
                        )));
                    }
                }
            };

            // Normalize closure-body references to concrete symbols/characters.
            if in_closure_body {
                let concrete = resolved.as_concrete();
                resolved = match concrete {
                    RObject::Symbol(name) => RObject::Symbol(name),
                    RObject::Character(names) if !names.is_empty() && names.is_loaded() => {
                        RObject::Symbol(names[0].clone())
                    }
                    other => other,
                };
            }

            return Ok(resolved);
        }
        ALTREP_SXP => {
            // ALTREP object (version 3 feature)
            // Structure: class_info, state, attributes
            // ALTREP handles its own attributes internally, so parse them here
            let class_info = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            let state = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            let attributes_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

            // Convert ALTREP to native representation
            let native_obj = convert_altrep_to_native(ctx, class_info, state)?;

            // Parse and apply attributes if present
            let final_obj = if !matches!(attributes_obj, RObject::Null) {
                let attrs = parse_attributes(attributes_obj, ctx)?;
                if !attrs.is_empty() {
                    RObject::WithAttributes {
                        object: Box::new(native_obj),
                        attributes: attrs,
                    }
                } else {
                    native_obj
                }
            } else {
                native_obj
            };

            // Update reference table and return early to prevent double attribute parsing
            if let Some(idx) = ref_index {
                ref_table.update(idx, final_obj);
                return Ok(RObject::Shared(ref_table.get(idx).expect("Just updated")));
            }
            return Ok(final_obj);
        }
        NAMESPACESXP => {
            // Namespace - parse and discard, then return early to handle attributes specially

            let namespace_result =
                parse_namespace(ctx, cursor, ref_table, symbol_table, dedup_table)?;

            // For namespaces with attributes, we need to parse and discard them
            if has_attr {
                let _attrs = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            }

            // Update ref table if needed
            if let Some(idx) = ref_index {
                ref_table.update(idx, namespace_result);
                return Ok(RObject::Shared(ref_table.get(idx).expect("Just updated")));
            }

            return Ok(namespace_result);
        }
        BCREPREF | BCREPDEF => {
            // Bytecode representation reference/definition
            // These are used for circular references in bytecode serialization
            // Treat as references similar to REFSXP
            let ref_index_val = flags >> 8;

            match ref_table.get(ref_index_val) {
                Some(obj) => return Ok(RObject::Shared(obj)),
                None => {
                    // If not found in ref table, this might be a definition, return NULL for now
                    RObject::Null
                }
            }
        }
        NAMESPACESXP_SERIAL | BASENAMESPACE_SXP => {
            // Namespace/base namespace markers in serialization format
            // Similar to NAMESPACESXP (123) but use format type 249/250

            let namespace_result =
                parse_namespace(ctx, cursor, ref_table, symbol_table, dedup_table)?;

            if has_attr {
                let _attrs = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            }

            return Ok(namespace_result);
        }
        PACKAGESXP => {
            // Package environment marker
            // Similar to namespace handling
            RObject::Null
        }
        MISSINGARG_SXP => {
            // Missing argument marker (default value placeholder in formals)
            RObject::MissingArg
        }
        GENERICREFSXP | CLASSREFSXP => {
            // Generic function or class reference
            // These reference metadata in the serialization stream
            let ref_index_val = flags >> 8;

            match ref_table.get(ref_index_val) {
                Some(obj) => return Ok(RObject::Shared(obj)),
                None => RObject::Null,
            }
        }
        PERSISTSXP => {
            // Persistent object marker
            RObject::Null
        }
        ATTRLISTSXP | ATTRLANGSXP => {
            // Attribute list/language alternate encoding
            // Parse as regular list/language
            if sexp_type == ATTRLISTSXP {
                parse_pairlist(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?
            } else {
                parse_language(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?
            }
        }
        _ if sexp_type > 25 && sexp_type < 238 => {
            // Unknown type in the gap between standard types (0-25) and pseudo-types (238-255)
            // This might be data misalignment or a format variation
            // For now, return NULL and log a warning
            RObject::Null
        }
        _ => {
            return Err(Error::UnknownSexpType(sexp_type));
        }
    };

    // Parse attributes if present (unless already parsed early for LISTSXP/LANGSXP)
    let attributes = if let Some(attrs) = early_attributes {
        attrs
    } else if has_attr {
        let pos_before_attr = cursor.position();
        let mut attr_obj = None;
        let stream_len = cursor.len();
        let remaining = stream_len.saturating_sub(cursor.position());
        if remaining == 0 {
            attr_obj = Some(RObject::Null);
        }
        let attr_value = match attr_obj {
            Some(obj) => obj,
            None => {
                let prev = ctx.parsing_attributes;
                ctx.parsing_attributes = true;
                let obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
                ctx.parsing_attributes = prev;
                obj
            }
        };
        if std::env::var("RDS_DEBUG_ATTR_POS").is_ok() {
            debug_log!(
                "[ATTR_POS] type={} pos_before={} pos_after={}, remaining={}",
                sexp_type,
                pos_before_attr,
                cursor.position(),
                cursor.len().saturating_sub(cursor.position())
            );
        }
        if sexp_type == S4SXP && std::env::var("RDS_DEBUG_S4_ATTR_OBJ").is_ok() {
            match &attr_value {
                RObject::Pairlist(list) => {
                    let tags: Vec<_> = list
                        .iter()
                        .map(|p| p.tag.as_deref().unwrap_or("<none>").to_string())
                        .collect();
                    debug_log!("[S4_ATTR_OBJ] pairlist len={} tags={:?}", list.len(), tags);
                }
                other => {
                    debug_log!(
                        "[S4_ATTR_OBJ] non-pairlist attr type={:?}",
                        std::mem::discriminant(other)
                    );
                }
            }
        }

        parse_attributes(attr_value, ctx)?
    } else {
        Attributes::new()
    };

    // Apply attributes if non-empty
    // S4 objects can have attributes via HAS_ATTR_BIT or via HAS_TAG_BIT (early_attributes)
    if sexp_type == S4SXP && !attributes.is_empty() {
        let mut attributes = attributes;
        // If the S4 attributes are missing class (or other trailing attrs),
        // try to merge any recently parsed attribute set that carried a class
        // or package information.
        if let Some(extra) = ctx.pending_class_attrs.take() {
            if attributes.get("class").is_none() || attributes.get("package").is_none() {
                for (k, v) in extra.attrs.into_iter() {
                    if !attributes.attrs.iter().any(|(ek, _)| ek == &k) {
                        attributes.insert(k, *v);
                    }
                }
            }
        }

        if std::env::var("RDS_DEBUG_S4_ATTRS").is_ok() {
            let keys: Vec<_> = attributes
                .attrs
                .iter()
                .map(|(k, v)| (k.as_ref(), std::mem::discriminant(v.as_ref())))
                .collect();
            debug_log!("[S4_ATTRS] len={} keys={:?}", keys.len(), keys);
        }
        // S4 object: all attributes become slots, except class
        obj = convert_to_s4_object(attributes);
    } else if has_attr && !attributes.is_empty() {
        // Check if this is an S4 object (S4SXP type) - shouldn't happen here now
        if sexp_type == S4SXP {
            let mut attributes = attributes;
            // If the S4 attributes are missing class (or other trailing attrs),
            // try to merge any recently parsed attribute set that carried a class.
            if attributes.get("class").is_none() {
                if let Some(extra) = ctx.pending_class_attrs.take() {
                    for (k, v) in extra.attrs.into_iter() {
                        if !attributes.attrs.iter().any(|(ek, _)| ek == &k) {
                            attributes.insert(k, *v);
                        }
                    }
                }
            } else {
                // Clear pending if we already have class to avoid leaking across objects.
                ctx.pending_class_attrs.take();
            }

            if std::env::var("RDS_DEBUG_S4_ATTRS").is_ok() {
                let keys: Vec<_> = attributes
                    .attrs
                    .iter()
                    .map(|(k, v)| (k.as_ref(), std::mem::discriminant(v.as_ref())))
                    .collect();
                debug_log!("[S4_ATTRS] len={} keys={:?}", keys.len(), keys);
            }
            // S4 object: all attributes become slots, except class
            obj = convert_to_s4_object(attributes);
        } else {
            // Check if this has a class attribute (for S3 objects)
            let has_class = attributes.get("class").is_some();
            let has_s4_class = attributes.get("class").is_some_and(|class_obj| {
                    let class_obj = class_obj.as_concrete();
                    matches!(
                        class_obj,
                        RObject::WithAttributes { attributes, .. } if attributes.get("package").is_some()
                    )
                });

            if has_s4_class {
                // Data-part S4 objects should remain as a vector with attributes.
                obj = RObject::WithAttributes {
                    object: Box::new(obj),
                    attributes,
                };
            } else if has_class {
                // Check if this is a data.frame (special S3 object)
                if let Some(dataframe) = try_convert_to_dataframe(&obj, &attributes) {
                    obj = dataframe;
                } else if let Some(factor) = try_convert_to_factor(&obj, &attributes) {
                    // Check if this is a factor (special S3 object)
                    obj = factor;
                } else {
                    // General S3 object with class attribute
                    obj = convert_to_s3_object(obj, attributes);
                }
            } else {
                // Regular object with attributes (no class)
                obj = RObject::WithAttributes {
                    object: Box::new(obj),
                    attributes,
                };
            }
        }
    }

    // Update the reference table with the final object if we added a placeholder earlier
    // IMPORTANT: Don't double-wrap Shared objects
    if let Some(idx) = ref_index {
        // If the object is already Shared, ensure the ref table points at the same Arc.
        // Otherwise, update the existing placeholder in place to preserve any earlier lookups.
        let arc_to_store = match obj {
            RObject::Shared(ref arc) => {
                // Avoid storing a Shared wrapper inside the ref table (would create self-cycles).
                let inner = arc.read().unwrap().clone();
                ref_table.update(idx, inner);
                ref_table
                    .get(idx)
                    .ok_or_else(|| Error::InvalidFormat(format!("Missing ref idx {}", idx)))?
            }
            other => {
                ref_table.update(idx, other);
                ref_table
                    .get(idx)
                    .ok_or_else(|| Error::InvalidFormat(format!("Missing ref idx {}", idx)))?
            }
        };

        // Return as Shared wrapper
        obj = RObject::Shared(arc_to_store);
    }

    // If this is a symbol (SYMSXP), add it to the symbol table in parse order
    // This is used for resolving REFSXP in TAG positions (e.g., pairlist attribute names)
    if sexp_type == SYMSXP {
        symbol_table.add(obj.clone());
        if std::env::var("RDS_DEBUG_SYM").is_ok() {
            if let RObject::Character(names) = &obj {
                if let Some(name) = names.first() {
                    debug_log!("[SYMBOL_ADD] {}", name);
                }
            }
        }
    }

    // Try to deduplicate the object before returning
    // If we've seen an identical object before, return that instead
    // IMPORTANT: Don't deduplicate Shared objects - they're already deduplicated by design
    // (multiple Shared wrappers point to the same Arc, dedup would break this by cloning)
    if !matches!(obj, RObject::Shared(_)) {
        if let Some(deduped_obj) = dedup_table.deduplicate(&obj) {
            return Ok(deduped_obj);
        }
    }

    Ok(obj)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamControl {
    Continue,
    Stop,
}

struct StreamingProgressState<'a> {
    total_bytes: Option<u64>,
    objects_visited: usize,
    base_offset: u64,
    callback: Option<&'a mut dyn FnMut(StreamingProgress)>,
}

impl<'a> StreamingProgressState<'a> {
    fn new(
        total_bytes: Option<u64>,
        callback: Option<&'a mut dyn FnMut(StreamingProgress)>,
    ) -> Self {
        Self {
            total_bytes,
            objects_visited: 0,
            base_offset: 0,
            callback,
        }
    }

    fn report_object(&mut self, cursor_pos: u64) {
        self.objects_visited = self.objects_visited.saturating_add(1);
        if let Some(callback) = self.callback.as_mut() {
            callback(StreamingProgress {
                bytes_read: self.base_offset.saturating_add(cursor_pos),
                total_bytes: self.total_bytes,
                objects_visited: self.objects_visited,
            });
        }
    }
}

struct StreamingRefTable {
    paths: Vec<Option<crate::ObjectPath>>,
}

impl StreamingRefTable {
    fn new() -> Self {
        Self { paths: vec![None] }
    }

    fn insert(&mut self, index: u32, path: crate::ObjectPath) {
        let idx = index as usize;
        if self.paths.len() <= idx {
            self.paths.resize_with(idx + 1, || None);
        }
        self.paths[idx] = Some(path);
    }

    fn get(&self, index: u32) -> Option<&crate::ObjectPath> {
        self.paths
            .get(index as usize)
            .and_then(|path| path.as_ref())
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_object_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    progress: &mut StreamingProgressState<'_>,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    emit: bool,
) -> StreamingResult<StreamControl, V::Error> {
    let pos = cursor.position();
    ensure_bytes_available(cursor, 1, "streaming:parse_object:first_byte")?;
    let first_byte = cursor
        .read_u8()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
    cursor.set_position(pos);

    if first_byte >= 240 {
        let _ = cursor
            .read_u8()
            .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
        if emit {
            visitor
                .on_object_start(path, "Null")
                .map_err(StreamingError::Visitor)?;
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }
        progress.report_object(cursor.position());
        return Ok(StreamControl::Continue);
    }

    ensure_bytes_available(cursor, 4, "streaming:parse_object:flags")?;
    let flags = cursor
        .read_u32::<BigEndian>()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };

    let has_attr = if sexp_type == REFSXP {
        false
    } else {
        (flags & HAS_ATTR_BIT) != 0
    };
    let has_tag = if sexp_type == REFSXP {
        false
    } else {
        (flags & HAS_TAG_BIT) != 0
    };

    if sexp_type == REFSXP {
        let ref_index = flags >> 8;
        if emit {
            visitor
                .on_object_start(path, "SharedRef")
                .map_err(StreamingError::Visitor)?;
            visitor
                .on_shared_reference(path, ref_paths.get(ref_index))
                .map_err(StreamingError::Visitor)?;
            visitor
                .on_object_end(path)
                .map_err(StreamingError::Visitor)?;
        }
        progress.report_object(cursor.position());
        return Ok(StreamControl::Continue);
    }

    let early_attributes = if has_attr
        && (sexp_type == LISTSXP
            || sexp_type == LANGSXP
            || sexp_type == CLOSXP
            || sexp_type == PROMSXP)
    {
        let prev = ctx.parsing_attributes;
        ctx.parsing_attributes = true;
        let obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        ctx.parsing_attributes = prev;
        Some(parse_attributes(obj, ctx)?)
    } else if sexp_type == S4SXP && has_tag {
        ctx.parsing_s4_tag = true;
        let pairlist = parse_pairlist(ctx, cursor, true, ref_table, symbol_table, dedup_table)?;
        let attrs = if let RObject::Pairlist(list) = pairlist {
            parse_attributes(RObject::Pairlist(list), ctx)?
        } else {
            parse_attributes(pairlist, ctx)?
        };
        ctx.parsing_s4_tag = false;
        Some(attrs)
    } else {
        None
    };

    let track_reference = should_track_reference(sexp_type, has_attr);
    let ref_index = if track_reference && sexp_type != CLOSXP {
        let index = ref_table.add(RObject::Null);
        ref_paths.insert(index, path.clone());
        Some(index)
    } else {
        None
    };

    let obj_type = sexp_type_name(sexp_type);
    let mut emit_children = emit;
    if emit {
        match visitor
            .on_object_start(path, obj_type)
            .map_err(StreamingError::Visitor)?
        {
            VisitAction::Stop => {
                progress.report_object(cursor.position());
                return Ok(StreamControl::Stop);
            }
            VisitAction::Skip => emit_children = false,
            VisitAction::Continue => {}
        }
    }

    if let Some(ref attrs) = early_attributes {
        if emit {
            visitor
                .on_attributes(path, attrs)
                .map_err(StreamingError::Visitor)?;
            if emit_children {
                emit_attribute_values_streaming(attrs, path, visitor)?;
            }
        }
    }

    let control = match sexp_type {
        NILSXP | NILVALUE_SXP => StreamControl::Continue,
        INTSXP => parse_atomic_vector_streaming::<i32, V>(
            ctx,
            cursor,
            VectorKind::Integer,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        REALSXP => parse_atomic_vector_streaming::<f64, V>(
            ctx,
            cursor,
            VectorKind::Real,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        LGLSXP => parse_atomic_vector_streaming::<Logical, V>(
            ctx,
            cursor,
            VectorKind::Logical,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        RAWSXP => parse_atomic_vector_streaming::<u8, V>(
            ctx,
            cursor,
            VectorKind::Raw,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        CPLXSXP => parse_atomic_vector_streaming::<Complex, V>(
            ctx,
            cursor,
            VectorKind::Complex,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        STRSXP => parse_character_vector_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        SYMSXP => parse_symbol_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            emit_children,
            visitor,
            path,
            ref_index,
            progress,
        )?,
        VECSXP => parse_list_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        LISTSXP | ATTRLISTSXP => parse_pairlist_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            has_tag,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        LANGSXP | ATTRLANGSXP => parse_language_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            has_tag,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        EXPRSXP => parse_expression_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        CLOSXP => parse_closure_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        ENVSXP => parse_environment_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        PROMSXP => parse_promise_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        BCODESXP => parse_bytecode_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        S4SXP => StreamControl::Continue,
        NAMESPACESXP | NAMESPACESXP_SERIAL | BASENAMESPACE_SXP => {
            let namespace_obj = parse_namespace(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            if emit_children {
                if let RObject::Namespace(values) = namespace_obj {
                    visitor
                        .on_vector_metadata(path, VectorKind::Character, values.len())
                        .map_err(StreamingError::Visitor)?;
                }
            }
            StreamControl::Continue
        }
        ALTREP_SXP => parse_altrep_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            emit_children,
            visitor,
            path,
            progress,
        )?,
        WEAKREFSXP | EXTPTRSXP | PACKAGESXP | MISSINGARG_SXP | PERSISTSXP | GLOBALENV_SXP
        | BASEENV_SXP | EMPTYENV_SXP | UNBOUNDVALUE_SXP => StreamControl::Continue,
        GENERICREFSXP | CLASSREFSXP | 244 => StreamControl::Continue,
        _ if sexp_type > 25 && sexp_type < 238 => StreamControl::Continue,
        _ => return Err(StreamingError::Parse(Error::UnknownSexpType(sexp_type))),
    };

    if has_attr && early_attributes.is_none() {
        let prev = ctx.parsing_attributes;
        ctx.parsing_attributes = true;
        let attr_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        ctx.parsing_attributes = prev;
        let attrs = parse_attributes(attr_obj, ctx)?;
        if emit {
            visitor
                .on_attributes(path, &attrs)
                .map_err(StreamingError::Visitor)?;
            if emit_children {
                emit_attribute_values_streaming(&attrs, path, visitor)?;
            }
        }
    }

    if emit {
        visitor
            .on_object_end(path)
            .map_err(StreamingError::Visitor)?;
    }

    progress.report_object(cursor.position());
    Ok(control)
}

fn sexp_type_name(sexp_type: u32) -> &'static str {
    match sexp_type {
        NILSXP | NILVALUE_SXP => "Null",
        SYMSXP => "Symbol",
        LISTSXP => "Pairlist",
        CLOSXP => "Closure",
        ENVSXP => "Environment",
        PROMSXP => "Promise",
        LANGSXP => "Language",
        SPECIALSXP => "Special",
        BUILTINSXP => "Builtin",
        CHARSXP => "Charsxp",
        LGLSXP => "Logical",
        INTSXP => "Integer",
        REALSXP => "Real",
        CPLXSXP => "Complex",
        STRSXP => "Character",
        VECSXP => "List",
        EXPRSXP => "Expression",
        BCODESXP => "Bytecode",
        EXTPTRSXP => "ExternalPtr",
        WEAKREFSXP => "WeakRef",
        S4SXP => "S4Object",
        NAMESPACESXP | NAMESPACESXP_SERIAL | BASENAMESPACE_SXP => "Namespace",
        ALTREP_SXP => "Altrep",
        _ => "Unknown",
    }
}

fn emit_attribute_values_streaming<V: RdsVisitor>(
    attrs: &Attributes,
    path: &mut crate::ObjectPath,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    for (key, value) in attrs.iter() {
        let segment = Arc::from(format!("@{}", key.as_ref()));
        path.push(segment);
        emit_parsed_object_streaming(value, path, visitor)?;
        path.pop();
    }
    Ok(())
}

fn emit_parsed_object_streaming<V: RdsVisitor>(
    obj: &RObject,
    path: &mut crate::ObjectPath,
    visitor: &mut V,
) -> StreamingResult<(), V::Error> {
    let action = visitor
        .on_object_start(path, object_type_name(obj))
        .map_err(StreamingError::Visitor)?;
    let emit_children = match action {
        VisitAction::Stop => return Ok(()),
        VisitAction::Skip => false,
        VisitAction::Continue => true,
    };

    if emit_children {
        match obj {
            RObject::Null
            | RObject::Symbol(_)
            | RObject::Special { .. }
            | RObject::Builtin { .. }
            | RObject::Bytecode { .. }
            | RObject::Environment { .. }
            | RObject::Promise { .. }
            | RObject::Language { .. }
            | RObject::Expression(_)
            | RObject::Namespace(_)
            | RObject::GlobalEnv
            | RObject::BaseEnv
            | RObject::EmptyEnv
            | RObject::MissingArg
            | RObject::UnboundValue => {}
            RObject::Shared(inner) => {
                if let Ok(inner) = inner.read() {
                    emit_parsed_object_streaming(&inner, path, visitor)?;
                }
            }
            RObject::WithAttributes { object, attributes } => {
                visitor
                    .on_attributes(path, attributes)
                    .map_err(StreamingError::Visitor)?;
                emit_attribute_values_streaming(attributes, path, visitor)?;
                emit_parsed_object_streaming(object, path, visitor)?;
            }
            RObject::Closure {
                formals,
                body,
                environment,
            } => {
                path.push(Arc::from("formals"));
                emit_parsed_object_streaming(formals, path, visitor)?;
                path.pop();
                path.push(Arc::from("body"));
                emit_parsed_object_streaming(body, path, visitor)?;
                path.pop();
                path.push(Arc::from("environment"));
                emit_parsed_object_streaming(environment, path, visitor)?;
                path.pop();
            }
            RObject::Integer(vec) => {
                emit_vector_metadata(VectorKind::Integer, vec, path, visitor)?;
            }
            RObject::Real(vec) => {
                emit_vector_metadata(VectorKind::Real, vec, path, visitor)?;
            }
            RObject::Logical(vec) => {
                emit_vector_metadata(VectorKind::Logical, vec, path, visitor)?;
            }
            RObject::Raw(vec) => {
                emit_vector_metadata(VectorKind::Raw, vec, path, visitor)?;
            }
            RObject::Complex(vec) => {
                emit_vector_metadata(VectorKind::Complex, vec, path, visitor)?;
            }
            RObject::Character(vec) => {
                emit_vector_metadata(VectorKind::Character, vec, path, visitor)?;
            }
            RObject::List(values) => {
                for (index, value) in values.iter().enumerate() {
                    path.push(Arc::from(format!("[{}]", index)));
                    emit_parsed_object_streaming(value, path, visitor)?;
                    path.pop();
                }
            }
            RObject::Pairlist(values) => {
                for (index, value) in values.iter().enumerate() {
                    path.push(Arc::from(format!("[{}]", index)));
                    emit_parsed_object_streaming(&value.value, path, visitor)?;
                    path.pop();
                }
            }
            RObject::DataFrame(data) => {
                for (name, column) in data.columns.iter() {
                    path.push(Arc::clone(name));
                    emit_parsed_object_streaming(column, path, visitor)?;
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
                visitor
                    .on_attributes(path, &data.attributes)
                    .map_err(StreamingError::Visitor)?;
                emit_attribute_values_streaming(&data.attributes, path, visitor)?;
                path.push(Arc::from("base"));
                emit_parsed_object_streaming(&data.base, path, visitor)?;
                path.pop();
            }
            RObject::S4Object(data) => {
                let mut attrs = Attributes::new();
                for (name, slot) in data.slots.iter() {
                    attrs.insert(Arc::clone(name), slot.clone());
                }
                visitor
                    .on_attributes(path, &attrs)
                    .map_err(StreamingError::Visitor)?;
                for (name, slot) in data.slots.iter() {
                    path.push(Arc::clone(name));
                    emit_parsed_object_streaming(slot, path, visitor)?;
                    path.pop();
                }
            }
        }
    }

    visitor
        .on_object_end(path)
        .map_err(StreamingError::Visitor)?;
    Ok(())
}

fn emit_vector_metadata<V, T>(
    kind: VectorKind,
    data: &VectorData<T>,
    path: &mut crate::ObjectPath,
    visitor: &mut V,
) -> StreamingResult<(), V::Error>
where
    V: RdsVisitor,
{
    let len = data.len();
    visitor
        .on_vector_metadata(path, kind, len)
        .map_err(StreamingError::Visitor)?;
    if let VectorData::Lazy(span) = data {
        let _ = visitor
            .on_vector_chunk_available(path, *span)
            .map_err(StreamingError::Visitor)?;
    }
    Ok(())
}

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

fn parse_atomic_vector_streaming<T, V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    kind: VectorKind,
    emit: bool,
    visitor: &mut V,
    path: &crate::ObjectPath,
    _progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let length = cursor
        .read_u32::<BigEndian>()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))? as usize;
    let elem_size = match kind {
        VectorKind::Logical => 4,
        _ => std::mem::size_of::<T>(),
    };
    guard_allocation(ctx, length, elem_size, cursor, "vector")?;
    let offset = cursor.position();
    let byte_len = length
        .checked_mul(elem_size)
        .ok_or_else(|| Error::InvalidFormat("vector byte length overflow".to_string()))?;
    if emit {
        visitor
            .on_vector_metadata(path, kind, length)
            .map_err(StreamingError::Visitor)?;
        let span = LazyVector {
            length,
            offset,
            byte_len: byte_len as u64,
        };
        let _ = visitor
            .on_vector_chunk_available(path, span)
            .map_err(StreamingError::Visitor)?;
    }
    ensure_bytes_available(cursor, byte_len, "streaming:vector_skip")?;
    cursor
        .seek(SeekFrom::Current(byte_len as i64))
        .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
    Ok(StreamControl::Continue)
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn parse_tag_name_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    progress: &mut StreamingProgressState<'_>,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
) -> StreamingResult<Option<Arc<str>>, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!("sequential tag parse start pos={}", cursor.position());
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }
    let flags = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)?;
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type =
        if type_from_0_7 == REFSXP || (2..=S4SXP).contains(&type_from_0_7) || type_from_0_7 == 1 {
            type_from_0_7
        } else if type_from_0_7 == 0 && type_from_8_15 >= 2 {
            type_from_8_15
        } else {
            type_from_0_7
        };
    #[cfg(target_arch = "wasm32")]
    {
        let msg = format!(
            "sequential tag flags=0x{:08x} type0_7={} type8_15={} sexp_type={}",
            flags, type_from_0_7, type_from_8_15, sexp_type
        );
        web_sys::console::debug_1(&JsValue::from_str(&msg));
    }

    if sexp_type == REFSXP {
        let ref_index = flags >> 8;
        let tag_obj = if let Some(sym) = symbol_table.get(ref_index) {
            sym.clone()
        } else if let Some(obj) = ref_table.get(ref_index) {
            obj.read()
                .map_err(|_| {
                    StreamingError::Parse(Error::Unsupported(
                        "shared object lock poisoned".to_string(),
                    ))
                })?
                .clone()
        } else {
            return Err(StreamingError::Parse(Error::InvalidFormat(format!(
                "Invalid TAG REFSXP index {} (symbol table size={}, ref table size={})",
                ref_index,
                symbol_table.len(),
                ref_table.next_index - 1
            ))));
        };
        return Ok(extract_tag_name(tag_obj));
    }

    if sexp_type == SYMSXP {
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("sequential tag SYMSXP at pos={}", cursor.position());
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        let name_flags = read_u32_async(cursor)
            .await
            .map_err(StreamingError::Parse)?;
        let name_type_from_0_7 = name_flags & 0xFF;
        let name_type_from_8_15 = (name_flags >> 8) & 0xFF;
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!(
                "sequential tag SYMSXP name flags=0x{:08x} type0_7={} type8_15={}",
                name_flags, name_type_from_0_7, name_type_from_8_15
            );
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        let has_attr = (name_flags & HAS_ATTR_BIT) != 0;

        let name = if name_type_from_0_7 == REFSXP {
            let ref_index = name_flags >> 8;
            let tag_obj = ref_table
                .get(ref_index)
                .ok_or_else(|| {
                    StreamingError::Parse(Error::InvalidFormat(format!(
                        "Invalid symbol REFSXP index {}",
                        ref_index
                    )))
                })?
                .read()
                .map_err(|_| {
                    StreamingError::Parse(Error::Unsupported(
                        "shared object lock poisoned".to_string(),
                    ))
                })?
                .clone();
            extract_tag_name(tag_obj).unwrap_or_else(|| Arc::from("NA"))
        } else if name_type_from_8_15 == CHARSXP || name_type_from_0_7 == CHARSXP {
            Arc::from(
                parse_charsxp_content_async(ctx, cursor, name_flags)
                    .await
                    .map_err(StreamingError::Parse)?
                    .as_str(),
            )
        } else {
            return Err(StreamingError::Parse(Error::InvalidFormat(format!(
                "Unexpected SYMSXP name type {} (flags: 0x{:08x})",
                name_type_from_0_7, name_flags
            ))));
        };

        if has_attr {
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
        }

        let sym = RObject::Symbol(name.clone());
        symbol_table.add(sym.clone());
        return Ok(extract_tag_name(sym));
    }

    if sexp_type == CHARSXP {
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("sequential tag CHARSXP at pos={}", cursor.position());
            web_sys::console::debug_1(&JsValue::from_str(&msg));
        }
        let has_attr = (flags & HAS_ATTR_BIT) != 0;
        let name = parse_charsxp_content_async(ctx, cursor, flags)
            .await
            .map_err(StreamingError::Parse)?;
        if has_attr {
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
        }
        let obj = RObject::Character(vec![Arc::from(name.as_str())].into());
        return Ok(extract_tag_name(obj));
    }

    let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )))
    .await?;

    Ok(None)
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn parse_pairlist_element_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    index: usize,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<(Option<Arc<str>>, StreamControl), V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    let tag_name = if has_tag {
        parse_tag_name_streaming_sequential_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
        )
        .await?
    } else {
        None
    };

    if emit {
        let segment = tag_name
            .clone()
            .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
        path.push(segment);
        let control = std::pin::Pin::from(Box::new(parse_object_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )))
        .await?;
        path.pop();
        Ok((tag_name, control))
    } else {
        let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            false,
        )))
        .await?;
        Ok((tag_name, StreamControl::Continue))
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn parse_pairlist_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    let mut index = 0usize;
    let _ = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)?;
    let (_tag_name, control) = parse_pairlist_element_streaming_sequential_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        has_tag,
        emit,
        visitor,
        path,
        index,
        progress,
    )
    .await?;
    if matches!(control, StreamControl::Stop) {
        return Ok(StreamControl::Stop);
    }
    index += 1;

    loop {
        cursor
            .ensure_available(4)
            .await
            .map_err(StreamingError::Parse)?;
        let flags = cursor.peek_u32().map_err(StreamingError::Parse)?;
        let next_type = flags & 0xFF;
        let has_tag_next = (flags & HAS_TAG_BIT) != 0;
        let continues_pairlist =
            has_tag_next || matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP);

        if next_type == REFSXP {
            let _ = read_u32_async(cursor)
                .await
                .map_err(StreamingError::Parse)?;
            if emit {
                visitor
                    .on_object_start(path, "SharedRef")
                    .map_err(StreamingError::Visitor)?;
                visitor
                    .on_object_end(path)
                    .map_err(StreamingError::Visitor)?;
            }
            break;
        } else if continues_pairlist {
            let _flags = read_u32_async(cursor)
                .await
                .map_err(StreamingError::Parse)?;
            let (_tag_name, control) = parse_pairlist_element_streaming_sequential_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                has_tag_next,
                emit,
                visitor,
                path,
                index,
                progress,
            )
            .await?;
            if matches!(control, StreamControl::Stop) {
                return Ok(StreamControl::Stop);
            }
            index += 1;
        } else {
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
            break;
        }
    }

    Ok(StreamControl::Continue)
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn parse_language_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    if has_tag {
        let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            false,
        )))
        .await?;
    }

    if emit {
        path.push(Arc::from("function"));
        if matches!(
            std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )))
            .await?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();

        path.push(Arc::from("args"));
        let control = std::pin::Pin::from(Box::new(parse_object_streaming_async(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )))
        .await?;
        path.pop();
        return Ok(control);
    }

    let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )))
    .await?;
    let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )))
    .await?;

    Ok(StreamControl::Continue)
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn parse_list_streaming_sequential_async<C, V>(
    ctx: &mut ParserContext,
    cursor: &mut C,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error>
where
    C: AsyncCursor,
    V: RdsVisitor,
{
    let length = read_u32_async(cursor)
        .await
        .map_err(StreamingError::Parse)? as usize;
    guard_allocation_common(ctx, length, 1, "VECSXP/list")?;

    for index in 0..length {
        if emit {
            path.push(Arc::from(format!("[{}]", index)));
            if matches!(
                std::pin::Pin::from(Box::new(parse_object_streaming_async(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    ref_paths,
                    progress,
                    visitor,
                    path,
                    true,
                )))
                .await?,
                StreamControl::Stop
            ) {
                path.pop();
                return Ok(StreamControl::Stop);
            }
            path.pop();
        } else {
            let _ = std::pin::Pin::from(Box::new(parse_object_streaming_async(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )))
            .await?;
        }

        if index + 1 < length {
            cursor
                .ensure_available(1)
                .await
                .map_err(StreamingError::Parse)?;
            let marker = cursor.as_sync_slice(1).map_err(StreamingError::Parse)?[0];
            if marker == NILVALUE_SXP as u8 {
                cursor.advance(1).map_err(StreamingError::Parse)?;
            }
        }
    }

    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_character_vector_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    emit: bool,
    visitor: &mut V,
    path: &crate::ObjectPath,
    _progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let length = cursor
        .read_u32::<BigEndian>()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))? as usize;
    guard_allocation(ctx, length, 1, cursor, "character vector")?;
    let offset = cursor.position();
    let start_pos = cursor.position();
    if emit {
        visitor
            .on_vector_metadata(path, VectorKind::Character, length)
            .map_err(StreamingError::Visitor)?;
    }

    for _ in 0..length {
        let pos = cursor.position();
        let flags = cursor
            .read_u32::<BigEndian>()
            .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
        let type_from_0_7 = flags & 0xFF;
        let type_from_8_15 = (flags >> 8) & 0xFF;

        if type_from_0_7 == REFSXP {
            continue;
        }

        if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
            let _ = parse_charsxp_content(ctx, cursor, flags)?;
            continue;
        }

        cursor.set_position(pos);
        let _ = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    }

    let byte_len = cursor.position().saturating_sub(start_pos);
    if emit {
        let span = LazyVector {
            length,
            offset,
            byte_len,
        };
        let _ = visitor
            .on_vector_chunk_available(path, span)
            .map_err(StreamingError::Visitor)?;
    }

    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_symbol_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    _emit: bool,
    _visitor: &mut V,
    _path: &crate::ObjectPath,
    ref_index: Option<u32>,
    _progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let name_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let symbol_obj = match name_obj {
        RObject::Character(names) if names.len() == 1 => {
            let name = &names[0];
            if name.as_ref() == "\x01NULL\x01" {
                RObject::Symbol(names.into_vec().into_iter().next().unwrap())
            } else {
                RObject::Symbol(name.clone())
            }
        }
        other => other,
    };
    symbol_table.add(symbol_obj.clone());
    if let Some(index) = ref_index {
        ref_table.update(index, symbol_obj.clone());
    }
    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_list_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let length = cursor
        .read_u32::<BigEndian>()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))? as usize;
    guard_allocation(ctx, length, 1, cursor, "VECSXP/list")?;
    for index in 0..length {
        if emit {
            path.push(Arc::from(format!("[{}]", index)));
            if matches!(
                parse_object_streaming(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    ref_paths,
                    progress,
                    visitor,
                    path,
                    true,
                )?,
                StreamControl::Stop
            ) {
                path.pop();
                return Ok(StreamControl::Stop);
            }
            path.pop();
        } else {
            let _ = parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )?;
        }

        if index + 1 < length {
            let pos = cursor.position();
            ensure_bytes_available(cursor, 1, "streaming:list:peek")?;
            let marker = cursor
                .read_u8()
                .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
            if marker != NILVALUE_SXP as u8 {
                cursor.set_position(pos);
            }
        }
    }
    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_pairlist_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let mut index = 0usize;
    let (_tag_name, control) = parse_pairlist_element_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        has_tag,
        emit,
        visitor,
        path,
        index,
        progress,
    )?;
    if matches!(control, StreamControl::Stop) {
        return Ok(StreamControl::Stop);
    }
    index += 1;

    loop {
        let pos = cursor.position();
        let remaining = cursor.len().saturating_sub(pos);
        if remaining == 0 {
            break;
        }
        ensure_bytes_available(cursor, 4, "streaming:pairlist:next_flags")?;
        let flags = cursor
            .read_u32::<BigEndian>()
            .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
        let next_type = flags & 0xFF;
        let has_tag_next = (flags & HAS_TAG_BIT) != 0;
        let continues_pairlist =
            has_tag_next || matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP);

        if next_type == REFSXP {
            if emit {
                visitor
                    .on_object_start(path, "SharedRef")
                    .map_err(StreamingError::Visitor)?;
                visitor
                    .on_object_end(path)
                    .map_err(StreamingError::Visitor)?;
            }
            break;
        } else if continues_pairlist {
            let (_tag_name, control) = parse_pairlist_element_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                has_tag_next,
                emit,
                visitor,
                path,
                index,
                progress,
            )?;
            if matches!(control, StreamControl::Stop) {
                return Ok(StreamControl::Stop);
            }
            index += 1;
        } else {
            cursor.set_position(pos);
            let _ = parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )?;
            break;
        }
    }
    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_pairlist_element_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    index: usize,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<(Option<Arc<str>>, StreamControl), V::Error> {
    let tag_name = if has_tag {
        let pos = cursor.position();
        ensure_bytes_available(cursor, 4, "streaming:pairlist:tag_flags")?;
        let flags = cursor
            .read_u32::<BigEndian>()
            .map_err(|e| StreamingError::Parse(Error::Io(e)))?;
        let tag_type = flags & 0xFF;
        let tag_obj = if tag_type == REFSXP {
            let sym_index = flags >> 8;
            if let Some(sym) = symbol_table.get(sym_index) {
                sym.clone()
            } else if let Some(obj) = ref_table.get(sym_index) {
                obj.read()
                    .map_err(|_| {
                        StreamingError::Parse(Error::Unsupported(
                            "shared object lock poisoned".to_string(),
                        ))
                    })?
                    .clone()
            } else {
                return Err(StreamingError::Parse(Error::InvalidFormat(format!(
                    "Invalid TAG REFSXP index {} (symbol table size={}, ref table size={})",
                    sym_index,
                    symbol_table.len(),
                    ref_table.next_index - 1
                ))));
            }
        } else {
            cursor.set_position(pos);
            parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?
        };
        extract_tag_name(tag_obj.clone())
    } else {
        None
    };

    if emit {
        let segment = tag_name
            .clone()
            .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
        path.push(segment);
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        Ok((tag_name, control))
    } else {
        let _ = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            false,
        )?;
        Ok((tag_name, StreamControl::Continue))
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_language_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    has_tag: bool,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    if has_tag {
        let _ = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    }
    if emit {
        path.push(Arc::from("function"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();

        path.push(Arc::from("args"));
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        return Ok(control);
    }

    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_expression_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let length = cursor
        .read_u32::<BigEndian>()
        .map_err(|e| StreamingError::Parse(Error::Io(e)))? as usize;
    guard_allocation(ctx, length, 1, cursor, "expression vector")?;
    for index in 0..length {
        if emit {
            path.push(Arc::from(format!("[{}]", index)));
            if matches!(
                parse_object_streaming(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    ref_paths,
                    progress,
                    visitor,
                    path,
                    true,
                )?,
                StreamControl::Stop
            ) {
                path.pop();
                return Ok(StreamControl::Stop);
            }
            path.pop();
        } else {
            let _ = parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                false,
            )?;
        }
    }
    Ok(StreamControl::Continue)
}

#[allow(clippy::too_many_arguments)]
fn parse_closure_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    if emit {
        path.push(Arc::from("formals"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("body"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("environment"));
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        return Ok(control);
    }

    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_environment_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    if emit {
        path.push(Arc::from("enclosing"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("frame"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("hashtab"));
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        return Ok(control);
    }

    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_promise_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    if emit {
        path.push(Arc::from("value"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("expression"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("environment"));
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        return Ok(control);
    }

    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_bytecode_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    if emit {
        path.push(Arc::from("code"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("constants"));
        if matches!(
            parse_object_streaming(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                ref_paths,
                progress,
                visitor,
                path,
                true,
            )?,
            StreamControl::Stop
        ) {
            path.pop();
            return Ok(StreamControl::Stop);
        }
        path.pop();
        path.push(Arc::from("expr"));
        let control = parse_object_streaming(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            ref_paths,
            progress,
            visitor,
            path,
            true,
        )?;
        path.pop();
        return Ok(control);
    }

    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    let _ = parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )?;
    parse_object_streaming(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        ref_paths,
        progress,
        visitor,
        path,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_altrep_streaming<V: RdsVisitor>(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    _ref_paths: &mut StreamingRefTable,
    emit: bool,
    visitor: &mut V,
    path: &mut crate::ObjectPath,
    _progress: &mut StreamingProgressState<'_>,
) -> StreamingResult<StreamControl, V::Error> {
    let class_info = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let state = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let prev = ctx.parsing_attributes;
    ctx.parsing_attributes = true;
    let attr_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    ctx.parsing_attributes = prev;
    let attrs = parse_attributes(attr_obj, ctx)?;
    if emit {
        if let Some((kind, len)) = estimate_altrep_metadata(&class_info, &state)
            .or_else(|| estimate_compact_seq_fallback(&state))
        {
            visitor
                .on_vector_metadata(path, kind, len)
                .map_err(StreamingError::Visitor)?;
        }
        visitor
            .on_attributes(path, &attrs)
            .map_err(StreamingError::Visitor)?;
    }
    Ok(StreamControl::Continue)
}

fn estimate_altrep_metadata(class_info: &RObject, state: &RObject) -> Option<(VectorKind, usize)> {
    let class_name = extract_altrep_class_name(class_info)?;
    let state = state.as_concrete();

    let name = class_name.as_str();
    if name.contains("compact_intseq") {
        return estimate_compact_seq(&state, VectorKind::Integer);
    }
    if name.contains("compact_realseq") {
        return estimate_compact_seq(&state, VectorKind::Real);
    }
    if name.contains("wrap_int") {
        return estimate_wrapped_len(&state, VectorKind::Integer);
    }
    if name.contains("wrap_real") {
        return estimate_wrapped_len(&state, VectorKind::Real);
    }
    None
}

fn estimate_compact_seq(state: &RObject, kind: VectorKind) -> Option<(VectorKind, usize)> {
    match state {
        RObject::Real(params) => match params {
            crate::VectorData::Owned(values) if !values.is_empty() => {
                let len = values[0] as i64;
                if len >= 0 {
                    Some((kind, len as usize))
                } else {
                    None
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn estimate_wrapped_len(state: &RObject, kind: VectorKind) -> Option<(VectorKind, usize)> {
    match (kind, state) {
        (VectorKind::Integer, RObject::Integer(crate::VectorData::Owned(vec))) => {
            Some((kind, vec.len()))
        }
        (VectorKind::Real, RObject::Real(crate::VectorData::Owned(vec))) => Some((kind, vec.len())),
        _ => None,
    }
}

fn estimate_compact_seq_fallback(state: &RObject) -> Option<(VectorKind, usize)> {
    let state = state.as_concrete();
    let values = match &state {
        RObject::Real(crate::VectorData::Owned(values)) => values,
        _ => return None,
    };
    if values.len() != 3 {
        return None;
    }
    let len = values[0];
    if len < 0.0 {
        return None;
    }
    let first = values[1];
    let stride = values[2];
    let is_integer_seq = first.fract() == 0.0 && stride.fract() == 0.0 && stride == 1.0;
    let kind = if is_integer_seq {
        VectorKind::Integer
    } else {
        VectorKind::Real
    };
    Some((kind, len as usize))
}

/// Parse an integer vector.
fn parse_integer_vector(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(
        ctx,
        length,
        std::mem::size_of::<i32>(),
        cursor,
        "integer vector",
    )?;

    // Check if we should parse lazily
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        // Record position before data
        let offset = cursor.position();
        let elem_size = std::mem::size_of::<i32>();
        let byte_len = (length * elem_size) as u64;

        // In non-WASM mode, skip the data by reading into a buffer
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut buf = vec![0u8; length * elem_size];
            cursor.read_exact(&mut buf)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if !ctx.async_parse_mode {
                ensure_bytes_available(cursor, length * elem_size, "integer vector lazy skip")?;
                cursor.seek(SeekFrom::Current((length * elem_size) as i64))?;
            }
        }

        return Ok(RObject::Integer(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        let val = read_int_flexible(cursor)?;
        vec.push(val);
    }

    Ok(RObject::Integer(vec.into()))
}

/// Parse a real (double) vector.
fn parse_real_vector(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(
        ctx,
        length,
        std::mem::size_of::<f64>(),
        cursor,
        "real vector",
    )?;

    // Check if we should parse lazily
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        let offset = cursor.position();
        let elem_size = std::mem::size_of::<f64>();
        let byte_len = (length * elem_size) as u64;

        // In non-WASM mode, skip the data by reading into a buffer
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut buf = vec![0u8; length * elem_size];
            cursor.read_exact(&mut buf)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if !ctx.async_parse_mode {
                ensure_bytes_available(cursor, length * elem_size, "real vector lazy skip")?;
                cursor.seek(SeekFrom::Current((length * elem_size) as i64))?;
            }
        }

        return Ok(RObject::Real(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        let val = cursor.read_f64::<BigEndian>()?;
        vec.push(val);
    }

    Ok(RObject::Real(vec.into()))
}

/// Parse a logical vector.
fn parse_logical_vector(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(
        ctx,
        length,
        std::mem::size_of::<Logical>(),
        cursor,
        "logical vector",
    )?;

    // Check if we should parse lazily
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        let offset = cursor.position();
        let elem_size = std::mem::size_of::<i32>(); // Logicals are stored as i32
        let byte_len = (length * elem_size) as u64;

        // In non-WASM mode, skip the data by reading into a buffer
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut buf = vec![0u8; length * elem_size];
            cursor.read_exact(&mut buf)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if !ctx.async_parse_mode {
                ensure_bytes_available(cursor, length * elem_size, "logical vector lazy skip")?;
                cursor.seek(SeekFrom::Current((length * elem_size) as i64))?;
            }
        }

        return Ok(RObject::Logical(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        // R seems to write logical values with variable byte length
        // Try to read 4 bytes, but if only 3 are available, pad with 0
        let val = read_int_flexible(cursor)?;
        let logical = match val {
            0 => Logical::False,
            1 => Logical::True,
            i32::MIN => Logical::Na, // NA_LOGICAL
            _ => Logical::Na,        // Treat any other value as NA
        };
        vec.push(logical);
    }

    Ok(RObject::Logical(vec.into()))
}

/// Read an integer - always reads 4 bytes in big-endian format.
fn read_int_flexible(cursor: &mut RdsCursor<'_>) -> Result<i32> {
    Ok(cursor.read_i32::<BigEndian>()?)
}

/// Parse a character vector (STRSXP - a vector of CHARSXP).
fn parse_character_vector(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let pos_before_length = cursor.position();
    ensure_bytes_available(cursor, 4, "parse_character_vector:length")?;
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, length, 1, cursor, "character vector")?;

    // Check if we should parse lazily
    // Note: For character vectors, we need to parse through all elements to calculate byte_len
    // since strings are variable length. We'll skip string content but still need to read lengths.
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        let offset = cursor.position();
        let start_pos = cursor.position();

        // Skip through all character elements to calculate total byte length
        for _ in 0..length {
            let flags = cursor.read_u32::<BigEndian>()?;
            let type_from_0_7 = flags & 0xFF;

            if type_from_0_7 == REFSXP {
                // Reference, no additional data to skip
                continue;
            } else if type_from_0_7 == CHARSXP || ((flags >> 8) & 0xFF) == CHARSXP {
                // Parse CHARSXP length and skip content
                let str_len = cursor.read_i32::<BigEndian>()?;
                if str_len >= 0 {
                    let mut buf = vec![0u8; str_len as usize];
                    cursor.read_exact(&mut buf)?;
                }
                // NA strings have negative length, no data to read
            } else {
                // For other types, we need to parse and skip them
                // This is complex, so for now we'll fall back to full parsing for safety
                // Reset cursor and do full parsing
                cursor.set_position(pos_before_length);
                let _ = cursor.read_u32::<BigEndian>()?; // Re-read length
                return parse_character_vector_full(
                    ctx,
                    cursor,
                    ref_table,
                    symbol_table,
                    dedup_table,
                    length,
                );
            }
        }

        let byte_len = cursor.position() - start_pos;

        return Ok(RObject::Character(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    parse_character_vector_full(ctx, cursor, ref_table, symbol_table, dedup_table, length)
}

// Helper function for full character vector parsing
fn parse_character_vector_full(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    length: usize,
) -> Result<RObject> {
    if debug_enabled() {
        let remaining = cursor.len().saturating_sub(cursor.position());
        debug_log!(
            "[STRSXP] Parsing character vector of length {} (now at pos {}), remaining={}",
            length,
            cursor.position(),
            remaining
        );
    }

    let mut vec = Vec::with_capacity(length);
    // Local string cache for REFSXP within this character vector
    let mut string_cache: Vec<Arc<str>> = Vec::new();

    for _ in 0..length {
        // Parse the flags to check the type
        let pos = cursor.position();
        let flags = cursor.read_u32::<BigEndian>()?;
        let type_from_0_7 = flags & 0xFF;

        // Check if this is a REFSXP (string deduplication)
        if type_from_0_7 == REFSXP {
            // It's a reference to a previously seen string in this vector
            let ref_index = (flags >> 8) as usize;

            // Look up in local string cache (1-based indexing)
            if ref_index > 0 && ref_index <= string_cache.len() {
                vec.push(string_cache[ref_index - 1].clone());
            } else {
                return Err(Error::InvalidFormat(format!(
                    "Invalid string reference: {} (cache size: {})",
                    ref_index,
                    string_cache.len()
                )));
            }
        } else if type_from_0_7 == SYMSXP {
            // Symbol in a string vector - read the CHARSXP name directly
            // SYMSXP structure: flags (already read) + CHARSXP (name)
            // The name can also be a REFSXP, so handle that case
            match parse_charsxp(ctx, cursor) {
                Ok(name_string) => {
                    let arc_str: Arc<str> = Arc::from(name_string.as_str());
                    string_cache.push(arc_str.clone());
                    vec.push(arc_str);
                }
                Err(Error::InvalidFormat(msg)) if msg.contains("REFSXP in CHARSXP context") => {
                    // Extract reference index from error message
                    // Format: "REFSXP in CHARSXP context requires caller to handle reference (ref=N)"
                    if let Some(ref_str) = msg.split("ref=").nth(1) {
                        if let Ok(ref_index) = ref_str.trim_end_matches(')').parse::<usize>() {
                            // Look up in local string cache (1-based indexing)
                            if ref_index > 0 && ref_index <= string_cache.len() {
                                vec.push(string_cache[ref_index - 1].clone());
                            } else {
                                // Reference out of range - might be global ref or error
                                // Use placeholder for now
                                let arc_str: Arc<str> =
                                    Arc::from(format!("<ref_{}>", ref_index).as_str());
                                string_cache.push(arc_str.clone());
                                vec.push(arc_str);
                            }
                        } else {
                            return Err(Error::InvalidFormat(format!(
                                "Failed to parse REFSXP index from: {}",
                                msg
                            )));
                        }
                    } else {
                        return Err(Error::InvalidFormat(format!(
                            "Unexpected REFSXP error format: {}",
                            msg
                        )));
                    }
                }
                Err(e) => return Err(e),
            }
        } else if type_from_0_7 == STRSXP {
            // Nested character vector - this is unusual and suggests a different structure
            // For now, skip it entirely by using a placeholder
            // TODO: Investigate if this should be handled differently
            let arc_str: Arc<str> = Arc::from("<nested_strsxp>");
            string_cache.push(arc_str.clone());
            vec.push(arc_str);

            // Skip the nested STRSXP by parsing and discarding it
            cursor.set_position(pos);
            let _ = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        } else {
            // Check if it's a CHARSXP (most common case)
            let type_from_8_15 = (flags >> 8) & 0xFF;
            if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
                // Reset position and parse as CHARSXP
                cursor.set_position(pos);
                let string = parse_charsxp(ctx, cursor)?;
                let arc_str: Arc<str> = Arc::from(string.as_str());

                // Add to local string cache for future REFSXP references
                string_cache.push(arc_str.clone());
                vec.push(arc_str);
            } else {
                // Some other type - parse it and convert to string
                cursor.set_position(pos);
                let pos_before_parse = cursor.position();
                let obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
                let pos_after_parse = cursor.position();
                if debug_enabled() {
                    debug_log!(
                        "[STRSXP] Non-CHARSXP element parsed at pos {}-{} => {:?}",
                        pos_before_parse,
                        pos_after_parse,
                        std::mem::discriminant(&obj)
                    );
                }

                // Convert object to string representation
                let string_repr = match &obj {
                    RObject::Integer(v) if v.len() == 1 => format!("{}", v[0]),
                    RObject::Integer(v) => format!("<int_vec_len_{}>", v.len()),
                    RObject::Real(v) if v.len() == 1 => format!("{}", v[0]),
                    RObject::Real(v) => format!("<real_vec_len_{}>", v.len()),
                    RObject::Logical(v) if v.len() == 1 => format!("{:?}", v[0]),
                    RObject::Logical(v) => format!("<logical_vec_len_{}>", v.len()),
                    RObject::Character(v) if v.len() == 1 => v[0].to_string(),
                    RObject::Character(v) => format!("<char_vec_len_{}>", v.len()),
                    RObject::Null => "NULL".to_string(),
                    _ => format!("<object_type_{}>", type_from_0_7),
                };

                let arc_str: Arc<str> = Arc::from(string_repr.as_str());
                string_cache.push(arc_str.clone());
                vec.push(arc_str);
            }
        }
    }

    Ok(RObject::Character(vec.into()))
}

/// Parse a raw vector (RAWSXP - a vector of bytes).
fn parse_raw_vector(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, length, 1, cursor, "raw vector")?;

    // Check if we should parse lazily
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        let offset = cursor.position();
        let byte_len = length as u64;

        // In non-WASM mode, skip the data by reading into a buffer
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut buf = vec![0u8; length];
            cursor.read_exact(&mut buf)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if !ctx.async_parse_mode {
                ensure_bytes_available(cursor, length, "raw vector lazy skip")?;
                cursor.seek(SeekFrom::Current(length as i64))?;
            }
        }

        return Ok(RObject::Raw(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    let mut vec = vec![0u8; length];
    cursor.read_exact(&mut vec)?;

    Ok(RObject::Raw(vec.into()))
}

/// Parse a complex vector (CPLXSXP).
fn parse_complex_vector(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<RObject> {
    use crate::types::Complex;

    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(
        ctx,
        length,
        std::mem::size_of::<Complex>(),
        cursor,
        "complex vector",
    )?;

    // Check if we should parse lazily
    if matches!(ctx.mode, crate::ParseMode::LazyMetadata) && length > ctx.effective_lazy_threshold()
    {
        use crate::types::{LazyVector, VectorData};

        let offset = cursor.position();
        let elem_size = std::mem::size_of::<Complex>(); // 2 * f64 = 16 bytes
        let byte_len = (length * elem_size) as u64;

        // In non-WASM mode, skip the data by reading into a buffer
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut buf = vec![0u8; length * elem_size];
            cursor.read_exact(&mut buf)?;
        }
        #[cfg(target_arch = "wasm32")]
        {
            if !ctx.async_parse_mode {
                ensure_bytes_available(cursor, length * elem_size, "complex vector lazy skip")?;
                cursor.seek(SeekFrom::Current((length * elem_size) as i64))?;
            }
        }

        return Ok(RObject::Complex(VectorData::Lazy(LazyVector {
            length,
            offset,
            byte_len,
        })));
    }

    // Full parsing mode
    let mut vec = Vec::with_capacity(length);
    for _ in 0..length {
        // Each complex number is two 64-bit floats: real part then imaginary part
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;

        vec.push(Complex { real, imaginary });
    }

    Ok(RObject::Complex(vec.into()))
}

/// Parse an S4 object (S4SXP).
/// S4 objects in RDS are just markers - the actual data is in attributes.
/// We return a placeholder NULL and let the attribute parsing handle it.
fn parse_s4_object(
    _ctx: &mut ParserContext,
    _cursor: &mut RdsCursor<'_>,
    _ref_table: &mut RefTable,
    _symbol_table: &mut SymbolTable,
    _dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Slot data is stored in the attributes - the data component is typically unused.
    // Leave parsing to the attribute handler.
    Ok(RObject::Null)
}

/// Parse a symbol (SYMSXP).
fn parse_symbol(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // A symbol consists of a CHARSXP for the name
    let name_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    // Extract the name
    match name_obj {
        RObject::Character(names) if names.len() == 1 => {
            let name = &names[0];
            // Check for the special NULL marker used by R for OptionalCharacter slots
            if name.as_ref() == "\x01NULL\x01" {
                Ok(RObject::Symbol(
                    names.into_vec().into_iter().next().unwrap(),
                ))
            } else {
                // Regular symbol
                Ok(RObject::Symbol(name.clone()))
            }
        }
        RObject::Character(names) => Ok(RObject::Character(names)),
        _ => {
            // If we got something unexpected, just return it
            Ok(name_obj)
        }
    }
}

/// Parse a generic list (VECSXP).
fn parse_list(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    _list_has_attr: bool,
) -> Result<RObject> {
    let pos_before_length = cursor.position();
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, length, 1, cursor, "VECSXP/list")?;
    let mut elements = Vec::with_capacity(length);

    if debug_enabled() {
        let remaining = cursor.len().saturating_sub(cursor.position());
        debug_log!(
            "[PARSE_LIST] At pos={}, length={}, remaining bytes={}",
            pos_before_length,
            length,
            remaining
        );
    }

    for i in 0..length {
        // Defensive EOF check before attempting to parse the next element so we can
        // surface a structured error instead of a generic IO failure.
        let pos = cursor.position() as usize;
        let total = cursor.len() as usize;
        let remaining = total.saturating_sub(pos);
        if debug_enabled() {
            debug_log!(
                "[PARSE_LIST] Parsing element {}/{} at pos={}, remaining={}",
                i,
                length,
                pos,
                remaining
            );
        }
        if remaining == 0 {
            return Err(Error::UnexpectedEofDetail {
                position: pos,
                needed: 1,
                available: 0,
            });
        }
        let element = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

        // Check if this is a Real vector that looks like an ALTREP compact_intseq state
        // R sometimes serializes repeated ALTREP sequences as bare state vectors
        // Skip this check for lazy vectors
        let converted_element = match element.as_concrete() {
            RObject::Real(vec) if vec.len() == 3 && vec.is_loaded() => {
                let n = vec[0];
                let start = vec[1];
                let stride = vec[2];

                // Check if this matches compact_intseq pattern: [length, start, 1.0]
                // where length > 0, and stride = 1.0
                if n > 0.0 && n.floor() == n && stride == 1.0 && start.floor() == start {
                    let length_val = n as i32;
                    let first = start as i32;

                    // Convert to integer sequence
                    let int_vec: Vec<i32> = (0..length_val).map(|j| first + j).collect();

                    // WORKAROUND: R writes a NILVALUE after bare REALSXP state vectors.
                    // We need to consume it ONLY if this is not the last element (to avoid
                    // consuming the marker before list attributes).
                    if i < length - 1 {
                        let _next =
                            parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
                    }

                    RObject::Integer(int_vec.into())
                } else {
                    element
                }
            }
            _ => element,
        };

        elements.push(converted_element);
    }

    Ok(RObject::List(elements))
}

/// Parse an expression vector (EXPRSXP).
/// Expression vectors are identical in structure to VECSXP - they're vectors of R objects,
/// typically language objects. The difference is semantic: EXPRSXP is used for collections
/// of unevaluated expressions (e.g., the result of parse()).
fn parse_expression(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, length, 1, cursor, "expression vector")?;
    let mut elements = Vec::with_capacity(length);

    for _ in 0..length {
        let element = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        elements.push(element);
    }

    Ok(RObject::Expression(elements))
}

/// Parse bytecode (BCODESXP) using R's ReadBC/ReadBC1 structure.
fn parse_bytecode(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let reps_len = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(
        ctx,
        reps_len,
        std::mem::size_of::<Option<RObject>>(),
        cursor,
        "bytecode reps",
    )?;
    let mut reps = vec![None; reps_len];
    parse_bytecode_body(ctx, cursor, ref_table, symbol_table, dedup_table, &mut reps)
}

fn parse_bytecode_body(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
) -> Result<RObject> {
    let code = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let constants = parse_bc_constants(ctx, cursor, ref_table, symbol_table, dedup_table, reps)?;

    Ok(RObject::Bytecode {
        code: Box::new(code),
        constants: Box::new(RObject::List(constants)),
        expr: Box::new(RObject::Null),
    })
}

fn parse_bc_constants(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
) -> Result<Vec<RObject>> {
    let count = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, count, 1, cursor, "bytecode constants")?;
    let mut constants = Vec::with_capacity(count);

    // Set bytecode flag for parsing constants
    let _prev_bytecode_ctx = ctx.in_bytecode_context;
    ctx.in_bytecode_context = true;

    for _ in 0..count {
        let type_code = cursor.read_i32::<BigEndian>()?;
        let value = match type_code as u32 {
            BCODESXP => {
                parse_bytecode_body(ctx, cursor, ref_table, symbol_table, dedup_table, reps)?
            }
            BCREPREF | BCREPDEF | LANGSXP | LISTSXP | ATTRLANGSXP | ATTRLISTSXP => parse_bc_lang(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                reps,
                type_code,
            )?,
            _ => parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?,
        };
        constants.push(value);
    }

    Ok(constants)
}

fn parse_bc_lang(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
    type_code: i32,
) -> Result<RObject> {
    match type_code as u32 {
        BCREPREF => {
            let index = cursor.read_u32::<BigEndian>()? as usize;
            reps.get(index)
                .and_then(|entry| entry.clone())
                .ok_or_else(|| Error::InvalidFormat(format!("Invalid BCREPREF index {}", index)))
        }
        BCREPDEF => {
            let index = cursor.read_u32::<BigEndian>()? as usize;
            let inner_type = cursor.read_i32::<BigEndian>()?;
            let value = parse_bc_lang(
                ctx,
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                reps,
                inner_type,
            )?;
            if let Some(slot) = reps.get_mut(index) {
                *slot = Some(value.clone());
            }
            Ok(value)
        }
        ATTRLANGSXP => parse_bc_lang_struct(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LANGSXP,
            true,
        ),
        ATTRLISTSXP => parse_bc_lang_struct(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LISTSXP,
            true,
        ),
        LANGSXP => parse_bc_lang_struct(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LANGSXP,
            false,
        ),
        LISTSXP => parse_bc_lang_struct(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LISTSXP,
            false,
        ),
        _ => parse_object(ctx, cursor, ref_table, symbol_table, dedup_table),
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_bc_lang_struct(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
    actual_type: u32,
    has_attr: bool,
) -> Result<RObject> {
    let attr_obj = if has_attr {
        Some(parse_object(
            ctx,
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
        )?)
    } else {
        None
    };

    let tag_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let car_type = cursor.read_i32::<BigEndian>()?;
    let car = parse_bc_lang(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        reps,
        car_type,
    )?;
    let cdr_type = cursor.read_i32::<BigEndian>()?;
    let cdr = parse_bc_lang(
        ctx,
        cursor,
        ref_table,
        symbol_table,
        dedup_table,
        reps,
        cdr_type,
    )?;

    let mut base = match actual_type {
        LANGSXP => build_language_from_bc(car, cdr),
        LISTSXP => build_pairlist_from_bc(tag_obj, car, cdr),
        _ => {
            return Err(Error::InvalidFormat(format!(
                "Unknown BC lang type {}",
                actual_type
            )))
        }
    };

    if let Some(attr) = attr_obj {
        let attrs = parse_attributes(attr, ctx)?;
        if !attrs.is_empty() {
            base = RObject::WithAttributes {
                object: Box::new(base),
                attributes: attrs,
            };
        }
    }

    Ok(base)
}

fn build_language_from_bc(car: RObject, cdr: RObject) -> RObject {
    let args = match cdr {
        RObject::Null => Vec::new(),
        RObject::Pairlist(rest) => rest,
        other => vec![PairlistElement {
            tag: None,
            value: other,
            tag_object: None,
        }],
    };
    RObject::Language {
        function: Box::new(car),
        args,
    }
}

fn build_pairlist_from_bc(tag_obj: RObject, car: RObject, cdr: RObject) -> RObject {
    let tag_name = extract_tag_name(tag_obj.clone());
    let tag_storage = match tag_obj {
        RObject::Null => None,
        other => Some(Box::new(other)),
    };

    let mut elements = Vec::new();
    elements.push(PairlistElement {
        tag: tag_name,
        value: car,
        tag_object: tag_storage,
    });

    match cdr {
        RObject::Null => {}
        RObject::Pairlist(mut rest) => elements.append(&mut rest),
        other => elements.push(PairlistElement {
            tag: None,
            value: other,
            tag_object: None,
        }),
    }

    RObject::Pairlist(elements)
}

/// Parse a closure (CLOSXP).
/// R's serialization format (from serialize.c WriteItem for CLOSXP):
/// 1. ATTRIB (attributes) - only if hasattr flag is set
/// 2. CLOENV (closure environment)
/// 3. FORMALS (formal parameters)
/// 4. BODY (function body)
///
/// Note: The has_tag parameter indicates whether attributes were written first.
/// When has_tag is true, it means the closure has attributes that need to be parsed
/// and returned separately for the caller to handle.
fn parse_closure(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    _has_tag: bool,
    track_reference: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // If has_tag is true, attributes were serialized first and should be parsed by parse_object,
    // not by parse_closure. The has_tag flag tells us that attributes exist, but they're handled
    // at a higher level (in parse_object's attribute handling at line ~598).
    //
    // However, we still need to parse the closure components in the correct order.
    // When has_tag is set, it changes the serialization order subtly in some R versions,
    // but the core components are always: CLOENV, FORMALS, BODY

    // Standard order (from R's serialize.c): environment, formals, body
    let _env_start = cursor.position();
    let env = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    let _form_start = cursor.position();
    let form = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    // Delay registering the closure placeholder until after formals are parsed so that
    // symbols introduced by formals occupy the earliest reference slots before the
    // body is parsed.
    let ref_index = if track_reference {
        let idx = ref_table.add(RObject::Null);
        Some(idx)
    } else {
        None
    };

    let _body_start = cursor.position();
    let prev = ctx.parsing_closure_body;
    ctx.parsing_closure_body = true;
    let bod = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    ctx.parsing_closure_body = prev;

    let closure_obj = RObject::Closure {
        formals: Box::new(form),
        body: Box::new(bod),
        environment: Box::new(env),
    };

    if let Some(idx) = ref_index {
        // Update the placeholder in place to preserve shared arcs for any earlier lookups.
        ref_table.update(idx, closure_obj);
        if let Some(arc) = ref_table.get(idx) {
            return Ok(RObject::Shared(arc));
        } else {
            return Err(Error::InvalidFormat(format!(
                "Failed to retrieve closure ref idx {} after update",
                idx
            )));
        }
    }

    Ok(closure_obj)
}

/// Parse an environment (ENVSXP).
/// Environments consist of: locked flag, enclosing environment, frame (pairlist), hashtab
fn parse_environment(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Parse locked flag (raw integer: 0 or 1)
    // We read it but don't currently store it in the Environment struct.
    let _locked = cursor.read_i32::<BigEndian>()?;
    // Parse enclosing environment (can be another environment or NULL for global env)
    let enclosing = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    // Parse frame (pairlist of bindings)
    let frame = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    // Parse hashtab (can be NULL or a VECSXP)
    let hashtab = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    // Parse attributes (serialized even when NULL)
    let _attrs = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    // Note: attributes are NOT parsed here - they're handled by the HAS_ATTR flag
    // in parse_object

    Ok(RObject::Environment {
        enclosing: Box::new(enclosing),
        frame: Box::new(frame),
        hashtab: Box::new(hashtab),
    })
}

/// Parse a namespace environment (NAMESPACESXP, type 123).
/// Namespaces are special environments used by R packages.
/// They trigger automatic package loading when the RDS file is read in R.
fn parse_namespace(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Namespaces are serialized using OutStringVec: an unused marker,
    // a length, then that many CHARSXP entries.
    let _names_flag = cursor.read_u32::<BigEndian>()?;
    let length = cursor.read_u32::<BigEndian>()? as usize;
    guard_allocation(ctx, length, 1, cursor, "namespace names")?;

    let mut names = Vec::with_capacity(length);
    for _ in 0..length {
        // Each entry is written via WriteItem on a CHARSXP
        let obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        // Extract the string from the parsed object
        if let RObject::Character(chars) = obj {
            if let Some(s) = chars.first() {
                names.push(s.clone());
            }
        }
    }

    Ok(RObject::Namespace(names))
}

/// Parse a language object (LANGSXP).
/// Language objects represent unevaluated expressions/calls.
/// They're structured like pairlists: TAG (if present), CAR (function), CDR (arguments).
fn parse_language(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Parse the TAG if present (usually not for language objects)
    if has_tag {
        let _tag_obj = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
        // Tags in language objects are rare, we'll skip them for now
    }

    // Parse the CAR (the function being called)
    let function = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    // Parse the CDR (the argument list)
    let cdr = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    // Extract arguments with their names (tags) from the CDR
    let args = match cdr.into_concrete() {
        RObject::Null => {
            // No arguments
            Vec::new()
        }
        RObject::Pairlist(pairlist_elements) => {
            // Keep all arguments with their tags (names)
            pairlist_elements
        }
        other => {
            // Single argument (unusual but possible)
            vec![PairlistElement {
                tag: None,
                value: other,
                tag_object: None,
            }]
        }
    };

    Ok(RObject::Language {
        function: Box::new(function),
        args,
    })
}

/// Helper function to parse a single pairlist element (TAG if has_tag, then CAR).
/// Does NOT parse the CDR - that's handled by the iterative loop in parse_pairlist.
/// Returns (tag_name, tag_object, car_value).
#[allow(clippy::type_complexity)]
fn parse_pairlist_element(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<(Option<Arc<str>>, Option<Box<RObject>>, RObject)> {
    // Parse the TAG if present (comes before CAR)
    let (tag, tag_object) = if has_tag {
        // In attribute/tag positions, REFSXP indices point into the symbol table,
        // not the main reference table. Peek the next flags to handle this path.
        let pos = cursor.position();
        ensure_bytes_available(cursor, 4, "parse_pairlist_element:tag_flags")?;
        let flags = cursor.read_u32::<BigEndian>()?;
        let tag_type = flags & 0xFF;

        let tag_obj = if tag_type == REFSXP {
            let sym_index = flags >> 8;
            let prefer_symbol_table = true;

            // TAG REFSXP indices refer to the symbol table in R serialization.
            // Always prefer symbol table here, with ref_table as a fallback.
            if prefer_symbol_table {
                if let Some(sym) = symbol_table.get(sym_index) {
                    if std::env::var("RDS_DEBUG_TAG").is_ok() {
                        let name = extract_tag_name(sym.clone())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "<unknown>".to_string());
                        debug_log!(
                            "[TAG_REF] flags=0x{:08x} idx={} name={}",
                            flags,
                            sym_index,
                            name
                        );
                    }
                    sym.clone()
                } else if let Some(obj) = ref_table.get(sym_index) {
                    if std::env::var("RDS_DEBUG_REF_FALLBACK").is_ok() {
                        debug_log!(
                            "[TAG_REF_FALLBACK] idx={} sym_table={} ref_table={}",
                            sym_index,
                            symbol_table.len(),
                            ref_table.next_index - 1
                        );
                    }
                    obj.read().unwrap().clone()
                } else {
                    return Err(Error::InvalidFormat(format!(
                        "Invalid TAG REFSXP index {} (symbol table size={}, ref_table size={})",
                        sym_index,
                        symbol_table.len(),
                        ref_table.next_index - 1
                    )));
                }
            } else if let Some(obj) = ref_table.get(sym_index) {
                let obj_val = obj.read().unwrap().clone();
                if let Some(name) = extract_tag_name(obj_val.clone()) {
                    if std::env::var("RDS_DEBUG_TAG").is_ok() {
                        debug_log!(
                            "[TAG_REF] flags=0x{:08x} idx={} name={}",
                            flags,
                            sym_index,
                            name
                        );
                    }
                    obj_val
                } else if let Some(sym) = symbol_table.get(sym_index) {
                    if std::env::var("RDS_DEBUG_TAG").is_ok() {
                        let name = extract_tag_name(sym.clone())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "<unknown>".to_string());
                        debug_log!(
                            "[TAG_REF] flags=0x{:08x} idx={} name={}",
                            flags,
                            sym_index,
                            name
                        );
                    }
                    sym.clone()
                } else {
                    return Err(Error::InvalidFormat(format!(
                        "Invalid TAG REFSXP index {} (symbol table size={}, ref_table size={})",
                        sym_index,
                        symbol_table.len(),
                        ref_table.next_index - 1
                    )));
                }
            } else if let Some(sym) = symbol_table.get(sym_index) {
                if std::env::var("RDS_DEBUG_TAG").is_ok() {
                    let name = extract_tag_name(sym.clone())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string());
                    debug_log!(
                        "[TAG_REF] flags=0x{:08x} idx={} name={}",
                        flags,
                        sym_index,
                        name
                    );
                }
                sym.clone()
            } else {
                return Err(Error::InvalidFormat(format!(
                    "Invalid TAG REFSXP index {} (symbol table size={}, ref_table size={})",
                    sym_index,
                    symbol_table.len(),
                    ref_table.next_index - 1
                )));
            }
        } else {
            // Reset and parse normally for non-REFSXP tags.
            cursor.set_position(pos);
            parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?
        };

        // Extract the tag name from the symbol or character object
        let tag_name = extract_tag_name(tag_obj.clone());
        // Store both the extracted name and the raw object
        (tag_name, Some(Box::new(tag_obj)))
    } else {
        (None, None)
    };

    // Parse the CAR (the value for this element)
    let car = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    Ok((tag, tag_object, car))
}

/// Parse a pairlist (LISTSXP).
/// Uses an iterative approach matching R's ReadItem_Iterative to handle circular references.
/// R's serialization format: FLAGS (with type), TAG (if HAS_TAG_BIT), CAR, then FLAGS for next element.
/// If next FLAGS indicate LISTSXP/LANGSXP/etc., it's a continuation; otherwise it's the CDR terminator.
fn parse_pairlist(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let mut elements = Vec::new();
    let mut iterations: usize = 0;

    if debug_enabled() {
        debug_log!(
            "[PAIRLIST] Start parse, has_tag={}, pos={}",
            has_tag,
            cursor.position()
        );
    }

    // Parse the first element (TAG if has_tag, then CAR)
    let (first_tag, first_tag_object, first_car) =
        parse_pairlist_element(ctx, cursor, has_tag, ref_table, symbol_table, dedup_table)?;
    elements.push(PairlistElement {
        tag: first_tag,
        value: first_car,
        tag_object: first_tag_object,
    });

    // Now iteratively parse remaining elements
    // This mirrors R's ReadItem_Iterative which reads flags, checks type, and continues or exits
    loop {
        // Peek at next flags to determine if we continue or terminate
        let pos = cursor.position();
        let remaining = cursor.len() - pos;
        iterations += 1;
        if iterations > MAX_VECTOR_LENGTH {
            return Err(Error::InvalidFormat(
                "Pairlist exceeded maximum element cap".to_string(),
            ));
        }
        if remaining == 0 {
            if debug_enabled() {
                debug_log!(
                    "[PAIRLIST_LOOP] No bytes remaining at pos={}, treating as NIL CDR termination",
                    pos
                );
            }
            break;
        }
        let loop_start = pos;
        ensure_bytes_available(cursor, 4, "parse_pairlist:next_flags")?;
        let flags = cursor.read_u32::<BigEndian>()?;
        let next_type = flags & 0xFF;
        if debug_enabled() {
            debug_log!(
                "[PAIRLIST_LOOP] At byte {}: flags=0x{:08x}, type={}",
                pos,
                flags,
                next_type
            );
        }

        // Check if the next element continues the pairlist
        // R continues for: LISTSXP, LANGSXP, CLOSXP, PROMSXP, DOTSXP
        // IMPORTANT: Elements with tags (HAS_TAG_BIT) are also pairlist continuations.
        // This is critical for attribute pairlists where tagged elements of any type
        // (e.g., VECSXP or STRSXP entries) continue the pairlist.
        let has_tag_next = (flags & HAS_TAG_BIT) != 0;
        let continues_pairlist =
            has_tag_next || matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP);

        // INSTRUMENTATION: Log when we encounter types that don't continue pairlist
        if std::env::var("RDS_DEBUG_PAIRLIST_TERM").is_ok()
            && !continues_pairlist
            && next_type != REFSXP
        {
            debug_log!(
                "[PAIRLIST_TERM] Terminating at {} elements, next_type={}, flags=0x{:08x}",
                elements.len(),
                next_type,
                flags
            );
        }

        // SPECIAL: the CDR can be a REFSXP pointing to an existing pairlist tail.
        if next_type == REFSXP {
            // Rewind to let parse_object consume from the REFSXP flags we peeked.
            cursor.set_position(pos);
            let referenced = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
            match referenced.into_concrete() {
                RObject::Null => break,
                RObject::Pairlist(mut tail) => {
                    elements.append(&mut tail);
                    break;
                }
                other => {
                    return Err(Error::InvalidFormat(format!(
                        "Expected REFSXP tail to resolve to pairlist/NULL, got {:?}",
                        std::mem::discriminant(&other)
                    )));
                }
            }
        } else if continues_pairlist {
            // Continue building the pairlist - this is another element
            // The flags are already consumed, so parse_pairlist_element will read the TAG and CAR
            if debug_enabled() {
                debug_log!(
                    "[PAIRLIST_LOOP] Continuing pairlist, has_tag={}",
                    has_tag_next
                );
            }

            let (tag, tag_object, car) = parse_pairlist_element(
                ctx,
                cursor,
                has_tag_next,
                ref_table,
                symbol_table,
                dedup_table,
            )?;
            elements.push(PairlistElement {
                tag,
                value: car,
                tag_object,
            });
        } else {
            // Not a pairlist continuation - reset position and parse as CDR terminator
            cursor.set_position(pos);
            let cdr = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

            // Unwrap Shared if present to check the actual type
            let cdr_concrete = cdr.into_concrete();

            // Handle the CDR based on its type
            match cdr_concrete {
                RObject::Null => {
                    // Normal list termination
                    // INSTRUMENTATION: Check if there's more data after NULL terminator
                    if std::env::var("RDS_DEBUG_PAIRLIST_AFTER_NULL").is_ok() {
                        let remaining = cursor.len().saturating_sub(cursor.position());
                        debug_log!("[PAIRLIST_AFTER_NULL] {} elements parsed, {} bytes remaining at pos={}",
                            elements.len(), remaining, cursor.position());
                        if remaining >= 4 {
                            let pos_save = cursor.position();
                            if let Ok(next_flags) = cursor.read_u32::<BigEndian>() {
                                let next_type = next_flags & 0xFF;
                                debug_log!(
                                    "[PAIRLIST_AFTER_NULL] Next: flags=0x{:08x}, type={}",
                                    next_flags,
                                    next_type
                                );
                                cursor.set_position(pos_save);
                            }
                        }
                    }
                    break;
                }
                RObject::Pairlist(mut rest) => {
                    // CDR is another pairlist (rare but possible) - append elements
                    elements.append(&mut rest);
                    break;
                }
                other => {
                    // CDR is some other object - add it as untagged element
                    elements.push(PairlistElement {
                        tag: None,
                        value: other,
                        tag_object: None,
                    });
                    break;
                }
            }
        }

        // Ensure forward progress to avoid infinite loops that allocate unboundedly.
        if cursor.position() <= loop_start {
            return Err(Error::InvalidFormat(format!(
                "Pairlist parser made no progress at pos {}",
                loop_start
            )));
        }
    }

    // INSTRUMENTATION: Log all tags in the final pairlist
    if std::env::var("RDS_DEBUG_PAIRLIST_FINAL").is_ok() && !elements.is_empty() {
        debug_log!("[PAIRLIST_FINAL] Returning {} elements:", elements.len());
        for (i, elem) in elements.iter().enumerate() {
            debug_log!("  [{}] tag={:?}", i, elem.tag.as_deref());
        }
    }

    Ok(RObject::Pairlist(elements))
}

/// Extract a tag name from a tag object (usually a symbol or character).
///
/// Returns `None` if the tag is lazy (not loaded), to prevent panics during
/// bytecode parsing with large character vectors.
fn extract_tag_name(tag_obj: RObject) -> Option<Arc<str>> {
    // Unwrap Shared wrappers first
    let tag_obj = tag_obj.into_concrete();

    match tag_obj {
        RObject::Symbol(name) => Some(name),
        // Only extract from loaded character vectors to avoid panics
        RObject::Character(vec) if !vec.is_empty() && vec.is_loaded() => Some(vec[0].clone()),
        RObject::Null => None,
        _ => None, // Includes lazy character vectors - return None gracefully
    }
}

/// Parse a promise (PROMSXP).
/// Promises are lazy evaluation constructs containing: value, expression, environment
fn parse_promise(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // PROMSXP is serialized like a dotted pair: TAG (environment) if present,
    // then CAR (value), then CDR (expression).
    let environment = if has_tag {
        parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?
    } else {
        RObject::Null
    };
    let value = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;
    let expression = parse_object(ctx, cursor, ref_table, symbol_table, dedup_table)?;

    Ok(RObject::Promise {
        value: Box::new(value),
        expression: Box::new(expression),
        environment: Box::new(environment),
    })
}

/// Parse a special primitive function (SPECIALSXP).
/// Special functions like 'if', 'for', 'while' have special evaluation rules.
/// Format: type flag, then length (i32), then name bytes (no SYMSXP wrapper)
fn parse_special(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    _ref_table: &mut RefTable,
    _symbol_table: &mut SymbolTable,
    _dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Read the string length
    let length = cursor.read_i32::<BigEndian>()?;

    if length < 0 {
        return Err(Error::InvalidFormat(
            "Negative length for special function name".to_string(),
        ));
    }

    let length = length as usize;
    guard_allocation(ctx, length, 1, cursor, "special function name")?;
    ensure_bytes_available(cursor, length, "special:name_bytes")?;

    // Read the string bytes
    let mut bytes = vec![0u8; length];
    cursor.read_exact(&mut bytes)?;

    // Convert to UTF-8 string and intern it
    let name = String::from_utf8(bytes)?;
    let name = Arc::from(name.as_str());

    Ok(RObject::Special { name })
}

/// Parse a builtin primitive function (BUILTINSXP).
/// Builtin functions like 'sum', 'c', '+' are internal R functions.
/// Format: type flag, then length (i32), then name bytes (no SYMSXP wrapper)
fn parse_builtin(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    _ref_table: &mut RefTable,
    _symbol_table: &mut SymbolTable,
    _dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Read the string length
    let length = cursor.read_i32::<BigEndian>()?;

    if length < 0 {
        return Err(Error::InvalidFormat(
            "Negative length for builtin function name".to_string(),
        ));
    }

    let length = length as usize;
    guard_allocation(ctx, length, 1, cursor, "builtin function name")?;
    ensure_bytes_available(cursor, length, "builtin:name_bytes")?;

    // Read the string bytes
    let mut bytes = vec![0u8; length];
    cursor.read_exact(&mut bytes)?;

    // Convert to UTF-8 string and intern it
    let name = String::from_utf8(bytes)?;
    let name = Arc::from(name.as_str());

    Ok(RObject::Builtin { name })
}

/// Convert an ALTREP object to its native representation.
fn convert_altrep_to_native(
    ctx: &mut ParserContext,
    class_info: RObject,
    state: RObject,
) -> Result<RObject> {
    // Debug logging to understand ALTREP structure
    let class_info = class_info.into_concrete();
    let state = state.into_concrete();

    // Try to extract ALTREP class name from class_info
    let altrep_class_name = extract_altrep_class_name(&class_info);

    // Handle different ALTREP types based on class name or state structure
    if let Some(class_name) = altrep_class_name {
        match class_name.as_str() {
            "wrap_real" => {
                return convert_wrap_real(state);
            }
            "wrap_int" => {
                return convert_wrap_int(state);
            }
            "compact_intseq" => {
                return convert_compact_intseq(ctx, state);
            }
            "compact_realseq" => {
                return convert_compact_intseq(ctx, state);
            }
            _ => {}
        }
    }

    // Fallback: Infer the ALTREP type from the state structure
    // compact_intseq has state: [length (real), first (real), stride (real)]
    match &state {
        RObject::Real(params) if params.len() == 3 => {
            // Standard compact_intseq with state vector
            convert_compact_intseq(ctx, state)
        }
        RObject::Integer(params) if params.len() == 3 => {
            // Sometimes the state is stored as integers instead of reals
            let real_params = vec![params[0] as f64, params[1] as f64, params[2] as f64];
            convert_compact_intseq(ctx, RObject::Real(real_params.into()))
        }
        RObject::Integer(vec) if vec.len() == 1 && vec[0] == 13 => {
            // Special case: when state is Integer([13]), R has stored the actual data
            // in the class_info field (likely as a REFSXP or pairlist containing the data)
            // Extract the actual data from class_info
            match class_info {
                RObject::Pairlist(elements) if !elements.is_empty() => {
                    // The first element should contain the actual data
                    Ok(elements[0].value.clone())
                }
                other => Ok(other),
            }
        }
        RObject::Pairlist(_) => {
            // ALTREP wrappers often have pairlist state containing the actual data
            convert_altrep_pairlist_state(state)
        }
        _ => {
            // For unsupported ALTREP types or compressed ALTREP references,
            // just return NULL for now. This is a known limitation for some
            // R-specific ALTREP optimizations when serializing repeated instances.
            Ok(RObject::Null)
        }
    }
}

/// Extract the ALTREP class name from class_info.
/// Returns the simple class name (e.g., "wrap_real", "compact_intseq").
fn extract_altrep_class_name(class_info: &RObject) -> Option<String> {
    let class_info = class_info.as_concrete();
    // class_info can be:
    // 1. Character vector with [package, class]
    // 2. Pairlist with class information
    // 3. List with class information
    match class_info {
        RObject::Character(vec) if vec.len() >= 2 => {
            // Return the class name (second element)
            let class_name = vec[1].to_string();
            Some(class_name)
        }
        RObject::Character(vec) if vec.len() == 1 => {
            // Sometimes just the class name
            let class_name = vec[0].to_string();
            Some(class_name)
        }
        RObject::Symbol(name) => Some(name.to_string()),
        RObject::Pairlist(elements) => {
            // Pairlist might contain [package_symbol, class_symbol, ...]
            // Symbols are stored as Character vectors

            // Look through pairlist elements for character data
            for elem in elements.iter() {
                // Check if this is a character vector (symbol converted to character)
                if let RObject::Character(vec) = &elem.value {
                    if !vec.is_empty() {
                        let class_name = vec[0].to_string();
                        // Common ALTREP class names
                        if class_name.contains("wrap_")
                            || class_name.contains("compact_")
                            || class_name.contains("deferred_")
                        {
                            return Some(class_name);
                        }
                    }
                }
            }

            // If we have at least 2 elements, try the second one (often the class)
            if elements.len() >= 2 {
                if let RObject::Character(vec) = &elements[1].value {
                    if !vec.is_empty() {
                        let class_name = vec[0].to_string();
                        return Some(class_name);
                    }
                }
            }
            None
        }
        RObject::List(elements) if elements.len() >= 2 => {
            // List might contain [package, class]
            if let RObject::Character(vec) = &elements[1] {
                if !vec.is_empty() {
                    return Some(vec[0].to_string());
                }
            }
            None
        }
        RObject::WithAttributes { object, .. } => {
            // Class info might be wrapped with attributes
            extract_altrep_class_name(&object)
        }
        _ => None,
    }
}

/// Convert a wrap_real ALTREP object to a native real vector.
fn convert_wrap_real(state: RObject) -> Result<RObject> {
    let state = state.into_concrete();
    // For wrap_real, the state contains the actual real vector
    match state {
        RObject::Real(_) => {
            // Already a real vector, return as-is
            Ok(state)
        }
        RObject::Pairlist(elements) if !elements.is_empty() => {
            // State is a pairlist, extract the first element which should be the data
            Ok(elements[0].value.clone())
        }
        RObject::List(elements) if !elements.is_empty() => {
            // State is a list, extract the first element
            Ok(elements[0].clone())
        }
        RObject::WithAttributes { object, .. } => {
            // Unwrap attributes and try again
            convert_wrap_real(*object)
        }
        _ => Ok(RObject::Null),
    }
}

/// Convert a wrap_int ALTREP object to a native integer vector.
fn convert_wrap_int(state: RObject) -> Result<RObject> {
    let state = state.into_concrete();
    // For wrap_int, the state contains the actual integer vector
    match state {
        RObject::Integer(_) => {
            // Already an integer vector, return as-is
            Ok(state)
        }
        RObject::Pairlist(elements) if !elements.is_empty() => {
            // State is a pairlist, extract the first element which should be the data
            Ok(elements[0].value.clone())
        }
        RObject::List(elements) if !elements.is_empty() => {
            // State is a list, extract the first element
            Ok(elements[0].clone())
        }
        RObject::WithAttributes { object, .. } => {
            // Unwrap attributes and try again
            convert_wrap_int(*object)
        }
        _ => Ok(RObject::Null),
    }
}

/// Convert an ALTREP object with pairlist state to a native object.
/// This handles generic ALTREP wrappers where the data is in a pairlist.
fn convert_altrep_pairlist_state(state: RObject) -> Result<RObject> {
    match state.into_concrete() {
        RObject::Pairlist(elements) if !elements.is_empty() => {
            // The actual data is typically in the first element
            Ok(elements[0].value.clone())
        }
        _ => Ok(RObject::Null),
    }
}

/// Convert a compact integer sequence to a regular integer vector.
fn convert_compact_intseq(ctx: &mut ParserContext, state: RObject) -> Result<RObject> {
    let state = state.into_concrete();
    // compact_intseq state is a Real vector: [length, first, stride]
    let (length, first, stride) = match state {
        RObject::Real(params) if params.len() == 3 => {
            if params.is_loaded() {
                let len = params[0] as i64;
                let first_val = params[1] as i32;
                let stride_val = params[2] as i32;
                (len, first_val, stride_val)
            } else if matches!(ctx.mode, crate::ParseMode::LazyMetadata) {
                return Ok(RObject::Null);
            } else {
                return Err(Error::InvalidFormat(
                    "Invalid compact_intseq state".to_string(),
                ));
            }
        }
        _ => {
            return Err(Error::InvalidFormat(
                "Invalid compact_intseq state".to_string(),
            ))
        }
    };

    // Generate the sequence
    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative compact_intseq length {}",
            length
        )));
    }
    let length_usize = length as usize;
    // Each element is an i32
    guard_allocation_common(
        ctx,
        length_usize,
        std::mem::size_of::<i32>(),
        "compact_intseq",
    )?;
    let mut vec = Vec::with_capacity(length_usize);
    for i in 0..length_usize {
        vec.push(first + (i as i32) * stride);
    }

    Ok(RObject::Integer(vec.into()))
}

/// Parse a CHARSXP (internal character string).
fn parse_charsxp(ctx: &mut ParserContext, cursor: &mut RdsCursor<'_>) -> Result<String> {
    // Read the CHARSXP header
    let flags = cursor.read_u32::<BigEndian>()?;
    let type_from_0_7 = flags & 0xFF;
    let type_from_8_15 = (flags >> 8) & 0xFF;

    // Check for HAS_ATTR_BIT - CHARSXP can have attributes (e.g., encoding)
    let has_attr = (flags & HAS_ATTR_BIT) != 0;

    // Check both positions for CHARSXP (type 9) FIRST
    // This is important because flags might have type 0 in bits 0-7 but type 9 in bits 8-15
    if type_from_8_15 == CHARSXP || type_from_0_7 == CHARSXP {
        // Parse the string content, passing flags to detect compact encoding
        let string = parse_charsxp_content(ctx, cursor, flags)?;

        // If there are attributes, we need to skip them (they're just metadata like encoding)
        // For CHARSXP, attributes come AFTER the string data (unlike LISTSXP where they come before)
        if has_attr {
            // Read and discard the attributes
            // We can't use parse_object here as we're in a lower-level function
            // Just read the attributes length and skip that many bytes
            // Actually, we need to properly parse and discard the attribute object
            // This is tricky - CHARSXP attributes are rare, usually just encoding info
        }

        return Ok(string);
    }

    // Handle NULL as NA_character_
    if type_from_0_7 == NILSXP || type_from_0_7 == NILVALUE_SXP {
        return Ok(String::from("NA"));
    }

    // Handle REFSXP - this can appear when a symbol name is a reference to a previously seen string
    // This is a limitation: we can't look up the reference here without access to caches/tables.
    // For now, return a placeholder. In the future, parse_charsxp should accept cache parameters.
    if type_from_0_7 == REFSXP {
        let ref_index = (flags >> 8) as usize;
        // Return a placeholder indicating this is a reference
        // The caller (parse_character_vector or parse_symbol) should handle this
        return Err(Error::InvalidFormat(format!(
            "REFSXP in CHARSXP context requires caller to handle reference (ref={})",
            ref_index
        )));
    }

    // If we get here, the flags don't contain CHARSXP type
    // This shouldn't happen in a well-formed file - it indicates a parsing error
    debug_log!("[DEBUG parse_charsxp] Unexpected type:");
    debug_log!("  Full flags: 0x{:08x}", flags);
    debug_log!("  Type from bits 0-7: {}", type_from_0_7);
    debug_log!("  Type from bits 8-15: {}", type_from_8_15);
    debug_log!("  Position: {}", cursor.position());

    Err(Error::InvalidFormat(format!(
        "Expected CHARSXP ({}), got {} (flags: 0x{:08x})",
        CHARSXP, type_from_0_7, flags
    )))
}

/// Parse the content of a CHARSXP (without the header).
///
/// R normally uses 4-byte big-endian integers for lengths (R_XDR_INTEGER_SIZE = 4).
/// However, some R versions or serialization contexts use a compact 3-byte encoding
/// signaled by bits 24-31 of the flags being non-zero (specifically 0x04).
///
/// The compact encoding is detected by checking bits 24-31 of the flags field.
fn parse_charsxp_content(
    ctx: &mut ParserContext,
    cursor: &mut RdsCursor<'_>,
    flags: u32,
) -> Result<String> {
    let pos_before = cursor.position();
    if debug_enabled() {
        debug_log!("[parse_charsxp_content] Starting at pos {}", pos_before);
    }

    // Check if this uses compact 3-byte length encoding.
    // Compact encoding is signaled by bits 24-31 being non-zero (e.g., 0x04000900).
    let compact_flag = (flags >> 24) & 0xFF;
    let use_compact = compact_flag > 0;

    // Peek at the next 8 bytes for debugging

    let length = if use_compact {
        // Read 3-byte length (big-endian)
        let mut bytes_3 = [0u8; 3];
        ensure_bytes_available(cursor, 3, "charsxp:compact_len")?;
        cursor.read_exact(&mut bytes_3)?;

        ((bytes_3[0] as i32) << 16) | ((bytes_3[1] as i32) << 8) | (bytes_3[2] as i32)
    } else {
        // Read standard 4-byte length (R always uses 4-byte integers in standard mode)
        ensure_bytes_available(cursor, 4, "charsxp:std_len")?;

        cursor.read_i32::<BigEndian>()?
    };

    if length == -1 {
        // NA_character_
        return Ok(String::from("NA"));
    }

    if length < 0 {
        return Err(Error::InvalidFormat(format!(
            "Negative CHARSXP length {}",
            length
        )));
    }

    let length = length as usize;
    guard_allocation(ctx, length, 1, cursor, "charsxp content")?;

    // Read the string bytes
    let mut bytes = vec![0u8; length];
    ensure_bytes_available(cursor, length, "charsxp:string_bytes")?;
    cursor.read_exact(&mut bytes)?;

    // Try to convert to UTF-8 string
    // If it fails, it might be Latin-1 or another encoding
    let string = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            // Try to interpret as Latin-1 (ISO-8859-1) and convert to UTF-8
            // Latin-1 bytes 0-255 map directly to Unicode codepoints 0-255
            bytes.iter().map(|&b| b as char).collect()
        }
    };

    Ok(string)
}

/// Parse attributes from a pairlist object.
/// Attributes are stored as pairlists where TAG = attribute name, CAR = attribute value.
fn parse_attributes(attr_obj: RObject, ctx: &mut ParserContext) -> Result<Attributes> {
    // INSTRUMENTATION: Log BEFORE unwrapping Shared to see what we receive
    if std::env::var("RDS_DEBUG_ATTR_UNWRAP").is_ok() {
        thread_local! {
            static CALL_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        }
        let count = CALL_COUNT.with(|c| {
            let n = c.get() + 1;
            c.set(n);
            n
        });
        debug_log!(
            "[PARSE_ATTRS_PRE #{}] Received type: {:?}, is_shared={}",
            count,
            std::mem::discriminant(&attr_obj),
            matches!(attr_obj, RObject::Shared(_))
        );
        if let RObject::Shared(ref inner) = attr_obj {
            let inner_obj = inner.read().unwrap();
            debug_log!(
                "[PARSE_ATTRS_PRE #{}] Shared wraps: {:?}",
                count,
                std::mem::discriminant(&*inner_obj)
            );
            if let RObject::Pairlist(ref elems) = *inner_obj {
                debug_log!(
                    "[PARSE_ATTRS_PRE #{}] Shared->Pairlist with {} elements",
                    count,
                    elems.len()
                );
            } else if let RObject::Symbol(ref name) = *inner_obj {
                debug_log!("[PARSE_ATTRS_PRE #{}] Shared->Symbol: '{}'", count, name);
            }
        }
    }

    let attr_obj = attr_obj.into_concrete();
    let mut attrs = Attributes::new();

    fn store_attrs_for_class(attrs: &Attributes, ctx: &mut ParserContext) {
        if attrs.get("class").is_some() || attrs.get("package").is_some() {
            if let Some(existing) = ctx.pending_class_attrs.take() {
                let mut merged = existing;
                for (k, v) in attrs.iter() {
                    if !merged.attrs.iter().any(|(ek, _)| ek == k) {
                        merged.insert(k.clone(), v.clone());
                    }
                }
                ctx.pending_class_attrs = Some(merged);
            } else {
                ctx.pending_class_attrs = Some(attrs.clone());
            }
        }
    }

    if debug_enabled() {
        debug_log!(
            "[PARSE_ATTRS] Received attr_obj type: {:?}",
            std::mem::discriminant(&attr_obj)
        );
    }

    // Attributes are typically stored as a pairlist (LISTSXP)
    // We need to extract the TAG (name) and CAR (value) from each pair
    match attr_obj {
        RObject::Null => {
            // No attributes
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::Pairlist(elements) => {
            // Extract TAG (name) and CAR (value) from each pairlist element
            if debug_enabled() && !elements.is_empty() {
                debug_log!("[PARSE_ATTRS] Pairlist with {} elements", elements.len());
            }
            for elem in elements {
                // SPECIAL CASE: Check if tag_object contains an S4Object
                // This happens when TAG is REFSXP→S4Object. If the tag_object is an S4Object
                // and we haven't extracted a tag name, it might be the actual object we want.
                if elem.tag.is_none() {
                    if let Some(tag_obj) = &elem.tag_object {
                        if let RObject::S4Object(_s4) = tag_obj.as_ref() {
                            // Found an S4Object in the TAG position without a tag name!
                            // Store it with a special marker so convert_to_s4_object can find it
                            attrs.insert(Arc::from("__tag_s4_object__"), *tag_obj.clone());
                            continue; // Skip the normal processing for this element
                        }
                    }
                }

                if let Some(name) = elem.tag {
                    if std::env::var("RDS_DEBUG_ATTR_VALUES").is_ok() {
                        debug_log!(
                            "[ATTR_VAL] name='{}' type={:?}",
                            name,
                            std::mem::discriminant(&elem.value)
                        );
                    }
                    if debug_enabled() {
                        debug_log!(
                            "[PARSE_ATTRS]   Tag: '{}' -> {:?}",
                            name,
                            std::mem::discriminant(&elem.value)
                        );
                    }
                    attrs.insert(name.clone(), elem.value);
                } else {
                    // No explicit tag - check if this is a special case like "class" or a reference to an actual object
                    // In R serialization, the class attribute for S4 objects can be stored without a tag
                    // as WithAttributes(Character) with package information
                    // We ONLY infer "class" for WithAttributes(Character), not plain Character,
                    // to avoid false positives with expression lists
                    //
                    // SPECIAL CASE: If the element's TAG was a REFSXP that resolved to an S4Object,
                    // the elem.tag would be None (since extract_tag_name returned None), but the actual
                    // TAG object (before extraction) contained the S4Object. Unfortunately, we've lost
                    // that information by this point. However, we can detect this pattern:
                    // If an element has no tag but its VALUE is an S4Object, it might be the real object!
                    let inferred_name = match &elem.value {
                        RObject::WithAttributes {
                            object,
                            attributes: inner_attrs,
                        } => {
                            // If it's WithAttributes wrapping a Character, it's likely "class"
                            // S4 classes often have package info stored in attributes
                            match object.as_ref() {
                                RObject::Character(_chars) if !inner_attrs.is_empty() => {
                                    // This is likely a class with package information
                                    Some(Arc::from("class"))
                                }
                                _ => None,
                            }
                        }
                        RObject::S4Object(_s4) => {
                            // An S4Object without a tag might be a reference to the actual object
                            // Store it with a special marker key so convert_to_s4_object can find it
                            Some(Arc::from("__ref_object__"))
                        }
                        _ => None,
                    };

                    if let Some(name) = inferred_name {
                        attrs.insert(name, elem.value);
                    }
                    // Otherwise, skip elements without tags that we can't identify
                }
            }
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::List(_elements) => {
            // Regular list (VECSXP) - names should be stored as a "names" attribute
            // This case shouldn't happen for attributes themselves, but handle it gracefully
            // Just return empty attributes
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::WithAttributes {
            object,
            attributes: inner_attrs,
        } => {
            if let RObject::List(list) = object.as_ref() {
                if let Some(names_obj) = inner_attrs.get("names") {
                    if let RObject::Character(names) = names_obj.as_concrete() {
                        let mut attrs = Attributes::new();
                        for (idx, name) in names.iter().enumerate() {
                            if name.is_empty() || idx >= list.len() {
                                continue;
                            }
                            attrs.insert(name.clone(), list[idx].clone());
                        }
                        store_attrs_for_class(&attrs, ctx);
                        return Ok(attrs);
                    }
                }
            }

            // When we receive a WithAttributes as an attributes object,
            // we should return its attributes field directly, not transform it.
            // The inner_attrs already contains the parsed attributes (like "names", "row.names", etc.)
            store_attrs_for_class(&inner_attrs, ctx);
            Ok(inner_attrs.clone())
        }
        RObject::Integer(vec) if vec.len() == 1 => {
            // Single integer might be a reference index or special marker
            // In some cases, R uses compact formats for attributes
            // For now, treat as no attributes
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::Real(vec) if vec.len() == 3 => {
            // This might be ALTREP state being passed as attributes
            // This shouldn't happen, but handle it gracefully
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::Character(_names) => {
            // This might be a compact attribute format where we only have names
            // This can happen with certain R objects where attributes are stored
            // in a special compact format. For now, treat as no attributes.
            // In the future, we may need to look up values elsewhere.
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        RObject::S3Object(s3) => {
            // S3 object used as attributes container
            // Extract the attributes from the S3 object
            store_attrs_for_class(&s3.attributes, ctx);
            Ok(s3.attributes.clone())
        }
        RObject::S4Object(s4) => {
            // S4 object used as attributes container
            // Extract the class field and slots and convert them to attributes
            // Add the class as a "class" attribute (RObject::Character)
            if !s4.class.is_empty() {
                attrs.insert(
                    Arc::from("class"),
                    RObject::Character(s4.class.clone().into()),
                );
            }
            for (slot_name, slot_value) in &s4.slots {
                attrs.insert(slot_name.clone(), slot_value.clone());
            }
            store_attrs_for_class(&attrs, ctx);
            Ok(attrs)
        }
        _ => {
            // Unexpected attribute structure - this can happen with certain R serialization patterns
            // For example, when attributes are encoded using alternate representations
            // Return empty attributes with a warning rather than failing
            store_attrs_for_class(&attrs, ctx);
            Ok(Attributes::new())
        }
    }
}

/// Try to convert a list with attributes to a data.frame if it has the right structure.
fn try_convert_to_dataframe(obj: &RObject, attributes: &Attributes) -> Option<RObject> {
    // Check if this has class="data.frame"
    let class_attr = attributes.get("class")?.as_concrete();
    let is_dataframe = match class_attr {
        RObject::Character(classes) => {
            classes.is_loaded() && classes.iter().any(|c| c.as_ref() == "data.frame")
        }
        _ => false,
    };

    if !is_dataframe {
        return None;
    }

    // The object should be a list (columns)
    let columns_list = match obj {
        RObject::List(cols) => cols,
        _ => return None,
    };

    // Get the column names from the "names" attribute
    let names_attr = attributes.get("names")?.as_concrete();
    let column_names = match names_attr {
        RObject::Character(names) => names.clone(),
        _ => return None,
    };

    // Check that we have the same number of names as columns
    if column_names.len() != columns_list.len() {
        return None;
    }

    // Build the columns IndexMap (preserves insertion order from R)
    let mut columns = IndexMap::new();
    for (name, column) in column_names.iter().zip(columns_list.iter()) {
        columns.insert(name.clone(), column.clone());
    }

    // Get row names from the "row.names" attribute
    let row_names = if let Some(row_names_attr) = attributes.get("row.names") {
        match row_names_attr.as_concrete() {
            RObject::Character(names) if names.is_loaded() => names.as_vec().clone(),
            RObject::Integer(indices) if indices.is_loaded() => {
                // R uses a compact representation for default row names:
                // A 2-element vector [NA_integer_, -n] represents row names 1:n
                // where n is the number of rows
                if indices.len() == 2 && indices[0] == RObject::NA_INTEGER && indices[1] < 0 {
                    // Compact format: expand to ["1", "2", ..., "n"]
                    let n = -indices[1] as usize;
                    (1..=n).map(|i| Arc::from(i.to_string().as_str())).collect()
                } else {
                    // Explicit integer row names: convert to strings
                    indices
                        .iter()
                        .map(|i| Arc::from(i.to_string().as_str()))
                        .collect()
                }
            }
            _ => {
                // Default row names: just number them based on first column length
                (1..=columns_list
                    .first()
                    .map(|c| match c {
                        RObject::Integer(v) => v.len(),
                        RObject::Real(v) => v.len(),
                        RObject::Logical(v) => v.len(),
                        RObject::Character(v) => v.len(),
                        _ => 0,
                    })
                    .unwrap_or(0))
                    .map(|i| Arc::from(i.to_string().as_str()))
                    .collect()
            }
        }
    } else {
        // No row.names attribute, create default based on first column length
        let n = columns_list
            .first()
            .map(|c| match c {
                RObject::Integer(v) => v.len(),
                RObject::Real(v) => v.len(),
                RObject::Logical(v) => v.len(),
                RObject::Character(v) => v.len(),
                _ => 0,
            })
            .unwrap_or(0);
        (1..=n).map(|i| Arc::from(i.to_string().as_str())).collect()
    };

    Some(RObject::DataFrame(Box::new(DataFrameData {
        columns,
        row_names,
    })))
}

/// Try to convert an object with attributes to a Factor.
/// Returns Some(Factor) if it's a factor, None otherwise.
fn try_convert_to_factor(obj: &RObject, attributes: &Attributes) -> Option<RObject> {
    // Check if the class attribute indicates this is a factor
    let class_attr = attributes.get("class")?.as_concrete();
    let classes = match class_attr {
        RObject::Character(classes) => classes,
        _ => return None,
    };

    // Check if "factor" is in the class list
    let is_factor = classes.iter().any(|c| c.as_ref() == "factor");
    if !is_factor {
        return None;
    }

    // Check if it's an ordered factor
    let ordered = classes.iter().any(|c| c.as_ref() == "ordered");

    // The base object should be an integer vector (the indices)
    let values = match obj {
        RObject::Integer(vals) if vals.is_loaded() => vals.as_vec().clone(),
        _ => return None,
    };

    // Get the levels from the "levels" attribute
    let levels_attr = attributes.get("levels")?.as_concrete();
    let levels = match levels_attr {
        RObject::Character(levels) if levels.is_loaded() => levels.as_vec().clone(),
        _ => return None,
    };

    let factor = RObject::Factor(Box::new(FactorData {
        values,
        levels,
        ordered,
    }));

    // Preserve any additional attributes (e.g., names, contrasts) by wrapping
    // the factor in WithAttributes.
    let mut extra_attrs = Attributes::new();
    for (key, value) in attributes.iter() {
        if matches!(key.as_ref(), "levels" | "class") {
            continue;
        }
        extra_attrs.insert(key.clone(), value.as_concrete());
    }

    if extra_attrs.is_empty() {
        Some(factor)
    } else {
        Some(RObject::WithAttributes {
            object: Box::new(factor),
            attributes: extra_attrs,
        })
    }
}

/// Convert an object with attributes to an S3 object.
/// Assumes the class attribute has already been checked.
fn convert_to_s3_object(obj: RObject, mut attributes: Attributes) -> RObject {
    // Extract the class attribute from SmallVec
    let class = attributes
        .attrs
        .iter()
        .position(|(k, _)| k.as_ref() == "class")
        .and_then(|idx| match attributes.attrs[idx].1.as_concrete() {
            RObject::Character(classes) => {
                if classes.is_loaded() {
                    Some(classes.as_vec().clone())
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap_or_default();

    // Remove class from attributes
    attributes.attrs.retain(|(k, _)| k.as_ref() != "class");

    // Normalize attribute values to concrete objects (unwrap Shared) for user-facing API.
    let mut normalized_attrs = Attributes::new();
    for (k, v) in attributes.attrs.into_iter() {
        normalized_attrs.insert(k, v.as_concrete());
    }

    // Create the S3 object
    RObject::S3Object(Box::new(S3ObjectData {
        base: Box::new(obj),
        class,
        attributes: normalized_attrs,
    }))
}

/// Convert attributes to an S4 object.
/// For S4 objects, the class is in attributes, and all other attributes are slots.
fn convert_to_s4_object(mut attributes: Attributes) -> RObject {
    let debug_s4 = std::env::var("RDS_DEBUG_S4").is_ok();
    if debug_s4 {
        debug_log!(
            "[S4] starting convert_to_s4_object with attrs: {:?}",
            attributes
                .attrs
                .iter()
                .map(|(k, v)| (k.as_ref(), std::mem::discriminant(v.as_ref())))
                .collect::<Vec<_>>(),
        );
    }

    // Extract the class attribute and package attribute
    // The class may be wrapped in WithAttributes if it has a package attribute
    // It may also be wrapped in Shared due to reference tracking
    let (class, package) = attributes
        .attrs
        .iter()
        .position(|(k, _)| k.as_ref() == "class")
        .map(|idx| {
            // Unwrap any Shared wrapper first
            let class_obj = attributes.attrs[idx].1.as_concrete();

            if debug_s4 {
                debug_log!("[S4] class attribute variant: {}", class_obj.variant_name());
            }

            match class_obj {
                RObject::Character(classes) => {
                    if classes.is_loaded() {
                        (classes.as_vec().clone(), None)
                    } else {
                        (vec![], None)
                    }
                }
                RObject::WithAttributes {
                    object,
                    attributes: class_attrs,
                } => {
                    // Unwrap the WithAttributes to get the actual class vector
                    let classes = match object.as_ref() {
                        RObject::Character(classes) => {
                            if classes.is_loaded() {
                                classes.as_vec().clone()
                            } else {
                                vec![]
                            }
                        }
                        _ => vec![],
                    };
                    // Extract the package attribute from the class's attributes
                    let pkg = class_attrs.get("package").and_then(|p| match p {
                        RObject::Character(pkgs) if pkgs.is_loaded() && !pkgs.is_empty() => {
                            Some(pkgs[0].clone())
                        }
                        _ => None,
                    });
                    (classes, pkg)
                }
                RObject::Symbol(ref name) => {
                    if debug_s4 {
                        debug_log!("[S4] WARNING: class is Symbol: '{}'", name);
                    }
                    (vec![], None)
                }
                _ => {
                    if debug_s4 {
                        debug_log!(
                            "[S4] WARNING: Unexpected class attribute type: {}",
                            class_obj.variant_name()
                        );
                    }
                    (vec![], None)
                }
            }
        })
        .unwrap_or((vec![], None));

    // Fallback: package may also be stored as a separate attribute
    let mut package = package;
    if package.is_none() {
        if let Some(pkg_attr) = attributes.get("package") {
            if let RObject::Character(pkgs) = pkg_attr.as_concrete() {
                if let Some(first) = pkgs.first() {
                    package = Some(first.clone());
                }
            }
        }
    }

    if debug_s4 {
        debug_log!("[S4] extracted class {:?}, package {:?}", class, package);
    }

    // WORKAROUND: Check if we have an S4Object from a TAG position that matches this class
    // This can happen when the attributes pairlist has REFSXP in TAG positions that resolve to S4Objects.
    // If we find one with matching class, use it directly instead of building a malformed object.
    if !class.is_empty() {
        if let Some(tag_s4_obj) = attributes.get("__tag_s4_object__") {
            // tag_s4_obj is a &RObject
            if let RObject::S4Object(ref s4) = tag_s4_obj {
                if s4.class == class {
                    // Found an S4 object in TAG position with matching class!
                    // This is the actual correct object. Return it directly.
                    return tag_s4_obj.clone();
                }
            }
        }
    }

    // Remove class, package, and special marker attributes
    attributes.attrs.retain(|(k, _)| {
        k.as_ref() != "class"
            && k.as_ref() != "package"
            && k.as_ref() != "__tag_s4_object__"
            && k.as_ref() != "__ref_object__"
    });

    // All remaining attributes are the slots (using IndexMap to preserve order)
    let mut slots = IndexMap::new();
    for (key, value) in attributes.attrs.into_iter() {
        slots.insert(key, *value); // Unbox the RObject
    }

    RObject::S4Object(Box::new(S4ObjectData {
        class,
        package,
        slots,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_parse_header() {
        // This is a minimal RDS header for format version 2
        let header = vec![
            b'X', b'\n', // Magic bytes
            0, 0, 0, 2, // Format version (2)
            0, 3, 5, 0, // R version 3.5.0
            0, 3, 0, 0, // Min R version 3.0.0
        ];

        let mut cursor = RdsCursor::new_slice(header.as_slice());
        let version = parse_header(&mut cursor).unwrap();
        assert_eq!(version, 2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_invalid_magic() {
        let header = vec![b'Y', b'\n', 0, 0, 0, 2];
        let mut cursor = RdsCursor::new_slice(header.as_slice());
        assert!(parse_header(&mut cursor).is_err());
    }
}
