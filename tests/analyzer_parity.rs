use gdscript_lsp::parse_script;
use std::fs;
use std::path::PathBuf;

fn fixture_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("analyzer")
        .join("upstream_errors")
        .join(format!("{name}.gd"));

    fs::read_to_string(path).expect("failed to read analyzer fixture")
}

#[test]
fn upstream_analyzer_error_fixtures_map_to_expected_categories() {
    let cases = [
        (
            "constant_used_as_function",
            "Name \"CONSTANT\" called as a function but is a \"int\".",
        ),
        (
            "function_used_as_property",
            "Cannot assign a new value to a constant.",
        ),
        ("not_found_type", "Could not find type \"Foo\" in the current scope."),
        (
            "missing_argument",
            "Too few arguments for \"args()\" call.",
        ),
        ("extend_unknown", "Could not find nested type \"Baz\"."),
        ("extend_variable", "Cannot use variable \"A\" in extends chain."),
        (
            "call_not_existing_static_method",
            "Static function \"not_existing_method()\" not found in base \"MyClass\".",
        ),
        (
            "get_node_shorthand_in_static_function",
            "Cannot use shorthand \"get_node()\" notation (\"$\") in a static function.",
        ),
        (
            "get_node_shorthand_within_non_node",
            "Cannot use shorthand \"get_node()\" notation (\"$\") on a class that isn't a node.",
        ),
        (
            "constructor_call_type",
            "Expression is of type \"B\" so it can't be of type \"C\".",
        ),
        (
            "invalid_array_index",
            "Invalid index type \"bool\" for a base of type \"Array\".",
        ),
        (
            "invalid_concatenation_bool",
            "Invalid operands to operator +, bool and bool.",
        ),
        (
            "invalid_concatenation_dictionary",
            "Invalid operands \"Dictionary\" and \"Dictionary\" for \"+\" operator.",
        ),
        (
            "invalid_concatenation_mixed",
            "Invalid operands \"String\" and \"Array\" for \"+\" operator.",
        ),
        (
            "leading_number_separator",
            "Identifier \"_123\" not declared in the current scope.",
        ),
        (
            "annotation_non_constant_parameter",
            "Argument 1 of annotation \"@export_range\" isn't a constant expression.",
        ),
        (
            "onready_within_non_node",
            "\"@onready\" can only be used in classes that inherit \"Node\".",
        ),
        (
            "onready_within_non_node_inner_class",
            "\"@onready\" can only be used in classes that inherit \"Node\".",
        ),
        (
            "static_func_call_non_static",
            "Cannot call non-static function \"non_static_func()\" from the static function \"static_func()\".",
        ),
        (
            "static_func_access_non_static",
            "Cannot access non-static function \"non_static_func\" from the static function \"static_func()\".",
        ),
        (
            "static_var_init_non_static_access",
            "Cannot access non-static function \"non_static\" from a static variable initializer.",
        ),
        (
            "cast_int_to_array",
            "Invalid cast. Cannot convert from \"int\" to \"Array\".",
        ),
        (
            "cast_int_to_object",
            "Invalid cast. Cannot convert from \"int\" to \"Node\".",
        ),
        (
            "bitwise_float_left_operand",
            "Invalid operands to operator <<, float and int.",
        ),
        (
            "bitwise_float_right_operand",
            "Invalid operands to operator >>, int and float.",
        ),
        (
            "for_loop_on_literal_bool",
            "Unable to iterate on value of type \"bool\".",
        ),
        (
            "assign_to_read_only_property",
            "Cannot assign a new value to a read-only property.",
        ),
        (
            "assign_signal",
            "Cannot assign a new value to a constant.",
        ),
        (
            "assign_named_enum",
            "Cannot assign a new value to a constant.",
        ),
        (
            "assign_enum",
            "Cannot assign a new value to a constant.",
        ),
        ("cyclic_inheritance", "Cyclic inheritance."),
        (
            "extend_engine_singleton",
            "Cannot inherit native class \"Time\" because it is an engine singleton.",
        ),
        (
            "cast_object_to_int",
            "Invalid cast. Cannot convert from \"RefCounted\" to \"int\".",
        ),
        (
            "for_loop_on_constant_int",
            "Expression is of type \"int\" so it can't be of type \"String\".",
        ),
        (
            "for_loop_on_hard_float",
            "Expression is of type \"float\" so it can't be of type \"String\".",
        ),
        (
            "for_loop_on_hard_int",
            "Expression is of type \"int\" so it can't be of type \"String\".",
        ),
        (
            "for_loop_on_hard_string",
            "Expression is of type \"String\" so it can't be of type \"int\".",
        ),
        (
            "for_loop_on_constant_float",
            "Expression is of type \"float\" so it can't be of type \"String\".",
        ),
        (
            "for_loop_on_literal_int",
            "Expression is of type \"int\" so it can't be of type \"String\".",
        ),
        (
            "for_loop_on_enum_value",
            "Expression is of type \"int\" so it can't be of type \"String\".",
        ),
        (
            "constant_array_index_assign",
            "Cannot assign a new value to a constant.",
        ),
        (
            "constant_dictionary_index_assign",
            "Cannot assign a new value to a constant.",
        ),
        (
            "constant_subscript_type",
            "Expression is of type \"int\" so it can't be of type \"String\".",
        ),
        (
            "invalid_constant",
            "Assigned value for constant \"TEST\" isn't a constant expression.",
        ),
        (
            "inferring_with_weak_type_parameter",
            "Cannot infer the type of \"inferred\" parameter because the value doesn't have a set type.",
        ),
        ("lambda_no_return", "Not all code paths return a value."),
        (
            "engine_singleton_instantiate",
            "Cannot construct native class \"Time\" because it is an engine singleton.",
        ),
        (
            "enum_builtin_access",
            "Type \"Axis\" in base \"Vector3\" cannot be used on its own.",
        ),
        (
            "enum_global_access",
            "Type \"Operator\" in base \"Variant\" cannot be used on its own.",
        ),
        (
            "enum_native_access",
            "Type \"ProcessMode\" in base \"Node\" cannot be used on its own.",
        ),
        (
            "enum_native_bad_value",
            "Cannot find member \"THIS_DOES_NOT_EXIST\" in base \"TileSet.TileShape\".",
        ),
        (
            "gd_utility_function_wrong_arg",
            "Invalid argument for \"len()\" function",
        ),
        (
            "native_type_errors",
            "Cannot find member \"this_does_not_exist\" in base \"TileSet\".",
        ),
        (
            "object_invalid_constructor",
            "Invalid constructor \"Object()\", use \"Object.new()\" instead.",
        ),
        (
            "return_null_in_void_func",
            "A void function cannot return a value.",
        ),
        (
            "use_value_of_void_function_gd_utility",
            "Cannot get return value of call to \"print_debug()\" because it returns \"void\".",
        ),
        (
            "use_value_of_void_function_utility",
            "Cannot get return value of call to \"print()\" because it returns \"void\".",
        ),
        (
            "utility_function_wrong_arg",
            "Invalid argument for \"floor()\" function",
        ),
        (
            "class_name_shadows_builtin_type",
            "Class \"Vector2\" hides a built-in type.",
        ),
        (
            "constant_name_shadows_builtin_type",
            "The member \"Vector2\" cannot have the same name as a builtin type.",
        ),
        (
            "enum_name_shadows_builtin_type",
            "The member \"Vector2\" cannot have the same name as a builtin type.",
        ),
    ];

    for (fixture, expected_message) in cases {
        let source = fixture_text(fixture);
        let parsed = parse_script(&source, format!("upstream/analyzer/{fixture}.gd"));
        let messages = parsed
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message.contains(expected_message)),
            "fixture `{fixture}` missing expected analyzer category `{expected_message}`; messages: {messages:?}"
        );
    }
}
