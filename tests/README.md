# Test Data

## Generating Test Files

To generate test RDS files, you need R installed on your system. Run:

```bash
Rscript tests/generate_test_data.R
```

This will create RDS files in `tests/data/` directory.

## Manual Test Data

If R is not available, you can use the manually created test files or install R:

**On macOS:**
```bash
brew install r
```

**On Ubuntu/Debian:**
```bash
sudo apt-get install r-base
```

**On Windows:**
Download from https://cran.r-project.org/

## Test File Descriptions

The generated test files include:

- **Integer vectors**: Single values, vectors, NA values, empty vectors
- **Double vectors**: Single values, vectors, special values (NA, Inf, NaN), empty vectors
- **Logical vectors**: TRUE, FALSE, NA, mixed vectors, empty vectors
- **Character vectors**: Single strings, vectors, NA values, empty vectors
- **NULL**: The NULL object
- **Raw vectors**: Byte arrays
- **Complex vectors**: Complex numbers
- **Lists**: Simple lists, named lists, nested lists

Each test file is saved in RDS format version 2 (the most common format).
