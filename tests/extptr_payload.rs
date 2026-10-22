#![cfg(not(target_arch = "wasm32"))]
//! External pointers carry a serialized payload: R writes the protected value
//! and the tag after the type word. Generated in R with:
//! a_first <- 1; p <- new("externalptr"); z_last <- 2
//! save(a_first, p, z_last, file = "extptr.rda", version = 2, compress = FALSE)
//! Skipping the payload used to end the workspace pairlist at the pointer, so
//! every later binding was silently dropped.
use rds2rust::{
    read_rds_with_config, traverse_rds_streaming, ObjectPath, ParseConfig, ParseMode, RObject,
    RdsInput, RdsVisitor, VisitAction,
};

#[test]
fn workspace_bindings_after_an_external_pointer_are_kept() {
    let payload = workspace_payload();
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let parsed = read_rds_with_config(payload, ParseConfig::default().with_mode(mode))
            .expect("workspace with an external pointer must parse");
        let RObject::Pairlist(elements) = parsed.object.into_concrete() else {
            panic!("workspace must remain a pairlist");
        };
        let names: Vec<_> = elements.iter().map(|e| e.tag.as_deref()).collect();
        assert_eq!(names, vec![Some("a_first"), Some("p"), Some("z_last")]);
    }
}

const WORKSPACE: &[u8] = include_bytes!("fixtures/extptr.rda");

fn workspace_payload() -> &'static [u8] {
    WORKSPACE
        .strip_prefix(b"RDX2\n")
        .expect("R workspace header")
}

struct Slice(&'static [u8]);

impl RdsInput for Slice {
    fn read_at(&self, offset: u64, len: usize) -> rds2rust::Result<Vec<u8>> {
        let start = (offset as usize).min(self.0.len());
        let end = start.saturating_add(len).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }

    fn len(&self) -> Option<u64> {
        Some(self.0.len() as u64)
    }
}

#[derive(Default)]
struct TypeCounter {
    reals: usize,
}

impl RdsVisitor for TypeCounter {
    type Error = std::convert::Infallible;

    fn on_object_start(
        &mut self,
        _path: &ObjectPath,
        obj_type: &str,
    ) -> Result<VisitAction, Self::Error> {
        if obj_type == "Real" {
            self.reals += 1;
        }
        Ok(VisitAction::Continue)
    }
}

#[test]
fn streaming_traversal_consumes_the_external_pointer_payload() {
    let source = Slice(workspace_payload());
    let mut visitor = TypeCounter::default();
    traverse_rds_streaming(&source, ParseConfig::default(), &mut visitor)
        .expect("stream with an external pointer must traverse");
    assert_eq!(
        visitor.reals, 2,
        "both reals around the pointer are visited"
    );
}
