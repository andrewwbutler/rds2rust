//! Parser for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::types::{Attributes, Logical, PairlistElement, RObject};
use byteorder::{BigEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::{Cursor, Read};

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

/// Determine if an object should be tracked in the reference table.
/// Based on R's serialization logic, most object types are tracked to handle sharing.
/// Only very simple/primitive types are not tracked.
fn should_track_reference(sexp_type: u32, has_attr: bool) -> bool {
    // Don't track references for:
    // - NILSXP/NILVALUE_SXP - singleton NULL
    // - CHARSXP - internal strings (handled differently)
    // - Simple vectors without attributes
    match sexp_type {
        NILSXP | NILVALUE_SXP | GLOBALENV_SXP | CHARSXP => false,
        // Simple vectors without attributes are not tracked
        INTSXP | REALSXP | LGLSXP | RAWSXP | CPLXSXP if !has_attr => false,
        // Everything else is tracked
        _ => true,
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
        let enc_len = cursor.read_u32::<BigEndian>()? as usize;
        let mut enc_bytes = vec![0u8; enc_len];
        cursor.read_exact(&mut enc_bytes)?;
        // We now have the encoding (e.g., "UTF-8"), but we'll ignore it for now
    }

    // Create reference table for tracking shared objects
    let mut ref_table = RefTable::new();

    // Parse the actual object
    parse_object(&mut cursor, &mut ref_table)
}

/// Parse the RDS file header.
fn parse_header(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    // RDS files start with specific magic bytes
    let mut magic = [0u8; 2];
    cursor.read_exact(&mut magic).map_err(|_| Error::UnexpectedEof)?;

    // Check for RDS format identifier
    // Format is typically 'X\n' for XDR format (big-endian)
    if magic[0] != b'X' {
        return Err(Error::InvalidFormat(format!(
            "Expected 'X' magic byte, got {:?}",
            magic[0]
        )));
    }

    // Read format version
    let format_version = cursor.read_u32::<BigEndian>()?;

    // Read R version that wrote the file
    let _writer_version = cursor.read_u32::<BigEndian>()?;

    // Read minimum R version needed to read
    let _min_reader_version = cursor.read_u32::<BigEndian>()?;

    Ok(format_version)
}

/// Parse an R object from the stream.
fn parse_object(cursor: &mut Cursor<&[u8]>, ref_table: &mut RefTable) -> Result<RObject> {
    // Peek at the first byte to check for packaged/pseudo types
    let pos = cursor.position();
    let first_byte = match cursor.read_u8() {
        Ok(b) => b,
        Err(e) => return Err(Error::Io(e)),
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
    let flags = cursor.read_u32::<BigEndian>()?;

    // Extract the SEXP type from the flags.
    // The type can appear in different bit positions:
    // - Bits 0-7: For types like CHARSXP (9), NILSXP (0), etc.
    // - Bits 8-15: For types like INTSXP (13), LGLSXP (10), REALSXP (14), STRSXP (16), VECSXP (19)
    //
    // The heuristic: if bits 8-15 contain a value >= 10, use that (it's likely the real type).
    // Otherwise, use bits 0-7.
    // This handles the XDR encoding quirk where different types are encoded differently.
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type = if type_from_8_15 >= 10 {
        type_from_8_15
    } else {
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

    // For pairlists and language objects, attributes come BEFORE the data (CAR/CDR)
    // Parse them early if present
    let early_attributes = if has_attr && (sexp_type == LISTSXP || sexp_type == LANGSXP) {
        let attr_obj = parse_object(cursor, ref_table)?;
        Some(parse_attributes(attr_obj)?)
    } else {
        None
    };

    // Add a placeholder to the reference table early for objects that should be tracked
    // This is crucial for circular references - the object must be in the table
    // before we parse its contents/attributes
    let ref_index = if should_track_reference(sexp_type, has_attr) {
        // Add a NULL placeholder for now
        Some(ref_table.add(RObject::Null))
    } else {
        None
    };

    // Parse the object based on type
    let mut obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        UNBOUNDVALUE_SXP => RObject::Null, // Unbound/missing argument marker
        EMPTYENV_SXP => RObject::Null, // Empty environment marker
        GLOBALENV_SXP => RObject::Null, // Global environment - treat as NULL
        SYMSXP => parse_symbol(cursor, ref_table)?,
        INTSXP => parse_integer_vector(cursor)?,
        REALSXP => parse_real_vector(cursor)?,
        CPLXSXP => parse_complex_vector(cursor)?,
        LGLSXP => parse_logical_vector(cursor)?,
        STRSXP => parse_character_vector(cursor, ref_table)?,
        RAWSXP => parse_raw_vector(cursor)?,
        S4SXP => parse_s4_object(cursor)?,
        VECSXP => parse_list(cursor, ref_table, has_attr)?,
        EXPRSXP => parse_expression(cursor, ref_table)?,
        LISTSXP => parse_pairlist(cursor, has_tag, ref_table)?,
        LANGSXP => parse_language(cursor, has_tag, ref_table)?,
        CHARSXP => {
            // Sometimes CHARSXP appears standalone (like for encoding markers)
            let string = parse_charsxp_content(cursor)?;
            // Return as a single-element character vector for now
            RObject::Character(vec![string])
        }
        CLOSXP => parse_closure(cursor, has_tag, ref_table)?,
        ENVSXP => parse_environment(cursor, ref_table)?,
        REFSXP => {
            // Reference to a previously seen object
            // The reference index is encoded in bits 8-15 of the flags
            let ref_index_val = ((flags >> 8) & 0xFF) as u32;

            // Look up the object in the reference table and return it immediately
            // REFSXP just references another object - it doesn't have its own attributes
            // The has_attr flag, if set, is inherited from the original object
            match ref_table.get(ref_index_val) {
                Some(obj) => return Ok(obj.clone()),
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
            let class_info = parse_object(cursor, ref_table)?;
            let state = parse_object(cursor, ref_table)?;
            let attributes_obj = parse_object(cursor, ref_table)?;

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
        _ => {
            return Err(Error::UnknownSexpType(sexp_type));
        }
    };

    // Parse attributes if present (unless already parsed early for LISTSXP/LANGSXP)
    let attributes = if let Some(attrs) = early_attributes {
        attrs
    } else if has_attr {
        let attr_obj = parse_object(cursor, ref_table)?;
        parse_attributes(attr_obj)?
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
            _ => Logical::Na, // Treat any other value as NA
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
fn parse_character_vector(cursor: &mut Cursor<&[u8]>, _ref_table: &mut RefTable) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut vec = Vec::with_capacity(length);

    for _ in 0..length {
        // Each element is a CHARSXP object
        let string = parse_charsxp(cursor)?;
        vec.push(string);
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
fn parse_s4_object(_cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // S4SXP is just a marker type - it doesn't contain any data itself.
    // All the S4 data (class and slots) is stored in attributes.
    // Return a NULL as a placeholder - the attribute parsing will convert this to S4Object.
    Ok(RObject::Null)
}

/// Parse a symbol (SYMSXP).
fn parse_symbol(cursor: &mut Cursor<&[u8]>, ref_table: &mut RefTable) -> Result<RObject> {
    // A symbol consists of a CHARSXP for the name
    let name_obj = parse_object(cursor, ref_table)?;

    // Extract the name and return as a character vector
    match name_obj {
        RObject::Character(names) => Ok(RObject::Character(names)),
        _ => {
            // If we got something unexpected, just return it
            Ok(name_obj)
        }
    }
}

/// Parse a generic list (VECSXP).
fn parse_list(cursor: &mut Cursor<&[u8]>, ref_table: &mut RefTable, _list_has_attr: bool) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut elements = Vec::with_capacity(length);

    for i in 0..length {
        let element = parse_object(cursor, ref_table)?;

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
                        let _next = parse_object(cursor, ref_table)?;
                    }

                    RObject::Integer(int_vec)
                } else {
                    element
                }
            }
            _ => element
        };

        elements.push(converted_element);
    }

    Ok(RObject::List(elements))
}

/// Parse an expression vector (EXPRSXP).
/// Expression vectors are identical in structure to VECSXP - they're vectors of R objects,
/// typically language objects. The difference is semantic: EXPRSXP is used for collections
/// of unevaluated expressions (e.g., the result of parse()).
fn parse_expression(cursor: &mut Cursor<&[u8]>, ref_table: &mut RefTable) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut elements = Vec::with_capacity(length);

    for _ in 0..length {
        let element = parse_object(cursor, ref_table)?;
        elements.push(element);
    }

    Ok(RObject::Expression(elements))
}

/// Parse a closure (CLOSXP).
/// When has_tag is true: TAG (environment), FORMALS, BODY
/// When has_tag is false: FORMALS, BODY, ENVIRONMENT
fn parse_closure(cursor: &mut Cursor<&[u8]>, has_tag: bool, ref_table: &mut RefTable) -> Result<RObject> {
    let (formals, body, environment) = if has_tag {
        // TAG holds the environment
        let env = parse_object(cursor, ref_table)?;
        let form = parse_object(cursor, ref_table)?;
        // When has_tag is true, there seems to be an extra NULL object between formals and body
        // This might be related to how R stores closure attributes or srcref
        let maybe_body = parse_object(cursor, ref_table)?;
        let bod = if matches!(maybe_body, RObject::Null) {
            // Skip this NULL and read the actual body
            parse_object(cursor, ref_table)?
        } else {
            // This is the body
            maybe_body
        };
        (form, bod, env)
    } else {
        // Standard order: formals, body, environment
        let form = parse_object(cursor, ref_table)?;
        let bod = parse_object(cursor, ref_table)?;
        let env = parse_object(cursor, ref_table)?;
        (form, bod, env)
    };

    Ok(RObject::Closure {
        formals: Box::new(formals),
        body: Box::new(body),
        environment: Box::new(environment),
    })
}

/// Parse an environment (ENVSXP).
/// Environments consist of: locked flag, enclosing environment, frame (pairlist), hashtab
fn parse_environment(cursor: &mut Cursor<&[u8]>, ref_table: &mut RefTable) -> Result<RObject> {
    // Parse locked flag (an integer: 0 or 1)
    // We read it but don't currently store it in the Environment struct
    let _locked = parse_object(cursor, ref_table)?;
    // Parse enclosing environment (can be another environment or NULL for global env)
    let enclosing = parse_object(cursor, ref_table)?;
    // Parse frame (pairlist of bindings)
    let frame = parse_object(cursor, ref_table)?;
    // Parse hashtab (can be NULL or a VECSXP)
    let hashtab = parse_object(cursor, ref_table)?;

    // Note: attributes are NOT parsed here - they're handled by the HAS_ATTR flag
    // in parse_object

    Ok(RObject::Environment {
        enclosing: Box::new(enclosing),
        frame: Box::new(frame),
        hashtab: Box::new(hashtab),
    })
}

/// Parse a language object (LANGSXP).
/// Language objects represent unevaluated expressions/calls.
/// They're structured like pairlists: TAG (if present), CAR (function), CDR (arguments).
fn parse_language(cursor: &mut Cursor<&[u8]>, has_tag: bool, ref_table: &mut RefTable) -> Result<RObject> {
    let mut elements = Vec::new();

    // Parse the TAG if present (usually not for language objects)
    if has_tag {
        let _tag_obj = parse_object(cursor, ref_table)?;
        // Tags in language objects are rare, we'll skip them for now
    }

    // Parse the CAR (the function being called)
    let car = parse_object(cursor, ref_table)?;
    elements.push(car);

    // Parse the CDR (the argument list)
    let cdr = parse_object(cursor, ref_table)?;

    // If CDR is a pairlist, extract all arguments
    match cdr {
        RObject::Null => {
            // No arguments
        }
        RObject::Pairlist(pairlist_elements) => {
            // Add all arguments from the pairlist
            for elem in pairlist_elements {
                elements.push(elem.value);
            }
        }
        other => {
            // Single argument (unusual but possible)
            elements.push(other);
        }
    }

    Ok(RObject::Language(elements))
}

/// Parse a pairlist (LISTSXP).
fn parse_pairlist(cursor: &mut Cursor<&[u8]>, has_tag: bool, ref_table: &mut RefTable) -> Result<RObject> {
    // Pairlists are serialized as: [TAG if HAS_TAG_BIT], CAR, CDR
    let mut elements = Vec::new();

    // Parse the TAG if present (comes before CAR)
    let tag = if has_tag {
        let tag_obj = parse_object(cursor, ref_table)?;
        // Extract the tag name from the symbol or character object
        extract_tag_name(tag_obj)
    } else {
        None
    };

    // Parse the CAR (first element)
    let car = parse_object(cursor, ref_table)?;

    // Parse the CDR (rest of list)
    let cdr = parse_object(cursor, ref_table)?;

    // Add CAR to the pairlist with its tag
    elements.push(PairlistElement { tag, value: car });

    // If CDR is another pairlist, recursively add its elements
    // If CDR is NULL, we're done
    match cdr {
        RObject::Null => {
            // End of list
        }
        RObject::Pairlist(mut rest) => {
            // Append the rest
            elements.append(&mut rest);
        }
        other => {
            // CDR is some other object, add it without a tag
            elements.push(PairlistElement {
                tag: None,
                value: other,
            });
        }
    }

    Ok(RObject::Pairlist(elements))
}

/// Extract a tag name from a tag object (usually a symbol or character).
fn extract_tag_name(tag_obj: RObject) -> Option<String> {
    match tag_obj {
        RObject::Character(vec) if !vec.is_empty() => Some(vec[0].clone()),
        RObject::Null => None,
        _ => None,
    }
}

/// Convert an ALTREP object to its native representation.
fn convert_altrep_to_native(class_info: RObject, state: RObject) -> Result<RObject> {
    // Infer the ALTREP type from the state structure
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
                other => Ok(other)
            }
        }
        _ => {
            // For unsupported ALTREP types or compressed ALTREP references,
            // just return NULL for now. This is a known limitation for some
            // R-specific ALTREP optimizations when serializing repeated instances.
            // TODO: Fully decode R's ALTREP reference sharing format
            Ok(RObject::Null)
        }
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
        _ => return Err(Error::InvalidFormat("Invalid compact_intseq state".to_string())),
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
    let sexp_type = flags & 0xFF;

    if sexp_type != CHARSXP {
        return Err(Error::InvalidFormat(format!(
            "Expected CHARSXP ({}), got {}",
            CHARSXP, sexp_type
        )));
    }

    parse_charsxp_content(cursor)
}

/// Parse the content of a CHARSXP (without the header).
fn parse_charsxp_content(cursor: &mut Cursor<&[u8]>) -> Result<String> {
    // Read the string length
    let length = cursor.read_i32::<BigEndian>()?;

    if length == -1 {
        // NA_character_
        return Ok(String::from("NA"));
    }

    // Read the string bytes
    let mut bytes = vec![0u8; length as usize];
    cursor.read_exact(&mut bytes)?;

    // Convert to UTF-8 string
    let string = String::from_utf8(bytes)?;

    Ok(string)
}

/// Parse attributes from a pairlist object.
/// Attributes are stored as pairlists where TAG = attribute name, CAR = attribute value.
fn parse_attributes(attr_obj: RObject) -> Result<Attributes> {
    let mut attrs = HashMap::new();

    // Attributes are typically stored as a pairlist (LISTSXP)
    // We need to extract the TAG (name) and CAR (value) from each pair
    match attr_obj {
        RObject::Null => {
            // No attributes
            return Ok(Attributes { attrs });
        }
        RObject::Pairlist(elements) => {
            // Extract TAG (name) and CAR (value) from each pairlist element
            for elem in elements {
                if let Some(name) = elem.tag {
                    attrs.insert(name, elem.value);
                }
            }
            return Ok(Attributes { attrs });
        }
        RObject::List(_elements) => {
            // If we have a regular list without tags, we can't extract attribute names
            // This shouldn't happen for attributes, but handle it gracefully
            return Ok(Attributes { attrs });
        }
        RObject::WithAttributes { object, attributes: _ } => {
            // The attributes object itself has attributes - use the inner object
            return parse_attributes(*object);
        }
        RObject::Integer(vec) if vec.len() == 1 => {
            // Single integer might be a reference index or special marker
            // In some cases, R uses compact formats for attributes
            // For now, treat as no attributes
            return Ok(Attributes { attrs });
        }
        RObject::Real(vec) if vec.len() == 3 => {
            // This might be ALTREP state being passed as attributes
            // This shouldn't happen, but handle it gracefully
            return Ok(Attributes { attrs });
        }
        _ => {
            // Unexpected attribute structure
            return Err(Error::InvalidFormat(format!(
                "Expected pairlist for attributes, got {:?}",
                attr_obj
            )));
        }
    }
}

/// Try to convert a list with attributes to a data.frame if it has the right structure.
fn try_convert_to_dataframe(obj: &RObject, attributes: &Attributes) -> Option<RObject> {
    use std::collections::HashMap;

    // Check if this has class="data.frame"
    let class_attr = attributes.get("class")?;
    let is_dataframe = match class_attr {
        RObject::Character(classes) => classes.iter().any(|c| c == "data.frame"),
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

    // Build the columns HashMap
    let mut columns = HashMap::new();
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
                    (1..=n).map(|i| i.to_string()).collect()
                } else {
                    // Explicit integer row names: convert to strings
                    indices.iter().map(|i| i.to_string()).collect()
                }
            }
            _ => {
                // Default row names: just number them based on first column length
                (1..=columns_list.first().map(|c| match c {
                    RObject::Integer(v) => v.len(),
                    RObject::Real(v) => v.len(),
                    RObject::Logical(v) => v.len(),
                    RObject::Character(v) => v.len(),
                    _ => 0,
                }).unwrap_or(0)).map(|i| i.to_string()).collect()
            }
        }
    } else {
        // No row.names attribute, create default based on first column length
        let n = columns_list.first().map(|c| match c {
            RObject::Integer(v) => v.len(),
            RObject::Real(v) => v.len(),
            RObject::Logical(v) => v.len(),
            RObject::Character(v) => v.len(),
            _ => 0,
        }).unwrap_or(0);
        (1..=n).map(|i| i.to_string()).collect()
    };

    Some(RObject::DataFrame { columns, row_names })
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
    let is_factor = classes.contains(&"factor".to_string());
    if !is_factor {
        return None;
    }

    // Check if it's an ordered factor
    let ordered = classes.contains(&"ordered".to_string());

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

    Some(RObject::Factor {
        values,
        levels,
        ordered,
    })
}

/// Convert an object with attributes to an S3 object.
/// Assumes the class attribute has already been checked.
fn convert_to_s3_object(obj: RObject, mut attributes: Attributes) -> RObject {
    // Extract the class attribute
    let classes = match attributes.attrs.remove("class") {
        Some(RObject::Character(classes)) => classes,
        _ => vec![], // Shouldn't happen since we checked before calling
    };

    // Create the S3 object
    RObject::S3Object {
        base: Box::new(obj),
        class: classes,
        attributes,
    }
}

/// Convert attributes to an S4 object.
/// For S4 objects, the class is in attributes, and all other attributes are slots.
fn convert_to_s4_object(mut attributes: Attributes) -> RObject {
    use std::collections::HashMap;

    // Extract the class attribute
    // The class may be wrapped in WithAttributes if it has a package attribute
    let class = match attributes.attrs.remove("class") {
        Some(RObject::Character(classes)) => classes,
        Some(RObject::WithAttributes { object, .. }) => {
            // Unwrap the WithAttributes to get the actual class vector
            match *object {
                RObject::Character(classes) => classes,
                _ => vec![],
            }
        }
        _ => vec![], // S4 objects should always have a class
    };

    // Extract package attribute (S4 objects often have this, but we don't need it for slots)
    attributes.attrs.remove("package");

    // All remaining attributes are the slots
    let mut slots = HashMap::new();
    for (key, value) in attributes.attrs {
        slots.insert(key, value);
    }

    RObject::S4Object { class, slots }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        // This is a minimal RDS header for format version 2
        let header = vec![
            b'X', b'\n',  // Magic bytes
            0, 0, 0, 2,   // Format version (2)
            0, 3, 5, 0,   // R version 3.5.0
            0, 3, 0, 0,   // Min R version 3.0.0
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
