use gdscript_lsp::parse_script;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct AnalyzerOutExpectation {
    line: usize,
    message: String,
}

fn analyzer_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("analyzer")
        .join("upstream_errors")
}

fn fixture_stem_list(dir: &Path, extension: &str) -> Vec<String> {
    let mut stems = fs::read_dir(dir)
        .expect("failed to read analyzer fixture directory")
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

fn parse_analyzer_out(contents: &str) -> Vec<AnalyzerOutExpectation> {
    contents
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let marker = ">> ERROR at line ";
            let rest = line.strip_prefix(marker)?;
            let (line_part, message) = rest.split_once(':')?;
            let line = line_part.trim().parse::<usize>().ok()?;
            Some(AnalyzerOutExpectation {
                line,
                message: message.trim().to_string(),
            })
        })
        .collect::<Vec<_>>()
}

#[test]
fn analyzer_out_fixtures_match_expected_messages_and_lines() {
    let fixture_dir = analyzer_fixture_dir();
    let fixtures = fixture_stem_list(&fixture_dir, "out");
    assert_eq!(fixtures.len(), 170, "expected 170 analyzer out fixtures");

    for fixture in fixtures {
        let source = fs::read_to_string(fixture_dir.join(format!("{fixture}.gd")))
            .unwrap_or_else(|err| panic!("missing analyzer source fixture {fixture}.gd: {err}"));
        let out = fs::read_to_string(fixture_dir.join(format!("{fixture}.out")))
            .unwrap_or_else(|err| panic!("missing analyzer output fixture {fixture}.out: {err}"));
        let expected = parse_analyzer_out(&out);
        let parsed = parse_script(&source, format!("upstream/analyzer/{fixture}.gd"));

        for expected_issue in expected {
            let found = parsed.issues.iter().any(|actual| {
                actual.line == expected_issue.line
                    && actual.message.contains(expected_issue.message.as_str())
            });
            assert!(
                found,
                "fixture `{fixture}` missing expected analyzer issue (line={}, message=`{}`); got {:?}",
                expected_issue.line, expected_issue.message, parsed.issues
            );
        }
    }
}
