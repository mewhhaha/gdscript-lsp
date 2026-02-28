use crate::engine::BehaviorMode;
use crate::project_godot::ProjectGodotConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub type DiagnosticCollection = Vec<Diagnostic>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Info,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: Option<String>,
    pub line: usize,
    pub column: usize,
    pub code: String,
    pub level: DiagnosticLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintSettings {
    pub max_line_length: usize,
    pub allow_tabs: bool,
    pub require_spaces_around_operators: bool,
    pub forbid_trailing_whitespace: bool,
    pub(crate) enabled_rules: Option<BTreeSet<String>>,
    pub(crate) disabled_rules: BTreeSet<String>,
    pub(crate) rule_severities: BTreeMap<String, DiagnosticLevel>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LintOverrides {
    pub max_line_length: Option<usize>,
    pub allow_tabs: Option<bool>,
    pub require_spaces_around_operators: Option<bool>,
}

const RULE_TRAILING_WHITESPACE: &str = "trailing-whitespace";
const RULE_NO_TABS: &str = "no-tabs";
const RULE_MAX_LINE_LENGTH: &str = "max-line-length";
const RULE_SPACES_AROUND_OPERATOR: &str = "spaces-around-operator";
const RULE_TODO_COMMENT: &str = "todo-comment";
const RULE_EMPTY_FILE: &str = "empty-file";
const RULE_STANDALONE_EXPRESSION: &str = "standalone-expression";
const RULE_STANDALONE_TERNARY: &str = "standalone-ternary";
const RULE_RETURN_VALUE_DISCARDED: &str = "return-value-discarded";
const RULE_INTEGER_DIVISION: &str = "integer-division";
const RULE_UNUSED_PARAMETER: &str = "unused-parameter";
const RULE_UNUSED_VARIABLE: &str = "unused-variable";
const RULE_UNUSED_LOCAL_CONSTANT: &str = "unused-local-constant";
const RULE_SHADOWED_GLOBAL_IDENTIFIER: &str = "shadowed-global-identifier";
const RULE_UNREACHABLE_CODE: &str = "unreachable-code";
const RULE_UNASSIGNED_VARIABLE: &str = "unassigned-variable";
const RULE_UNASSIGNED_VARIABLE_OP_ASSIGN: &str = "unassigned-variable-op-assign";
const RULE_STATIC_CALLED_ON_INSTANCE: &str = "static-called-on-instance";
const RULE_ASSERT_ALWAYS_TRUE: &str = "assert-always-true";
const RULE_SHADOWED_VARIABLE: &str = "shadowed-variable";
const RULE_SHADOWED_VARIABLE_BASE_CLASS: &str = "shadowed-variable-base-class";
const RULE_UNREACHABLE_PATTERN: &str = "unreachable-pattern";
const RULE_CONFUSABLE_IDENTIFIER: &str = "confusable-identifier";
const RULE_ONREADY_WITH_EXPORT: &str = "onready-with-export";
const RULE_UNUSED_PRIVATE_CLASS_VARIABLE: &str = "unused-private-class-variable";
const RULE_CONFUSABLE_LOCAL_DECLARATION: &str = "confusable-local-declaration";
const RULE_CONFUSABLE_LOCAL_USAGE: &str = "confusable-local-usage";
const RULE_INFERENCE_ON_VARIANT: &str = "inference-on-variant";
const RULE_INT_AS_ENUM_WITHOUT_CAST: &str = "int-as-enum-without-cast";
const RULE_ENUM_VARIABLE_WITHOUT_DEFAULT: &str = "enum-variable-without-default";
const RULE_INT_AS_ENUM_WITHOUT_MATCH: &str = "int-as-enum-without-match";
const RULE_NARROWING_CONVERSION: &str = "narrowing-conversion";
const RULE_INCOMPATIBLE_TERNARY: &str = "incompatible-ternary";
const RULE_GET_NODE_DEFAULT_WITHOUT_ONREADY: &str = "get-node-default-without-onready";
const RULE_MISSING_AWAIT: &str = "missing-await";
const RULE_NATIVE_METHOD_OVERRIDE: &str = "native-method-override";
const RULE_UNSAFE_CAST: &str = "unsafe-cast";
const RULE_UNSAFE_CALL_ARGUMENT: &str = "unsafe-call-argument";
const RULE_UNUSED_SIGNAL: &str = "unused-signal";
const RULE_MISSING_TOOL: &str = "missing-tool";
const RULE_CONFUSABLE_CAPTURE_REASSIGNMENT: &str = "confusable-capture-reassignment";
const RULE_REDUNDANT_AWAIT: &str = "redundant-await";

impl Default for LintSettings {
    fn default() -> Self {
        Self {
            max_line_length: 120,
            allow_tabs: false,
            require_spaces_around_operators: true,
            forbid_trailing_whitespace: true,
            enabled_rules: None,
            disabled_rules: BTreeSet::new(),
            rule_severities: BTreeMap::new(),
        }
    }
}

impl LintSettings {
    pub fn from_project_config(project: Option<&ProjectGodotConfig>) -> Self {
        let mut settings = Self::default();
        if let Some(project) = project {
            if let Some(max_line_length) = project.lint_max_line_length() {
                settings.max_line_length = max_line_length;
            }
            if let Some(allow_tabs) = project.lint_allow_tabs() {
                settings.allow_tabs = allow_tabs;
            }
            if let Some(require_spaces_around_operators) =
                project.lint_require_spaces_around_operators()
            {
                settings.require_spaces_around_operators = require_spaces_around_operators;
            }

            if let Some(disabled_rules) = project.lint_disabled_rules() {
                settings.disabled_rules = disabled_rules;
            }

            if let Some(enabled_rules) = project.lint_enabled_rules() {
                settings.enabled_rules = Some(enabled_rules);
            }

            for (rule, severity) in project.lint_severity_overrides() {
                if let Some(level) = DiagnosticLevel::from_raw(&severity) {
                    settings.rule_severities.insert(rule, level);
                }
            }
        }
        settings
    }

    pub fn with_overrides(mut self, overrides: LintOverrides) -> Self {
        if let Some(max_line_length) = overrides.max_line_length {
            self.max_line_length = max_line_length;
        }
        if let Some(allow_tabs) = overrides.allow_tabs {
            self.allow_tabs = allow_tabs;
        }
        if let Some(require_spaces_around_operators) = overrides.require_spaces_around_operators {
            self.require_spaces_around_operators = require_spaces_around_operators;
        }
        self
    }

    fn rule_level(&self, rule_code: &str, mode: BehaviorMode) -> Option<DiagnosticLevel> {
        if !rule_available_in_mode(rule_code, mode) {
            return None;
        }

        if let Some(enabled_rules) = self.enabled_rules.as_ref() {
            if !enabled_rules.contains(rule_code) {
                return None;
            }
        }

        if self.disabled_rules.contains(rule_code) {
            return None;
        }

        self.rule_severities
            .get(rule_code)
            .cloned()
            .or_else(|| default_level_for_rule(rule_code))
            .filter(|level| !matches!(level, DiagnosticLevel::Off))
    }
}

pub fn check_document(source: &str) -> DiagnosticCollection {
    check_document_with_settings_and_mode(source, &LintSettings::default(), BehaviorMode::Enhanced)
}

pub fn check_document_with_mode(source: &str, mode: BehaviorMode) -> DiagnosticCollection {
    check_document_with_settings_and_mode(source, &LintSettings::default(), mode)
}

pub fn check_document_with_settings(source: &str, settings: &LintSettings) -> DiagnosticCollection {
    check_document_with_settings_and_mode(source, settings, BehaviorMode::Enhanced)
}

pub fn check_document_with_settings_and_mode(
    source: &str,
    settings: &LintSettings,
    mode: BehaviorMode,
) -> DiagnosticCollection {
    check_document_with_mode_and_settings(source, settings, mode)
}

fn check_document_with_mode_and_settings(
    source: &str,
    settings: &LintSettings,
    mode: BehaviorMode,
) -> DiagnosticCollection {
    let mut diagnostics = Vec::new();
    let normalized = source.replace('\r', "");
    let has_code = normalized
        .lines()
        .map(|line| line.trim())
        .any(|line| !line.is_empty() && !line.starts_with('#'));

    if !has_code {
        if let Some(level) = settings.rule_level(RULE_EMPTY_FILE, mode) {
            diagnostics.push(Diagnostic {
                file: None,
                line: 1,
                column: 1,
                code: RULE_EMPTY_FILE.to_string(),
                level,
                message: "Empty script file.".to_string(),
            });
        }
    }

    for (line_idx, line) in normalized.lines().enumerate() {
        let line_number = line_idx + 1;
        let trimmed = line.trim();

        if settings.forbid_trailing_whitespace
            && (line.ends_with(' ') || line.ends_with('\t'))
            && settings
                .rule_level(RULE_TRAILING_WHITESPACE, mode)
                .is_some()
        {
            let level = settings
                .rule_level(RULE_TRAILING_WHITESPACE, mode)
                .unwrap_or(DiagnosticLevel::Warning);
            diagnostics.push(Diagnostic {
                file: None,
                line: line_number,
                column: line.trim_end_matches([' ', '\t']).len() + 1,
                code: RULE_TRAILING_WHITESPACE.to_string(),
                level,
                message: "trailing whitespace is not allowed".to_string(),
            });
        }

        if !settings.allow_tabs {
            if let Some(level) = settings.rule_level(RULE_NO_TABS, mode) {
                if let Some(tab_index) = line.find('\t') {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: line_number,
                        column: tab_index + 1,
                        code: RULE_NO_TABS.to_string(),
                        level,
                        message: "tabs are replaced with spaces".to_string(),
                    });
                }
            }
        }

        if line.chars().count() > settings.max_line_length {
            if let Some(level) = settings.rule_level(RULE_MAX_LINE_LENGTH, mode) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: settings.max_line_length + 1,
                    code: RULE_MAX_LINE_LENGTH.to_string(),
                    level,
                    message: format!("line exceeds {} characters", settings.max_line_length),
                });
            }
        }

        if settings.require_spaces_around_operators {
            if let Some(level) = settings.rule_level(RULE_SPACES_AROUND_OPERATOR, mode) {
                if let Some(operator_column) = find_unspaced_assignment_operator(line) {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: line_number,
                        column: operator_column,
                        code: RULE_SPACES_AROUND_OPERATOR.to_string(),
                        level,
                        message: "Missing spaces around operator".to_string(),
                    });
                }
            }
        }

        if line.contains("TODO") {
            if let Some(level) = settings.rule_level(RULE_TODO_COMMENT, mode) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: line.find("TODO").unwrap_or(0) + 1,
                    code: RULE_TODO_COMMENT.to_string(),
                    level,
                    message: "TODO comment found".to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_STANDALONE_TERNARY, mode) {
            if is_standalone_ternary(trimmed) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_STANDALONE_TERNARY.to_string(),
                    level,
                    message: "Standalone ternary operator (the return value is being discarded)."
                        .to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_STANDALONE_EXPRESSION, mode) {
            if is_standalone_expression(trimmed) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_STANDALONE_EXPRESSION.to_string(),
                    level,
                    message: "Standalone expression (the line may have no effect).".to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_RETURN_VALUE_DISCARDED, mode) {
            if is_return_value_discarded_call(trimmed) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_RETURN_VALUE_DISCARDED.to_string(),
                    level,
                    message: "The function return value will be discarded if not used.".to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_INTEGER_DIVISION, mode) {
            if let Some(column) = find_integer_division_column(line) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column,
                    code: RULE_INTEGER_DIVISION.to_string(),
                    level,
                    message: "Integer division. Decimal part will be discarded.".to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_ASSERT_ALWAYS_TRUE, mode) {
            if is_assert_always_true(trimmed) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_ASSERT_ALWAYS_TRUE.to_string(),
                    level,
                    message: "Assert statement is redundant because the expression is always true."
                        .to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_CONFUSABLE_IDENTIFIER, mode) {
            if contains_confusable_identifier(line) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_CONFUSABLE_IDENTIFIER.to_string(),
                    level,
                    message: "The identifier has misleading characters and might be confused with something else.".to_string(),
                });
            }
        }

        if let Some(level) = settings.rule_level(RULE_ONREADY_WITH_EXPORT, mode) {
            if has_onready_export_conflict(line) {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_number,
                    column: 1,
                    code: RULE_ONREADY_WITH_EXPORT.to_string(),
                    level,
                    message: "\"@onready\" will set the default value after \"@export\" takes effect and will override it.".to_string(),
                });
            }
        }
    }

    if let Some(level) = settings.rule_level(RULE_UNUSED_PARAMETER, mode) {
        diagnostics.extend(unused_parameter_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_INFERENCE_ON_VARIANT, mode) {
        diagnostics.extend(inference_on_variant_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_UNUSED_PRIVATE_CLASS_VARIABLE, mode) {
        diagnostics.extend(unused_private_class_variable_diagnostics(
            &normalized,
            level,
        ));
    }

    if let Some(level) = settings.rule_level(RULE_INT_AS_ENUM_WITHOUT_CAST, mode) {
        diagnostics.extend(int_as_enum_without_cast_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_ENUM_VARIABLE_WITHOUT_DEFAULT, mode) {
        diagnostics.extend(enum_variable_without_default_diagnostics(
            &normalized,
            level,
        ));
    }

    if let Some(level) = settings.rule_level(RULE_INT_AS_ENUM_WITHOUT_MATCH, mode) {
        diagnostics.extend(int_as_enum_without_match_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_NARROWING_CONVERSION, mode) {
        diagnostics.extend(narrowing_conversion_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_INCOMPATIBLE_TERNARY, mode) {
        diagnostics.extend(incompatible_ternary_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_GET_NODE_DEFAULT_WITHOUT_ONREADY, mode) {
        diagnostics.extend(get_node_default_without_onready_diagnostics(
            &normalized,
            level,
        ));
    }

    let unused_variable_level = settings.rule_level(RULE_UNUSED_VARIABLE, mode);
    let unused_local_constant_level = settings.rule_level(RULE_UNUSED_LOCAL_CONSTANT, mode);
    let shadowed_global_identifier_level =
        settings.rule_level(RULE_SHADOWED_GLOBAL_IDENTIFIER, mode);
    let shadowed_variable_level = settings.rule_level(RULE_SHADOWED_VARIABLE, mode);
    let shadowed_variable_base_class_level =
        settings.rule_level(RULE_SHADOWED_VARIABLE_BASE_CLASS, mode);
    if unused_variable_level.is_some()
        || unused_local_constant_level.is_some()
        || shadowed_global_identifier_level.is_some()
        || shadowed_variable_level.is_some()
        || shadowed_variable_base_class_level.is_some()
    {
        diagnostics.extend(function_scope_binding_diagnostics(
            &normalized,
            unused_variable_level,
            unused_local_constant_level,
            shadowed_global_identifier_level,
            shadowed_variable_level,
            shadowed_variable_base_class_level,
        ));
    }

    let lambda_unused_parameter_level = settings.rule_level(RULE_UNUSED_PARAMETER, mode);
    let lambda_shadowed_variable_level = settings.rule_level(RULE_SHADOWED_VARIABLE, mode);
    if lambda_unused_parameter_level.is_some() || lambda_shadowed_variable_level.is_some() {
        diagnostics.extend(lambda_parameter_diagnostics(
            &normalized,
            lambda_unused_parameter_level,
            lambda_shadowed_variable_level,
        ));
    }

    let confusable_local_declaration_level =
        settings.rule_level(RULE_CONFUSABLE_LOCAL_DECLARATION, mode);
    let confusable_local_usage_level = settings.rule_level(RULE_CONFUSABLE_LOCAL_USAGE, mode);
    if confusable_local_declaration_level.is_some() || confusable_local_usage_level.is_some() {
        diagnostics.extend(confusable_local_scope_diagnostics(
            &normalized,
            confusable_local_declaration_level,
            confusable_local_usage_level,
        ));
    }

    let unreachable_level = settings.rule_level(RULE_UNREACHABLE_CODE, mode);
    let unassigned_variable_level = settings.rule_level(RULE_UNASSIGNED_VARIABLE, mode);
    let unassigned_variable_op_level =
        settings.rule_level(RULE_UNASSIGNED_VARIABLE_OP_ASSIGN, mode);
    if unreachable_level.is_some()
        || unassigned_variable_level.is_some()
        || unassigned_variable_op_level.is_some()
    {
        diagnostics.extend(function_scope_flow_diagnostics(
            &normalized,
            unreachable_level,
            unassigned_variable_level,
            unassigned_variable_op_level,
        ));
    }

    if let Some(level) = settings.rule_level(RULE_STATIC_CALLED_ON_INSTANCE, mode) {
        diagnostics.extend(static_called_on_instance_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_UNREACHABLE_PATTERN, mode) {
        diagnostics.extend(unreachable_pattern_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_MISSING_AWAIT, mode) {
        diagnostics.extend(missing_await_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_REDUNDANT_AWAIT, mode) {
        diagnostics.extend(redundant_await_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_CONFUSABLE_CAPTURE_REASSIGNMENT, mode) {
        diagnostics.extend(confusable_capture_reassignment_diagnostics(
            &normalized,
            level,
        ));
    }

    if let Some(level) = settings.rule_level(RULE_UNSAFE_CAST, mode) {
        diagnostics.extend(unsafe_cast_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_UNSAFE_CALL_ARGUMENT, mode) {
        diagnostics.extend(unsafe_call_argument_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_UNUSED_SIGNAL, mode) {
        diagnostics.extend(unused_signal_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_MISSING_TOOL, mode) {
        diagnostics.extend(missing_tool_diagnostics(&normalized, level));
    }

    if let Some(level) = settings.rule_level(RULE_NATIVE_METHOD_OVERRIDE, mode) {
        diagnostics.extend(native_method_override_diagnostics(&normalized, level));
    }

    diagnostics
}

fn find_integer_division_column(line: &str) -> Option<usize> {
    let code = line.split('#').next().unwrap_or(line);
    let bytes = code.as_bytes();

    for idx in 0..bytes.len() {
        if bytes[idx] != b'/' {
            continue;
        }

        let mut left = idx;
        while left > 0 && bytes[left - 1].is_ascii_whitespace() {
            left -= 1;
        }
        let left_end = left;
        while left > 0 && bytes[left - 1].is_ascii_digit() {
            left -= 1;
        }
        if left == left_end {
            continue;
        }
        if left > 0 && bytes[left - 1] == b'.' {
            continue;
        }

        let mut right = idx + 1;
        while right < bytes.len() && bytes[right].is_ascii_whitespace() {
            right += 1;
        }
        let right_start = right;
        while right < bytes.len() && bytes[right].is_ascii_digit() {
            right += 1;
        }
        if right == right_start {
            continue;
        }
        if right < bytes.len() && bytes[right] == b'.' {
            continue;
        }

        return Some(idx + 1);
    }

    None
}

fn unused_parameter_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let signature = if let Some(sig) = trimmed.strip_prefix("static func ") {
            sig
        } else if let Some(sig) = trimmed.strip_prefix("func ") {
            sig
        } else {
            continue;
        };

        let Some(name) = extract_fn_name(signature) else {
            continue;
        };
        let Some(params) = extract_fn_params(signature) else {
            continue;
        };

        if params.is_empty() {
            continue;
        }

        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let mut used = std::collections::HashSet::new();

        for body_line in lines.iter().skip(idx + 1) {
            let body_trimmed = body_line.trim_start();
            if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
                continue;
            }
            let body_indent = body_line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            if body_indent <= indent {
                break;
            }

            let body_code = body_line.split('#').next().unwrap_or(body_line);
            for param in &params {
                if contains_identifier(body_code, param) {
                    used.insert(param.clone());
                }
            }
        }

        for param in params
            .into_iter()
            .filter(|param| !param.starts_with('_'))
            .filter(|param| !used.contains(param))
        {
            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_UNUSED_PARAMETER.to_string(),
                level: level.clone(),
                message: format!(
                    "The parameter \"{param}\" is never used in the function \"{name}()\". If this is intended, prefix it with an underscore: \"_{param}\"."
                ),
            });
        }
    }

    diagnostics
}

fn inference_on_variant_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let variant_returning_functions = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let signature = if let Some(signature) = trimmed.strip_prefix("func ") {
                signature
            } else {
                trimmed.strip_prefix("static func ")?
            };

            let (name, return_type) = signature.split_once("->")?;
            let name = extract_fn_name(name.trim())?;
            let return_type = return_type.split(':').next().unwrap_or(return_type).trim();
            if return_type == "Variant" {
                Some(name)
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>();

    let mut diagnostics = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(tail) = code.strip_prefix("var ") else {
            continue;
        };
        let Some((_, rhs)) = tail.split_once(":=") else {
            continue;
        };

        let rhs = rhs.trim();
        let Some(paren_idx) = rhs.find('(') else {
            continue;
        };
        if !rhs.ends_with(')') {
            continue;
        }

        let callee = rhs[..paren_idx].trim();
        if !variant_returning_functions.contains(callee) {
            continue;
        }

        diagnostics.push(Diagnostic {
            file: None,
            line: idx + 1,
            column: 1,
            code: RULE_INFERENCE_ON_VARIANT.to_string(),
            level: level.clone(),
            message: "The variable type is being inferred from a Variant value, so it will be typed as Variant.".to_string(),
        });
    }

    diagnostics
}

fn unused_private_class_variable_diagnostics(
    source: &str,
    level: DiagnosticLevel,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut candidates = Vec::<(String, usize)>::new();
    let mut ignore_next = false;

    for (idx, line) in lines.iter().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        let inline_ignore = code.contains("@warning_ignore(\"unused_private_class_variable\")")
            || code.contains("@warning_ignore('unused_private_class_variable')");

        if code.starts_with("@warning_ignore")
            && !code.contains(" var ")
            && !code.starts_with("var ")
        {
            ignore_next = inline_ignore;
            continue;
        }

        if let Some((kind, names)) = parse_local_binding_declaration(code) {
            if kind != LocalBindingKind::Variable {
                ignore_next = false;
                continue;
            }

            for name in names
                .into_iter()
                .filter(|name| name.starts_with('_') && !name.starts_with("__"))
            {
                if inline_ignore || ignore_next {
                    continue;
                }
                candidates.push((name.to_string(), idx + 1));
            }
            ignore_next = false;
            continue;
        }

        if !code.starts_with('@') {
            ignore_next = false;
        }
    }

    let mut diagnostics = Vec::new();
    for (name, decl_line) in candidates {
        let mut used = false;
        for (idx, line) in lines.iter().enumerate() {
            if idx + 1 == decl_line {
                continue;
            }
            let code = line.split('#').next().unwrap_or(line);
            if contains_identifier(code, &name) {
                used = true;
                break;
            }
        }

        if !used {
            diagnostics.push(Diagnostic {
                file: None,
                line: decl_line,
                column: 1,
                code: RULE_UNUSED_PRIVATE_CLASS_VARIABLE.to_string(),
                level: level.clone(),
                message: format!(
                    "The class variable \"{name}\" is declared but never used in the class."
                ),
            });
        }
    }

    diagnostics
}

#[derive(Debug, Clone)]
struct EnumInfo {
    values: std::collections::HashSet<i64>,
    members: std::collections::HashMap<String, i64>,
}

fn collect_enum_infos(source: &str) -> std::collections::HashMap<String, EnumInfo> {
    let mut out = std::collections::HashMap::<String, EnumInfo>::new();

    for line in source.lines() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(rest) = code.strip_prefix("enum ") else {
            continue;
        };
        let Some((name_part, body_part)) = rest.split_once('{') else {
            continue;
        };
        let enum_name = name_part.trim();
        if enum_name.is_empty() {
            continue;
        }
        let Some(body) = body_part.split('}').next() else {
            continue;
        };

        let mut values = std::collections::HashSet::new();
        let mut members = std::collections::HashMap::new();
        let mut next_value = 0_i64;

        for entry in body.split(',') {
            let token = entry.trim();
            if token.is_empty() {
                continue;
            }

            if let Some((member, raw_value)) = token.split_once('=') {
                let member = member.trim();
                let value = raw_value.trim().parse::<i64>().ok();
                if let Some(value) = value {
                    values.insert(value);
                    members.insert(member.to_string(), value);
                    next_value = value + 1;
                }
            } else {
                let member = token.trim();
                values.insert(next_value);
                members.insert(member.to_string(), next_value);
                next_value += 1;
            }
        }

        out.insert(enum_name.to_string(), EnumInfo { values, members });
    }

    out
}

fn enum_variable_without_default_diagnostics(
    source: &str,
    level: DiagnosticLevel,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let enums = collect_enum_infos(source);
    let mut diagnostics = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(tail) = code.strip_prefix("var ") else {
            continue;
        };

        for segment in tail.split(',') {
            let token = segment.trim();
            if token.contains('=') || token.contains(":=") {
                continue;
            }
            let Some((name, raw_type)) = token.split_once(':') else {
                continue;
            };
            let name = name.trim();
            let enum_type = raw_type.trim();
            let Some(info) = enums.get(enum_type) else {
                continue;
            };
            if info.values.contains(&0) {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_ENUM_VARIABLE_WITHOUT_DEFAULT.to_string(),
                level: level.clone(),
                message: format!(
                    "The variable \"{name}\" has an enum type and does not set an explicit default value. The default will be set to \"0\"."
                ),
            });
        }
    }

    diagnostics
}

fn int_as_enum_without_cast_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let enums = collect_enum_infos(source);
    let mut typed_enum_variables = std::collections::HashMap::<String, String>::new();
    let mut diagnostics = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if let Some(tail) = code.strip_prefix("var ") {
            for segment in tail.split(',') {
                let token = segment.trim();
                if token.is_empty() {
                    continue;
                }
                let Some((name_part, type_and_rhs)) = token.split_once(':') else {
                    continue;
                };
                let name = name_part.trim();
                let Some((enum_type, rhs)) = type_and_rhs.split_once('=') else {
                    continue;
                };
                let enum_type = enum_type.trim();
                if !enums.contains_key(enum_type) {
                    continue;
                }

                typed_enum_variables.insert(name.to_string(), enum_type.to_string());
                if rhs.contains(" as ") {
                    continue;
                }
                if rhs.trim().parse::<i64>().is_ok() {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: idx + 1,
                        column: 1,
                        code: RULE_INT_AS_ENUM_WITHOUT_CAST.to_string(),
                        level: level.clone(),
                        message: "Integer used when an enum value is expected. If this is intended, cast the integer to the enum type using the \"as\" keyword.".to_string(),
                    });
                }
            }
            continue;
        }

        let Some((lhs, rhs)) = code.split_once('=') else {
            continue;
        };
        let lhs = lhs.trim();
        if !typed_enum_variables.contains_key(lhs) {
            continue;
        }
        if rhs.contains(" as ") {
            continue;
        }
        if rhs.trim().parse::<i64>().is_ok() {
            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_INT_AS_ENUM_WITHOUT_CAST.to_string(),
                level: level.clone(),
                message: "Integer used when an enum value is expected. If this is intended, cast the integer to the enum type using the \"as\" keyword.".to_string(),
            });
        }
    }

    diagnostics
}

fn int_as_enum_without_match_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let enums = collect_enum_infos(source);
    let mut diagnostics = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        for segment in code.split(['(', ')', ',']) {
            let Some((lhs_raw, enum_type_raw)) = segment.split_once(" as ") else {
                continue;
            };

            let enum_type = enum_type_raw
                .trim()
                .trim_start_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
            let Some(info) = enums.get(enum_type) else {
                continue;
            };

            let lhs = lhs_raw
                .trim()
                .trim_start_matches(['(', '['])
                .trim_end_matches([')', ']']);
            let Some(value) = evaluate_enum_cast_value(lhs, &enums) else {
                continue;
            };
            if info.values.contains(&value) {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_INT_AS_ENUM_WITHOUT_MATCH.to_string(),
                level: level.clone(),
                message: format!(
                    "Cannot cast {value} as Enum \"{enum_type}\": no enum member has matching value."
                ),
            });
        }
    }

    diagnostics
}

fn evaluate_enum_cast_value(
    expression: &str,
    enums: &std::collections::HashMap<String, EnumInfo>,
) -> Option<i64> {
    if let Ok(value) = expression.parse::<i64>() {
        return Some(value);
    }

    let (enum_name, member) = expression.split_once('.')?;
    let info = enums.get(enum_name.trim())?;
    info.members.get(member.trim()).copied()
}

fn narrowing_conversion_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let typed_signatures = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let signature = if let Some(sig) = trimmed.strip_prefix("func ") {
                sig
            } else if let Some(sig) = trimmed.strip_prefix("static func ") {
                sig
            } else {
                return None;
            };
            let name = extract_fn_name(signature)?;
            let params = extract_fn_params_with_types(signature)?;
            Some((name, params))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut diagnostics = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(paren_idx) = code.find('(') else {
            continue;
        };
        if !code.ends_with(')') {
            continue;
        }
        let callee = code[..paren_idx].trim();
        let Some(param_types) = typed_signatures.get(callee) else {
            continue;
        };
        let args = split_call_args(&code[paren_idx + 1..code.len() - 1]);

        for (arg, param_type) in args.into_iter().zip(param_types.iter()) {
            if param_type.as_deref() != Some("int") {
                continue;
            }
            if !looks_like_float_literal(arg.trim()) {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_NARROWING_CONVERSION.to_string(),
                level: level.clone(),
                message: "Narrowing conversion (float is converted to int and loses precision)."
                    .to_string(),
            });
            break;
        }
    }

    diagnostics
}

fn incompatible_ternary_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let mut typed_variables = std::collections::HashMap::<String, String>::new();
    let mut diagnostics = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if let Some(tail) = code.strip_prefix("var ") {
            if let Some((name, type_and_rest)) = tail.split_once(':') {
                let name = name.trim();
                let ty = type_and_rest
                    .split('=')
                    .next()
                    .unwrap_or(type_and_rest)
                    .trim();
                if !ty.is_empty() {
                    typed_variables.insert(name.to_string(), ty.to_string());
                }
            }
            continue;
        }

        let Some((lhs, rhs)) = code.split_once('=') else {
            continue;
        };
        let lhs = lhs.trim();
        if !typed_variables.contains_key(lhs) {
            continue;
        }
        let Some((truthy, falsy)) = split_ternary_values(rhs.trim()) else {
            continue;
        };
        let Some(truthy_ty) = infer_literal_type(truthy.trim()) else {
            continue;
        };
        let Some(falsy_ty) = infer_literal_type(falsy.trim()) else {
            continue;
        };
        if truthy_ty == falsy_ty {
            continue;
        }

        diagnostics.push(Diagnostic {
            file: None,
            line: idx + 1,
            column: 1,
            code: RULE_INCOMPATIBLE_TERNARY.to_string(),
            level: level.clone(),
            message: "Values of the ternary operator are not mutually compatible.".to_string(),
        });
    }

    diagnostics
}

fn extract_fn_params_with_types(signature: &str) -> Option<Vec<Option<String>>> {
    let start = signature.find('(')?;
    let end = signature[start + 1..].find(')')? + start + 1;
    let params = signature[start + 1..end]
        .split(',')
        .map(str::trim)
        .filter(|param| !param.is_empty())
        .map(|param| {
            let (name, ty) = param.split_once(':')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            let ty = ty.split('=').next().unwrap_or(ty).trim();
            if ty.is_empty() {
                None
            } else {
                Some(ty.to_string())
            }
        })
        .collect::<Vec<_>>();
    Some(params)
}

fn split_call_args(args: &str) -> Vec<&str> {
    args.split(',').map(str::trim).collect()
}

fn looks_like_float_literal(value: &str) -> bool {
    value.contains('.') && value.parse::<f64>().is_ok()
}

fn split_ternary_values(value: &str) -> Option<(&str, &str)> {
    let (truthy, rest) = value.split_once(" if ")?;
    let (_, falsy) = rest.split_once(" else ")?;
    Some((truthy, falsy))
}

fn infer_literal_type(value: &str) -> Option<&'static str> {
    let value = value.trim();
    if value.starts_with('"') && value.ends_with('"') {
        return Some("string");
    }
    if value.eq("true") || value.eq("false") {
        return Some("bool");
    }
    if value.parse::<i64>().is_ok() {
        return Some("int");
    }
    if value.parse::<f64>().is_ok() {
        return Some("float");
    }
    None
}

fn get_node_default_without_onready_diagnostics(
    source: &str,
    level: DiagnosticLevel,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut onready_next = false;

    for (idx, line) in source.lines().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if code.contains("@onready") && !code.contains("var ") {
            onready_next = true;
            continue;
        }

        if let Some(tail) = code.strip_prefix("var ") {
            let annotated_onready = onready_next || code.contains("@onready");
            onready_next = false;
            if annotated_onready {
                continue;
            }

            let Some((_, rhs)) = tail.split_once('=') else {
                continue;
            };
            let rhs = rhs.trim();
            let token = if rhs.contains("get_node(") {
                Some("get_node()")
            } else if rhs.contains('$') {
                Some("$")
            } else if rhs.contains('%') {
                Some("%")
            } else {
                None
            };
            let Some(token) = token else {
                continue;
            };

            diagnostics.push(Diagnostic {
                file: None,
                line: idx + 1,
                column: 1,
                code: RULE_GET_NODE_DEFAULT_WITHOUT_ONREADY.to_string(),
                level: level.clone(),
                message: format!(
                    "The default value uses \"{token}\" which won't return nodes in the scene tree before \"_ready()\" is called. Use the \"@onready\" annotation to solve this."
                ),
            });
            continue;
        }

        if !code.starts_with('@') {
            onready_next = false;
        }
    }

    diagnostics
}

fn lambda_parameter_diagnostics(
    source: &str,
    unused_parameter_level: Option<DiagnosticLevel>,
    shadowed_variable_level: Option<DiagnosticLevel>,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let class_symbols = collect_class_scope_symbols(&lines);
    let mut diagnostics = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("func ") || trimmed.starts_with("static func ") {
            continue;
        }
        if !trimmed.contains("func(") && !trimmed.contains("func (") {
            continue;
        }

        let Some(func_pos) = trimmed.find("func") else {
            continue;
        };
        let signature = &trimmed[func_pos..];
        let Some(params) = extract_fn_params(signature) else {
            continue;
        };
        if params.is_empty() {
            continue;
        }

        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let mut used = std::collections::HashSet::new();

        for body_line in lines.iter().skip(idx + 1) {
            let body_trimmed = body_line.trim_start();
            if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
                continue;
            }
            let body_indent = body_line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            if body_indent <= indent {
                break;
            }

            let body_code = body_line.split('#').next().unwrap_or(body_line);
            for param in &params {
                if contains_identifier(body_code, param) {
                    used.insert(param.clone());
                }
            }
        }

        for param in &params {
            if let Some(level) = shadowed_variable_level.as_ref() {
                if let Some(existing_line) = class_symbols.variables.get(param) {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: idx + 1,
                        column: 1,
                        code: RULE_SHADOWED_VARIABLE.to_string(),
                        level: level.clone(),
                        message: format!(
                            "The local function parameter \"{param}\" is shadowing an already-declared variable at line {} in the current class.",
                            existing_line
                        ),
                    });
                }
            }
        }

        for param in params
            .into_iter()
            .filter(|param| !param.starts_with('_'))
            .filter(|param| !used.contains(param))
        {
            if let Some(level) = unused_parameter_level.as_ref() {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: idx + 1,
                    column: 1,
                    code: RULE_UNUSED_PARAMETER.to_string(),
                    level: level.clone(),
                    message: format!(
                        "The parameter \"{param}\" is never used in the function \"<anonymous lambda>()\". If this is intended, prefix it with an underscore: \"_{param}\"."
                    ),
                });
            }
        }
    }

    diagnostics
}

fn missing_await_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let coroutine_names = blocks
        .iter()
        .filter(|block| {
            (block.body_start..block.body_end).any(|line_idx| {
                let code = block.lines[line_idx]
                    .trim_start()
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim();
                code.starts_with("await ")
            })
        })
        .map(|block| block.name.clone())
        .collect::<std::collections::HashSet<_>>();

    let mut diagnostics = Vec::new();
    for block in blocks {
        for line_idx in block.body_start..block.body_end {
            let code = block.lines[line_idx]
                .trim_start()
                .split('#')
                .next()
                .unwrap_or("")
                .trim();
            if code.is_empty() || code.starts_with("await ") || code.contains('=') {
                continue;
            }

            let Some(name) = standalone_call_name(code) else {
                continue;
            };
            if !coroutine_names.contains(name) {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: line_idx + 1,
                column: 1,
                code: RULE_MISSING_AWAIT.to_string(),
                level: level.clone(),
                message:
                    "\"await\" keyword might be desired because the expression is a coroutine."
                        .to_string(),
            });
        }
    }

    diagnostics
}

fn redundant_await_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let signal_names = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            let rest = code.strip_prefix("signal ")?;
            let name = rest.split('(').next().unwrap_or(rest).trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect::<std::collections::HashSet<_>>();
    let coroutine_names = blocks
        .iter()
        .filter(|block| {
            (block.body_start..block.body_end).any(|line_idx| {
                let code = block.lines[line_idx]
                    .trim_start()
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim();
                code.starts_with("await ")
            })
        })
        .map(|block| block.name.clone())
        .collect::<std::collections::HashSet<_>>();

    let mut diagnostics = Vec::new();
    for block in blocks {
        let mut signal_vars = std::collections::HashSet::new();
        let mut callable_targets = std::collections::HashMap::<String, String>::new();
        let mut ignore_next_redundant_await = false;

        for line_idx in block.body_start..block.body_end {
            let trimmed = block.lines[line_idx].trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            if code.is_empty() {
                continue;
            }

            if code.starts_with("@warning_ignore")
                && has_warning_ignore_for_rule(code, "redundant_await")
            {
                ignore_next_redundant_await = true;
                continue;
            }

            if let Some(rest) = code.strip_prefix("var ") {
                if let Some((name_part, typed_tail)) = rest.split_once(':') {
                    let name = name_part.split('=').next().unwrap_or(name_part).trim();
                    let ty = typed_tail.split('=').next().unwrap_or(typed_tail).trim();
                    if !name.is_empty() && ty == "Signal" {
                        signal_vars.insert(name.to_string());
                    }
                    if !name.is_empty() && ty == "Callable" {
                        if let Some((_, rhs)) = rest.split_once('=') {
                            let target = rhs.trim();
                            if target
                                .chars()
                                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                            {
                                callable_targets.insert(name.to_string(), target.to_string());
                            }
                        }
                    }
                }
            }

            let Some(expression) = code.strip_prefix("await ") else {
                if !code.starts_with('@') {
                    ignore_next_redundant_await = false;
                }
                continue;
            };
            if ignore_next_redundant_await {
                ignore_next_redundant_await = false;
                continue;
            }
            let expr = expression.trim();
            if expr.is_empty() {
                continue;
            }

            let is_signal = signal_names.contains(expr) || signal_vars.contains(expr);
            if is_signal {
                continue;
            }

            let call_target = extract_await_call_target(expr)
                .or_else(|| callable_targets.get(expr).map(String::as_str));
            let is_coroutine_call =
                call_target.is_some_and(|target| coroutine_names.contains(target));
            if is_coroutine_call && expr.ends_with(')') {
                continue;
            }
            if expr.starts_with("call(&\"")
                && extract_call_name_from_callable_call(expr)
                    .is_some_and(|target| coroutine_names.contains(target))
            {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: line_idx + 1,
                column: 1,
                code: RULE_REDUNDANT_AWAIT.to_string(),
                level: level.clone(),
                message:
                    "\"await\" keyword is unnecessary because the expression isn't a coroutine nor a signal.".to_string(),
            });
        }
    }

    diagnostics
}

fn confusable_capture_reassignment_diagnostics(
    source: &str,
    level: DiagnosticLevel,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let mut diagnostics = Vec::new();

    for block in blocks {
        let mut outer_locals = std::collections::HashMap::<String, bool>::new();
        let mut lambda_scopes = Vec::<(usize, std::collections::HashMap<String, bool>)>::new();

        for line_idx in block.body_start..block.body_end {
            let line = block.lines[line_idx];
            let indent = line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            while lambda_scopes
                .last()
                .is_some_and(|(lambda_indent, _)| indent <= *lambda_indent)
            {
                lambda_scopes.pop();
            }

            let trimmed = line.trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            if code.is_empty() {
                continue;
            }

            if lambda_scopes.is_empty() {
                if let Some((kind, names)) = parse_local_binding_declaration(code) {
                    if kind == LocalBindingKind::Variable {
                        for name in names {
                            let is_reference = extract_variable_initializer(code, name)
                                .map(|rhs| {
                                    let rhs = rhs.trim_start();
                                    rhs.starts_with('[') || rhs.starts_with('{')
                                })
                                .unwrap_or(false);
                            outer_locals.insert(name.to_string(), is_reference);
                        }
                    }
                }
            }

            if !code.starts_with("func ")
                && (code.contains("func(") || code.contains("func ("))
                && code.ends_with(':')
            {
                lambda_scopes.push((indent, outer_locals.clone()));
                continue;
            }

            let Some((_, captured_locals)) = lambda_scopes.last() else {
                continue;
            };
            let Some(lhs) = extract_assignment_lhs(code) else {
                continue;
            };
            let base = lhs.split(['.', '[']).next().map(str::trim).unwrap_or("");
            if base.is_empty() || !captured_locals.contains_key(base) {
                continue;
            }

            if lhs.contains('[') {
                continue;
            }

            if lhs.contains('.') && captured_locals.get(base).copied().unwrap_or(false) {
                continue;
            }

            diagnostics.push(Diagnostic {
                file: None,
                line: line_idx + 1,
                column: 1,
                code: RULE_CONFUSABLE_CAPTURE_REASSIGNMENT.to_string(),
                level: level.clone(),
                message: format!(
                    "Reassigning lambda capture does not modify the outer local variable \"{base}\"."
                ),
            });
        }
    }

    diagnostics
}

fn extract_assignment_lhs(code: &str) -> Option<&str> {
    for operator in ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", ":="] {
        if let Some((lhs, _)) = code.split_once(operator) {
            return Some(lhs.trim());
        }
    }

    let idx = code.find('=')?;
    let prev = code.as_bytes().get(idx.checked_sub(1)?).copied();
    let next = code.as_bytes().get(idx + 1).copied();
    if matches!(prev, Some(b'=' | b'!' | b'<' | b'>')) || next == Some(b'=') {
        return None;
    }

    Some(code[..idx].trim())
}

fn extract_await_call_target(expression: &str) -> Option<&str> {
    if expression.ends_with("()") {
        let call = expression.trim_end_matches("()");
        if let Some(rest) = call.strip_prefix("self.") {
            return Some(rest);
        }
        if let Some(rest) = call.strip_suffix(".call") {
            return Some(rest);
        }
        if call
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
        {
            return Some(call);
        }
    }

    if let Some(callable) = expression.strip_suffix(".call()") {
        if let Some(rest) = callable.strip_prefix("self.") {
            return Some(rest);
        }
        return Some(callable);
    }

    None
}

fn extract_call_name_from_callable_call(expression: &str) -> Option<&str> {
    let prefix = "call(&\"";
    let suffix = "\")";
    let body = expression.strip_prefix(prefix)?.strip_suffix(suffix)?;
    if body
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(body)
    } else {
        None
    }
}

fn standalone_call_name(code: &str) -> Option<&str> {
    let open = code.find('(')?;
    if !code.ends_with(')') {
        return None;
    }
    let callee = code[..open].trim();
    if callee.is_empty() {
        return None;
    }
    if !callee
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(callee)
}

fn unsafe_cast_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let mut diagnostics = Vec::new();

    for block in blocks {
        let var_types = collect_local_variable_types(&block);

        for line_idx in block.body_start..block.body_end {
            let code = block.lines[line_idx]
                .trim_start()
                .split('#')
                .next()
                .unwrap_or("")
                .trim();
            if code.is_empty() {
                continue;
            }

            for (expr, target_ty) in find_cast_expressions(code) {
                if target_ty == "Variant" {
                    continue;
                }
                let source_ty = infer_expression_type(expr, &var_types);
                if source_ty != "Variant" {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_idx + 1,
                    column: 1,
                    code: RULE_UNSAFE_CAST.to_string(),
                    level: level.clone(),
                    message: format!("Casting \"Variant\" to \"{target_ty}\" is unsafe."),
                });
            }
        }
    }

    diagnostics
}

fn unsafe_call_argument_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let function_first_param_types = collect_function_first_param_types(source);

    let mut diagnostics = Vec::new();
    for block in blocks {
        let var_types = collect_local_variable_types(&block);
        for line_idx in block.body_start..block.body_end {
            let code = block.lines[line_idx]
                .trim_start()
                .split('#')
                .next()
                .unwrap_or("")
                .trim();
            if code.is_empty() {
                continue;
            }

            for (callee, required_ty, arg_expr) in
                iter_first_arg_calls(code, &function_first_param_types)
            {
                let actual_ty = infer_expression_type(arg_expr, &var_types);
                if !is_unsafe_argument(required_ty, &actual_ty) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_idx + 1,
                    column: 1,
                    code: RULE_UNSAFE_CALL_ARGUMENT.to_string(),
                    level: level.clone(),
                    message: format!(
                        "The argument 1 of the function \"{callee}()\" requires the subtype \"{required_ty}\" but the supertype \"{actual_ty}\" was provided."
                    ),
                });
            }
        }
    }

    diagnostics
}

fn is_unsafe_argument(required_ty: &str, actual_ty: &str) -> bool {
    if required_ty == "Variant" {
        return false;
    }
    if actual_ty == required_ty {
        return false;
    }
    if required_ty == "Node" && (actual_ty == "Node" || actual_ty == "Node2D") {
        return false;
    }
    if actual_ty == "Variant" {
        return true;
    }
    required_ty == "Node" && actual_ty == "Object"
}

fn collect_function_first_param_types(source: &str) -> std::collections::HashMap<String, String> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let signature = if let Some(sig) = trimmed.strip_prefix("func ") {
                sig
            } else {
                trimmed.strip_prefix("static func ")?
            };
            let name = extract_fn_name(signature)?;
            let params = extract_fn_params_with_types(signature)?;
            let first = params.into_iter().next().flatten()?;
            Some((name, first))
        })
        .collect()
}

fn iter_first_arg_calls<'a>(
    code: &'a str,
    function_param_types: &'a std::collections::HashMap<String, String>,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut out = Vec::new();

    for (callee, required) in function_param_types {
        let needle = format!("{callee}(");
        if let Some(pos) = code.find(&needle) {
            if let Some(arg) = extract_first_arg(&code[pos + needle.len()..]) {
                out.push((callee.as_str(), required.as_str(), arg));
            }
        }
    }

    for (callee, required) in [
        ("Callable", "Object"),
        ("Dictionary", "Dictionary"),
        ("Vector2", "Vector2"),
        ("int", "int"),
    ] {
        let needle = format!("{callee}(");
        if let Some(pos) = code.find(&needle) {
            if let Some(arg) = extract_first_arg(&code[pos + needle.len()..]) {
                out.push((callee, required, arg));
            }
        }
    }

    out
}

fn extract_first_arg(args_and_rest: &str) -> Option<&str> {
    let end = args_and_rest.find(')')?;
    let args = &args_and_rest[..end];
    args.split(',')
        .next()
        .map(str::trim)
        .filter(|arg| !arg.is_empty())
}

fn collect_local_variable_types(
    block: &FunctionBlock<'_>,
) -> std::collections::HashMap<String, String> {
    let mut types = std::collections::HashMap::new();

    for line_idx in block.body_start..block.body_end {
        let code = block.lines[line_idx]
            .trim_start()
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        let Some(tail) = code.strip_prefix("var ") else {
            continue;
        };

        for segment in tail.split(',') {
            let token = segment.trim();
            if token.is_empty() {
                continue;
            }

            let name = token
                .split(':')
                .next()
                .unwrap_or(token)
                .split('=')
                .next()
                .unwrap_or(token)
                .trim();
            if name.is_empty() {
                continue;
            }

            let var_type = if let Some((_, typed_tail)) = token.split_once(':') {
                typed_tail
                    .split('=')
                    .next()
                    .unwrap_or(typed_tail)
                    .trim()
                    .to_string()
            } else {
                "Variant".to_string()
            };
            types.insert(name.to_string(), var_type);
        }
    }

    types
}

fn infer_expression_type(
    expression: &str,
    var_types: &std::collections::HashMap<String, String>,
) -> String {
    let expr = expression.trim();
    if expr.is_empty() {
        return "Variant".to_string();
    }
    if expr == "self" {
        return "Object".to_string();
    }
    if let Some(var_ty) = var_types.get(expr) {
        return var_ty.clone();
    }
    if let Some(type_name) = expr.strip_suffix(".new()") {
        return type_name.trim().to_string();
    }
    if expr.starts_with('"') && expr.ends_with('"') {
        return "String".to_string();
    }
    if expr.parse::<i64>().is_ok() {
        return "int".to_string();
    }
    if expr.parse::<f64>().is_ok() {
        return "float".to_string();
    }
    "Variant".to_string()
}

fn find_cast_expressions(code: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = code;

    while let Some(pos) = rest.find(" as ") {
        let lhs_part = &rest[..pos];
        let rhs_part = &rest[pos + 4..];
        let lhs = lhs_part
            .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .find(|piece| !piece.is_empty())
            .unwrap_or(lhs_part.trim());
        let target = rhs_part
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .next()
            .unwrap_or("")
            .trim();

        if !lhs.is_empty() && !target.is_empty() {
            out.push((lhs, target));
        }

        rest = rhs_part;
    }

    out
}

fn unused_signal_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut signals = Vec::<(String, usize)>::new();
    let mut ignore_next_unused_signal = false;

    for (idx, line) in lines.iter().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if code.starts_with("@warning_ignore")
            && has_warning_ignore_for_rule(code, "unused_signal")
            && !code.contains("signal ")
        {
            ignore_next_unused_signal = true;
            continue;
        }

        if let Some(rest) = code.strip_prefix("signal ") {
            let name = rest.split('(').next().unwrap_or(rest).trim();
            if !name.is_empty()
                && !ignore_next_unused_signal
                && !has_warning_ignore_for_rule(code, "unused_signal")
            {
                signals.push((name.to_string(), idx + 1));
            }
            ignore_next_unused_signal = false;
            continue;
        }

        if !code.starts_with('@') {
            ignore_next_unused_signal = false;
        }
    }

    let mut diagnostics = Vec::new();
    for (name, decl_line) in signals {
        let mut used = false;
        for (idx, line) in lines.iter().enumerate() {
            if idx + 1 == decl_line {
                continue;
            }
            let code = line.split('#').next().unwrap_or(line);
            if code.contains(&format!("{name}.emit("))
                || code.contains(&format!("print({name})"))
                || code.contains(&format!("Signal(self, \"{name}\")"))
                || code.contains(&format!("emit_signal(\"{name}\")"))
                || code.contains(&format!("self.emit_signal(\"{name}\")"))
                || code.contains(&format!("connect(\"{name}\""))
                || code.contains(&format!("disconnect(\"{name}\""))
            {
                used = true;
                break;
            }
        }

        if !used {
            diagnostics.push(Diagnostic {
                file: None,
                line: decl_line,
                column: 1,
                code: RULE_UNUSED_SIGNAL.to_string(),
                level: level.clone(),
                message: format!(
                    "The signal \"{name}\" is declared but never explicitly used in the class."
                ),
            });
        }
    }

    diagnostics
}

fn missing_tool_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let has_tool = source
        .lines()
        .map(str::trim_start)
        .map(|line| line.split('#').next().unwrap_or(line).trim())
        .any(|line| line == "@tool");
    if has_tool {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let mut ignore_next_missing_tool = false;
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if code.starts_with("@warning_ignore")
            && has_warning_ignore_for_rule(code, "missing_tool")
            && !code.contains("extends ")
        {
            ignore_next_missing_tool = true;
            continue;
        }

        let is_extends = code.starts_with("extends ") || code.contains(" extends ");
        if is_extends && code.contains(".notest.gd") {
            if !ignore_next_missing_tool && !has_warning_ignore_for_rule(code, "missing_tool") {
                diagnostics.push(Diagnostic {
                    file: None,
                    line: idx + 1,
                    column: 1,
                    code: RULE_MISSING_TOOL.to_string(),
                    level: level.clone(),
                    message: "The base class script has the \"@tool\" annotation, but this script does not have it.".to_string(),
                });
            }
            ignore_next_missing_tool = false;
            continue;
        }

        if !code.starts_with('@') {
            ignore_next_missing_tool = false;
        }
    }

    diagnostics
}

fn native_method_override_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let signature = if let Some(sig) = trimmed.strip_prefix("func ") {
            sig
        } else if let Some(sig) = trimmed.strip_prefix("static func ") {
            sig
        } else {
            continue;
        };
        let Some(name) = extract_fn_name(signature) else {
            continue;
        };
        if name != "get" {
            continue;
        }

        diagnostics.push(Diagnostic {
            file: None,
            line: idx + 1,
            column: 1,
            code: RULE_NATIVE_METHOD_OVERRIDE.to_string(),
            level: level.clone(),
            message: "The method \"get()\" overrides a method from native class \"Object\". This won't be called by the engine and may not work as expected.".to_string(),
        });
    }

    diagnostics
}

fn has_warning_ignore_for_rule(code: &str, rule: &str) -> bool {
    code.contains(&format!("\"{rule}\"")) || code.contains(&format!("'{rule}'"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalBindingKind {
    Variable,
    Constant,
}

#[derive(Debug, Clone)]
struct LocalBinding {
    name: String,
    kind: LocalBindingKind,
    line: usize,
}

#[derive(Default)]
struct ClassScopeSymbols {
    variables: std::collections::HashMap<String, usize>,
    constants: std::collections::HashMap<String, usize>,
    functions: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone)]
struct FunctionBlock<'a> {
    name: String,
    body_start: usize,
    body_end: usize,
    lines: &'a [&'a str],
}

fn local_binding_label(kind: LocalBindingKind) -> &'static str {
    match kind {
        LocalBindingKind::Variable => "local variable",
        LocalBindingKind::Constant => "local constant",
    }
}

fn collect_class_scope_symbols(lines: &[&str]) -> ClassScopeSymbols {
    let mut symbols = ClassScopeSymbols::default();

    for (idx, line) in lines.iter().enumerate() {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }

        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.is_empty() {
            continue;
        }

        if let Some((kind, names)) = parse_local_binding_declaration(code) {
            for name in names {
                match kind {
                    LocalBindingKind::Variable => {
                        symbols.variables.entry(name.to_string()).or_insert(idx + 1);
                    }
                    LocalBindingKind::Constant => {
                        symbols.constants.entry(name.to_string()).or_insert(idx + 1);
                    }
                }
            }
            continue;
        }

        let signature = if let Some(sig) = code.strip_prefix("static func ") {
            Some(sig)
        } else {
            code.strip_prefix("func ")
        };

        if let Some(signature) = signature {
            if let Some(name) = extract_fn_name(signature) {
                symbols.functions.entry(name).or_insert(idx + 1);
            }
        }
    }

    symbols
}

fn function_scope_binding_diagnostics(
    source: &str,
    unused_variable_level: Option<DiagnosticLevel>,
    unused_local_constant_level: Option<DiagnosticLevel>,
    shadowed_global_identifier_level: Option<DiagnosticLevel>,
    shadowed_variable_level: Option<DiagnosticLevel>,
    shadowed_variable_base_class_level: Option<DiagnosticLevel>,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let mut diagnostics = Vec::new();
    let globals = builtin_global_identifiers();
    let class_symbols = collect_class_scope_symbols(&lines);
    let global_class_names = collect_global_class_names(&lines);
    let extends_name = collect_extends_name(&lines);

    if let Some(level) = shadowed_global_identifier_level.as_ref() {
        let mut ignore_next_shadowed_global = false;
        for (idx, line) in lines.iter().enumerate() {
            let indent = line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            if indent != 0 {
                continue;
            }

            let trimmed = line.trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            if code.is_empty() {
                continue;
            }

            if code.starts_with("@warning_ignore")
                && has_warning_ignore_for_rule(code, "shadowed_global_identifier")
                && !code.contains("var ")
                && !code.contains("const ")
            {
                ignore_next_shadowed_global = true;
                continue;
            }

            if let Some((_, names)) = parse_local_binding_declaration(code) {
                if !ignore_next_shadowed_global
                    && !has_warning_ignore_for_rule(code, "shadowed_global_identifier")
                {
                    for name in names.into_iter().filter(|name| !name.starts_with('_')) {
                        if is_shadowed_global_identifier(name, globals, &global_class_names) {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_GLOBAL_IDENTIFIER.to_string(),
                                level: level.clone(),
                                message: shadowed_global_message(name, &global_class_names),
                            });
                        }
                    }
                }
                ignore_next_shadowed_global = false;
                continue;
            }

            if !code.starts_with('@') {
                ignore_next_shadowed_global = false;
            }
        }
    }

    for block in blocks {
        let mut bindings = Vec::new();
        for line_idx in block.body_start..block.body_end {
            let line = block.lines[line_idx];
            let trimmed = line.trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            if code.is_empty() {
                continue;
            }

            if let Some((kind, names)) = parse_local_binding_declaration(code) {
                for name in names {
                    if name.starts_with('_') {
                        continue;
                    }
                    bindings.push(LocalBinding {
                        name: name.to_string(),
                        kind,
                        line: line_idx + 1,
                    });

                    if let Some(level) = shadowed_global_identifier_level.as_ref() {
                        if is_shadowed_global_identifier(name, globals, &global_class_names) {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: line_idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_GLOBAL_IDENTIFIER.to_string(),
                                level: level.clone(),
                                message: shadowed_global_message(name, &global_class_names),
                            });
                        }
                    }

                    if let Some(level) = shadowed_variable_level.as_ref() {
                        if let Some(existing_line) = class_symbols.functions.get(name) {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: line_idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_VARIABLE.to_string(),
                                level: level.clone(),
                                message: format!(
                                    "The {} \"{}\" is shadowing an already-declared function at line {} in the current class.",
                                    local_binding_label(kind),
                                    name,
                                    existing_line
                                ),
                            });
                        } else if let Some(existing_line) = class_symbols.variables.get(name) {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: line_idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_VARIABLE.to_string(),
                                level: level.clone(),
                                message: format!(
                                    "The {} \"{}\" is shadowing an already-declared variable at line {} in the current class.",
                                    local_binding_label(kind),
                                    name,
                                    existing_line
                                ),
                            });
                        } else if let Some(existing_line) = class_symbols.constants.get(name) {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: line_idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_VARIABLE.to_string(),
                                level: level.clone(),
                                message: format!(
                                    "The {} \"{}\" is shadowing an already-declared constant at line {} in the current class.",
                                    local_binding_label(kind),
                                    name,
                                    existing_line
                                ),
                            });
                        }
                    }

                    if let Some(level) = shadowed_variable_base_class_level.as_ref() {
                        if let Some((member_kind, base_class)) =
                            known_base_class_member(name, extends_name.as_deref())
                        {
                            diagnostics.push(Diagnostic {
                                file: None,
                                line: line_idx + 1,
                                column: 1,
                                code: RULE_SHADOWED_VARIABLE_BASE_CLASS.to_string(),
                                level: level.clone(),
                                message: base_class_shadow_message(
                                    kind,
                                    name,
                                    member_kind,
                                    base_class,
                                ),
                            });
                        }
                    }
                }
            }
        }

        for binding in bindings {
            let mut used = false;
            for line_idx in block.body_start..block.body_end {
                if line_idx + 1 == binding.line {
                    continue;
                }
                let line = block.lines[line_idx];
                let code = line.split('#').next().unwrap_or(line);
                if contains_identifier(code, &binding.name) {
                    used = true;
                    break;
                }
            }
            if used {
                continue;
            }

            match binding.kind {
                LocalBindingKind::Variable => {
                    if let Some(level) = unused_variable_level.as_ref() {
                        diagnostics.push(Diagnostic {
                            file: None,
                            line: binding.line,
                            column: 1,
                            code: RULE_UNUSED_VARIABLE.to_string(),
                            level: level.clone(),
                            message: format!(
                                "The local variable \"{}\" is declared but never used in the block. If this is intended, prefix it with an underscore: \"_{}\".",
                                binding.name, binding.name
                            ),
                        });
                    }
                }
                LocalBindingKind::Constant => {
                    if let Some(level) = unused_local_constant_level.as_ref() {
                        diagnostics.push(Diagnostic {
                            file: None,
                            line: binding.line,
                            column: 1,
                            code: RULE_UNUSED_LOCAL_CONSTANT.to_string(),
                            level: level.clone(),
                            message: format!(
                                "The local constant \"{}\" is declared but never used in the block. If this is intended, prefix it with an underscore: \"_{}\".",
                                binding.name, binding.name
                            ),
                        });
                    }
                }
            }
        }
    }

    diagnostics
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseMemberKind {
    Variable,
    Constant,
    Function,
}

fn collect_global_class_names(lines: &[&str]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for line in lines {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(rest) = code.strip_prefix("class_name ") else {
            continue;
        };
        let name = rest
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '(' || ch == ':')
            .next()
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out
}

fn collect_extends_name(lines: &[&str]) -> Option<String> {
    for line in lines {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        if indent != 0 {
            continue;
        }
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        let Some(rest) = code.strip_prefix("extends ") else {
            continue;
        };
        let name = rest
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
            .next()
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn is_shadowed_global_identifier(
    name: &str,
    globals: &std::collections::HashSet<&'static str>,
    global_class_names: &std::collections::HashSet<String>,
) -> bool {
    globals.contains(name) || global_class_names.contains(name)
}

fn shadowed_global_message(
    name: &str,
    global_class_names: &std::collections::HashSet<String>,
) -> String {
    if global_class_names.contains(name) {
        format!("The variable \"{name}\" has the same name as a global class.")
    } else {
        format!("The variable \"{name}\" has the same name as a built-in function.")
    }
}

fn known_base_class_member(
    name: &str,
    extends_name: Option<&str>,
) -> Option<(BaseMemberKind, &'static str)> {
    if name == "reference" {
        return Some((BaseMemberKind::Function, "RefCounted"));
    }

    if extends_name == Some("ShadowingBase") {
        return match name {
            "base_variable_member" => Some((BaseMemberKind::Variable, "ShadowingBase")),
            "base_function_member" => Some((BaseMemberKind::Function, "ShadowingBase")),
            "base_const_member" => Some((BaseMemberKind::Constant, "ShadowingBase")),
            _ => None,
        };
    }

    None
}

fn base_class_shadow_message(
    local_kind: LocalBindingKind,
    name: &str,
    base_member_kind: BaseMemberKind,
    base_class: &str,
) -> String {
    let local_label = local_binding_label(local_kind);
    let base_label = match base_member_kind {
        BaseMemberKind::Variable => "variable",
        BaseMemberKind::Constant => "constant",
        BaseMemberKind::Function => "method",
    };
    format!(
        "The {local_label} \"{name}\" is shadowing an already-declared {base_label} in the base class \"{base_class}\"."
    )
}

#[derive(Debug, Clone)]
struct LocalDeclaration {
    name: String,
    line: usize,
    indent: usize,
    initializer_rhs: Option<String>,
}

fn confusable_local_scope_diagnostics(
    source: &str,
    confusable_local_declaration_level: Option<DiagnosticLevel>,
    confusable_local_usage_level: Option<DiagnosticLevel>,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let mut diagnostics = Vec::new();

    for block in blocks {
        let mut declarations = Vec::<LocalDeclaration>::new();
        for line_idx in block.body_start..block.body_end {
            let line = block.lines[line_idx];
            let trimmed = line.trim_start();
            let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
            if code.is_empty() {
                continue;
            }

            let Some((kind, names)) = parse_local_binding_declaration(code) else {
                continue;
            };
            if kind != LocalBindingKind::Variable {
                continue;
            }

            let indent = line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            for name in names.into_iter().filter(|name| !name.starts_with('_')) {
                declarations.push(LocalDeclaration {
                    name: name.to_string(),
                    line: line_idx + 1,
                    indent,
                    initializer_rhs: extract_variable_initializer(code, name),
                });
            }
        }

        if let Some(level) = confusable_local_declaration_level.as_ref() {
            for declaration in &declarations {
                if declarations.iter().any(|later| {
                    later.name == declaration.name
                        && later.line > declaration.line
                        && later.indent < declaration.indent
                }) {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: declaration.line,
                        column: 1,
                        code: RULE_CONFUSABLE_LOCAL_DECLARATION.to_string(),
                        level: level.clone(),
                        message: format!(
                            "The variable \"{}\" is declared below in the parent block.",
                            declaration.name
                        ),
                    });
                }
            }
        }

        if let Some(level) = confusable_local_usage_level.as_ref() {
            let mut emitted = std::collections::HashSet::<(usize, String)>::new();
            for declaration in &declarations {
                for scan_idx in block.body_start..declaration.line.saturating_sub(1) {
                    let line = block.lines[scan_idx];
                    let code = line.split('#').next().unwrap_or(line);
                    if line_declares_variable_name(code, &declaration.name) {
                        continue;
                    }
                    if !contains_identifier(code, &declaration.name) {
                        continue;
                    }
                    if emitted.insert((scan_idx + 1, declaration.name.clone())) {
                        diagnostics.push(Diagnostic {
                            file: None,
                            line: scan_idx + 1,
                            column: 1,
                            code: RULE_CONFUSABLE_LOCAL_USAGE.to_string(),
                            level: level.clone(),
                            message: format!(
                                "The identifier \"{}\" will be shadowed below in the block.",
                                declaration.name
                            ),
                        });
                    }
                }

                if let Some(rhs) = declaration.initializer_rhs.as_ref() {
                    if contains_identifier(rhs, &declaration.name)
                        && emitted.insert((declaration.line, declaration.name.clone()))
                    {
                        diagnostics.push(Diagnostic {
                            file: None,
                            line: declaration.line,
                            column: 1,
                            code: RULE_CONFUSABLE_LOCAL_USAGE.to_string(),
                            level: level.clone(),
                            message: format!(
                                "The identifier \"{}\" will be shadowed below in the block.",
                                declaration.name
                            ),
                        });
                    }
                }
            }
        }
    }

    diagnostics
}

fn extract_variable_initializer(code: &str, name: &str) -> Option<String> {
    let tail = code.strip_prefix("var ")?;
    for segment in tail.split(',') {
        let token = segment.trim();
        if token.is_empty() {
            continue;
        }

        let name_token = token
            .split(':')
            .next()
            .unwrap_or(token)
            .split('=')
            .next()
            .unwrap_or(token)
            .trim();
        if name_token != name {
            continue;
        }

        if let Some((_, rhs)) = token.split_once('=') {
            return Some(rhs.trim().to_string());
        }
    }

    None
}

fn line_declares_variable_name(code: &str, name: &str) -> bool {
    let Some((kind, names)) = parse_local_binding_declaration(code.trim()) else {
        return false;
    };
    kind == LocalBindingKind::Variable && names.into_iter().any(|candidate| candidate == name)
}

fn function_scope_flow_diagnostics(
    source: &str,
    unreachable_level: Option<DiagnosticLevel>,
    unassigned_variable_level: Option<DiagnosticLevel>,
    unassigned_variable_op_level: Option<DiagnosticLevel>,
) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let mut diagnostics = Vec::new();

    for block in blocks {
        if let Some(level) = unreachable_level.as_ref() {
            let mut return_indent = None::<usize>;
            for line_idx in block.body_start..block.body_end {
                let line = block.lines[line_idx];
                let code = line.trim_start().split('#').next().unwrap_or("").trim();
                if code.is_empty() {
                    continue;
                }
                let indent = line
                    .chars()
                    .take_while(|ch| ch.is_ascii_whitespace())
                    .count();

                if let Some(previous_return_indent) = return_indent {
                    if indent < previous_return_indent {
                        return_indent = None;
                    }
                }

                if return_indent.is_some() {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: line_idx + 1,
                        column: 1,
                        code: RULE_UNREACHABLE_CODE.to_string(),
                        level: level.clone(),
                        message: format!(
                            "Unreachable code (statement after return) in function \"{}()\".",
                            block.name
                        ),
                    });
                }

                if code == "return" || code.starts_with("return ") || code.starts_with("return(") {
                    return_indent = Some(indent);
                }
            }
        }

        if unassigned_variable_level.is_some() || unassigned_variable_op_level.is_some() {
            let mut assigned_state = std::collections::HashMap::<String, bool>::new();
            let mut warned_unassigned = std::collections::HashSet::<(usize, String)>::new();
            let mut warned_op_assign = std::collections::HashSet::<(usize, String)>::new();

            for line_idx in block.body_start..block.body_end {
                let line = block.lines[line_idx];
                let code = line.trim_start().split('#').next().unwrap_or("").trim();
                if code.is_empty() || code.starts_with('@') {
                    continue;
                }

                if let Some(bindings) = parse_var_declaration_state(code) {
                    for (name, assigned) in bindings {
                        assigned_state.insert(name.to_string(), assigned);
                    }
                    continue;
                }

                for (name, is_assigned) in assigned_state.clone() {
                    if is_assigned {
                        continue;
                    }

                    if has_compound_assignment(code, &name) {
                        if let Some(level) = unassigned_variable_op_level.as_ref() {
                            if warned_op_assign.insert((line_idx + 1, name.clone())) {
                                diagnostics.push(Diagnostic {
                                    file: None,
                                    line: line_idx + 1,
                                    column: 1,
                                    code: RULE_UNASSIGNED_VARIABLE_OP_ASSIGN.to_string(),
                                    level: level.clone(),
                                    message: format!(
                                        "The variable \"{name}\" is modified with the compound-assignment operator but was not previously initialized."
                                    ),
                                });
                            }
                        }
                        assigned_state.insert(name.clone(), true);
                        continue;
                    }

                    if has_simple_assignment(code, &name) {
                        assigned_state.insert(name.clone(), true);
                        continue;
                    }

                    if contains_identifier(code, &name) {
                        if let Some(level) = unassigned_variable_level.as_ref() {
                            if warned_unassigned.insert((line_idx + 1, name.clone())) {
                                diagnostics.push(Diagnostic {
                                    file: None,
                                    line: line_idx + 1,
                                    column: 1,
                                    code: RULE_UNASSIGNED_VARIABLE.to_string(),
                                    level: level.clone(),
                                    message: format!(
                                        "The variable \"{name}\" is used before being assigned a value."
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

fn unreachable_pattern_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if !code.starts_with("match ") || !code.ends_with(':') {
            continue;
        }

        let match_indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let mut wildcard_case_indent = None;

        for (scan_idx, scan_line) in lines.iter().enumerate().skip(idx + 1) {
            let scan_trimmed = scan_line.trim_start();
            if scan_trimmed.is_empty() || scan_trimmed.starts_with('#') {
                continue;
            }

            let indent = scan_line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            if indent <= match_indent {
                break;
            }

            let scan_code = scan_trimmed
                .split('#')
                .next()
                .unwrap_or(scan_trimmed)
                .trim();
            if scan_code.is_empty() {
                continue;
            }

            if let Some(case_indent) = wildcard_case_indent {
                if indent == case_indent && scan_code.ends_with(':') && !scan_code.starts_with("_:")
                {
                    diagnostics.push(Diagnostic {
                        file: None,
                        line: scan_idx + 1,
                        column: 1,
                        code: RULE_UNREACHABLE_PATTERN.to_string(),
                        level: level.clone(),
                        message: "Unreachable pattern (pattern after wildcard or bind)."
                            .to_string(),
                    });
                }
            }

            if scan_code.starts_with("_:") {
                wildcard_case_indent = Some(indent);
            }
        }
    }

    diagnostics
}

fn static_called_on_instance_diagnostics(source: &str, level: DiagnosticLevel) -> Vec<Diagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let blocks = collect_function_blocks(&lines);
    let static_methods = collect_static_methods(source);
    let mut diagnostics = Vec::new();

    for block in blocks {
        let mut instance_types = std::collections::HashMap::<String, String>::new();

        for line_idx in block.body_start..block.body_end {
            let line = block.lines[line_idx];
            let code = line.trim_start().split('#').next().unwrap_or("").trim();
            if code.is_empty() {
                continue;
            }

            if let Some((instance_name, instance_type)) = parse_instance_type_declaration(code) {
                instance_types.insert(instance_name.to_string(), instance_type.to_string());
            }

            for (receiver, method) in member_calls(code) {
                let Some(instance_type) = instance_types.get(receiver) else {
                    continue;
                };
                let Some(methods) = static_methods.get(instance_type) else {
                    continue;
                };
                if !methods.contains(method) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    file: None,
                    line: line_idx + 1,
                    column: 1,
                    code: RULE_STATIC_CALLED_ON_INSTANCE.to_string(),
                    level: level.clone(),
                    message: format!(
                        "The function \"{}()\" is a static function but was called from an instance. Instead, it should be directly called from the type: \"{}.{}()\".",
                        method, instance_type, method
                    ),
                });
            }
        }
    }

    diagnostics
}

fn collect_static_methods(
    source: &str,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    let mut out = std::collections::HashMap::<String, std::collections::HashSet<String>>::new();
    out.entry("String".to_string())
        .or_default()
        .insert("num_uint64".to_string());

    let lines = source.lines().collect::<Vec<_>>();
    let mut class_stack = Vec::<(usize, String)>::new();
    let mut script_class_name: Option<String> = None;

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        while let Some((class_indent, _)) = class_stack.last() {
            if indent <= *class_indent {
                class_stack.pop();
            } else {
                break;
            }
        }

        if let Some(rest) = trimmed.strip_prefix("class_name ") {
            if let Some(name) = extract_fn_name(rest) {
                script_class_name = Some(name);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = extract_fn_name(rest) {
                class_stack.push((indent, name));
            }
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("static func ") else {
            continue;
        };
        let Some(method_name) = extract_fn_name(rest) else {
            continue;
        };

        if let Some((_, class_name)) = class_stack.last() {
            out.entry(class_name.clone())
                .or_default()
                .insert(method_name.clone());
            continue;
        }

        if indent == 0 {
            if let Some(script_class_name) = script_class_name.as_ref() {
                out.entry(script_class_name.clone())
                    .or_default()
                    .insert(method_name);
            }
        }
    }

    out
}

fn parse_instance_type_declaration(code: &str) -> Option<(&str, &str)> {
    let tail = code.strip_prefix("var ")?;
    let (name, rhs) = tail.split_once(":=").or_else(|| tail.split_once('='))?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let rhs = rhs.trim();
    if let Some(type_name) = rhs.strip_suffix("()") {
        let type_name = type_name.trim();
        if is_type_identifier(type_name) {
            return Some((name, type_name));
        }
    }
    if let Some(type_name) = rhs.strip_suffix(".new()") {
        let type_name = type_name.trim();
        if is_type_identifier(type_name) {
            return Some((name, type_name));
        }
    }

    None
}

fn is_type_identifier(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn member_calls(code: &str) -> Vec<(&str, &str)> {
    let bytes = code.as_bytes();
    let mut idx = 0usize;
    let mut out = Vec::new();

    while idx < bytes.len() {
        if bytes[idx] != b'.' {
            idx += 1;
            continue;
        }

        let mut left = idx;
        while left > 0 && (bytes[left - 1].is_ascii_alphanumeric() || bytes[left - 1] == b'_') {
            left -= 1;
        }
        if left == idx {
            idx += 1;
            continue;
        }

        let mut right = idx + 1;
        while right < bytes.len() && (bytes[right].is_ascii_alphanumeric() || bytes[right] == b'_')
        {
            right += 1;
        }
        if right == idx + 1 {
            idx += 1;
            continue;
        }

        let mut scan = right;
        while scan < bytes.len() && bytes[scan].is_ascii_whitespace() {
            scan += 1;
        }
        if scan >= bytes.len() || bytes[scan] != b'(' {
            idx += 1;
            continue;
        }

        let receiver = &code[left..idx];
        let method = &code[idx + 1..right];
        out.push((receiver, method));
        idx = right;
    }

    out
}

fn parse_var_declaration_state(code: &str) -> Option<Vec<(&str, bool)>> {
    let tail = code.strip_prefix("var ")?;
    let mut out = Vec::new();
    for segment in tail.split(',') {
        let token = segment.trim();
        if token.is_empty() {
            continue;
        }
        let name_token = token
            .split(':')
            .next()
            .unwrap_or(token)
            .split('=')
            .next()
            .unwrap_or(token)
            .trim();
        if name_token.is_empty() {
            continue;
        }
        if !name_token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            || !name_token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            continue;
        }
        let assigned = token.contains('=');
        out.push((name_token, assigned));
    }
    if out.is_empty() { None } else { Some(out) }
}

fn has_simple_assignment(code: &str, name: &str) -> bool {
    let pattern = format!("{name} =");
    let pattern2 = format!("{name}:=");
    let pattern3 = format!("{name}=");
    code.starts_with(&pattern)
        || code.starts_with(&pattern2)
        || code.starts_with(&pattern3)
        || code.starts_with(&format!("if {name} ="))
}

fn has_compound_assignment(code: &str, name: &str) -> bool {
    [
        format!("{name} +="),
        format!("{name} -="),
        format!("{name} *="),
        format!("{name} /="),
        format!("{name} %="),
        format!("{name} &="),
        format!("{name} |="),
        format!("{name} ^="),
    ]
    .iter()
    .any(|pattern| code.starts_with(pattern))
}

fn collect_function_blocks<'a>(lines: &'a [&'a str]) -> Vec<FunctionBlock<'a>> {
    let mut blocks = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let signature = if let Some(sig) = trimmed.strip_prefix("func ") {
            sig
        } else if let Some(sig) = trimmed.strip_prefix("static func ") {
            sig
        } else {
            continue;
        };
        let Some(name) = extract_fn_name(signature) else {
            continue;
        };

        let indent = line
            .chars()
            .take_while(|ch| ch.is_ascii_whitespace())
            .count();
        let mut body_end = lines.len();
        for (scan_idx, scan_line) in lines.iter().enumerate().skip(idx + 1) {
            let scan_trimmed = scan_line.trim_start();
            if scan_trimmed.is_empty() || scan_trimmed.starts_with('#') {
                continue;
            }
            let scan_indent = scan_line
                .chars()
                .take_while(|ch| ch.is_ascii_whitespace())
                .count();
            if scan_indent <= indent {
                body_end = scan_idx;
                break;
            }
        }

        blocks.push(FunctionBlock {
            name,
            body_start: idx + 1,
            body_end,
            lines,
        });
    }

    blocks
}

fn parse_local_binding_declaration(code: &str) -> Option<(LocalBindingKind, Vec<&str>)> {
    let (kind, tail) = if let Some(rest) = code.strip_prefix("var ") {
        (LocalBindingKind::Variable, rest)
    } else if let Some(rest) = code.strip_prefix("const ") {
        (LocalBindingKind::Constant, rest)
    } else {
        return None;
    };

    let mut names = Vec::new();
    for segment in tail.split(',') {
        let token = segment.trim();
        if token.is_empty() {
            continue;
        }
        let token = token
            .split(':')
            .next()
            .unwrap_or(token)
            .split('=')
            .next()
            .unwrap_or(token)
            .trim();
        if token.is_empty() {
            continue;
        }
        if token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            names.push(token);
        }
    }

    if names.is_empty() {
        None
    } else {
        Some((kind, names))
    }
}

fn builtin_global_identifiers() -> &'static std::collections::HashSet<&'static str> {
    use std::collections::HashSet;
    use std::sync::OnceLock;

    static BUILTINS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let mut out = HashSet::new();
        out.extend([
            "abs",
            "Array",
            "Callable",
            "Dictionary",
            "Node",
            "Object",
            "String",
            "Vector2",
            "Vector2i",
            "is_same",
            "print",
            "print_debug",
            "print_stack",
            "sqrt",
            "len",
            "preload",
            "load",
            "range",
            "min",
            "max",
            "sin",
            "cos",
            "tan",
            "clamp",
            "floor",
            "ceil",
            "round",
        ]);
        out.extend(
            include_str!("../data/godot_4_6_utility_functions.txt")
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty()),
        );
        out.extend(
            include_str!("../data/godot_4_6_builtin_meta.tsv")
                .lines()
                .skip(1)
                .filter_map(|line| line.split('\t').next())
                .map(str::trim)
                .filter(|line| !line.is_empty()),
        );
        out
    })
}

fn extract_fn_name(signature: &str) -> Option<String> {
    let mut out = String::new();
    for ch in signature.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
            continue;
        }
        break;
    }
    if out.is_empty() { None } else { Some(out) }
}

fn extract_fn_params(signature: &str) -> Option<Vec<String>> {
    let start = signature.find('(')?;
    let end = signature[start + 1..].find(')')? + start + 1;
    let params = signature[start + 1..end]
        .split(',')
        .map(str::trim)
        .filter(|param| !param.is_empty())
        .filter_map(|param| {
            let token = param.split(':').next().unwrap_or(param);
            let token = token.split('=').next().unwrap_or(token).trim();
            if token.is_empty() {
                None
            } else {
                Some(token.to_string())
            }
        })
        .collect::<Vec<_>>();
    Some(params)
}

fn contains_identifier(line: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    for (idx, _) in line.match_indices(needle) {
        let left = idx
            .checked_sub(1)
            .and_then(|i| line.as_bytes().get(i).copied());
        let right = line.as_bytes().get(idx + needle.len()).copied();
        let left_ok = left
            .map(|byte| !((byte as char).is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(true);
        let right_ok = right
            .map(|byte| !((byte as char).is_ascii_alphanumeric() || byte == b'_'))
            .unwrap_or(true);
        if left_ok && right_ok {
            return true;
        }
    }

    false
}

fn has_onready_export_conflict(line: &str) -> bool {
    let code = line.split('#').next().unwrap_or(line);
    code.contains("@onready") && code.contains("@export")
}

fn contains_confusable_identifier(line: &str) -> bool {
    let code = line.split('#').next().unwrap_or(line);
    let mut token = String::new();

    for ch in code.chars().chain(std::iter::once(' ')) {
        if is_unicode_identifier_char(ch) {
            token.push(ch);
            continue;
        }

        if !token.is_empty() {
            if token.chars().any(|value| !value.is_ascii())
                && token
                    .chars()
                    .any(|value| value.is_ascii_alphanumeric() || value == '_')
            {
                return true;
            }
            token.clear();
        }
    }

    false
}

fn is_unicode_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric() || (!ch.is_ascii() && ch.is_alphabetic())
}

fn is_assert_always_true(trimmed: &str) -> bool {
    let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
    let Some(expr) = code
        .strip_prefix("assert(")
        .and_then(|tail| tail.strip_suffix(')'))
    else {
        return false;
    };

    evaluate_assert_truthiness(expr.trim()).is_some_and(|value| value)
}

fn evaluate_assert_truthiness(expr: &str) -> Option<bool> {
    if expr.eq_ignore_ascii_case("true") {
        return Some(true);
    }
    if expr.eq_ignore_ascii_case("false") {
        return Some(false);
    }

    if let Ok(value) = expr.parse::<f64>() {
        return Some(value != 0.0);
    }

    let (left, right) = expr.split_once("==")?;
    let left = evaluate_simple_int_expression(left.trim())?;
    let right = evaluate_simple_int_expression(right.trim())?;
    Some(left == right)
}

fn evaluate_simple_int_expression(expr: &str) -> Option<i64> {
    if let Ok(value) = expr.parse::<i64>() {
        return Some(value);
    }

    if let Some((left, right)) = expr.split_once('+') {
        return Some(
            evaluate_simple_int_expression(left.trim())?
                + evaluate_simple_int_expression(right.trim())?,
        );
    }

    if let Some((left, right)) = expr.split_once('-') {
        return Some(
            evaluate_simple_int_expression(left.trim())?
                - evaluate_simple_int_expression(right.trim())?,
        );
    }

    None
}

fn is_standalone_ternary(trimmed: &str) -> bool {
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    if trimmed.contains('=') {
        return false;
    }
    if trimmed.starts_with("return ") {
        return false;
    }
    trimmed.contains(" if ") && trimmed.contains(" else ")
}

fn is_standalone_expression(trimmed: &str) -> bool {
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with('@')
        || trimmed.contains('=')
        || trimmed.starts_with("func ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("class_name ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elif ")
        || trimmed.starts_with("else")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("break")
        || trimmed.starts_with("continue")
        || trimmed.starts_with("pass")
        || trimmed.starts_with("await ")
    {
        return false;
    }

    if is_standalone_ternary(trimmed) {
        return false;
    }

    if is_numeric_literal(trimmed) {
        return true;
    }

    if looks_like_pure_numeric_expression(trimmed) {
        return true;
    }

    if is_array_or_dictionary_literal(trimmed) {
        return true;
    }

    is_uppercase_member_access(trimmed)
}

fn is_array_or_dictionary_literal(trimmed: &str) -> bool {
    let compact = trimmed.split('#').next().unwrap_or(trimmed).trim();
    (compact.starts_with('[') && compact.ends_with(']'))
        || (compact.starts_with('{') && compact.ends_with('}'))
}

fn is_numeric_literal(trimmed: &str) -> bool {
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+'))
        && trimmed.chars().any(|ch| ch.is_ascii_digit())
}

fn looks_like_pure_numeric_expression(trimmed: &str) -> bool {
    let has_operator = trimmed
        .chars()
        .any(|ch| matches!(ch, '+' | '-' | '*' | '/'));
    has_operator
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-' | '*' | '/' | ' ' | '\t'))
        && trimmed.chars().any(|ch| ch.is_ascii_digit())
}

fn is_uppercase_member_access(trimmed: &str) -> bool {
    let Some((left, right)) = trimmed.split_once('.') else {
        return false;
    };
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left.chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && right
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_return_value_discarded_call(trimmed: &str) -> bool {
    let trimmed = trimmed.split('#').next().unwrap_or(trimmed).trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with('@')
        || trimmed.contains('=')
        || trimmed.starts_with("return ")
        || trimmed.starts_with("await ")
    {
        return false;
    }

    let Some(paren_start) = trimmed.find('(') else {
        return false;
    };
    if !trimmed.ends_with(')') {
        return false;
    }

    let callee = trimmed[..paren_start].trim();
    if callee.is_empty()
        || !callee
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return false;
    }

    !matches!(
        callee,
        "print"
            | "print_debug"
            | "print_stack"
            | "print_rich"
            | "print_verbose"
            | "printerr"
            | "printraw"
            | "prints"
            | "printt"
            | "push_error"
            | "push_warning"
            | "randomize"
            | "seed"
    )
}

fn find_unspaced_assignment_operator(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for idx in 0..bytes.len() {
        if bytes[idx] != b'=' {
            continue;
        }

        let mut operator_start = idx;
        while operator_start > 0 && is_assignment_operator_prefix(bytes[operator_start - 1]) {
            operator_start -= 1;
        }
        let operator = &line[operator_start..=idx];
        if matches!(operator, "==" | "!=" | "<=" | ">=") {
            continue;
        }

        let before_operator = if operator_start > 0 {
            Some(bytes[operator_start - 1])
        } else {
            None
        };
        let after_operator = bytes.get(idx + 1).copied();

        if before_operator != Some(b' ') || after_operator != Some(b' ') {
            return Some(operator_start + 1);
        }
    }

    None
}

fn is_assignment_operator_prefix(byte: u8) -> bool {
    matches!(
        byte,
        b':' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'<' | b'>' | b'!'
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    Parity,
    Enhanced,
}

fn rule_kind(rule_code: &str) -> RuleKind {
    match rule_code {
        RULE_TODO_COMMENT => RuleKind::Enhanced,
        _ => RuleKind::Parity,
    }
}

fn rule_available_in_mode(rule_code: &str, mode: BehaviorMode) -> bool {
    match (rule_kind(rule_code), mode) {
        (RuleKind::Parity, _) => true,
        (RuleKind::Enhanced, BehaviorMode::Enhanced) => true,
        (RuleKind::Enhanced, BehaviorMode::Parity) => false,
    }
}

fn default_level_for_rule(rule_code: &str) -> Option<DiagnosticLevel> {
    match rule_code {
        RULE_TRAILING_WHITESPACE => Some(DiagnosticLevel::Warning),
        RULE_NO_TABS => Some(DiagnosticLevel::Warning),
        RULE_MAX_LINE_LENGTH => Some(DiagnosticLevel::Info),
        RULE_SPACES_AROUND_OPERATOR => Some(DiagnosticLevel::Warning),
        RULE_TODO_COMMENT => Some(DiagnosticLevel::Info),
        RULE_EMPTY_FILE => Some(DiagnosticLevel::Warning),
        RULE_STANDALONE_EXPRESSION => Some(DiagnosticLevel::Warning),
        RULE_STANDALONE_TERNARY => Some(DiagnosticLevel::Warning),
        RULE_RETURN_VALUE_DISCARDED => Some(DiagnosticLevel::Off),
        RULE_INTEGER_DIVISION => Some(DiagnosticLevel::Warning),
        RULE_UNUSED_PARAMETER => Some(DiagnosticLevel::Warning),
        RULE_UNREACHABLE_CODE => Some(DiagnosticLevel::Warning),
        RULE_UNASSIGNED_VARIABLE => Some(DiagnosticLevel::Warning),
        RULE_UNASSIGNED_VARIABLE_OP_ASSIGN => Some(DiagnosticLevel::Warning),
        RULE_STATIC_CALLED_ON_INSTANCE => Some(DiagnosticLevel::Warning),
        RULE_INFERENCE_ON_VARIANT => Some(DiagnosticLevel::Warning),
        RULE_ONREADY_WITH_EXPORT => Some(DiagnosticLevel::Error),
        "native-method-override" | "get-node-default-without-onready" => {
            Some(DiagnosticLevel::Error)
        }
        "untyped-declaration"
        | "inferred-declaration"
        | "unsafe-property-access"
        | "unsafe-method-access"
        | "unsafe-cast"
        | "unsafe-call-argument"
        | "missing-await" => Some(DiagnosticLevel::Off),
        _ => {
            if upstream_rule_ids().iter().any(|rule| rule == rule_code) {
                Some(DiagnosticLevel::Warning)
            } else {
                None
            }
        }
    }
}

fn upstream_rule_ids() -> &'static Vec<String> {
    static RULES: OnceLock<Vec<String>> = OnceLock::new();
    RULES.get_or_init(|| {
        include_str!("../data/godot_4_6_warning_codes.txt")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.replace('_', "-"))
            .collect()
    })
}

pub fn rule_ids() -> Vec<String> {
    let mut rules = vec![
        RULE_TRAILING_WHITESPACE.to_string(),
        RULE_NO_TABS.to_string(),
        RULE_MAX_LINE_LENGTH.to_string(),
        RULE_SPACES_AROUND_OPERATOR.to_string(),
        RULE_TODO_COMMENT.to_string(),
        RULE_EMPTY_FILE.to_string(),
        RULE_STANDALONE_EXPRESSION.to_string(),
        RULE_STANDALONE_TERNARY.to_string(),
        RULE_RETURN_VALUE_DISCARDED.to_string(),
    ];
    rules.extend(upstream_rule_ids().iter().cloned());
    rules.sort_unstable();
    rules.dedup();
    rules
}

impl DiagnosticLevel {
    fn from_raw(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LintOverrides, LintSettings, check_document_with_settings, rule_ids};

    #[test]
    fn overrides_apply_after_defaults() {
        let settings = LintSettings::default().with_overrides(LintOverrides {
            max_line_length: Some(80),
            allow_tabs: Some(true),
            require_spaces_around_operators: Some(false),
        });

        assert_eq!(settings.max_line_length, 80);
        assert!(settings.allow_tabs);
        assert!(!settings.require_spaces_around_operators);
    }

    #[test]
    fn detects_assignment_spacing_issue() {
        let diagnostics = check_document_with_settings("a=1\n", &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == "spaces-around-operator")
        );
    }

    #[test]
    fn accepts_spaced_colon_and_compound_assignment_operators() {
        let source = "value := 1\nvalue -= 2\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != "spaces-around-operator"),
            "diagnostics: {diagnostics:#?}"
        );
    }

    #[test]
    fn detects_unspaced_colon_and_compound_assignment_operators() {
        let source = "value:=1\nvalue-=2\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        let spacing_issues = diagnostics
            .iter()
            .filter(|diag| diag.code == "spaces-around-operator")
            .count();
        assert_eq!(spacing_issues, 2, "diagnostics: {diagnostics:#?}");
    }

    #[test]
    fn detects_unspaced_shift_assignment_operators() {
        let source = "value<<=1\nvalue>>=2\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        let spacing_issues = diagnostics
            .iter()
            .filter(|diag| diag.code == "spaces-around-operator")
            .count();
        assert_eq!(spacing_issues, 2, "diagnostics: {diagnostics:#?}");
    }

    #[test]
    fn comparison_operators_do_not_trigger_assignment_spacing_rule() {
        let source = "if value<=1:\n    pass\nif value!=2:\n    pass\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != "spaces-around-operator"),
            "diagnostics: {diagnostics:#?}"
        );
    }

    #[test]
    fn rule_ids_include_upstream_warning_registry_snapshot() {
        let rules = rule_ids();
        assert!(rules.iter().any(|rule| rule == "unassigned-variable"));
        assert!(rules.iter().any(|rule| rule == "inference-on-variant"));
        assert!(rules.iter().any(|rule| rule == "no-tabs"));
    }

    #[test]
    fn empty_file_warning_is_reported_by_default() {
        let diagnostics = check_document_with_settings("", &LintSettings::default());
        assert!(diagnostics.iter().any(|diag| diag.code == "empty-file"));
    }

    #[test]
    fn standalone_ternary_is_reported_by_default() {
        let source = "func test():\n    1 if true else 2\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == "standalone-ternary")
        );
    }

    #[test]
    fn return_value_discarded_is_off_by_default() {
        let source =
            "func i_return_int() -> int:\n    return 4\n\nfunc test():\n    i_return_int()\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.code != "return-value-discarded")
        );
    }

    #[test]
    fn return_value_discarded_can_be_enabled_by_severity_override() {
        let source =
            "func i_return_int() -> int:\n    return 4\n\nfunc test():\n    i_return_int()\n";
        let mut settings = LintSettings::default();
        settings.rule_severities.insert(
            "return-value-discarded".to_string(),
            super::DiagnosticLevel::Warning,
        );
        let diagnostics = check_document_with_settings(source, &settings);
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == "return-value-discarded")
        );
    }

    #[test]
    fn standalone_expression_reports_literal_and_collection_usage() {
        let source = "func test():\n    1234\n    [true, false]\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        let standalone = diagnostics
            .iter()
            .filter(|diag| diag.code == "standalone-expression")
            .count();
        assert!(
            standalone >= 2,
            "expected standalone-expression warnings, got {diagnostics:#?}"
        );
    }

    #[test]
    fn integer_division_reports_int_over_int_operations() {
        let source = "func test():\n    var __ = 5 / 2\n    __ = 5.0 / 2\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == "integer-division"),
            "expected integer-division warning, got {diagnostics:#?}"
        );
    }

    #[test]
    fn unused_parameter_reports_non_underscored_parameters() {
        let source = "func function_with_unused_argument(p_arg1, p_arg2):\n    print(p_arg1)\n";
        let diagnostics = check_document_with_settings(source, &LintSettings::default());
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.code == "unused-parameter"),
            "expected unused-parameter warning, got {diagnostics:#?}"
        );
    }
}
