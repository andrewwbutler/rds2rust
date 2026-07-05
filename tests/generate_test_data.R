#!/usr/bin/env Rscript
# Generate test RDS files for the rds2rust test suite

output_dir <- "tests/data"
dir.create(output_dir, showWarnings = FALSE, recursive = TRUE)

# Packages used for sparse matrices and S4 classes
suppressMessages(library(Matrix))

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

# List with attributes at end-of-stream to exercise EOF-tolerant parsing
attr_eof_list <- structure(
  list(
    info = list(id = 1L),
    payload = list(values = as.numeric(1:3)),
    meta = list(package = "examplepkg")
  ),
  names = c("info", "payload", "meta"),
  class = c("attr_eof_test"),
  tools = list(source = "generator")
)
saveRDS(attr_eof_list, file.path(output_dir, "attr_at_eof.rds"))

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

# Sparse matrix (dgCMatrix) with dimnames
set.seed(123)
sparse_mat <- Matrix::rsparsematrix(4, 4, density = 0.35)
dimnames(sparse_mat) <- list(paste0("r", 1:4), paste0("c", 1:4))
saveRDS(sparse_mat, file.path(output_dir, "sparse_dimnames.rds"))

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

# Generic closure bundles with environments (used for cross-language roundtrip)
# Minimal closure with an environment and simple body
make_minimal_closure <- function() {
    seed <- 1
    function(x) x + seed
}
minimal_closure <- make_minimal_closure()
saveRDS(minimal_closure, file.path(output_dir, "test_minimal_closure.rds"))

# Single closure wrapped in a list to exercise nested structures
make_scaler <- function(factor) {
    function(x) x * factor
}
scaled_fn <- make_scaler(2)
single_wrapper <- list(fn = scaled_fn, label = "scale_by_2")
saveRDS(single_wrapper, file.path(output_dir, "command_one_real.rds"))

# Multiple closures in a list to exercise repeated references
make_offset <- function(offset) {
    function(x) x + offset
}
offset_a <- make_offset(3)
offset_b <- make_offset(7)
closures_list <- list(a = offset_a, b = offset_b, again = offset_a)
saveRDS(closures_list, file.path(output_dir, "commands_real_1.rds"))

# Nested closures with shared environments
make_pair <- function(mult, add) {
    env <- new.env(parent = emptyenv())
    env$mult <- mult
    env$add <- add
    list(
        mult = function(x) x * env$mult,
        add = function(x) x + env$add,
        env = env
    )
}
pair_obj <- make_pair(4, 9)
saveRDS(pair_obj, file.path(output_dir, "commands_real_2.rds"))

# Real namespace functions inside an S4 command bundle.
# This intentionally mirrors integration-style command payloads while avoiding
# optional external package dependencies.
setClass("TestCommand", slots = c(dummy = "logical"), prototype = list(dummy = FALSE))
setClass("TestSeurat", slots = c(dummy = "logical"), prototype = list(dummy = FALSE))
test_cmd <- new("TestCommand")
attr(test_cmd, "name") <- "FindVariableFeatures.RNA"
attr(test_cmd, "params") <- list(
    selection.method = "vst",
    mean.function = stats::median,
    dispersion.function = stats::sd,
    nfeatures = 2000
)
test_with_real_functions <- new("TestSeurat")
attr(test_with_real_functions, "data") <- list(x = 1:10)
attr(test_with_real_functions, "commands") <- list(test_cmd)
saveRDS(
    test_with_real_functions,
    file.path(output_dir, "test_with_real_functions.rds")
)

# More realistic closure structure with attributes and metadata
make_pipeline <- function(scale, shift) {
    inner_env <- new.env(parent = emptyenv())
    inner_env$scale <- scale
    inner_env$shift <- shift
    function(x) (x * inner_env$scale) + inner_env$shift
}
pipeline <- make_pipeline(1.5, 2.0)
realistic_obj <- structure(
    list(
        step = pipeline,
        params = list(scale = 1.5, shift = 2.0),
        info = "pipeline"
    ),
    class = "pipeline_bundle"
)
saveRDS(realistic_obj, file.path(output_dir, "command_realistic.rds"))

# With-attributes language object
lang_with_attr <- quote(x + y)
attr(lang_with_attr, "note") <- "example"
saveRDS(lang_with_attr, file.path(output_dir, "withattr_language.rds"))

# With-attributes closure
attr_closure <- function(x) x + 1
attr(attr_closure, "note") <- "example"
saveRDS(attr_closure, file.path(output_dir, "withattr_closure.rds"))

# Standalone environment
env <- new.env()
env$x <- 42
env$y <- "hello"
saveRDS(env, file.path(output_dir, "environment_simple.rds"))

# Persistent objects (PERSISTSXP) written via serialize(refhook=).
# This mimics R's lazy-load databases (e.g. help/<pkg>.rdb), where srcfile
# environments are persisted as "env::N" strings by a ref hook. The ref hook
# takes precedence over back-references, so the second occurrence of the same
# environment is re-persisted as another PERSISTSXP entry; the trailing
# string verifies stream alignment.
persist_env <- new.env()
persist_obj <- list(
  first = persist_env,
  second = persist_env,
  after = "still-aligned"
)
con <- file(file.path(output_dir, "persistsxp.rds"), "wb")
serialize(persist_obj, con, refhook = function(e) "env::1")
close(con)

# Persisted environment inside an attribute, like the srcref/srcfile
# attributes on Rd objects stored in help databases.
persist_attr_obj <- structure(
  list("payload"),
  srcenv = persist_env,
  tail_attr = "tail"
)
con <- file(file.path(output_dir, "persistsxp_attr.rds"), "wb")
serialize(persist_attr_obj, con, refhook = function(e) "env::1")
close(con)

# A ref hook may return more than one string per object.
con <- file(file.path(output_dir, "persistsxp_multi.rds"), "wb")
serialize(
  list(env = persist_env, after = "multi-aligned"),
  con,
  refhook = function(e) c("env", "extra", "names")
)
close(con)


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

# ALTREP wrapper tests: These test wrap_real and wrap_int wrappers
# Create a matrix - R may use ALTREP wrappers for efficient storage
altrep_matrix_real <- matrix(rnorm(200 * 30), nrow = 200, ncol = 30)
saveRDS(altrep_matrix_real, file.path(output_dir, "altrep_matrix_real.rds"))

# Matrix with attributes (dimnames) - common in data analysis packages
altrep_matrix_with_dimnames <- matrix(rnorm(100 * 10), nrow = 100, ncol = 10)
rownames(altrep_matrix_with_dimnames) <- paste0("row_", 1:100)
colnames(altrep_matrix_with_dimnames) <- paste0("col_", 1:10)
saveRDS(altrep_matrix_with_dimnames, file.path(output_dir, "altrep_matrix_dimnames.rds"))

# Force ALTREP wrapper using .Internal(wrap_meta(...))
# This creates actual wrap_real/wrap_int ALTREP objects as used by some packages
if (getRversion() >= "3.5.0") {
  tryCatch({
    # Create real vector and wrap it
    real_data <- rnorm(1000)
    wrapped_real <- .Internal(wrap_meta(real_data, NULL, NULL))
    saveRDS(wrapped_real, file.path(output_dir, "altrep_wrap_real.rds"))

    # Create integer vector and wrap it
    int_data <- sample(1L:1000L, 500, replace = TRUE)
    wrapped_int <- .Internal(wrap_meta(int_data, NULL, NULL))
    saveRDS(wrapped_int, file.path(output_dir, "altrep_wrap_int.rds"))

    # Wrapped matrix (common in analysis packages)
    matrix_data <- matrix(rnorm(100 * 10), nrow = 100, ncol = 10)
    rownames(matrix_data) <- paste0("row_", 1:100)
    colnames(matrix_data) <- paste0("col_", 1:10)
    wrapped_matrix <- .Internal(wrap_meta(matrix_data, NULL, NULL))
    saveRDS(wrapped_matrix, file.path(output_dir, "altrep_wrap_matrix.rds"))
  }, error = function(e) {
    message("Note: wrap_meta not available in this R version (OK)")
  })
}

# Bytecode test: Compiled function
# R compiles functions to bytecode for performance
# Use compiler::cmpfun() to explicitly compile a function
library(compiler)

# Simple function
simple_func <- function(x) {
    x + 1
}

# Compile it to bytecode
compiled_func <- cmpfun(simple_func)
saveRDS(compiled_func, file.path(output_dir, "bytecode_func.rds"))

# Function with bytecode in a list
func_list <- list(
    name = "my_function",
    func = compiled_func,
    data = c(1, 2, 3)
)
saveRDS(func_list, file.path(output_dir, "bytecode_in_list.rds"))

# Regular (uncompiled) function for comparison
uncompiled_func <- simple_func
saveRDS(uncompiled_func, file.path(output_dir, "uncompiled_func.rds"))

# ==============================================================================
# Advanced serialization format tests
# ==============================================================================

# Test compact 3-byte CHARSXP length encoding
# Note: This is an R-internal optimization that may or may not trigger
# We create various string patterns to potentially trigger compact encoding
compact_strings <- c(
  "ExampleObject",  # 13 characters
  "package",        # 7 characters
  "namespace",      # 9 characters
  "environment"     # 11 characters
)
saveRDS(compact_strings, file.path(output_dir, "compact_strings.rds"))

# Test SYMSXP (symbols) in character vectors
# Symbols can appear in character vectors in some R serialization contexts
# Create a structure that might serialize symbols
sym_vec <- c("x", "y", "z")
# Add as symbols by using them in a formula context
formula_with_syms <- y ~ x + z
saveRDS(formula_with_syms, file.path(output_dir, "formula_with_symbols.rds"))

# Test nested character vectors (unusual but can occur)
# Create a list containing character vectors at multiple levels
nested_chars <- list(
  outer = c("level1_a", "level1_b"),
  inner = list(
    nested_vec = c("level2_a", "level2_b", "level2_c"),
    deep = list(
      deep_vec = c("level3_a", "level3_b")
    )
  )
)
saveRDS(nested_chars, file.path(output_dir, "nested_char_vectors.rds"))

# Test character vector with various element types
# This tests the generic object handling in STRSXP parsing
mixed_attr_list <- list(
  strings = c("a", "b", "c"),
  numbers = 1:5,
  reals = c(1.1, 2.2, 3.3)
)
saveRDS(mixed_attr_list, file.path(output_dir, "mixed_types_list.rds"))

# Test S3 object used as attributes container
# Create an S3 object with rich attributes
s3_with_attrs <- structure(
  list(value = 42),
  class = "custom_class",
  metadata = "test",
  version = 1L,
  timestamp = "2024-01-01"
)
saveRDS(s3_with_attrs, file.path(output_dir, "s3_rich_attributes.rds"))

# Test namespace-related structures
# Create a function from a package namespace to potentially trigger namespace serialization
# Note: Actual namespace serialization requires package context
pkg_function <- stats::median
saveRDS(pkg_function, file.path(output_dir, "package_function.rds"))

# Test multiple pseudo-types in one structure
# Create a complex structure that exercises various pseudo-types
complex_structure <- list(
  func = compiled_func,           # May contain bytecode (BCODESXP, BCREPREF, BCREPDEF)
  env = env_with_promise,        # Contains promises (PROMSXP)
  special = special_if,          # Special function (SPECIALSXP)
  builtin = builtin_sum,         # Builtin function (BUILTINSXP)
  pkg_func = stats::median       # Package function (may have namespace refs)
)
saveRDS(complex_structure, file.path(output_dir, "complex_pseudo_types.rds"))

# Test large integer vector (to test non-CHARSXP elements in STRSXP contexts)
large_int_vec <- 1:1000
list_with_large_int <- list(
  name = "large_vector",
  data = large_int_vec,
  size = length(large_int_vec)
)
saveRDS(list_with_large_int, file.path(output_dir, "list_with_large_int.rds"))

# Test REFSXP with various reference distances
# Create multiple shared references to test reference table
shared_obj <- list(data = 1:100, meta = "shared")
multi_ref_structure <- list(
  ref1 = shared_obj,
  middle = list(
    ref2 = shared_obj,
    other = "data"
  ),
  ref3 = shared_obj,
  deep = list(
    nested = list(
      ref4 = shared_obj
    )
  )
)
saveRDS(multi_ref_structure, file.path(output_dir, "multi_level_refs.rds"))

# Test attribute edge cases
# NULL attributes
obj_null_attrs <- structure(c(1, 2, 3))
saveRDS(obj_null_attrs, file.path(output_dir, "no_attributes.rds"))

# Empty character vector as attribute
obj_empty_char_attr <- structure(
  c(1, 2, 3),
  names = character(0)
)
saveRDS(obj_empty_char_attr, file.path(output_dir, "empty_char_attribute.rds"))

# Test WithAttributes wrapping various types
# Integer with attributes
int_with_custom_attrs <- structure(
  1:10,
  custom_attr = "metadata",
  dimension_info = c(2, 5)
)
saveRDS(int_with_custom_attrs, file.path(output_dir, "int_with_custom_attrs.rds"))

# Real with attributes
real_with_custom_attrs <- structure(
  seq(1.0, 10.0, by=0.5),
  units = "meters",
  precision = 0.5
)
saveRDS(real_with_custom_attrs, file.path(output_dir, "real_with_custom_attrs.rds"))

# Character with attributes
char_with_custom_attrs <- structure(
  c("x", "y", "z"),
  encoding = "UTF-8",
  origin = "user_input"
)
saveRDS(char_with_custom_attrs, file.path(output_dir, "char_with_custom_attrs.rds"))

# Test compact encoding edge cases
# Very short string (< 256 bytes)
short_str <- "x"
saveRDS(short_str, file.path(output_dir, "string_very_short.rds"))

# Medium string (256-65535 bytes)
medium_str <- paste(rep("a", 500), collapse="")
saveRDS(medium_str, file.path(output_dir, "string_medium.rds"))

# Long string (> 65535 bytes)
long_str <- paste(rep("abcdefghij", 7000), collapse="")
saveRDS(long_str, file.path(output_dir, "string_long.rds"))

# Test CHARSXP with encoding attributes
# UTF-8 string
utf8_str <- "Hello 世界 🌍"
Encoding(utf8_str) <- "UTF-8"
saveRDS(utf8_str, file.path(output_dir, "string_utf8.rds"))

# Latin1 string
latin1_str <- "Caf\xe9"  # Café in Latin1
Encoding(latin1_str) <- "latin1"
saveRDS(latin1_str, file.path(output_dir, "string_latin1.rds"))

# Test reference tracking with symbols
# Create structure with many repeated symbol names
ref_symbol_structure <- list(
  a = list(x = 1, y = 2, z = 3),
  b = list(x = 4, y = 5, z = 6),
  c = list(x = 7, y = 8, z = 9),
  d = list(x = 10, y = 11, z = 12)
)
saveRDS(ref_symbol_structure, file.path(output_dir, "repeated_symbol_names.rds"))

# Test various NULL-like values
# Plain NULL
saveRDS(NULL, file.path(output_dir, "null_plain.rds"))

# NULL in a list
null_in_list <- list(a = 1, b = NULL, c = 3)
saveRDS(null_in_list, file.path(output_dir, "null_in_list.rds"))

# Multiple NULLs
multi_null <- list(NULL, NULL, NULL)
saveRDS(multi_null, file.path(output_dir, "multi_null.rds"))

# Test environment edge cases
# Empty environment
empty_env <- new.env()
saveRDS(empty_env, file.path(output_dir, "environment_empty.rds"))

# Environment with multiple types
rich_env <- new.env()
rich_env$int_val <- 42L
rich_env$real_val <- 3.14
rich_env$char_val <- "hello"
rich_env$list_val <- list(a = 1, b = 2)
rich_env$func_val <- function(x) x + 1
saveRDS(rich_env, file.path(output_dir, "environment_rich.rds"))

# Test pairlist edge cases (used for attributes and function formals)
# Pairlist with mixed types
mixed_pairlist <- pairlist(a = 1L, b = 2.5, c = "text", d = TRUE)
saveRDS(mixed_pairlist, file.path(output_dir, "pairlist_mixed.rds"))

# Test language objects with various structures
# Simple call
simple_call <- quote(f(x))
saveRDS(simple_call, file.path(output_dir, "lang_simple_call.rds"))

# Call with named arguments
named_call <- quote(f(x = 1, y = 2, z = 3))
saveRDS(named_call, file.path(output_dir, "lang_named_args.rds"))

# Deeply nested call
deep_call <- quote(f(g(h(i(j(k(x)))))))
saveRDS(deep_call, file.path(output_dir, "lang_deep_nested.rds"))

# Test edge cases for unknown types (types 26-237)
# These shouldn't normally occur, but we test robustness
# We can't directly create unknown types, but we test our handling

# Test malformed data resistance
# Very small valid RDS file
tiny_obj <- 1L
saveRDS(tiny_obj, file.path(output_dir, "tiny_object.rds"))

# Test attributes with all standard types
all_types_attrs <- structure(
  c(1, 2, 3),
  int_attr = 42L,
  real_attr = 3.14,
  char_attr = "text",
  logical_attr = TRUE,
  null_attr = NULL,
  list_attr = list(a = 1, b = 2),
  vec_attr = c(1, 2, 3)
)
saveRDS(all_types_attrs, file.path(output_dir, "all_types_attributes.rds"))

# S4 objects - multiple instances with same class (regression test for stack address bug)
# Create a simple Container class
setClass("Container",
         slots = c(data = "integer",
                   name = "character"))

# Create three instances with identical class/package
container1 <- new("Container", data = c(100L, 101L, 102L), name = "first")
container2 <- new("Container", data = c(200L, 201L, 202L), name = "second")
container3 <- new("Container", data = c(300L, 301L, 302L), name = "third")

# Save as a list (reproduces the bug scenario)
containers_list <- list(container1, container2, container3)
saveRDS(containers_list, file.path(output_dir, "s4_multiple_same_class.rds"))

# S4 objects - complex nested structure with matrices
# Define classes for nested S4 structure
setClass("NestedData",
         slots = c(values = "numeric"))

setClass("MatrixContainer",
         slots = c(
           primary_matrix = "matrix",
           secondary_matrix = "matrix",
           label = "character",
           nested = "NestedData"
         ))

# Create instances with matrices having dimnames
mat1 <- matrix(c(1.0, 2.0, 3.0, 4.0), nrow = 2, ncol = 2)
dimnames(mat1) <- list(c("Row1", "Row2"), c("Col1", "Col2"))

mat2 <- matrix(c(0.1, 0.2, 0.3, 0.4), nrow = 2, ncol = 2)
dimnames(mat2) <- list(c("ItemA", "ItemB"), c("Col1", "Col2"))

nested1 <- new("NestedData", values = c(0.01, 0.05))

mc1 <- new("MatrixContainer",
           primary_matrix = mat1,
           secondary_matrix = mat2,
           label = "alpha",
           nested = nested1)

# Create two more with same structure
mat3 <- matrix(c(5.0, 6.0, 7.0, 8.0), nrow = 2, ncol = 2)
dimnames(mat3) <- list(c("Row1", "Row2"), c("Col1", "Col2"))

mat4 <- matrix(c(0.5, 0.6, 0.7, 0.8), nrow = 2, ncol = 2)
dimnames(mat4) <- list(c("ItemA", "ItemB"), c("Col1", "Col2"))

nested2 <- new("NestedData", values = c(0.02, 0.06))

mc2 <- new("MatrixContainer",
           primary_matrix = mat3,
           secondary_matrix = mat4,
           label = "beta",
           nested = nested2)

mat5 <- matrix(c(9.0, 10.0, 11.0, 12.0), nrow = 2, ncol = 2)
dimnames(mat5) <- list(c("Row1", "Row2"), c("Col1", "Col2"))

mat6 <- matrix(c(0.9, 1.0, 1.1, 1.2), nrow = 2, ncol = 2)
dimnames(mat6) <- list(c("ItemA", "ItemB"), c("Col1", "Col2"))

nested3 <- new("NestedData", values = c(0.03, 0.07))

mc3 <- new("MatrixContainer",
           primary_matrix = mat5,
           secondary_matrix = mat6,
           label = "gamma",
           nested = nested3)

# Save as list
matrix_containers <- list(mc1, mc2, mc3)
saveRDS(matrix_containers, file.path(output_dir, "s4_multiple_with_matrices.rds"))

# Optional large vector fixture (off by default to avoid huge repos).
# Enable with: RDS_LARGE_VECTOR_LEN=10000000 Rscript tests/generate_test_data.R
large_len <- suppressWarnings(as.numeric(Sys.getenv("RDS_LARGE_VECTOR_LEN", "0")))
if (!is.na(large_len) && large_len > 0) {
  if (large_len > .Machine$integer.max) {
    stop("RDS_LARGE_VECTOR_LEN exceeds integer max for this fixture")
  }
  large_len <- as.integer(large_len)
  large_vec <- seq_len(large_len)
  large_name <- sprintf("large_int_%d.rds", large_len)
  saveRDS(large_vec, file.path(output_dir, large_name))
  cat("Generated large vector fixture:", large_name, "\n")
}

cat("Test RDS files generated successfully in", output_dir, "\n")
cat("Total files:", length(list.files(output_dir, pattern = "\\.rds$")), "\n")

# xz compressed RDS
saveRDS(mtcars, file.path(output_dir, "xz_compressed.rds"), compress = "xz")
saveRDS(mtcars, file.path(output_dir, "gzip_compressed.rds"), compress = "gzip")
