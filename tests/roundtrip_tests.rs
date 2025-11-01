use rds2rust::{read_rds, write_rds, Logical, RObject};

/// Helper to read an existing test file
fn read_test_file(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/data/{}", name)).expect("Failed to read test file")
}

/// Helper to check if test data exists
fn test_data_exists() -> bool {
    std::path::Path::new("tests/data").exists()
}

#[test]
fn test_roundtrip_null() {
    let obj = RObject::Null;
    let serialized = write_rds(&obj).expect("Failed to write NULL");
    let deserialized = read_rds(&serialized).expect("Failed to read NULL");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_integer_vector() {
    let obj = RObject::Integer(vec![1, 2, 3, 4, 5]);
    let serialized = write_rds(&obj).expect("Failed to write integer vector");
    let deserialized = read_rds(&serialized).expect("Failed to read integer vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_real_vector() {
    let obj = RObject::Real(vec![1.5, 2.5, 3.5]);
    let serialized = write_rds(&obj).expect("Failed to write real vector");
    let deserialized = read_rds(&serialized).expect("Failed to read real vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_character_vector() {
    let obj = RObject::Character(vec!["hello".to_string(), "world".to_string()]);
    let serialized = write_rds(&obj).expect("Failed to write character vector");
    let deserialized = read_rds(&serialized).expect("Failed to read character vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_list() {
    let obj = RObject::List(vec![
        RObject::Integer(vec![1, 2, 3]),
        RObject::Character(vec!["test".to_string()]),
        RObject::Real(vec![4.5]),
    ]);
    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read list");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_integer() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // Read an existing RDS file
    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing int");

    // Write it back and read again
    let serialized = write_rds(&obj).expect("Failed to write int");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized int");

    // Should be equal
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_real() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing real");

    let serialized = write_rds(&obj).expect("Failed to write real");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized real");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_character() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing character");

    let serialized = write_rds(&obj).expect("Failed to write character");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized character");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_list() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing list");

    let serialized = write_rds(&obj).expect("Failed to write list");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized list");

    assert_eq!(obj, deserialized);
}

// Logical vectors
#[test]
fn test_roundtrip_logical_vector() {
    let obj = RObject::Logical(vec![Logical::True, Logical::False, Logical::Na, Logical::True]);
    let serialized = write_rds(&obj).expect("Failed to write logical vector");
    let deserialized = read_rds(&serialized).expect("Failed to read logical vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_logical() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing logical");

    let serialized = write_rds(&obj).expect("Failed to write logical");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized logical");

    assert_eq!(obj, deserialized);
}

// Raw vectors
#[test]
fn test_roundtrip_raw_vector() {
    let obj = RObject::Raw(vec![0x01, 0x02, 0x03, 0xFF, 0x00]);
    let serialized = write_rds(&obj).expect("Failed to write raw vector");
    let deserialized = read_rds(&serialized).expect("Failed to read raw vector");
    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_raw() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("raw_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing raw");

    let serialized = write_rds(&obj).expect("Failed to write raw");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized raw");

    assert_eq!(obj, deserialized);
}

// Complex vectors
#[test]
fn test_roundtrip_existing_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing complex");

    let serialized = write_rds(&obj).expect("Failed to write complex");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized complex");

    assert_eq!(obj, deserialized);
}

// Data frames
#[test]
fn test_roundtrip_existing_dataframe_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing dataframe");

    let serialized = write_rds(&obj).expect("Failed to write dataframe");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized dataframe");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_dataframe_mixed() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_mixed.rds");
    let obj = read_rds(&data).expect("Failed to read existing mixed dataframe");

    let serialized = write_rds(&obj).expect("Failed to write mixed dataframe");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized mixed dataframe");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_dataframe_rownames() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_rownames.rds");
    let obj = read_rds(&data).expect("Failed to read existing dataframe with rownames");

    let serialized = write_rds(&obj).expect("Failed to write dataframe with rownames");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized dataframe with rownames");

    assert_eq!(obj, deserialized);
}

// Factors
#[test]
fn test_roundtrip_existing_factor_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing factor");

    let serialized = write_rds(&obj).expect("Failed to write factor");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized factor");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_factor_ordered() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_ordered.rds");
    let obj = read_rds(&data).expect("Failed to read existing ordered factor");

    let serialized = write_rds(&obj).expect("Failed to write ordered factor");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized ordered factor");

    assert_eq!(obj, deserialized);
}

// S3 objects
#[test]
fn test_roundtrip_existing_s3_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing S3 object");

    let serialized = write_rds(&obj).expect("Failed to write S3 object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S3 object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_s3_multi_class() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_multi_class.rds");
    let obj = read_rds(&data).expect("Failed to read existing S3 multi-class object");

    let serialized = write_rds(&obj).expect("Failed to write S3 multi-class object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S3 multi-class object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_s3_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_vector.rds");
    let obj = read_rds(&data).expect("Failed to read existing S3 vector object");

    let serialized = write_rds(&obj).expect("Failed to write S3 vector object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S3 vector object");

    assert_eq!(obj, deserialized);
}

// S4 objects
#[test]
fn test_roundtrip_existing_s4_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 object");

    let serialized = write_rds(&obj).expect("Failed to write S4 object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_s4_inheritance() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_inheritance.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 inheritance object");

    let serialized = write_rds(&obj).expect("Failed to write S4 inheritance object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 inheritance object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_s4_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_complex.rds");
    let obj = read_rds(&data).expect("Failed to read existing S4 complex object");

    let serialized = write_rds(&obj).expect("Failed to write S4 complex object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized S4 complex object");

    assert_eq!(obj, deserialized);
}

// Language objects
#[test]
fn test_roundtrip_existing_lang_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing simple language object");

    let serialized = write_rds(&obj).expect("Failed to write simple language object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized simple language object");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_lang_with_args() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_with_args.rds");
    let obj = read_rds(&data).expect("Failed to read existing language object with args");

    let serialized = write_rds(&obj).expect("Failed to write language object with args");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized language object with args");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_lang_nested() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_nested.rds");
    let obj = read_rds(&data).expect("Failed to read existing nested language object");

    let serialized = write_rds(&obj).expect("Failed to write nested language object");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized nested language object");

    assert_eq!(obj, deserialized);
}

// Expression vectors
#[test]
fn test_roundtrip_existing_expr_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_single.rds");
    let obj = read_rds(&data).expect("Failed to read existing single expression");

    let serialized = write_rds(&obj).expect("Failed to write single expression");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized single expression");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_expr_multiple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_multiple.rds");
    let obj = read_rds(&data).expect("Failed to read existing multiple expressions");

    let serialized = write_rds(&obj).expect("Failed to write multiple expressions");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized multiple expressions");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_expr_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_empty.rds");
    let obj = read_rds(&data).expect("Failed to read existing empty expression");

    let serialized = write_rds(&obj).expect("Failed to write empty expression");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized empty expression");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_expr_calls() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_calls.rds");
    let obj = read_rds(&data).expect("Failed to read existing expression with calls");

    let serialized = write_rds(&obj).expect("Failed to write expression with calls");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized expression with calls");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_expr_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_complex.rds");
    let obj = read_rds(&data).expect("Failed to read existing complex expression");

    let serialized = write_rds(&obj).expect("Failed to write complex expression");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized complex expression");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_expr_manual() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("expr_manual.rds");
    let obj = read_rds(&data).expect("Failed to read existing manually created expression");

    let serialized = write_rds(&obj).expect("Failed to write manually created expression");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized manually created expression");

    assert_eq!(obj, deserialized);
}

// Formulas (S3 objects with language base)
#[test]
fn test_roundtrip_existing_formula_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_simple.rds");
    let obj = read_rds(&data).expect("Failed to read existing simple formula");

    let serialized = write_rds(&obj).expect("Failed to write simple formula");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized simple formula");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_formula_multiple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_multiple.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with multiple predictors");

    let serialized = write_rds(&obj).expect("Failed to write formula with multiple predictors");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with multiple predictors");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_formula_interaction() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_interaction.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with interaction");

    let serialized = write_rds(&obj).expect("Failed to write formula with interaction");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with interaction");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_formula_functions() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_functions.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula with functions");

    let serialized = write_rds(&obj).expect("Failed to write formula with functions");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula with functions");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_formula_no_intercept() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_no_intercept.rds");
    let obj = read_rds(&data).expect("Failed to read existing formula without intercept");

    let serialized = write_rds(&obj).expect("Failed to write formula without intercept");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized formula without intercept");

    assert_eq!(obj, deserialized);
}

#[test]
fn test_roundtrip_existing_formula_one_sided() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("formula_one_sided.rds");
    let obj = read_rds(&data).expect("Failed to read existing one-sided formula");

    let serialized = write_rds(&obj).expect("Failed to write one-sided formula");
    let deserialized = read_rds(&serialized).expect("Failed to read serialized one-sided formula");

    assert_eq!(obj, deserialized);
}

