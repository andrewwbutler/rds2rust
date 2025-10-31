//! Writer for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::types::{Attributes, Complex, Logical, PairlistElement, RObject};
use byteorder::{BigEndian, WriteBytesExt};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::io::Write;

/// Write an RObject to RDS format.
/// Returns the serialized bytes (gzip compressed).
pub fn write_rds(obj: &RObject) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();

    // Write header
    write_header(&mut buffer)?;

    // Write the object
    write_object(&mut buffer, obj)?;

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

    // Format version: 2 (most common)
    writer.write_u32::<BigEndian>(2)?;

    // R version that wrote the file: 4.3.0 (0x00040300)
    writer.write_u32::<BigEndian>(0x00040300)?;

    // Minimum R version needed: 3.5.0 (0x00030500)
    writer.write_u32::<BigEndian>(0x00030500)?;

    Ok(())
}

/// Write an R object to the stream.
fn write_object(writer: &mut Vec<u8>, obj: &RObject) -> Result<()> {
    match obj {
        RObject::Null => write_null(writer),
        RObject::Integer(vec) => write_integer_vector(writer, vec),
        RObject::Real(vec) => write_real_vector(writer, vec),
        RObject::Logical(vec) => write_logical_vector(writer, vec),
        RObject::Character(vec) => write_character_vector(writer, vec),
        RObject::Raw(vec) => write_raw_vector(writer, vec),
        RObject::Complex(vec) => write_complex_vector(writer, vec),
        RObject::List(elements) => write_list(writer, elements),
        RObject::Pairlist(elements) => write_pairlist(writer, elements),
        RObject::DataFrame { columns, row_names } => {
            write_dataframe(writer, columns, row_names)
        }
        RObject::Factor { values, levels, ordered } => {
            write_factor(writer, values, levels, *ordered)
        }
        RObject::S3Object { base, class, attributes } => {
            write_s3_object(writer, base, class, attributes)
        }
        RObject::S4Object { class, slots } => write_s4_object(writer, class, slots),
        RObject::WithAttributes { object, attributes } => {
            write_object_with_attributes(writer, object, attributes)
        }
    }
}

/// Write NULL.
fn write_null(writer: &mut Vec<u8>) -> Result<()> {
    // Use NILVALUE_SXP (254) for singleton NULL
    write_flags(writer, NILVALUE_SXP, false, false)?;
    Ok(())
}

/// Write flags (type + attribute/tag bits).
fn write_flags(writer: &mut Vec<u8>, sexp_type: u32, has_attr: bool, has_tag: bool) -> Result<()> {
    let mut flags = sexp_type;
    if has_attr {
        flags |= HAS_ATTR_BIT;
    }
    if has_tag {
        flags |= HAS_TAG_BIT;
    }
    writer.write_u32::<BigEndian>(flags)?;
    Ok(())
}

/// Write an integer vector.
fn write_integer_vector(writer: &mut Vec<u8>, vec: &[i32]) -> Result<()> {
    write_flags(writer, INTSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for &val in vec {
        writer.write_i32::<BigEndian>(val)?;
    }
    Ok(())
}

/// Write a real (double) vector.
fn write_real_vector(writer: &mut Vec<u8>, vec: &[f64]) -> Result<()> {
    write_flags(writer, REALSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for &val in vec {
        writer.write_f64::<BigEndian>(val)?;
    }
    Ok(())
}

/// Write a logical vector.
fn write_logical_vector(writer: &mut Vec<u8>, vec: &[Logical]) -> Result<()> {
    write_flags(writer, LGLSXP, false, false)?;
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
fn write_character_vector(writer: &mut Vec<u8>, vec: &[String]) -> Result<()> {
    write_flags(writer, STRSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for s in vec {
        write_charsxp(writer, s)?;
    }
    Ok(())
}

/// Write a CHARSXP (internal string).
fn write_charsxp(writer: &mut Vec<u8>, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    write_flags(writer, CHARSXP, false, false)?;
    writer.write_u32::<BigEndian>(bytes.len() as u32)?;
    writer.write_all(bytes)?;
    Ok(())
}

/// Write a raw vector.
fn write_raw_vector(writer: &mut Vec<u8>, vec: &[u8]) -> Result<()> {
    write_flags(writer, RAWSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    writer.write_all(vec)?;
    Ok(())
}

/// Write a complex vector.
fn write_complex_vector(writer: &mut Vec<u8>, vec: &[Complex]) -> Result<()> {
    write_flags(writer, CPLXSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for complex in vec {
        writer.write_f64::<BigEndian>(complex.real)?;
        writer.write_f64::<BigEndian>(complex.imaginary)?;
    }
    Ok(())
}

/// Write a list (VECSXP).
fn write_list(writer: &mut Vec<u8>, elements: &[RObject]) -> Result<()> {
    write_flags(writer, VECSXP, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element)?;
    }
    Ok(())
}

/// Write a pairlist (LISTSXP).
fn write_pairlist(writer: &mut Vec<u8>, elements: &[PairlistElement]) -> Result<()> {
    for (i, element) in elements.iter().enumerate() {
        let has_tag = element.tag.is_some();
        let is_last = i == elements.len() - 1;

        write_flags(writer, LISTSXP, false, has_tag)?;

        // Write the tag if present
        if let Some(ref tag) = element.tag {
            write_symbol(writer, tag)?;
        }

        // Write the value
        write_object(writer, &element.value)?;

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

/// Write a symbol (SYMSXP).
fn write_symbol(writer: &mut Vec<u8>, name: &str) -> Result<()> {
    write_flags(writer, SYMSXP, false, false)?;
    write_charsxp(writer, name)?;
    Ok(())
}

/// Write a data frame.
fn write_dataframe(
    writer: &mut Vec<u8>,
    columns: &HashMap<String, RObject>,
    row_names: &[String],
) -> Result<()> {
    // Convert HashMap to Vec for consistent ordering
    let mut cols_vec: Vec<_> = columns.iter().collect();
    cols_vec.sort_by_key(|(name, _)| *name);

    let column_names: Vec<String> = cols_vec.iter().map(|(name, _)| (*name).clone()).collect();
    let column_values: Vec<&RObject> = cols_vec.iter().map(|(_, obj)| *obj).collect();

    // Write as a list with attributes
    write_flags(writer, VECSXP, true, false)?;
    writer.write_u32::<BigEndian>(column_values.len() as u32)?;

    // Write each column
    for col in &column_values {
        write_object(writer, col)?;
    }

    // Write attributes (names, row.names, class)
    let mut attrs = Attributes::new();
    attrs.insert("names".to_string(), RObject::Character(column_names));
    attrs.insert("row.names".to_string(), RObject::Character(row_names.to_vec()));
    attrs.insert("class".to_string(), RObject::Character(vec!["data.frame".to_string()]));

    write_attributes(writer, &attrs)?;

    Ok(())
}

/// Write a factor.
fn write_factor(
    writer: &mut Vec<u8>,
    values: &[i32],
    levels: &[String],
    ordered: bool,
) -> Result<()> {
    // Write the integer vector with attributes
    write_flags(writer, INTSXP, true, false)?;
    writer.write_u32::<BigEndian>(values.len() as u32)?;
    for &val in values {
        writer.write_i32::<BigEndian>(val)?;
    }

    // Write attributes (levels and class)
    let mut attrs = Attributes::new();
    attrs.insert("levels".to_string(), RObject::Character(levels.to_vec()));

    let class = if ordered {
        vec!["ordered".to_string(), "factor".to_string()]
    } else {
        vec!["factor".to_string()]
    };
    attrs.insert("class".to_string(), RObject::Character(class));

    write_attributes(writer, &attrs)?;

    Ok(())
}

/// Write an S3 object.
fn write_s3_object(
    writer: &mut Vec<u8>,
    base: &RObject,
    class: &[String],
    attributes: &Attributes,
) -> Result<()> {
    // Write the base object with attributes
    match base {
        RObject::List(elements) => {
            write_flags(writer, VECSXP, true, false)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element)?;
            }
        }
        RObject::Integer(vec) => {
            write_flags(writer, INTSXP, true, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_i32::<BigEndian>(*val)?;
            }
        }
        RObject::Real(vec) => {
            write_flags(writer, REALSXP, true, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_f64::<BigEndian>(*val)?;
            }
        }
        _ => {
            return Err(Error::Unsupported(
                "Unsupported S3 base type for writing".to_string(),
            ));
        }
    }

    // Write attributes with class added
    let mut attrs = attributes.clone();
    attrs.insert("class".to_string(), RObject::Character(class.to_vec()));
    write_attributes(writer, &attrs)?;

    Ok(())
}

/// Write an S4 object.
fn write_s4_object(writer: &mut Vec<u8>, class: &[String], slots: &HashMap<String, RObject>) -> Result<()> {
    // S4 objects are written as S4SXP with attributes
    write_flags(writer, S4SXP, true, false)?;

    // Write attributes (class + slots)
    let mut attrs = Attributes::new();
    attrs.insert("class".to_string(), RObject::Character(class.to_vec()));

    // Add all slots as attributes
    for (name, value) in slots {
        attrs.insert(name.clone(), value.clone());
    }

    write_attributes(writer, &attrs)?;

    Ok(())
}

/// Write an object with attributes.
fn write_object_with_attributes(
    writer: &mut Vec<u8>,
    object: &RObject,
    attributes: &Attributes,
) -> Result<()> {
    // Write the base object with HAS_ATTR flag set
    match object {
        RObject::Integer(vec) => {
            write_flags(writer, INTSXP, true, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_i32::<BigEndian>(*val)?;
            }
        }
        RObject::Real(vec) => {
            write_flags(writer, REALSXP, true, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for val in vec {
                writer.write_f64::<BigEndian>(*val)?;
            }
        }
        RObject::List(elements) => {
            write_flags(writer, VECSXP, true, false)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element)?;
            }
        }
        _ => {
            return Err(Error::Unsupported(
                "Unsupported type for WithAttributes writing".to_string(),
            ));
        }
    }

    write_attributes(writer, attributes)?;

    Ok(())
}

/// Write attributes as a pairlist.
fn write_attributes(writer: &mut Vec<u8>, attributes: &Attributes) -> Result<()> {
    if attributes.is_empty() {
        return Ok(());
    }

    // Convert to pairlist elements
    let mut elements = Vec::new();

    // Sort keys for consistent output
    let mut keys: Vec<_> = attributes.attrs.keys().collect();
    keys.sort();

    for key in keys {
        if let Some(value) = attributes.attrs.get(key) {
            elements.push(PairlistElement {
                tag: Some(key.clone()),
                value: value.clone(),
            });
        }
    }

    // Write the pairlist
    write_pairlist(writer, &elements)?;

    Ok(())
}
