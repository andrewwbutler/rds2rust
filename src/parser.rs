//! Parser for RDS files.

use crate::error::{Error, Result};
use crate::types::RObject;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

/// Parse an RDS file from a byte slice.
pub fn parse_rds(data: &[u8]) -> Result<RObject> {
    let mut cursor = Cursor::new(data);

    // Parse header
    parse_header(&mut cursor)?;

    // Parse the object
    parse_object(&mut cursor)
}

/// Parse the RDS file header.
fn parse_header(cursor: &mut Cursor<&[u8]>) -> Result<()> {
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
    let _format_version = cursor.read_u32::<BigEndian>()?;

    // Read R version that wrote the file
    let _writer_version = cursor.read_u32::<BigEndian>()?;

    // Read minimum R version needed to read
    let _min_reader_version = cursor.read_u32::<BigEndian>()?;

    Ok(())
}

/// Parse an R object from the stream.
fn parse_object(_cursor: &mut Cursor<&[u8]>) -> Result<RObject> {
    // Placeholder - will be implemented as we add tests
    todo!("Object parsing not yet implemented")
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
        assert!(parse_header(&mut cursor).is_ok());
    }

    #[test]
    fn test_invalid_magic() {
        let header = vec![b'Y', b'\n', 0, 0, 0, 2];
        let mut cursor = Cursor::new(header.as_slice());
        assert!(parse_header(&mut cursor).is_err());
    }
}
