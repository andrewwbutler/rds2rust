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
     - `Language` - Language objects (unevaluated expressions/calls)
     - `DataFrame` - Data frames with columns and row names
     - `Factor` - Factors (categorical variables with levels)
     - `S3Object` - S3 objects with class attribute
     - `S4Object` - S4 objects with slots
     - `WithAttributes` - Objects with attributes
   - Special value handling (NA, NaN, Inf)
   - `PairlistElement` struct for tagged pairlist elements
   - `Attributes` struct with HashMap storage

4. **Test Infrastructure**
   - Integration test file: [tests/integration_tests.rs](tests/integration_tests.rs)
   - Roundtrip test file: [tests/roundtrip_tests.rs](tests/roundtrip_tests.rs)
   - Reference tracking test file: [tests/ref_tracking_tests.rs](tests/ref_tracking_tests.rs)
   - R script to generate test data: [tests/generate_test_data.R](tests/generate_test_data.R)
   - **108 passing tests** (3 unit + 48 integration + 5 parser + 12 reference tracking + 40 roundtrip) covering:
     - NULL, integers, reals, logicals, characters
     - Empty vectors and vectors with NA values
     - Special float values (Inf, -Inf, NaN)
     - Lists (simple, empty, nested, named)
     - Named vectors (integer, real, character)
     - Matrices (integer, real, with dimnames)
     - Data frames (simple, mixed types, with row names)
     - Raw vectors (byte arrays)
     - Complex vectors (complex numbers)
     - Factors (simple, ordered)
     - S3 objects (simple, multi-class, on vectors)
     - S4 objects (simple, inheritance, complex slots)
     - Language objects (simple calls, nested expressions, named arguments)
     - Expression vectors (single, multiple, empty, calls, nested, manual)
     - Formulas (simple, multiple predictors, interactions, functions, no intercept, one-sided)
     - Reference tracking (REFSXP, ALTREP optimizations, shared objects)
     - **Complete roundtrip coverage**: All types verified with read -> write -> read

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

### ✅ Phase 5: Remaining Basic Types (COMPLETED)

1. ✅ **Raw Vectors (RAWSXP)**
   - Parse byte vectors
   - Integration tests added

2. ✅ **Complex Vectors (CPLXSXP)**
   - Parse complex number vectors (real + imaginary pairs)
   - Integration tests added

### ✅ Phase 6: Object-Oriented Systems (COMPLETED)

1. ✅ **S3 Objects**
   - Automatic S3 object detection via class attribute
   - Conversion from objects-with-attributes
   - Support for multiple classes (inheritance)
   - S3 objects on vectors with additional attributes
   - Integration tests (simple, multi-class, vector-based)

2. ✅ **S4 Objects**
   - S4SXP type (25) parsing
   - Slot extraction from attributes
   - Class attribute handling (unwrapping WithAttributes wrapper)
   - Package attribute filtering
   - Support for S4 inheritance
   - Integration tests (simple Animal class, Bird inheritance, Aquarium with multiple slot types)

### ✅ Phase 7: Factors (COMPLETED)

1. ✅ **Factor Support**
   - Dedicated `Factor` variant in RObject enum
   - Automatic factor detection via class attribute
   - Integer values (1-based indices into levels)
   - Level labels (character vector)
   - Ordered factor support (ordered flag)
   - Integration tests (simple factor, ordered factor)

### ✅ Phase 8: Writing Support (COMPLETED)

1. ✅ **Basic Serialization**
   - Header writing (XDR format, version 2)
   - Type flag encoding (SEXP type + attribute/tag bits)
   - Gzip compression

2. ✅ **Vector Writing**
   - Integer vectors (INTSXP)
   - Real vectors (REALSXP)
   - Logical vectors (LGLSXP) with TRUE/FALSE/NA
   - Character vectors (STRSXP) with CHARSXP encoding
   - Raw vectors (RAWSXP)
   - Complex vectors (CPLXSXP)

3. ✅ **Complex Type Writing**
   - Lists (VECSXP)
   - Pairlists (LISTSXP) with tags
   - Data frames (list with attributes)
   - Factors (integer vector with levels and class attributes)

4. ✅ **Object-Oriented Writing**
   - S3 objects (base object with class attribute)
   - S4 objects (S4SXP with slots as attributes)
   - Objects with attributes (WithAttributes)

5. ✅ **Roundtrip Tests**
   - 28 comprehensive roundtrip tests verifying read -> write -> read integrity
   - Tests for all basic types: NULL, vectors (integer, real, logical, character, raw, complex)
   - Tests for all complex types: lists, data frames (simple, mixed, with rownames)
   - Tests for all object-oriented types: factors (simple, ordered), S3 objects (simple, multi-class, vector), S4 objects (simple, inheritance, complex)
   - Tests for language objects: simple calls, nested expressions, named arguments
   - All tests pass with byte-perfect equality

### ✅ Phase 9: Language Objects (COMPLETED)

1. ✅ **Language Objects (LANGSXP)**
   - Added `Language` variant to RObject enum
   - Implemented LANGSXP parsing (unevaluated expressions/calls)
   - Structure: function + arguments as flat list
   - Handles nested language objects
   - Writing support for serialization
   - Test data generation for simple, complex, and nested expressions
   - Integration tests (3 tests for language objects)

### ✅ Phase 10: Expression Vectors (COMPLETED)

1. ✅ **Expression Vectors (EXPRSXP)**
   - Added `Expression` variant to RObject enum
   - Implemented EXPRSXP parsing (collections of unevaluated expressions)
   - Identical structure to VECSXP but semantically represents parsed code
   - Typically result of `parse()` or `expression()` in R
   - Writing support for serialization
   - Test data generation:
     - Single expression: `parse(text = "x + 1")`
     - Multiple expressions: `parse(text = c("x + 1", "y * 2", "z / 3"))`
     - Empty expression vector: `expression()`
     - Function calls: `parse(text = c("mean(x)", "sum(y)", "sd(z)"))`
     - Nested calls: `parse(text = "sqrt(x + y)")`
     - Manual creation: `expression(a + b, c * d, sqrt(e))`
   - Integration tests (6 tests for expression vectors)
   - Roundtrip tests (6 tests for expression vectors)

### ✅ Phase 11: Formulas (COMPLETED)

1. ✅ **Formula Support**
   - Formulas are S3 objects (Language base with class="formula")
   - Fixed LANGSXP/LISTSXP attribute parsing (attributes come BEFORE CAR/CDR)
   - Added GLOBALENV_SXP constant (253) for global environment references
   - Updated parser to handle early attribute parsing for pairlists and language objects
   - Updated writer to write attributes before CAR/CDR for language objects
   - Test data generation:
     - Simple formula: `y ~ x`
     - Multiple predictors: `y ~ x + z`
     - Interaction terms: `y ~ x * z`
     - Functions in formula: `log(y) ~ sqrt(x) + I(z^2)`
     - No intercept: `y ~ x - 1`
     - One-sided formula: `~ x + y`
   - Integration tests (6 tests for formulas)
   - Roundtrip tests (6 tests for formulas)

### ✅ Phase 12: Reference Tracking (COMPLETED)

1. ✅ **REFSXP Support**
   - Reference index encoded in bits 8-15 of flags (not as separate u32)
   - Reference table for tracking shared objects
   - Placeholder-based forward reference support
   - Automatic deduplication of shared objects

2. ✅ **ALTREP Optimized Serialization**
   - Bare Real vector detection for ALTREP compact_intseq state
   - Pattern matching: `[length, start, 1.0]` → Integer sequence conversion
   - Integer([13]) state format handling (data in class_info)
   - NILVALUE consumption after bare REALSXP state vectors
   - Position-aware parsing (non-last element handling)

3. ✅ **Reference Tracking Tests**
   - **12 comprehensive tests (100% pass rate)**:
     - test_non_altrep - Non-ALTREP vector handling
     - test_two_copies - Two ALTREP copies
     - test_three_copies - Three ALTREP copies with bare state
     - test_three_shared - Three shared references
     - test_four_copies - Four ALTREP copies
     - test_third_only - Standalone ALTREP
     - test_simple_ref - Simple reference with attributes
     - test_ref_shared_vector - Shared vector references
     - test_ref_shared_list - Shared list references
     - test_ref_shared_expression - Shared expression references
     - test_ref_complex_shared - Complex shared structures
     - test_ref_large_shared - Large ALTREP sequences (1:1000)

## Next Steps

### 📋 Phase 13: Additional Language Features (UPCOMING)

1. **Full Closure and Environment Support**
   - Complete function object parsing
   - Environment frame parsing
   - Binding resolution

2. **Promises and Special Types**
   - PROMSXP handling
   - SPECIALSXP handling
   - BUILTINSXP handling

### 📋 Phase 14: Additional Compression (UPCOMING)

1. **Bzip2 Support**
   - Bzip2 decompression support
   - XZ decompression support (if needed)

### 📋 Phase 15: Performance & Polish (UPCOMING)

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

7. **S4 Object Parsing**
   - S4SXP is a marker type with no data payload
   - All S4 data (class and slots) stored in attributes
   - Class attribute may be wrapped in WithAttributes (with package info)
   - Slots are all attributes except class and package
   - HashMap-based slot storage for O(1) access

8. **Factor Recognition**
   - Automatic detection via class attribute ("factor" or "ordered")
   - Conversion from integer vector + attributes structure
   - Priority order: data.frame > factor > S3 object > attributes
   - 1-based integer indices into level labels

9. **Reference Tracking System**
   - REFSXP index encoding in bits 8-15 of flags (discovered through debugging)
   - Reference table with placeholder-based forward reference support
   - Automatic shared object deduplication
   - Handles circular references and complex object graphs

10. **ALTREP Optimized Serialization Handling**
    - Detection of bare Real vector ALTREP states in lists
    - Pattern recognition: `[length, start, 1.0]` → compact_intseq conversion
    - Special Integer([13]) format with data in class_info field
    - Position-aware NILVALUE consumption (non-last elements only)
    - Handles R's serialization optimization where 3rd+ ALTREP copies become bare state vectors

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
