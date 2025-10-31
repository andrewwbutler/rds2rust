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
   - [src/parser.rs](src/parser.rs) - RDS parsing (stub)
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
     - `List` - Generic lists
     - `WithAttributes` - Objects with attributes
   - Special value handling (NA, NaN, Inf)

4. **Test Infrastructure**
   - Integration test file: [tests/integration_tests.rs](tests/integration_tests.rs)
   - R script to generate test data: [tests/generate_test_data.R](tests/generate_test_data.R)
   - Tests for: NULL, integers, reals, logicals, characters
   - Tests skip gracefully if test data not generated

5. **Documentation**
   - [RDS_FORMAT.md](RDS_FORMAT.md) - Detailed RDS format specification
   - [tests/README.md](tests/README.md) - How to generate test files
   - Comprehensive format documentation including:
     - File structure
     - SEXP types
     - Vector formats
     - Attributes
     - Reference tracking
     - Compression

6. **Basic Header Parsing**
   - Header parsing implemented with tests
   - Validates magic bytes
   - Reads format version and R version info

## Next Steps

### 🔄 Phase 2: Basic Type Parsing (READY TO START)

Following TDD approach, we need to:

1. **Generate Test Data**
   - Install R if not available: `brew install r` (macOS)
   - Run: `Rscript tests/generate_test_data.R`
   - This creates RDS files in `tests/data/`

2. **Implement Object Type Parsing**
   - Parse SEXP type tags and flags
   - Implement reference tracking system
   - Handle attributes flag

3. **Implement NULL Parsing**
   - Simplest type to start with
   - Test: `test_null()`

4. **Implement Integer Vector Parsing**
   - Parse length field
   - Read integer values (big-endian)
   - Handle NA_integer_ special value
   - Tests: `test_integer_single()`, `test_integer_vector()`

5. **Implement Real Vector Parsing**
   - Parse length field
   - Read double values (big-endian IEEE 754)
   - Handle NA, NaN, Inf, -Inf
   - Tests: `test_real_single()`

6. **Implement Logical Vector Parsing**
   - Parse length field
   - Read logical values (int32 encoding)
   - Handle TRUE (1), FALSE (0), NA (INT32_MIN)
   - Tests: `test_logical_true()`

7. **Implement Character Vector Parsing**
   - Parse STRSXP (vector of CHARSXP)
   - Implement CHARSXP parsing
   - Handle UTF-8 encoding
   - Handle NA_character_
   - Tests: `test_character_single()`

### 📋 Phase 3: Complex Types (UPCOMING)

- Raw vectors
- Complex vectors
- Lists (VECSXP)
- Pairlists (LISTSXP)
- Attributes system
- Nested structures

### 📋 Phase 4: Advanced Features (UPCOMING)

- Reference tracking for shared objects
- Compression support (gzip, bzip2, xz)
- S3/S4 object support
- Data frames
- Factors

### 📋 Phase 5: Writing Support (UPCOMING)

- Serialization (write_rds implementation)
- Roundtrip tests

### 📋 Phase 6: Performance & Polish (UPCOMING)

- Benchmarking
- Optimization
- Documentation
- Examples

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

1. **Big-endian (XDR) format focus**: Most common RDS format
2. **Public API**: Simple `read_rds()` and `write_rds()` functions
3. **Error handling**: Using `thiserror` for ergonomic errors
4. **Type safety**: Strong Rust types for R objects
5. **NA handling**: Explicit representation in type system
6. **TDD approach**: Write tests before implementation

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
