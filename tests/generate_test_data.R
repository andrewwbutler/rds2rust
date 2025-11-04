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

# Closures (functions) and environments
# Simple function with default parameters
simple_func <- function(x, y = 10) { x + y }
saveRDS(simple_func, file.path(output_dir, "closure_simple.rds"))

# Function with no parameters (closure with custom environment)
make_counter <- function() {
    count <- 0
    function() {
        count <<- count + 1
        count
    }
}
counter <- make_counter()
saveRDS(counter, file.path(output_dir, "closure_with_env.rds"))

# Standalone environment
env <- new.env()
env$x <- 42
env$y <- "hello"
saveRDS(env, file.path(output_dir, "environment_simple.rds"))

# Reference tracking test cases
# These test REFSXP (reference tracking) functionality

# Shared reference: same vector appears multiple times in a list
shared_vec <- c(1, 2, 3, 4, 5)
list_with_shared <- list(a = shared_vec, b = shared_vec, c = shared_vec)
saveRDS(list_with_shared, file.path(output_dir, "ref_shared_vector.rds"))

# Shared list: same list appears multiple times
shared_list <- list(x = 1:3, y = c("a", "b"))
nested_with_shared <- list(
  first = shared_list,
  second = shared_list,
  third = list(inner = shared_list)
)
saveRDS(nested_with_shared, file.path(output_dir, "ref_shared_list.rds"))

# Circular reference: list that contains itself
# Note: R doesn't allow simple circular references in normal construction,
# but we can create them using environments or careful manipulation
circular_list <- list(value = 42)
# We can't easily create true circular references in base R without using
# environments, so we'll create a structure that has repeated references
# which will use REFSXP

# Self-referential structure using repeated objects
obj_a <- list(name = "A", data = 1:10)
obj_b <- list(name = "B", ref_to_a = obj_a)
obj_c <- list(name = "C", ref_to_a = obj_a, ref_to_b = obj_b)
complex_shared <- list(a = obj_a, b = obj_b, c = obj_c)
saveRDS(complex_shared, file.path(output_dir, "ref_complex_shared.rds"))

# Expression with shared components
shared_expr <- quote(x + y)
expr_with_shared <- list(
  expr1 = shared_expr,
  expr2 = shared_expr,
  wrapped = list(inner = shared_expr)
)
saveRDS(expr_with_shared, file.path(output_dir, "ref_shared_expression.rds"))

# Large repeated structure to ensure REFSXP is used
large_vec <- 1:1000
list_with_large <- list(
  copy1 = large_vec,
  copy2 = large_vec,
  copy3 = large_vec,
  copy4 = large_vec,
  copy5 = large_vec
)
saveRDS(list_with_large, file.path(output_dir, "ref_large_shared.rds"))

# ALTREP reference tracking tests
# These test compact_intseq (ALTREP integer sequences) with reference tracking

# Simple reference test: 3 copies of 1:10
simple_ref_vec <- 1:10
simple_ref_list <- list(simple_ref_vec, simple_ref_vec, simple_ref_vec)
saveRDS(simple_ref_list, file.path(output_dir, "ref_altrep_simple.rds"))

# Two copies of ALTREP sequence
two_vec <- 1:10
two_list <- list(two_vec, two_vec)
saveRDS(two_list, file.path(output_dir, "ref_altrep_two_copies.rds"))

# Three copies of ALTREP sequence
three_vec <- 1:10
three_list <- list(three_vec, three_vec, three_vec)
saveRDS(three_list, file.path(output_dir, "ref_altrep_three_copies.rds"))

# Four copies of ALTREP sequence
four_vec <- 1:10
four_list <- list(four_vec, four_vec, four_vec, four_vec)
saveRDS(four_list, file.path(output_dir, "ref_altrep_four_copies.rds"))

# Single ALTREP sequence (no references)
third_only <- 1:10
saveRDS(third_only, file.path(output_dir, "ref_altrep_single.rds"))

# Non-ALTREP vector (regular integer vector, not sequence)
non_altrep_vec <- c(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L)
non_altrep_list <- list(non_altrep_vec, non_altrep_vec, non_altrep_vec)
saveRDS(non_altrep_list, file.path(output_dir, "ref_altrep_non_sequence.rds"))

# Three shared regular (non-ALTREP) vectors
shared_regular <- c(1L, 2L, 3L, 4L, 5L)
three_shared_list <- list(shared_regular, shared_regular, shared_regular)
saveRDS(three_shared_list, file.path(output_dir, "ref_altrep_three_shared.rds"))

# Promises (PROMSXP)
# Promises are created through lazy evaluation - they're not typically
# user-facing objects. We can create them through delayedAssign or by
# capturing function arguments

# Simple promise: delayedAssign creates an unevaluated promise
delayedAssign("promise_simple", x + 1)
# Note: We can't directly save a promise, as saveRDS will evaluate it
# We need to save the environment containing the promise
env_with_promise <- new.env()
delayedAssign("value", 2 + 2, eval.env = env_with_promise, assign.env = env_with_promise)
saveRDS(env_with_promise, file.path(output_dir, "promise_in_env.rds"))

# Special functions (SPECIALSXP)
# Special primitive functions with special evaluation rules
special_if <- `if`
saveRDS(special_if, file.path(output_dir, "special_if.rds"))

special_for <- `for`
saveRDS(special_for, file.path(output_dir, "special_for.rds"))

special_while <- `while`
saveRDS(special_while, file.path(output_dir, "special_while.rds"))

special_function <- `function`
saveRDS(special_function, file.path(output_dir, "special_function.rds"))

special_bracket <- `[`
saveRDS(special_bracket, file.path(output_dir, "special_bracket.rds"))

# Builtin functions (BUILTINSXP)
# Builtin primitive functions evaluated normally
builtin_sum <- sum
saveRDS(builtin_sum, file.path(output_dir, "builtin_sum.rds"))

builtin_c <- c
saveRDS(builtin_c, file.path(output_dir, "builtin_c.rds"))

builtin_plus <- `+`
saveRDS(builtin_plus, file.path(output_dir, "builtin_plus.rds"))

builtin_sqrt <- sqrt
saveRDS(builtin_sqrt, file.path(output_dir, "builtin_sqrt.rds"))

builtin_length <- length
saveRDS(builtin_length, file.path(output_dir, "builtin_length.rds"))

builtin_min <- min
saveRDS(builtin_min, file.path(output_dir, "builtin_min.rds"))

# Symbol table test: List with attributes using REFSXP in TAG positions
# This tests that REFSXP in pairlist TAG positions are correctly looked up
# in the symbol table, not the ref table. The test creates a structure where
# multiple objects share the same attribute name symbols via REFSXP.
# Create multiple lists with the same attribute name to trigger REFSXP usage
list1 <- list(a = 1, b = 2, c = 3)
list2 <- list(x = 10, y = 20, z = 30)
list3 <- list(p = 100, q = 200, r = 300)
# Wrap them in a parent list - R will use REFSXP for repeated "names" symbols
symbol_table_test <- list(first = list1, second = list2, third = list3)
saveRDS(symbol_table_test, file.path(output_dir, "symbol_table_test.rds"))

# ALTREP test: Compact integer sequence
# R uses ALTREP to efficiently represent sequences like 1:n
# This should be serialized as ALTREP type 249 (0xF9) or 238 (0xEE)
altrep_intseq <- 1:1000
saveRDS(altrep_intseq, file.path(output_dir, "altrep_intseq.rds"))

# ALTREP test: Compact real sequence
altrep_realseq <- seq(1.0, 1000.0, by = 1.0)
saveRDS(altrep_realseq, file.path(output_dir, "altrep_realseq.rds"))

# ALTREP test: List containing ALTREP sequence
altrep_in_list <- list(
    seq = 1:100,
    data = c(1.5, 2.5, 3.5),
    another_seq = 50:150
)
saveRDS(altrep_in_list, file.path(output_dir, "altrep_in_list.rds"))

# Regular integer vector (no ALTREP) for comparison
regular_int <- c(1L, 2L, 3L, 4L, 5L)
saveRDS(regular_int, file.path(output_dir, "regular_int.rds"))

cat("Test RDS files generated successfully in", output_dir, "\n")
