//! Two separately serialized but structurally equal cyclic environments.
//! Generated in R with:
//! a <- new.env(parent = emptyenv()); a$self <- a
//! b <- new.env(parent = emptyenv()); b$self <- b
//! x <- pairlist(a); y <- pairlist(b)
//! save(x, y, file = "cyclic-pairlists.rda", version = 2, compress = FALSE)
//! Deduplicating the enclosing pairlists used to recurse indefinitely in ==.
use rds2rust::{read_rds_with_config, ParseConfig, ParseMode, RObject};

#[test]
fn cyclic_composite_workspaces_parse_without_dedup_recursion() {
    let bytes = include_bytes!("fixtures/cyclic-pairlists.rda");
    let payload = bytes.strip_prefix(b"RDX2\n").expect("R workspace header");
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let parsed = read_rds_with_config(payload, ParseConfig::default().with_mode(mode))
            .expect("cyclic environment workspace must parse");
        let RObject::Pairlist(elements) = parsed.object.into_concrete() else {
            panic!("workspace must remain a pairlist");
        };
        let names: Vec<_> = elements.iter().map(|e| e.tag.as_deref()).collect();
        assert_eq!(names, vec![Some("x"), Some("y")]);
    }
}
