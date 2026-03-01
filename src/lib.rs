mod cli;
pub mod code_actions;
mod docs_meta;
pub mod engine;
pub mod formatter;
pub mod hover;
pub mod lint;
pub mod lsp;
pub mod parity;
pub mod parser;
pub mod project_godot;
pub mod semantic;

pub use cli::{Cli, Commands, GlobalOptions, LintRuleOverrides};
pub use code_actions::{
    CodeAction, CodeActionKind, CodeActionPatch, TextRange, code_actions_for_diagnostics,
    code_actions_for_diagnostics_and_mode,
};
pub use engine::{BehaviorMode, EngineConfig, Version};
pub use formatter::{format_gdscript, is_formatted};
pub use hover::{Hover, hover_at};
pub use lint::{
    Diagnostic, DiagnosticCollection, DiagnosticLevel, LintOverrides, LintSettings, check_document,
    check_document_with_mode, check_document_with_settings, check_document_with_settings_and_mode,
    rule_ids,
};
pub use parity::{build_parity_gap_report, render_parity_gap_report};
pub use parser::{ParsedScript, ParserError, ScriptDecl, parse_script};
pub use project_godot::{
    ProjectGodotConfig, load_project_godot_config, parse_project_godot_config,
};

use anyhow::Result;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn lint_command(
    paths: Vec<PathBuf>,
    project: Option<PathBuf>,
    overrides: LintOverrides,
    godot_version: Option<Version>,
    mode: Option<BehaviorMode>,
) -> Result<()> {
    let mut has_errors = false;
    let settings = resolve_lint_settings(project.as_deref(), overrides);
    let engine_config = resolve_engine_config(project.as_deref(), godot_version, mode);

    for path in paths {
        let source = fs::read_to_string(&path)?;
        let parsed = parse_script(&source, &path);
        let mut diagnostics: DiagnosticCollection = parsed
            .issues
            .iter()
            .map(parser_error_to_diagnostic)
            .collect();
        diagnostics.extend(crate::lint::check_document_with_settings_and_mode(
            &source,
            &settings,
            engine_config.behavior_mode,
        ));

        has_errors |= !diagnostics.is_empty();
        render_diagnostics(&source, &diagnostics);
    }

    if has_errors {
        return Err(anyhow::anyhow!("lint failed"));
    }
    Ok(())
}

pub fn format_command(paths: Vec<PathBuf>, write: bool, check: bool) -> Result<()> {
    let mut has_changes = false;

    for path in paths {
        let source = fs::read_to_string(&path)?;
        let formatted = format_gdscript(&source);

        if check {
            if formatted == source {
                println!("{}: no formatting changes needed", path.display());
            } else {
                println!("{}: would reformat file", path.display());
                has_changes = true;
            }
            continue;
        }

        if write {
            fs::write(&path, formatted)?;
        } else {
            let mut stdout = io::stdout();
            stdout.write_all(formatted.as_bytes())?;
            stdout.write_all(b"\n")?;
        }
    }

    if check && has_changes {
        return Err(anyhow::anyhow!("format check failed"));
    }

    Ok(())
}

pub fn check_command(
    paths: Vec<PathBuf>,
    project: Option<PathBuf>,
    overrides: LintOverrides,
    godot_version: Option<Version>,
    mode: Option<BehaviorMode>,
) -> Result<()> {
    let mut has_diagnostics = false;
    let settings = resolve_lint_settings(project.as_deref(), overrides);
    let engine_config = resolve_engine_config(project.as_deref(), godot_version, mode);

    for path in &paths {
        let source = fs::read_to_string(path)?;
        let parsed = parse_script(&source, path);

        if !parsed.issues.is_empty() {
            eprintln!("{}: parse error", path.display());
            has_diagnostics = true;
            continue;
        }

        let diagnostics = crate::lint::check_document_with_settings_and_mode(
            &source,
            &settings,
            engine_config.behavior_mode,
        );
        if !diagnostics.is_empty() {
            has_diagnostics = true;
            render_diagnostics(&source, &diagnostics);
        } else {
            println!("{}: ok", path.display());
        }
    }

    if has_diagnostics {
        return Err(anyhow::anyhow!("check failed"));
    }

    Ok(())
}

fn resolve_lint_settings(project_path: Option<&Path>, overrides: LintOverrides) -> LintSettings {
    let config = load_project_config(project_path);
    LintSettings::from_project_config(config.as_ref()).with_overrides(overrides)
}

pub fn resolve_engine_config(
    project_path: Option<&Path>,
    godot_version: Option<Version>,
    mode: Option<BehaviorMode>,
) -> EngineConfig {
    let config = load_project_config(project_path);
    let config_version = config.as_ref().and_then(ProjectGodotConfig::godot_version);
    let config_mode = config.as_ref().and_then(ProjectGodotConfig::behavior_mode);

    EngineConfig {
        godot_version: godot_version.or(config_version).unwrap_or_default(),
        behavior_mode: mode.or(config_mode).unwrap_or_default(),
    }
}

fn load_project_config(project_path: Option<&Path>) -> Option<ProjectGodotConfig> {
    if let Some(path) = project_path {
        return load_project_godot_config(path).ok();
    }

    let cwd_project = Path::new("project.godot");
    if cwd_project.exists() {
        return load_project_godot_config(cwd_project).ok();
    }

    None
}

fn render_diagnostics(source: &str, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        if let Some(line) = source.lines().nth(diagnostic.line.saturating_sub(1)) {
            eprintln!("{}", line);
            eprintln!(
                "{}^ {}",
                " ".repeat(diagnostic.column.saturating_sub(1)),
                diagnostic.message
            );
        }
    }
}

fn parser_error_to_diagnostic(error: &ParserError) -> Diagnostic {
    Diagnostic {
        file: None,
        line: error.line,
        column: 1,
        code: "parser-error".to_string(),
        level: DiagnosticLevel::Error,
        message: error.message.clone(),
    }
}

pub fn rules_command() -> Result<()> {
    let mut out = io::stdout();
    for rule in rule_ids() {
        writeln!(out, "{rule}")?;
    }
    Ok(())
}

pub fn parity_report_command(json: bool, strict: bool, limit: usize) -> Result<()> {
    let report = build_parity_gap_report()?;
    let mut out = io::stdout();

    if json {
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else {
        write!(out, "{}", render_parity_gap_report(&report, limit))?;
    }

    if strict && report.summary.total_gaps() > 0 {
        return Err(anyhow::anyhow!(
            "parity gaps found (total={})",
            report.summary.total_gaps()
        ));
    }

    Ok(())
}
