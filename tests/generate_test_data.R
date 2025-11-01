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

# Data frames
df1 <- data.frame(
  x = 1:3,
  y = c("a", "b", "c"),
  z = c(TRUE, FALSE, TRUE),
  stringsAsFactors = FALSE
)
saveRDS(df1, file.path(output_dir, "dataframe_simple.rds"))

# Data frame with different column types
df2 <- data.frame(
  int_col = c(1L, 2L, 3L, 4L),
  real_col = c(1.1, 2.2, 3.3, 4.4),
  char_col = c("foo", "bar", "baz", "qux"),
  logical_col = c(TRUE, FALSE, TRUE, FALSE),
  stringsAsFactors = FALSE
)
saveRDS(df2, file.path(output_dir, "dataframe_mixed.rds"))

# Data frame with row names
df3 <- data.frame(
  name = c("Alice", "Bob", "Charlie"),
  age = c(25L, 30L, 35L),
  stringsAsFactors = FALSE
)
rownames(df3) <- c("person1", "person2", "person3")
saveRDS(df3, file.path(output_dir, "dataframe_rownames.rds"))

# S3 objects
# Simple S3 object (custom class on a list)
my_obj <- list(x = 1:3, y = "test", z = TRUE)
class(my_obj) <- "my_custom_class"
saveRDS(my_obj, file.path(output_dir, "s3_simple.rds"))

# S3 object with multiple classes (class inheritance)
my_obj2 <- list(value = 137, name = "fine_structure")
class(my_obj2) <- c("special_class", "base_class")
saveRDS(my_obj2, file.path(output_dir, "s3_multi_class.rds"))

# S3 object on a vector
my_vec <- c(10, 20, 30)
class(my_vec) <- "custom_vector"
attr(my_vec, "description") <- "A custom vector class"
saveRDS(my_vec, file.path(output_dir, "s3_vector.rds"))

# Factor (built-in S3 class)
fac <- factor(c("low", "high", "medium", "low", "high"))
saveRDS(fac, file.path(output_dir, "factor_simple.rds"))

# Ordered factor
ord_fac <- ordered(c("low", "medium", "high", "low"),
                   levels = c("low", "medium", "high"))
saveRDS(ord_fac, file.path(output_dir, "factor_ordered.rds"))

# S4 objects
# Define a simple S4 class - Animal
setClass("Animal",
         slots = c(species = "character",
                   age = "numeric",
                   habitat = "character"))

# Create an instance
tiger <- new("Animal",
             species = "Tiger",
             age = 5,
             habitat = "Rainforest")
saveRDS(tiger, file.path(output_dir, "s4_simple.rds"))

# S4 class with inheritance - Bird extends Animal
setClass("Bird",
         contains = "Animal",
         slots = c(wingspan = "numeric",
                   can_fly = "logical"))

parrot <- new("Bird",
              species = "Macaw",
              age = 3,
              habitat = "Tropical Forest",
              wingspan = 1.2,
              can_fly = TRUE)
saveRDS(parrot, file.path(output_dir, "s4_inheritance.rds"))

# S4 object with various slot types - Aquarium
setClass("Aquarium",
         slots = c(temperatures = "numeric",
                   fish_species = "character",
                   saltwater = "logical"))

tank <- new("Aquarium",
            temperatures = c(24.5, 25.0, 24.8),
            fish_species = c("clownfish", "tang", "angelfish"),
            saltwater = TRUE)
saveRDS(tank, file.path(output_dir, "s4_complex.rds"))

# Language objects (unevaluated expressions/calls)
# Simple function call: sum(1, 2, 3)
lang_simple <- quote(sum(1, 2, 3))
saveRDS(lang_simple, file.path(output_dir, "lang_simple.rds"))

# Function call with variables: mean(x, na.rm = TRUE)
lang_with_args <- quote(mean(x, na.rm = TRUE))
saveRDS(lang_with_args, file.path(output_dir, "lang_with_args.rds"))

# Nested expression: sqrt(sum(x, y))
lang_nested <- quote(sqrt(sum(x, y)))
saveRDS(lang_nested, file.path(output_dir, "lang_nested.rds"))

# Expression vectors (EXPRSXP) - collections of unevaluated expressions
# Typically the result of parse()

# Simple expression vector: a single expression
expr_single <- parse(text = "x + 1")
saveRDS(expr_single, file.path(output_dir, "expr_single.rds"))

# Multiple expressions
expr_multiple <- parse(text = c("x + 1", "y * 2", "z / 3"))
saveRDS(expr_multiple, file.path(output_dir, "expr_multiple.rds"))

# Empty expression vector
expr_empty <- expression()
saveRDS(expr_empty, file.path(output_dir, "expr_empty.rds"))

# Expression vector with function calls
expr_calls <- parse(text = c("mean(x)", "sum(y)", "sd(z)"))
saveRDS(expr_calls, file.path(output_dir, "expr_calls.rds"))

# Complex expression with nested calls
# Using simpler expression to avoid REFSXP (reference tracking)
expr_complex <- parse(text = "sqrt(x + y)")
saveRDS(expr_complex, file.path(output_dir, "expr_complex.rds"))

# Expression vector created with expression() function
expr_manual <- expression(a + b, c * d, sqrt(e))
saveRDS(expr_manual, file.path(output_dir, "expr_manual.rds"))

# Formulas (special language objects with class "formula")
# Simple formula: y ~ x
formula_simple <- y ~ x
saveRDS(formula_simple, file.path(output_dir, "formula_simple.rds"))

# Formula with multiple predictors: y ~ x + z
formula_multiple <- y ~ x + z
saveRDS(formula_multiple, file.path(output_dir, "formula_multiple.rds"))

# Formula with interaction: y ~ x * z (expands to y ~ x + z + x:z)
formula_interaction <- y ~ x * z
saveRDS(formula_interaction, file.path(output_dir, "formula_interaction.rds"))

# Formula with functions: log(y) ~ sqrt(x) + I(z^2)
formula_functions <- log(y) ~ sqrt(x) + I(z^2)
saveRDS(formula_functions, file.path(output_dir, "formula_functions.rds"))

# Two-sided formula with no intercept: y ~ x - 1
formula_no_intercept <- y ~ x - 1
saveRDS(formula_no_intercept, file.path(output_dir, "formula_no_intercept.rds"))

# One-sided formula: ~ x + y
formula_one_sided <- ~ x + y
saveRDS(formula_one_sided, file.path(output_dir, "formula_one_sided.rds"))

cat("Test RDS files generated successfully in", output_dir, "\n")
