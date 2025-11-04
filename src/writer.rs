//! Writer for RDS files.

use crate::constants::*;
use crate::error::{Error, Result};
use crate::types::{Attributes, Complex, Logical, PairlistElement, RObject};
use byteorder::{BigEndian, WriteBytesExt};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

/// Reference table for tracking objects during serialization.
/// R's serialization uses reference tracking to avoid duplicating shared objects.
/// When the same object appears multiple times, the first occurrence is written normally,
/// and subsequent occurrences are written as REFSXP with an index pointing to the first.
///
/// Note: For now, this is a placeholder that doesn't actually perform deduplication.
/// True reference tracking requires tracking object identity across the entire object graph,
/// which is complex in Rust. This infrastructure is in place for future enhancement.
struct RefTable {
    /// Placeholder for future reference tracking
    #[allow(dead_code)]
    next_index: u32,
}

impl RefTable {
    fn new() -> Self {
        RefTable {
            next_index: 1, // R uses 1-based indexing for references
        }
    }
}

/// Determine if an object type should be tracked for references.
/// This matches the same logic used in the parser.
#[allow(dead_code)]
fn should_track_reference_type(obj: &RObject) -> bool {
    matches!(
        obj,
        RObject::List(_)
            | RObject::Expression(_)
            | RObject::Language(_)
            | RObject::Pairlist(_)
            | RObject::S4Object { .. }
            | RObject::WithAttributes { .. }
    )
}

/// Write an RObject to RDS format.
/// Returns the serialized bytes (gzip compressed).
pub fn write_rds(obj: &RObject) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();

    // Write header
    write_header(&mut buffer)?;

    // Create reference table for tracking shared objects
    let mut ref_table = RefTable::new();

    // Write the object
    write_object(&mut buffer, obj, &mut ref_table)?;

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
fn write_object(writer: &mut Vec<u8>, obj: &RObject, ref_table: &mut RefTable) -> Result<()> {
    match obj {
        RObject::Null => write_null(writer),
        RObject::Integer(vec) => write_integer_vector(writer, vec),
        RObject::Real(vec) => write_real_vector(writer, vec),
        RObject::Logical(vec) => write_logical_vector(writer, vec),
        RObject::Character(vec) => write_character_vector(writer, vec.as_slice()),
        RObject::Raw(vec) => write_raw_vector(writer, vec),
        RObject::Complex(vec) => write_complex_vector(writer, vec),
        RObject::List(elements) => write_list(writer, elements, ref_table),
        RObject::Expression(elements) => write_expression(writer, elements, ref_table),
        RObject::Pairlist(elements) => write_pairlist(writer, elements, ref_table),
        RObject::Language(elements) => write_language(writer, elements, ref_table),
        RObject::Closure { formals, body, environment } => {
            write_closure(writer, formals, body, environment, ref_table)
        }
        RObject::Environment { enclosing, frame, hashtab } => {
            write_environment(writer, enclosing, frame, hashtab, ref_table)
        }
        RObject::Promise { value, expression, environment } => {
            write_promise(writer, value, expression, environment, ref_table)
        }
        RObject::Special { name } => write_special(writer, name.as_ref()),
        RObject::Builtin { name } => write_builtin(writer, name.as_ref()),
        RObject::Bytecode { code, constants, expr } => {
            write_bytecode(writer, code, constants, expr, ref_table)
        }
        RObject::DataFrame(data) => {
            write_dataframe(writer, &data.columns, &data.row_names, ref_table)
        }
        RObject::Factor(data) => {
            write_factor(writer, &data.values, &data.levels, data.ordered, ref_table)
        }
        RObject::S3Object(data) => {
            write_s3_object(writer, &data.base, &data.class, &data.attributes, ref_table)
        }
        RObject::S4Object(data) => write_s4_object(writer, &data.class, &data.slots, ref_table),
        RObject::WithAttributes { object, attributes } => {
            write_object_with_attributes(writer, object, attributes, ref_table)
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
fn write_character_vector(writer: &mut Vec<u8>, vec: &[Arc<str>]) -> Result<()> {
    write_flags(writer, STRSXP, false, false)?;
    writer.write_u32::<BigEndian>(vec.len() as u32)?;
    for s in vec {
        write_charsxp(writer, s.as_ref())?;
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
fn write_list(writer: &mut Vec<u8>, elements: &[RObject], ref_table: &mut RefTable) -> Result<()> {
    write_flags(writer, VECSXP, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table)?;
    }
    Ok(())
}

/// Write an expression vector (EXPRSXP).
/// Expression vectors are structurally identical to VECSXP, but semantically represent
/// collections of unevaluated expressions (typically language objects).
fn write_expression(writer: &mut Vec<u8>, elements: &[RObject], ref_table: &mut RefTable) -> Result<()> {
    write_flags(writer, EXPRSXP, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table)?;
    }
    Ok(())
}

/// Write a language object (LANGSXP).
/// Language objects represent unevaluated calls: function + arguments.
fn write_language(writer: &mut Vec<u8>, elements: &[RObject], ref_table: &mut RefTable) -> Result<()> {
    if elements.is_empty() {
        // Empty language object? Just write NULL
        return write_null(writer);
    }

    // Language objects are structured as: CAR (function) + CDR (argument list)
    let has_tag = false; // Language objects typically don't have tags

    write_flags(writer, LANGSXP, false, has_tag)?;

    // Write the function (CAR)
    write_object(writer, &elements[0], ref_table)?;

    // Write the arguments (CDR) as a pairlist or NULL
    if elements.len() > 1 {
        // Convert remaining elements to a pairlist
        let args: Vec<PairlistElement> = elements[1..]
            .iter()
            .map(|obj| PairlistElement {
                tag: None,
                value: obj.clone(),
                tag_object: None,
            })
            .collect();
        write_pairlist(writer, &args, ref_table)?;
    } else {
        // No arguments
        write_null(writer)?;
    }

    Ok(())
}

/// Write a pairlist (LISTSXP).
fn write_pairlist(writer: &mut Vec<u8>, elements: &[PairlistElement], ref_table: &mut RefTable) -> Result<()> {
    for (i, element) in elements.iter().enumerate() {
        let has_tag = element.tag.is_some();
        let is_last = i == elements.len() - 1;

        write_flags(writer, LISTSXP, false, has_tag)?;

        // Write the tag if present
        if let Some(ref tag) = element.tag {
            write_symbol(writer, tag)?;
        }

        // Write the value
        write_object(writer, &element.value, ref_table)?;

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

/// Write a closure (CLOSXP).
fn write_closure(
    writer: &mut Vec<u8>,
    formals: &RObject,
    body: &RObject,
    environment: &RObject,
    ref_table: &mut RefTable,
) -> Result<()> {
    write_flags(writer, CLOSXP, false, false)?;

    // Write formals (parameter list)
    write_object(writer, formals, ref_table)?;

    // Write body (function body)
    write_object(writer, body, ref_table)?;

    // Write environment (closure environment)
    write_object(writer, environment, ref_table)?;

    Ok(())
}

/// Write an environment (ENVSXP).
fn write_environment(
    writer: &mut Vec<u8>,
    enclosing: &RObject,
    frame: &RObject,
    hashtab: &RObject,
    ref_table: &mut RefTable,
) -> Result<()> {
    write_flags(writer, ENVSXP, false, false)?;

    // Write locked flag (0 = unlocked)
    write_integer_vector(writer, &[0])?;

    // Write enclosing environment
    write_object(writer, enclosing, ref_table)?;

    // Write frame (bindings pairlist)
    write_object(writer, frame, ref_table)?;

    // Write hashtab
    write_object(writer, hashtab, ref_table)?;

    Ok(())
}

/// Write a promise (PROMSXP).
fn write_promise(
    writer: &mut Vec<u8>,
    value: &RObject,
    expression: &RObject,
    environment: &RObject,
    ref_table: &mut RefTable,
) -> Result<()> {
    write_flags(writer, PROMSXP, false, false)?;

    // Write the three components: value, expression, environment
    write_object(writer, value, ref_table)?;
    write_object(writer, expression, ref_table)?;
    write_object(writer, environment, ref_table)?;

    Ok(())
}

/// Write a special primitive function (SPECIALSXP).
/// Format: type flag, then length (i32), then name bytes (no SYMSXP wrapper)
fn write_special(writer: &mut Vec<u8>, name: &str) -> Result<()> {
    write_flags(writer, SPECIALSXP, false, false)?;
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
    write_flags(writer, BUILTINSXP, false, false)?;
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
    expr: &RObject,
    ref_table: &mut RefTable,
) -> Result<()> {
    write_flags(writer, BCODESXP, false, false)?;
    // Write the three components
    write_object(writer, code, ref_table)?;
    write_object(writer, constants, ref_table)?;
    write_object(writer, expr, ref_table)?;
    Ok(())
}

/// Write a data frame.
fn write_dataframe(
    writer: &mut Vec<u8>,
    columns: &HashMap<Arc<str>, RObject>,
    row_names: &[Arc<str>],
    ref_table: &mut RefTable,
) -> Result<()> {
    // Convert HashMap to Vec for consistent ordering
    let mut cols_vec: Vec<_> = columns.iter().collect();
    cols_vec.sort_by(|(a, _), (b, _)| a.as_ref().cmp(b.as_ref()));

    let column_names: Vec<Arc<str>> = cols_vec.iter().map(|(name, _)| (*name).clone()).collect();
    let column_values: Vec<&RObject> = cols_vec.iter().map(|(_, obj)| *obj).collect();

    // Write as a list with attributes
    write_flags(writer, VECSXP, true, false)?;
    writer.write_u32::<BigEndian>(column_values.len() as u32)?;

    // Write each column
    for col in &column_values {
        write_object(writer, col, ref_table)?;
    }

    // Write attributes (names, row.names, class)
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), RObject::Character(column_names));
    attrs.insert(Arc::from("row.names"), RObject::Character(row_names.to_vec()));
    attrs.insert(Arc::from("class"), RObject::Character(vec![Arc::from("data.frame")]));

    write_attributes(writer, &attrs, ref_table)?;

    Ok(())
}

/// Write a factor.
fn write_factor(
    writer: &mut Vec<u8>,
    values: &[i32],
    levels: &[Arc<str>],
    ordered: bool,
    ref_table: &mut RefTable,
) -> Result<()> {
    // Write the integer vector with attributes
    write_flags(writer, INTSXP, true, false)?;
    writer.write_u32::<BigEndian>(values.len() as u32)?;
    for &val in values {
        writer.write_i32::<BigEndian>(val)?;
    }

    // Write attributes (levels and class)
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("levels"), RObject::Character(levels.to_vec()));

    let class = if ordered {
        vec![Arc::from("ordered"), Arc::from("factor")]
    } else {
        vec![Arc::from("factor")]
    };
    attrs.insert(Arc::from("class"), RObject::Character(class));

    write_attributes(writer, &attrs, ref_table)?;

    Ok(())
}

/// Write an S3 object.
fn write_s3_object(
    writer: &mut Vec<u8>,
    base: &RObject,
    class: &[Arc<str>],
    attributes: &Attributes,
    ref_table: &mut RefTable,
) -> Result<()> {
    // Write the base object with attributes
    match base {
        RObject::List(elements) => {
            write_flags(writer, VECSXP, true, false)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element, ref_table)?;
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
        RObject::Language(elements) => {
            // Language objects with attributes (e.g., formulas)
            // Attributes come BEFORE CAR/CDR for language objects
            let has_tag = false;
            write_flags(writer, LANGSXP, true, has_tag)?;

            // Write attributes FIRST (before CAR/CDR)
            let mut attrs = attributes.clone();
            attrs.insert(Arc::from("class"), RObject::Character(class.to_vec()));
            write_attributes(writer, &attrs, ref_table)?;

            // Now write the CAR/CDR
            if elements.is_empty() {
                return write_null(writer);
            }

            // Write the function (CAR)
            write_object(writer, &elements[0], ref_table)?;

            // Write the arguments (CDR) as a pairlist or NULL
            if elements.len() > 1 {
                let args: Vec<PairlistElement> = elements[1..]
                    .iter()
                    .map(|obj| PairlistElement {
                        tag: None,
                        value: obj.clone(),
                        tag_object: None,
                    })
                    .collect();
                write_pairlist(writer, &args, ref_table)?;
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
    attrs.insert(Arc::from("class"), RObject::Character(class.to_vec()));
    write_attributes(writer, &attrs, ref_table)?;

    Ok(())
}

/// Write an S4 object.
fn write_s4_object(writer: &mut Vec<u8>, class: &[Arc<str>], slots: &HashMap<Arc<str>, RObject>, ref_table: &mut RefTable) -> Result<()> {
    // S4 objects are written as S4SXP with attributes
    write_flags(writer, S4SXP, true, false)?;

    // Write attributes (class + slots)
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("class"), RObject::Character(class.to_vec()));

    // Add all slots as attributes
    for (name, value) in slots {
        attrs.insert(name.clone(), value.clone());
    }

    write_attributes(writer, &attrs, ref_table)?;

    Ok(())
}

/// Write an object with attributes.
fn write_object_with_attributes(
    writer: &mut Vec<u8>,
    object: &RObject,
    attributes: &Attributes,
    ref_table: &mut RefTable,
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
                write_object(writer, element, ref_table)?;
            }
        }
        _ => {
            return Err(Error::Unsupported(
                "Unsupported type for WithAttributes writing".to_string(),
            ));
        }
    }

    write_attributes(writer, attributes, ref_table)?;

    Ok(())
}

/// Write attributes as a pairlist.
fn write_attributes(writer: &mut Vec<u8>, attributes: &Attributes, ref_table: &mut RefTable) -> Result<()> {
    if attributes.is_empty() {
        return Ok(());
    }

    // Convert to pairlist elements
    let mut elements = Vec::new();

    // Sort keys for consistent output (sort by string content)
    let mut sorted_attrs: Vec<_> = attributes.attrs.iter().collect();
    sorted_attrs.sort_by_key(|(k, _)| k.as_ref());

    for (key, value) in sorted_attrs {
        elements.push(PairlistElement {
            tag: Some(key.clone()),
            value: (**value).clone(),  // Unbox the RObject
            tag_object: None,
        });
    }

    // Write the pairlist
    write_pairlist(writer, &elements, ref_table)?;

    Ok(())
}
