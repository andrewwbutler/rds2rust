//! Parser for RDS files.

use crate::error::{Error, Result};
use crate::types::{Logical, RObject};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use std::io::{Cursor, Read};

/// SEXP type constants
const NILSXP: u32 = 0;
const SYMSXP: u32 = 1;
const LISTSXP: u32 = 2;
const CLOSXP: u32 = 3;
const ENVSXP: u32 = 4;
const PROMSXP: u32 = 5;
const CHARSXP: u32 = 9;
const LGLSXP: u32 = 10;
const INTSXP: u32 = 13;
const REALSXP: u32 = 14;
const STRSXP: u32 = 16;
const VECSXP: u32 = 19;

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
    // Read the flags as a big-endian u32
    let flags = cursor.read_u32::<BigEndian>()?;

    // Extract the SEXP type from the flags.
    // In theory, it should be in bits 0-7, but due to XDR encoding quirks,
    // it appears in different positions for different types:
    // - Regular types (INTSXP, LGLSXP, etc.): bits 8-15
    // - Special types (NILVALUE_SXP, etc.): bits 0-7
    // We check bits 8-15 first, and if that's 0, fall back to bits 0-7
    let type_from_8_15 = (flags >> 8) & 0xFF;
    let type_from_0_7 = flags & 0xFF;
    let sexp_type = if type_from_8_15 != 0 {
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
    let obj = match sexp_type {
        NILSXP | NILVALUE_SXP => RObject::Null,
        SYMSXP => parse_symbol(cursor)?,
        INTSXP => parse_integer_vector(cursor)?,
        REALSXP => parse_real_vector(cursor)?,
        LGLSXP => parse_logical_vector(cursor)?,
        STRSXP => parse_character_vector(cursor)?,
        VECSXP => parse_list(cursor)?,
        LISTSXP => parse_pairlist(cursor, has_tag)?,
        CHARSXP => {
            // Sometimes CHARSXP appears standalone (like for encoding markers)
            let string = parse_charsxp_content(cursor)?;
            // Return as a single-element character vector for now
            RObject::Character(vec![string])
        }
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
        // For now, skip attributes - we'll implement this later
        // Just parse and discard the attributes object
        let _attrs = parse_object(cursor)?;
        // TODO: Wrap object with attributes
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

/// Parse a symbol (SYMSXP).
fn parse_symbol(cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // A symbol consists of a CHARSXP for the name
    // For now, just parse it as a character vector
    let _name = parse_object(cursor)?;
    // Symbols can also have pname and value, but for encoding markers we only need the name
    // Return as NULL for now since we don't have a Symbol type yet
    Ok(RObject::Null)
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

/// Parse a pairlist (LISTSXP).
fn parse_pairlist(cursor: &mut Cursor<&[u8]>, has_tag: bool) -> Result<RObject> {
    // Pairlists are serialized as: [TAG if HAS_TAG_BIT], CAR, CDR
    // We'll convert to a regular list for simplicity
    let mut elements = Vec::new();

    // Parse the TAG if present (comes before CAR)
    let _tag = if has_tag {
        Some(parse_object(cursor)?)
    } else {
        None
    };

    // Parse the CAR (first element)
    let car = parse_object(cursor)?;

    // Parse the CDR (rest of list)
    let cdr = parse_object(cursor)?;

    // Add CAR to the list
    elements.push(car);

    // If CDR is another pairlist, recursively add its elements
    // If CDR is NULL, we're done
    match cdr {
        RObject::Null => {
            // End of list
        }
        RObject::List(mut rest) => {
            // Append the rest
            elements.append(&mut rest);
        }
        other => {
            // CDR is some other object, add it
            elements.push(other);
        }
    }

    Ok(RObject::List(elements))
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
