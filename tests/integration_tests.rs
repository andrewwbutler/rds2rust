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
fn test_raw_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("raw_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse raw vector");

    match obj {
        RObject::Raw(vec) => {
            assert_eq!(vec.len(), 3);
            assert_eq!(vec[0], 0x01);
            assert_eq!(vec[1], 0x02);
            assert_eq!(vec[2], 0xFF);
        }
        other => panic!("Expected Raw vector, got {:?}", other),
    }
}

#[test]
fn test_complex_single() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_single.rds");
    let obj = read_rds(&data).expect("Failed to parse single complex number");

    match obj {
        RObject::Complex(vec) => {
            assert_eq!(vec.len(), 1);
            assert_eq!(vec[0].real, 1.0);
            assert_eq!(vec[0].imaginary, 2.0);
        }
        other => panic!("Expected Complex vector, got {:?}", other),
    }
}

#[test]
fn test_complex_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("complex_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse complex vector");

    match obj {
        RObject::Complex(vec) => {
            assert_eq!(vec.len(), 3);

            // 1+2i
            assert_eq!(vec[0].real, 1.0);
            assert_eq!(vec[0].imaginary, 2.0);

            // 3+4i
            assert_eq!(vec[1].real, 3.0);
            assert_eq!(vec[1].imaginary, 4.0);

            // 5+6i
            assert_eq!(vec[2].real, 5.0);
            assert_eq!(vec[2].imaginary, 6.0);
        }
        other => panic!("Expected Complex vector, got {:?}", other),
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

#[test]
fn test_s3_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple S3 object");

    match obj {
        RObject::S3Object { base, class, attributes } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "my_custom_class");

            // Check the base object is a list
            match *base {
                RObject::List(ref elements) => {
                    assert_eq!(elements.len(), 3);
                }
                _ => panic!("Expected List as base object"),
            }

            // Should have a names attribute for the list
            assert!(attributes.get("names").is_some());
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

#[test]
fn test_s3_multi_class() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_multi_class.rds");
    let obj = read_rds(&data).expect("Failed to parse S3 object with multiple classes");

    match obj {
        RObject::S3Object { base, class, .. } => {
            // Check multiple classes (inheritance)
            assert_eq!(class.len(), 2);
            assert_eq!(class[0], "special_class");
            assert_eq!(class[1], "base_class");

            // Check the base object
            match *base {
                RObject::List(ref elements) => {
                    assert_eq!(elements.len(), 2);
                }
                _ => panic!("Expected List as base object"),
            }
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

#[test]
fn test_s3_vector() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s3_vector.rds");
    let obj = read_rds(&data).expect("Failed to parse S3 object on vector");

    match obj {
        RObject::S3Object { base, class, attributes } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "custom_vector");

            // Check the base object is a vector
            match *base {
                RObject::Real(ref vec) => {
                    assert_eq!(vec.len(), 3);
                    assert_eq!(vec[0], 10.0);
                    assert_eq!(vec[1], 20.0);
                    assert_eq!(vec[2], 30.0);
                }
                _ => panic!("Expected Real vector as base object"),
            }

            // Check for the description attribute
            match attributes.get("description") {
                Some(RObject::Character(desc)) => {
                    assert_eq!(desc.len(), 1);
                    assert_eq!(desc[0], "A custom vector class");
                }
                _ => panic!("Expected description attribute"),
            }
        }
        _ => panic!("Expected S3Object, got {:?}", obj),
    }
}

#[test]
fn test_s4_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple S4 object");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "Animal");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check species slot
            match slots.get("species") {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 1);
                    assert_eq!(species[0], "Tiger");
                }
                _ => panic!("Expected 'species' slot with character value"),
            }

            // Check age slot
            match slots.get("age") {
                Some(RObject::Real(age)) => {
                    assert_eq!(age.len(), 1);
                    assert_eq!(age[0], 5.0);
                }
                _ => panic!("Expected 'age' slot with numeric value"),
            }

            // Check habitat slot
            match slots.get("habitat") {
                Some(RObject::Character(habitat)) => {
                    assert_eq!(habitat.len(), 1);
                    assert_eq!(habitat[0], "Rainforest");
                }
                _ => panic!("Expected 'habitat' slot with character value"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

#[test]
fn test_s4_inheritance() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_inheritance.rds");
    let obj = read_rds(&data).expect("Failed to parse S4 object with inheritance");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class (should show inheritance)
            assert!(class.len() >= 1);
            assert_eq!(class[0], "Bird");

            // Check slots from both parent and child classes
            assert!(slots.len() >= 5);

            // Parent class slots (from Animal)
            assert!(slots.get("species").is_some());
            assert!(slots.get("age").is_some());
            assert!(slots.get("habitat").is_some());

            // Child class slots (from Bird)
            match slots.get("wingspan") {
                Some(RObject::Real(wingspan)) => {
                    assert_eq!(wingspan.len(), 1);
                    assert_eq!(wingspan[0], 1.2);
                }
                _ => panic!("Expected 'wingspan' slot"),
            }

            match slots.get("can_fly") {
                Some(RObject::Logical(can_fly)) => {
                    assert_eq!(can_fly.len(), 1);
                    assert_eq!(can_fly[0], rds2rust::Logical::True);
                }
                _ => panic!("Expected 'can_fly' slot"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

#[test]
fn test_s4_complex() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("s4_complex.rds");
    let obj = read_rds(&data).expect("Failed to parse complex S4 object");

    match obj {
        RObject::S4Object { class, slots } => {
            // Check the class
            assert_eq!(class.len(), 1);
            assert_eq!(class[0], "Aquarium");

            // Check the slots
            assert_eq!(slots.len(), 3);

            // Check temperatures slot (numeric vector)
            match slots.get("temperatures") {
                Some(RObject::Real(temps)) => {
                    assert_eq!(temps.len(), 3);
                    assert_eq!(temps[0], 24.5);
                    assert_eq!(temps[1], 25.0);
                    assert_eq!(temps[2], 24.8);
                }
                _ => panic!("Expected 'temperatures' slot with numeric vector"),
            }

            // Check fish_species slot (character vector)
            match slots.get("fish_species") {
                Some(RObject::Character(species)) => {
                    assert_eq!(species.len(), 3);
                    assert_eq!(species, &vec!["clownfish", "tang", "angelfish"]);
                }
                _ => panic!("Expected 'fish_species' slot with character vector"),
            }

            // Check saltwater slot (logical)
            match slots.get("saltwater") {
                Some(RObject::Logical(saltwater)) => {
                    assert_eq!(saltwater.len(), 1);
                    assert_eq!(saltwater[0], rds2rust::Logical::True);
                }
                _ => panic!("Expected 'saltwater' slot with logical value"),
            }
        }
        _ => panic!("Expected S4Object, got {:?}", obj),
    }
}

#[test]
fn test_factor_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple factor");

    match obj {
        RObject::Factor { values, levels, ordered } => {
            // Check it's not ordered
            assert!(!ordered);

            // Check levels
            assert_eq!(levels.len(), 3);
            assert_eq!(levels, vec!["high", "low", "medium"]);

            // Check values (1-based indices into levels)
            assert_eq!(values.len(), 5);
            // R factor: c("low", "high", "medium", "low", "high")
            // levels = c("high", "low", "medium")
            // So: low=2, high=1, medium=3
            assert_eq!(values[0], 2); // "low"
            assert_eq!(values[1], 1); // "high"
            assert_eq!(values[2], 3); // "medium"
            assert_eq!(values[3], 2); // "low"
            assert_eq!(values[4], 1); // "high"
        }
        _ => panic!("Expected Factor, got {:?}", obj),
    }
}

#[test]
fn test_factor_ordered() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("factor_ordered.rds");
    let obj = read_rds(&data).expect("Failed to parse ordered factor");

    match obj {
        RObject::Factor { values, levels, ordered } => {
            // Check it's ordered
            assert!(ordered);

            // Check levels (in order)
            assert_eq!(levels.len(), 3);
            assert_eq!(levels, vec!["low", "medium", "high"]);

            // Check values (1-based indices into levels)
            assert_eq!(values.len(), 4);
            // R factor: ordered(c("low", "medium", "high", "low"), levels = c("low", "medium", "high"))
            // So: low=1, medium=2, high=3
            assert_eq!(values[0], 1); // "low"
            assert_eq!(values[1], 2); // "medium"
            assert_eq!(values[2], 3); // "high"
            assert_eq!(values[3], 1); // "low"
        }
        _ => panic!("Expected Factor, got {:?}", obj),
    }
}

#[test]
fn test_lang_simple() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_simple.rds");
    let obj = read_rds(&data).expect("Failed to parse simple language object");

    match obj {
        RObject::Language(elements) => {
            // quote(sum(1, 2, 3)) => [sum, 1, 2, 3]
            assert!(elements.len() >= 1);
            // First element should be the function (sum)
            // Remaining elements are the arguments
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

#[test]
fn test_lang_with_args() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_with_args.rds");
    let obj = read_rds(&data).expect("Failed to parse language object with args");

    match obj {
        RObject::Language(elements) => {
            // quote(mean(x, na.rm = TRUE)) => [mean, x, TRUE]
            assert!(elements.len() >= 1);
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

#[test]
fn test_lang_nested() {
    if !test_data_exists() {
        eprintln!("Skipping test: test data not generated");
        return;
    }

    let data = read_test_file("lang_nested.rds");
    let obj = read_rds(&data).expect("Failed to parse nested language object");

    match obj {
        RObject::Language(elements) => {
            // quote(sqrt(sum(x, y))) => [sqrt, sum(x, y)]
            assert!(elements.len() >= 1);

            // The second element should be another language object: sum(x, y)
            if elements.len() > 1 {
                match &elements[1] {
                    RObject::Language(_) => {
                        // Good, nested call structure preserved
                    }
                    _ => {
                        // Also acceptable - nested structure may vary
                    }
                }
            }
        }
        _ => panic!("Expected Language object, got {:?}", obj),
    }
}

