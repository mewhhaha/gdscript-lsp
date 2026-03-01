mod harness;

use gdscript_lsp::format_gdscript;
use harness::run_cli_with_args;
use harness::run_fixture_case;
use serde_json::Value;
use std::{env, fs, path::PathBuf, process};

#[test]
fn lint_ok_fixture_has_no_diagnostics() {
    run_fixture_case("lint", "ok");
}

#[test]
fn lint_bad_fixture_reports_problem() {
    run_fixture_case("lint", "bad");
}

#[test]
fn lint_tabs_fixture_reports_problem() {
    run_fixture_case("lint", "tabs");
}

#[test]
fn lint_max_line_length_fixture_reports_problem() {
    run_fixture_case("lint", "max-line-length");
}

#[test]
fn lint_project_override_fixtures_are_honored() {
    run_fixture_case("lint", "project-overrides");
}

#[test]
fn lint_respects_project_configuration() {
    run_fixture_case("lint", "project-config");
}

#[test]
fn lint_project_disabled_rules_and_severity_overrides_are_honored() {
    run_fixture_case("lint", "project-disabled-rules-severity");
}

#[test]
fn check_project_configuration_in_fixture() {
    run_fixture_case("check", "project-config");
}

#[test]
fn check_project_disabled_rules_and_severity_overrides_are_honored() {
    run_fixture_case("check", "project-disabled-rules-severity");
}

#[test]
fn check_mode_enhanced_is_accepted_as_global_flag() {
    run_fixture_case("check", "mode-enhanced");
}

#[test]
fn check_mode_parity_is_accepted_as_global_flag() {
    run_fixture_case("check", "mode-parity");
}

#[test]
fn check_allows_global_mode_flag_after_subcommand() {
    run_fixture_case("check", "mode-parity-after-check");
}

#[test]
fn check_rejects_unmatched_delimiters() {
    run_fixture_case("check", "unmatched-delimiters");
}

#[test]
fn check_accepts_project_flag_after_subcommand() {
    run_fixture_case("check", "project-config-after");
}

#[test]
fn format_check_clean_file_is_idempotent() {
    run_fixture_case("format-check", "clean");
}

#[test]
fn format_check_operator_spacing_is_idempotent() {
    run_fixture_case("format-check", "operator-spacing-idempotent");
}

#[test]
fn format_check_dirty_file_rejects_unformatted_input() {
    run_fixture_case("format-check", "needs-format");
}

#[test]
fn check_passes_for_valid_file() {
    run_fixture_case("check", "ok");
}

#[test]
fn check_fails_for_invalid_file() {
    run_fixture_case("check", "bad");
}

#[test]
fn rules_command_returns_expected_payload() {
    run_fixture_case("rules", "default");
}

#[test]
fn format_write_updates_source_when_requested() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("format-check")
        .join("needs-format");

    let fixture_input = fs::read_to_string(fixture_dir.join("input.gd")).unwrap();
    let expected_output = format_gdscript(&fixture_input);

    let temp_dir = env::temp_dir().join("gdscript-lsp-tests");
    fs::create_dir_all(&temp_dir).unwrap();
    let temp_path = temp_dir.join(format!("needs-format-{}.gd", process::id()));

    fs::write(&temp_path, &fixture_input).unwrap();

    let (code, stdout, stderr) =
        run_cli_with_args(&["format", "--write", temp_path.to_str().unwrap()], None);

    assert_eq!(code, 0, "unexpected CLI code");
    assert!(stdout.is_empty(), "unexpected stdout for --write: {stdout}");
    assert!(stderr.is_empty(), "unexpected stderr for --write: {stderr}");
    assert_eq!(
        fs::read_to_string(&temp_path).unwrap(),
        expected_output,
        "formatted output should be written in place"
    );
}

#[test]
fn parity_report_json_command_returns_valid_payload() {
    let (code, stdout, stderr) = run_cli_with_args(&["parity-report", "--json"], None);
    assert_eq!(code, 0, "unexpected CLI exit code");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");

    let parsed: Value = serde_json::from_str(&stdout).expect("parity report JSON");
    assert!(
        parsed.get("summary").is_some(),
        "missing summary section: {parsed}"
    );
    assert!(
        parsed.get("parser").is_some(),
        "missing parser section: {parsed}"
    );
    assert!(
        parsed.get("analyzer").is_some(),
        "missing analyzer section: {parsed}"
    );
    assert!(
        parsed.get("warnings").is_some(),
        "missing warnings section: {parsed}"
    );
}
