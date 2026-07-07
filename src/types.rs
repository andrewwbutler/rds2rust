//! Type definitions for R objects.

use indexmap::IndexMap;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::ObjectPath;

/// Lazy vector metadata for deferred loading.
///
/// Stores the position and size information needed to materialize
/// a vector's data from the source without loading it during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LazyVector {
    /// Number of elements in the vector
    pub length: usize,
    /// Offset in the DECOMPRESSED stream (not the compressed file).
    /// Using u64 to support files >4GB.
    pub offset: u64,
    /// Number of bytes to read for materialization.
    /// Enables validation against truncation.
    pub byte_len: u64,
}

/// Vector data that can be either fully loaded or lazy.
///
/// This unified representation avoids enum explosion by allowing
/// all vector types to use the same container pattern.
#[derive(Debug, Clone)]
pub enum VectorData<T> {
    /// Fully loaded vector data
    Owned(Vec<T>),
    /// Lazy vector with metadata only
    Lazy(LazyVector),
}

impl<T: PartialEq> PartialEq for VectorData<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (VectorData::Owned(a), VectorData::Owned(b)) => a == b,
            (VectorData::Lazy(a), VectorData::Lazy(b)) => a == b,
            _ => false, // Lazy and Owned are never equal
        }
    }
}

impl<T> Default for VectorData<T> {
    fn default() -> Self {
        VectorData::Owned(Vec::new())
    }
}

impl<T> VectorData<T> {
    /// Check if this vector is fully loaded.
    pub fn is_loaded(&self) -> bool {
        matches!(self, VectorData::Owned(_))
    }

    /// Get the lazy span if this vector is not yet materialized.
    pub fn lazy_span(&self) -> Option<LazyVector> {
        match self {
            VectorData::Lazy(span) => Some(*span),
            VectorData::Owned(_) => None,
        }
    }

    /// Get the length of the vector regardless of load state.
    pub fn len(&self) -> usize {
        match self {
            VectorData::Owned(v) => v.len(),
            VectorData::Lazy(lazy) => lazy.length,
        }
    }

    /// Check if the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a reference to the owned vector, or panic if lazy.
    ///
    /// # Panics
    ///
    /// Panics if the vector data is lazy and not yet materialized.
    ///
    /// # Note
    ///
    /// If you encounter this panic during parsing, the file likely contains
    /// bytecode or other structures that require loaded data. Use `read_rds()`
    /// in full mode instead of `read_rds_lazy()` for such files.
    pub fn as_vec(&self) -> &Vec<T> {
        match self {
            VectorData::Owned(v) => v,
            VectorData::Lazy(lazy) => panic!(
                "Cannot access lazy vector data (length={}, offset={}). \
                 This file may contain bytecode or structures requiring full parsing. \
                 Use read_rds() instead of read_rds_lazy(), or check is_loaded() before accessing.",
                lazy.length, lazy.offset
            ),
        }
    }

    /// Get a mutable reference to the owned vector, or panic if lazy.
    ///
    /// # Panics
    ///
    /// Panics if the vector data is lazy and not yet materialized.
    pub fn as_vec_mut(&mut self) -> &mut Vec<T> {
        match self {
            VectorData::Owned(v) => v,
            VectorData::Lazy(lazy) => panic!(
                "Cannot access lazy vector data (length={}, offset={}). \
                 Use read_rds() instead of read_rds_lazy(), or check is_loaded() before accessing.",
                lazy.length, lazy.offset
            ),
        }
    }

    /// Unwrap into the owned vector, or panic if lazy.
    ///
    /// # Panics
    ///
    /// Panics if the vector data is lazy and not yet materialized.
    pub fn into_vec(self) -> Vec<T> {
        match self {
            VectorData::Owned(v) => v,
            VectorData::Lazy(lazy) => panic!(
                "Cannot access lazy vector data (length={}, offset={}). \
                 Use read_rds() instead of read_rds_lazy(), or check is_loaded() before accessing.",
                lazy.length, lazy.offset
            ),
        }
    }
}

impl<T> std::ops::Index<usize> for VectorData<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            VectorData::Owned(v) => &v[index],
            VectorData::Lazy(lazy) => panic!(
                "Cannot access lazy vector data (length={}, offset={}). \
                 This file may contain bytecode or structures requiring full parsing. \
                 Use read_rds() instead of read_rds_lazy(), or check is_loaded() before accessing.",
                lazy.length, lazy.offset
            ),
        }
    }
}

impl<T> std::ops::Deref for VectorData<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        match self {
            VectorData::Owned(v) => v,
            VectorData::Lazy(_) => &[],
        }
    }
}

impl<T> From<Vec<T>> for VectorData<T> {
    fn from(vec: Vec<T>) -> Self {
        VectorData::Owned(vec)
    }
}

impl<T> FromIterator<T> for VectorData<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        VectorData::Owned(iter.into_iter().collect())
    }
}

impl<'a, T: 'a + Clone> IntoIterator for &'a VectorData<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            VectorData::Owned(v) => v.iter(),
            VectorData::Lazy(_) => [].iter(),
        }
    }
}

// Additional PartialEq implementations for test convenience
impl<T: PartialEq> PartialEq<Vec<T>> for VectorData<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        match self {
            VectorData::Owned(v) => v == other,
            VectorData::Lazy(_) => false,
        }
    }
}

impl<T: PartialEq> PartialEq<&[T]> for VectorData<T> {
    fn eq(&self, other: &&[T]) -> bool {
        match self {
            VectorData::Owned(v) => v.as_slice() == *other,
            VectorData::Lazy(_) => false,
        }
    }
}

impl<T: PartialEq, const N: usize> PartialEq<[T; N]> for VectorData<T> {
    fn eq(&self, other: &[T; N]) -> bool {
        match self {
            VectorData::Owned(v) => v.as_slice() == other.as_slice(),
            VectorData::Lazy(_) => false,
        }
    }
}

/// Represents any R object that can be stored in an RDS file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RObject {
    /// NULL object
    Null,

    /// Integer vector (can be fully loaded or lazy)
    Integer(VectorData<i32>),

    /// Real (double) vector (can be fully loaded or lazy)
    Real(VectorData<f64>),

    /// Logical vector (can be fully loaded or lazy)
    Logical(VectorData<Logical>),

    /// Character vector (using Arc<str> for string interning, can be fully loaded or lazy)
    Character(VectorData<Arc<str>>),

    /// Symbol (SYMSXP) - a named symbol
    /// Used for R's internal symbol table and special markers
    Symbol(Arc<str>),

    /// Raw (byte) vector (can be fully loaded or lazy)
    Raw(VectorData<u8>),

    /// Complex vector (can be fully loaded or lazy)
    Complex(VectorData<Complex>),

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

    /// Recursively convert all Shared wrappers to concrete objects throughout the entire tree.
    ///
    /// This is essential for thread safety when the resulting RObject will be used across
    /// threads. While each parse has its own RefTable, nested Shared objects (in Lists,
    /// Pairlists, WithAttributes, etc.) share Arc<RwLock> pointers that can cause race
    /// conditions when accessed concurrently from different threads.
    ///
    /// Uses cycle detection to handle circular references (e.g., in environments).
    pub fn into_concrete_deep(self) -> RObject {
        let mut visited = std::collections::HashMap::new();
        self.into_concrete_deep_impl(&mut visited)
    }

    /// Internal implementation with cycle detection.
    ///
    /// When a cycle is detected (Shared object already visited), we break the cycle by
    /// returning a clone of the already-processed concrete object.
    fn into_concrete_deep_impl(self, visited: &mut HashMap<usize, RObject>) -> RObject {
        use RObject::*;

        // First unwrap top-level Shared
        let obj = match self {
            Shared(inner) => {
                let addr = Arc::as_ptr(&inner) as usize;
                // Check if we've already processed this Shared object
                if let Some(concrete) = visited.get(&addr) {
                    // Cycle detected - return the already-processed object
                    return concrete.clone();
                }
                // Lock and clone the inner object
                let inner_obj = inner.read().unwrap().clone();
                // Recursively process it and store in visited map
                let concrete = inner_obj.into_concrete_deep_impl(visited);
                visited.insert(addr, concrete.clone());
                return concrete;
            }
            other => other,
        };

        // Now recursively process nested objects
        match obj {
            List(elements) => List(
                elements
                    .into_iter()
                    .map(|e| e.into_concrete_deep_impl(visited))
                    .collect(),
            ),
            Pairlist(elements) => Pairlist(
                elements
                    .into_iter()
                    .map(|e| PairlistElement {
                        tag: e.tag,
                        value: e.value.into_concrete_deep_impl(visited),
                        tag_object: e
                            .tag_object
                            .map(|t| Box::new(t.into_concrete_deep_impl(visited))),
                    })
                    .collect(),
            ),
            Language { function, args } => Language {
                function: Box::new(function.into_concrete_deep_impl(visited)),
                args: args
                    .into_iter()
                    .map(|e| PairlistElement {
                        tag: e.tag,
                        value: e.value.into_concrete_deep_impl(visited),
                        tag_object: e
                            .tag_object
                            .map(|t| Box::new(t.into_concrete_deep_impl(visited))),
                    })
                    .collect(),
            },
            Expression(elements) => Expression(
                elements
                    .into_iter()
                    .map(|e| e.into_concrete_deep_impl(visited))
                    .collect(),
            ),
            Closure {
                formals,
                body,
                environment,
            } => Closure {
                formals: Box::new(formals.into_concrete_deep_impl(visited)),
                body: Box::new(body.into_concrete_deep_impl(visited)),
                environment: Box::new(environment.into_concrete_deep_impl(visited)),
            },
            Environment {
                enclosing,
                frame,
                hashtab,
            } => Environment {
                enclosing: Box::new(enclosing.into_concrete_deep_impl(visited)),
                frame: Box::new(frame.into_concrete_deep_impl(visited)),
                hashtab: Box::new(hashtab.into_concrete_deep_impl(visited)),
            },
            Promise {
                value,
                expression,
                environment,
            } => Promise {
                value: Box::new(value.into_concrete_deep_impl(visited)),
                expression: Box::new(expression.into_concrete_deep_impl(visited)),
                environment: Box::new(environment.into_concrete_deep_impl(visited)),
            },
            Bytecode {
                code,
                constants,
                expr,
            } => Bytecode {
                code: Box::new(code.into_concrete_deep_impl(visited)),
                constants: Box::new(constants.into_concrete_deep_impl(visited)),
                expr: Box::new(expr.into_concrete_deep_impl(visited)),
            },
            DataFrame(mut df) => {
                df.columns = df
                    .columns
                    .into_iter()
                    .map(|(name, obj)| (name, obj.into_concrete_deep_impl(visited)))
                    .collect();
                DataFrame(df)
            }
            Factor(f) => {
                // FactorData only contains primitive types, no nested RObjects
                Factor(f)
            }
            S3Object(mut s3) => {
                s3.base = Box::new(s3.base.into_concrete_deep_impl(visited));
                s3.attributes = s3.attributes.into_concrete_deep_impl(visited);
                S3Object(s3)
            }
            S4Object(mut s4) => {
                s4.slots = s4
                    .slots
                    .into_iter()
                    .map(|(k, v)| (k, v.into_concrete_deep_impl(visited)))
                    .collect();
                S4Object(s4)
            }
            WithAttributes { object, attributes } => WithAttributes {
                object: Box::new(object.into_concrete_deep_impl(visited)),
                attributes: attributes.into_concrete_deep_impl(visited),
            },
            Namespace(ns) => {
                // Namespace is Vec<Arc<str>>, no nested RObjects
                Namespace(ns)
            }
            // All other variants don't contain nested RObjects
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

    /// Check if this object is fully loaded (no lazy vectors).
    ///
    /// Returns `true` if all vector data is materialized (Owned), `false` if any
    /// vector is in Lazy state. For non-vector objects, recursively checks nested
    /// objects.
    ///
    /// Note: Uses cycle detection to handle circular references in environments/promises.
    pub fn is_fully_loaded(&self) -> bool {
        use std::collections::HashSet;
        let mut visited = HashSet::new();
        self.is_fully_loaded_impl(&mut visited)
    }

    /// Internal implementation with cycle detection.
    fn is_fully_loaded_impl(&self, visited: &mut std::collections::HashSet<usize>) -> bool {
        use RObject::*;

        // For Shared objects, use the Arc pointer address for cycle detection
        if let Shared(inner) = self {
            let addr = Arc::as_ptr(inner) as usize;
            if !visited.insert(addr) {
                // Already visited - assume loaded to avoid infinite recursion
                return true;
            }
            return inner.read().unwrap().is_fully_loaded_impl(visited);
        }

        match self {
            // Vector types - check if data is loaded
            Integer(v) => v.is_loaded(),
            Real(v) => v.is_loaded(),
            Logical(v) => v.is_loaded(),
            Character(v) => v.is_loaded(),
            Raw(v) => v.is_loaded(),
            Complex(v) => v.is_loaded(),

            // Container types - recursively check contents
            List(items) => items.iter().all(|item| item.is_fully_loaded_impl(visited)),
            Expression(items) => items.iter().all(|item| item.is_fully_loaded_impl(visited)),
            Pairlist(elements) => elements.iter().all(|elem| {
                elem.value.is_fully_loaded_impl(visited)
                    && elem
                        .tag_object
                        .as_ref()
                        .is_none_or(|t| t.is_fully_loaded_impl(visited))
            }),
            Language { function, args } => {
                function.is_fully_loaded_impl(visited)
                    && args.iter().all(|elem| {
                        elem.value.is_fully_loaded_impl(visited)
                            && elem
                                .tag_object
                                .as_ref()
                                .is_none_or(|t| t.is_fully_loaded_impl(visited))
                    })
            }

            // DataFrame - check all columns
            DataFrame(df) => df
                .columns
                .values()
                .all(|col| col.is_fully_loaded_impl(visited)),

            // Factor - levels are always loaded (Vec<Arc<str>>), values are Vec<i32>
            Factor(_f) => true, // No lazy data in factors

            // S3/S4 objects - check base object and slots
            S3Object(s3) => s3.base.is_fully_loaded_impl(visited),
            S4Object(s4) => s4
                .slots
                .values()
                .all(|val| val.is_fully_loaded_impl(visited)),

            // Closures and environments
            Closure {
                formals,
                body,
                environment,
            } => {
                formals.is_fully_loaded_impl(visited)
                    && body.is_fully_loaded_impl(visited)
                    && environment.is_fully_loaded_impl(visited)
            }
            Environment {
                enclosing,
                frame,
                hashtab,
            } => {
                enclosing.is_fully_loaded_impl(visited)
                    && frame.is_fully_loaded_impl(visited)
                    && hashtab.is_fully_loaded_impl(visited)
            }

            // Promise
            Promise {
                value,
                expression,
                environment,
            } => {
                value.is_fully_loaded_impl(visited)
                    && expression.is_fully_loaded_impl(visited)
                    && environment.is_fully_loaded_impl(visited)
            }

            // Bytecode
            Bytecode {
                code,
                constants,
                expr,
            } => {
                code.is_fully_loaded_impl(visited)
                    && constants.is_fully_loaded_impl(visited)
                    && expr.is_fully_loaded_impl(visited)
            }

            // Namespace - just a Vec<Arc<str>>, always loaded
            Namespace(_ns) => true,

            // WithAttributes wrapper
            WithAttributes { object, .. } => object.is_fully_loaded_impl(visited),

            // Shared wrapper - handled at the top of the function
            Shared(_) => unreachable!("Shared case handled above"),

            // Atomic types and special values - always fully loaded
            Null
            | Symbol(_)
            | Special { .. }
            | Builtin { .. }
            | GlobalEnv
            | BaseEnv
            | EmptyEnv
            | MissingArg
            | UnboundValue => true,
        }
    }

    /// Collect all lazy vector spans in this object tree.
    ///
    /// Returns a vector of tuples containing the path to each lazy vector and its
    /// LazyVector metadata (offset, byte_len, length). The path is a dot-separated
    /// string describing the location (e.g., "columns.gene_names" for a DataFrame column).
    ///
    /// This is useful for understanding the lazy structure of a parsed RDS file.
    ///
    /// Note: Uses cycle detection to handle circular references in environments/promises.
    pub fn lazy_spans(&self) -> Vec<(String, LazyVector)> {
        use std::collections::HashSet;
        let mut spans = Vec::new();
        let mut visited = HashSet::new();
        self.collect_lazy_spans_impl("", &mut spans, &mut visited);
        spans
    }

    pub(crate) fn lazy_vector_infos(&self) -> Vec<(ObjectPath, &'static str, LazyVector)> {
        use std::collections::HashSet;
        let mut infos = Vec::new();
        let mut visited = HashSet::new();
        let mut path = ObjectPath::new(Vec::new());
        self.collect_lazy_vector_infos_impl(&mut path, &mut infos, &mut visited);
        infos
    }

    /// Internal helper for recursively collecting lazy spans with cycle detection.
    fn collect_lazy_spans_impl(
        &self,
        path_prefix: &str,
        spans: &mut Vec<(String, LazyVector)>,
        visited: &mut std::collections::HashSet<usize>,
    ) {
        use RObject::*;

        // For Shared objects, use the Arc pointer address for cycle detection
        if let Shared(inner) = self {
            let addr = Arc::as_ptr(inner) as usize;
            if !visited.insert(addr) {
                // Already visited - skip to avoid infinite recursion
                return;
            }
            inner
                .read()
                .unwrap()
                .collect_lazy_spans_impl(path_prefix, spans, visited);
            return;
        }

        match self {
            // Vector types - check if data is lazy
            Integer(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }
            Real(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }
            Logical(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }
            Character(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }
            Raw(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }
            Complex(v) => {
                if let VectorData::Lazy(lazy) = v {
                    spans.push((path_prefix.to_string(), *lazy));
                }
            }

            // Container types - recursively collect from contents
            List(items) => {
                for (i, item) in items.iter().enumerate() {
                    let path = if path_prefix.is_empty() {
                        format!("[{}]", i)
                    } else {
                        format!("{}[{}]", path_prefix, i)
                    };
                    item.collect_lazy_spans_impl(&path, spans, visited);
                }
            }
            Expression(items) => {
                for (i, item) in items.iter().enumerate() {
                    let path = if path_prefix.is_empty() {
                        format!("[{}]", i)
                    } else {
                        format!("{}[{}]", path_prefix, i)
                    };
                    item.collect_lazy_spans_impl(&path, spans, visited);
                }
            }
            Pairlist(elements) => {
                for (i, elem) in elements.iter().enumerate() {
                    let value_path = if path_prefix.is_empty() {
                        format!("[{}].value", i)
                    } else {
                        format!("{}[{}].value", path_prefix, i)
                    };
                    elem.value
                        .collect_lazy_spans_impl(&value_path, spans, visited);

                    if let Some(tag_obj) = &elem.tag_object {
                        let tag_path = if path_prefix.is_empty() {
                            format!("[{}].tag_object", i)
                        } else {
                            format!("{}[{}].tag_object", path_prefix, i)
                        };
                        tag_obj.collect_lazy_spans_impl(&tag_path, spans, visited);
                    }
                }
            }
            Language { function, args } => {
                let func_path = if path_prefix.is_empty() {
                    "function".to_string()
                } else {
                    format!("{}.function", path_prefix)
                };
                function.collect_lazy_spans_impl(&func_path, spans, visited);

                for (i, elem) in args.iter().enumerate() {
                    let value_path = if path_prefix.is_empty() {
                        format!("args[{}].value", i)
                    } else {
                        format!("{}.args[{}].value", path_prefix, i)
                    };
                    elem.value
                        .collect_lazy_spans_impl(&value_path, spans, visited);

                    if let Some(tag_obj) = &elem.tag_object {
                        let tag_path = if path_prefix.is_empty() {
                            format!("args[{}].tag_object", i)
                        } else {
                            format!("{}.args[{}].tag_object", path_prefix, i)
                        };
                        tag_obj.collect_lazy_spans_impl(&tag_path, spans, visited);
                    }
                }
            }

            // DataFrame - collect from all columns
            DataFrame(df) => {
                for (col_name, col_obj) in &df.columns {
                    let col_path = if path_prefix.is_empty() {
                        col_name.to_string()
                    } else {
                        format!("{}.{}", path_prefix, col_name)
                    };
                    col_obj.collect_lazy_spans_impl(&col_path, spans, visited);
                }
            }

            // Factor - no lazy data (values are Vec<i32>, levels are Vec<Arc<str>>)
            Factor(_f) => {}

            // S3/S4 objects
            S3Object(s3) => {
                let base_path = if path_prefix.is_empty() {
                    "base".to_string()
                } else {
                    format!("{}.base", path_prefix)
                };
                s3.base.collect_lazy_spans_impl(&base_path, spans, visited);
            }
            S4Object(s4) => {
                for (slot_name, slot_val) in &s4.slots {
                    let slot_path = if path_prefix.is_empty() {
                        slot_name.to_string()
                    } else {
                        format!("{}.{}", path_prefix, slot_name)
                    };
                    slot_val.collect_lazy_spans_impl(&slot_path, spans, visited);
                }
            }

            // Closures and environments
            Closure {
                formals,
                body,
                environment,
            } => {
                let formals_path = if path_prefix.is_empty() {
                    "formals".to_string()
                } else {
                    format!("{}.formals", path_prefix)
                };
                formals.collect_lazy_spans_impl(&formals_path, spans, visited);

                let body_path = if path_prefix.is_empty() {
                    "body".to_string()
                } else {
                    format!("{}.body", path_prefix)
                };
                body.collect_lazy_spans_impl(&body_path, spans, visited);

                let env_path = if path_prefix.is_empty() {
                    "environment".to_string()
                } else {
                    format!("{}.environment", path_prefix)
                };
                environment.collect_lazy_spans_impl(&env_path, spans, visited);
            }
            Environment {
                enclosing,
                frame,
                hashtab,
            } => {
                let enclosing_path = if path_prefix.is_empty() {
                    "enclosing".to_string()
                } else {
                    format!("{}.enclosing", path_prefix)
                };
                enclosing.collect_lazy_spans_impl(&enclosing_path, spans, visited);

                let frame_path = if path_prefix.is_empty() {
                    "frame".to_string()
                } else {
                    format!("{}.frame", path_prefix)
                };
                frame.collect_lazy_spans_impl(&frame_path, spans, visited);

                let hashtab_path = if path_prefix.is_empty() {
                    "hashtab".to_string()
                } else {
                    format!("{}.hashtab", path_prefix)
                };
                hashtab.collect_lazy_spans_impl(&hashtab_path, spans, visited);
            }

            // Promise
            Promise {
                value,
                expression,
                environment,
            } => {
                let value_path = if path_prefix.is_empty() {
                    "value".to_string()
                } else {
                    format!("{}.value", path_prefix)
                };
                value.collect_lazy_spans_impl(&value_path, spans, visited);

                let expr_path = if path_prefix.is_empty() {
                    "expression".to_string()
                } else {
                    format!("{}.expression", path_prefix)
                };
                expression.collect_lazy_spans_impl(&expr_path, spans, visited);

                let env_path = if path_prefix.is_empty() {
                    "environment".to_string()
                } else {
                    format!("{}.environment", path_prefix)
                };
                environment.collect_lazy_spans_impl(&env_path, spans, visited);
            }

            // Bytecode
            Bytecode {
                code,
                constants,
                expr,
            } => {
                let code_path = if path_prefix.is_empty() {
                    "code".to_string()
                } else {
                    format!("{}.code", path_prefix)
                };
                code.collect_lazy_spans_impl(&code_path, spans, visited);

                let constants_path = if path_prefix.is_empty() {
                    "constants".to_string()
                } else {
                    format!("{}.constants", path_prefix)
                };
                constants.collect_lazy_spans_impl(&constants_path, spans, visited);

                let expr_path = if path_prefix.is_empty() {
                    "expr".to_string()
                } else {
                    format!("{}.expr", path_prefix)
                };
                expr.collect_lazy_spans_impl(&expr_path, spans, visited);
            }

            // Namespace - just a Vec<Arc<str>>, no lazy data
            Namespace(_ns) => {}

            // WithAttributes wrapper
            WithAttributes { object, .. } => {
                object.collect_lazy_spans_impl(path_prefix, spans, visited);
            }

            // Shared wrapper
            Shared(inner) => {
                inner
                    .read()
                    .unwrap()
                    .collect_lazy_spans_impl(path_prefix, spans, visited);
            }

            // Atomic types and special values - no lazy data
            Null
            | Symbol(_)
            | Special { .. }
            | Builtin { .. }
            | GlobalEnv
            | BaseEnv
            | EmptyEnv
            | MissingArg
            | UnboundValue => {}
        }
    }

    fn collect_lazy_vector_infos_impl(
        &self,
        path: &mut ObjectPath,
        infos: &mut Vec<(ObjectPath, &'static str, LazyVector)>,
        visited: &mut std::collections::HashSet<usize>,
    ) {
        use RObject::*;

        if let Shared(inner) = self {
            let addr = Arc::as_ptr(inner) as usize;
            if !visited.insert(addr) {
                return;
            }
            inner
                .read()
                .unwrap()
                .collect_lazy_vector_infos_impl(path, infos, visited);
            return;
        }

        match self {
            Integer(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "integer", *lazy));
                }
            }
            Real(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "real", *lazy));
                }
            }
            Logical(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "logical", *lazy));
                }
            }
            Character(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "character", *lazy));
                }
            }
            Raw(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "raw", *lazy));
                }
            }
            Complex(v) => {
                if let VectorData::Lazy(lazy) = v {
                    infos.push((path.clone(), "complex", *lazy));
                }
            }

            List(items) | Expression(items) => {
                for (index, item) in items.iter().enumerate() {
                    path.push(Arc::from(format!("[{}]", index)));
                    item.collect_lazy_vector_infos_impl(path, infos, visited);
                    path.pop();
                }
            }
            Pairlist(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    let segment = element
                        .tag
                        .as_ref()
                        .map(Arc::clone)
                        .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
                    path.push(segment);
                    element
                        .value
                        .collect_lazy_vector_infos_impl(path, infos, visited);
                    path.pop();
                }
            }
            Language { function, args } => {
                path.push(Arc::from("function"));
                function.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                for (index, arg) in args.iter().enumerate() {
                    let segment = arg
                        .tag
                        .as_ref()
                        .map(Arc::clone)
                        .unwrap_or_else(|| Arc::from(format!("[{}]", index)));
                    path.push(segment);
                    arg.value
                        .collect_lazy_vector_infos_impl(path, infos, visited);
                    path.pop();
                }
            }
            DataFrame(df) => {
                for (name, column) in df.columns.iter() {
                    path.push(Arc::clone(name));
                    column.collect_lazy_vector_infos_impl(path, infos, visited);
                    path.pop();
                }
            }
            S3Object(s3) => {
                self.collect_attributes_lazy_infos(&s3.attributes, path, infos, visited);
                path.push(Arc::from("base"));
                s3.base.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
            }
            S4Object(s4) => {
                for (name, slot) in s4.slots.iter() {
                    path.push(Arc::clone(name));
                    slot.collect_lazy_vector_infos_impl(path, infos, visited);
                    path.pop();
                }
            }
            Closure {
                formals,
                body,
                environment,
            } => {
                path.push(Arc::from("formals"));
                formals.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("body"));
                body.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("environment"));
                environment.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
            }
            Environment {
                enclosing,
                frame,
                hashtab,
            } => {
                path.push(Arc::from("enclosing"));
                enclosing.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("frame"));
                frame.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("hashtab"));
                hashtab.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
            }
            Promise {
                value,
                expression,
                environment,
            } => {
                path.push(Arc::from("value"));
                value.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("expression"));
                expression.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("environment"));
                environment.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
            }
            Bytecode {
                code,
                constants,
                expr,
            } => {
                path.push(Arc::from("code"));
                code.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("constants"));
                constants.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
                path.push(Arc::from("expr"));
                expr.collect_lazy_vector_infos_impl(path, infos, visited);
                path.pop();
            }
            WithAttributes { object, attributes } => {
                self.collect_attributes_lazy_infos(attributes, path, infos, visited);
                object.collect_lazy_vector_infos_impl(path, infos, visited);
            }

            Factor(_)
            | Namespace(_)
            | Null
            | Symbol(_)
            | Special { .. }
            | Builtin { .. }
            | GlobalEnv
            | BaseEnv
            | EmptyEnv
            | MissingArg
            | UnboundValue => {}
            Shared(_) => {}
        }
    }

    fn collect_attributes_lazy_infos(
        &self,
        attributes: &Attributes,
        path: &mut ObjectPath,
        infos: &mut Vec<(ObjectPath, &'static str, LazyVector)>,
        visited: &mut std::collections::HashSet<usize>,
    ) {
        for (key, value) in attributes.iter() {
            let segment = Arc::from(format!("@{}", key.as_ref()));
            path.push(segment);
            value.collect_lazy_vector_infos_impl(path, infos, visited);
            path.pop();
        }
    }
}

impl PartialEq for RObject {
    fn eq(&self, other: &Self) -> bool {
        use RObject::*;

        // Unwrap `Shared` by comparing through the read guard instead of
        // cloning the (potentially large) underlying object. The previous
        // implementation called `as_concrete()` on both sides unconditionally,
        // which deep-clones every value being compared -- even when neither
        // side is `Shared` at all. That made every `==` on a composite
        // RObject (List, WithAttributes, ...) pay for a full recursive clone
        // before the actual comparison, which is redundant and, on the dedup
        // table's linear scan over many concrete objects, turns an O(n)
        // scan into an O(n) scan of O(size) clones each.
        //
        // Fast path: two `Shared` wrappers around the same Arc are the same
        // object, so they are trivially equal. This also avoids re-entrant
        // read-lock acquisition on the same RwLock (the branch below would
        // hold a read guard while recursing into the second branch, which
        // std::sync::RwLock documents as a potential deadlock).
        if let (Shared(a), Shared(b)) = (self, other) {
            if Arc::ptr_eq(a, b) {
                return true;
            }
        }
        if let Shared(a) = self {
            return *a.read().unwrap() == *other;
        }
        if let Shared(b) = other {
            return *self == *b.read().unwrap();
        }

        match (self, other) {
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

impl From<i32> for Logical {
    fn from(value: i32) -> Self {
        match value {
            1 => Logical::True,
            0 => Logical::False,
            i32::MIN => Logical::Na,
            _ => Logical::Na,
        }
    }
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

    /// Recursively convert all Shared wrappers in attribute values to concrete objects.
    ///
    /// This is called from RObject::into_concrete_deep_impl to ensure attributes
    /// are also fully concrete.
    pub(crate) fn into_concrete_deep_impl(mut self, visited: &mut HashMap<usize, RObject>) -> Self {
        self.attrs = self
            .attrs
            .into_iter()
            .map(|(k, v)| {
                let concrete_obj = (*v).into_concrete_deep_impl(visited);
                (k, Box::new(concrete_obj))
            })
            .collect();
        self
    }
}

impl FactorData {
    pub(crate) fn base_attributes(&self) -> Attributes {
        let mut attrs = Attributes::new();
        attrs.insert(
            Arc::from("levels"),
            RObject::Character(self.levels.clone().into()),
        );

        let class = if self.ordered {
            vec![Arc::from("ordered"), Arc::from("factor")]
        } else {
            vec![Arc::from("factor")]
        };
        attrs.insert(Arc::from("class"), RObject::Character(class.into()));

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
