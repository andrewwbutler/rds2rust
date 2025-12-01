//! Type definitions for R objects.

use indexmap::IndexMap;
use smallvec::SmallVec;
use std::sync::{Arc, RwLock};

/// Represents any R object that can be stored in an RDS file.
#[derive(Debug, Clone)]
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

    /// Symbol (SYMSXP) - a named symbol
    /// Used for R's internal symbol table and special markers
    Symbol(Arc<str>),

    /// Raw (byte) vector
    Raw(Vec<u8>),

    /// Complex vector
    Complex(Vec<Complex>),

    /// Generic list (VECSXP)
    List(Vec<RObject>),

    /// Pairlist (LISTSXP) with optional tags
    Pairlist(Vec<PairlistElement>),

    /// Language object (unevaluated call/expression)
    /// Contains the function being called and its arguments with optional names
    Language {
        function: Box<RObject>,     // Function being called (symbol, closure, etc.)
        args: Vec<PairlistElement>, // Arguments with optional names (tags)
    },

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

    /// Global environment reference
    /// This is a singleton in R that persists across the session
    GlobalEnv,

    /// Base environment reference
    /// Contains base package bindings
    BaseEnv,

    /// Empty environment reference
    /// The root of the environment tree (has no parent)
    EmptyEnv,

    /// Missing argument marker
    /// Used for default arguments in function formals
    MissingArg,

    /// Unbound value marker
    /// Used to indicate an unbound variable
    UnboundValue,

    /// Shared reference to an existing object (used for REFSXP backreferences)
    /// to avoid deep cloning large structures during parsing.
    Shared(Arc<RwLock<RObject>>),

    /// Object with attributes (no class)
    WithAttributes {
        object: Box<RObject>,
        attributes: Attributes,
    },
}

impl RObject {
    /// If this object is a Shared wrapper, return the underlying object reference.
    /// Otherwise return self.
    pub fn as_concrete(&self) -> RObject {
        match self {
            RObject::Shared(inner) => inner.read().unwrap().clone(),
            other => other.clone(),
        }
    }

    /// Consume the object, unwrapping Shared by cloning the underlying object.
    pub fn into_concrete(self) -> RObject {
        match self {
            RObject::Shared(inner) => inner.read().unwrap().clone(),
            other => other,
        }
    }

    /// Human-friendly variant name for debugging.
    pub fn variant_name(&self) -> &'static str {
        use RObject::*;
        match self {
            Null => "Null",
            Integer(_) => "Integer",
            Real(_) => "Real",
            Logical(_) => "Logical",
            Character(_) => "Character",
            Symbol(_) => "Symbol",
            Raw(_) => "Raw",
            Complex(_) => "Complex",
            List(_) => "List",
            Pairlist(_) => "Pairlist",
            Language { .. } => "Language",
            Expression(_) => "Expression",
            Closure { .. } => "Closure",
            Environment { .. } => "Environment",
            Promise { .. } => "Promise",
            Special { .. } => "Special",
            Builtin { .. } => "Builtin",
            Bytecode { .. } => "Bytecode",
            DataFrame(_) => "DataFrame",
            Factor(_) => "Factor",
            S3Object(_) => "S3Object",
            S4Object(_) => "S4Object",
            Namespace(_) => "Namespace",
            GlobalEnv => "GlobalEnv",
            BaseEnv => "BaseEnv",
            EmptyEnv => "EmptyEnv",
            MissingArg => "MissingArg",
            UnboundValue => "UnboundValue",
            WithAttributes { .. } => "WithAttributes",
            Shared(_) => "Shared",
        }
    }
}

impl PartialEq for RObject {
    fn eq(&self, other: &Self) -> bool {
        use RObject::*;
        let a = self.as_concrete();
        let b = other.as_concrete();
        match (a, b) {
            (Null, Null) => true,
            (Integer(x), Integer(y)) => x == y,
            (Real(x), Real(y)) => x == y,
            (Logical(x), Logical(y)) => x == y,
            (Character(x), Character(y)) => x == y,
            (Symbol(x), Symbol(y)) => x == y,
            (Raw(x), Raw(y)) => x == y,
            (Complex(x), Complex(y)) => x == y,
            (List(x), List(y)) => x == y,
            (Pairlist(x), Pairlist(y)) => x == y,
            (
                Language {
                    function: fx,
                    args: ax,
                },
                Language {
                    function: fy,
                    args: ay,
                },
            ) => fx == fy && ax == ay,
            (Expression(x), Expression(y)) => x == y,
            (
                Closure {
                    formals: fx,
                    body: bx,
                    environment: ex,
                },
                Closure {
                    formals: fy,
                    body: by,
                    environment: ey,
                },
            ) => fx == fy && bx == by && ex == ey,
            (
                Environment {
                    enclosing: ex1,
                    frame: frx,
                    hashtab: hx,
                },
                Environment {
                    enclosing: ex2,
                    frame: fry,
                    hashtab: hy,
                },
            ) => ex1 == ex2 && frx == fry && hx == hy,
            (
                Promise {
                    value: vx,
                    expression: px,
                    environment: ex,
                },
                Promise {
                    value: vy,
                    expression: py,
                    environment: ey,
                },
            ) => vx == vy && px == py && ex == ey,
            (Special { name: nx }, Special { name: ny }) => nx == ny,
            (Builtin { name: nx }, Builtin { name: ny }) => nx == ny,
            (
                Bytecode {
                    code: cx,
                    constants: kx,
                    expr: ex,
                },
                Bytecode {
                    code: cy,
                    constants: ky,
                    expr: ey,
                },
            ) => cx == cy && kx == ky && ex == ey,
            (DataFrame(x), DataFrame(y)) => x == y,
            (Factor(x), Factor(y)) => x == y,
            (S3Object(x), S3Object(y)) => x == y,
            (S4Object(x), S4Object(y)) => x == y,
            (Namespace(x), Namespace(y)) => x == y,
            (GlobalEnv, GlobalEnv) => true,
            (BaseEnv, BaseEnv) => true,
            (EmptyEnv, EmptyEnv) => true,
            (MissingArg, MissingArg) => true,
            (UnboundValue, UnboundValue) => true,
            (
                WithAttributes {
                    object: ox,
                    attributes: ax,
                },
                WithAttributes {
                    object: oy,
                    attributes: ay,
                },
            ) => ox == oy && ax == ay,
            _ => false,
        }
    }
}

/// Data frame structure (boxed to reduce RObject enum size)
#[derive(Debug, Clone, PartialEq)]
pub struct DataFrameData {
    pub columns: IndexMap<Arc<str>, RObject>,
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
    pub class: Vec<Arc<str>>,               // Class names (interned)
    pub package: Option<Arc<str>>,          // Package attribute for S4 objects
    pub slots: IndexMap<Arc<str>, RObject>, // Slot names (interned)
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

impl FactorData {
    pub(crate) fn base_attributes(&self) -> Attributes {
        let mut attrs = Attributes::new();
        attrs.insert(Arc::from("levels"), RObject::Character(self.levels.clone()));

        let class = if self.ordered {
            vec![Arc::from("ordered"), Arc::from("factor")]
        } else {
            vec![Arc::from("factor")]
        };
        attrs.insert(Arc::from("class"), RObject::Character(class));

        attrs
    }

    /// Build a factor object with additional attributes (e.g., names, contrasts).
    /// Base factor attributes (levels, class) are included automatically and can be
    /// overridden by the provided attributes if needed.
    pub fn with_attributes(self, attributes: Attributes) -> RObject {
        let mut merged = self.base_attributes();

        for (key, value) in attributes.attrs.into_iter() {
            merged.insert(key, *value);
        }

        RObject::WithAttributes {
            object: Box::new(RObject::Factor(Box::new(self))),
            attributes: merged,
        }
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
