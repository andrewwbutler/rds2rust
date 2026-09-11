#![cfg(not(target_arch = "wasm32"))]
//! Resource boundaries using small objects written by this library.
use rds2rust::{read_rds_with_config, write_rds, ParseConfig, ParseMode, RObject};

fn nested_list() -> RObject {
    RObject::List(vec![RObject::List(vec![RObject::List(vec![
        RObject::Null,
    ])])])
}

#[test]
fn small_nesting_limit_rejects_an_ordinary_nested_list() {
    let bytes = write_rds(&nested_list()).unwrap();
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let config = ParseConfig::default()
            .with_mode(mode)
            .with_max_nesting_depth(2);
        let error = read_rds_with_config(&bytes, config).unwrap_err();
        assert!(error.to_string().contains("nesting limit"), "{error}");
    }
    assert!(read_rds_with_config(&bytes, ParseConfig::default()).is_ok());
}

#[test]
fn object_vector_limits_use_materialized_element_width() {
    let object = RObject::List(vec![RObject::Null; 4]);
    let bytes = write_rds(&object).unwrap();
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let config = ParseConfig::default()
            .with_mode(mode)
            .with_lazy_threshold(0)
            .with_max_allocation_bytes(3 * std::mem::size_of::<RObject>());
        let error = read_rds_with_config(&bytes, config).unwrap_err();
        assert!(
            error.to_string().contains("Materialized allocation"),
            "{error}"
        );
    }
    assert!(read_rds_with_config(&bytes, ParseConfig::default()).is_ok());
}

#[test]
fn lazy_primitive_vectors_do_not_need_a_materialized_buffer_budget() {
    let object = RObject::Integer((0..32).collect::<Vec<i32>>().into());
    let bytes = write_rds(&object).unwrap();
    let config = ParseConfig::default()
        .with_mode(ParseMode::LazyMetadata)
        .with_lazy_threshold(0)
        .with_max_allocation_bytes(16);
    let result = read_rds_with_config(&bytes, config).unwrap();
    assert!(!result.object.is_fully_loaded());
}

#[test]
fn streaming_uses_the_same_nesting_limit() {
    use rds2rust::{traverse_rds_streaming, MmapRdsSource, RdsVisitor};
    struct Visitor;
    impl RdsVisitor for Visitor {
        type Error = std::convert::Infallible;
        fn on_object_start(
            &mut self,
            _: &rds2rust::ObjectPath,
            _: &str,
        ) -> Result<rds2rust::VisitAction, Self::Error> {
            Ok(rds2rust::VisitAction::Continue)
        }
    }
    let bytes = write_rds(&nested_list()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.rds");
    std::fs::write(&path, bytes).unwrap();
    let source = MmapRdsSource::from_path(&path).unwrap();
    let result = traverse_rds_streaming(
        &source,
        ParseConfig::default().with_max_nesting_depth(2),
        &mut Visitor,
    );
    assert!(result.unwrap_err().to_string().contains("nesting limit"));
    traverse_rds_streaming(&source, ParseConfig::default(), &mut Visitor).unwrap();
}

#[test]
fn siblings_release_the_nesting_budget() {
    let object = RObject::List(vec![RObject::List(vec![RObject::Null]); 8]);
    let bytes = write_rds(&object).unwrap();
    for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
        let config = ParseConfig::default()
            .with_mode(mode)
            .with_max_nesting_depth(3);
        read_rds_with_config(&bytes, config).unwrap();
    }
}

#[test]
fn eager_character_metadata_uses_the_allocation_limit_in_lazy_mode() {
    let object = RObject::Character(vec![Some(std::sync::Arc::from("a")); 4].into());
    let bytes = write_rds(&object).unwrap();
    let config = ParseConfig::default()
        .with_mode(ParseMode::LazyMetadata)
        .with_lazy_threshold(10)
        .with_max_allocation_bytes(16);
    let error = read_rds_with_config(&bytes, config).unwrap_err();
    assert!(
        error.to_string().contains("Materialized allocation"),
        "{error}"
    );
}

#[test]
fn small_primitive_vectors_loaded_in_lazy_mode_respect_the_budget() {
    let bytes = write_rds(&RObject::Integer(vec![1, 2, 3, 4].into())).unwrap();
    let config = ParseConfig::default()
        .with_mode(ParseMode::LazyMetadata)
        .with_lazy_threshold(10)
        .with_max_allocation_bytes(8);
    let error = read_rds_with_config(&bytes, config).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("allocation"),
        "{error}"
    );
}

#[test]
fn lazy_vector_length_is_bounded_even_without_materialization() {
    let bytes = write_rds(&RObject::Integer(vec![1; 8].into())).unwrap();
    let config = ParseConfig::default()
        .with_mode(ParseMode::LazyMetadata)
        .with_lazy_threshold(0)
        .with_max_vector_length(6);
    let error = read_rds_with_config(&bytes, config).unwrap_err();
    assert!(error.to_string().contains("Length 8 exceeds"), "{error}");
}
