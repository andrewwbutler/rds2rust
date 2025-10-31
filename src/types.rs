//! Type definitions for R objects.

use std::collections::HashMap;

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

    /// Character vector
    Character(Vec<String>),

    /// Raw (byte) vector
    Raw(Vec<u8>),

    /// Complex vector
    Complex(Vec<Complex>),

    /// Generic list (VECSXP)
    List(Vec<RObject>),

    /// Pairlist (LISTSXP) with optional tags
    Pairlist(Vec<PairlistElement>),

    /// Data frame (list with class="data.frame")
    DataFrame {
        columns: HashMap<String, RObject>,
        row_names: Vec<String>,
    },

    /// Factor (categorical variable with levels)
    Factor {
        values: Vec<i32>,      // Integer indices (1-based, 0 = NA)
        levels: Vec<String>,   // Level labels
        ordered: bool,         // Whether it's an ordered factor
    },

    /// S3 object (any object with a class attribute)
    S3Object {
        base: Box<RObject>,
        class: Vec<String>,
        attributes: Attributes,
    },

    /// S4 object (formal object with slots)
    S4Object {
        class: Vec<String>,
        slots: HashMap<String, RObject>,
    },

    /// Object with attributes (no class)
    WithAttributes {
        object: Box<RObject>,
        attributes: Attributes,
    },
}

/// An element in a pairlist, optionally tagged
#[derive(Debug, Clone, PartialEq)]
pub struct PairlistElement {
    pub tag: Option<String>,
    pub value: RObject,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Attributes {
    pub attrs: HashMap<String, RObject>,
}

impl Attributes {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: RObject) {
        self.attrs.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&RObject> {
        self.attrs.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
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
