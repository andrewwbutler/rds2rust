// Integration tests for rds2rust
//
// Note: These tests require test RDS files to be generated.
// Run: Rscript tests/generate_test_data.R

use rds2rust::{read_rds, RObject};
use std::fs;
use std::path::Path;

// Helper function to read test data file
fn read_test_file(filename: &str) -> Vec<u8> {
    let path = Path::new("tests/data").join(filename);
    fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "Failed to read test file: {}. Did you run 'Rscript tests/generate_test_data.R'?",
            path.display()
        )
    })
}

// Helper to check if test data exists
fn test_data_exists() -> bool {
    Path::new("tests/data/null.rds").exists()
}

#[test]
fn test_null() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("null.rds");
    let obj = read_rds(&data).expect("Failed to parse NULL");

    match obj {
        RObject::Null => {} // Success
        other => panic!("Expected Null, got {:?}", other),
    }
}

#[test]
fn test_integer_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_single.rds");
    let obj = read_rds(&data).expect("Failed to parse integer");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1);
        }
        other => panic!("Expected Integer, got {:?}", other),
    }
}

#[test]
fn test_integer_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse integer vector");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 10);
            assert_eq!(vec, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        }
        other => panic!("Expected Integer vector, got {:?}", other),
    }
}

#[test]
fn test_real_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_single.rds");
    let obj = read_rds(&data).expect("Failed to parse real");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], 1.5);
        }
        other => panic!("Expected Real, got {:?}", other),
    }
}

#[test]
fn test_logical_true() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_true.rds");
    let obj = read_rds(&data).expect("Failed to parse logical");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], rds2rust::Logical::True);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_character_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_single.rds");
    let obj = read_rds(&data).expect("Failed to parse character");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], "hello");
        }
        other => panic!("Expected Character, got {:?}", other),
    }
}

#[test]
fn test_character_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse character vector");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], "foo");
            assert_eq!(vec[1], "bar");
            assert_eq!(vec[2], "baz");
        }
        other => panic!("Expected Character vector, got {:?}", other),
    }
}

#[test]
fn test_character_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_with_na.rds");
    let obj = read_rds(&data).expect("Failed to parse character with NA");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], "test");
            assert_eq!(vec[1], "NA"); // NA_character_ is currently parsed as "NA"
            assert_eq!(vec[2], "string");
        }
        other => panic!("Expected Character vector with NA, got {:?}", other),
    }
}

#[test]
fn test_character_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("char_empty.rds");
    let obj = read_rds(&data).expect("Failed to parse empty character vector");

    match obj {
        RObject::Character(vec) => {
            assert_eq!(vec.len(), 0);
        }
        other => panic!("Expected empty Character vector, got {:?}", other),
    }
}

#[test]
fn test_logical_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse logical vector");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], rds2rust::Logical::True);
            assert_eq!(vec[1], rds2rust::Logical::False);
            assert_eq!(vec[2], rds2rust::Logical::Na);
            assert_eq!(vec[3], rds2rust::Logical::True);
        }
        other => panic!("Expected Logical vector, got {:?}", other),
    }
}

#[test]
fn test_logical_false() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("logical_false.rds");
    let obj = read_rds(&data).expect("Failed to parse logical FALSE");

    match obj {
        RObject::Logical(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0], rds2rust::Logical::False);
        }
        other => panic!("Expected Logical, got {:?}", other),
    }
}

#[test]
fn test_real_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse real vector");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 4);
            assert_eq!(vec[0], 1.1);
            assert_eq!(vec[1], 2.2);
            assert_eq!(vec[2], 3.3);
            assert_eq!(vec[3], 4.4);
        }
        other => panic!("Expected Real vector, got {:?}", other),
    }
}

#[test]
fn test_real_special() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_special.rds");
    let obj = read_rds(&data).expect("Failed to parse real with special values");

    match obj {
        RObject::Real(vec) => {
            assert_eq!(vec.len(), 5);
            assert_eq!(vec[0], 1.5);
            // vec[1] is NA_real_ (a specific NaN bit pattern)
            assert!(vec[1].is_nan());
            assert_eq!(vec[2], f64::INFINITY);
            assert_eq!(vec[3], f64::NEG_INFINITY);
            // vec[4] is NaN
            assert!(vec[4].is_nan());
        }
        other => panic!("Expected Real vector with special values, got {:?}", other),
    }
}

#[test]
fn test_integer_with_na() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_with_na.rds");
    let obj = read_rds(&data).expect("Failed to parse integer with NA");

    match obj {
        RObject::Integer(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 1);
            assert_eq!(vec[1], i32::MIN); // NA_integer_ is i32::MIN
            assert_eq!(vec[2], 3);
        }
        other => panic!("Expected Integer vector with NA, got {:?}", other),
    }
}

#[test]
fn test_list_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple list");

    match obj {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 3);
            // Each element should be an integer vector with one element
            match &elements[0] {
                RObject::Integer(v) => assert_eq!(v, &vec![1]),
                other => panic!("Expected Integer, got {:?}", other),
            }
            match &elements[1] {
                RObject::Integer(v) => assert_eq!(v, &vec![2]),
                other => panic!("Expected Integer, got {:?}", other),
            }
            match &elements[2] {
                RObject::Integer(v) => assert_eq!(v, &vec![3]),
                other => panic!("Expected Integer, got {:?}", other),
            }
        }
        other => panic!("Expected List, got {:?}", other),
    }
}

#[test]
fn test_list_empty() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("list_empty.rds");
    let obj = read_rds(&data).expect("Failed to parse empty list");

    match obj {
        RObject::List(elements) => {
            assert_eq!(elements.len(), 0);
        }
        other => panic!("Expected empty List, got {:?}", other),
    }
}

#[test]
fn test_int_named() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("int_named.rds");
    let obj = read_rds(&data).expect("Failed to parse named integer vector");

    // Named vectors have a "names" attribute
    match obj {
        RObject::WithAttributes { object, attributes } => {
            // The object should be an integer vector with 3 elements
            match *object {
                RObject::Integer(ref vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 1);
                    assert_eq!(vec[1], 2);
                    assert_eq!(vec[2], 3);
                }
                _ => panic!("Expected Integer vector"),
            }

            // Check that we have a "names" attribute
            let names = attributes.get("names");
            assert!(names.is_some(), "Expected 'names' attribute");

            // The names should be a character vector ["a", "b", "c"]
            match names.unwrap() {
                RObject::Character(names_vec) => {
                    assert_eq!(names_vec.len(), 3);
                    assert_eq!(names_vec[0], "a");
                    assert_eq!(names_vec[1], "b");
                    assert_eq!(names_vec[2], "c");
                }
                _ => panic!("Expected Character vector for names"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

#[test]
fn test_real_named() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("real_named.rds");
    let obj = read_rds(&data).expect("Failed to parse named real vector");

    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Check the vector values
            match *object {
                RObject::Real(ref vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 1.5);
                    assert_eq!(vec[1], 2.5);
                    assert_eq!(vec[2], 3.5);
                }
                _ => panic!("Expected Real vector"),
            }

            // Check the names attribute
            let names = attributes.get("names");
            assert!(names.is_some(), "Expected 'names' attribute");

            match names.unwrap() {
                RObject::Character(names_vec) => {
                    assert_eq!(names_vec.len(), 3);
                    assert_eq!(names_vec[0], "x");
                    assert_eq!(names_vec[1], "y");
                    assert_eq!(names_vec[2], "z");
                }
                _ => panic!("Expected Character vector for names"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

#[test]
fn test_matrix_int() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("matrix_int.rds");
    let obj = read_rds(&data).expect("Failed to parse integer matrix");

    match obj {
        RObject::WithAttributes { object, attributes } => {
            // Matrix is stored as a vector in column-major order
            match *object {
                RObject::Integer(ref vec) => {
                    assert_eq!(vec.len(), 6);
                    // Matrix is 2x3, column-major: [1,2] [3,4] [5,6]
                    assert_eq!(vec, &vec![1, 2, 3, 4, 5, 6]);
                }
                _ => panic!("Expected Integer vector"),
            }

            // Check the dim attribute
            let dim = attributes.get("dim");
            assert!(dim.is_some(), "Expected 'dim' attribute");

            match dim.unwrap() {
                RObject::Integer(dim_vec) => {
                    assert_eq!(dim_vec.len(), 2);
                    assert_eq!(dim_vec[0], 2); // nrow
                    assert_eq!(dim_vec[1], 3); // ncol
                }
                _ => panic!("Expected Integer vector for dim"),
            }
        }
        _ => panic!("Expected object with attributes, got {:?}", obj),
    }
}

#[test]
fn test_dataframe_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple data frame");

    match obj {
        RObject::DataFrame { columns, row_names } => {
            // Check that we have 3 columns
            assert_eq!(columns.len(), 3);

            // Check column "x" (integers 1, 2, 3)
            match columns.get("x") {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec, &vec![1, 2, 3]);
                }
                _ => panic!("Expected integer column 'x'"),
            }

            // Check column "y" (characters "a", "b", "c")
            match columns.get("y") {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], "a");
                    assert_eq!(vec[1], "b");
                    assert_eq!(vec[2], "c");
                }
                _ => panic!("Expected character column 'y'"),
            }

            // Check column "z" (logicals TRUE, FALSE, TRUE)
            match columns.get("z") {
                Some(RObject::Logical(vec)) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], rds2rust::Logical::True);
                    assert_eq!(vec[1], rds2rust::Logical::False);
                    assert_eq!(vec[2], rds2rust::Logical::True);
                }
                _ => panic!("Expected logical column 'z'"),
            }

            // Check row names (default: "1", "2", "3")
            assert_eq!(row_names.len(), 3);
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}

#[test]
fn test_dataframe_mixed() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_mixed.rds");
    let obj = read_rds(&data).expect("Failed to parse mixed data frame");

    match obj {
        RObject::DataFrame { columns, .. } => {
            // Check that we have 4 columns
            assert_eq!(columns.len(), 4);

            // Verify int_col
            match columns.get("int_col") {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec, &vec![1, 2, 3, 4]);
                }
                _ => panic!("Expected integer column 'int_col'"),
            }

            // Verify real_col
            match columns.get("real_col") {
                Some(RObject::Real(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0], 1.1);
                    assert_eq!(vec[3], 4.4);
                }
                _ => panic!("Expected real column 'real_col'"),
            }

            // Verify char_col
            match columns.get("char_col") {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0], "foo");
                    assert_eq!(vec[3], "qux");
                }
                _ => panic!("Expected character column 'char_col'"),
            }

            // Verify logical_col
            match columns.get("logical_col") {
                Some(RObject::Logical(vec)) => {
                    assert_eq!(vec.len(), 4);
                    assert_eq!(vec[0], rds2rust::Logical::True);
                    assert_eq!(vec[1], rds2rust::Logical::False);
                }
                _ => panic!("Expected logical column 'logical_col'"),
            }
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}

#[test]
fn test_dataframe_rownames() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("dataframe_rownames.rds");
    let obj = read_rds(&data).expect("Failed to parse data frame with row names");

    match obj {
        RObject::DataFrame { columns, row_names } => {
            // Check columns
            assert_eq!(columns.len(), 2);

            // Check row names
            assert_eq!(row_names.len(), 3);
            assert_eq!(row_names[0], "person1");
            assert_eq!(row_names[1], "person2");
            assert_eq!(row_names[2], "person3");

            // Verify data
            match columns.get("name") {
                Some(RObject::Character(vec)) => {
                    assert_eq!(vec, &vec!["Alice", "Bob", "Charlie"]);
                }
                _ => panic!("Expected character column 'name'"),
            }

            match columns.get("age") {
                Some(RObject::Integer(vec)) => {
                    assert_eq!(vec, &vec![25, 30, 35]);
                }
                _ => panic!("Expected integer column 'age'"),
            }
        }
        _ => panic!("Expected DataFrame, got {:?}", obj),
    }
}
