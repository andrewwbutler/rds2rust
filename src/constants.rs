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

/// Promise (lazy evaluation)
pub const PROMSXP: u32 = 5;

/// Language construct (unevaluated call/expression)
pub const LANGSXP: u32 = 6;

/// Special function (primitive like 'if', 'for')
pub const SPECIALSXP: u32 = 7;

/// Builtin function (primitive like 'sum', 'c')
pub const BUILTINSXP: u32 = 8;

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

/// Expression vector (vector of language objects)
pub const EXPRSXP: u32 = 20;

/// Raw (byte) vector
pub const RAWSXP: u32 = 24;

/// S4 object
pub const S4SXP: u32 = 25;

/// Special pseudo-types
/// These are marker types used in the serialization format.

/// ALTREP object (version 3 feature)
/// Note: R uses different ALTREP type codes depending on context:
/// - 238 (0xEE) for some ALTREP types
/// - 249 (0xF9) for other ALTREP types (newer format)
pub const ALTREP_SXP: u32 = 238; // 0xEE
pub const ALTREP_SXP_ALT: u32 = 249; // 0xF9

/// Unbound value (missing argument marker)
pub const UNBOUNDVALUE_SXP: u32 = 251; // 0xFB

/// Empty argument marker
pub const EMPTYENV_SXP: u32 = 252; // 0xFC

/// Global environment
pub const GLOBALENV_SXP: u32 = 253; // 0xFD

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
