use crate::engine::BehaviorMode;
use crate::formatter;
use crate::lint::Diagnostic;
use serde::{Deserialize, Serialize};

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
            | Self::IntegerDivision => CodeActionRuleKind::Parity,
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

#[cfg(test)]
mod tests {
    use super::fix_assignment_spacing;

    #[test]
    fn formats_simple_assignment() {
        assert_eq!(fix_assignment_spacing("a=1"), "a = 1");
    }
}
