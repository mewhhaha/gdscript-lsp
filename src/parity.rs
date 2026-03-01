use crate::{
    BehaviorMode, LintSettings, check_document_with_settings_and_mode, parse_project_godot_config,
    parse_script, rule_ids,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ParityGapReport {
    pub parser: ErrorParityCategory,
    pub analyzer: ErrorParityCategory,
    pub warnings: WarningParityCategory,
    pub summary: ParityGapSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorParityCategory {
    pub coverage: FixtureCoverage,
    pub missing_expected: Vec<ErrorGap>,
    pub unexpected_actual: Vec<ErrorGap>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarningParityCategory {
    pub coverage: FixtureCoverage,
    pub missing_expected: Vec<WarningExpectedGap>,
    pub unexpected_actual: Vec<WarningActualGap>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureCoverage {
    pub gd_fixtures: usize,
    pub out_fixtures: usize,
    pub notest_fixtures: Vec<String>,
    pub fixtures_without_out: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorGap {
    pub fixture: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarningExpectedGap {
    pub fixture: String,
    pub line: usize,
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarningActualGap {
    pub fixture: String,
    pub line: usize,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParityGapSummary {
    pub uncovered_fixtures: usize,
    pub missing_expected_issues: usize,
    pub unexpected_issues: usize,
}

impl ParityGapSummary {
    pub fn total_gaps(&self) -> usize {
        self.uncovered_fixtures + self.missing_expected_issues + self.unexpected_issues
    }
}

#[derive(Debug, Clone)]
struct ExpectedErrorIssue {
    line: usize,
    message: String,
}

#[derive(Debug, Clone)]
struct ExpectedWarningIssue {
    line: usize,
    code: String,
}

pub fn build_parity_gap_report() -> Result<ParityGapReport> {
    let parser_dir = fixture_dir("parser", "upstream_errors");
    let analyzer_dir = fixture_dir("analyzer", "upstream_errors");
    let warning_dir = fixture_dir("lint", "upstream_warnings");

    let parser = build_parser_category(&parser_dir)?;
    let analyzer = build_analyzer_category(&analyzer_dir)?;
    let warnings = build_warning_category(&warning_dir)?;

    let summary = ParityGapSummary {
        uncovered_fixtures: parser.coverage.fixtures_without_out.len()
            + analyzer.coverage.fixtures_without_out.len()
            + warnings.coverage.fixtures_without_out.len(),
        missing_expected_issues: parser.missing_expected.len()
            + analyzer.missing_expected.len()
            + warnings.missing_expected.len(),
        unexpected_issues: parser.unexpected_actual.len()
            + analyzer.unexpected_actual.len()
            + warnings.unexpected_actual.len(),
    };

    Ok(ParityGapReport {
        parser,
        analyzer,
        warnings,
        summary,
    })
}

pub fn render_parity_gap_report(report: &ParityGapReport, limit: usize) -> String {
    let mut out = String::new();
    out.push_str("Parity Gap Report\n");
    out.push_str("=================\n");
    out.push_str(&format!(
        "Summary: uncovered_fixtures={}, missing_expected_issues={}, unexpected_issues={}, total_gaps={}\n\n",
        report.summary.uncovered_fixtures,
        report.summary.missing_expected_issues,
        report.summary.unexpected_issues,
        report.summary.total_gaps()
    ));

    append_error_category(&mut out, "Parser", &report.parser, limit);
    append_error_category(&mut out, "Analyzer", &report.analyzer, limit);
    append_warning_category(&mut out, "Warnings", &report.warnings, limit);
    out
}

fn append_error_category(
    out: &mut String,
    title: &str,
    category: &ErrorParityCategory,
    limit: usize,
) {
    out.push_str(&format!("{title}\n"));
    out.push_str(&format!(
        "  coverage: gd={}, out={}, notest={}, uncovered={}\n",
        category.coverage.gd_fixtures,
        category.coverage.out_fixtures,
        category.coverage.notest_fixtures.len(),
        category.coverage.fixtures_without_out.len()
    ));

    if !category.coverage.fixtures_without_out.is_empty() {
        out.push_str("  uncovered fixtures:\n");
        for fixture in category.coverage.fixtures_without_out.iter().take(limit) {
            out.push_str(&format!("    - {fixture}\n"));
        }
    }

    out.push_str(&format!(
        "  missing expected issues: {}\n",
        category.missing_expected.len()
    ));
    for issue in category.missing_expected.iter().take(limit) {
        out.push_str(&format!(
            "    - {}:{} expected `{}`\n",
            issue.fixture, issue.line, issue.message
        ));
    }

    out.push_str(&format!(
        "  unexpected issues: {}\n",
        category.unexpected_actual.len()
    ));
    for issue in category.unexpected_actual.iter().take(limit) {
        out.push_str(&format!(
            "    - {}:{} unexpected `{}`\n",
            issue.fixture, issue.line, issue.message
        ));
    }
    out.push('\n');
}

fn append_warning_category(
    out: &mut String,
    title: &str,
    category: &WarningParityCategory,
    limit: usize,
) {
    out.push_str(&format!("{title}\n"));
    out.push_str(&format!(
        "  coverage: gd={}, out={}, notest={}, uncovered={}\n",
        category.coverage.gd_fixtures,
        category.coverage.out_fixtures,
        category.coverage.notest_fixtures.len(),
        category.coverage.fixtures_without_out.len()
    ));

    if !category.coverage.fixtures_without_out.is_empty() {
        out.push_str("  uncovered fixtures:\n");
        for fixture in category.coverage.fixtures_without_out.iter().take(limit) {
            out.push_str(&format!("    - {fixture}\n"));
        }
    }

    out.push_str(&format!(
        "  missing expected issues: {}\n",
        category.missing_expected.len()
    ));
    for issue in category.missing_expected.iter().take(limit) {
        out.push_str(&format!(
            "    - {}:{} expected code `{}`\n",
            issue.fixture, issue.line, issue.code
        ));
    }

    out.push_str(&format!(
        "  unexpected issues: {}\n",
        category.unexpected_actual.len()
    ));
    for issue in category.unexpected_actual.iter().take(limit) {
        out.push_str(&format!(
            "    - {}:{} unexpected {} `{}`\n",
            issue.fixture, issue.line, issue.code, issue.message
        ));
    }
    out.push('\n');
}

fn build_parser_category(dir: &Path) -> Result<ErrorParityCategory> {
    let coverage = fixture_coverage(dir)?;
    let out_fixtures = fixture_stems(dir, "out")?;
    let mut missing_expected = Vec::new();
    let mut unexpected_actual = Vec::new();

    for fixture in out_fixtures {
        let source = fs::read_to_string(dir.join(format!("{fixture}.gd")))
            .with_context(|| format!("failed to read parser fixture source `{fixture}.gd`"))?;
        let out = fs::read_to_string(dir.join(format!("{fixture}.out")))
            .with_context(|| format!("failed to read parser fixture output `{fixture}.out`"))?;
        let expected = parse_parser_out(&out);
        let parsed = parse_script(&source, format!("upstream/{fixture}.gd"));
        let actual = parsed
            .issues
            .iter()
            .map(|issue| ExpectedErrorIssue {
                line: issue.line,
                message: issue.message.clone(),
            })
            .collect::<Vec<_>>();

        let mut matched_actual = vec![false; actual.len()];
        for expected_issue in expected {
            if let Some((idx, _)) = actual.iter().enumerate().find(|(idx, actual_issue)| {
                !matched_actual[*idx]
                    && (expected_issue.line == 0 || expected_issue.line == actual_issue.line)
                    && parser_messages_match(&expected_issue.message, &actual_issue.message)
            }) {
                matched_actual[idx] = true;
            } else {
                missing_expected.push(ErrorGap {
                    fixture: fixture.clone(),
                    line: expected_issue.line,
                    message: expected_issue.message,
                });
            }
        }

        for (idx, actual_issue) in actual.into_iter().enumerate() {
            if matched_actual[idx] {
                continue;
            }
            unexpected_actual.push(ErrorGap {
                fixture: fixture.clone(),
                line: actual_issue.line,
                message: actual_issue.message,
            });
        }
    }

    Ok(ErrorParityCategory {
        coverage,
        missing_expected,
        unexpected_actual,
    })
}

fn build_analyzer_category(dir: &Path) -> Result<ErrorParityCategory> {
    let coverage = fixture_coverage(dir)?;
    let out_fixtures = fixture_stems(dir, "out")?;
    let mut missing_expected = Vec::new();
    let mut unexpected_actual = Vec::new();

    for fixture in out_fixtures {
        let source = fs::read_to_string(dir.join(format!("{fixture}.gd")))
            .with_context(|| format!("failed to read analyzer fixture source `{fixture}.gd`"))?;
        let out = fs::read_to_string(dir.join(format!("{fixture}.out")))
            .with_context(|| format!("failed to read analyzer fixture output `{fixture}.out`"))?;
        let expected = parse_error_out(&out);
        let expected_lines = expected
            .iter()
            .map(|issue| issue.line)
            .collect::<HashSet<_>>();
        let parsed = parse_script(&source, format!("upstream/analyzer/{fixture}.gd"));
        let actual = parsed
            .issues
            .iter()
            .map(|issue| ExpectedErrorIssue {
                line: issue.line,
                message: issue.message.clone(),
            })
            .collect::<Vec<_>>();

        let mut matched_actual = vec![false; actual.len()];
        for expected_issue in expected {
            if let Some((idx, _)) = actual.iter().enumerate().find(|(idx, actual_issue)| {
                !matched_actual[*idx]
                    && expected_issue.line == actual_issue.line
                    && actual_issue
                        .message
                        .contains(expected_issue.message.as_str())
            }) {
                matched_actual[idx] = true;
            } else {
                missing_expected.push(ErrorGap {
                    fixture: fixture.clone(),
                    line: expected_issue.line,
                    message: expected_issue.message,
                });
            }
        }

        for (idx, actual_issue) in actual.into_iter().enumerate() {
            if matched_actual[idx] {
                continue;
            }
            if expected_lines.contains(&actual_issue.line) {
                continue;
            }
            unexpected_actual.push(ErrorGap {
                fixture: fixture.clone(),
                line: actual_issue.line,
                message: actual_issue.message,
            });
        }
    }

    Ok(ErrorParityCategory {
        coverage,
        missing_expected,
        unexpected_actual,
    })
}

fn build_warning_category(dir: &Path) -> Result<WarningParityCategory> {
    let coverage = fixture_coverage(dir)?;
    let out_fixtures = fixture_stems(dir, "out")?;
    let mut missing_expected = Vec::new();
    let mut unexpected_actual = Vec::new();

    for fixture in out_fixtures {
        let source = fs::read_to_string(dir.join(format!("{fixture}.gd")))
            .with_context(|| format!("failed to read warning fixture source `{fixture}.gd`"))?;
        let out = fs::read_to_string(dir.join(format!("{fixture}.out")))
            .with_context(|| format!("failed to read warning fixture output `{fixture}.out`"))?;
        let expected = parse_warning_out(&out);
        let settings = warning_settings_for_expected_codes(&expected);
        let actual =
            check_document_with_settings_and_mode(&source, &settings, BehaviorMode::Parity)
                .iter()
                .map(|diagnostic| WarningActualGap {
                    fixture: fixture.clone(),
                    line: diagnostic.line,
                    code: diagnostic.code.clone(),
                    message: diagnostic.message.clone(),
                })
                .collect::<Vec<_>>();

        let mut matched_actual = vec![false; actual.len()];
        for expected_issue in expected {
            if let Some((idx, _)) = actual.iter().enumerate().find(|(idx, actual_issue)| {
                !matched_actual[*idx]
                    && expected_issue.line == actual_issue.line
                    && expected_issue.code == actual_issue.code
            }) {
                matched_actual[idx] = true;
            } else {
                missing_expected.push(WarningExpectedGap {
                    fixture: fixture.clone(),
                    line: expected_issue.line,
                    code: expected_issue.code,
                });
            }
        }

        for (idx, actual_issue) in actual.into_iter().enumerate() {
            if matched_actual[idx] {
                continue;
            }
            unexpected_actual.push(actual_issue);
        }
    }

    Ok(WarningParityCategory {
        coverage,
        missing_expected,
        unexpected_actual,
    })
}

fn fixture_dir(suite: &str, case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(suite)
        .join(case)
}

fn fixture_coverage(dir: &Path) -> Result<FixtureCoverage> {
    let gd_fixtures = fixture_stems(dir, "gd")?;
    let out_fixtures = fixture_stems(dir, "out")?;
    let out_set = out_fixtures.iter().cloned().collect::<HashSet<_>>();
    let notest_fixtures = gd_fixtures
        .iter()
        .filter(|fixture| fixture.contains(".notest"))
        .cloned()
        .collect::<Vec<_>>();

    let fixtures_without_out = gd_fixtures
        .iter()
        .filter(|fixture| !fixture.contains(".notest") && !out_set.contains(*fixture))
        .cloned()
        .collect::<Vec<_>>();

    Ok(FixtureCoverage {
        gd_fixtures: gd_fixtures.len(),
        out_fixtures: out_fixtures.len(),
        notest_fixtures,
        fixtures_without_out,
    })
}

fn fixture_stems(dir: &Path, extension: &str) -> Result<Vec<String>> {
    let mut stems = fs::read_dir(dir)
        .with_context(|| format!("failed to read fixture directory `{}`", dir.display()))?
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
    Ok(stems)
}

fn parse_error_out(contents: &str) -> Vec<ExpectedErrorIssue> {
    contents
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let marker = ">> ERROR at line ";
            let rest = line.strip_prefix(marker)?;
            let (line_part, message) = rest.split_once(':')?;
            let line = line_part.trim().parse::<usize>().ok()?;
            Some(ExpectedErrorIssue {
                line,
                message: message.trim().to_string(),
            })
        })
        .collect()
}

fn parse_parser_out(contents: &str) -> Vec<ExpectedErrorIssue> {
    let parsed = parse_error_out(contents);
    if !parsed.is_empty() {
        return parsed;
    }

    let lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() < 2 {
        return Vec::new();
    }

    let payload = lines[1];
    let marker = ">> ERROR at line ";
    if let Some(rest) = payload.strip_prefix(marker) {
        if let Some((line_part, message)) = rest.split_once(':') {
            if let Ok(line) = line_part.trim().parse::<usize>() {
                return vec![ExpectedErrorIssue {
                    line,
                    message: message.trim().to_string(),
                }];
            }
        }
    }

    vec![ExpectedErrorIssue {
        line: 0,
        message: payload.to_string(),
    }]
}

fn parse_warning_out(contents: &str) -> Vec<ExpectedWarningIssue> {
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
        let Some(line) = line_part.trim().parse::<usize>().ok() else {
            continue;
        };
        let code = tail[code_start + 1..code_end]
            .to_ascii_lowercase()
            .replace('_', "-");
        out.push(ExpectedWarningIssue { line, code });
    }
    out
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

fn warning_settings_for_expected_codes(expected: &[ExpectedWarningIssue]) -> LintSettings {
    let mut project = String::from("[gdscript]\n");
    let expected_codes = expected
        .iter()
        .map(|issue| issue.code.as_str())
        .collect::<HashSet<_>>();

    for rule in rule_ids() {
        let severity = if expected_codes.contains(rule.as_str()) {
            "warning"
        } else {
            "off"
        };
        project.push_str(&format!(
            "lint/severity/{}={severity}\n",
            rule.replace('-', "_")
        ));
    }
    let config = parse_project_godot_config(&project);
    LintSettings::from_project_config(Some(&config))
}
