use gdscript_lsp::{LintSettings, check_document_with_settings, parse_project_godot_config};
use std::fs;
use std::path::PathBuf;

fn fixture_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lint")
        .join("upstream_warnings")
        .join(format!("{name}.gd"));
    fs::read_to_string(path).expect("failed to read warning fixture")
}

#[test]
fn empty_file_fixture_emits_empty_file_warning() {
    let source = fixture_text("empty_file");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());

    assert!(
        diagnostics.iter().any(|diag| diag.code == "empty-file"),
        "expected empty-file warning, got {diagnostics:#?}"
    );
}

#[test]
fn standalone_ternary_fixture_emits_warning() {
    let source = fixture_text("standalone_ternary");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());

    let standalone_ternary_count = diagnostics
        .iter()
        .filter(|diag| diag.code == "standalone-ternary")
        .count();
    assert!(
        standalone_ternary_count >= 2,
        "expected standalone-ternary warnings, got {diagnostics:#?}"
    );
}

#[test]
fn return_value_discarded_fixture_can_be_enabled_via_project_settings() {
    let source = fixture_text("return_value_discarded");
    let config = parse_project_godot_config(
        r#"
[gdscript]
lint/severity/return_value_discarded=warning
"#,
    );
    let settings = LintSettings::from_project_config(Some(&config));
    let diagnostics = check_document_with_settings(&source, &settings);

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "return-value-discarded"),
        "expected return-value-discarded warning, got {diagnostics:#?}"
    );
}

#[test]
fn integer_division_fixture_emits_warning() {
    let source = fixture_text("integer_division");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "integer-division"),
        "expected integer-division warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_parameter_fixture_emits_warning_for_non_underscored_parameter() {
    let source = fixture_text("unused_parameter");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-parameter"),
        "expected unused-parameter warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_variable_fixture_emits_warning() {
    let source = fixture_text("unused_variable");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-variable"),
        "expected unused-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_local_constant_fixture_emits_warning() {
    let source = fixture_text("unused_local_constant");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-local-constant"),
        "expected unused-local-constant warning, got {diagnostics:#?}"
    );
}

#[test]
fn shadowed_global_identifier_fixture_emits_warning() {
    let source = fixture_text("shadowed_global_identifier");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "shadowed-global-identifier"),
        "expected shadowed-global-identifier warning, got {diagnostics:#?}"
    );
}

#[test]
fn unreachable_code_fixture_emits_warning() {
    let source = fixture_text("unreachable_code");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unreachable-code"),
        "expected unreachable-code warning, got {diagnostics:#?}"
    );
}

#[test]
fn unassigned_variable_fixture_emits_warning() {
    let source = fixture_text("unassigned_variable");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unassigned-variable"),
        "expected unassigned-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn unassigned_variable_op_assign_fixture_emits_warning() {
    let source = fixture_text("unassigned_variable_op_assign");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unassigned-variable-op-assign"),
        "expected unassigned-variable-op-assign warning, got {diagnostics:#?}"
    );
}

#[test]
fn static_called_on_instance_fixture_emits_warning() {
    let source = fixture_text("static_called_on_instance");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "static-called-on-instance"),
        "expected static-called-on-instance warning, got {diagnostics:#?}"
    );
}

#[test]
fn standalone_expression_fixture_emits_warning() {
    let source = fixture_text("standalone_expression");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "standalone-expression"),
        "expected standalone-expression warning, got {diagnostics:#?}"
    );
}

#[test]
fn assert_always_true_fixture_emits_warning() {
    let source = fixture_text("assert_always_true");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "assert-always-true"),
        "expected assert-always-true warning, got {diagnostics:#?}"
    );
}

#[test]
fn shadowed_variable_function_fixture_emits_warning() {
    let source = fixture_text("shadowed_variable_function");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "shadowed-variable"),
        "expected shadowed-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn shadowed_variable_class_fixture_emits_warning() {
    let source = fixture_text("shadowed_variable_class");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "shadowed-variable"),
        "expected shadowed-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn shadowed_constant_fixture_emits_warning() {
    let source = fixture_text("shadowed_constant");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "shadowed-variable"),
        "expected shadowed-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn match_default_not_at_end_fixture_emits_warning() {
    let source = fixture_text("match_default_not_at_end");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unreachable-pattern"),
        "expected unreachable-pattern warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_identifier_fixture_emits_warning() {
    let source = fixture_text("confusable_identifier");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-identifier"),
        "expected confusable-identifier warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_capture_reassignment_fixture_emits_warning() {
    let source = fixture_text("confusable_capture_reassignment");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-capture-reassignment"),
        "expected confusable-capture-reassignment warning, got {diagnostics:#?}"
    );
}

#[test]
fn onready_with_export_fixture_emits_warning() {
    let source = fixture_text("onready_with_export");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "onready-with-export"),
        "expected onready-with-export warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_private_class_variable_fixture_emits_warning() {
    let source = fixture_text("unused_private_class_variable");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-private-class-variable"),
        "expected unused-private-class-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_local_declaration_fixture_emits_warning() {
    let source = fixture_text("confusable_local_declaration");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-local-declaration"),
        "expected confusable-local-declaration warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_local_usage_fixture_emits_warning() {
    let source = fixture_text("confusable_local_usage");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-local-usage"),
        "expected confusable-local-usage warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_local_usage_initializer_fixture_emits_warning() {
    let source = fixture_text("confusable_local_usage_initializer");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-local-usage"),
        "expected confusable-local-usage warning, got {diagnostics:#?}"
    );
}

#[test]
fn confusable_local_usage_loop_fixture_emits_warning() {
    let source = fixture_text("confusable_local_usage_loop");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "confusable-local-usage"),
        "expected confusable-local-usage warning, got {diagnostics:#?}"
    );
}

#[test]
fn inference_with_variant_fixture_emits_warning() {
    let source = fixture_text("inference_with_variant");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "inference-on-variant"),
        "expected inference-on-variant warning, got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "inference-on-variant"
                && diag.level == gdscript_lsp::DiagnosticLevel::Warning),
        "expected inference-on-variant default severity warning, got {diagnostics:#?}"
    );
}

#[test]
fn enum_assign_int_without_casting_fixture_emits_warning() {
    let source = fixture_text("enum_assign_int_without_casting");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "int-as-enum-without-cast"),
        "expected int-as-enum-without-cast warning, got {diagnostics:#?}"
    );
}

#[test]
fn enum_without_default_value_fixture_emits_warning() {
    let source = fixture_text("enum_without_default_value");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "enum-variable-without-default"),
        "expected enum-variable-without-default warning, got {diagnostics:#?}"
    );
}

#[test]
fn cast_enum_bad_int_fixture_emits_warning() {
    let source = fixture_text("cast_enum_bad_int");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "int-as-enum-without-match"),
        "expected int-as-enum-without-match warning, got {diagnostics:#?}"
    );
}

#[test]
fn cast_enum_bad_enum_fixture_emits_warning() {
    let source = fixture_text("cast_enum_bad_enum");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "int-as-enum-without-match"),
        "expected int-as-enum-without-match warning, got {diagnostics:#?}"
    );
}

#[test]
fn narrowing_conversion_fixture_emits_warning() {
    let source = fixture_text("narrowing_conversion");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "narrowing-conversion"),
        "expected narrowing-conversion warning, got {diagnostics:#?}"
    );
}

#[test]
fn incompatible_ternary_fixture_emits_warning() {
    let source = fixture_text("incompatible_ternary");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "incompatible-ternary"),
        "expected incompatible-ternary warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_argument_fixture_emits_warning() {
    let source = fixture_text("unused_argument");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-parameter"),
        "expected unused-parameter warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_constant_fixture_emits_warning() {
    let source = fixture_text("unused_constant");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-local-constant"),
        "expected unused-local-constant warning, got {diagnostics:#?}"
    );
}

#[test]
fn unreachable_code_after_return_fixture_emits_warning() {
    let source = fixture_text("unreachable_code_after_return");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unreachable-code"),
        "expected unreachable-code warning, got {diagnostics:#?}"
    );
}

#[test]
fn unreachable_code_after_return_bug_55154_fixture_does_not_emit_warning() {
    let source = fixture_text("unreachable_code_after_return_bug_55154");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != "unreachable-code"),
        "expected no unreachable-code warning for bug_55154 fixture, got {diagnostics:#?}"
    );
}

#[test]
fn get_node_without_onready_fixture_emits_warning() {
    let source = fixture_text("get_node_without_onready");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "get-node-default-without-onready"),
        "expected get-node-default-without-onready warning, got {diagnostics:#?}"
    );
}

#[test]
fn lambda_shadowing_arg_fixture_emits_warning() {
    let source = fixture_text("lambda_shadowing_arg");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "shadowed-variable"),
        "expected shadowed-variable warning, got {diagnostics:#?}"
    );
}

#[test]
fn lambda_unused_arg_fixture_emits_warning() {
    let source = fixture_text("lambda_unused_arg");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unused-parameter"),
        "expected unused-parameter warning, got {diagnostics:#?}"
    );
}

#[test]
fn missing_await_fixture_emits_warning_when_enabled() {
    let source = fixture_text("missing_await");
    let config = parse_project_godot_config(
        r#"
[gdscript]
lint/severity/missing_await=warning
"#,
    );
    let settings = LintSettings::from_project_config(Some(&config));
    let diagnostics = check_document_with_settings(&source, &settings);
    assert!(
        diagnostics.iter().any(|diag| diag.code == "missing-await"),
        "expected missing-await warning, got {diagnostics:#?}"
    );
}

#[test]
fn shadowning_fixture_emits_shadow_warnings() {
    let source = fixture_text("shadowning");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == "shadowed-global-identifier" || diag.code == "shadowed-variable"
        }),
        "expected shadow warnings for shadowning fixture, got {diagnostics:#?}"
    );
}

#[test]
fn unsafe_cast_fixture_emits_warning_when_enabled() {
    let source = fixture_text("unsafe_cast");
    let config = parse_project_godot_config(
        r#"
[gdscript]
lint/severity/unsafe_cast=warning
"#,
    );
    let settings = LintSettings::from_project_config(Some(&config));
    let diagnostics = check_document_with_settings(&source, &settings);
    assert!(
        diagnostics.iter().any(|diag| diag.code == "unsafe-cast"),
        "expected unsafe-cast warning, got {diagnostics:#?}"
    );
}

#[test]
fn redundant_await_fixture_emits_warning() {
    let source = fixture_text("redundant_await");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "redundant-await"),
        "expected redundant-await warning, got {diagnostics:#?}"
    );
}

#[test]
fn deprecated_operators_fixture_has_no_deprecated_keyword_warning() {
    let source = fixture_text("deprecated_operators");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != "deprecated-keyword"),
        "expected no deprecated-keyword warning for fixture, got {diagnostics:#?}"
    );
}

#[test]
fn unsafe_call_argument_fixture_emits_warning_when_enabled() {
    let source = fixture_text("unsafe_call_argument");
    let config = parse_project_godot_config(
        r#"
[gdscript]
lint/severity/unsafe_call_argument=warning
"#,
    );
    let settings = LintSettings::from_project_config(Some(&config));
    let diagnostics = check_document_with_settings(&source, &settings);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "unsafe-call-argument"),
        "expected unsafe-call-argument warning, got {diagnostics:#?}"
    );
}

#[test]
fn unused_signal_fixture_emits_warning() {
    let source = fixture_text("unused_signal");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics.iter().any(|diag| diag.code == "unused-signal"),
        "expected unused-signal warning, got {diagnostics:#?}"
    );
}

#[test]
fn non_tool_extends_tool_fixture_emits_warning() {
    let source = fixture_text("non_tool_extends_tool");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics.iter().any(|diag| diag.code == "missing-tool"),
        "expected missing-tool warning, got {diagnostics:#?}"
    );
}

#[test]
fn non_tool_extends_tool_ignored_fixture_has_no_missing_tool_warning() {
    let source = fixture_text("non_tool_extends_tool_ignored");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics.iter().all(|diag| diag.code != "missing-tool"),
        "expected missing-tool warning to be ignored, got {diagnostics:#?}"
    );
}

#[test]
fn overriding_native_method_fixture_emits_warning() {
    let source = fixture_text("overriding_native_method");
    let diagnostics = check_document_with_settings(&source, &LintSettings::default());
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == "native-method-override"),
        "expected native-method-override warning, got {diagnostics:#?}"
    );
}
