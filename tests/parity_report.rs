use gdscript_lsp::{build_parity_gap_report, render_parity_gap_report};

#[test]
fn parity_report_builds_with_expected_fixture_presence() {
    let report = build_parity_gap_report().expect("parity report");
    assert!(report.parser.coverage.gd_fixtures > 0);
    assert!(report.analyzer.coverage.gd_fixtures > 0);
    assert!(report.warnings.coverage.gd_fixtures > 0);
}

#[test]
fn parity_report_text_render_includes_all_sections() {
    let report = build_parity_gap_report().expect("parity report");
    let rendered = render_parity_gap_report(&report, 3);
    assert!(rendered.contains("Parser"));
    assert!(rendered.contains("Analyzer"));
    assert!(rendered.contains("Warnings"));
    assert!(rendered.contains("Summary:"));
}
