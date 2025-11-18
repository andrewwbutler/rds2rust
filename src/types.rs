//! Type definitions for R objects.

use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;

/// Represents any R object that can be stored in an RDS file.
#[derive(Debug, Clone, PartialEq)]
pub enum RObject {
    /// NULL object
    Null,

    /// Integer vector
    Integer(Vec<i32>),

    /// Real (double) vector
    Real(Vec<f64>),

    /// Logical vector
    Logical(Vec<Logical>),

    /// Character vector (using Arc<str> for string interning)
    Character(Vec<Arc<str>>),

    /// Raw (byte) vector
    Raw(Vec<u8>),

    /// Complex vector
    Complex(Vec<Complex>),

    /// Generic list (VECSXP)
    List(Vec<RObject>),

    /// Pairlist (LISTSXP) with optional tags
    Pairlist(Vec<PairlistElement>),

    /// Language object (unevaluated call/expression)
    /// Stored as a list: [function, arg1, arg2, ...]
    Language(Vec<RObject>),

    /// Expression vector (vector of language objects)
    /// Typically the result of parse() - a collection of unevaluated expressions
    Expression(Vec<RObject>),

    /// Closure (function)
    /// Contains formal parameters, body, and enclosing environment
    Closure {
        formals: Box<RObject>,     // Parameter list (pairlist)
        body: Box<RObject>,        // Function body (language object)
        environment: Box<RObject>, // Closure environment
    },

    /// Environment
    /// Contains parent environment, bindings frame, and hash table
    Environment {
        enclosing: Box<RObject>, // Parent environment
        frame: Box<RObject>,     // Bindings (pairlist)
        hashtab: Box<RObject>,   // Hash table (VECSXP)
    },

    /// Promise (lazy evaluation)
    /// Contains value, expression, and environment
    Promise {
        value: Box<RObject>,       // Evaluated value (or unbound)
        expression: Box<RObject>,  // Expression to evaluate
        environment: Box<RObject>, // Evaluation environment
    },

    /// Special primitive function (like 'if', 'for', 'while')
    /// These are internal R functions with special evaluation rules
    Special {
        name: Arc<str>, // Function name (interned)
    },

    /// Builtin primitive function (like 'sum', 'c', '+')
    /// These are internal R functions evaluated normally
    Builtin {
        name: Arc<str>, // Function name (interned)
    },

    /// Bytecode (compiled R function)
    /// Contains code vector, constants pool, and original expression
    Bytecode {
        code: Box<RObject>,      // Bytecode instructions
        constants: Box<RObject>, // Constants pool
        expr: Box<RObject>,      // Original source expression (optional)
    },

    /// Data frame (list with class="data.frame")
    /// Boxed to reduce enum size
    DataFrame(Box<DataFrameData>),

    /// Factor (categorical variable with levels)
    /// Boxed to reduce enum size
    Factor(Box<FactorData>),

    /// S3 object (any object with a class attribute)
    /// Boxed to reduce enum size
    S3Object(Box<S3ObjectData>),

    /// S4 object (formal object with slots)
    /// Boxed to reduce enum size
    S4Object(Box<S4ObjectData>),

    /// Namespace reference (triggers automatic package loading in R)
    /// Contains namespace name components (e.g., ["Matrix"] or ["base"])
    Namespace(Vec<Arc<str>>),

    /// Object with attributes (no class)
    WithAttributes {
        object: Box<RObject>,
        attributes: Attributes,
    },
}

/// Data frame structure (boxed to reduce RObject enum size)
#[derive(Debug, Clone, PartialEq)]
pub struct DataFrameData {
    pub columns: HashMap<Arc<str>, RObject>,
    pub row_names: Vec<Arc<str>>,
}

/// Factor structure (boxed to reduce RObject enum size)
#[derive(Debug, Clone, PartialEq)]
pub struct FactorData {
    pub values: Vec<i32>,      // Integer indices (1-based, 0 = NA)
    pub levels: Vec<Arc<str>>, // Level labels (interned)
    pub ordered: bool,         // Whether it's an ordered factor
}

/// S3 object structure (boxed to reduce RObject enum size)
#[derive(Debug, Clone, PartialEq)]
pub struct S3ObjectData {
    pub base: Box<RObject>,
    pub class: Vec<Arc<str>>, // Class names (interned)
    pub attributes: Attributes,
}

/// S4 object structure (boxed to reduce RObject enum size)
#[derive(Debug, Clone, PartialEq)]
pub struct S4ObjectData {
    pub class: Vec<Arc<str>>,              // Class names (interned)
    pub package: Option<Arc<str>>,         // Package attribute (e.g., "SeuratObject", "Matrix")
    pub slots: HashMap<Arc<str>, RObject>, // Slot names (interned)
}

/// An element in a pairlist, optionally tagged
#[derive(Debug, Clone, PartialEq)]
pub struct PairlistElement {
    pub tag: Option<Arc<str>>, // Tag name (interned)
    pub value: RObject,
    pub tag_object: Option<Box<RObject>>, // Raw TAG object before name extraction (for special cases)
}

/// Represents a logical value in R (TRUE, FALSE, or NA)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Logical {
    True,
    False,
    Na,
}

/// Represents a complex number
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Complex {
    pub real: f64,
    pub imaginary: f64,
}

/// Attributes attached to R objects
/// Optimized with SmallVec to avoid heap allocation for common case of 0-2 attributes
#[derive(Debug, Clone, PartialEq)]
pub struct Attributes {
    /// Most R objects have 0-2 attributes (names, class, dim, etc.)
    /// SmallVec stores up to 2 inline, avoiding HashMap overhead
    /// Box<RObject> breaks the recursion cycle between Attributes and RObject
    pub attrs: SmallVec<[(Arc<str>, Box<RObject>); 2]>,
}

impl Attributes {
    pub fn new() -> Self {
        Self {
            attrs: SmallVec::new(),
        }
    }

    pub fn insert(&mut self, key: Arc<str>, value: RObject) {
        // Check if key already exists and update it
        for (k, v) in self.attrs.iter_mut() {
            if k.as_ref() == key.as_ref() {
                *v = Box::new(value);
                return;
            }
        }
        // Key doesn't exist, add new entry
        self.attrs.push((key, Box::new(value)));
    }

    pub fn get(&self, key: &str) -> Option<&RObject> {
        self.attrs
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Get iterator over attribute entries
    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, &RObject)> {
        self.attrs.iter().map(|(k, v)| (k, v.as_ref()))
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Self::new()
    }
}

// Special NA values for different types
impl RObject {
    /// Integer NA value in R
    pub const NA_INTEGER: i32 = i32::MIN;

    /// Check if an integer is NA
    pub fn is_na_integer(val: i32) -> bool {
        val == Self::NA_INTEGER
    }
}
