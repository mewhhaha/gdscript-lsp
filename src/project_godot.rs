use crate::engine::{BehaviorMode, Version};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGodotConfig {
    pub sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl ProjectGodotConfig {
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|sec| sec.get(key))
            .map(String::as_str)
    }

    pub fn lint_max_line_length(&self) -> Option<usize> {
        self.get_first(&[
            ("gdscript", "lint/max_line_length"),
            ("gdscript", "max_line_length"),
            ("lint", "max_line_length"),
        ])
        .and_then(|raw| raw.parse::<usize>().ok())
    }

    pub fn lint_allow_tabs(&self) -> Option<bool> {
        self.get_first(&[
            ("gdscript", "lint/allow_tabs"),
            ("gdscript", "allow_tabs"),
            ("lint", "allow_tabs"),
        ])
        .and_then(parse_bool)
    }

    pub fn lint_require_spaces_around_operators(&self) -> Option<bool> {
        self.get_first(&[
            ("gdscript", "lint/require_spaces_around_operators"),
            ("gdscript", "require_spaces_around_operators"),
            ("lint", "require_spaces_around_operators"),
        ])
        .and_then(parse_bool)
    }

    pub fn lint_disabled_rules(&self) -> Option<BTreeSet<String>> {
        self.get_first(&[
            ("gdscript", "lint/disabled_rules"),
            ("gdscript", "disabled_rules"),
            ("lint", "disabled_rules"),
        ])
        .map(parse_rule_list)
    }

    pub fn lint_enabled_rules(&self) -> Option<BTreeSet<String>> {
        self.get_first(&[
            ("gdscript", "lint/enabled_rules"),
            ("gdscript", "enabled_rules"),
            ("lint", "enabled_rules"),
        ])
        .map(parse_rule_list)
    }

    pub fn lint_severity_overrides(&self) -> BTreeMap<String, String> {
        let mut overrides = BTreeMap::new();

        if let Some(section) = self.sections.get("gdscript") {
            for (key, value) in section {
                if let Some(rule) = key.strip_prefix("lint/severity/") {
                    overrides.insert(normalize_rule_key(rule).to_string(), value.clone());
                }
            }
        }

        if let Some(section) = self.sections.get("lint") {
            for (key, value) in section {
                if let Some(rule) = key.strip_prefix("lint/severity/") {
                    overrides.insert(normalize_rule_key(rule).to_string(), value.clone());
                }
            }
        }

        overrides
    }

    pub fn godot_version(&self) -> Option<Version> {
        self.get_first(&[
            ("gdscript", "godot_version"),
            ("gdscript", "godot-version"),
            ("engine", "godot_version"),
            ("engine", "godot-version"),
        ])
        .and_then(Version::from_raw)
    }

    pub fn behavior_mode(&self) -> Option<BehaviorMode> {
        self.get_first(&[
            ("gdscript", "behavior_mode"),
            ("gdscript", "behavior-mode"),
            ("engine", "behavior_mode"),
            ("engine", "behavior-mode"),
            ("lsp", "mode"),
        ])
        .and_then(BehaviorMode::from_raw)
    }

    fn get_first<'a>(&'a self, pairs: &[(&str, &str)]) -> Option<&'a str> {
        for (section, key) in pairs {
            if let Some(value) = self.get(section, key) {
                return Some(value);
            }
        }
        None
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_rule_list(raw: &str) -> BTreeSet<String> {
    raw.split(',')
        .map(normalize_rule_key)
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_rule_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('_', "-")
}

pub fn parse_project_godot_config(contents: &str) -> ProjectGodotConfig {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current_section = String::from("global");

    for raw_line in contents.replace("\r\n", "\n").lines().map(str::trim) {
        if raw_line.is_empty() || raw_line.starts_with(';') || raw_line.starts_with('#') {
            continue;
        }
        if raw_line.starts_with('[') && raw_line.ends_with(']') {
            current_section = raw_line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            sections.entry(current_section.clone()).or_default();
            continue;
        }
        if let Some((key, value)) = raw_line.split_once('=') {
            let entry = sections.entry(current_section.clone()).or_default();
            entry.insert(
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            );
        }
    }

    ProjectGodotConfig { sections }
}

pub fn load_project_godot_config(path: impl AsRef<Path>) -> Result<ProjectGodotConfig> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read project config {}", path.display()))?;
    Ok(parse_project_godot_config(&contents))
}

#[cfg(test)]
mod tests {
    use super::parse_project_godot_config;

    #[test]
    fn parses_lint_settings() {
        let config = parse_project_godot_config(
            r#"
[gdscript]
lint/max_line_length=88
lint/allow_tabs=true
lint/require_spaces_around_operators=false
"#,
        );

        assert_eq!(config.lint_max_line_length(), Some(88));
        assert_eq!(config.lint_allow_tabs(), Some(true));
        assert_eq!(config.lint_require_spaces_around_operators(), Some(false));
    }

    #[test]
    fn parses_rule_controls_and_severity_overrides() {
        let config = parse_project_godot_config(
            r#"
[gdscript]
lint/disabled_rules = trailing-whitespace,no-tabs
lint/enabled_rules=todo-comment,spaces-around-operator
lint/severity/max-line-length=warning
lint/severity/todo-comment=error
"#,
        );

        let disabled_rules = config.lint_disabled_rules().expect("disabled rules");
        assert!(disabled_rules.contains("trailing-whitespace"));
        assert!(disabled_rules.contains("no-tabs"));

        let enabled_rules = config.lint_enabled_rules().expect("enabled rules");
        assert!(enabled_rules.contains("todo-comment"));
        assert!(enabled_rules.contains("spaces-around-operator"));

        let severity = config.lint_severity_overrides();
        assert_eq!(
            severity.get("max-line-length"),
            Some(&"warning".to_string())
        );
        assert_eq!(severity.get("todo-comment"), Some(&"error".to_string()));
    }
}
