#!/usr/bin/env Rscript
# Generate test RDS files for the rds2rust test suite

output_dir <- "tests/data"
dir.create(output_dir, showWarnings = FALSE, recursive = TRUE)

# Integer vectors
saveRDS(1L, file.path(output_dir, "int_single.rds"))
saveRDS(1:10, file.path(output_dir, "int_vector.rds"))
saveRDS(c(1L, NA_integer_, 3L), file.path(output_dir, "int_with_na.rds"))
saveRDS(integer(0), file.path(output_dir, "int_empty.rds"))

# Double/Real vectors
saveRDS(1.5, file.path(output_dir, "real_single.rds"))
saveRDS(c(1.1, 2.2, 3.3, 4.4), file.path(output_dir, "real_vector.rds"))
saveRDS(c(1.5, NA_real_, Inf, -Inf, NaN), file.path(output_dir, "real_special.rds"))
saveRDS(numeric(0), file.path(output_dir, "real_empty.rds"))

# Logical vectors
saveRDS(TRUE, file.path(output_dir, "logical_true.rds"))
saveRDS(FALSE, file.path(output_dir, "logical_false.rds"))
saveRDS(c(TRUE, FALSE, NA, TRUE), file.path(output_dir, "logical_vector.rds"))
saveRDS(logical(0), file.path(output_dir, "logical_empty.rds"))

# Character vectors
saveRDS("hello", file.path(output_dir, "char_single.rds"))
saveRDS(c("foo", "bar", "baz"), file.path(output_dir, "char_vector.rds"))
saveRDS(c("test", NA_character_, "string"), file.path(output_dir, "char_with_na.rds"))
saveRDS(character(0), file.path(output_dir, "char_empty.rds"))

# NULL
saveRDS(NULL, file.path(output_dir, "null.rds"))

# Raw vectors
saveRDS(as.raw(c(0x01, 0x02, 0xFF)), file.path(output_dir, "raw_vector.rds"))

# Complex vectors
saveRDS(1+2i, file.path(output_dir, "complex_single.rds"))
saveRDS(c(1+2i, 3+4i, 5+6i), file.path(output_dir, "complex_vector.rds"))

# Lists
saveRDS(list(1L, 2L, 3L), file.path(output_dir, "list_simple.rds"))
saveRDS(list(a=1, b=2, c=3), file.path(output_dir, "list_named.rds"))
saveRDS(list(), file.path(output_dir, "list_empty.rds"))

# Nested structures
saveRDS(list(x=1:5, y=c("a", "b"), z=list(nested=TRUE)),
        file.path(output_dir, "list_nested.rds"))

# Named vectors (vectors with names attribute)
x <- c(1L, 2L, 3L)
names(x) <- c("a", "b", "c")
saveRDS(x, file.path(output_dir, "int_named.rds"))

y <- c(1.5, 2.5, 3.5)
names(y) <- c("x", "y", "z")
saveRDS(y, file.path(output_dir, "real_named.rds"))

z <- c("foo", "bar", "baz")
names(z) <- c("first", "second", "third")
saveRDS(z, file.path(output_dir, "char_named.rds"))

# Matrices (with dim attribute)
mat <- matrix(1:6, nrow=2, ncol=3)
saveRDS(mat, file.path(output_dir, "matrix_int.rds"))

mat2 <- matrix(c(1.1, 2.2, 3.3, 4.4), nrow=2, ncol=2)
saveRDS(mat2, file.path(output_dir, "matrix_real.rds"))

# Matrix with dimnames
mat3 <- matrix(1:4, nrow=2, ncol=2)
dimnames(mat3) <- list(c("row1", "row2"), c("col1", "col2"))
saveRDS(mat3, file.path(output_dir, "matrix_dimnames.rds"))

cat("Test RDS files generated successfully in", output_dir, "\n")
