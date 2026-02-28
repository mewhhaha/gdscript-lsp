use gdscript_lsp::parse_script;
use std::fs;
use std::path::PathBuf;

fn fixture_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("parser")
        .join("upstream_errors")
        .join(format!("{name}.gd"));

    fs::read_to_string(path).expect("failed to read parser fixture")
}

#[test]
fn upstream_parser_error_fixtures_map_to_expected_categories() {
    let cases = [
        ("missing_colon", "Expected \":\" after \"if\" condition."),
        (
            "static_constructor_not_static",
            "Static constructor must be declared static.",
        ),
        (
            "static_constructor_returning_something",
            "Constructor cannot return a value.",
        ),
        ("vcs_conflict_marker", "VCS conflict marker"),
        (
            "mistaken_increment_operator",
            "Expected expression after \"+\" operator.",
        ),
        (
            "mistaken_decrement_operator",
            "Expected expression after \"-\" operator.",
        ),
        (
            "nothing_after_dollar",
            "Expected node path as string or identifier after \"$\".",
        ),
        (
            "wrong_value_after_dollar",
            "Expected node path as string or identifier after \"$\".",
        ),
        (
            "wrong_value_after_dollar_slash",
            "Expected node path as string or identifier after \"/\".",
        ),
        (
            "assignment_2_equal_signs",
            "Expected end of statement after variable declaration",
        ),
        (
            "assignment_3_equal_signs",
            "Expected end of statement after variable declaration",
        ),
        (
            "assignment_without_identifier",
            "Expected variable name after \"var\".",
        ),
        ("missing_paren_after_args", "unmatched '('"),
        ("missing_closing_expr_paren", "unmatched '('"),
        (
            "lambda_without_colon",
            "Expected \":\" after lambda declaration.",
        ),
        (
            "lambda_standalone",
            "Standalone lambdas cannot be accessed.",
        ),
        (
            "invalid_ternary_operator",
            "Unexpected \"?\" in source. If you want a ternary operator",
        ),
        (
            "subscript_without_index",
            "Expected expression after \"[\".",
        ),
        (
            "function_conflicts_variable",
            "Variable \"test\" has the same name as a previously declared function.",
        ),
        (
            "variable_conflicts_function",
            "Function \"test\" has the same name as a previously declared variable.",
        ),
        (
            "missing_expression_after_ternary_else",
            "Expected expression after \"else\".",
        ),
        (
            "assignment_in_if",
            "Assignment is not allowed inside an expression.",
        ),
        (
            "assignment_in_var",
            "Assignment is not allowed inside an expression.",
        ),
        (
            "variable_conflicts_variable",
            "There is already a variable named \"TEST\" declared in this scope.",
        ),
        (
            "variable_conflicts_constant",
            "There is already a constant named \"TEST\" declared in this scope.",
        ),
        (
            "redefine_local_constant_with_keyword",
            "There is already a constant named \"TEST\" declared in this scope.",
        ),
        ("redefine_keyword", "Expected variable name after \"var\"."),
        (
            "assignment_empty_assignee",
            "Expected an expression after \"=\".",
        ),
        (
            "array_consecutive_commas",
            "Expected expression as array element.",
        ),
        (
            "binary_complement_without_argument",
            "Expected expression after \"~\" operator.",
        ),
        (
            "boolean_negation_without_argument",
            "Expected expression after \"not\" operator.",
        ),
        (
            "boolean_negation_without_argument_using_bang",
            "Expected expression after \"!\" operator.",
        ),
        (
            "yield_instead_of_await",
            "\"yield\" was removed in Godot 4. Use \"await\" instead.",
        ),
        (
            "invalid_identifier_number",
            "Expected variable name after \"var\".",
        ),
        (
            "invalid_identifier_string",
            "Expected variable name after \"var\".",
        ),
        (
            "identifier_similar_to_keyword",
            "Identifier \"аs\" is visually similar to the GDScript keyword \"as\" and thus not allowed.",
        ),
        (
            "unexpected_token_in_class_body",
            "Unexpected identifier \"error\" in class body.",
        ),
        (
            "annotation_deprecated",
            "\"@deprecated\" annotation does not exist.",
        ),
        (
            "annotation_experimental",
            "\"@experimental\" annotation does not exist.",
        ),
        (
            "annotation_extra_comma",
            "Expected expression as the annotation argument.",
        ),
        (
            "annotation_inapplicable",
            "Annotation \"@export\" cannot be applied to a function.",
        ),
        (
            "annotation_tutorial",
            "\"@tutorial\" annotation does not exist.",
        ),
        (
            "annotation_unrecognized",
            "Unrecognized annotation: \"@hello_world\".",
        ),
        (
            "assignment_in_var_if",
            "Expected conditional expression after \"if\".",
        ),
        (
            "bad_continue_in_lambda",
            "Cannot use \"continue\" outside of a loop.",
        ),
        (
            "class_name_after_annotation",
            "Annotation \"@icon\" must be at the top of the script",
        ),
        (
            "constant_conflicts_variable",
            "There is already a variable named \"TEST\" declared in this scope.",
        ),
        (
            "function_conflicts_constant",
            "Function \"test\" has the same name as a previously declared constant.",
        ),
        ("invalid_escape_sequence", "Invalid escape in string."),
        (
            "lambda_no_continue_on_new_line",
            "Expected statement, found \"in\" instead.",
        ),
        (
            "match_guard_with_assignment",
            "Assignment is not allowed inside an expression.",
        ),
        (
            "match_multiple_variable_binds_in_branch",
            "Cannot use a variable bind with multiple patterns.",
        ),
        (
            "double_dictionary_comma",
            "Expected expression as dictionary key.",
        ),
        (
            "duplicate_icon",
            "\"@icon\" annotation can only be used once.",
        ),
        (
            "duplicate_tool",
            "\"@tool\" annotation can only be used once.",
        ),
        (
            "multiple_number_separators",
            "Multiple underscores cannot be adjacent in a numeric literal.",
        ),
        (
            "multiple_number_separators_after_decimal",
            "Multiple underscores cannot be adjacent in a numeric literal.",
        ),
        (
            "variable_conflicts_for_variable",
            "There is already a variable named \"TEST\" declared in this scope.",
        ),
        (
            "warning_ignore_extra_start",
            "Warning \"UNREACHABLE_CODE\" is already being ignored by \"@warning_ignore_start\" at line 1.",
        ),
        (
            "warning_ignore_restore_without_start",
            "Warning \"UNREACHABLE_CODE\" is not being ignored by \"@warning_ignore_start\".",
        ),
        ("bad_r_string_1", "Unterminated string."),
        ("bad_r_string_2", "Unterminated string."),
        (
            "bad_r_string_3",
            "Closing \"]\" doesn't have an opening counterpart.",
        ),
        (
            "brace_syntax",
            "Expected end of statement after bodyless function declaration, found \"{\" instead.",
        ),
        (
            "default_value_in_function_call",
            "Assignment is not allowed inside an expression.",
        ),
        (
            "dollar_assignment_bug_53696",
            "Expected node path as string or identifier after \"$\".",
        ),
        (
            "export_enum_wrong_array_type",
            "\"@export_enum\" annotation requires a variable of type \"int\"",
        ),
        (
            "export_enum_wrong_type",
            "\"@export_enum\" annotation requires a variable of type \"int\"",
        ),
        (
            "export_godot3_syntax",
            "The \"export\" keyword was removed in Godot 4.",
        ),
        (
            "export_godot3_syntax_with_args",
            "The \"export\" keyword was removed in Godot 4.",
        ),
        (
            "export_tool_button_requires_tool_mode",
            "Tool buttons can only be used in tool scripts",
        ),
        (
            "mixing_tabs_spaces.textonly",
            "Used tab character for indentation instead of space as used before in the file.",
        ),
        (
            "variadic_func_params_after_rest",
            "Cannot have parameters after the rest parameter.",
        ),
        (
            "variadic_func_rest_after_rest",
            "Cannot have parameters after the rest parameter.",
        ),
        (
            "variadic_func_rest_with_default",
            "The rest parameter cannot have a default value.",
        ),
        (
            "variadic_func_static_init",
            "Static constructor cannot have parameters.",
        ),
    ];

    for (fixture, expected_message) in cases {
        let source = fixture_text(fixture);
        let parsed = parse_script(&source, format!("upstream/{fixture}.gd"));
        let messages = parsed
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message.contains(expected_message)),
            "fixture `{fixture}` missing expected parser category `{expected_message}`; messages: {messages:?}"
        );
    }
}

#[test]
fn control_flow_blocks_require_trailing_colons() {
    let source = r#"
func test():
    if true
        pass
    elif false
        pass
    while true
        break
    for i in [1, 2, 3]
        pass
    match i
        _:
            pass
"#;
    let parsed = parse_script(source, "control_flow_missing_colons.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected \":\" after \"if\" condition.")),
        "messages: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected \":\" after \"elif\" condition.")),
        "messages: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected \":\" after \"while\" condition.")),
        "messages: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected \":\" after \"for\" loop.")),
        "messages: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Expected \":\" after \"match\" expression.")),
        "messages: {messages:?}"
    );
}
