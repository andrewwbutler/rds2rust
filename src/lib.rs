//! A Rust library for reading and writing R RDS files without requiring an R runtime.
//!
//! This library provides functionality to serialize and deserialize R objects to/from
//! the RDS binary format.

use std::sync::Arc;

mod constants;
mod error;
mod parser;
mod types;
mod writer;

pub use error::{Error, Result};
pub use types::{
    Attributes, DataFrameData, FactorData, Logical, PairlistElement, RObject, S4ObjectData,
};

/// Configuration for parsing RDS files.
///
/// Allows customization of memory allocation limits to handle large files
/// or enforce stricter safety constraints.
#[derive(Debug, Clone, Copy)]
pub struct ParseConfig {
    /// Maximum number of elements allowed in a vector (default: 50,000,000)
    pub max_vector_length: usize,
    /// Maximum bytes that can be allocated for a single vector (default: 128 MB)
    pub max_allocation_bytes: usize,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            max_vector_length: 50_000_000,
            max_allocation_bytes: 128 * 1024 * 1024, // 128 MB
        }
    }
}

impl ParseConfig {
    /// Create a new ParseConfig with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum vector length.
    pub fn with_max_vector_length(mut self, max: usize) -> Self {
        self.max_vector_length = max;
        self
    }

    /// Set the maximum allocation bytes.
    pub fn with_max_allocation_bytes(mut self, max: usize) -> Self {
        self.max_allocation_bytes = max;
        self
    }

    /// Create a config suitable for large scientific datasets (e.g., genomics).
    ///
    /// Sets higher limits:
    /// - max_vector_length: 500,000,000 (500M elements)
    /// - max_allocation_bytes: 2 GB
    pub fn large_data() -> Self {
        Self {
            max_vector_length: 500_000_000,
            max_allocation_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
        }
    }

    /// Create a config with unlimited size (use with caution).
    ///
    /// Only use this when you trust the input files and have sufficient memory.
    pub fn unlimited() -> Self {
        Self {
            max_vector_length: usize::MAX,
            max_allocation_bytes: usize::MAX,
        }
    }
}

/// Read an RDS file from a byte slice with default safety limits.
///
/// For large files, consider using [`read_rds_with_config`] with [`ParseConfig::large_data()`].
pub fn read_rds(data: &[u8]) -> Result<RObject> {
    read_rds_with_config(data, ParseConfig::default())
}

/// Read an RDS file from a byte slice with custom configuration.
///
/// # Examples
///
/// ```
/// use rds2rust::{read_rds_with_config, ParseConfig};
///
/// // For large scientific datasets
/// let config = ParseConfig::large_data();
/// // let obj = read_rds_with_config(&data, config)?;
///
/// // For custom limits
/// let config = ParseConfig::new()
///     .with_max_allocation_bytes(512 * 1024 * 1024); // 512 MB
/// // let obj = read_rds_with_config(&data, config)?;
/// ```
pub fn read_rds_with_config(data: &[u8], config: ParseConfig) -> Result<RObject> {
    let obj = parser::parse_rds_with_config(data, config)?;
    Ok(unwrap_top_level_shared(obj))
}

/// Unwrap Shared wrappers added by the parser for reference tracking.
///
/// The parser wraps all tracked objects in RObject::Shared to maintain Arc consistency
/// for REFSXP references. At the API boundary, we recursively unwrap Shared objects
/// that only have one strong reference (not actually shared via REFSXP).
///
/// Objects with multiple references (actual shared references from REFSXP) are kept as Shared.
fn unwrap_top_level_shared(obj: RObject) -> RObject {
    unwrap_shared_recursive(obj)
}

fn unwrap_shared_recursive(obj: RObject) -> RObject {
    match obj {
        RObject::Shared(arc) => {
            let strong_count = Arc::strong_count(&arc);
            if strong_count == 1 {
                // Only one reference - this is just for tracking, unwrap it
                match Arc::try_unwrap(arc) {
                    Ok(rwlock) => {
                        let inner = rwlock.into_inner().unwrap();
                        // Recursively unwrap the inner object
                        unwrap_shared_recursive(inner)
                    }
                    Err(arc) => {
                        // Shouldn't happen if strong_count was 1, but handle gracefully
                        RObject::Shared(arc)
                    }
                }
            } else {
                // Multiple references - this is a real shared reference, keep it
                RObject::Shared(arc)
            }
        }
        // Recursively unwrap container types
        RObject::List(elements) => {
            RObject::List(elements.into_iter().map(unwrap_shared_recursive).collect())
        }
        RObject::Pairlist(elements) => RObject::Pairlist(
            elements
                .into_iter()
                .map(|elem| PairlistElement {
                    tag: elem.tag,
                    value: unwrap_shared_recursive(elem.value),
                    tag_object: elem
                        .tag_object
                        .map(|obj| Box::new(unwrap_shared_recursive(*obj))),
                })
                .collect(),
        ),
        RObject::Language { function, args } => RObject::Language {
            function: Box::new(unwrap_shared_recursive(*function)),
            args: args
                .into_iter()
                .map(|elem| PairlistElement {
                    tag: elem.tag,
                    value: unwrap_shared_recursive(elem.value),
                    tag_object: elem
                        .tag_object
                        .map(|obj| Box::new(unwrap_shared_recursive(*obj))),
                })
                .collect(),
        },
        RObject::Expression(elements) => {
            RObject::Expression(elements.into_iter().map(unwrap_shared_recursive).collect())
        }
        RObject::Closure {
            formals,
            body,
            environment,
        } => RObject::Closure {
            formals: Box::new(unwrap_shared_recursive(*formals)),
            body: Box::new(unwrap_shared_recursive(*body)),
            environment: Box::new(unwrap_shared_recursive(*environment)),
        },
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => RObject::Environment {
            enclosing: Box::new(unwrap_shared_recursive(*enclosing)),
            frame: Box::new(unwrap_shared_recursive(*frame)),
            hashtab: Box::new(unwrap_shared_recursive(*hashtab)),
        },
        RObject::Promise {
            value,
            expression,
            environment,
        } => RObject::Promise {
            value: Box::new(unwrap_shared_recursive(*value)),
            expression: Box::new(unwrap_shared_recursive(*expression)),
            environment: Box::new(unwrap_shared_recursive(*environment)),
        },
        RObject::Bytecode {
            code,
            constants,
            expr,
        } => RObject::Bytecode {
            code: Box::new(unwrap_shared_recursive(*code)),
            constants: Box::new(unwrap_shared_recursive(*constants)),
            expr: Box::new(unwrap_shared_recursive(*expr)),
        },
        RObject::DataFrame(mut df_data) => {
            // Unwrap RObjects in the columns
            for (_, value) in df_data.columns.iter_mut() {
                *value = unwrap_shared_recursive(std::mem::replace(value, RObject::Null));
            }
            RObject::DataFrame(df_data)
        }
        RObject::S3Object(mut s3_data) => {
            s3_data.base = Box::new(unwrap_shared_recursive(*s3_data.base));
            s3_data.attributes = unwrap_attributes(s3_data.attributes);
            RObject::S3Object(s3_data)
        }
        RObject::S4Object(mut s4_data) => {
            // Unwrap RObjects in the slots
            for (_, value) in s4_data.slots.iter_mut() {
                *value = unwrap_shared_recursive(std::mem::replace(value, RObject::Null));
            }
            RObject::S4Object(s4_data)
        }
        RObject::WithAttributes { object, attributes } => RObject::WithAttributes {
            object: Box::new(unwrap_shared_recursive(*object)),
            attributes: unwrap_attributes(attributes),
        },
        // Other types don't contain nested RObjects or don't need unwrapping
        other => other,
    }
}

/// Helper to recursively unwrap Shared objects in attributes
fn unwrap_attributes(mut attrs: Attributes) -> Attributes {
    for (_, value) in attrs.attrs.iter_mut() {
        *value = Box::new(unwrap_shared_recursive(*std::mem::replace(
            value,
            Box::new(RObject::Null),
        )));
    }
    attrs
}

/// Write an RObject to RDS format.
/// Returns gzip-compressed RDS data.
pub fn write_rds(obj: &RObject) -> Result<Vec<u8>> {
    writer::write_rds(obj)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Placeholder test - will be replaced with actual tests
        assert!(true);
    }
}
