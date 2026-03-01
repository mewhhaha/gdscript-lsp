use gdscript_lsp::parse_script;

#[test]
fn typed_array_argument_mismatch_does_not_emit_dictionary_message() {
    let source = r#"
func expect_typed(typed: Array[int]):
    pass

func test():
    var differently: Array[float] = [1.0]
    expect_typed(differently)
"#;

    let parsed = parse_script(source, "regression_array_call.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages.iter().any(|message| {
            message.contains("argument 1 should be \"Array[int]\" but is \"Array[float]\"")
        }),
        "expected array mismatch diagnostic, got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Dictionary[int, int]")),
        "unexpected dictionary mismatch diagnostic in array case: {messages:?}"
    );
}

#[test]
fn typed_dictionary_argument_mismatch_does_not_emit_array_message() {
    let source = r#"
func expect_typed(typed: Dictionary[int, int]):
    pass

func test():
    var differently: Dictionary[float, float] = { 1.0: 0.0 }
    expect_typed(differently)
"#;

    let parsed = parse_script(source, "regression_dict_call.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages.iter().any(|message| {
            message.contains(
                "argument 1 should be \"Dictionary[int, int]\" but is \"Dictionary[float, float]\"",
            )
        }),
        "expected dictionary mismatch diagnostic, got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Array[int]")),
        "unexpected array mismatch diagnostic in dictionary case: {messages:?}"
    );
}

#[test]
fn node_param_function_without_override_does_not_emit_parent_signature_mismatch() {
    let source = r#"
class A:
    func f(_p: Node):
        pass

func test():
    pass
"#;

    let parsed = parse_script(source, "regression_no_override.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages
            .iter()
            .all(|message| !message.contains("The function signature doesn't match the parent")),
        "unexpected parent-signature mismatch diagnostics: {messages:?}"
    );
}

#[test]
fn dictionary_literal_lua_syntax_does_not_emit_assignment_in_expression_noise() {
    let source = r#"
func test():
    var dict = { a = 1 }
    print(dict)
"#;

    let parsed = parse_script(source, "regression_dict_literal.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Assignment is not allowed inside an expression")),
        "unexpected assignment-in-expression diagnostic noise: {messages:?}"
    );
}

#[test]
fn super_call_with_defined_parent_method_does_not_emit_missing_base_error() {
    let source = r#"
class A:
    func say():
        pass

class B extends A:
    func say():
        super()
"#;

    let parsed = parse_script(source, "regression_super_call.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Function \"say()\" not found in base")),
        "unexpected missing-base method diagnostic: {messages:?}"
    );
}

#[test]
fn simple_variable_alias_does_not_emit_cyclic_reference_for_f() {
    let source = r#"
var f = 1
var x = f
"#;

    let parsed = parse_script(source, "regression_alias.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Could not resolve member \"f\": Cyclic reference")),
        "unexpected cyclic-reference diagnostic for plain alias: {messages:?}"
    );
}
