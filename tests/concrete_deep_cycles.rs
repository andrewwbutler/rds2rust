//! `into_concrete_deep` on cyclic and shared-but-acyclic object graphs.
//! Generated in R with:
//! e <- new.env(parent = emptyenv()); e$self <- e
//! q <- new("externalptr"); attr(q, "self") <- q
//! plain <- new.env(parent = emptyenv()); plain$v <- 1
//! x <- list(cyc = e, ptr = q, plain1 = plain, plain2 = plain, after = 42)
//! save(x, file = "concrete-deep-cycles.rda", version = 2, compress = FALSE)
//! Both cycles used to overflow the stack and abort the process, because a
//! node was only recorded as visited after its contents were converted.
use rds2rust::{read_rds, RObject};

const FIXTURE: &[u8] = include_bytes!("fixtures/concrete-deep-cycles.rda");

/// Count `Shared` nodes without following them, so this always terminates.
fn count_shared(obj: &RObject) -> usize {
    match obj {
        RObject::Shared(_) => 1,
        RObject::List(items) | RObject::Expression(items) => items.iter().map(count_shared).sum(),
        RObject::Pairlist(elements) => elements.iter().map(|e| count_shared(&e.value)).sum(),
        RObject::Environment {
            enclosing,
            frame,
            hashtab,
        } => count_shared(enclosing) + count_shared(frame) + count_shared(hashtab),
        RObject::S3Object(s3) => {
            count_shared(&s3.base)
                + s3.attributes
                    .iter()
                    .map(|(_, v)| count_shared(v))
                    .sum::<usize>()
        }
        RObject::WithAttributes { object, attributes } => {
            count_shared(object)
                + attributes
                    .iter()
                    .map(|(_, v)| count_shared(v))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn items() -> Vec<RObject> {
    let payload = FIXTURE.strip_prefix(b"RDX2\n").expect("R workspace header");
    let parsed = read_rds(payload).expect("cycle fixture must parse");
    let RObject::Pairlist(mut bindings) = parsed.object.into_concrete_deep() else {
        panic!("workspace must be a pairlist");
    };
    let RObject::WithAttributes { object, .. } = bindings.remove(0).value else {
        panic!("named list must carry its names attribute");
    };
    let RObject::List(items) = *object else {
        panic!("fixture must be a list");
    };
    items
}

#[test]
fn cyclic_graphs_convert_without_overflowing() {
    let items = items();
    assert_eq!(items.len(), 5);
    let RObject::Real(after) = &items[4] else {
        panic!("element after the cycles must stay a real vector");
    };
    assert_eq!(*after, [42.0]);
}

#[test]
fn only_the_edge_closing_a_cycle_stays_shared() {
    let items = items();
    assert_eq!(count_shared(&items[0]), 1, "environment that binds itself");
    assert_eq!(
        count_shared(&items[1]),
        1,
        "pointer whose attribute is itself"
    );
}

#[test]
fn shared_acyclic_references_become_fully_concrete() {
    let items = items();
    for plain in &items[2..4] {
        assert_eq!(count_shared(plain), 0);
        assert!(matches!(plain, RObject::Environment { .. }));
    }
    assert_eq!(items[2], items[3]);
}
