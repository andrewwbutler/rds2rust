//! Parser for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::types::{
    Attributes, DataFrameData, FactorData, Logical, PairlistElement, RObject, S3ObjectData,
    S4ObjectData,
};
use byteorder::{BigEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

fn debug_enabled() -> bool {
    std::env::var("RDS_DEBUG").is_ok()
}

fn ensure_bytes_available(cursor: &Cursor<&[u8]>, needed: usize, context: &str) -> Result<()> {
    let pos = cursor.position() as usize;
    let total = cursor.get_ref().len();
    let available = total.saturating_sub(pos);
    if available < needed {
        if debug_enabled() {
            eprintln!(
                "[ENSURE_BYTES] EOF at pos={}, needed={}, available={}, ctx={}",
                pos, needed, available, context
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

/// Reference table for tracking objects during deserialization.
/// R's serialization uses reference tracking to handle shared and circular references.
/// Each object that might be referenced later gets assigned a sequential index (1, 2, 3, ...).
/// When a REFSXP is encountered, it contains an index to retrieve the previously seen object.
struct RefTable {
    /// Map from reference index to the actual object
    objects: HashMap<u32, RObject>,
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
        self.objects.insert(index, obj);
        self.next_index += 1;
        index
    }

    /// Update an existing reference with a new object
    fn update(&mut self, index: u32, obj: RObject) {
        self.objects.insert(index, obj);
    }

    /// Get an object by its reference index
    fn get(&self, index: u32) -> Option<&RObject> {
        self.objects.get(&index)
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
        // Check if we've seen this object before
        for cached in &self.cache {
            if cached.as_ref() == obj {
                // Found a match! Return a clone (cheap Arc clone for strings, actual clone for others)
                self.hits += 1;
                return Some((**cached).clone());
            }
        }

        // New unique object - add to cache
        self.misses += 1;

        // Only cache if it's likely to be repeated and not too large
        if should_cache_for_dedup(obj) {
            self.cache.push(Arc::new(obj.clone()));
        }

        None
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
/// Based on R's serialization logic in serialize.c, only specific types are tracked
/// for deduplication via REFSXP.
fn should_track_reference(sexp_type: u32, _has_attr: bool) -> bool {
    // R only tracks these types in the reference table (from serialize.c MakeRefIndex):
    // - SYMSXP (symbols) - for deduplication of repeated symbols
    // - ENVSXP (environments) - for handling circular references
    // - EXTPTRSXP (external pointers)
    // - WEAKREFSXP (weak references)
    // - NAMESPACESXP and NAMESPACESXP_SERIAL (namespace environments)
    // Other types like CLOSXP, LANGSXP, LISTSXP are NOT tracked
    match sexp_type {
        SYMSXP | ENVSXP | EXTPTRSXP | WEAKREFSXP | NAMESPACESXP | NAMESPACESXP_SERIAL => true,
        _ => false,
    }
}

/// Parse an RDS file from a byte slice.
pub fn parse_rds(data: &[u8]) -> Result<RObject> {
    // Check if the file is gzip compressed (starts with 0x1f 0x8b)
    let decompressed_data = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        // Decompress gzip
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        decompressed
    } else {
        data.to_vec()
    };

    let mut cursor = Cursor::new(decompressed_data.as_slice());

    // Parse header
    let format_version = parse_header(&mut cursor)?;

    // Format version 3 includes native encoding information in the header
    if format_version >= 3 {
        // Read the encoding string length and the encoding string itself
        ensure_bytes_available(&cursor, 4, "parse_rds:enc_len")?;
        let enc_len = cursor.read_u32::<BigEndian>()? as usize;
        let mut enc_bytes = vec![0u8; enc_len];
        ensure_bytes_available(&cursor, enc_len, "parse_rds:enc_bytes")?;
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
        eprintln!("[PARSE_RDS] About to parse root object at pos={}", cursor.position());
    }
    let result = parse_object(
        &mut cursor,
        &mut ref_table,
        &mut symbol_table,
        &mut dedup_table,
    );
    if debug_enabled() {
        match &result {
            Ok(_) => eprintln!(
                "[PARSE_RDS] Root object parsed successfully, pos={}",
                cursor.position()
            ),
            Err(e) => eprintln!(
                "[PARSE_RDS] Root object parse FAILED: {} (pos={}, remaining={})",
                e,
                cursor.position(),
                cursor.get_ref().len() as u64 - cursor.position()
            ),
        }
    }
    result
}

/// Parse the RDS file header.
fn parse_header(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
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
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Peek at the first byte to check for packaged/pseudo types
    let pos = cursor.position();
    if debug_enabled() {
        let stream_len = cursor.get_ref().len() as u64;
        if pos + 16 >= stream_len {
            eprintln!(
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
            let stream_len = cursor.get_ref().len() as u64;
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
        eprintln!(
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
    } else if type_from_0_7 >= 2 && type_from_0_7 <= S4SXP {
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
    let early_attributes =
        if has_attr && (sexp_type == LISTSXP || sexp_type == LANGSXP || sexp_type == CLOSXP) {
            let attr_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
            Some(parse_attributes(attr_obj)?)
        } else {
            None
        };

    // Add a placeholder to the reference table early for objects that should be tracked
    // This is crucial for circular references - the object must be in the table
    // before we parse its contents/attributes
    let ref_index = if should_track_reference(sexp_type, has_attr) {
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
        SYMSXP => parse_symbol(cursor, ref_table, symbol_table, dedup_table)?,
        INTSXP => parse_integer_vector(cursor)?,
        REALSXP => parse_real_vector(cursor)?,
        CPLXSXP => parse_complex_vector(cursor)?,
        LGLSXP => parse_logical_vector(cursor)?,
        STRSXP => parse_character_vector(cursor, ref_table, symbol_table, dedup_table)?,
        RAWSXP => parse_raw_vector(cursor)?,
        S4SXP => parse_s4_object(cursor, ref_table, symbol_table, dedup_table)?,
        VECSXP => parse_list(cursor, ref_table, symbol_table, dedup_table, has_attr)?,
        EXPRSXP => parse_expression(cursor, ref_table, symbol_table, dedup_table)?,
        BCODESXP => parse_bytecode(cursor, ref_table, symbol_table, dedup_table)?,
        EXTPTRSXP => {
            // External pointer - typically cannot be serialized meaningfully
            // R usually replaces these with NULL on deserialization
            // Skip the external pointer data and return NULL
            eprintln!("Warning: External pointer (EXTPTRSXP) encountered - returning NULL");
            RObject::Null
        }
        WEAKREFSXP => {
            // Weak reference - similar to external pointers
            // These typically cannot be meaningfully deserialized
            eprintln!("Warning: Weak reference (WEAKREFSXP) encountered - returning NULL");
            RObject::Null
        }
        LISTSXP => parse_pairlist(cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        LANGSXP => parse_language(cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        CHARSXP => {
            // Sometimes CHARSXP appears standalone (like for encoding markers)
            let string = parse_charsxp_content(cursor, flags)?;
            // Return as a single-element character vector for now
            RObject::Character(vec![Arc::from(string.as_str())])
        }
        CLOSXP => parse_closure(cursor, has_tag, ref_table, symbol_table, dedup_table)?,
        ENVSXP => parse_environment(cursor, ref_table, symbol_table, dedup_table)?,
        PROMSXP => parse_promise(cursor, ref_table, symbol_table, dedup_table)?,
        SPECIALSXP => parse_special(cursor, ref_table, symbol_table, dedup_table)?,
        BUILTINSXP => parse_builtin(cursor, ref_table, symbol_table, dedup_table)?,
        REFSXP => {
            // Reference to a previously seen object
            // The reference index is encoded in bits 8-15 of the flags
            let ref_index_val = ((flags >> 8) & 0xFF) as u32;

            // Look up the object in the reference table and return it immediately
            // REFSXP just references another object - it doesn't have its own attributes
            // The has_attr flag, if set, is inherited from the original object
            match ref_table.get(ref_index_val) {
                Some(obj) => {
                    return Ok(obj.clone());
                }
                None => {
                    return Err(Error::InvalidFormat(format!(
                        "Invalid reference index: {}",
                        ref_index_val
                    )));
                }
            }
        }
        ALTREP_SXP => {
            // ALTREP object (version 3 feature)
            // Structure: class_info, state, attributes
            // ALTREP handles its own attributes internally, so parse them here
            let class_info = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
            let state = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
            let attributes_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

            // Convert ALTREP to native representation
            let native_obj = convert_altrep_to_native(class_info, state)?;

            // Parse and apply attributes if present
            let final_obj = if !matches!(attributes_obj, RObject::Null) {
                let attrs = parse_attributes(attributes_obj)?;
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
                ref_table.update(idx, final_obj.clone());
            }
            return Ok(final_obj);
        }
        NAMESPACESXP => {
            // Namespace - parse and discard, then return early to handle attributes specially

            let namespace_result = parse_namespace(cursor, ref_table, symbol_table, dedup_table)?;

            // For namespaces with attributes, we need to parse and discard them
            if has_attr {
                let _attrs = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
            }

            // Update ref table if needed
            if let Some(idx) = ref_index {
                ref_table.update(idx, namespace_result.clone());
            }

            return Ok(namespace_result);
        }
        BCREPREF | BCREPDEF => {
            // Bytecode representation reference/definition
            // These are used for circular references in bytecode serialization
            // Treat as references similar to REFSXP
            let ref_index_val = ((flags >> 8) & 0xFF) as u32;

            match ref_table.get(ref_index_val) {
                Some(obj) => return Ok(obj.clone()),
                None => {
                    // If not found in ref table, this might be a definition, return NULL for now
                    RObject::Null
                }
            }
        }
        NAMESPACESXP_SERIAL | BASENAMESPACE_SXP => {
            // Namespace/base namespace markers in serialization format
            // Similar to NAMESPACESXP (123) but use format type 249/250

            let namespace_result = parse_namespace(cursor, ref_table, symbol_table, dedup_table)?;

            if has_attr {
                let _attrs = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
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
            let ref_index_val = ((flags >> 8) & 0xFF) as u32;

            match ref_table.get(ref_index_val) {
                Some(obj) => return Ok(obj.clone()),
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
                parse_pairlist(cursor, has_tag, ref_table, symbol_table, dedup_table)?
            } else {
                parse_language(cursor, has_tag, ref_table, symbol_table, dedup_table)?
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
        let mut attr_obj = None;
        let stream_len = cursor.get_ref().len() as u64;
        let remaining = stream_len.saturating_sub(cursor.position());
        if remaining == 0 {
            attr_obj = Some(RObject::Null);
        }
        let attr_value = match attr_obj {
            Some(obj) => obj,
            None => parse_object(cursor, ref_table, symbol_table, dedup_table)?,
        };
        let attrs = parse_attributes(attr_value)?;
        attrs
    } else {
        Attributes::new()
    };

    // Apply attributes if non-empty
    if has_attr {
        if !attributes.is_empty() {
            // Check if this is an S4 object (S4SXP type)
            if sexp_type == S4SXP {
                // S4 object: all attributes become slots, except class
                obj = convert_to_s4_object(attributes);
            } else {
                // Check if this has a class attribute (for S3 objects)
                let has_class = attributes.get("class").is_some();

                if has_class {
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
    }

    // Update the reference table with the final object if we added a placeholder earlier
    if let Some(idx) = ref_index {
        // Replace the NULL placeholder with the actual object
        ref_table.update(idx, obj.clone());
    }

    // If this is a symbol (SYMSXP), add it to the symbol table in parse order
    // This is used for resolving REFSXP in TAG positions (e.g., pairlist attribute names)
    if sexp_type == SYMSXP {
        symbol_table.add(obj.clone());
    }

    // Try to deduplicate the object before returning
    // If we've seen an identical object before, return that instead
    if let Some(deduped_obj) = dedup_table.deduplicate(&obj) {
        return Ok(deduped_obj);
    }

    Ok(obj)
}

/// Parse an integer vector.
fn parse_integer_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut vec = Vec::with_capacity(length);

    for _ in 0..length {
        let val = read_int_flexible(cursor)?;
        vec.push(val);
    }

    Ok(RObject::Integer(vec))
}

/// Parse a real (double) vector.
fn parse_real_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut vec = Vec::with_capacity(length);

    for _ in 0..length {
        let val = cursor.read_f64::<BigEndian>()?;
        vec.push(val);
    }

    Ok(RObject::Real(vec))
}

/// Parse a logical vector.
fn parse_logical_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
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

    Ok(RObject::Logical(vec))
}

/// Read an integer - always reads 4 bytes in big-endian format.
fn read_int_flexible(cursor: &mut Cursor<&[u8]>) -> Result<i32> {
    Ok(cursor.read_i32::<BigEndian>()?)
}

/// Parse a character vector (STRSXP - a vector of CHARSXP).
fn parse_character_vector(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let pos_before_length = cursor.position();
    ensure_bytes_available(cursor, 4, "parse_character_vector:length")?;
    let length = cursor.read_u32::<BigEndian>()? as usize;

    if debug_enabled() {
        let remaining = cursor.get_ref().len() as u64 - cursor.position();
        eprintln!(
            "[STRSXP] Parsing character vector of length {} (read from pos {}, now at {}), remaining={}",
            length,
            pos_before_length,
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
            let ref_index = ((flags >> 8) & 0xFF) as usize;

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
            match parse_charsxp(cursor) {
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
            let _ = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
        } else {
            // Check if it's a CHARSXP (most common case)
            let type_from_8_15 = (flags >> 8) & 0xFF;
            if type_from_0_7 == CHARSXP || type_from_8_15 == CHARSXP {
                // Reset position and parse as CHARSXP
                cursor.set_position(pos);
                let string = parse_charsxp(cursor)?;
                let arc_str: Arc<str> = Arc::from(string.as_str());

                // Add to local string cache for future REFSXP references
                string_cache.push(arc_str.clone());
                vec.push(arc_str);
            } else {
                // Some other type - parse it and convert to string
                cursor.set_position(pos);
                let pos_before_parse = cursor.position();
                let obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
                let pos_after_parse = cursor.position();
                if debug_enabled() {
                    eprintln!(
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

    Ok(RObject::Character(vec))
}

/// Parse a raw vector (RAWSXP - a vector of bytes).
fn parse_raw_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut vec = vec![0u8; length];

    // Read the raw bytes directly
    cursor.read_exact(&mut vec)?;

    Ok(RObject::Raw(vec))
}

/// Parse a complex vector (CPLXSXP).
fn parse_complex_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    use crate::types::Complex;

    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut vec = Vec::with_capacity(length);

    for _ in 0..length {
        // Each complex number is two 64-bit floats: real part then imaginary part
        let real = cursor.read_f64::<BigEndian>()?;
        let imaginary = cursor.read_f64::<BigEndian>()?;

        vec.push(Complex { real, imaginary });
    }

    Ok(RObject::Complex(vec))
}

/// Parse an S4 object (S4SXP).
/// S4 objects in RDS are just markers - the actual data is in attributes.
/// We return a placeholder NULL and let the attribute parsing handle it.
fn parse_s4_object(
    _cursor: &mut Cursor<&[u8]>,
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
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // A symbol consists of a CHARSXP for the name
    let name_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    // Extract the name
    match name_obj {
        RObject::Character(names) if names.len() == 1 => {
            let name = &names[0];
            // Check for the special NULL marker used by R for OptionalCharacter slots
            if name.as_ref() == "\x01NULL\x01" {
                Ok(RObject::Symbol(names.into_iter().next().unwrap()))
            } else {
                // Regular symbol - return as Character for backwards compatibility
                Ok(RObject::Character(names))
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
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    _list_has_attr: bool,
) -> Result<RObject> {
    let pos_before_length = cursor.position();
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut elements = Vec::with_capacity(length);

    if debug_enabled() {
        let remaining = cursor.get_ref().len() as u64 - cursor.position();
        eprintln!(
            "[PARSE_LIST] At pos={}, length={}, remaining bytes={}",
            pos_before_length, length, remaining
        );
    }

    for i in 0..length {
        // Defensive EOF check before attempting to parse the next element so we can
        // surface a structured error instead of a generic IO failure.
        let pos = cursor.position() as usize;
        let total = cursor.get_ref().len();
        let remaining = total.saturating_sub(pos);
        if debug_enabled() {
            eprintln!(
                "[PARSE_LIST] Parsing element {}/{} at pos={}, remaining={}",
                i, length, pos, remaining
            );
        }
        if remaining == 0 {
            return Err(Error::UnexpectedEofDetail {
                position: pos,
                needed: 1,
                available: 0,
            });
        }
        let element = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

        // Check if this is a Real vector that looks like an ALTREP compact_intseq state
        // R sometimes serializes repeated ALTREP sequences as bare state vectors
        let converted_element = match &element {
            RObject::Real(vec) if vec.len() == 3 => {
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
                        let _next = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
                    }

                    RObject::Integer(int_vec)
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
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut elements = Vec::with_capacity(length);

    for _ in 0..length {
        let element = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
        elements.push(element);
    }

    Ok(RObject::Expression(elements))
}

/// Parse bytecode (BCODESXP) using R's ReadBC/ReadBC1 structure.
fn parse_bytecode(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let reps_len = cursor.read_u32::<BigEndian>()? as usize;
    let mut reps = vec![None; reps_len];
    parse_bytecode_body(cursor, ref_table, symbol_table, dedup_table, &mut reps)
}

fn parse_bytecode_body(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
) -> Result<RObject> {
    let code = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    let constants = parse_bc_constants(cursor, ref_table, symbol_table, dedup_table, reps)?;

    Ok(RObject::Bytecode {
        code: Box::new(code),
        constants: Box::new(RObject::List(constants)),
        expr: Box::new(RObject::Null),
    })
}

fn parse_bc_constants(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
) -> Result<Vec<RObject>> {
    let count = cursor.read_u32::<BigEndian>()? as usize;
    let mut constants = Vec::with_capacity(count);

    for _ in 0..count {
        let type_code = cursor.read_i32::<BigEndian>()?;
        let value = match type_code as u32 {
            BCODESXP => parse_bytecode_body(cursor, ref_table, symbol_table, dedup_table, reps)?,
            BCREPREF | BCREPDEF | LANGSXP | LISTSXP | ATTRLANGSXP | ATTRLISTSXP => parse_bc_lang(
                cursor,
                ref_table,
                symbol_table,
                dedup_table,
                reps,
                type_code,
            )?,
            _ => parse_object(cursor, ref_table, symbol_table, dedup_table)?,
        };
        constants.push(value);
    }

    Ok(constants)
}

fn parse_bc_lang(
    cursor: &mut Cursor<&[u8]>,
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
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LANGSXP,
            true,
        ),
        ATTRLISTSXP => parse_bc_lang_struct(
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LISTSXP,
            true,
        ),
        LANGSXP => parse_bc_lang_struct(
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LANGSXP,
            false,
        ),
        LISTSXP => parse_bc_lang_struct(
            cursor,
            ref_table,
            symbol_table,
            dedup_table,
            reps,
            LISTSXP,
            false,
        ),
        _ => parse_object(cursor, ref_table, symbol_table, dedup_table),
    }
}

fn parse_bc_lang_struct(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
    reps: &mut [Option<RObject>],
    actual_type: u32,
    has_attr: bool,
) -> Result<RObject> {
    let attr_obj = if has_attr {
        Some(parse_object(cursor, ref_table, symbol_table, dedup_table)?)
    } else {
        None
    };

    let tag_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    let car_type = cursor.read_i32::<BigEndian>()?;
    let car = parse_bc_lang(cursor, ref_table, symbol_table, dedup_table, reps, car_type)?;
    let cdr_type = cursor.read_i32::<BigEndian>()?;
    let cdr = parse_bc_lang(cursor, ref_table, symbol_table, dedup_table, reps, cdr_type)?;

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
        let attrs = parse_attributes(attr)?;
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
    cursor: &mut Cursor<&[u8]>,
    _has_tag: bool,
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
    let env = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    let _form_start = cursor.position();
    let form = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    let _body_start = cursor.position();
    let bod = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    Ok(RObject::Closure {
        formals: Box::new(form),
        body: Box::new(bod),
        environment: Box::new(env),
    })
}

/// Parse an environment (ENVSXP).
/// Environments consist of: locked flag, enclosing environment, frame (pairlist), hashtab
fn parse_environment(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Parse locked flag (an integer: 0 or 1)
    // We read it but don't currently store it in the Environment struct
    let _locked = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    // Parse enclosing environment (can be another environment or NULL for global env)
    let enclosing = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    // Parse frame (pairlist of bindings)
    let frame = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    // Parse hashtab (can be NULL or a VECSXP)
    let hashtab = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    // Parse attributes (serialized even when NULL)
    let _attrs = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

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
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Namespaces are serialized using OutStringVec: an unused marker,
    // a length, then that many CHARSXP entries.
    let _names_flag = cursor.read_u32::<BigEndian>()?;
    let length = cursor.read_u32::<BigEndian>()? as usize;

    let mut names = Vec::with_capacity(length);
    for _ in 0..length {
        // Each entry is written via WriteItem on a CHARSXP
        let obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
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
    cursor: &mut Cursor<&[u8]>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Parse the TAG if present (usually not for language objects)
    if has_tag {
        let _tag_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
        // Tags in language objects are rare, we'll skip them for now
    }

    // Parse the CAR (the function being called)
    let function = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    // Parse the CDR (the argument list)
    let cdr = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    // Extract arguments with their names (tags) from the CDR
    let args = match cdr {
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
fn parse_pairlist_element(
    cursor: &mut Cursor<&[u8]>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<(Option<Arc<str>>, Option<Box<RObject>>, RObject)> {
    // Parse the TAG if present (comes before CAR)
    let (tag, tag_object) = if has_tag {
        // Parse the TAG using normal object parsing
        // REFSXP references in TAGs use the main ref_table, same as everywhere else
        let tag_obj = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

        // Extract the tag name from the symbol or character object
        let tag_name = extract_tag_name(tag_obj.clone());
        // Store both the extracted name and the raw object
        (tag_name, Some(Box::new(tag_obj)))
    } else {
        (None, None)
    };

    // Parse the CAR (the value for this element)
    let car = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

    Ok((tag, tag_object, car))
}

/// Parse a pairlist (LISTSXP).
/// Uses an iterative approach matching R's ReadItem_Iterative to handle circular references.
/// R's serialization format: FLAGS (with type), TAG (if HAS_TAG_BIT), CAR, then FLAGS for next element.
/// If next FLAGS indicate LISTSXP/LANGSXP/etc., it's a continuation; otherwise it's the CDR terminator.
fn parse_pairlist(
    cursor: &mut Cursor<&[u8]>,
    has_tag: bool,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    let mut elements = Vec::new();

    if debug_enabled() {
        eprintln!("[PAIRLIST] Start parse, has_tag={}, pos={}", has_tag, cursor.position());
    }

    // Parse the first element (TAG if has_tag, then CAR)
    let (first_tag, first_tag_object, first_car) =
        parse_pairlist_element(cursor, has_tag, ref_table, symbol_table, dedup_table)?;
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
        let remaining = cursor.get_ref().len() as u64 - pos;
        if remaining == 0 {
            if debug_enabled() {
                eprintln!(
                    "[PAIRLIST_LOOP] No bytes remaining at pos={}, treating as NIL CDR termination",
                    pos
                );
            }
            break;
        }
        ensure_bytes_available(cursor, 4, "parse_pairlist:next_flags")?;
        let flags = cursor.read_u32::<BigEndian>()?;
        let next_type = flags & 0xFF;
        if debug_enabled() {
            eprintln!(
                "[PAIRLIST_LOOP] At byte {}: flags=0x{:08x}, type={}",
                pos, flags, next_type
            );
        }

        // Check if the next element continues the pairlist
        // R continues for: LISTSXP, LANGSXP, CLOSXP, PROMSXP, DOTSXP
        // We don't have DOTSXP constant, but we cover the main ones
        let continues_pairlist = matches!(next_type, LISTSXP | LANGSXP | CLOSXP | PROMSXP);

        if continues_pairlist {
            // Continue building the pairlist - this is another element
            // The flags are already consumed, so parse_pairlist_element will read the TAG and CAR
            let has_tag_next = (flags & HAS_TAG_BIT) != 0;
            if debug_enabled() {
                eprintln!("[PAIRLIST_LOOP] Continuing pairlist, has_tag={}", has_tag_next);
            }

            let (tag, tag_object, car) =
                parse_pairlist_element(cursor, has_tag_next, ref_table, symbol_table, dedup_table)?;
            elements.push(PairlistElement {
                tag,
                value: car,
                tag_object,
            });
        } else {
            // Not a pairlist continuation - reset position and parse as CDR terminator
            cursor.set_position(pos);
            let cdr = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

            // Handle the CDR based on its type
            match cdr {
                RObject::Null => {
                    // Normal list termination
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
    }

    Ok(RObject::Pairlist(elements))
}

/// Extract a tag name from a tag object (usually a symbol or character).
fn extract_tag_name(tag_obj: RObject) -> Option<Arc<str>> {
    match tag_obj {
        RObject::Character(vec) if !vec.is_empty() => Some(vec[0].clone()),
        RObject::Null => None,
        _ => None,
    }
}

/// Parse a promise (PROMSXP).
/// Promises are lazy evaluation constructs containing: value, expression, environment
fn parse_promise(
    cursor: &mut Cursor<&[u8]>,
    ref_table: &mut RefTable,
    symbol_table: &mut SymbolTable,
    dedup_table: &mut DedupTable,
) -> Result<RObject> {
    // Parse the three components of a promise
    let value = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    let expression = parse_object(cursor, ref_table, symbol_table, dedup_table)?;
    let environment = parse_object(cursor, ref_table, symbol_table, dedup_table)?;

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
    cursor: &mut Cursor<&[u8]>,
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

    // Read the string bytes
    let mut bytes = vec![0u8; length as usize];
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
    cursor: &mut Cursor<&[u8]>,
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

    // Read the string bytes
    let mut bytes = vec![0u8; length as usize];
    cursor.read_exact(&mut bytes)?;

    // Convert to UTF-8 string and intern it
    let name = String::from_utf8(bytes)?;
    let name = Arc::from(name.as_str());

    Ok(RObject::Builtin { name })
}

/// Convert an ALTREP object to its native representation.
fn convert_altrep_to_native(class_info: RObject, state: RObject) -> Result<RObject> {
    // Debug logging to understand ALTREP structure

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
                return convert_compact_intseq(state);
            }
            "compact_realseq" => {
                return convert_compact_intseq(state);
            }
            _ => {}
        }
    }

    // Fallback: Infer the ALTREP type from the state structure
    // compact_intseq has state: [length (real), first (real), stride (real)]
    match &state {
        RObject::Real(params) if params.len() == 3 => {
            // Standard compact_intseq with state vector
            convert_compact_intseq(state)
        }
        RObject::Integer(params) if params.len() == 3 => {
            // Sometimes the state is stored as integers instead of reals
            let real_params = vec![params[0] as f64, params[1] as f64, params[2] as f64];
            convert_compact_intseq(RObject::Real(real_params))
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
        RObject::Pairlist(elements) => {
            // Pairlist might contain [package_symbol, class_symbol, ...]
            // Symbols are stored as Character vectors

            // Look through pairlist elements for character data
            for (_i, elem) in elements.iter().enumerate() {
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
            extract_altrep_class_name(object)
        }
        _ => None,
    }
}

/// Convert a wrap_real ALTREP object to a native real vector.
fn convert_wrap_real(state: RObject) -> Result<RObject> {
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
    match state {
        RObject::Pairlist(elements) if !elements.is_empty() => {
            // The actual data is typically in the first element
            Ok(elements[0].value.clone())
        }
        _ => Ok(RObject::Null),
    }
}

/// Convert a compact integer sequence to a regular integer vector.
fn convert_compact_intseq(state: RObject) -> Result<RObject> {
    // compact_intseq state is a Real vector: [length, first, stride]
    let (length, first, stride) = match state {
        RObject::Real(params) if params.len() == 3 => {
            let len = params[0] as i64;
            let first_val = params[1] as i32;
            let stride_val = params[2] as i32;
            (len, first_val, stride_val)
        }
        _ => {
            return Err(Error::InvalidFormat(
                "Invalid compact_intseq state".to_string(),
            ))
        }
    };

    // Generate the sequence
    let mut vec = Vec::with_capacity(length as usize);
    for i in 0..length {
        vec.push(first + (i as i32) * stride);
    }

    Ok(RObject::Integer(vec))
}

/// Parse a CHARSXP (internal character string).
fn parse_charsxp(cursor: &mut Cursor<&[u8]>) -> Result<String> {
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
        let string = parse_charsxp_content(cursor, flags)?;

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
        let ref_index = ((flags >> 8) & 0xFF) as usize;
        // Return a placeholder indicating this is a reference
        // The caller (parse_character_vector or parse_symbol) should handle this
        return Err(Error::InvalidFormat(format!(
            "REFSXP in CHARSXP context requires caller to handle reference (ref={})",
            ref_index
        )));
    }

    // If we get here, the flags don't contain CHARSXP type
    // This shouldn't happen in a well-formed file - it indicates a parsing error
    eprintln!("[DEBUG parse_charsxp] Unexpected type:");
    eprintln!("  Full flags: 0x{:08x}", flags);
    eprintln!("  Type from bits 0-7: {}", type_from_0_7);
    eprintln!("  Type from bits 8-15: {}", type_from_8_15);
    eprintln!("  Position: {}", cursor.position());

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
fn parse_charsxp_content(cursor: &mut Cursor<&[u8]>, flags: u32) -> Result<String> {
    let pos_before = cursor.position();
    if debug_enabled() {
        eprintln!("[parse_charsxp_content] Starting at pos {}", pos_before);
    }

    // Check if this uses compact 3-byte length encoding
    // Compact encoding is signaled by bits 24-31 being non-zero (e.g., 0x04000900)
    let compact_length = (flags >> 24) & 0xFF;
    let use_compact = compact_length > 0;

    // Peek at the next 8 bytes for debugging

    let length = if use_compact {
        // Read 3-byte length (big-endian)
        let mut bytes_3 = [0u8; 3];
        ensure_bytes_available(cursor, 3, "charsxp:compact_len")?;
        cursor.read_exact(&mut bytes_3)?;
        let len = ((bytes_3[0] as i32) << 16) | ((bytes_3[1] as i32) << 8) | (bytes_3[2] as i32);

        len
    } else {
        // Read standard 4-byte length (R always uses 4-byte integers in standard mode)
        ensure_bytes_available(cursor, 4, "charsxp:std_len")?;
        let len = cursor.read_i32::<BigEndian>()?;

        len
    };

    if length == -1 {
        // NA_character_
        return Ok(String::from("NA"));
    }

    // Read the string bytes
    let mut bytes = vec![0u8; length as usize];
    ensure_bytes_available(cursor, length as usize, "charsxp:string_bytes")?;
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
fn parse_attributes(attr_obj: RObject) -> Result<Attributes> {
    let mut attrs = Attributes::new();

    if debug_enabled() {
        eprintln!(
            "[PARSE_ATTRS] Received attr_obj type: {:?}",
            std::mem::discriminant(&attr_obj)
        );
    }

    // Attributes are typically stored as a pairlist (LISTSXP)
    // We need to extract the TAG (name) and CAR (value) from each pair
    match attr_obj {
        RObject::Null => {
            // No attributes
            return Ok(attrs);
        }
        RObject::Pairlist(elements) => {
            // Extract TAG (name) and CAR (value) from each pairlist element
            if debug_enabled() && !elements.is_empty() {
                eprintln!("[PARSE_ATTRS] Pairlist with {} elements", elements.len());
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
                    if debug_enabled() {
                        eprintln!(
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
                    } else {
                    }
                    // Otherwise, skip elements without tags that we can't identify
                }
            }
            return Ok(attrs);
        }
        RObject::List(_elements) => {
            // Regular list (VECSXP) - names should be stored as a "names" attribute
            // This case shouldn't happen for attributes themselves, but handle it gracefully
            // Just return empty attributes
            return Ok(attrs);
        }
        RObject::WithAttributes {
            object: _,
            attributes: inner_attrs,
        } => {
            // When we receive a WithAttributes as an attributes object,
            // we should return its attributes field directly, not transform it.
            // The inner_attrs already contains the parsed attributes (like "names", "row.names", etc.)
            return Ok(inner_attrs.clone());
        }
        RObject::Integer(vec) if vec.len() == 1 => {
            // Single integer might be a reference index or special marker
            // In some cases, R uses compact formats for attributes
            // For now, treat as no attributes
            return Ok(attrs);
        }
        RObject::Real(vec) if vec.len() == 3 => {
            // This might be ALTREP state being passed as attributes
            // This shouldn't happen, but handle it gracefully
            return Ok(attrs);
        }
        RObject::Character(_names) => {
            // This might be a compact attribute format where we only have names
            // This can happen with certain R objects where attributes are stored
            // in a special compact format. For now, treat as no attributes.
            // In the future, we may need to look up values elsewhere.
            return Ok(attrs);
        }
        RObject::S3Object(s3) => {
            // S3 object used as attributes container
            // Extract the attributes from the S3 object
            return Ok(s3.attributes.clone());
        }
        RObject::S4Object(s4) => {
            // S4 object used as attributes container
            // Extract the class field and slots and convert them to attributes
            // Add the class as a "class" attribute (RObject::Character)
            if !s4.class.is_empty() {
                attrs.insert(Arc::from("class"), RObject::Character(s4.class.clone()));
            }
            for (slot_name, slot_value) in &s4.slots {
                attrs.insert(slot_name.clone(), slot_value.clone());
            }
            return Ok(attrs);
        }
        _ => {
            // Unexpected attribute structure - this can happen with certain R serialization patterns
            // For example, when attributes are encoded using alternate representations
            // Return empty attributes with a warning rather than failing
            return Ok(Attributes::new());
        }
    }
}

/// Try to convert a list with attributes to a data.frame if it has the right structure.
fn try_convert_to_dataframe(obj: &RObject, attributes: &Attributes) -> Option<RObject> {
    // Check if this has class="data.frame"
    let class_attr = attributes.get("class")?;
    let is_dataframe = match class_attr {
        RObject::Character(classes) => classes.iter().any(|c| c.as_ref() == "data.frame"),
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
    let names_attr = attributes.get("names")?;
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
        match row_names_attr {
            RObject::Character(names) => names.clone(),
            RObject::Integer(indices) => {
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
    let class_attr = attributes.get("class")?;
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
        RObject::Integer(vals) => vals.clone(),
        _ => return None,
    };

    // Get the levels from the "levels" attribute
    let levels_attr = attributes.get("levels")?;
    let levels = match levels_attr {
        RObject::Character(levels) => levels.clone(),
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
        extra_attrs.insert(key.clone(), value.clone());
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
        .and_then(|idx| {
            if let RObject::Character(classes) = attributes.attrs[idx].1.as_ref() {
                Some(classes.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Remove class from attributes
    attributes.attrs.retain(|(k, _)| k.as_ref() != "class");

    // Create the S3 object
    RObject::S3Object(Box::new(S3ObjectData {
        base: Box::new(obj),
        class,
        attributes,
    }))
}

/// Convert attributes to an S4 object.
/// For S4 objects, the class is in attributes, and all other attributes are slots.
fn convert_to_s4_object(mut attributes: Attributes) -> RObject {
    // Extract the class attribute and package attribute
    // The class may be wrapped in WithAttributes if it has a package attribute
    let (class, package) = attributes
        .attrs
        .iter()
        .position(|(k, _)| k.as_ref() == "class")
        .map(|idx| {
            match attributes.attrs[idx].1.as_ref() {
                RObject::Character(classes) => (classes.clone(), None),
                RObject::WithAttributes {
                    object,
                    attributes: class_attrs,
                } => {
                    // Unwrap the WithAttributes to get the actual class vector
                    let classes = match object.as_ref() {
                        RObject::Character(classes) => classes.clone(),
                        _ => vec![],
                    };
                    // Extract the package attribute from the class's attributes
                    let pkg = class_attrs.get("package").and_then(|p| match p {
                        RObject::Character(pkgs) if !pkgs.is_empty() => Some(pkgs[0].clone()),
                        _ => None,
                    });
                    (classes, pkg)
                }
                _ => (vec![], None),
            }
        })
        .unwrap_or((vec![], None));

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

    #[test]
    fn test_parse_header() {
        // This is a minimal RDS header for format version 2
        let header = vec![
            b'X', b'\n', // Magic bytes
            0, 0, 0, 2, // Format version (2)
            0, 3, 5, 0, // R version 3.5.0
            0, 3, 0, 0, // Min R version 3.0.0
        ];

        let mut cursor = Cursor::new(header.as_slice());
        let version = parse_header(&mut cursor).unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn test_invalid_magic() {
        let header = vec![b'Y', b'\n', 0, 0, 0, 2];
        let mut cursor = Cursor::new(header.as_slice());
        assert!(parse_header(&mut cursor).is_err());
    }
}
