use gdscript_lsp::{
    BehaviorMode, LintSettings, check_document_with_mode, check_document_with_settings,
    code_actions_for_diagnostics, code_actions_for_diagnostics_and_mode, hover_at,
    parse_project_godot_config, parse_script,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn lint_settings_load_from_project_godot() {
    let cfg = parse_project_godot_config(
        r#"
[gdscript]
lint/max_line_length=88
lint/allow_tabs=true
lint/require_spaces_around_operators=false
"#,
    );

    let settings = LintSettings::from_project_config(Some(&cfg));
    assert_eq!(settings.max_line_length, 88);
    assert!(settings.allow_tabs);
    assert!(!settings.require_spaces_around_operators);
}

#[test]
fn lint_respects_project_based_settings() {
    let cfg = parse_project_godot_config(
        r#"
[gdscript]
lint/allow_tabs=true
lint/require_spaces_around_operators=false
"#,
    );

    let source = "a=1\n\tprint(\"ok\")\n";
    let settings = LintSettings::from_project_config(Some(&cfg));
    let diagnostics = check_document_with_settings(source, &settings);

    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != "no-tabs" && d.code != "spaces-around-operator"),
        "diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn code_actions_offer_fixes_for_spacing_and_trailing_whitespace() {
    let source = "a=1 \n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Insert spaces around operator")),
        "actions: {actions:#?}"
    );
    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Trim trailing whitespace")),
        "actions: {actions:#?}"
    );
}

#[test]
fn hover_returns_builtin_documentation() {
    let source = "func _ready():\n    print(\"hello\")\n";
    let parsed = parse_script(source, "hover_test.gd");
    let hover = hover_at(2, 7, &parsed).expect("hover response");

    assert!(hover.title.contains("print"), "hover: {hover:#?}");
    assert!(!hover.body.trim().is_empty(), "hover: {hover:#?}");
}

#[test]
fn hover_uses_snapshot_metadata_for_builtin_signatures() {
    let source = "func _ready():\n    cos(1.0)\n";
    let parsed = parse_script(source, "hover_cos_test.gd");
    let hover = hover_at(2, 6, &parsed).expect("hover response");

    assert!(hover.title.contains("cos("), "hover: {hover:#?}");
    assert!(hover.body.to_ascii_lowercase().contains("cosine"));
}

#[test]
fn enhanced_mode_adds_todo_diagnostic() {
    let source = "# TODO: remove this\n";
    let parity = check_document_with_mode(source, BehaviorMode::Parity);
    let enhanced = check_document_with_mode(source, BehaviorMode::Enhanced);

    assert!(
        !parity.iter().any(|diag| diag.code == "todo-comment"),
        "parity diagnostics: {parity:#?}"
    );
    assert!(
        enhanced.iter().any(|diag| diag.code == "todo-comment"),
        "enhanced diagnostics: {enhanced:#?}"
    );
}

#[test]
fn enhanced_mode_exposes_todo_code_action() {
    let source = "# TODO: remove this\n";
    let diagnostics = check_document_with_mode(source, BehaviorMode::Enhanced);

    let parity_actions =
        code_actions_for_diagnostics_and_mode(source, &diagnostics, BehaviorMode::Parity);
    let enhanced_actions =
        code_actions_for_diagnostics_and_mode(source, &diagnostics, BehaviorMode::Enhanced);

    assert!(
        !parity_actions
            .iter()
            .any(|action| action.title.contains("Remove TODO comment")),
        "parity actions: {parity_actions:#?}"
    );
    assert!(
        enhanced_actions
            .iter()
            .any(|action| action.title.contains("Remove TODO comment")),
        "enhanced actions: {enhanced_actions:#?}"
    );
}

fn fixture_text(suite: &str, name: &str, file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(suite)
        .join(name)
        .join(file);

    fs::read_to_string(path).unwrap()
}

#[test]
fn parser_reports_unmatched_delimiters_in_checker() {
    let source = fixture_text("check", "unmatched-delimiters", "input.gd");
    let parsed = parse_script(
        &source,
        "tests/fixtures/check/unmatched-delimiters/input.gd",
    );

    assert!(
        parsed
            .issues
            .iter()
            .any(|issue| issue.message.contains("unmatched")),
        "expected unmatched delimiter diagnostic, got {parsed:#?}"
    );
}

#[test]
fn parser_avoids_false_positives_for_common_node_patterns() {
    let source = r#"
extends CharacterBody3D

const FALL_STATE: StringName = &"fall"

@export_range(0.1, 10.0, 0.1) var gravity_scale: float = 1.0
@onready var animation_tree: AnimationTree = $AnimationTree
var playback: AnimationNodeStateMachinePlayback
"#;
    let parsed = parse_script(source, "false_positive_node_patterns.gd");
    let messages = parsed
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(
        !messages.iter().any(|message| message.contains(
            "Assigned value for constant \"FALL_STATE\" isn't a constant expression."
        )),
        "unexpected constant-expression diagnostic: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("Unrecognized annotation: \"@export_range\"")),
        "unexpected export_range diagnostic: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains(
            "\"@onready\" can only be used in classes that inherit \"Node\"."
        )),
        "unexpected @onready inheritance diagnostic: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains(
            "Cannot use shorthand \"get_node()\" notation (\"$\") on a class that isn't a node."
        )),
        "unexpected get_node shorthand diagnostic: {messages:?}"
    );
    assert!(
        !messages.iter().any(|message| message.contains(
            "Could not find type \"AnimationNodeStateMachinePlayback\" in the current scope."
        )),
        "unexpected unknown type diagnostic: {messages:?}"
    );
}

#[test]
fn code_actions_offer_prefix_for_unused_parameter() {
    let source = "func function_with_unused_argument(p_arg1, p_arg2):\n\tprint(p_arg1)\n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Prefix unused parameter")),
        "actions: {actions:#?}"
    );
}

#[test]
fn code_actions_offer_prefix_for_unused_variable() {
    let source = "func test():\n\tvar unused = \"not used\"\n\tpass\n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Prefix unused variable")),
        "actions: {actions:#?}"
    );
}

#[test]
fn code_actions_offer_prefix_for_unused_local_constant() {
    let source = "func test():\n\tconst UNUSED = \"not used\"\n\tpass\n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Prefix unused constant")),
        "actions: {actions:#?}"
    );
}

#[test]
fn code_actions_offer_static_receiver_rewrite() {
    let source =
        "func test():\n\tvar some_string := String()\n\tsome_string.num_uint64(8589934592)\n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Call static method on type")),
        "actions: {actions:#?}"
    );
}

#[test]
fn code_actions_offer_integer_division_cast() {
    let source = "func test():\n\tvar __ = 5 / 2\n";
    let diagnostics = check_document_with_settings(source, &LintSettings::default());
    let actions = code_actions_for_diagnostics(source, &diagnostics);

    assert!(
        actions
            .iter()
            .any(|action| action.title.contains("Convert left operand to float")),
        "actions: {actions:#?}"
    );
}
