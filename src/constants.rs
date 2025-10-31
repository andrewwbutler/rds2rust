//! Constants for RDS file format.
//!
//! This module contains all SEXP type codes, flag bit masks, and special values
//! used in RDS serialization.

/// SEXP type constants
/// These represent the different types of R objects that can be serialized.

/// NULL object
pub const NILSXP: u32 = 0;

/// Symbol (variable name)
pub const SYMSXP: u32 = 1;

/// Pairlist (linked list with tags)
pub const LISTSXP: u32 = 2;

/// Closure (function)
pub const CLOSXP: u32 = 3;

/// Environment
pub const ENVSXP: u32 = 4;

/// Language construct (unevaluated call/expression)
pub const LANGSXP: u32 = 6;

/// Internal character string
pub const CHARSXP: u32 = 9;

/// Logical vector (TRUE/FALSE/NA)
pub const LGLSXP: u32 = 10;

/// Integer vector
pub const INTSXP: u32 = 13;

/// Real (double) vector
pub const REALSXP: u32 = 14;

/// Complex vector
pub const CPLXSXP: u32 = 15;

/// Character vector (vector of strings)
pub const STRSXP: u32 = 16;

/// Generic list (VECSXP)
pub const VECSXP: u32 = 19;

/// Raw (byte) vector
pub const RAWSXP: u32 = 24;

/// S4 object
pub const S4SXP: u32 = 25;

/// Special pseudo-types
/// These are marker types used in the serialization format.

/// ALTREP object (version 3 feature)
pub const ALTREP_SXP: u32 = 238; // 0xEE

/// Singleton NULL value
pub const NILVALUE_SXP: u32 = 254; // 0xFE

/// Reference to an already seen object
pub const REFSXP: u32 = 255; // 0xFF

/// Flag bit masks
/// These are used in the flags field of serialized objects.

/// Object has attributes
pub const HAS_ATTR_BIT: u32 = 1 << 9; // 0x00000200

/// Pairlist node has a tag
pub const HAS_TAG_BIT: u32 = 1 << 10; // 0x00000400
