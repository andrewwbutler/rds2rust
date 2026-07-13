# RDS File Format Documentation

This document describes the RDS (R Data Serialization) file format used by R's `saveRDS()` and `readRDS()` functions.

## File Structure

An RDS file consists of:
1. Header (13-14 bytes)
2. Serialized R object

## Header Format

The header contains:

```
Byte 0-1:   Magic bytes (format identifier)
            'X\n' (0x58 0x0a) - XDR format (big-endian)
            'A\n' (0x41 0x0a) - ASCII format
            'B\n' (0x42 0x0a) - Binary format (little-endian)

Byte 2-5:   Format version (32-bit big-endian integer)
            Version 2 is most common (0x00000002)
            Version 3 adds support for custom reference hooks

Byte 6-9:   R version that wrote the file (32-bit big-endian integer)
            Encoded as: (major << 16) | (minor << 8) | patch
            Example: R 3.5.0 = 0x00030500

Byte 10-13: Minimum R version needed to read (32-bit big-endian integer)
            Same encoding as above
```

## SEXP Types

R objects are called SEXPs (S-EXPressions). Each object has a type tag:

```
Type    Value   Description
----    -----   -----------
NILSXP  0       NULL
SYMSXP  1       Symbol
LISTSXP 2       Pairlist
CLOSXP  3       Closure (function)
ENVSXP  4       Environment
PROMSXP 5       Promise
LANGSXP 6       Language construct
SPECIALSXP 7    Special function
BUILTINSXP 8    Built-in function
CHARSXP 9       Internal character string
LGLSXP  10      Logical vector
INTSXP  13      Integer vector
REALSXP 14      Real (double) vector
CPLXSXP 15      Complex vector
STRSXP  16      Character vector
DOTSXP  17      Dot-dot-dot object
ANYSXP  18      Any type (for matching)
VECSXP  19      List (generic vector)
EXPRSXP 20      Expression vector (vector of language objects)
BCODESXP 21     Byte code
EXTPTRSXP 22    External pointer
WEAKREFSXP 23   Weak reference
RAWSXP  24      Raw vector
S4SXP   25      S4 object
```

## Object Format

Each object in the stream has:

```
Byte 0-3:   Flags and type (32-bit big-endian integer)
            Bits 0-7:   SEXP type (see table above)
            Bit 8:      Has attributes flag
            Bit 9:      Has tag flag (for pairlists)
            Bits 10-15: GP field (general purpose)
            Bits 16-23: Levels (for reference tracking)
            Bits 24-31: Object flag and other flags
```

### Flag Bits Detail

```
IS_OBJECT_BIT_MASK  (1 << 8)   = 0x00000100 - Object has S3/S4 class
HAS_ATTR_BIT_MASK   (1 << 9)   = 0x00000200 - Object has attributes
HAS_TAG_BIT_MASK    (1 << 10)  = 0x00000400 - Pairlist node has tag
```

### Reference Tracking

Format version 2 introduced reference tracking to handle shared/recursive structures:

- When an object might be referenced again, it's added to a reference table
- Subsequent references use a REFSXP pseudo-type with an index
- REFSXP = 0xFF (255) is a special marker for a reference

## Vector Formats

### Integer Vector (INTSXP)

```
4 bytes:  Length (N) as big-endian int32
N*4 bytes: N integers as big-endian int32
```

Special value: `NA_integer_` = 0x80000000 (INT32_MIN)

### Real Vector (REALSXP)

```
4 bytes:    Length (N) as big-endian int32
N*8 bytes:  N doubles as big-endian IEEE 754 double
```

Special values:
- `NA_real_`: Specific NaN bit pattern (0x7FF00000000007A2)
- `Inf`: IEEE 754 infinity
- `-Inf`: IEEE 754 negative infinity
- `NaN`: IEEE 754 NaN (various bit patterns)

### Logical Vector (LGLSXP)

```
4 bytes:  Length (N) as big-endian int32
N*4 bytes: N logicals as big-endian int32
           0 = FALSE
           1 = TRUE
           INT32_MIN (0x80000000) = NA
```

### Character Vector (STRSXP)

```
4 bytes:  Length (N) as big-endian int32
N*X bytes: N CHARSXP objects (see below)
```

### CHARSXP (Internal String)

```
4 bytes:  Length (L) in bytes as big-endian int32
          -1 (0xFFFFFFFF) = NA_character_
L bytes:  UTF-8 encoded string data (no null terminator)
```

Special handling:
- Length includes encoding byte if present
- Encoding can be UTF-8, Latin1, or native

### Raw Vector (RAWSXP)

```
4 bytes:  Length (N) as big-endian int32
N bytes:  N raw bytes
```

### Complex Vector (CPLXSXP)

```
4 bytes:     Length (N) as big-endian int32
N*16 bytes:  N complex numbers
             Each: 8 bytes real (double) + 8 bytes imaginary (double)
             Both big-endian
```

### List (VECSXP)

```
4 bytes:   Length (N) as big-endian int32
N*X bytes: N serialized R objects (recursive)
```

### Expression Vector (EXPRSXP)

```
4 bytes:   Length (N) as big-endian int32
N*X bytes: N serialized R objects (recursive)
```

Note: EXPRSXP is structurally identical to VECSXP. The difference is semantic - EXPRSXP represents collections of unevaluated expressions (typically language objects), most often as the result of `parse()` or `expression()` in R.

## Attributes

If the HAS_ATTR_BIT_MASK flag is set, attributes follow the object data:

```
X bytes:  Pairlist (LISTSXP) of attributes
          Each attribute is a pair: tag (symbol) + value (object)
```

Common attributes:
- `names`: Character vector of element names
- `dim`: Integer vector for matrix/array dimensions
- `dimnames`: List of dimension names
- `class`: Character vector of class names
- `row.names`: Row names for data frames

## Pairlist (LISTSXP)

```
For each node:
  4 bytes:   Flags and type (with HAS_TAG_BIT if tagged)
  X bytes:   CAR (value object)
  [X bytes:  TAG (symbol) - only if HAS_TAG_BIT set]
  X bytes:   CDR (rest of list or NILSXP if end)
```

## Compression

RDS files can be compressed. The compression is applied to the entire file after serialization:

- **gzip**: Most common, uses deflate algorithm
- **bzip2**: Better compression, slower
- **xz**: Best compression, slowest

Compression is transparent - the header magic bytes appear after decompression.

## Implementation Notes

1. All multi-byte integers are big-endian (XDR format is most common)
2. Strings are UTF-8 encoded by default
3. Reference tracking must be maintained during parsing
4. Attributes are stored separately from object data
5. Special NA values must be preserved exactly
6. Float special values (NaN, Inf) follow IEEE 754

## Example: Simple Integer Vector

For `saveRDS(42L, "test.rds")`:

```
58 0a                 # Magic: 'X\n' (XDR format)
00 00 00 02           # Format version 2
00 04 04 01           # R version 4.4.1
00 03 05 00           # Min R version 3.5.0
00 00 00 0d           # Type: INTSXP (13)
00 00 00 01           # Length: 1
00 00 00 2a           # Value: 42
```

## References

- R source code: `src/main/serialize.c`
- R Internals manual: Section on serialization
- rds2cpp: C++ implementation by LTLA
