//! Regression coverage for dataframe row names under lazy parsing.
#![cfg(not(target_arch = "wasm32"))]

use rds2rust::{
    read_rds_with_config, read_rds_with_input, write_rds, Attributes, ChunkedRdsSource, FactorData,
    ParseConfig, RObject, VectorData,
};
use std::sync::Arc;

fn dataframe(row_names: RObject, factor: bool, n: usize) -> RObject {
    let column = if factor {
        RObject::Factor(Box::new(FactorData {
            values: vec![1; n],
            levels: vec![Some(Arc::from("A"))],
            ordered: false,
        }))
    } else {
        RObject::Real(vec![1.0; n].into())
    };
    let mut attributes = Attributes::new();
    attributes.insert(
        Arc::from("names"),
        RObject::Character(vec![Some(Arc::from("group"))].into()),
    );
    attributes.insert(
        Arc::from("class"),
        RObject::Character(vec![Some(Arc::from("data.frame"))].into()),
    );
    attributes.insert(Arc::from("row.names"), row_names);
    RObject::WithAttributes {
        object: Box::new(RObject::List(vec![column])),
        attributes,
    }
}

fn check_rows(row_names: RObject, expected: Vec<Option<Arc<str>>>, factor: bool) {
    let data = write_rds(&dataframe(row_names, factor, expected.len())).unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &data).unwrap();
    let source = ChunkedRdsSource::from_path(file.path()).unwrap();
    for parsed in [
        read_rds_with_config(&data, ParseConfig::for_trusted_large_file()).unwrap(),
        read_rds_with_input(&source, ParseConfig::for_trusted_large_file()).unwrap(),
    ] {
        let RObject::DataFrame(frame) = parsed.object else {
            panic!("expected dataframe")
        };
        assert_eq!(frame.row_names, expected);
        // Loading row identifiers must not eagerly load the data columns.
        let mut column = &frame.columns["group"];
        if let RObject::S3Object(s3) = column {
            column = &s3.base;
        }
        assert!(matches!(column,
            RObject::Integer(VectorData::Lazy(span)) | RObject::Real(VectorData::Lazy(span))
            if span.length == expected.len()
        ));
    }
}

#[test]
fn lazy_dataframe_preserves_character_row_names() {
    let names: Vec<_> = (0..101)
        .map(|i| Some(Arc::from(format!("cell-{}", 101 - i))))
        .collect();
    for factor in [true, false] {
        check_rows(
            RObject::Character(names.clone().into()),
            names.clone(),
            factor,
        );
    }
}

#[test]
fn lazy_dataframe_preserves_explicit_integer_row_names() {
    let indices: Vec<i32> = (101..202).rev().collect();
    let names = indices
        .iter()
        .map(|i| Some(Arc::from(i.to_string())))
        .collect();
    check_rows(RObject::Integer(indices.into()), names, true);
}

#[test]
fn lazy_dataframe_preserves_compact_row_names() {
    let names = (1..=101).map(|i| Some(Arc::from(i.to_string()))).collect();
    check_rows(
        RObject::Integer(vec![RObject::NA_INTEGER, -101].into()),
        names,
        true,
    );
}
