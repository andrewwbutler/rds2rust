use std::fs::File;
use std::io::Read;
use std::path::Path;

use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};

use crate::{Endian, Error, Result, VectorKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub object_kind: String,
    pub vectors: Vec<ManifestVector>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVector {
    pub path: String,
    pub file: String,
    pub kind: String,
    pub length: usize,
    pub elem_size: usize,
    pub endian: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorFileHeader {
    pub version: u8,
    pub kind: VectorKind,
    pub endian: Endian,
    pub length: u64,
    pub elem_size: u32,
}

pub fn read_extraction_manifest<P: AsRef<Path>>(path: P) -> Result<Manifest> {
    let contents = std::fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|err| Error::InvalidFormat(err.to_string()))
}

pub fn read_vector_file_header<P: AsRef<Path>>(path: P) -> Result<VectorFileHeader> {
    const MAGIC: &[u8; 8] = b"RDS2VEC1";
    let mut file = File::open(path)?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(Error::InvalidFormat("invalid vector header magic".to_string()));
    }

    let version = file.read_u8()?;
    let kind_tag = file.read_u8()?;
    let endian_tag = file.read_u8()?;
    let _reserved = file.read_u8()?;
    let length = file.read_u64::<BigEndian>()?;
    let elem_size = file.read_u32::<BigEndian>()?;

    let kind = kind_from_tag(kind_tag)?;
    let endian = endian_from_tag(endian_tag)?;

    Ok(VectorFileHeader {
        version,
        kind,
        endian,
        length,
        elem_size,
    })
}

pub fn validate_vector_file_header<P: AsRef<Path>>(
    path: P,
    expected: &ManifestVector,
) -> Result<VectorFileHeader> {
    let header = read_vector_file_header(path)?;
    let expected_kind = kind_from_str(&expected.kind)?;
    let expected_endian = endian_from_str(&expected.endian)?;

    if header.kind != expected_kind {
        return Err(Error::InvalidFormat(format!(
            "vector kind mismatch: expected {:?}, got {:?}",
            expected_kind, header.kind
        )));
    }
    if header.endian != expected_endian {
        return Err(Error::InvalidFormat(format!(
            "vector endian mismatch: expected {:?}, got {:?}",
            expected_endian, header.endian
        )));
    }
    if header.length != expected.length as u64 {
        return Err(Error::InvalidFormat(format!(
            "vector length mismatch: expected {}, got {}",
            expected.length, header.length
        )));
    }
    if header.elem_size != expected.elem_size as u32 {
        return Err(Error::InvalidFormat(format!(
            "vector elem_size mismatch: expected {}, got {}",
            expected.elem_size, header.elem_size
        )));
    }

    Ok(header)
}

fn kind_from_tag(tag: u8) -> Result<VectorKind> {
    match tag {
        1 => Ok(VectorKind::Integer),
        2 => Ok(VectorKind::Real),
        3 => Ok(VectorKind::Logical),
        4 => Ok(VectorKind::Raw),
        5 => Ok(VectorKind::Complex),
        6 => Ok(VectorKind::Character),
        _ => Err(Error::InvalidFormat(format!(
            "invalid vector kind tag {}",
            tag
        ))),
    }
}

fn endian_from_tag(tag: u8) -> Result<Endian> {
    match tag {
        1 => Ok(Endian::Big),
        2 => Ok(Endian::Little),
        _ => Err(Error::InvalidFormat(format!(
            "invalid vector endian tag {}",
            tag
        ))),
    }
}

fn kind_from_str(value: &str) -> Result<VectorKind> {
    match value {
        "Integer" => Ok(VectorKind::Integer),
        "Real" => Ok(VectorKind::Real),
        "Logical" => Ok(VectorKind::Logical),
        "Raw" => Ok(VectorKind::Raw),
        "Complex" => Ok(VectorKind::Complex),
        "Character" => Ok(VectorKind::Character),
        _ => Err(Error::InvalidFormat(format!(
            "invalid vector kind '{}'",
            value
        ))),
    }
}

fn endian_from_str(value: &str) -> Result<Endian> {
    match value {
        "Big" => Ok(Endian::Big),
        "Little" => Ok(Endian::Little),
        _ => Err(Error::InvalidFormat(format!(
            "invalid vector endian '{}'",
            value
        ))),
    }
}
