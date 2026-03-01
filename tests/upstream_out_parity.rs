use gdscript_lsp::{
    LintSettings, check_document_with_settings, parse_project_godot_config, parse_script, rule_ids,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ParserOutExpectation {
    line: Option<usize>,
    message: String,
}

#[derive(Debug, Clone)]
struct WarningOutExpectation {
    line: usize,
    code: String,
}

fn parser_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("parser")
        .join("upstream_errors")
}

fn warning_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lint")
        .join("upstream_warnings")
}

fn fixture_stem_list(dir: &Path, extension: &str) -> Vec<String> {
    let mut stems = fs::read_dir(dir)
        .expect("failed to read fixture directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == extension)
        })
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    stems.sort_unstable();
    stems
}

fn parse_parser_out(contents: &str) -> ParserOutExpectation {
    let lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert!(
        lines.len() >= 2,
        "invalid parser out fixture format: expected at least 2 lines, got {lines:?}"
    );
    let payload = lines[1];
    let marker = ">> ERROR at line ";
    if let Some(rest) = payload.strip_prefix(marker) {
        let Some((line_part, message)) = rest.split_once(':') else {
            return ParserOutExpectation {
                line: None,
                message: payload.to_string(),
            };
        };
        let line = line_part.trim().parse::<usize>().ok();
        return ParserOutExpectation {
            line,
            message: message.trim().to_string(),
        };
    }

    ParserOutExpectation {
        line: None,
        message: payload.to_string(),
    }
}

fn normalized_message(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parser_messages_match(expected: &str, actual: &str) -> bool {
    let expected_normalized = normalized_message(expected);
    let actual_normalized = normalized_message(actual);
    if actual_normalized.contains(&expected_normalized)
        || expected_normalized.contains(&actual_normalized)
    {
        return true;
    }

    expected_normalized.contains("expected closing after grouping expression")
        && actual_normalized.contains("unmatched")
        || expected_normalized.contains("expected closing after call arguments")
            && actual_normalized.contains("unmatched")
}

fn parse_warning_out(contents: &str) -> Vec<WarningOutExpectation> {
    let mut out = Vec::new();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let marker = "~~ WARNING at line ";
        let Some(rest) = line.strip_prefix(marker) else {
            continue;
        };
        let Some((line_part, tail)) = rest.split_once(':') else {
            continue;
        };
        let Some(code_start) = tail.find('(') else {
            continue;
        };
        let Some(code_end) = tail.find(')') else {
            continue;
        };
        if code_end <= code_start + 1 {
            continue;
        }
        let line_number = line_part.trim().parse::<usize>().ok();
        let line_number = line_number.unwrap_or(0);
        if line_number == 0 {
            continue;
        }
        let upstream_code = &tail[code_start + 1..code_end];
        out.push(WarningOutExpectation {
            line: line_number,
            code: upstream_warning_code_to_local_rule_id(upstream_code),
        });
    }
    out
}

fn upstream_warning_code_to_local_rule_id(code: &str) -> String {
    code.to_ascii_lowercase().replace('_', "-")
}

fn warning_parity_settings() -> LintSettings {
    let mut project = String::from("[gdscript]\n");
    for rule in rule_ids() {
        project.push_str(&format!(
            "lint/severity/{}=warning\n",
            rule.replace('-', "_")
        ));
    }
    let config = parse_project_godot_config(&project);
    LintSettings::from_project_config(Some(&config))
}

#[test]
fn parser_out_fixtures_match_expected_error_messages() {
    let fixture_dir = parser_fixture_dir();
    let fixtures = fixture_stem_list(&fixture_dir, "out");
    assert_eq!(fixtures.len(), 76, "expected 76 parser out fixtures");

    for name in fixtures {
        let source = fs::read_to_string(fixture_dir.join(format!("{name}.gd")))
            .unwrap_or_else(|err| panic!("missing parser fixture source {name}.gd: {err}"));
        let out = fs::read_to_string(fixture_dir.join(format!("{name}.out")))
            .unwrap_or_else(|err| panic!("missing parser fixture output {name}.out: {err}"));
        let expected = parse_parser_out(&out);
        let parsed = parse_script(&source, format!("upstream/{name}.gd"));

        let found = parsed.issues.iter().any(|issue| {
            parser_messages_match(&expected.message, &issue.message)
                && expected.line.is_none_or(|line| issue.line == line)
        });
        assert!(
            found,
            "fixture `{name}` missing expected parser issue (line={:?}, message=`{}`); got: {:?}",
            expected.line, expected.message, parsed.issues
        );
    }
}

#[test]
fn warning_out_fixtures_match_expected_codes_and_lines() {
    let fixture_dir = warning_fixture_dir();
    let fixtures = fixture_stem_list(&fixture_dir, "out");
    assert_eq!(fixtures.len(), 50, "expected 50 warning out fixtures");

    let settings = warning_parity_settings();
    for name in fixtures {
        let source = fs::read_to_string(fixture_dir.join(format!("{name}.gd")))
            .unwrap_or_else(|err| panic!("missing warning fixture source {name}.gd: {err}"));
        let out = fs::read_to_string(fixture_dir.join(format!("{name}.out")))
            .unwrap_or_else(|err| panic!("missing warning fixture output {name}.out: {err}"));
        let expected = parse_warning_out(&out);
        let diagnostics = check_document_with_settings(&source, &settings);

        for warning in expected {
            let found = diagnostics
                .iter()
                .any(|diag| diag.code == warning.code && diag.line == warning.line);
            assert!(
                found,
                "fixture `{name}` missing expected warning code+line {}:{}; got {:?}",
                warning.code, warning.line, diagnostics
            );
        }
    }
}
