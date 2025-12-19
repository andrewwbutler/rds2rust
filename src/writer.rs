//! Writer for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::types::{Attributes, Complex, FactorData, Logical, PairlistElement, RObject};
use byteorder::{BigEndian, WriteBytesExt};
use flate2::write::GzEncoder;
use flate2::Compression;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::io::Write;
use std::mem;
use std::sync::Arc;

// Note: Thread-local state (WRITE_STACK, WRITE_DEPTH, WRITE_CALLS) removed for thread safety.
// These were only used for debugging and cycle detection.
// Cycle detection is now handled by ref_table which already tracks object pointers.

/// Reference table for tracking objects during serialization.
/// R's serialization uses reference tracking to avoid duplicating shared objects.
/// When the same object appears multiple times, the first occurrence is written normally,
/// and subsequent occurrences are written as REFSXP with an index pointing to the first.
struct RefTable {
    /// Next reference index to assign for objects
    next_index: u32,
    /// Next symbol index to assign (separate from object index space)
    next_symbol_index: u32,
    /// Map from namespace name to reference index
    namespace_refs: HashMap<String, u32>,
    /// Map from symbol name to symbol index
    symbol_refs: HashMap<String, u32>,
    /// Map from object identity (pointer) to reference index for Shared handling
    object_refs: HashMap<usize, u32>,
}

impl RefTable {
    fn new() -> Self {
        RefTable {
            next_index: 1,        // R uses 1-based indexing for references
            next_symbol_index: 1, // Symbols have separate 1-based indexing
            namespace_refs: HashMap::new(),
            symbol_refs: HashMap::new(),
            object_refs: HashMap::new(),
        }
    }

    /// Check if a namespace has been written before, returning its reference index if so.
    /// Otherwise, return None (caller decides whether/how to allocate).
    fn check_namespace(&mut self, names: &[Arc<str>]) -> Option<u32> {
        let key = names
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        self.namespace_refs.get(&key).copied()
    }

    /// Check if a symbol has been written before, returning its symbol index if so.
    /// Otherwise, register it and return None.
    /// Note: Symbols use a separate index space from objects.
    fn check_symbol(&mut self, name: &str) -> Option<u32> {
        if let Some(&symbol_idx) = self.symbol_refs.get(name) {
            Some(symbol_idx)
        } else {
            let idx = self.next_symbol_index;
            self.next_symbol_index += 1;
            self.symbol_refs.insert(name.to_string(), idx);
            None
        }
    }

    /// Check if an object pointer has been written before.
    fn check_object_ptr(&self, ptr: usize) -> Option<u32> {
        self.object_refs.get(&ptr).cloned()
    }

    /// Register an object pointer and return its reference index.
    fn register_object_ptr(&mut self, ptr: usize) -> u32 {
        if let Some(&idx) = self.object_refs.get(&ptr) {
            return idx;
        }
        let idx = self.next_index;
        self.next_index += 1;
        self.object_refs.insert(ptr, idx);
        idx
    }
}

/// Shared object context for tracking identity across unwrapping.
#[derive(Copy, Clone, Debug)]
struct SharedInfo {
    arc_ptr: usize, // Arc::as_ptr() value for identity tracking
}

/// Context for symbol writing - determines which index space to use for REFSXP
#[derive(Copy, Clone, Debug)]
enum SymbolContext {
    Tag,                 // TAG position (formals, attributes) - use symbol REFSXP
    NonTag,              // General non-TAG position - use object REFSXP
    NonTagPreferSymbol,  // Language/closure symbol positions - use symbol REFSXP
}

/// Coordinates symbol tracking across both index spaces (object and symbol).
/// This enables proper REFSXP emission for symbols that appear in both TAG and non-TAG positions.
struct SymbolTracker {
    /// Map from Shared(Symbol) Arc pointer → (object_idx, symbol_idx)
    /// Preferred path when Shared wrapper is available
    pointer_symbols: HashMap<usize, (u32, u32)>,

    /// Map from symbol value (string) → (object_idx, symbol_idx)
    /// Fallback for plain Symbol (non-Shared) to enable REFSXP reuse
    value_symbols: HashMap<Arc<str>, (u32, u32)>,
}

impl SymbolTracker {
    fn new() -> Self {
        Self {
            pointer_symbols: HashMap::new(),
            value_symbols: HashMap::new(),
        }
    }

    /// Check if a symbol was already written (pointer-based first, then value-based)
    fn lookup(&self, ptr_opt: Option<usize>, name: &str) -> Option<(u32, u32)> {
        // Try pointer-based lookup first (more precise)
        if let Some(ptr) = ptr_opt {
            if let Some(indices) = self.pointer_symbols.get(&ptr) {
                return Some(*indices);
            }
        }
        // Fall back to value-based lookup for plain Symbols
        self.value_symbols.get(name).copied()
    }

    /// Register a newly written symbol
    ///
    /// Policy: Value map is ALWAYS populated, even when pointer is present.
    /// This enables REFSXP reuse across:
    /// - Plain Symbol instances (no Shared wrapper) via value lookup
    /// - Shared(Symbol) instances via pointer lookup (preferred) or value fallback
    ///
    /// Rationale for dual registration:
    /// R symbols are semantically interned by name - Symbol("x") from one Arc instance
    /// is identical to Symbol("x") from another. Value-based tracking ensures correctness
    /// even when Arc pointer identity is unavailable (plain Symbol in Language.function,
    /// TAG without Shared wrapper). This is INTENTIONAL, not a bug.
    ///
    /// Pointer tracking is preferred when available for precision, but value fallback
    /// ensures we never miss REFSXP opportunities.
    fn register(&mut self, ptr_opt: Option<usize>, name: Arc<str>, indices: (u32, u32)) {
        if let Some(ptr) = ptr_opt {
            // Shared(Symbol) - register by pointer (preferred path)
            self.pointer_symbols.insert(ptr, indices);

            // Debug assertion: pointer and value mappings must agree
            // If value map already has this name, indices should match
            debug_assert!(
                self.value_symbols
                    .get(&name)
                    .map_or(true, |v| v == &indices),
                "Pointer and value mappings disagree for symbol '{}': ptr={:?}, value={:?}",
                name,
                Some(indices),
                self.value_symbols.get(&name)
            );
        }

        // Always register by value to enable REFSXP reuse for plain Symbols
        // (Language function, TAG without Shared wrapper, etc.)
        self.value_symbols.insert(name, indices);
    }
}

/// Atomically register a symbol in both index spaces
///
/// This is the ONLY function that should allocate indices for symbols.
/// Returns: (object_idx, symbol_idx) for the written symbol
///
/// CRITICAL: Never manipulate next_index or table entries directly elsewhere!
/// - Pointer path: MUST use `register_object_ptr()` (no direct next_index manipulation)
/// - Value-only path: This function is the SOLE exception for direct next_index access
///
/// Policy: Always allocates BOTH indices when writing SYMSXP, even for TAG-only symbols.
/// Rationale: Parser assigns both indices to SYMSXP (ref_table + symbol_table), so writer
/// must mirror this. If we deferred object_idx for TAG-only symbols, we'd create index
/// misalignment when those symbols later appear in non-TAG positions.
fn register_symbol_atomic(
    name: Arc<str>,
    ptr_opt: Option<usize>,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> (u32, u32) {
    // Allocate symbol index via RefTable's check_symbol method
    // API behavior: registers if not found, returns None for first write, Some(idx) for reuse
    let symbol_idx = match ref_table.check_symbol(name.as_ref()) {
        Some(existing_idx) => existing_idx,
        None => {
            // Just registered - retrieve the index that was assigned
            // check_symbol has side effect of inserting into symbol_refs
            ref_table
                .symbol_refs
                .get(name.as_ref())
                .copied()
                .expect("check_symbol just registered this symbol")
        }
    };

    // Allocate object index
    // Two paths: pointer-based (Shared wrapper) vs value-only (plain Symbol)
    let object_idx = if let Some(ptr) = ptr_opt {
        // POINTER PATH: Use RefTable's existing registration method
        // This is the ONLY allowed way to allocate object indices for pointers
        ref_table.register_object_ptr(ptr)
    } else {
        // VALUE-ONLY PATH: Allocate object index without pointer
        // This is the ONLY place outside register_object_ptr that may touch next_index
        // Enables REFSXP reuse by value for plain Symbol (Language function, TAG without Shared)
        //
        // IMPORTANT: We allocate an index but don't insert into object_refs (no pointer).
        // SymbolTracker will track this mapping. When REFSXP(object_idx) is emitted,
        // parser will assign this index to the symbol in ref_table.
        let idx = ref_table.next_index;
        ref_table.next_index += 1;
        idx
    };

    // Record in SymbolTracker for future lookups
    // This is the canonical mapping for REFSXP emission
    symbol_tracker.register(ptr_opt, name.clone(), (object_idx, symbol_idx));

    // Debug assertion: both indices must be valid
    debug_assert!(symbol_idx < ref_table.next_symbol_index);
    debug_assert!(object_idx < ref_table.next_index);

    (object_idx, symbol_idx)
}

/// Write a symbol with full tracking support
///
/// Returns: (object_idx, symbol_idx) for the written symbol
fn write_symbol_with_tracking(
    writer: &mut Vec<u8>,
    name: Arc<str>,
    shared_ptr: Option<usize>, // Arc pointer if from Shared(Symbol)
    context: SymbolContext,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<(u32, u32)> {
    // Check if this symbol was already written
    if let Some((obj_idx, sym_idx)) = symbol_tracker.lookup(shared_ptr, name.as_ref()) {
        // Already written - emit appropriate REFSXP based on context
        // TAG positions use the symbol table index; non-TAG uses the object ref index.
        let refsxp_idx = match context {
            SymbolContext::Tag => sym_idx,
            SymbolContext::NonTag => obj_idx,
            SymbolContext::NonTagPreferSymbol => sym_idx,
        };

        // Debug logging
        if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
            eprintln!(
                "[SYMBOL] REFSXP '{}' ptr={:?} obj={} sym={} ctx={:?} emit_idx={}",
                name,
                shared_ptr.map(|p| format!("{:#x}", p)),
                obj_idx,
                sym_idx,
                context,
                refsxp_idx
            );
        }

        // Debug assertion: enforce context rules
        debug_assert!(
            (matches!(context, SymbolContext::Tag) && refsxp_idx == sym_idx)
                || (matches!(context, SymbolContext::NonTag) && refsxp_idx == obj_idx)
                || (matches!(context, SymbolContext::NonTagPreferSymbol) && refsxp_idx == sym_idx),
            "REFSXP index mismatch for context {:?}: emitting {}, expected obj_idx {} sym_idx {}",
            context,
            refsxp_idx,
            obj_idx,
            sym_idx
        );

        write_refsxp(writer, refsxp_idx)?;
        return Ok((obj_idx, sym_idx));
    }

    // First time writing this symbol - write SYMSXP
    write_flags(writer, SYMSXP, false, false, false)?;
    write_charsxp(writer, name.as_ref())?;

    // Atomically register in both index spaces - NO manual next_index manipulation!
    let indices = register_symbol_atomic(name.clone(), shared_ptr, ref_table, symbol_tracker);

    // Debug logging
    if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
        eprintln!(
            "[SYMBOL] SYMSXP '{}' ptr={:?} obj={} sym={} ctx={:?}",
            name,
            shared_ptr.map(|p| format!("{:#x}", p)),
            indices.0,
            indices.1,
            context
        );
    }

    // Debug assertion: verify registration
    debug_assert!(
        symbol_tracker.lookup(shared_ptr, name.as_ref()).is_some(),
        "Symbol '{}' not properly registered after writing SYMSXP",
        name
    );

    Ok(indices)
}

/// Extract symbol name and Shared pointer (if any) from an object.
fn symbol_name_and_ptr(obj: &RObject) -> Option<(Arc<str>, Option<usize>)> {
    match obj {
        RObject::Symbol(name) => Some((name.clone(), None)),
        RObject::Shared(arc) => {
            let inner = arc.read().unwrap();
            match &*inner {
                RObject::Symbol(name) => Some((name.clone(), Some(Arc::as_ptr(arc) as usize))),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Determine if an object type should be tracked for references.
/// This matches the same logic used in the parser.
#[allow(dead_code)]
fn should_track_reference_type(obj: &RObject) -> bool {
    match obj {
        // Singletons and non-heap values are not reference-tracked.
        RObject::Null
        | RObject::Symbol(_)
        | RObject::Special { .. }
        | RObject::Builtin { .. } => false,

        // Atomic vectors should be tracked to keep REFSXP indices aligned with the parser.
        RObject::Integer(_)
        | RObject::Real(_)
        | RObject::Logical(_)
        | RObject::Character(_)
        | RObject::Raw(_)
        | RObject::Complex(_) => true,

        // WithAttributes must be tracked to maintain index alignment with parser.
        // However, DO NOT deduplicate by pointer - see ref_key() below.
        RObject::WithAttributes { .. } => true,

        // Shared is ALWAYS tracked by its Arc pointer, regardless of inner content.
        // Don't unwrap it here to avoid infinite recursion on cycles!
        RObject::Shared(_) => true,

        // Structured objects participate in ref graphs.
        RObject::List(_)
        | RObject::Expression(_)
        | RObject::Language { .. }
        | RObject::Pairlist(_)
        | RObject::Closure { .. }
        | RObject::Environment { .. }
        | RObject::Promise { .. }
        | RObject::Bytecode { .. }
        | RObject::DataFrame(_)
        | RObject::Factor(_)
        | RObject::S3Object(_)
        | RObject::S4Object(_)
        | RObject::Namespace(_)
        | RObject::MissingArg
        | RObject::UnboundValue => true,

        // Environment markers and singleton namespaces are not tracked.
        RObject::GlobalEnv | RObject::BaseEnv | RObject::EmptyEnv => false,
    }
}

/// Compute a stable key for reference-tracking an object.
fn ref_key(obj: &RObject) -> Option<usize> {
    match obj {
        RObject::Shared(inner) => Some(Arc::as_ptr(inner) as usize),
        // S4 objects are often constructed as temporaries; avoid pointer-based dedup.
        RObject::S4Object(_) => None,
        // Atomic vectors should increment ref indices, but should NOT use pointer-based
        // deduplication since temporary vectors can reuse stack addresses.
        RObject::Integer(_)
        | RObject::Real(_)
        | RObject::Logical(_)
        | RObject::Character(_)
        | RObject::Raw(_)
        | RObject::Complex(_) => None,
        // WithAttributes should NOT use pointer-based deduplication because
        // stack-allocated instances can have the same address (see write_s4_object).
        // Return None to prevent deduplication while still allowing index tracking.
        RObject::WithAttributes { .. } => None,
        // List should NOT use pointer-based deduplication because
        // stack-allocated Vec instances (like dimnames lists) can have the same address.
        // This causes corruption when different matrices reuse the same stack location
        // for their dimnames List objects.
        RObject::List(_) => None,
        other if should_track_reference_type(other) => Some(other as *const RObject as usize),
        _ => None,
    }
}

/// Write an RObject to RDS format.
/// Returns the serialized bytes (gzip compressed).
pub fn write_rds(obj: &RObject) -> Result<Vec<u8>> {
    // Check if object is fully loaded (no lazy vectors)
    if !obj.is_fully_loaded() {
        return Err(Error::CannotWriteLazyObject);
    }

    let mut buffer = Vec::new();

    // Write header
    write_header(&mut buffer)?;

    // Create reference table for tracking shared objects
    let mut ref_table = RefTable::new();

    // Create symbol tracker for dual-index symbol tracking
    let mut symbol_tracker = SymbolTracker::new();

    // Write the object
    write_object(&mut buffer, obj, &mut ref_table, &mut symbol_tracker)?;

    // Compress with gzip
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&buffer)?;
    let compressed = encoder.finish()?;

    Ok(compressed)
}

/// Write the RDS file header.
fn write_header(writer: &mut Vec<u8>) -> Result<()> {
    // Magic bytes: 'X\n' for XDR format (big-endian)
    writer.write_all(b"X\n")?;

    // Format version: 3 (current version used by R 4.x)
    writer.write_u32::<BigEndian>(3)?;

    // R version that wrote the file: 4.3.3 (0x00040303)
    writer.write_u32::<BigEndian>(0x00040303)?;

    // Minimum R version needed: 3.5.0 (0x00030500)
    writer.write_u32::<BigEndian>(0x00030500)?;

    // Version 3 requires native encoding information
    let encoding = b"UTF-8";
    writer.write_u32::<BigEndian>(encoding.len() as u32)?;
    writer.write_all(encoding)?;

    Ok(())
}

/// Write an R object to the stream.
fn write_object(
    writer: &mut Vec<u8>,
    obj: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_object_inner(writer, obj, ref_table, symbol_tracker, true, None)
}

/// Internal helper with toggle to skip ref tracking (used when descending into Shared inner).
fn write_object_inner(
    writer: &mut Vec<u8>,
    obj: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
    allow_ref_tracking: bool,
    shared_info: Option<SharedInfo>,
) -> Result<()> {
    // Note: Call tracking and depth guards removed for thread safety

    // Track referenceable objects (including Shared) to avoid infinite recursion and emit REFSXP.
    let mut current_ref_idx: Option<u32> = None;
    if allow_ref_tracking {
        if let Some(ptr) = ref_key(obj) {
            // Detect recursion cycles in the current write call stack.
            let in_stack = false; // Cycle detection via ref_table
            if in_stack {
                // Object is currently being written (self-reference/cycle).
                // We CANNOT emit REFSXP because the object isn't fully defined yet,
                // and the parser would stack overflow trying to resolve it.
                // Instead, write NULL to break the cycle.
                if std::env::var("RDS_DEBUG").is_ok() {
                    eprintln!(
                        "[WRITE] Cycle detected for key={:p}, writing NULL to break cycle",
                        ptr as *const ()
                    );
                }
                if std::env::var("RDS_DEBUG_CLOSURE").is_ok() {
                    eprintln!(
                        "[WRITE_DBG] cycle hit type={:?} key={:p}, writing NULL",
                        mem::discriminant(obj),
                        ptr as *const ()
                    );
                }
                return write_null(writer);
            }

            // Already written? emit REFSXP.
            if let Some(idx) = ref_table.check_object_ptr(ptr) {
                if std::env::var("RDS_DEBUG").is_ok() {
                    eprintln!("[WRITE] REFSXP emit idx={} key={:p}", idx, ptr as *const ());
                }
                if std::env::var("RDS_DEBUG_CLOSURE").is_ok() {
                    eprintln!(
                        "[WRITE_DBG] reuse type={:?} key={:p} discr={:?}",
                        mem::discriminant(obj),
                        ptr as *const (),
                        obj.variant_name()
                    );
                }
                if std::env::var("RDS_DEBUG_WITHATTR").is_ok()
                    && matches!(obj, RObject::WithAttributes { .. })
                {
                    eprintln!(
                        "[WITHATTR] REFSXP idx={} ptr={:p} type={}",
                        idx,
                        ptr as *const (),
                        obj.variant_name()
                    );
                }
                write_refsxp(writer, idx)?;
                return Ok(());
            }

            // First time: reserve index and descend.
            let idx = ref_table.register_object_ptr(ptr);
            current_ref_idx = Some(idx);
            if std::env::var("RDS_DEBUG").is_ok() {
                eprintln!(
                    "[WRITE] DEFINE idx={} key={:p} type={:?}",
                    idx,
                    ptr as *const (),
                    mem::discriminant(obj)
                );
            }
            if std::env::var("RDS_DEBUG_CLOSURE").is_ok() {
                eprintln!(
                    "[WRITE_DBG] define idx={} key={:p} discr={:?}",
                    idx,
                    ptr as *const (),
                    obj.variant_name()
                );
            }
            if std::env::var("RDS_DEBUG_WITHATTR").is_ok()
                && matches!(obj, RObject::WithAttributes { .. })
            {
                eprintln!(
                    "[WITHATTR] DEFINE idx={} ptr={:p} type={}",
                    idx,
                    ptr as *const (),
                    obj.variant_name()
                );
            }

            // Stack guard for cycle detection removed (ref_table handles this)

            // For Shared, we've registered the Shared wrapper's Arc pointer.
            // Now write the inner object WITHOUT giving it its own index (skip ref tracking for immediate inner),
            // but WITH ref tracking for nested contents.
            // This avoids double-tracking the immediate inner object while still allowing
            // nested objects to be tracked properly.
            if let RObject::Shared(inner) = obj {
                // Extract Arc pointer for identity tracking
                let shared_info = SharedInfo {
                    arc_ptr: Arc::as_ptr(inner) as usize,
                };

                if std::env::var("RDS_DEBUG").is_ok() {
                    eprintln!("[WRITE] Unwrapping Shared at idx={}, arc_ptr={:p}, skipping inner's own tracking", idx, Arc::as_ptr(inner));
                }
                let guard = inner.read().unwrap();

                if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                    eprintln!(
                        "[SHARED] Unwrapping Shared at idx={} inner_type={:?}",
                        idx,
                        std::mem::discriminant(&*guard)
                    );
                    if let RObject::Symbol(name) = &*guard {
                        eprintln!("[SHARED] Inner is Symbol('{}')", name);
                    }
                }

                // Write the inner object WITHOUT ref tracking for the inner object itself,
                // but WITH SharedInfo propagated for symbol tracking.
                // CRITICAL: Symbol tracking is orthogonal to allow_ref_tracking.
                // Even when allow_ref_tracking=false (for the Shared wrapper),
                // symbol registration MUST still occur for the inner Symbol.
                // The SharedInfo propagation ensures this.
                return write_object_inner(
                    writer,
                    &*guard,
                    ref_table,
                    symbol_tracker,
                    false,
                    Some(shared_info),
                );
            }

            // fallthrough to write body with tracking already registered
        } else if should_track_reference_type(obj) {
            // Track the object to keep ref indices aligned with the parser,
            // but without pointer-based deduplication.
            let idx = ref_table.next_index;
            ref_table.next_index += 1;
            current_ref_idx = Some(idx);
            if std::env::var("RDS_DEBUG").is_ok() {
                eprintln!(
                    "[WRITE] DEFINE (anon) idx={} type={}",
                    idx,
                    obj.variant_name()
                );
            }
        }
    }

    // If tracking is disabled (because a Shared wrapper already assigned a ref) and we see another
    // Shared, we need to handle it with tracking enabled so it gets properly registered.
    if !allow_ref_tracking {
        if let RObject::Shared(_) = obj {
            // Re-enable tracking for nested Shared objects
            return write_object_inner(writer, obj, ref_table, symbol_tracker, true, None);
        }
    }

    match obj {
        RObject::Null => write_null(writer),
        RObject::Integer(vec) => write_integer_vector(writer, vec),
        RObject::Real(vec) => write_real_vector(writer, vec),
        RObject::Logical(vec) => write_logical_vector(writer, vec),
        RObject::Character(vec) => {
            // Character vectors are always written as STRSXP
            // Symbols (SYMSXP) are only written in specific contexts like pairlist tags
            // or Language function positions, not here
            write_character_vector(writer, vec)
        }
        RObject::Symbol(name) => {
            // Write as SYMSXP - used for R's symbol table and special markers
            // Phase 5: Use write_symbol_with_tracking for non-TAG symbols
            let ptr_opt = shared_info.map(|si| si.arc_ptr);

            if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                eprintln!(
                    "[SYMBOL ARM] Reached Symbol arm for '{}' ptr={:?}",
                    name,
                    ptr_opt.map(|p| format!("{:#x}", p))
                );
            }

            write_symbol_with_tracking(
                writer,
                name.clone(),
                ptr_opt,
                SymbolContext::NonTag,
                ref_table,
                symbol_tracker,
            )?;
            Ok(())
        }
        RObject::Raw(vec) => write_raw_vector(writer, vec),
        RObject::Complex(vec) => write_complex_vector(writer, vec),
        RObject::List(elements) => write_list(writer, elements, ref_table, symbol_tracker),
        RObject::Expression(elements) => {
            write_expression(writer, elements, ref_table, symbol_tracker)
        }
        RObject::Pairlist(elements) => write_pairlist(writer, elements, ref_table, symbol_tracker),
        RObject::Language { function, args } => {
            write_language(writer, function, args, ref_table, symbol_tracker)
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => write_closure(
            writer,
            formals,
            body,
            environment,
            ref_table,
            symbol_tracker,
        ),
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => write_environment(writer, enclosing, frame, hashtab, ref_table, symbol_tracker),
        RObject::Promise {
            value,
            expression,
            environment,
        } => write_promise(
            writer,
            value,
            expression,
            environment,
            ref_table,
            symbol_tracker,
        ),
        RObject::Special { name } => write_special(writer, name.as_ref()),
        RObject::Builtin { name } => write_builtin(writer, name.as_ref()),
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => write_bytecode(writer, code, constants, expr, ref_table, symbol_tracker),
        RObject::DataFrame(data) => write_dataframe(
            writer,
            &data.columns,
            &data.row_names,
            ref_table,
            symbol_tracker,
        ),
        RObject::Factor(data) => write_factor(writer, data, ref_table, symbol_tracker),
        RObject::S3Object(data) => write_s3_object(
            writer,
            &data.base,
            &data.class,
            &data.attributes,
            ref_table,
            symbol_tracker,
        ),
        RObject::S4Object(data) => write_s4_object(
            writer,
            &data.class,
            data.package.as_ref(),
            &data.slots,
            ref_table,
            symbol_tracker,
        ),
        RObject::Namespace(names) => write_namespace(writer, names, ref_table, current_ref_idx),
        RObject::GlobalEnv => write_global_env(writer),
        RObject::BaseEnv => write_base_env(writer),
        RObject::EmptyEnv => write_empty_env(writer),
        RObject::MissingArg => write_missing_arg(writer),
        RObject::UnboundValue => write_unbound_value(writer),
        RObject::WithAttributes { object, attributes } => {
            write_object_with_attributes(writer, object, attributes, ref_table, symbol_tracker)
        }
        RObject::Shared(_) => {
            // Shared should have been handled in the ref tracking block above.
            // If we reach here, something went wrong.
            Err(Error::InvalidFormat(
                "Unexpected Shared object in match - should have been handled earlier".to_string(),
            ))
        }
    }
}

/// Write NULL.
fn write_null(writer: &mut Vec<u8>) -> Result<()> {
    // Use NILVALUE_SXP (254) for singleton NULL
    write_flags(writer, NILVALUE_SXP, false, false, false)?;
    Ok(())
}

/// Write global environment reference (GLOBALENV_SXP).
fn write_global_env(writer: &mut Vec<u8>) -> Result<()> {
    write_flags(writer, GLOBALENV_SXP, false, false, false)?;
    Ok(())
}

/// Write base environment reference (BASEENV_SXP).
fn write_base_env(writer: &mut Vec<u8>) -> Result<()> {
    write_flags(writer, BASEENV_SXP, false, false, false)?;
    Ok(())
}

/// Write empty environment reference (EMPTYENV_SXP).
fn write_empty_env(writer: &mut Vec<u8>) -> Result<()> {
    write_flags(writer, EMPTYENV_SXP, false, false, false)?;
    Ok(())
}

/// Write missing argument marker (MISSINGARG_SXP).
fn write_missing_arg(writer: &mut Vec<u8>) -> Result<()> {
    write_flags(writer, MISSINGARG_SXP, false, false, false)?;
    Ok(())
}

/// Write unbound value marker (UNBOUNDVALUE_SXP).
fn write_unbound_value(writer: &mut Vec<u8>) -> Result<()> {
    write_flags(writer, UNBOUNDVALUE_SXP, false, false, false)?;
    Ok(())
}

/// Write a namespace reference (NAMESPACESXP).
/// This triggers automatic package loading when the RDS file is read in R.
fn write_namespace(
    writer: &mut Vec<u8>,
    names: &[Arc<str>],
    ref_table: &mut RefTable,
    reserved_idx: Option<u32>,
) -> Result<()> {
    // Check if this namespace was already written
    if let Some(ref_idx) = ref_table.check_namespace(names) {
        // Write a reference to the previous occurrence
        write_refsxp(writer, ref_idx)?;
        return Ok(());
    }

    // First occurrence - write the full namespace
    // Use NAMESPACESXP_SERIAL (249) not NAMESPACESXP (123) for serialization
    // Align the reference index with any reserved object slot (from ref tracking).
    let _idx = if let Some(idx) = reserved_idx {
        // Ensure the namespace_refs map records the existing index without bumping next_index again.
        ref_table.namespace_refs.insert(
            names
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join("::"),
            idx,
        );
        idx
    } else {
        // No reserved index; allocate a new one.
        let idx = ref_table.next_index;
        ref_table.next_index += 1;
        ref_table.namespace_refs.insert(
            names
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join("::"),
            idx,
        );
        idx
    };

    write_flags(writer, NAMESPACESXP_SERIAL, false, false, false)?;

    // Write as OutStringVec format: flags, length, then CHARSXP entries
    writer.write_u32::<BigEndian>(0)?; // unused flags
    writer.write_u32::<BigEndian>(names.len() as u32)?;

    for name in names {
        write_charsxp(writer, name.as_ref())?;
    }

    Ok(())
}

/// Write a reference to a previously written object (REFSXP).
fn write_refsxp(writer: &mut Vec<u8>, ref_index: u32) -> Result<()> {
    // REFSXP encodes the reference index in the flags field
    // The format is: type=255 (REFSXP), with the index in bits 8-31
    let flags = REFSXP | (ref_index << 8);
    writer.write_u32::<BigEndian>(flags)?;
    Ok(())
}

/// Write flags (type + attribute/tag bits).
fn write_flags(
    writer: &mut Vec<u8>,
    sexp_type: u32,
    has_attr: bool,
    has_tag: bool,
    is_s4: bool,
) -> Result<()> {
    write_flags_with_object(writer, sexp_type, has_attr, has_tag, is_s4, false)
}

fn write_flags_with_object(
    writer: &mut Vec<u8>,
    sexp_type: u32,
    has_attr: bool,
    has_tag: bool,
    is_s4: bool,
    is_object: bool,
) -> Result<()> {
    let mut flags = sexp_type;
    if has_attr {
        flags |= HAS_ATTR_BIT;
    }
    if has_tag {
        flags |= HAS_TAG_BIT;
    }
    if is_s4 {
        // S4 objects need both IS_OBJECT_BIT (bit 8) and S4_LEVELS (bit 4 in gp field)
        flags |= IS_OBJECT_BIT;
        flags |= S4_LEVELS;
    } else if is_object {
        // S3 objects (like data.frame) need IS_OBJECT_BIT but not S4_LEVELS
        flags |= IS_OBJECT_BIT;
    }
    writer.write_u32::<BigEndian>(flags)?;
    Ok(())
}

/// Write an integer vector.
fn write_integer_vector(writer: &mut Vec<u8>, vec: &[i32]) -> Result<()> {
    write_flags(writer, INTSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for &val in vec {
        writer.write_i32::<BigEndian>(val)?;
    }
    Ok(())
}

/// Write a real (double) vector.
fn write_real_vector(writer: &mut Vec<u8>, vec: &[f64]) -> Result<()> {
    write_flags(writer, REALSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for &val in vec {
        writer.write_f64::<BigEndian>(val)?;
    }
    Ok(())
}

/// Write a logical vector.
fn write_logical_vector(writer: &mut Vec<u8>, vec: &[Logical]) -> Result<()> {
    write_flags(writer, LGLSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for logical in vec {
        let val = match logical {
            Logical::False => 0i32,
            Logical::True => 1i32,
            Logical::Na => RObject::NA_INTEGER,
        };
        writer.write_i32::<BigEndian>(val)?;
    }
    Ok(())
}

/// Write a character vector.
fn write_character_vector(writer: &mut Vec<u8>, vec: &[Arc<str>]) -> Result<()> {
    write_flags(writer, STRSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for s in vec {
        write_charsxp(writer, s.as_ref())?;
    }
    Ok(())
}

/// Write a CHARSXP (internal string).
fn write_charsxp(writer: &mut Vec<u8>, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    // Check if the string is ASCII
    let is_ascii = bytes.iter().all(|&b| b < 128);
    // Build flags with ASCII encoding bit if applicable
    let mut flags = CHARSXP;
    if is_ascii {
        flags |= ASCII_LEVELS;
    }
    writer.write_u32::<BigEndian>(flags)?;
    writer.write_u32::<BigEndian>(bytes.len() as u32)?;
    writer.write_all(bytes)?;
    Ok(())
}

/// Write a raw vector.
fn write_raw_vector(writer: &mut Vec<u8>, vec: &[u8]) -> Result<()> {
    write_flags(writer, RAWSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    writer.write_all(vec)?;
    Ok(())
}

/// Write a complex vector.
fn write_complex_vector(writer: &mut Vec<u8>, vec: &[Complex]) -> Result<()> {
    write_flags(writer, CPLXSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for complex in vec {
        writer.write_f64::<BigEndian>(complex.real)?;
        writer.write_f64::<BigEndian>(complex.imaginary)?;
    }
    Ok(())
}

/// Write a list (VECSXP).
fn write_list(
    writer: &mut Vec<u8>,
    elements: &[RObject],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_flags(writer, VECSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table, symbol_tracker)?;
    }
    Ok(())
}

/// Write an expression vector (EXPRSXP).
/// Expression vectors are structurally identical to VECSXP, but semantically represent
/// collections of unevaluated expressions (typically language objects).
fn write_expression(
    writer: &mut Vec<u8>,
    elements: &[RObject],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_flags(writer, EXPRSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table, symbol_tracker)?;
    }
    Ok(())
}

/// Write a language object (LANGSXP).
/// Language objects represent unevaluated calls: function + arguments.
fn write_language(
    writer: &mut Vec<u8>,
    function: &RObject,
    args: &[PairlistElement],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // Language objects are structured as: CAR (function) + CDR (argument list)
    let has_tag = false; // Language objects typically don't have tags

    if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
        eprintln!(
            "[LANGUAGE] function type: {:?}",
            std::mem::discriminant(function)
        );
    }

    write_flags(writer, LANGSXP, false, has_tag, false)?;

    // Write the function (CAR)
    // Prefer symbols when possible (closure bodies expect symbol-table REFSXP)
    if let Some((name, ptr)) = symbol_name_and_ptr(function) {
        write_symbol_with_tracking(
            writer,
            name,
            ptr,
            SymbolContext::NonTagPreferSymbol,
            ref_table,
            symbol_tracker,
        )?;
    } else {
        // If it's a single-element Character, write it as a symbol (function name)
        match function {
            RObject::Character(vec) if vec.len() == 1 => {
            if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                eprintln!("[LANGUAGE] Writing function as symbol: '{}'", vec[0]);
            }
            write_symbol_with_tracking(
                writer,
                Arc::from(vec[0].as_ref()),
                None, // Plain string, not Shared
                SymbolContext::NonTagPreferSymbol,
                ref_table,
                symbol_tracker,
            )?;
            }
            _ => {
            if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                eprintln!(
                    "[LANGUAGE] Writing function via write_object: {:?}",
                    std::mem::discriminant(function)
                );
            }
            // For Shared(Symbol), SharedInfo will be propagated via write_object_inner
            write_object(writer, function, ref_table, symbol_tracker)?;
            }
        }
    }

    // Write the arguments (CDR) as a pairlist or NULL
    if !args.is_empty() {
        write_pairlist_as_args(writer, args, ref_table, symbol_tracker)?;
    } else {
        // No arguments
        write_null(writer)?;
    }

    Ok(())
}

/// Write a pairlist (LISTSXP).
/// When `values_are_symbols` is true, single-element Character values are written as SYMSXP.
/// This is used for Language argument lists where values may be variable references.
fn write_pairlist_internal(
    writer: &mut Vec<u8>,
    elements: &[PairlistElement],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
    values_are_symbols: bool,
) -> Result<()> {
    let debug_pairlist = std::env::var("RDS_DEBUG_PAIRLIST").is_ok();

    for (i, element) in elements.iter().enumerate() {
        let has_tag = element.tag.is_some();
        let is_last = i == elements.len() - 1;

        write_flags(writer, LISTSXP, false, has_tag, false)?;

        // Write the tag if present
        if let Some(ref tag) = element.tag {
            if debug_pairlist {
                eprintln!(
                    "[PAIRLIST] elem[{}] TAG='{}' (before write: obj_idx={}, sym_idx={})",
                    i, tag, ref_table.next_index, ref_table.next_symbol_index
                );
            }

            // Extract SharedInfo from tag_object if it's Shared(Symbol)
            let shared_ptr = element.tag_object.as_ref().and_then(|obj| {
                if let RObject::Shared(arc) = obj.as_ref() {
                    let inner = arc.read().unwrap();
                    if matches!(&*inner, RObject::Symbol(_)) {
                        return Some(Arc::as_ptr(arc) as usize);
                    }
                }
                None
            });

            write_symbol_with_tracking(
                writer,
                tag.clone(),
                shared_ptr,
                SymbolContext::Tag,
                ref_table,
                symbol_tracker,
            )?;

            if debug_pairlist {
                eprintln!(
                    "[PAIRLIST] elem[{}] TAG='{}' (after write: obj_idx={}, sym_idx={})",
                    i, tag, ref_table.next_index, ref_table.next_symbol_index
                );
            }
        }

        // Write the value
        if debug_pairlist {
            eprintln!(
                "[PAIRLIST] elem[{}] VALUE type={} (before write: obj_idx={}, sym_idx={})",
                i,
                element.value.variant_name(),
                ref_table.next_index,
                ref_table.next_symbol_index
            );
        }

        // If values_are_symbols and value is single-element Character, write as symbol
        if values_are_symbols {
            if let Some((name, ptr)) = symbol_name_and_ptr(&element.value) {
                write_symbol_with_tracking(
                    writer,
                    name,
                    ptr,
                    SymbolContext::NonTagPreferSymbol,
                    ref_table,
                    symbol_tracker,
                )?;
            } else {
                match &element.value {
                    RObject::Character(vec) if vec.len() == 1 => {
                        write_symbol_with_tracking(
                            writer,
                            Arc::from(vec[0].as_ref()),
                            None, // Plain string, not Shared
                            SymbolContext::NonTagPreferSymbol,
                            ref_table,
                            symbol_tracker,
                        )?;
                    }
                    _ => {
                        write_object(writer, &element.value, ref_table, symbol_tracker)?;
                    }
                }
            }
        } else {
            write_object(writer, &element.value, ref_table, symbol_tracker)?;
        }

        if debug_pairlist {
            eprintln!(
                "[PAIRLIST] elem[{}] VALUE type={} (after write: obj_idx={}, sym_idx={})",
                i,
                element.value.variant_name(),
                ref_table.next_index,
                ref_table.next_symbol_index
            );
        }

        // Write the CDR (tail)
        if is_last {
            // Last element: tail is NULL
            write_null(writer)?;
        }
        // If not last, the next iteration will write the next node
    }

    // If empty pairlist, write NULL
    if elements.is_empty() {
        write_null(writer)?;
    }

    Ok(())
}

/// Write a pairlist (LISTSXP) for general use.
fn write_pairlist(
    writer: &mut Vec<u8>,
    elements: &[PairlistElement],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // For general pairlists (like formals), don't convert values to symbols
    write_pairlist_internal(writer, elements, ref_table, symbol_tracker, false)
}

/// Write a pairlist for Language arguments where single-element Characters are symbols.
fn write_pairlist_as_args(
    writer: &mut Vec<u8>,
    elements: &[PairlistElement],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // For Language arguments, convert single-element Character values to symbols
    // The arguments list is a LISTSXP and should be reference-tracked.
    if !elements.is_empty() {
        ref_table.next_index += 1;
    }
    write_pairlist_internal(writer, elements, ref_table, symbol_tracker, true)
}

// Deprecated: Old symbol writing function replaced by write_symbol_with_tracking
// Kept for reference but no longer used
#[allow(dead_code)]
fn write_symbol_with_ref(writer: &mut Vec<u8>, name: &str, ref_table: &mut RefTable) -> Result<()> {
    // Check if this symbol was already written
    if let Some(ref_idx) = ref_table.check_symbol(name) {
        // Write a reference to the previous occurrence
        write_refsxp(writer, ref_idx)?;
    } else {
        // Write the symbol for the first time
        write_flags(writer, SYMSXP, false, false, false)?;
        write_charsxp(writer, name)?;
    }
    Ok(())
}

/// Write a closure (CLOSXP).
fn write_closure(
    writer: &mut Vec<u8>,
    formals: &RObject,
    body: &RObject,
    environment: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // R sets HAS_TAG for CLOSXP in most cases (for srcref tracking)
    // We match R's behavior by always setting has_tag=true
    write_flags(writer, CLOSXP, false, true, false)?;

    // Write environment (closure environment)
    write_object(writer, environment, ref_table, symbol_tracker)?;

    // Write formals (parameter list)
    write_object(writer, formals, ref_table, symbol_tracker)?;

    // Write body (function body)
    write_object(writer, body, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write an environment (ENVSXP).
fn write_environment(
    writer: &mut Vec<u8>,
    enclosing: &RObject,
    frame: &RObject,
    hashtab: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_flags(writer, ENVSXP, false, false, false)?;

    // Write locked flag (0 = unlocked)
    write_integer_vector(writer, &[0])?;

    // Write enclosing environment
    write_object(writer, enclosing, ref_table, symbol_tracker)?;

    // Write frame (bindings pairlist)
    write_object(writer, frame, ref_table, symbol_tracker)?;

    // Write hashtab
    write_object(writer, hashtab, ref_table, symbol_tracker)?;

    // Write attributes (environments always serialize an attribute field)
    write_object(writer, &RObject::Null, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write a promise (PROMSXP).
fn write_promise(
    writer: &mut Vec<u8>,
    value: &RObject,
    expression: &RObject,
    environment: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_flags(writer, PROMSXP, false, false, false)?;

    // Write the three components: value, expression, environment
    write_object(writer, value, ref_table, symbol_tracker)?;
    write_object(writer, expression, ref_table, symbol_tracker)?;
    write_object(writer, environment, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write a special primitive function (SPECIALSXP).
/// Format: type flag, then length (i32), then name bytes (no SYMSXP wrapper)
fn write_special(writer: &mut Vec<u8>, name: &str) -> Result<()> {
    write_flags(writer, SPECIALSXP, false, false, false)?;
    // Write the string length
    let bytes = name.as_bytes();
    writer.write_u32::<BigEndian>(bytes.len() as u32)?;
    // Write the string bytes
    writer.write_all(bytes)?;
    Ok(())
}

/// Write a builtin primitive function (BUILTINSXP).
/// Format: type flag, then length (i32), then name bytes (no SYMSXP wrapper)
fn write_builtin(writer: &mut Vec<u8>, name: &str) -> Result<()> {
    write_flags(writer, BUILTINSXP, false, false, false)?;
    // Write the string length
    let bytes = name.as_bytes();
    writer.write_u32::<BigEndian>(bytes.len() as u32)?;
    // Write the string bytes
    writer.write_all(bytes)?;
    Ok(())
}

/// Write bytecode (compiled R function).
fn write_bytecode(
    writer: &mut Vec<u8>,
    code: &RObject,
    constants: &RObject,
    _expr: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_flags(writer, BCODESXP, false, false, false)?;
    // For now, we don't emit any bytecode-specific reference table entries.
    writer.write_u32::<BigEndian>(0)?;
    write_bytecode_body(writer, code, constants, ref_table, symbol_tracker)
}

fn write_bytecode_body(
    writer: &mut Vec<u8>,
    code: &RObject,
    constants: &RObject,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    write_object(writer, code, ref_table, symbol_tracker)?;

    let const_list = match constants {
        RObject::List(elements) => elements,
        _ => {
            return Err(Error::InvalidFormat(
                "Bytecode constants must be stored as a list".to_string(),
            ));
        }
    };

    writer.write_u32::<BigEndian>(const_list.len() as u32)?;
    for value in const_list {
        match value {
            RObject::Bytecode {
                code, constants, ..
            } => {
                writer.write_i32::<BigEndian>(BCODESXP as i32)?;
                write_bytecode_body(writer, code, constants, ref_table, symbol_tracker)?;
            }
            _ => {
                writer.write_i32::<BigEndian>(0)?;
                write_object(writer, value, ref_table, symbol_tracker)?;
            }
        }
    }

    Ok(())
}

/// Write a data frame.
fn write_dataframe(
    writer: &mut Vec<u8>,
    columns: &IndexMap<Arc<str>, RObject>,
    row_names: &[Arc<str>],
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // IndexMap preserves insertion order, so we can iterate directly
    let cols_vec: Vec<_> = columns.iter().collect();

    let column_names: Vec<Arc<str>> = cols_vec.iter().map(|(name, _)| (*name).clone()).collect();
    let column_values: Vec<&RObject> = cols_vec.iter().map(|(_, obj)| *obj).collect();

    // Write as a list with attributes
    write_flags(writer, VECSXP, true, false, false)?;
    writer.write_u32::<BigEndian>(column_values.len() as u32)?;

    // Write each column
    for col in &column_values {
        write_object(writer, col, ref_table, symbol_tracker)?;
    }

    // Write attributes (names, row.names, class)
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), RObject::Character(column_names.into()));
    attrs.insert(
        Arc::from("row.names"),
        RObject::Character(row_names.to_vec().into()),
    );
    attrs.insert(
        Arc::from("class"),
        RObject::Character(vec![Arc::from("data.frame")].into()),
    );

    write_attributes(writer, &attrs, ref_table, symbol_tracker)?;

    Ok(())
}

fn merge_factor_attributes(data: &FactorData, attributes: &Attributes) -> Attributes {
    let mut merged = data.base_attributes();
    for (key, value) in attributes.iter() {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

fn validate_factor_attributes(attributes: &Attributes, value_len: usize) -> Result<()> {
    if let Some(names_attr) = attributes.get("names") {
        match names_attr {
            RObject::Character(names) => {
                if names.len() != value_len {
                    return Err(Error::InvalidFormat(format!(
                        "names attribute length ({}) does not match factor values ({})",
                        names.len(),
                        value_len
                    )));
                }
            }
            _ => {
                return Err(Error::Unsupported(
                    "Factor names attribute must be a character vector".to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Write a factor.
fn write_factor(
    writer: &mut Vec<u8>,
    data: &FactorData,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    let empty = Attributes::new();
    write_factor_with_attributes(writer, data, &empty, ref_table, symbol_tracker)
}

fn write_factor_with_attributes(
    writer: &mut Vec<u8>,
    data: &FactorData,
    attributes: &Attributes,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    let merged_attrs = merge_factor_attributes(data, attributes);

    validate_factor_attributes(&merged_attrs, data.values.len())?;

    // Write the integer vector with attributes
    write_flags(writer, INTSXP, true, false, false)?;
    writer.write_u32::<BigEndian>(data.values.len() as u32)?;
    for &val in &data.values {
        writer.write_i32::<BigEndian>(val)?;
    }

    write_attributes(writer, &merged_attrs, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write an S3 object.
fn write_s3_object(
    writer: &mut Vec<u8>,
    base: &RObject,
    class: &[Arc<str>],
    attributes: &Attributes,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // Write the base object with attributes
    match base {
        RObject::List(elements) => {
            write_flags(writer, VECSXP, true, false, false)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element, ref_table, symbol_tracker)?;
            }
        }
        RObject::Integer(vec) => {
            write_flags(writer, INTSXP, true, false, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_i32::<BigEndian>(*val)?;
            }
        }
        RObject::Real(vec) => {
            write_flags(writer, REALSXP, true, false, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_f64::<BigEndian>(*val)?;
            }
        }
        RObject::Language { function, args } => {
            // Language objects with attributes (e.g., formulas)
            // Attributes come BEFORE CAR/CDR for language objects
            let has_tag = false;
            write_flags(writer, LANGSXP, true, has_tag, false)?;

            // Write attributes FIRST (before CAR/CDR)
            let mut attrs = attributes.clone();
            attrs.insert(
                Arc::from("class"),
                RObject::Character(class.to_vec().into()),
            );
            write_attributes(writer, &attrs, ref_table, symbol_tracker)?;

            // Write the function (CAR)
            // Need to unwrap Shared to check if inner is single-element Character
            // Extract SharedInfo if function is Shared
            match function.as_ref() {
                RObject::Shared(arc) => {
                    let guard = arc.read().unwrap();
                    let ptr = Arc::as_ptr(arc) as usize;
                    // Check inner type
                    let func_written = match &*guard {
                        RObject::Character(vec) if vec.len() == 1 => {
                            // Inner is single-element Character - write as symbol
                            if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                                eprintln!(
                                    "[S3/LANGUAGE] Shared(Character(['{}'])) -> writing as symbol",
                                    vec[0]
                                );
                            }
                            write_symbol_with_tracking(
                                writer,
                                vec[0].clone(),
                                Some(ptr), // Track by Shared pointer
                                SymbolContext::NonTagPreferSymbol,
                                ref_table,
                                symbol_tracker,
                            )?;
                            true
                        }
                        RObject::Symbol(name) => {
                            // Inner is Symbol - write as symbol
                            if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                                eprintln!(
                                    "[S3/LANGUAGE] Shared(Symbol('{}')) -> writing as symbol",
                                    name
                                );
                            }
                            write_symbol_with_tracking(
                                writer,
                                name.clone(),
                                Some(ptr), // Track by Shared pointer
                                SymbolContext::NonTagPreferSymbol,
                                ref_table,
                                symbol_tracker,
                            )?;
                            true
                        }
                        _ => {
                            // Other type - delegate to write_object (below)
                            false
                        }
                    };
                    drop(guard); // Release lock

                    if !func_written {
                        // Fallback: write via write_object
                        if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                            eprintln!(
                                "[S3/LANGUAGE] Writing Shared function via write_object: {:?}",
                                std::mem::discriminant(function.as_ref())
                            );
                        }
                        write_object(writer, function, ref_table, symbol_tracker)?;
                    }
                }
                RObject::Character(vec) if vec.len() == 1 => {
                    // Plain single-element Character - write as symbol
                    if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                        eprintln!(
                            "[S3/LANGUAGE] Character(['{}']) -> writing as symbol",
                            vec[0]
                        );
                    }
                    write_symbol_with_tracking(
                        writer,
                        vec[0].clone(),
                        None, // Plain, not Shared
                        SymbolContext::NonTagPreferSymbol,
                        ref_table,
                        symbol_tracker,
                    )?;
                }
                _ => {
                    // Fallback: write via write_object
                    if std::env::var("RDS_DEBUG_SYMBOL").is_ok() {
                        eprintln!(
                            "[S3/LANGUAGE] Writing function via write_object: {:?}",
                            std::mem::discriminant(function.as_ref())
                        );
                    }
                    write_object(writer, function, ref_table, symbol_tracker)?;
                }
            }

            // Write the arguments (CDR) as a pairlist or NULL
            if !args.is_empty() {
                write_pairlist_as_args(writer, args, ref_table, symbol_tracker)?;
            } else {
                write_null(writer)?;
            }
            return Ok(());
        }
        _ => {
            return Err(Error::Unsupported(
                "Unsupported S3 base type for writing".to_string(),
            ));
        }
    }

    // Write attributes with class added
    let mut attrs = attributes.clone();
    attrs.insert(
        Arc::from("class"),
        RObject::Character(class.to_vec().into()),
    );
    write_attributes(writer, &attrs, ref_table, symbol_tracker)?;

    Ok(())
}

/// Build the standard S4 attribute map (class with package + slots).
/// This is shared between write_s4_object and write_object_with_attributes.
fn build_s4_attributes(
    class: &[Arc<str>],
    package: Option<&Arc<str>>,
    slots: &IndexMap<Arc<str>, RObject>,
) -> Attributes {
    let mut attrs = Attributes::new();

    // For S4 objects, the class attribute must have a package attribute
    // Use the stored package if available, otherwise fall back to ".GlobalEnv" for user-defined classes
    let class_obj = RObject::Character(class.to_vec().into());
    let mut class_attrs = Attributes::new();
    let pkg_value = package.cloned().unwrap_or_else(|| Arc::from(".GlobalEnv"));
    class_attrs.insert(
        Arc::from("package"),
        RObject::Character(vec![pkg_value].into()),
    );

    let class_with_package = RObject::WithAttributes {
        object: Box::new(class_obj),
        attributes: class_attrs,
    };

    attrs.insert(Arc::from("class"), class_with_package);

    // Add all slots as attributes
    for (name, value) in slots {
        attrs.insert(name.clone(), value.clone());
    }

    attrs
}

/// Write an S4 object.
fn write_s4_object(
    writer: &mut Vec<u8>,
    class: &[Arc<str>],
    package: Option<&Arc<str>>,
    slots: &IndexMap<Arc<str>, RObject>,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // S4 objects are written as S4SXP with attributes and IS_S4_BIT set
    write_flags(writer, S4SXP, true, false, true)?;

    // Build S4 attributes (class + slots)
    let attrs = build_s4_attributes(class, package, slots);

    write_attributes(writer, &attrs, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write an object with attributes.
///
/// Supports writing attributes for the following object types:
/// - Factor (delegates to write_factor_with_attributes)
/// - Integer vectors
/// - Real (double) vectors
/// - Logical vectors
/// - Character vectors
/// - Lists (generic vectors)
/// - S4 objects (merges outer attributes with class and slots)
fn write_object_with_attributes(
    writer: &mut Vec<u8>,
    object: &RObject,
    attributes: &Attributes,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    // Check if this has a class attribute that makes it an S3 object
    let is_s3_object = attributes.attrs.iter().any(|(k, v)| {
        k.as_ref() == "class" && matches!(**v, RObject::Character(ref vec) if !vec.is_empty())
    });

    // Write the base object with HAS_ATTR flag set (and OBJ flag if S3 object)
    match object {
        RObject::Factor(data) => {
            write_factor_with_attributes(writer, data, attributes, ref_table, symbol_tracker)?;
            return Ok(());
        }
        RObject::Integer(vec) => {
            write_flags_with_object(writer, INTSXP, true, false, false, is_s3_object)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_i32::<BigEndian>(*val)?;
            }
        }
        RObject::Real(vec) => {
            write_flags_with_object(writer, REALSXP, true, false, false, is_s3_object)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_f64::<BigEndian>(*val)?;
            }
        }
        RObject::Character(vec) => {
            write_flags_with_object(writer, STRSXP, true, false, false, is_s3_object)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for s in vec {
                write_charsxp(writer, s)?;
            }
        }
        RObject::Logical(vec) => {
            write_flags_with_object(writer, LGLSXP, true, false, false, is_s3_object)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            // NA mapping follows R spec: Logical::Na -> NA_INTEGER (i32::MIN)
            for logical in vec.as_vec() {
                let val = match logical {
                    Logical::False => 0i32,
                    Logical::True => 1i32,
                    Logical::Na => RObject::NA_INTEGER,
                };
                writer.write_i32::<BigEndian>(val)?;
            }
        }
        RObject::List(elements) => {
            write_flags_with_object(writer, VECSXP, true, false, false, is_s3_object)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element, ref_table, symbol_tracker)?;
            }
        }
        RObject::S4Object(s4_data) => {
            // S4 objects with outer attributes: write S4SXP with HAS_ATTR and IS_S4_BIT
            // Note: OBJ flag is NOT set for S4 objects (only for S3)
            write_flags(writer, S4SXP, true, false, true)?;

            // Build base S4 attributes (class with package + slots)
            let mut merged_attrs =
                build_s4_attributes(&s4_data.class, s4_data.package.as_ref(), &s4_data.slots);

            // Merge outer attributes into S4 attributes
            // Outer attributes can shadow slot names, but NOT the 'class' attribute
            for (key, value) in &attributes.attrs {
                if key.as_ref() != "class" {
                    // Allow outer attributes to override slots (explicit intent)
                    merged_attrs.insert(key.clone(), (**value).clone());
                }
                // If key == "class", silently ignore - S4 class is authoritative
            }

            write_attributes(writer, &merged_attrs, ref_table, symbol_tracker)?;
            return Ok(());
        }
        _ => {
            return Err(Error::Unsupported(
                "Unsupported type for WithAttributes writing".to_string(),
            ));
        }
    }

    write_attributes(writer, attributes, ref_table, symbol_tracker)?;

    Ok(())
}

/// Write attributes as a pairlist.
fn write_attributes(
    writer: &mut Vec<u8>,
    attributes: &Attributes,
    ref_table: &mut RefTable,
    symbol_tracker: &mut SymbolTracker,
) -> Result<()> {
    if attributes.is_empty() {
        return Ok(());
    }

    // Attributes are serialized as a LISTSXP pairlist and are reference-tracked
    // by the parser. Reserve a reference index here to keep ref_table alignment.
    let attr_ref_idx = ref_table.next_index;
    ref_table.next_index += 1;

    // Convert to pairlist elements
    let mut elements = Vec::new();

    // Check if this is a data.frame - they require specific attribute order
    let is_dataframe = attributes.attrs.iter().any(|(k, v)| {
        k.as_ref() == "class"
            && matches!(**v, RObject::Character(ref vec) if vec.iter().any(|s| s.as_ref() == "data.frame"))
    });

    // Check if this is an S4 object - they should preserve slot order
    let is_s4_object = attributes
        .attrs
        .iter()
        .any(|(k, v)| k.as_ref() == "class" && matches!(**v, RObject::WithAttributes { .. }));

    let attrs_iter: Vec<_> = if is_dataframe || is_s4_object {
        // For data.frames and S4 objects, preserve insertion order
        attributes.attrs.iter().collect()
    } else {
        // For other objects, sort keys for consistent output
        let mut sorted_attrs: Vec<_> = attributes.attrs.iter().collect();
        sorted_attrs.sort_by_key(|(k, _)| k.as_ref());
        sorted_attrs
    };

    let debug_attrs = std::env::var("RDS_DEBUG_ATTRS").is_ok();
    if debug_attrs {
        eprintln!(
            "[ATTRS] Writing {} attributes, is_s4={}, attr_ref_idx={}",
            attrs_iter.len(),
            is_s4_object,
            attr_ref_idx
        );
    }

    for (key, value) in attrs_iter {
        if debug_attrs {
            eprintln!(
                "[ATTRS]   tag='{}' value_type={} next_obj_idx={} next_sym_idx={}",
                key,
                value.variant_name(),
                ref_table.next_index,
                ref_table.next_symbol_index
            );
        }
        elements.push(PairlistElement {
            tag: Some(key.clone()),
            value: (**value).clone(), // Unbox the RObject
            tag_object: None,
        });
    }

    // Write the pairlist
    write_pairlist(writer, &elements, ref_table, symbol_tracker)?;

    Ok(())
}
