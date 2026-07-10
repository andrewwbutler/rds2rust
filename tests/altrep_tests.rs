//! Tests for ALTREP (Alternative Representations) support.
//!
//! ALTREP was introduced in R 3.5.0 for efficient memory representations.
//! These tests verify that we can parse and materialize ALTREP objects.

use rds2rust::{read_rds, RObject};
use std::fs;
use std::path::Path;

fn test_data_exists() -> bool {
    Path::new("tests/data").exists()
}

fn read_test_file(filename: &str) -> Vec<u8> {
    let path = format!("tests/data/{}", filename);
    fs::read(&path).unwrap_or_else(|_| panic!("Failed to read test file: {}", path))
}

#[test]
fn test_altrep_intseq() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_intseq.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse ALTREP integer sequence")
        .object;

    // ALTREP compact_intseq should be materialized to a regular integer vector
    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1, "First element should be 1");
            assert_eq!(vec[999], 1000, "Last element should be 1000");

            // Verify it's a proper sequence
            for (i, &val) in vec.iter().enumerate() {
                assert_eq!(val, (i + 1) as i32, "Element {} should be {}", i, i + 1);
            }
        }
        _ => panic!(
            "Expected Integer vector, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_realseq() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_realseq.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse ALTREP real sequence")
        .object;

    // ALTREP realseq might be materialized to integer or real vector
    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1.0, "First element should be 1.0");
            assert_eq!(vec[999], 1000.0, "Last element should be 1000.0");
        }
        RObject::Integer(vec) => {
            // Some ALTREP realseq are materialized as integers if stride is 1
            assert_eq!(vec.len(), 1000, "Expected 1000 elements in sequence");
            assert_eq!(vec[0], 1, "First element should be 1");
            assert_eq!(vec[999], 1000, "Last element should be 1000");
        }
        _ => panic!(
            "Expected Real or Integer vector, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_in_list() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_in_list.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse list containing ALTREP")
        .object;

    // The object should be a list with attributes
    let (list, attrs) = match obj {
        RObject::WithAttributes { object, attributes } => (object, attributes),
        _ => panic!(
            "Expected WithAttributes, got {:?}",
            std::mem::discriminant(&obj)
        ),
    };

    // Check the list has correct names
    if let Some(RObject::Character(names)) = attrs.get("names") {
        assert_eq!(names.len(), 3);
        assert_eq!(names[0].as_deref(), Some("seq"));
        assert_eq!(names[1].as_deref(), Some("data"));
        assert_eq!(names[2].as_deref(), Some("another_seq"));
    } else {
        panic!("Expected 'names' attribute");
    }

    // Check the list elements
    if let RObject::List(elements) = list.as_ref() {
        assert_eq!(elements.len(), 3);

        // First element: ALTREP sequence 1:100
        match &elements[0] {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 100);
                assert_eq!(vec[0], 1);
                assert_eq!(vec[99], 100);
            }
            _ => panic!("Expected first element to be Integer vector"),
        }

        // Second element: regular real vector
        match &elements[1] {
            RObject::Real(vec) => {
                assert_eq!(vec.len(), 3);
                assert_eq!(vec[0], 1.5);
                assert_eq!(vec[1], 2.5);
                assert_eq!(vec[2], 3.5);
            }
            _ => panic!("Expected second element to be Real vector"),
        }

        // Third element: ALTREP sequence 50:150
        match &elements[2] {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 101);
                assert_eq!(vec[0], 50);
                assert_eq!(vec[100], 150);
            }
            _ => panic!("Expected third element to be Integer vector"),
        }
    } else {
        panic!("Expected List");
    }
}

#[test]
fn test_regular_int_no_altrep() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("regular_int.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse regular integer vector")
        .object;

    // Regular integer vector (not ALTREP) should parse normally
    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 5);
            assert_eq!(vec[0], 1);
            assert_eq!(vec[1], 2);
            assert_eq!(vec[2], 3);
            assert_eq!(vec[3], 4);
            assert_eq!(vec[4], 5);
        }
        _ => panic!(
            "Expected Integer vector, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_matrix_real() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_matrix_real.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse ALTREP matrix")
        .object;

    // Should be a Real vector with dim attribute (matrix)
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check the inner object is Real (not Null!)
            match object.as_ref() {
                RObject::Real(vec) => {
                    assert_eq!(vec.len(), 6000, "Expected 200*30 = 6000 elements");
                }
                RObject::Null => {
                    panic!("BUG: Matrix data is Null - ALTREP wrapper not handled correctly!");
                }
                _ => panic!("Expected Real vector inside attributes"),
            }

            // Check for dim attribute
            assert!(
                attributes.get("dim").is_some(),
                "Matrix should have dim attribute"
            );
        }
        RObject::Real(vec) => {
            // Might not have attributes if R didn't use ALTREP
            assert_eq!(vec.len(), 6000, "Expected 200*30 = 6000 elements");
        }
        RObject::Null => {
            panic!("BUG: Matrix is Null - ALTREP wrapper not handled correctly!");
        }
        _ => panic!(
            "Expected Real or WithAttributes containing Real, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_matrix_dimnames() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("altrep_matrix_dimnames.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse ALTREP matrix with dimnames")
        .object;

    // Should be a Real vector with dim and dimnames attributes
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check the inner object is Real (not Null!)
            match object.as_ref() {
                RObject::Real(vec) => {
                    assert_eq!(vec.len(), 1000, "Expected 100*10 = 1000 elements");
                }
                RObject::Null => {
                    panic!("BUG: Matrix data is Null - ALTREP wrapper not handled correctly!");
                }
                _ => panic!("Expected Real vector inside attributes"),
            }

            // Check for dim and dimnames attributes
            assert!(
                attributes.get("dim").is_some(),
                "Matrix should have dim attribute"
            );
            assert!(
                attributes.get("dimnames").is_some(),
                "Matrix should have dimnames attribute"
            );
        }
        RObject::Real(vec) => {
            // Might not have attributes if R didn't use ALTREP
            assert_eq!(vec.len(), 1000, "Expected 100*10 = 1000 elements");
        }
        RObject::Null => {
            panic!("BUG: Matrix is Null - ALTREP wrapper not handled correctly!");
        }
        _ => panic!(
            "Expected Real or WithAttributes containing Real, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_wrap_real() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // This file might not exist if R version doesn't support wrap_meta
    if !Path::new("tests/data/altrep_wrap_real.rds").exists() {
        eprintln!("Skipping test: altrep_wrap_real.rds not available");
        return;
    }

    let data = read_test_file("altrep_wrap_real.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse wrap_real ALTREP")
        .object;

    // wrap_real should be materialized to a regular Real vector
    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1000, "Expected 1000 elements");
            // Verify it contains actual data (not all zeros/NaN)
            assert!(
                vec.iter().any(|&x| x != 0.0 && !x.is_nan()),
                "Vector should contain non-zero values"
            );
        }
        RObject::WithAttributes { object, .. } => match object.as_ref() {
            RObject::Real(vec) => {
                assert_eq!(vec.len(), 1000, "Expected 1000 elements");
            }
            RObject::Null => {
                panic!("BUG: wrap_real resulted in Null - wrapper not handled!");
            }
            _ => panic!("Expected Real vector"),
        },
        RObject::Null => {
            panic!("BUG: wrap_real resulted in Null - wrapper not handled correctly!");
        }
        _ => panic!(
            "Expected Real vector, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_wrap_int() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // This file might not exist if R version doesn't support wrap_meta
    if !Path::new("tests/data/altrep_wrap_int.rds").exists() {
        eprintln!("Skipping test: altrep_wrap_int.rds not available");
        return;
    }

    let data = read_test_file("altrep_wrap_int.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse wrap_int ALTREP")
        .object;

    // wrap_int should be materialized to a regular Integer vector
    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 500, "Expected 500 elements");
            // Verify it contains actual data
            assert!(
                vec.iter().any(|&x| x != 0),
                "Vector should contain non-zero values"
            );
        }
        RObject::WithAttributes { object, .. } => match object.as_ref() {
            RObject::Integer(vec) => {
                assert_eq!(vec.len(), 500, "Expected 500 elements");
            }
            RObject::Null => {
                panic!("BUG: wrap_int resulted in Null - wrapper not handled!");
            }
            _ => panic!("Expected Integer vector"),
        },
        RObject::Null => {
            panic!("BUG: wrap_int resulted in Null - wrapper not handled correctly!");
        }
        _ => panic!(
            "Expected Integer vector, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}

#[test]
fn test_altrep_wrap_matrix() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    // This file might not exist if R version doesn't support wrap_meta
    if !Path::new("tests/data/altrep_wrap_matrix.rds").exists() {
        eprintln!("Skipping test: altrep_wrap_matrix.rds not available");
        return;
    }

    let data = read_test_file("altrep_wrap_matrix.rds");
    let obj = read_rds(&data)
        .expect("Failed to parse wrapped matrix ALTREP")
        .object;

    // Wrapped matrix should have Real data with attributes
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check the inner object is Real (not Null!)
            match object.as_ref() {
                RObject::Real(vec) => {
                    assert_eq!(vec.len(), 1000, "Expected 100*10 = 1000 elements");
                }
                RObject::Null => {
                    panic!("BUG: Wrapped matrix data is Null - ALTREP wrapper not handled!");
                }
                _ => panic!("Expected Real vector inside attributes"),
            }

            // Check for expected attributes
            assert!(
                attributes.get("dim").is_some(),
                "Matrix should have dim attribute"
            );
            assert!(
                attributes.get("dimnames").is_some(),
                "Matrix should have dimnames attribute"
            );
        }
        RObject::Real(vec) => {
            // Might not have attributes
            assert_eq!(vec.len(), 1000, "Expected 100*10 = 1000 elements");
        }
        RObject::Null => {
            panic!("BUG: Wrapped matrix is Null - ALTREP wrapper not handled correctly!");
        }
        _ => panic!(
            "Expected Real or WithAttributes containing Real, got {:?}",
            std::mem::discriminant(&obj)
        ),
    }
}
