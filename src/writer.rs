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
struct RefTable {
    /// Next reference index to assign
    next_index: u32,
    /// Map from namespace name to reference index
    namespace_refs: HashMap<String, u32>,
    /// Map from symbol name to reference index
    symbol_refs: HashMap<String, u32>,
}

impl RefTable {
    fn new() -> Self {
        RefTable {
            next_index: 1, // R uses 1-based indexing for references
            namespace_refs: HashMap::new(),
            symbol_refs: HashMap::new(),
        }
    }

    /// Check if a namespace has been written before, returning its reference index if so.
    /// Otherwise, register it and return None.
    fn check_namespace(&mut self, names: &[Arc<str>]) -> Option<u32> {
        let key = names
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        if let Some(&ref_idx) = self.namespace_refs.get(&key) {
            Some(ref_idx)
        } else {
            let idx = self.next_index;
            self.next_index += 1;
            self.namespace_refs.insert(key, idx);
            None
        }
    }

    /// Check if a symbol has been written before, returning its reference index if so.
    /// Otherwise, register it and return None.
    fn check_symbol(&mut self, name: &str) -> Option<u32> {
        if let Some(&ref_idx) = self.symbol_refs.get(name) {
            Some(ref_idx)
        } else {
            let idx = self.next_index;
            self.next_index += 1;
            self.symbol_refs.insert(name.to_string(), idx);
            None
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
            | RObject::Language { .. }
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
fn write_object(writer: &mut Vec<u8>, obj: &RObject, ref_table: &mut RefTable) -> Result<()> {
    match obj {
        RObject::Null => write_null(writer),
        RObject::Integer(vec) => write_integer_vector(writer, vec),
        RObject::Real(vec) => write_real_vector(writer, vec),
        RObject::Logical(vec) => write_logical_vector(writer, vec),
        RObject::Character(vec) => {
            // Character vectors are always written as STRSXP
            // Symbols (SYMSXP) are only written in specific contexts like pairlist tags
            // or Language function positions, not here
            write_character_vector(writer, vec.as_slice())
        }
        RObject::Raw(vec) => write_raw_vector(writer, vec),
        RObject::Complex(vec) => write_complex_vector(writer, vec),
        RObject::List(elements) => write_list(writer, elements, ref_table),
        RObject::Expression(elements) => write_expression(writer, elements, ref_table),
        RObject::Pairlist(elements) => write_pairlist(writer, elements, ref_table),
        RObject::Language { function, args } => write_language(writer, function, args, ref_table),
        RObject::Closure {
            formals,
            body,
            environment,
        } => write_closure(writer, formals, body, environment, ref_table),
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => write_environment(writer, enclosing, frame, hashtab, ref_table),
        RObject::Promise {
            value,
            expression,
            environment,
        } => write_promise(writer, value, expression, environment, ref_table),
        RObject::Special { name } => write_special(writer, name.as_ref()),
        RObject::Builtin { name } => write_builtin(writer, name.as_ref()),
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => write_bytecode(writer, code, constants, expr, ref_table),
        RObject::DataFrame(data) => {
            write_dataframe(writer, &data.columns, &data.row_names, ref_table)
        }
        RObject::Factor(data) => {
            write_factor(writer, &data.values, &data.levels, data.ordered, ref_table)
        }
        RObject::S3Object(data) => {
            write_s3_object(writer, &data.base, &data.class, &data.attributes, ref_table)
        }
        RObject::S4Object(data) => write_s4_object(
            writer,
            &data.class,
            data.package.as_ref(),
            &data.slots,
            ref_table,
        ),
        RObject::Namespace(names) => write_namespace(writer, names, ref_table),
        RObject::GlobalEnv => write_global_env(writer),
        RObject::BaseEnv => write_base_env(writer),
        RObject::EmptyEnv => write_empty_env(writer),
        RObject::MissingArg => write_missing_arg(writer),
        RObject::UnboundValue => write_unbound_value(writer),
        RObject::WithAttributes { object, attributes } => {
            write_object_with_attributes(writer, object, attributes, ref_table)
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
) -> Result<()> {
    // Check if this namespace was already written
    if let Some(ref_idx) = ref_table.check_namespace(names) {
        // Write a reference to the previous occurrence
        write_refsxp(writer, ref_idx)?;
        return Ok(());
    }

    // First occurrence - write the full namespace
    // Use NAMESPACESXP_SERIAL (249) not NAMESPACESXP (123) for serialization
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
fn write_list(writer: &mut Vec<u8>, elements: &[RObject], ref_table: &mut RefTable) -> Result<()> {
    write_flags(writer, VECSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table)?;
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
) -> Result<()> {
    write_flags(writer, EXPRSXP, false, false, false)?;
    writer.write_u32::<BigEndian>(elements.len() as u32)?;
    for element in elements {
        write_object(writer, element, ref_table)?;
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
) -> Result<()> {
    // Language objects are structured as: CAR (function) + CDR (argument list)
    let has_tag = false; // Language objects typically don't have tags

    write_flags(writer, LANGSXP, false, has_tag, false)?;

    // Write the function (CAR)
    // If it's a single-element Character, write it as a symbol (function name)
    match function {
        RObject::Character(vec) if vec.len() == 1 => {
            write_symbol_with_ref(writer, &vec[0], ref_table)?;
        }
        _ => {
            write_object(writer, function, ref_table)?;
        }
    }

    // Write the arguments (CDR) as a pairlist or NULL
    if !args.is_empty() {
        write_pairlist_as_args(writer, args, ref_table)?;
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
    values_are_symbols: bool,
) -> Result<()> {
    for (i, element) in elements.iter().enumerate() {
        let has_tag = element.tag.is_some();
        let is_last = i == elements.len() - 1;

        write_flags(writer, LISTSXP, false, has_tag, false)?;

        // Write the tag if present
        if let Some(ref tag) = element.tag {
            write_symbol_with_ref(writer, tag, ref_table)?;
        }

        // Write the value
        // If values_are_symbols and value is single-element Character, write as symbol
        if values_are_symbols {
            match &element.value {
                RObject::Character(vec) if vec.len() == 1 => {
                    write_symbol_with_ref(writer, &vec[0], ref_table)?;
                }
                _ => {
                    write_object(writer, &element.value, ref_table)?;
                }
            }
        } else {
            write_object(writer, &element.value, ref_table)?;
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
) -> Result<()> {
    // For general pairlists (like formals), don't convert values to symbols
    write_pairlist_internal(writer, elements, ref_table, false)
}

/// Write a pairlist for Language arguments where single-element Characters are symbols.
fn write_pairlist_as_args(
    writer: &mut Vec<u8>,
    elements: &[PairlistElement],
    ref_table: &mut RefTable,
) -> Result<()> {
    // For Language arguments, convert single-element Character values to symbols
    write_pairlist_internal(writer, elements, ref_table, true)
}

/// Write a symbol (SYMSXP) with reference tracking.
fn write_symbol_with_ref(
    writer: &mut Vec<u8>,
    name: &str,
    ref_table: &mut RefTable,
) -> Result<()> {
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
) -> Result<()> {
    // R sets HAS_TAG for CLOSXP in most cases (for srcref tracking)
    // We match R's behavior by always setting has_tag=true
    write_flags(writer, CLOSXP, false, true, false)?;

    // Write environment (closure environment)
    write_object(writer, environment, ref_table)?;

    // Write formals (parameter list)
    write_object(writer, formals, ref_table)?;

    // Write body (function body)
    write_object(writer, body, ref_table)?;

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
    write_flags(writer, ENVSXP, false, false, false)?;

    // Write locked flag (0 = unlocked)
    write_integer_vector(writer, &[0])?;

    // Write enclosing environment
    write_object(writer, enclosing, ref_table)?;

    // Write frame (bindings pairlist)
    write_object(writer, frame, ref_table)?;

    // Write hashtab
    write_object(writer, hashtab, ref_table)?;

    // Write attributes (environments always serialize an attribute field)
    write_object(writer, &RObject::Null, ref_table)?;

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
    write_flags(writer, PROMSXP, false, false, false)?;

    // Write the three components: value, expression, environment
    write_object(writer, value, ref_table)?;
    write_object(writer, expression, ref_table)?;
    write_object(writer, environment, ref_table)?;

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
) -> Result<()> {
    write_flags(writer, BCODESXP, false, false, false)?;
    // For now, we don't emit any bytecode-specific reference table entries.
    writer.write_u32::<BigEndian>(0)?;
    write_bytecode_body(writer, code, constants, ref_table)
}

fn write_bytecode_body(
    writer: &mut Vec<u8>,
    code: &RObject,
    constants: &RObject,
    ref_table: &mut RefTable,
) -> Result<()> {
    write_object(writer, code, ref_table)?;

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
                write_bytecode_body(writer, code, constants, ref_table)?;
            }
            _ => {
                writer.write_i32::<BigEndian>(0)?;
                write_object(writer, value, ref_table)?;
            }
        }
    }

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
    write_flags(writer, VECSXP, true, false, false)?;
    writer.write_u32::<BigEndian>(column_values.len() as u32)?;

    // Write each column
    for col in &column_values {
        write_object(writer, col, ref_table)?;
    }

    // Write attributes (names, row.names, class)
    let mut attrs = Attributes::new();
    attrs.insert(Arc::from("names"), RObject::Character(column_names));
    attrs.insert(
        Arc::from("row.names"),
        RObject::Character(row_names.to_vec()),
    );
    attrs.insert(
        Arc::from("class"),
        RObject::Character(vec![Arc::from("data.frame")]),
    );

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
    write_flags(writer, INTSXP, true, false, false)?;
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
            write_flags(writer, VECSXP, true, false, false)?;
            writer.write_u32::<BigEndian>(elements.len() as u32)?;
            for element in elements {
                write_object(writer, element, ref_table)?;
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
            attrs.insert(Arc::from("class"), RObject::Character(class.to_vec()));
            write_attributes(writer, &attrs, ref_table)?;

            // Write the function (CAR)
            // If it's a single-element Character, write it as a symbol (function name)
            match function.as_ref() {
                RObject::Character(vec) if vec.len() == 1 => {
                    write_symbol_with_ref(writer, &vec[0], ref_table)?;
                }
                _ => {
                    write_object(writer, function, ref_table)?;
                }
            }

            // Write the arguments (CDR) as a pairlist or NULL
            if !args.is_empty() {
                write_pairlist_as_args(writer, args, ref_table)?;
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
fn write_s4_object(
    writer: &mut Vec<u8>,
    class: &[Arc<str>],
    package: Option<&Arc<str>>,
    slots: &HashMap<Arc<str>, RObject>,
    ref_table: &mut RefTable,
) -> Result<()> {
    // S4 objects are written as S4SXP with attributes and IS_S4_BIT set
    write_flags(writer, S4SXP, true, false, true)?;

    // Write attributes (class + slots)
    let mut attrs = Attributes::new();

    // For S4 objects, the class attribute must have a package attribute
    // Use the stored package if available, otherwise fall back to ".GlobalEnv" for user-defined classes
    let class_obj = RObject::Character(class.to_vec());
    let mut class_attrs = Attributes::new();
    let pkg_value = package.cloned().unwrap_or_else(|| Arc::from(".GlobalEnv"));
    class_attrs.insert(Arc::from("package"), RObject::Character(vec![pkg_value]));

    let class_with_package = RObject::WithAttributes {
        object: Box::new(class_obj),
        attributes: class_attrs,
    };

    attrs.insert(Arc::from("class"), class_with_package);

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
        RObject::Character(vec) => {
            write_flags(writer, STRSXP, true, false, false)?;
            writer.write_u32::<BigEndian>(vec.len() as u32)?;
            for s in vec {
                write_charsxp(writer, s)?;
            }
        }
        RObject::List(elements) => {
            write_flags(writer, VECSXP, true, false, false)?;
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
fn write_attributes(
    writer: &mut Vec<u8>,
    attributes: &Attributes,
    ref_table: &mut RefTable,
) -> Result<()> {
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
            value: (**value).clone(), // Unbox the RObject
            tag_object: None,
        });
    }

    // Write the pairlist
    write_pairlist(writer, &elements, ref_table)?;

    Ok(())
}
