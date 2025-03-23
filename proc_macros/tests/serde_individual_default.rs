use proc_macros::serde_individual_default;

use derive_getters::Getters;
use serde::Serialize;

#[derive(Serialize, Getters)]
#[serde_individual_default]
struct Example {
    #[getter(skip)]
    test_1: i64,
    test_2: i64,
    test_3: String,
}

impl Default for Example {
    fn default() -> Self {
        Example {
            test_1: 3942,
            test_2: 42390,
            test_3: "a".to_string(),
        }
    }
}

#[serde_individual_default]
struct ExampleWithoutSerialize {
    test_1: i64,
    test_2: i64,
}

impl Default for ExampleWithoutSerialize {
    fn default() -> Self {
        ExampleWithoutSerialize {
            test_1: 765,
            test_2: 126,
        }
    }
}

#[serde_individual_default]
pub struct ExamplePublic {
    test_1: i64,
    test_2: i64,
}

impl Default for ExamplePublic {
    fn default() -> Self {
        ExamplePublic {
            test_1: 598,
            test_2: 403,
        }
    }
}

#[test]
fn should_have_default_serialization() {
    // Case 1: test_3 missing

    // Example JSON string that doesn't contain `test_3` but has customized `test_1` and `test_2`
    let json_data_1 = serde_json::json!({
        "test_1": 500,
        "test_2": 100
    });

    let example_struct_1: Example = serde_json::from_value(json_data_1).unwrap();

    // Assert that`test_1` and `test_2` are set to the custom value
    assert_eq!(example_struct_1.test_1, 500);
    assert_eq!(example_struct_1.test_2, 100);

    // Assert that `test_3` is set to the default value (a)
    assert_eq!(example_struct_1.test_3, "a".to_string());

    // Case 2: test_2 missing

    // Example JSON string that doesn't contain `test_2` but has customized `test_1` and `test_3`
    let json_data_2 = serde_json::json!({
        "test_1": 999,
        "test_3": "test".to_string()
    });

    let example_struct_2: Example = serde_json::from_value(json_data_2).unwrap();

    // Assert that`test_1` and `test_3` are set to the custom value
    assert_eq!(example_struct_2.test_1, 999);
    assert_eq!(example_struct_2.test_3, "test".to_string());

    // Assert that `test_2` is set to the default value (42390)
    assert_eq!(example_struct_2.test_2, 42390);
}

#[test]
fn should_preserve_other_attributes() {
    // Example JSON string that doesn't contain `test_3` but has customized `test_1` and `test_2`
    let json_data = serde_json::json!({
        "test_1": 500,
        "test_2": 100,
        "test_3": "b".to_string()
    });

    let example_struct: Example = serde_json::from_value(json_data).unwrap();

    // Assert that`test_2` and `test_3` have getters
    assert_eq!(example_struct.test_1, 500);
    assert_eq!(example_struct.test_2(), 100);
    assert_eq!(example_struct.test_3(), &"b".to_string());
}

#[test]
fn test_struct_without_serialize() {
    let json_data = serde_json::json!({
        "test_2": 123,
    });

    let example_without_serialize: ExampleWithoutSerialize =
        serde_json::from_value(json_data).unwrap();

    assert_eq!(example_without_serialize.test_1, 765);
    assert_eq!(example_without_serialize.test_2, 123);
}

#[test]
fn test_public_struct() {
    let json_data = serde_json::json!({
        "test_1": 345,
    });

    let example_public: ExamplePublic = serde_json::from_value(json_data).unwrap();

    assert_eq!(example_public.test_1, 345);
    assert_eq!(example_public.test_2, 403);
}
