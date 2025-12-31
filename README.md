# rds2rust

A pure Rust library for reading and writing R's RDS (R Data Serialization) files without requiring an R runtime. Inspired by [rds2cpp](https://github.com/LTLA/rds2cpp), which provides similar functionality with a C++ implementation.


[![Crates.io](https://img.shields.io/crates/v/rds2rust.svg)](https://crates.io/crates/rds2rust)
[![Documentation](https://docs.rs/rds2rust/badge.svg)](https://docs.rs/rds2rust)
[![License](https://img.shields.io/crates/l/rds2rust.svg)](LICENSE)

## Features

- **Pure Rust implementation** - No R runtime required
- **Broad RDS format support** - Reads and writes core R object types
- **Memory efficient** - Optimized with string interning, compact attributes, and object deduplication
- **Automatic compression** - Transparent gzip compression/decompression
- **Type safe** - Strong Rust types for all R objects
- **Zero-copy where possible** - Efficient parsing and serialization
- **Thread safe** - Safe to use concurrently from multiple threads

### Supported R Types

- **Primitive types**: NULL, integers, doubles, logicals, characters, raw bytes, complex numbers
- **Collections**: vectors, lists, pairlists, expression vectors
- **Data structures**: data frames, matrices, factors (ordered and unordered)
- **Object-oriented**: S3 objects, S4 objects with slots
- **Language objects**: formulas, unevaluated expressions, function calls
- **Functions**: closures, environments, promises, special/builtin functions
- **Advanced**: reference tracking (REFSXP), ALTREP compact sequences

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rds2rust = "0.1"
```

## Quick Start

### Reading an RDS file

```rust
use rds2rust::{read_rds, RObject};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read RDS file (automatically decompresses if gzipped)
    let data = fs::read("data.rds")?;
    let obj = read_rds(&data)?;

    // Pattern match on R object type
    match obj {
        RObject::DataFrame(df) => {
            println!("Data frame with {} columns", df.columns.len());

            // Access a specific column
            if let Some(RObject::Real(values)) = df.columns.get("temperature") {
                println!("Temperature values: {:?}", values);
            }
        }
        RObject::Integer(vec) => {
            println!("Integer vector: {:?}", vec);
        }
        _ => println!("Other R object type"),
    }

    Ok(())
}
```

### Writing an RDS file

```rust
use rds2rust::{write_rds, RObject};
use std::fs;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an R object (e.g., a character vector)
    let obj = RObject::Character(vec![
        Arc::from("hello"),
        Arc::from("world"),
    ].into());

    // Serialize to RDS format (automatically gzip compressed)
    let rds_data = write_rds(&obj)?;

    // Write to file
    fs::write("output.rds", rds_data)?;

    Ok(())
}
```

### Working with Data Frames

```rust
use rds2rust::{read_rds, RObject};
use std::sync::Arc;

// Read a data frame
let data = std::fs::read("iris.rds")?;
let obj = read_rds(&data)?;

if let RObject::DataFrame(df) = obj {
    // Access columns by name
    let sepal_length = df.columns.get("Sepal.Length");
    let species = df.columns.get("Species");

    // Access row names
    println!("First row name: {}", df.row_names[0]);

    // Iterate over columns
    for (name, values) in &df.columns {
        println!("Column: {}", name);
    }
}
```

## Large-File Extraction (Streaming-Oriented)

For very large files, you can extract vectors without materializing the whole object in memory.
The `rds-extract` CLI writes one file per vector plus an optional JSON manifest.

### WASM Support (Large Files)

The WASM path uses async input, a Blob-backed chunk source, and worker-friendly helpers.
Decompression uses a size-based strategy:

- **<500MB**: in-memory buffer
- **500MB–10GB**: Blob-backed chunked reads
- **>10GB**: streaming mode (future)

See `docs/wasm_decompression.md` for the JS helper, worker wrapper, and validation targets.

### CLI

```bash
rds-extract data.rds out/ data.matrix meta.data --budget-mb 512 --manifest manifest.json
rds-extract data.rds out/ --object-path data --manifest manifest.json
rds-extract data.rds out/ --object-kind dataframe --object-path data
rds-extract convert data.rds out/ --object-kind dataframe --object-path data
rds-extract convert data.rds out/ --object-kind dataframe --chunked
rds-extract convert data.rds out/ --object-kind sparse-matrix --object-path data.matrix --chunked --chunk-size-mb 4
```

If no paths are provided, the root object is extracted. Use `--object-path` to expand higher-level
objects (data.frames, dense matrices, sparse matrices, lists) into their component vectors.
Use `--object-kind` to enforce the expected object type and emit a clearer error on mismatch.
Use `--chunked` to avoid mapping the full decompressed stream in memory; it trades some
performance for a lower steady-state memory footprint on huge files.
When field names contain dots (e.g., `slot.value`), use quoted segments:
`data["slot.value"]`.
Streaming is the default and avoids materializing large lazy vectors; it streams spans directly
from the backing store. Use `--no-streaming` to force materialization if needed. Use
`--chunk-size-mb` to cap per-read buffer size when streaming. Streaming is best paired with
`--chunked` to avoid mmap'ing large decompressed streams.

### WASM Extraction APIs

WASM exposes async extraction helpers that return JS typed arrays or call a callback per chunk:

- `extract_vector_to_js(obj, source, path) -> JsValue`
- `extract_vector_chunked(obj, source, path, chunk_size, callback)`

### Raw Dump Format

Each output file contains:
- Header: `RDS2VEC1` + version + kind + endian + reserved + length(u64) + elem_size(u32)
- Payload:
  - Numeric/logical/complex/raw: element bytes (big-endian for numeric types)
  - Character: repeated records of `i32 length` + UTF-8 bytes

The manifest JSON lists each extracted vector with its `path`, `file`, `kind`, `length`,
`elem_size`, and `endian`, plus a top-level `object_kind`. This allows a reader to map files
back to R object paths.

### Manifest Versioning

The manifest includes a top-level `version` field. Version 1 is the initial schema:
`{ "version": 1, "object_kind": "...", "vectors": [...], "missing": [...] }`. Future schema
changes will increment this number and preserve backward compatibility where possible.

### Reader Guidance

Recommended reader flow:
1. Load the manifest JSON.
2. For each entry, open the referenced `.rdsvec` file.
3. Validate the header (`RDS2VEC1`, version, kind, endian, length, elem_size).
4. Read payload:
   - Numeric/logical/complex/raw: fixed-size element bytes.
   - Character: repeated `i32 length` + UTF-8 bytes records.

Example validation helper:

```rust
use rds2rust::{read_extraction_manifest, validate_vector_file_header};

let manifest = read_extraction_manifest("out/manifest.json")?;
for entry in &manifest.vectors {
    let path = format!("out/{}", entry.file);
    validate_vector_file_header(&path, entry)?;
}
```

### High-Level Conversion Helpers

Library callers can use the higher-level conversion helpers to expand objects and emit raw dumps
plus manifests without manually enumerating paths:

```rust
use rds2rust::{
    extract_object_to_raw_files_with_input_streaming,
    extract_object_to_raw_files_with_kind_and_input_streaming,
    ChunkedRdsSource, ObjectKind, ParseConfig,
};

let source = ChunkedRdsSource::from_path("data.rds")?;
let obj = rds2rust::read_rds_with_input(&source, ParseConfig::for_trusted_large_file())?;
let output = extract_object_to_raw_files_with_input_streaming(
    &obj,
    &source,
    "data",
    Some(4 * 1024 * 1024),
    std::path::Path::new("out"),
    Some("manifest.json"),
)?;
let output = extract_object_to_raw_files_with_kind_and_input_streaming(
    &obj,
    &source,
    "data",
    ObjectKind::DataFrame,
    Some(4 * 1024 * 1024),
    std::path::Path::new("out"),
    Some("manifest.json"),
)?;
```

### Chunked Read APIs

If you want chunked reads in library code, use the chunked path helpers:

```rust
use rds2rust::{read_rds_from_path_chunked, ParseConfig};

let obj = read_rds_from_path_chunked("data.rds")?;
let obj = rds2rust::read_rds_from_path_chunked_with_config(
    "data.rds",
    ParseConfig::for_trusted_large_file(),
)?;
```

Lazy metadata parsing with chunked reads:

```rust
use rds2rust::read_rds_lazy_from_path_chunked;

let obj = read_rds_lazy_from_path_chunked("data.rds")?;
assert!(!obj.is_fully_loaded());
```

### Working with Factors

```rust
use rds2rust::{read_rds, RObject};

let data = std::fs::read("factor.rds")?;
let obj = read_rds(&data)?;

if let RObject::Factor(factor) = obj {
    // Check if it's an ordered factor
    if factor.ordered {
        println!("Ordered factor with {} levels", factor.levels.len());
    }

    // Get level labels
    for level in &factor.levels {
        println!("Level: {}", level);
    }

    // Get values (1-based indices into levels)
    for &index in &factor.values {
        if index > 0 && index <= factor.levels.len() as i32 {
            let level = &factor.levels[(index - 1) as usize];
            println!("Value: {}", level);
        }
    }
}
```

### Working with S3/S4 Objects

```rust
use rds2rust::{read_rds, RObject};
use std::sync::Arc;

let data = std::fs::read("model.rds")?;
let obj = read_rds(&data)?;

// S3 objects
if let RObject::S3Object(s3) = obj {
    println!("S3 class: {:?}", s3.class);

    // Access base object
    match s3.base.as_ref() {
        RObject::List(elements) => {
            println!("S3 object is a list with {} elements", elements.len());
        }
        _ => {}
    }

    // Access additional attributes
    if let Some(desc) = s3.attributes.get("description") {
        println!("Description: {:?}", desc);
    }
}

// S4 objects
if let RObject::S4Object(s4) = obj {
    println!("S4 class: {:?}", s4.class);

    // Access slots
    if let Some(slot_value) = s4.slots.get("data") {
        println!("Data slot: {:?}", slot_value);
    }
}
```

### Roundtrip: Read and Write

```rust
use rds2rust::{read_rds, write_rds};
use std::fs;

// Read an RDS file
let input_data = fs::read("input.rds")?;
let obj = read_rds(&input_data)?;

// Process the data...
// (modify the object as needed)

// Write back to RDS format
let output_data = write_rds(&obj)?;
fs::write("output.rds", output_data)?;

// Verify roundtrip
let obj2 = read_rds(&output_data)?;
assert_eq!(obj, obj2);
```

## Type System

The `RObject` enum represents all possible R object types:

```rust
pub enum RObject {
    Null,
    Integer(VectorData<i32>),
    Real(VectorData<f64>),
    Logical(VectorData<Logical>),
    Character(VectorData<Arc<str>>),
    Symbol(Arc<str>),
    Raw(VectorData<u8>),
    Complex(VectorData<Complex>),
    List(Vec<RObject>),
    Pairlist(Vec<PairlistElement>),
    Language { function: Box<RObject>, args: Vec<PairlistElement> },
    Expression(Vec<RObject>),
    Closure { formals: Box<RObject>, body: Box<RObject>, environment: Box<RObject> },
    Environment { enclosing: Box<RObject>, frame: Box<RObject>, hashtab: Box<RObject> },
    Promise { value: Box<RObject>, expression: Box<RObject>, environment: Box<RObject> },
    Special { name: Arc<str> },
    Builtin { name: Arc<str> },
    Bytecode { code: Box<RObject>, constants: Box<RObject>, expr: Box<RObject> },
    DataFrame(Box<DataFrameData>),
    Factor(Box<FactorData>),
    S3Object(Box<S3ObjectData>),
    S4Object(Box<S4ObjectData>),
    Namespace(Vec<Arc<str>>),
    GlobalEnv,
    BaseEnv,
    EmptyEnv,
    MissingArg,
    UnboundValue,
    Shared(Arc<RwLock<RObject>>),
    WithAttributes { object: Box<RObject>, attributes: Attributes },
}
```

### Special Values

R's special values are represented as:

- **NA (integers)**: `RObject::NA_INTEGER` constant (`i32::MIN`)
- **NA (logicals)**: `Logical::Na` enum variant
- **NA (real)**: Check with `f64::is_nan()`
- **Inf/-Inf**: `f64::INFINITY` and `f64::NEG_INFINITY`
- **NaN**: `f64::NAN`

## Memory Optimizations

rds2rust includes several memory optimizations for efficient data processing:

1. **String Interning** - All strings use `Arc<str>` for automatic deduplication
2. **Boxed Large Variants** - Large enum variants are boxed to reduce memory overhead
3. **Compact Attributes** - SmallVec stores 0-2 attributes inline without heap allocation
4. **Object Deduplication** - Identical objects are automatically shared during parsing

These optimizations provide **20-50% memory reduction** for typical RDS files while maintaining zero API overhead.

## Performance Tips

### Reading Large Files

```rust
use rds2rust::read_rds;
use std::fs::File;
use std::io::Read;

// For very large files, read in chunks if needed
let mut file = File::open("large.rds")?;
let mut buffer = Vec::new();
file.read_to_end(&mut buffer)?;

let obj = read_rds(&buffer)?;
```

### Reusing Parsed Objects

```rust
use std::sync::Arc;
use rds2rust::RObject;

// Wrap in Arc for cheap cloning
let obj = Arc::new(read_rds(&data)?);

// Clone is cheap (just increments reference count)
let obj2 = Arc::clone(&obj);
```

## Limitations

- **Write support**: All R types can be written except for some complex environment configurations
- **Compression formats**: Currently supports gzip; bzip2/xz support planned
- **ALTREP**: Reads ALTREP objects but writes them as regular vectors
- **External pointers**: Not supported (rarely used in serialized data)

## Development Status

**Current version**: 0.1.39

**Test coverage**: extensive test suite covering core R object types and roundtrips

**Completed phases**:
- ✅ All basic R types (NULL, vectors, matrices, data frames)
- ✅ All object-oriented types (S3, S4, factors)
- ✅ All language types (expressions, formulas, closures, environments)
- ✅ All special types (promises, special functions, builtin functions)
- ✅ Reference tracking and ALTREP optimization
- ✅ Complete read/write roundtrip support
- ✅ Memory optimizations (string interning, compact attributes, deduplication)

## License

Licensed under:

- MIT license ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT)

## Resources

- [RDS Format Documentation](RDS_FORMAT.md)
- [Project Plan](PROJECT_PLAN.md)
- [Test Generation Guide](tests/README.md)
- [WASM Decompression Guide](docs/wasm_decompression.md)
- [R Internals Manual](https://cran.r-project.org/doc/manuals/r-release/R-ints.html)
