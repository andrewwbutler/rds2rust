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

/// Bytecode (compiled R function)
pub const BCODESXP: u32 = 21;

/// External pointer
pub const EXTPTRSXP: u32 = 22;

/// Weak reference
pub const WEAKREFSXP: u32 = 23;

/// Raw (byte) vector
pub const RAWSXP: u32 = 24;

/// S4 object
pub const S4SXP: u32 = 25;

/// Special pseudo-types
/// These are marker types used in the serialization format.

/// ALTREP object (version 3 feature)
pub const ALTREP_SXP: u32 = 238; // 0xEE

/// Attribute list (alternate encoding)
pub const ATTRLISTSXP: u32 = 239; // 0xEF

/// Attribute language (alternate encoding)
pub const ATTRLANGSXP: u32 = 240; // 0xF0

/// Base environment marker
pub const BASEENV_SXP: u32 = 241; // 0xF1

/// Empty environment marker
pub const EMPTYENV_SXP: u32 = 242; // 0xF2

/// Bytecode representation reference
pub const BCREPREF: u32 = 243; // 0xF3

/// Bytecode representation definition
pub const BCREPDEF: u32 = 244; // 0xF4

/// Generic function reference
pub const GENERICREFSXP: u32 = 245; // 0xF5

/// Class reference
pub const CLASSREFSXP: u32 = 246; // 0xF6

/// Persistent object marker
pub const PERSISTSXP: u32 = 247; // 0xF7

/// Package environment
pub const PACKAGESXP: u32 = 248; // 0xF8

/// Namespace environment (serialization marker)
pub const NAMESPACESXP_SERIAL: u32 = 249; // 0xF9

/// Base namespace marker
pub const BASENAMESPACE_SXP: u32 = 250; // 0xFA

/// Missing argument (unbound value)
pub const MISSINGARG_SXP: u32 = 251; // 0xFB

/// Unbound value marker (same as MISSINGARG_SXP in some contexts)
pub const UNBOUNDVALUE_SXP: u32 = 252; // 0xFC

/// Global environment
pub const GLOBALENV_SXP: u32 = 253; // 0xFD

/// Singleton NULL value
pub const NILVALUE_SXP: u32 = 254; // 0xFE

/// Reference to an already seen object
pub const REFSXP: u32 = 255; // 0xFF

/// Namespace environment (type 123, 0x7B)
/// This appears in R packages objects and represents namespace environments
/// that cannot be meaningfully serialized across sessions
pub const NAMESPACESXP: u32 = 123; // 0x7B

/// Flag bit masks
/// These are used in the flags field of serialized objects.

/// Object has attributes
pub const HAS_ATTR_BIT: u32 = 1 << 9; // 0x00000200

/// Pairlist node has a tag
pub const HAS_TAG_BIT: u32 = 1 << 10; // 0x00000400

/// Object is an "object" (has class attribute) - used in serialization flags
pub const IS_OBJECT_BIT: u32 = 1 << 8; // 0x00000100

/// LEVELS field for S4 objects - bit 4 in the gp field indicates S4
/// ENCODE_LEVELS(v) = v << 12, so S4 bit (0x10) becomes 0x10000
pub const S4_LEVELS: u32 = 0x10 << 12; // 0x00010000

/// ASCII encoding flag for CHARSXP - bit 6 in GP field indicates ASCII string
/// In the serialization format, this becomes bit 18 in the flags word
pub const ASCII_LEVELS: u32 = 0x40 << 12; // 0x00040000
