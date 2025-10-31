# rds2rust Project Plan

## Overview

Port the functionality of rds2cpp (C++ library for reading/writing RDS files) to Rust, enabling reading and writing of R's RDS binary format without requiring an R runtime.

## Current Status

### ✅ Phase 1: Project Setup (COMPLETED)

1. **Cargo Project Initialized**
   - Library crate structure
   - Dependencies added:
     - `byteorder` - for big-endian XDR format handling
     - `thiserror` - for error handling
     - `flate2` - for gzip compression
     - `bzip2` - for bzip2 compression

2. **Module Structure Created**
   - [src/lib.rs](src/lib.rs) - Public API
   - [src/types.rs](src/types.rs) - R object type definitions
   - [src/error.rs](src/error.rs) - Error types
   - [src/parser.rs](src/parser.rs) - RDS parsing implementation
   - [src/writer.rs](src/writer.rs) - RDS writing (stub)

3. **Type System Defined**
   - `RObject` enum with variants:
     - `Null` - R's NULL
     - `Integer` - Integer vectors
     - `Real` - Double vectors
     - `Logical` - Logical vectors (TRUE/FALSE/NA)
     - `Character` - String vectors
     - `Raw` - Byte vectors
     - `Complex` - Complex number vectors
     - `List` - Generic lists (VECSXP)
     - `Pairlist` - Pairlists (LISTSXP) with tags
     - `DataFrame` - Data frames with columns and row names
     - `WithAttributes` - Objects with attributes
   - Special value handling (NA, NaN, Inf)
   - `PairlistElement` struct for tagged pairlist elements
   - `Attributes` struct with HashMap storage

4. **Test Infrastructure**
   - Integration test file: [tests/integration_tests.rs](tests/integration_tests.rs)
   - R script to generate test data: [tests/generate_test_data.R](tests/generate_test_data.R)
   - **22 passing integration tests** covering:
     - NULL, integers, reals, logicals, characters
     - Empty vectors and vectors with NA values
     - Special float values (Inf, -Inf, NaN)
     - Lists (simple, empty, nested, named)
     - Named vectors (integer, real, character)
     - Matrices (integer, real, with dimnames)
     - Data frames (simple, mixed types, with row names)

5. **Documentation**
   - [RDS_FORMAT.md](RDS_FORMAT.md) - Detailed RDS format specification
   - [tests/README.md](tests/README.md) - How to generate test files
   - Comprehensive format documentation

### ✅ Phase 2: Basic Type Parsing (COMPLETED)

1. ✅ **Header Parsing**
   - Magic byte validation (XDR format)
   - Format version parsing (v2 and v3 support)
   - R version info reading
   - Version 3 encoding string parsing

2. ✅ **Core Type Parsing**
   - SEXP type extraction with XDR encoding quirk handling
   - Flag parsing (HAS_ATTR, HAS_TAG bits)
   - Packaged type support (NILVALUE_SXP, etc.)
   - NULL (NILSXP) parsing
   - Integer vectors (INTSXP) with NA_integer_
   - Real vectors (REALSXP) with NA, Inf, -Inf, NaN
   - Logical vectors (LGLSXP) with TRUE/FALSE/NA
   - Character vectors (STRSXP) with CHARSXP elements
   - Symbol parsing (SYMSXP)

3. ✅ **Gzip Decompression**
   - Automatic detection of compressed files
   - Transparent decompression during parsing

### ✅ Phase 3: Complex Types (COMPLETED)

1. ✅ **Lists and Pairlists**
   - Generic lists (VECSXP)
   - Pairlists (LISTSXP) with TAG support
   - TAG name extraction from symbols
   - Recursive pairlist parsing (CAR/CDR)

2. ✅ **Attributes System**
   - Attribute parsing from pairlists
   - TAG to attribute name conversion
   - HashMap-based attribute storage
   - Common attributes: names, dim, class, row.names, dimnames

3. ✅ **Named Vectors**
   - Names attribute extraction
   - Integer, real, and character named vectors

4. ✅ **Matrices**
   - Dim attribute parsing
   - Column-major storage format
   - Dimnames support

5. ✅ **ALTREP Support**
   - ALTREP object detection (version 3)
   - Compact integer sequence expansion
   - Class info and state parsing

6. ✅ **Closure and Environment Stubs**
   - Basic CLOSXP parsing (returns NULL)
   - Basic ENVSXP parsing (returns NULL)
   - Structure parsing complete, full support pending

### ✅ Phase 4: Data Frames (COMPLETED)

1. ✅ **Data Frame Detection**
   - Class attribute checking ("data.frame")
   - Automatic conversion from list-with-attributes

2. ✅ **Data Frame Parsing**
   - Column extraction with names
   - Row names parsing (character and integer)
   - Compact row names format support (`[NA, -n]`)
   - Mixed column types (int, real, char, logical)
   - HashMap-based column storage

3. ✅ **Data Frame Tests**
   - Simple data frames
   - Mixed column types
   - Custom row names

## Next Steps

### 🔄 Phase 5: Remaining Basic Types (IN PROGRESS)

1. **Raw Vectors (RAWSXP)**
   - Parse byte vectors
   - Add integration tests

2. **Complex Vectors (CPLXSXP)**
   - Parse complex number vectors
   - Add integration tests

### 📋 Phase 6: Advanced Features (UPCOMING)

1. **Reference Tracking**
   - Implement REFSXP handling
   - Track shared objects to avoid duplication
   - Circular reference detection

2. **Additional Compression**
   - Bzip2 decompression support
   - XZ decompression support (if needed)

3. **S3/S4 Objects**
   - Full S3 class support
   - S4 object structure parsing
   - Slot handling

4. **Factors**
   - Factor level parsing
   - Ordered factor support

5. **Language Objects**
   - LANGSXP parsing
   - Expression objects
   - Formula objects

6. **Full Closure and Environment Support**
   - Complete function object parsing
   - Environment frame parsing
   - Binding resolution

7. **Promises and Special Types**
   - PROMSXP handling
   - SPECIALSXP handling
   - BUILTINSXP handling

### 📋 Phase 7: Writing Support (UPCOMING)

1. **Basic Serialization**
   - Header writing
   - Type flag encoding
   - Basic vector writing

2. **Complex Type Writing**
   - Attributes serialization
   - Pairlist serialization
   - List serialization

3. **Data Frame Writing**
   - Convert DataFrame to list-with-attributes
   - Row names serialization (including compact format)

4. **Roundtrip Tests**
   - read -> write -> read comparison
   - Verify data integrity

### 📋 Phase 8: Performance & Polish (UPCOMING)

1. **Optimization**
   - Benchmarking against rds2cpp
   - Memory usage optimization
   - Zero-copy optimizations where possible

2. **Documentation**
   - API documentation
   - Usage examples
   - Migration guide from rds2cpp

3. **Additional Features**
   - Streaming API for large files
   - Parallel decompression
   - Custom compression levels

## Development Workflow

**Test-Driven Development:**
1. Run tests (they will fail): `cargo test`
2. Implement minimal code to make one test pass
3. Verify test passes: `cargo test`
4. Refactor if needed
5. Move to next test

**Current Command:**
```bash
# Generate test data (requires R)
Rscript tests/generate_test_data.R

# Build project
cargo build

# Run tests
cargo test
```

## Key Design Decisions

1. **Big-endian (XDR) format focus**: Most common RDS format (primary implementation)
2. **Public API**: Simple `read_rds()` and `write_rds()` functions
3. **Error handling**: Using `thiserror` for ergonomic errors
4. **Type safety**: Strong Rust types for R objects
5. **NA handling**: Explicit representation in type system (Logical::Na, NA_INTEGER constant)
6. **TDD approach**: Write tests before implementation (followed throughout)
7. **HashMap for columns**: Fast column access in data frames
8. **Automatic decompression**: Transparent gzip handling
9. **Smart defaults**: Automatic data frame detection, compact row names expansion

## Key Technical Achievements

1. **XDR Encoding Quirk Handling**
   - Discovered SEXP types appear in different bit positions (8-15 vs 0-7)
   - Implemented heuristic: use bits 8-15 if >= 10, else bits 0-7
   - Critical for proper CHARSXP parsing with HAS_TAG flag

2. **Packaged Type Support**
   - Single-byte encoded types (NILVALUE_SXP = 0xFE)
   - Peek-ahead detection to distinguish from 4-byte types

3. **Compact Row Names Format**
   - Detected R's `[NA, -n]` encoding for default row names
   - Automatic expansion to `["1", "2", ..., "n"]`

4. **ALTREP Support**
   - Version 3 format compatibility
   - Compact integer sequence expansion
   - Pragmatic type inference from state structure

5. **Attribute System**
   - Pairlist to HashMap conversion
   - TAG extraction from symbols
   - Support for common attributes (names, dim, class, row.names)

6. **Data Frame Recognition**
   - Automatic detection via class attribute
   - Conversion from list-with-attributes structure
   - Mixed column type support

## Resources

- Original C++ library: https://github.com/LTLA/rds2cpp
- R Internals: https://cran.r-project.org/doc/manuals/r-release/R-ints.html
- R serialization: `src/main/serialize.c` in R source
- Format documentation: [RDS_FORMAT.md](RDS_FORMAT.md)

## Testing Strategy

- **Unit tests**: In each module ([src/parser.rs](src/parser.rs), etc.)
- **Integration tests**: In [tests/integration_tests.rs](tests/integration_tests.rs)
- **Test data**: Generated from R using [tests/generate_test_data.R](tests/generate_test_data.R)
- **Verification**: Compare against R's `readRDS()` output
- **Roundtrip tests**: read -> write -> read comparison (Phase 5)

## Project Structure

```
rds2rust/
├── Cargo.toml                     # Project manifest
├── PROJECT_PLAN.md               # This file
├── RDS_FORMAT.md                 # Format specification
├── src/
│   ├── lib.rs                    # Public API
│   ├── types.rs                  # R object types
│   ├── error.rs                  # Error handling
│   ├── parser.rs                 # RDS parsing
│   └── writer.rs                 # RDS writing (future)
└── tests/
    ├── README.md                 # Test documentation
    ├── generate_test_data.R      # R script to create test files
    ├── integration_tests.rs      # Integration tests
    └── data/                     # Test RDS files (generated)
```
