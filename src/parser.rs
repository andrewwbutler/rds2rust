//! Parser for RDS files.

use crate::error::{Error, Result};
use crate::types::{Attributes, Logical, PairlistElement, RObject};
use byteorder::{BigEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// SEXP type constants
const NILSXP: u32 = 0;
const SYMSXP: u32 = 1;
const LISTSXP: u32 = 2;
const CLOSXP: u32 = 3;
const ENVSXP: u32 = 4;
const CHARSXP: u32 = 9;
const LGLSXP: u32 = 10;
const INTSXP: u32 = 13;
const REALSXP: u32 = 14;
const CPLXSXP: u32 = 15;
const STRSXP: u32 = 16;
const VECSXP: u32 = 19;
const RAWSXP: u32 = 24;

/// Special pseudo-types
const ALTREP_SXP: u32 = 238; // 0xEE - ALTREP object (version 3)
const REFSXP: u32 = 255; // 0xFF - Reference to an already seen object
const NILVALUE_SXP: u32 = 254; // 0xFE - Singleton NULL value

/// Flag bit masks
const HAS_ATTR_BIT: u32 = 1 << 9;
const HAS_TAG_BIT: u32 = 1 << 10;

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

    // Parse the actual object
    parse_object(&mut cursor)
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
fn parse_object(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
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
        // TODO: Handle GLOBALENV_SXP, UNBOUNDVALUE_SXP, MISSINGARG_SXP properly
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
    let has_attr = (flags & HAS_ATTR_BIT) != 0;
    let has_tag = (flags & HAS_TAG_BIT) != 0;


    // Parse the object based on type
    let mut obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        SYMSXP => parse_symbol(cursor)?,
        INTSXP => parse_integer_vector(cursor)?,
        REALSXP => parse_real_vector(cursor)?,
        CPLXSXP => parse_complex_vector(cursor)?,
        LGLSXP => parse_logical_vector(cursor)?,
        STRSXP => parse_character_vector(cursor)?,
        RAWSXP => parse_raw_vector(cursor)?,
        VECSXP => parse_list(cursor)?,
        LISTSXP => parse_pairlist(cursor, has_tag)?,
        CHARSXP => {
            // Sometimes CHARSXP appears standalone (like for encoding markers)
            let string = parse_charsxp_content(cursor)?;
            // Return as a single-element character vector for now
            RObject::Character(vec![string])
        }
        CLOSXP => parse_closure(cursor)?,
        ENVSXP => parse_environment(cursor)?,
        REFSXP => {
            // Reference to a previously seen object
            // For now, return an error as we haven't implemented reference tracking yet
            return Err(Error::Unsupported(
                "Reference tracking not yet implemented".to_string(),
            ));
        }
        ALTREP_SXP => {
            // ALTREP object (version 3 feature)
            // Structure: class_info, state, attributes
            parse_altrep(cursor)?
        }
        _ => {
            return Err(Error::UnknownSexpType(sexp_type));
        }
    };

    // Parse attributes if present
    if has_attr {
        let attr_obj = parse_object(cursor)?;
        let attributes = parse_attributes(attr_obj)?;

        if !attributes.is_empty() {
            // Check if this has a class attribute
            let has_class = attributes.get("class").is_some();

            if has_class {
                // Check if this is a data.frame (special S3 object)
                if let Some(dataframe) = try_convert_to_dataframe(&obj, &attributes) {
                    obj = dataframe;
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
fn parse_character_vector(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
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

/// Parse a symbol (SYMSXP).
fn parse_symbol(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // A symbol consists of a CHARSXP for the name
    let name_obj = parse_object(cursor)?;

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
fn parse_list(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    let length = cursor.read_u32::<BigEndian>()? as usize;
    let mut elements = Vec::with_capacity(length);

    for _ in 0..length {
        let element = parse_object(cursor)?;
        elements.push(element);
    }

    Ok(RObject::List(elements))
}

/// Parse a closure (CLOSXP).
/// Closures consist of: formals (pairlist), body (any SEXP), environment (ENVSXP)
fn parse_closure(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // Parse formals (arguments)
    let _formals = parse_object(cursor)?;
    // Parse body
    let _body = parse_object(cursor)?;
    // Parse environment
    let _env = parse_object(cursor)?;

    // For now, return NULL as we don't have a Closure type yet
    Ok(RObject::Null)
}

/// Parse an environment (ENVSXP).
/// Environments consist of: locked flag, enclosing environment, frame (pairlist), hashtab
fn parse_environment(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // Parse locked flag (an integer: 0 or 1)
    let _locked = parse_object(cursor)?;
    // Parse enclosing environment (can be another environment or NULL for global env)
    let _enclos = parse_object(cursor)?;
    // Parse frame (pairlist of bindings)
    let _frame = parse_object(cursor)?;
    // Parse hashtab (can be NULL or a VECSXP)
    let _hashtab = parse_object(cursor)?;

    // Note: attributes are NOT parsed here - they're handled by the HAS_ATTR flag
    // in parse_object

    // For now, return NULL as we don't have an Environment type yet
    Ok(RObject::Null)
}

/// Parse a pairlist (LISTSXP).
fn parse_pairlist(cursor: &mut Cursor<&[u8]>, has_tag: bool) -> Result<RObject> {
    // Pairlists are serialized as: [TAG if HAS_TAG_BIT], CAR, CDR
    let mut elements = Vec::new();

    // Parse the TAG if present (comes before CAR)
    let tag = if has_tag {
        let tag_obj = parse_object(cursor)?;
        // Extract the tag name from the symbol or character object
        extract_tag_name(tag_obj)
    } else {
        None
    };

    // Parse the CAR (first element)
    let car = parse_object(cursor)?;

    // Parse the CDR (rest of list)
    let cdr = parse_object(cursor)?;

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

/// Parse an ALTREP object.
fn parse_altrep(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // ALTREP structure: class_info, state, attributes
    let class_info = parse_object(cursor)?;
    let state = parse_object(cursor)?;
    let _attributes = parse_object(cursor)?;

    // Try to convert ALTREP to a native representation
    // For now, we'll handle compact_intseq specifically
    convert_altrep_to_native(class_info, state)
}

/// Convert an ALTREP object to its native representation.
fn convert_altrep_to_native(_class_info: RObject, state: RObject) -> Result<RObject> {
    // Infer the ALTREP type from the state structure
    // compact_intseq has state: [length (real), first (real), stride (real)]
    match &state {
        RObject::Real(params) if params.len() == 3 => {
            // Likely a compact_intseq
            convert_compact_intseq(state)
        }
        _ => Err(Error::Unsupported(format!(
            "Unknown ALTREP state structure: {:?}",
            state
        ))),
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
