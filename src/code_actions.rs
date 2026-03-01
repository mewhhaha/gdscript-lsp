use crate::engine::BehaviorMode;
use crate::formatter;
use crate::lint::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRange {
    pub line: usize,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeActionPatch {
    pub line: usize,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeActionKind {
    QuickFix,
    Refactor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    pub title: String,
    pub kind: CodeActionKind,
    pub patch: CodeActionPatch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub fn code_actions_for_diagnostics(source: &str, diagnostics: &[Diagnostic]) -> Vec<CodeAction> {
    code_actions_for_diagnostics_and_mode(source, diagnostics, BehaviorMode::Enhanced)
}

pub fn code_actions_for_diagnostics_and_mode(
    source: &str,
    diagnostics: &[Diagnostic],
    mode: BehaviorMode,
) -> Vec<CodeAction> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic.code.as_str() {
            "trailing-whitespace" => {
                if !action_available_in_mode(CodeActionKindId::TrailingWhitespace, mode) {
                    return None;
                }
                let replacement = formatter::format_gdscript(source)
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default()
                    .to_string();

                Some(CodeAction {
                    title: "Trim trailing whitespace".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "no-tabs" => {
                if !action_available_in_mode(CodeActionKindId::NoTabs, mode) {
                    return None;
                }
                let replacement = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default()
                    .replace('\t', "    ");

                Some(CodeAction {
                    title: "Replace tabs with spaces".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "spaces-around-operator" => {
                if !action_available_in_mode(CodeActionKindId::SpacesAroundOperator, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = fix_assignment_spacing(current);

                Some(CodeAction {
                    title: "Insert spaces around operator".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "max-line-length" => {
                if !action_available_in_mode(CodeActionKindId::MaxLineLength, mode) {
                    return None;
                }
                let replacement: String = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect();

                Some(CodeAction {
                    title: "Wrap long line".to_string(),
                    kind: CodeActionKind::Refactor,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "todo-comment" => {
                if !action_available_in_mode(CodeActionKindId::TodoComment, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = remove_todo_from_line(current);

                Some(CodeAction {
                    title: "Remove TODO comment".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "unused-parameter" => {
                if !action_available_in_mode(CodeActionKindId::UnusedParameter, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let name = extract_first_quoted(&diagnostic.message)?;
                let replacement = prefix_unused_parameter(current, &name)?;

                Some(CodeAction {
                    title: "Prefix unused parameter with underscore".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "unused-variable" => {
                if !action_available_in_mode(CodeActionKindId::UnusedVariable, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let name = extract_first_quoted(&diagnostic.message)?;
                let replacement = prefix_binding_name(current, "var", &name)?;

                Some(CodeAction {
                    title: "Prefix unused variable with underscore".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "unused-local-constant" => {
                if !action_available_in_mode(CodeActionKindId::UnusedLocalConstant, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let name = extract_first_quoted(&diagnostic.message)?;
                let replacement = prefix_binding_name(current, "const", &name)?;

                Some(CodeAction {
                    title: "Prefix unused constant with underscore".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "static-called-on-instance" => {
                if !action_available_in_mode(CodeActionKindId::StaticCalledOnInstance, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = rewrite_static_receiver(current, &diagnostic.message)?;

                Some(CodeAction {
                    title: "Call static method on type".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "integer-division" => {
                if !action_available_in_mode(CodeActionKindId::IntegerDivision, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = cast_left_operand_for_integer_division(current)?;

                Some(CodeAction {
                    title: "Convert left operand to float".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "onready-with-export" => {
                if !action_available_in_mode(CodeActionKindId::OnreadyWithExport, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = remove_onready_annotation(current)?;

                Some(CodeAction {
                    title: "Remove @onready to keep export precedence".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "get-node-default-without-onready" => {
                if !action_available_in_mode(CodeActionKindId::GetNodeDefaultWithoutOnready, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = add_onready_annotation(current)?;

                Some(CodeAction {
                    title: "Add @onready annotation".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            "standalone-expression" => {
                if !action_available_in_mode(CodeActionKindId::StandaloneExpression, mode) {
                    return None;
                }
                let current = source
                    .lines()
                    .nth(diagnostic.line.saturating_sub(1))
                    .unwrap_or_default();
                let replacement = discard_standalone_expression(current)?;

                Some(CodeAction {
                    title: "Consume standalone expression with discard".to_string(),
                    kind: CodeActionKind::QuickFix,
                    patch: CodeActionPatch {
                        line: diagnostic.line,
                        replacement,
                    },
                    command: None,
                    data: None,
                })
            }
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum CodeActionRuleKind {
    Parity,
    Enhanced,
}

#[derive(Debug, Clone, Copy)]
enum CodeActionKindId {
    TrailingWhitespace,
    NoTabs,
    SpacesAroundOperator,
    MaxLineLength,
    TodoComment,
    UnusedParameter,
    UnusedVariable,
    UnusedLocalConstant,
    StaticCalledOnInstance,
    IntegerDivision,
    OnreadyWithExport,
    GetNodeDefaultWithoutOnready,
    StandaloneExpression,
}

impl CodeActionKindId {
    fn rule_kind(self) -> CodeActionRuleKind {
        match self {
            Self::TrailingWhitespace
            | Self::NoTabs
            | Self::SpacesAroundOperator
            | Self::MaxLineLength
            | Self::UnusedParameter
            | Self::UnusedVariable
            | Self::UnusedLocalConstant
            | Self::StaticCalledOnInstance
            | Self::IntegerDivision
            | Self::OnreadyWithExport
            | Self::GetNodeDefaultWithoutOnready
            | Self::StandaloneExpression => CodeActionRuleKind::Parity,
            Self::TodoComment => CodeActionRuleKind::Enhanced,
        }
    }
}

fn action_available_in_mode(kind: CodeActionKindId, mode: BehaviorMode) -> bool {
    match (kind.rule_kind(), mode) {
        (CodeActionRuleKind::Parity, _) => true,
        (CodeActionRuleKind::Enhanced, BehaviorMode::Enhanced) => true,
        (CodeActionRuleKind::Enhanced, BehaviorMode::Parity) => false,
    }
}

fn fix_assignment_spacing(line: &str) -> String {
    let bytes = line.as_bytes();
    for idx in 0..bytes.len() {
        if bytes[idx] != b'=' {
            continue;
        }

        let prev = if idx > 0 { Some(bytes[idx - 1]) } else { None };
        let next = if idx + 1 < bytes.len() {
            Some(bytes[idx + 1])
        } else {
            None
        };

        if prev == Some(b'=')
            || next == Some(b'=')
            || prev == Some(b'!')
            || prev == Some(b'<')
            || prev == Some(b'>')
        {
            continue;
        }

        let left = line[..idx].trim_end();
        let right = line[idx + 1..].trim_start();
        return format!("{left} = {right}");
    }

    line.to_string()
}

fn remove_todo_from_line(line: &str) -> String {
    line.replacen("TODO", "", 1).trim_end().to_string()
}

fn extract_first_quoted(message: &str) -> Option<String> {
    let mut parts = message.split('"');
    let _ = parts.next();
    parts.next().map(ToString::to_string)
}

fn prefix_unused_parameter(line: &str, parameter: &str) -> Option<String> {
    if parameter.starts_with('_') {
        return None;
    }

    let start = line.find('(')?;
    let end = line[start + 1..].find(')')? + start + 1;
    let parameters = &line[start + 1..end];
    let mut changed = false;
    let rewritten = parameters
        .split(',')
        .map(|raw| {
            let trimmed = raw.trim();
            let token = trimmed.split(':').next().unwrap_or(trimmed);
            let token = token.split('=').next().unwrap_or(token).trim();
            if token == parameter {
                changed = true;
                raw.replacen(parameter, &format!("_{parameter}"), 1)
            } else {
                raw.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    if !changed {
        return None;
    }

    Some(format!(
        "{}{}{}",
        &line[..start + 1],
        rewritten,
        &line[end..]
    ))
}

fn prefix_binding_name(line: &str, keyword: &str, name: &str) -> Option<String> {
    if name.starts_with('_') {
        return None;
    }

    let trimmed_start = line.trim_start();
    let indent_len = line.len().saturating_sub(trimmed_start.len());
    let tail = trimmed_start.strip_prefix(keyword)?.trim_start();
    if !tail.starts_with(name) {
        return None;
    }

    let mut replacement = String::new();
    replacement.push_str(&line[..indent_len]);
    replacement.push_str(keyword);
    replacement.push(' ');
    replacement.push('_');
    replacement.push_str(name);
    replacement.push_str(&tail[name.len()..]);
    Some(replacement)
}

fn rewrite_static_receiver(line: &str, message: &str) -> Option<String> {
    let mut last_quoted = None;
    for part in message.split('"').skip(1).step_by(2) {
        last_quoted = Some(part);
    }
    let suggestion = last_quoted?;
    let (type_name, method_with_paren) = suggestion.split_once('.')?;
    let method = method_with_paren.strip_suffix("()")?;

    let pattern = format!(".{method}(");
    let idx = line.find(&pattern)?;
    let receiver_start = line[..idx]
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |value| value + 1);

    Some(format!(
        "{}{}{}",
        &line[..receiver_start],
        type_name,
        &line[idx..]
    ))
}

fn cast_left_operand_for_integer_division(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
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

        return Some(format!(
            "{}float({}){}",
            &line[..left],
            &line[left..left_end],
            &line[left_end..]
        ));
    }

    None
}

fn split_code_and_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                    idx += 1;
                    continue;
                }
                if ch == b'\\' {
                    escaped = true;
                    idx += 1;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                idx += 1;
            }
            None => {
                if ch == b'\'' || ch == b'"' {
                    quote = Some(ch);
                    idx += 1;
                    continue;
                }
                if ch == b'#' {
                    let code = line[..idx].trim_end();
                    let comment = line[idx + 1..].trim();
                    if comment.is_empty() {
                        return (code, None);
                    }
                    return (code, Some(comment));
                }
                idx += 1;
            }
        }
    }

    (line.trim_end(), None)
}

fn remove_onready_annotation(line: &str) -> Option<String> {
    let (code, comment) = split_code_and_comment(line);
    if !code.contains("@onready") || !code.contains("@export") {
        return None;
    }

    let trimmed = code.trim_start();
    let indent_len = code.len().saturating_sub(trimmed.len());
    let indent = &code[..indent_len];

    let mut body = String::new();
    for token in trimmed.split_whitespace() {
        if token == "@onready" {
            continue;
        }
        if !body.is_empty() {
            body.push(' ');
        }
        body.push_str(token);
    }

    if body.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str(indent);
    out.push_str(&body);
    if let Some(comment) = comment {
        out.push_str(" # ");
        out.push_str(comment);
    }

    Some(out)
}

fn add_onready_annotation(line: &str) -> Option<String> {
    let (code, comment) = split_code_and_comment(line);
    if code.contains("@onready") {
        return None;
    }

    let current = code.trim_start();
    if !current.starts_with("var ") {
        return None;
    }

    let indent_len = code.len().saturating_sub(current.len());
    let indent = &code[..indent_len];
    let mut out = String::new();
    out.push_str(indent);
    out.push_str("@onready ");
    out.push_str(current);
    if let Some(comment) = comment {
        out.push_str(" # ");
        out.push_str(comment);
    }
    Some(out)
}

fn discard_standalone_expression(line: &str) -> Option<String> {
    let (code, comment) = split_code_and_comment(line);
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }

    let indent = &code[..code.len().saturating_sub(code.trim_start().len())];
    let mut out = format!("{indent}_ = {trimmed}");
    if let Some(comment) = comment {
        out.push_str(" # ");
        out.push_str(comment);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        add_onready_annotation, code_actions_for_diagnostics, discard_standalone_expression,
        fix_assignment_spacing, remove_onready_annotation,
    };
    use crate::lint::{Diagnostic, DiagnosticLevel};

    fn mk_diag(code: &str, line: usize, message: &str, level: DiagnosticLevel) -> Diagnostic {
        Diagnostic {
            file: None,
            line,
            column: 1,
            code: code.to_string(),
            level,
            message: message.to_string(),
        }
    }

    #[test]
    fn formats_simple_assignment() {
        assert_eq!(fix_assignment_spacing("a=1"), "a = 1");
    }

    #[test]
    fn remove_onready_annotation_action() {
        let source = "@export @onready var value = 1\n";
        let actions = code_actions_for_diagnostics(
            source,
            &[mk_diag(
                "onready-with-export",
                1,
                "rule",
                DiagnosticLevel::Warning,
            )],
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].title,
            "Remove @onready to keep export precedence"
        );
        assert_eq!(
            actions[0].patch.replacement, "@export var value = 1",
            "onready-with-export fix should remove redundant decorator"
        );
    }

    #[test]
    fn add_onready_annotation_action() {
        let source = "var node = get_node(\"%Player\")\n";
        let actions = code_actions_for_diagnostics(
            source,
            &[mk_diag(
                "get-node-default-without-onready",
                1,
                "rule",
                DiagnosticLevel::Warning,
            )],
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Add @onready annotation");
        assert_eq!(
            actions[0].patch.replacement, "@onready var node = get_node(\"%Player\")",
            "onready fix should add @onready annotation"
        );
    }

    #[test]
    fn consume_standalone_expression_action() {
        let source = "    1 + 2\n";
        let actions = code_actions_for_diagnostics(
            source,
            &[mk_diag(
                "standalone-expression",
                1,
                "rule",
                DiagnosticLevel::Warning,
            )],
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].title,
            "Consume standalone expression with discard"
        );
        assert_eq!(
            actions[0].patch.replacement, "    _ = 1 + 2",
            "standalone-expression should become explicit discard assignment"
        );
    }

    #[test]
    fn helper_remove_onready_annotation() {
        assert_eq!(
            remove_onready_annotation("@export @onready var value = 1"),
            Some("@export var value = 1".to_string())
        );
    }

    #[test]
    fn helper_add_onready_annotation() {
        assert_eq!(
            add_onready_annotation("var node = get_node(\"%Player\")"),
            Some("@onready var node = get_node(\"%Player\")".to_string())
        );
    }

    #[test]
    fn helper_discard_standalone_expression() {
        assert_eq!(
            discard_standalone_expression("    value"),
            Some("    _ = value".to_string())
        );
    }

    #[test]
    fn helper_split_comment_respects_string_literals() {
        assert_eq!(
            remove_onready_annotation("    @export @onready var label = \"#x\" # hello"),
            Some("    @export var label = \"#x\" # hello".to_string())
        );
    }

    #[test]
    fn helper_discard_preserves_comment() {
        assert_eq!(
            discard_standalone_expression("    value_call() # keep"),
            Some("    _ = value_call() # keep".to_string())
        );
    }
}
