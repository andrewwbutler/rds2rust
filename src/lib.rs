//! A Rust library for reading and writing R RDS files without requiring an R runtime.
//!
//! This library provides functionality to serialize and deserialize R objects to/from
//! the RDS binary format.

mod constants;
mod error;
mod parser;
mod types;
mod writer;

pub use error::{Error, Result};
pub use types::{Attributes, Logical, PairlistElement, RObject, S4ObjectData};

/// Read an RDS file from a byte slice.
pub fn read_rds(data: &[u8]) -> Result<RObject> {
    parser::parse_rds(data)
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
