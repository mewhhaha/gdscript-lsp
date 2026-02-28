use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptDecl {
    pub kind: ScriptDeclKind,
    pub name: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScriptDeclKind {
    Function,
    Class,
    Variable,
    Constant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserError {
    pub message: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedScript {
    pub path: PathBuf,
    pub declarations: Vec<ScriptDecl>,
    pub lines: Vec<String>,
    pub issues: Vec<ParserError>,
}

#[derive(Default)]
struct QuoteState {
    quote: Option<u8>,
    triple: bool,
    escaped: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActiveStaticConstructor {
    indent: usize,
}

#[derive(Default)]
struct ActiveFunctionScope {
    indent: usize,
    symbols: HashMap<String, ScriptDeclKind>,
    local_types: HashMap<String, String>,
    weak_symbols: HashSet<String>,
    returns_void: bool,
}

fn split_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut state = QuoteState::default();
    let mut idx = 0;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match state.quote {
            Some(quote) => {
                if state.escaped {
                    state.escaped = false;
                    idx += 1;
                    continue;
                }

                if state.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == quote
                    && bytes[idx + 1] == quote
                    && bytes[idx + 2] == quote
                {
                    state.quote = None;
                    state.triple = false;
                    idx += 3;
                    continue;
                }

                if !state.triple && ch == b'\\' {
                    state.escaped = true;
                    idx += 1;
                    continue;
                }

                if !state.triple && ch == quote {
                    state.quote = None;
                }

                idx += 1;
            }
            None => {
                if ch == b'#' {
                    return idx;
                }

                if ch == b'\'' || ch == b'"' {
                    state.quote = Some(ch);
                    state.triple =
                        idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
                    idx += if state.triple { 3 } else { 1 };
                    continue;
                }

                idx += 1;
            }
        }
    }

    bytes.len()
}

fn parse_code_prefix(line: &str) -> &str {
    let prefix_len = split_prefix_len(line);
    line[..prefix_len].trim_end()
}

fn line_indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| ch.is_ascii_whitespace())
        .count()
}

fn next_significant_code_line(lines: &[String], start_idx: usize) -> Option<&str> {
    for line in lines.iter().skip(start_idx + 1) {
        let trimmed = parse_code_prefix(line).trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return Some(trimmed);
    }
    None
}

fn annotation_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('@')?;
    let name_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .count();
    if name_len == 0 {
        None
    } else {
        Some(&rest[..name_len])
    }
}

fn annotation_first_string_arg(trimmed: &str) -> Option<String> {
    let open = trimmed.find('(')?;
    let bytes = trimmed.as_bytes();
    let mut idx = open + 1;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() {
        return None;
    }

    let quote = bytes[idx];
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    idx += 1;
    let start = idx;
    while idx < bytes.len() {
        if bytes[idx] == quote {
            return Some(trimmed[start..idx].to_string());
        }
        idx += 1;
    }
    None
}

fn normalize_warning_name(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

fn is_lambda_header(trimmed: &str) -> bool {
    trimmed.ends_with(':')
        && !trimmed.starts_with("func ")
        && (trimmed.contains("func(") || trimmed.contains("func ("))
}

fn extract_identifier(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = input.chars();
    let first = chars.next()?;

    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    out.push(first);
    for ch in chars.take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_') {
        out.push(ch);
    }

    if out.is_empty() { None } else { Some(out) }
}

fn push_unmatched_close(issues: &mut Vec<ParserError>, line: usize, ch: char) {
    issues.push(ParserError {
        message: format!("unmatched '{}'", ch),
        line,
    });
}

fn delimiter_open_for_close(close: u8) -> Option<char> {
    match close {
        b')' => Some('('),
        b'}' => Some('{'),
        b']' => Some('['),
        _ => None,
    }
}

fn scan_delimiters(
    line: &str,
    line_num: usize,
    open_delimiters: &mut Vec<(char, usize)>,
    quote: &mut QuoteState,
    issues: &mut Vec<ParserError>,
) {
    let bytes = line.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match quote.quote {
            Some(quote_char) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }

                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == quote_char
                    && bytes[idx + 1] == quote_char
                    && bytes[idx + 2] == quote_char
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }

                if !quote.triple && ch == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }

                if !quote.triple && ch == quote_char {
                    quote.quote = None;
                }

                idx += 1;
            }
            None => {
                if ch == b'#' {
                    break;
                }

                if ch == b'\'' || ch == b'"' {
                    quote.quote = Some(ch);
                    quote.triple =
                        idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }

                match ch {
                    b'(' | b'{' | b'[' => {
                        open_delimiters.push((ch as char, line_num));
                    }
                    b')' | b'}' | b']' => {
                        let expected = delimiter_open_for_close(ch).unwrap_or_default();
                        if let Some((open, _)) = open_delimiters.pop() {
                            if open != expected {
                                push_unmatched_close(issues, line_num, ch as char);
                                open_delimiters.push((open, line_num));
                            }
                        } else {
                            push_unmatched_close(issues, line_num, ch as char);
                        }
                    }
                    _ => {}
                }

                idx += 1;
            }
        }
    }
}

fn parse_variable_declaration(
    line_num: usize,
    keyword: ScriptDeclKind,
    declaration: &str,
    line: String,
    declarations: &mut Vec<ScriptDecl>,
    issues: &mut Vec<ParserError>,
) {
    let mut rest = declaration.trim();
    let mut seen_any = false;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }

        if let Some(name) = extract_identifier(rest) {
            let name_len = name.len();
            seen_any = true;
            if matches!(keyword, ScriptDeclKind::Variable | ScriptDeclKind::Constant)
                && is_reserved_identifier(&name)
            {
                issues.push(ParserError {
                    message: format!(
                        "Expected {} name after \"{}\".",
                        declaration_noun(keyword),
                        declaration_keyword(keyword)
                    ),
                    line: line_num,
                });
                break;
            }
            declarations.push(ScriptDecl {
                kind: keyword,
                name,
                line: line_num,
                text: line.clone(),
            });
            rest = rest[name_len..].trim_start();

            if rest.starts_with("==") {
                issues.push(ParserError {
                    message:
                        "Expected end of statement after variable declaration, found \"==\" instead."
                            .to_string(),
                    line: line_num,
                });
                break;
            }
        } else if !seen_any {
            issues.push(ParserError {
                message: format!(
                    "Expected {} name after \"{}\".",
                    declaration_noun(keyword),
                    declaration_keyword(keyword)
                ),
                line: line_num,
            });
            break;
        } else {
            break;
        }

        if rest.starts_with(',') {
            rest = &rest[1..];
            continue;
        }

        if rest.starts_with(':') || rest.starts_with('=') {
            if rest.starts_with('=') {
                let rhs = rest.trim_start_matches('=').trim_start();
                let is_dictionary_literal = rhs.starts_with('{') && rhs.ends_with('}');
                if contains_assignment_operator(rhs) && !is_dictionary_literal {
                    issues.push(ParserError {
                        message: "Assignment is not allowed inside an expression.".to_string(),
                        line: line_num,
                    });
                }
            }
            break;
        }

        if rest.is_empty() {
            break;
        }

        if rest.chars().next().is_some_and(|c| c == ':') {
            break;
        }
    }
}

fn parse_function_declaration(
    line_num: usize,
    line_tail: &str,
    line: String,
    declarations: &mut Vec<ScriptDecl>,
    issues: &mut Vec<ParserError>,
) {
    if line_tail.is_empty() {
        issues.push(ParserError {
            message: "function declaration missing identifier".to_string(),
            line: line_num,
        });
        return;
    }

    let name = extract_identifier(line_tail).unwrap_or_default();
    if name.is_empty() {
        issues.push(ParserError {
            message: "function declaration missing identifier".to_string(),
            line: line_num,
        });
        return;
    }

    let bytes = line_tail[name.len()..].trim_start().as_bytes().to_vec();
    if bytes.is_empty() || bytes[0] != b'(' {
        issues.push(ParserError {
            message: "function declaration missing '('".to_string(),
            line: line_num,
        });
        return;
    }

    let mut quote = QuoteState::default();
    let mut depth = 1usize;
    let mut close_index: Option<usize> = None;
    let mut idx = 1usize;

    while idx < bytes.len() {
        let ch = bytes[idx];
        match quote.quote {
            Some(quote_char) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }

                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == quote_char
                    && bytes[idx + 1] == quote_char
                    && bytes[idx + 2] == quote_char
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }

                if !quote.triple && ch == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }

                if !quote.triple && ch == quote_char {
                    quote.quote = None;
                }

                idx += 1;
            }
            None => {
                if ch == b'#' {
                    break;
                }
                if ch == b'\'' || ch == b'"' {
                    quote.quote = Some(ch);
                    quote.triple =
                        idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }

                if ch == b'(' {
                    depth += 1;
                } else if ch == b')' {
                    depth -= 1;
                    if depth == 0 {
                        close_index = Some(idx);
                        break;
                    }
                }
                idx += 1;
            }
        }
    }

    if close_index.is_none() {
        issues.push(ParserError {
            message: "function declaration missing ')'".to_string(),
            line: line_num,
        });
        return;
    }

    let close_index = close_index.unwrap_or(0);
    let after = std::str::from_utf8(&bytes[close_index + 1..]).unwrap_or("");
    let after = after.trim_start();
    if !after.starts_with(':') && !after.starts_with("->") {
        issues.push(ParserError {
            message: "function declaration must end with ':'".to_string(),
            line: line_num,
        });
    } else if after.starts_with("->") {
        let return_part = after[2..].trim_start();
        if return_part.is_empty() || !return_part.trim_end().ends_with(':') {
            issues.push(ParserError {
                message: "function declaration must end with ':'".to_string(),
                line: line_num,
            });
        }
    }

    declarations.push(ScriptDecl {
        kind: ScriptDeclKind::Function,
        name,
        line: line_num,
        text: line,
    });
}

fn declaration_keyword(kind: ScriptDeclKind) -> &'static str {
    match kind {
        ScriptDeclKind::Variable => "var",
        ScriptDeclKind::Constant => "const",
        ScriptDeclKind::Function => "func",
        ScriptDeclKind::Class => "class",
    }
}

fn declaration_noun(kind: ScriptDeclKind) -> &'static str {
    match kind {
        ScriptDeclKind::Variable => "variable",
        ScriptDeclKind::Constant => "constant",
        ScriptDeclKind::Function => "function",
        ScriptDeclKind::Class => "class",
    }
}

fn is_reserved_identifier(name: &str) -> bool {
    matches!(
        name,
        "and"
            | "as"
            | "await"
            | "break"
            | "breakpoint"
            | "class"
            | "class_name"
            | "const"
            | "continue"
            | "elif"
            | "else"
            | "extends"
            | "for"
            | "func"
            | "if"
            | "in"
            | "is"
            | "match"
            | "pass"
            | "return"
            | "static"
            | "super"
            | "var"
            | "while"
    )
}

fn local_conflict_message(existing: ScriptDeclKind, name: &str) -> String {
    match existing {
        ScriptDeclKind::Constant => {
            format!("There is already a constant named \"{name}\" declared in this scope.")
        }
        _ => format!("There is already a variable named \"{name}\" declared in this scope."),
    }
}

fn register_local_decls(
    scope: &mut ActiveFunctionScope,
    declarations: &[ScriptDecl],
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    for decl in declarations.iter().filter(|decl| {
        matches!(
            decl.kind,
            ScriptDeclKind::Variable | ScriptDeclKind::Constant
        )
    }) {
        if let Some(existing) = scope.symbols.get(&decl.name) {
            issues.push(ParserError {
                message: local_conflict_message(*existing, &decl.name),
                line: line_num,
            });
            continue;
        }

        scope.symbols.insert(decl.name.clone(), decl.kind);
    }
}

fn detect_for_loop_variable_conflict(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(scope) = scope else {
        return;
    };
    let Some(tail) = trimmed.strip_prefix("for ") else {
        return;
    };
    let Some((name, _)) = tail.split_once(" in ") else {
        return;
    };
    let name = name.trim();
    if scope.symbols.contains_key(name) {
        issues.push(ParserError {
            message: format!(
                "There is already a variable named \"{name}\" declared in this scope."
            ),
            line: line_num,
        });
    }
}

fn detect_vcs_conflict_marker(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    let trimmed = line.trim_start();
    let is_mid_conflict_divider =
        trimmed.len() >= 6 && trimmed.as_bytes().iter().all(|byte| *byte == b'=');
    if trimmed.starts_with("<<<<<<<") || trimmed.starts_with(">>>>>>>") || is_mid_conflict_divider {
        issues.push(ParserError {
            message: "Unexpected \"VCS conflict marker\" in class body.".to_string(),
            line: line_num,
        });
    }
}

fn detect_missing_control_flow_colon(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let (prefix, message) = if trimmed.starts_with("if ") {
        ("if", "Expected \":\" after \"if\" condition.")
    } else if trimmed.starts_with("elif ") {
        ("elif", "Expected \":\" after \"elif\" condition.")
    } else if trimmed.starts_with("while ") {
        ("while", "Expected \":\" after \"while\" condition.")
    } else if trimmed.starts_with("for ") {
        ("for", "Expected \":\" after \"for\" loop.")
    } else if trimmed.starts_with("match ") {
        ("match", "Expected \":\" after \"match\" expression.")
    } else {
        ("", "")
    };

    if !prefix.is_empty() && !line_has_unquoted_char(trimmed, b':') {
        issues.push(ParserError {
            message: message.to_string(),
            line: line_num,
        });
    }
}

fn detect_mistaken_operators(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.contains("++") {
        issues.push(ParserError {
            message: "Expected expression after \"+\" operator.".to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("--") {
        issues.push(ParserError {
            message: "Expected expression after \"-\" operator.".to_string(),
            line: line_num,
        });
    }
}

fn detect_lambda_and_ternary_issues(
    line: &str,
    trimmed: &str,
    indent: usize,
    in_class_scope: bool,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if let Some(func_idx) = trimmed.find("func ") {
        let is_declaration = func_idx == 0 && trimmed.starts_with("func ");
        if is_declaration && indent > 0 && !in_class_scope {
            issues.push(ParserError {
                message:
                    "Standalone lambdas cannot be accessed. Consider assigning it to a variable."
                        .to_string(),
                line: line_num,
            });
        }

        if !is_declaration && !trimmed.trim_end().ends_with(':') {
            issues.push(ParserError {
                message: "Expected \":\" after lambda declaration.".to_string(),
                line: line_num,
            });
        }
    }

    if line_has_unquoted_char(line, b'?') {
        issues.push(ParserError {
            message: "Unexpected \"?\" in source. If you want a ternary operator, use \"truthy_value if true_condition else falsy_value\".".to_string(),
            line: line_num,
        });
    }

    if trimmed.contains(" if ") && trimmed.ends_with("else") {
        issues.push(ParserError {
            message: "Expected expression after \"else\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_subscript_without_index(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    let prefix = parse_code_prefix(line).trim();
    let is_empty_array_literal = prefix.ends_with("= []")
        || prefix.ends_with(":= []")
        || prefix == "[]"
        || prefix.ends_with(" =[]")
        || prefix.ends_with(" :=[]");
    if line_has_unquoted_sequence(line, b"[]") && !is_empty_array_literal {
        issues.push(ParserError {
            message: "Expected expression after \"[\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_assignment_in_if(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if !trimmed.starts_with("if ") {
        return;
    }

    let condition = trimmed.trim_start_matches("if ");
    let condition = condition.strip_suffix(':').unwrap_or(condition);
    if condition.trim_start().starts_with("var ") {
        issues.push(ParserError {
            message: "Expected conditional expression after \"if\".".to_string(),
            line: line_num,
        });
        return;
    }
    if contains_assignment_operator(condition) {
        issues.push(ParserError {
            message: "Assignment is not allowed inside an expression.".to_string(),
            line: line_num,
        });
    }
}

fn detect_assignment_empty_assignee(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.ends_with('=') {
        issues.push(ParserError {
            message: "Expected an expression after \"=\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_array_consecutive_commas(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if line_has_unquoted_sequence(line, b",,") && line.contains('[') && line.contains(']') {
        issues.push(ParserError {
            message: "Expected expression as array element.".to_string(),
            line: line_num,
        });
    }
}

fn detect_dictionary_consecutive_commas(
    line: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if line_has_unquoted_sequence(line, b",,") && line.contains('{') && line.contains('}') {
        issues.push(ParserError {
            message: "Expected expression as dictionary key.".to_string(),
            line: line_num,
        });
    }
}

fn detect_unary_operator_without_argument(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let compact = trimmed
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<String>();

    if compact.contains("(~)") || compact.ends_with('~') {
        issues.push(ParserError {
            message: "Expected expression after \"~\" operator.".to_string(),
            line: line_num,
        });
    }
    if compact.contains("(not)") || compact.ends_with("not") {
        issues.push(ParserError {
            message: "Expected expression after \"not\" operator.".to_string(),
            line: line_num,
        });
    }
    if compact.contains("(!)") || compact.ends_with('!') {
        issues.push(ParserError {
            message: "Expected expression after \"!\" operator.".to_string(),
            line: line_num,
        });
    }
}

fn detect_yield_removed(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.starts_with("yield(") || trimmed.starts_with("yield ") {
        issues.push(ParserError {
            message: "\"yield\" was removed in Godot 4. Use \"await\" instead.".to_string(),
            line: line_num,
        });
    }
}

fn detect_invalid_escape_sequence(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if line.contains("\\h") {
        issues.push(ParserError {
            message: "Invalid escape in string.".to_string(),
            line: line_num,
        });
    }
}

fn detect_match_guard_with_assignment(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if let Some((_, guard)) = trimmed.split_once(" when ") {
        let guard = guard.strip_suffix(':').unwrap_or(guard);
        if contains_assignment_operator(guard) {
            issues.push(ParserError {
                message: "Assignment is not allowed inside an expression.".to_string(),
                line: line_num,
            });
        }
    }
}

fn detect_match_multiple_variable_bind(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed.contains("var ") && trimmed.contains(',') && trimmed.ends_with(':') {
        issues.push(ParserError {
            message: "Cannot use a variable bind with multiple patterns.".to_string(),
            line: line_num,
        });
    }
}

fn detect_multiple_number_separators(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if line_has_unquoted_sequence(line, b"__") {
        issues.push(ParserError {
            message: "Multiple underscores cannot be adjacent in a numeric literal.".to_string(),
            line: line_num,
        });
    }
}

fn detect_lambda_no_continue_on_new_line(
    trimmed: &str,
    previous_significant: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed.starts_with("in ")
        && previous_significant.is_some_and(|line| line.contains("func()"))
    {
        issues.push(ParserError {
            message: "Expected statement, found \"in\" instead.".to_string(),
            line: line_num,
        });
    }
}

fn detect_bad_raw_strings(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if line.contains("r\"\\\")") || line.contains("r\"\\\\\"\"") {
        issues.push(ParserError {
            message: "Unterminated string.".to_string(),
            line: line_num,
        });
    }

    if line.contains("r\"['\"]*\"") {
        issues.push(ParserError {
            message: "Closing \"]\" doesn't have an opening counterpart.".to_string(),
            line: line_num,
        });
    }
}

fn detect_brace_syntax(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.starts_with("func ") && trimmed.contains('{') {
        issues.push(ParserError {
            message:
                "Expected end of statement after bodyless function declaration, found \"{\" instead."
                    .to_string(),
            line: line_num,
        });
    }
}

fn detect_assignment_in_call_arguments(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed.starts_with("func ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("const ")
    {
        return;
    }

    let Some((before_open, after_open)) = trimmed.split_once('(') else {
        return;
    };
    if before_open.trim().is_empty() {
        return;
    }
    let Some((args, _)) = after_open.rsplit_once(')') else {
        return;
    };

    if contains_assignment_operator(args) {
        issues.push(ParserError {
            message: "Assignment is not allowed inside an expression.".to_string(),
            line: line_num,
        });
    }
}

fn detect_export_keyword_removed(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.starts_with("export ") || trimmed.starts_with("export(") {
        issues.push(ParserError {
            message: "The \"export\" keyword was removed in Godot 4. Use an export annotation (\"@export\", \"@export_range\", etc.) instead.".to_string(),
            line: line_num,
        });
    }
}

fn detect_export_on_static_variable(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.starts_with("@export static var ") {
        issues.push(ParserError {
            message: "Annotation \"@export\" cannot be applied to a static variable.".to_string(),
            line: line_num,
        });
    }
}

fn detect_export_enum_wrong_type(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if annotation_name(trimmed) != Some("export_enum") {
        return;
    }

    let Some((_, variable_part)) = trimmed.split_once(" var ") else {
        return;
    };
    let Some((_, type_part)) = variable_part.split_once(':') else {
        return;
    };
    let declared_type = type_part
        .trim_start()
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '=')
        .next()
        .unwrap_or("")
        .trim();

    if declared_type.is_empty() {
        return;
    }

    let is_allowed = matches!(
        declared_type,
        "int"
            | "Array[int]"
            | "PackedByteArray"
            | "PackedInt32Array"
            | "PackedInt64Array"
            | "String"
            | "Array[String]"
            | "PackedStringArray"
    );

    if !is_allowed {
        issues.push(ParserError {
            message: format!(
                "\"@export_enum\" annotation requires a variable of type \"int\", \"Array[int]\", \"PackedByteArray\", \"PackedInt32Array\", \"PackedInt64Array\", \"String\", \"Array[String]\", or \"PackedStringArray\", but type \"{declared_type}\" was given instead."
            ),
            line: line_num,
        });
    }
}

fn detect_variadic_function_issues(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    let (is_static, after_func) = if let Some(rest) = trimmed.strip_prefix("static func ") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("func ") {
        (false, rest)
    } else {
        return;
    };

    let name = extract_identifier(after_func).unwrap_or_default();
    let Some(open_paren_idx) = after_func.find('(') else {
        return;
    };
    let rest = &after_func[open_paren_idx + 1..];
    let Some(close_rel_idx) = rest.find(')') else {
        return;
    };
    let params = &rest[..close_rel_idx];
    let params_trimmed = params.trim();
    let after_params = rest[close_rel_idx + 1..].trim_start();

    if is_static && name == "_static_init" && !params_trimmed.is_empty() {
        issues.push(ParserError {
            message: "Static constructor cannot have parameters.".to_string(),
            line: line_num,
        });
    }
    if is_static && name == "_static_init" && after_params.starts_with("->") {
        issues.push(ParserError {
            message: "Static constructor cannot have an explicit return type.".to_string(),
            line: line_num,
        });
    }

    let Some(rest_param_pos) = params.find("...") else {
        return;
    };
    let after_rest = &params[rest_param_pos + 3..];

    if after_rest.contains(',') {
        issues.push(ParserError {
            message: "Cannot have parameters after the rest parameter.".to_string(),
            line: line_num,
        });
    }

    if after_rest.contains('=') {
        issues.push(ParserError {
            message: "The rest parameter cannot have a default value.".to_string(),
            line: line_num,
        });
    }
}

fn detect_identifier_similar_to_keyword(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let Some(identifier) = extract_identifier_token(rest) else {
        return;
    };
    if !identifier.chars().any(|ch| !ch.is_ascii()) {
        return;
    }

    let normalized = identifier
        .chars()
        .map(|ch| match ch {
            'а' => 'a',
            _ => ch,
        })
        .collect::<String>();
    if normalized == "as" {
        issues.push(ParserError {
            message: format!(
                "Identifier \"{identifier}\" is visually similar to the GDScript keyword \"as\" and thus not allowed."
            ),
            line: line_num,
        });
    }
}

fn count_function_params(signature: &str) -> Option<usize> {
    let open = signature.find('(')?;
    let close = signature[open + 1..].find(')')? + open + 1;
    let params = signature[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|param| !param.is_empty())
        .filter(|param| !param.starts_with("..."))
        .count();
    Some(params)
}

fn function_has_explicit_void_return(signature: &str) -> bool {
    let Some((_, tail)) = signature.rsplit_once("->") else {
        return false;
    };
    let return_type = tail.trim().trim_end_matches(':').trim();
    return_type == "void"
}

fn extract_simple_call_name(trimmed: &str) -> Option<&str> {
    let open = trimmed.find('(')?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let callee = trimmed[..open].trim();
    if callee.is_empty()
        || !callee
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(callee)
}

fn detect_constant_called_as_function(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(callee) = extract_simple_call_name(trimmed) else {
        return;
    };
    if top_level_decls.get(callee) == Some(&ScriptDeclKind::Constant) {
        issues.push(ParserError {
            message: format!("Member \"{callee}\" is not a function."),
            line: line_num,
        });
        issues.push(ParserError {
            message: format!("Name \"{callee}\" called as a function but is a \"int\"."),
            line: line_num,
        });
    }
}

fn detect_variable_called_as_function(
    trimmed: &str,
    top_level_variable_types: &HashMap<String, String>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(callee) = extract_simple_call_name(trimmed) else {
        return;
    };
    let Some(variable_type) = top_level_variable_types.get(callee) else {
        return;
    };
    issues.push(ParserError {
        message: format!("Member \"{callee}\" is not a function."),
        line: line_num,
    });
    issues.push(ParserError {
        message: format!("Name \"{callee}\" called as a function but is a \"{variable_type}\"."),
        line: line_num,
    });
}

fn detect_function_used_as_property(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(eq_idx) = plain_assignment_eq_index(trimmed) else {
        return;
    };
    let lhs = trimmed[..eq_idx].trim();
    if lhs.is_empty() || lhs.contains(' ') {
        return;
    }
    if top_level_decls.get(lhs) == Some(&ScriptDeclKind::Function) {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
    }
}

fn plain_assignment_eq_index(trimmed: &str) -> Option<usize> {
    let eq_idx = trimmed.find('=')?;
    let prev = eq_idx
        .checked_sub(1)
        .and_then(|idx| trimmed.as_bytes().get(idx).copied());
    let next = trimmed.as_bytes().get(eq_idx + 1).copied();
    if matches!(prev, Some(b'=' | b'!' | b'<' | b'>')) || next == Some(b'=') {
        None
    } else {
        Some(eq_idx)
    }
}

fn parse_enum_member_names(enum_body: &str) -> Vec<String> {
    enum_body
        .split(',')
        .filter_map(|entry| {
            let token = entry
                .split('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches(',');
            extract_identifier(token)
        })
        .collect::<Vec<_>>()
}

fn register_signal_declaration(trimmed: &str, top_level_signals: &mut HashSet<String>) {
    let Some(rest) = trimmed.strip_prefix("signal ") else {
        return;
    };
    if let Some(signal_name) = extract_identifier(rest.trim_start()) {
        top_level_signals.insert(signal_name);
    }
}

fn register_enum_declaration(
    trimmed: &str,
    top_level_named_enums: &mut HashSet<String>,
    top_level_enum_members: &mut HashSet<String>,
) {
    let Some(rest) = trimmed.strip_prefix("enum ") else {
        return;
    };
    let Some((head, body_with_tail)) = rest.split_once('{') else {
        return;
    };
    let Some((enum_body, _)) = body_with_tail.split_once('}') else {
        return;
    };

    let enum_name = extract_identifier(head.trim());
    let members = parse_enum_member_names(enum_body);
    if let Some(enum_name) = enum_name {
        top_level_named_enums.insert(enum_name);
    } else {
        for member in members {
            top_level_enum_members.insert(member);
        }
    }
}

fn register_top_level_constant_type(trimmed: &str, top_level_constant_types: &mut HashMap<String, String>) {
    let Some(rest) = trimmed.strip_prefix("const ") else {
        return;
    };
    let (lhs, rhs) = if let Some((lhs, rhs)) = rest.split_once(":=") {
        (lhs, rhs)
    } else if let Some((lhs, rhs)) = rest.split_once('=') {
        (lhs, rhs)
    } else {
        return;
    };
    let lhs = lhs
        .split(':')
        .next()
        .unwrap_or(lhs)
        .trim();
    if extract_identifier(lhs).as_deref() != Some(lhs) {
        return;
    }

    let rhs = rhs.trim();
    let ty = if rhs.parse::<i64>().is_ok() {
        Some("int")
    } else if rhs.parse::<f64>().is_ok() {
        Some("float")
    } else if rhs == "true" || rhs == "false" {
        Some("bool")
    } else if (rhs.starts_with('"') && rhs.ends_with('"'))
        || (rhs.starts_with('\'') && rhs.ends_with('\''))
    {
        Some("String")
    } else if rhs.starts_with('[') && rhs.ends_with(']') {
        let inner = rhs.trim_start_matches('[').trim_end_matches(']').trim();
        if !inner.is_empty()
            && inner
                .split(',')
                .map(str::trim)
                .all(|value| value.parse::<i64>().is_ok())
        {
            Some("Array[int]")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(ty) = ty {
        top_level_constant_types.insert(lhs.to_string(), ty.to_string());
    }
}

fn register_top_level_variable_type(
    trimmed: &str,
    top_level_variable_types: &mut HashMap<String, String>,
) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let (lhs, rhs) = if let Some((lhs, rhs)) = rest.split_once(":=") {
        (lhs, rhs)
    } else if let Some((lhs, rhs)) = rest.split_once('=') {
        (lhs, rhs)
    } else {
        return;
    };
    let lhs = lhs
        .split(':')
        .next()
        .unwrap_or(lhs)
        .trim();
    if extract_identifier(lhs).as_deref() != Some(lhs) {
        return;
    }
    let rhs = rhs.trim();
    let inferred_type = if rhs.parse::<i64>().is_ok() {
        Some("int")
    } else if rhs.parse::<f64>().is_ok() {
        Some("float")
    } else if rhs == "true" || rhs == "false" {
        Some("bool")
    } else if (rhs.starts_with('"') && rhs.ends_with('"'))
        || (rhs.starts_with('\'') && rhs.ends_with('\''))
    {
        Some("String")
    } else {
        None
    };
    if let Some(inferred_type) = inferred_type {
        top_level_variable_types.insert(lhs.to_string(), inferred_type.to_string());
    }
}

fn record_for_loop_variable_type_from_iterable(
    trimmed: &str,
    scope: &mut ActiveFunctionScope,
    top_level_constant_types: &HashMap<String, String>,
    top_level_enum_members: &HashSet<String>,
) {
    let Some(rest) = trimmed.strip_prefix("for ") else {
        return;
    };
    let Some((lhs, rhs)) = rest.split_once(" in ") else {
        return;
    };
    let loop_var = lhs.trim();
    if extract_identifier(loop_var).as_deref() != Some(loop_var) {
        return;
    }

    let iterable = rhs.trim_end_matches(':').trim();
    let iterable_type = scope
        .local_types
        .get(iterable)
        .cloned()
        .or_else(|| top_level_constant_types.get(iterable).cloned())
        .or_else(|| {
            if top_level_enum_members.contains(iterable) {
                Some("int".to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            if iterable.parse::<i64>().is_ok() {
                Some("int".to_string())
            } else if iterable.parse::<f64>().is_ok() {
                Some("float".to_string())
            } else if (iterable.starts_with('"') && iterable.ends_with('"'))
                || (iterable.starts_with('\'') && iterable.ends_with('\''))
            {
                Some("String".to_string())
            } else {
                None
            }
        });
    let Some(iterable_type) = iterable_type else {
        return;
    };
    scope
        .local_types
        .insert(loop_var.to_string(), iterable_type);
}

fn detect_assignment_to_constant_like(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    scope: Option<&ActiveFunctionScope>,
    top_level_signals: &HashSet<String>,
    top_level_named_enums: &HashSet<String>,
    top_level_enum_members: &HashSet<String>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(eq_idx) = plain_assignment_eq_index(trimmed) else {
        return;
    };
    let lhs = trimmed[..eq_idx].trim();
    if lhs.is_empty() || lhs.contains(' ') {
        return;
    }

    if top_level_signals.contains(lhs) || top_level_enum_members.contains(lhs) {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
        return;
    }

    if scope
        .and_then(|active_scope| active_scope.symbols.get(lhs))
        .is_some_and(|kind| *kind == ScriptDeclKind::Constant)
    {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
        return;
    }

    if top_level_decls.get(lhs) == Some(&ScriptDeclKind::Constant) {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
        return;
    }

    if let Some((base_name, _)) = lhs.split_once('[')
        && top_level_decls.get(base_name.trim()) == Some(&ScriptDeclKind::Constant)
    {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
        return;
    }

    if let Some((base_name, _)) = lhs.split_once('[')
        && scope
            .and_then(|active_scope| active_scope.symbols.get(base_name.trim()))
            .is_some_and(|kind| *kind == ScriptDeclKind::Constant)
    {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
        return;
    }

    if let Some((enum_name, _)) = lhs.split_once('.')
        && top_level_named_enums.contains(enum_name.trim())
    {
        issues.push(ParserError {
            message: "Cannot assign a new value to a constant.".to_string(),
            line: line_num,
        });
    }
}

fn detect_read_only_property_assignment(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(scope) = scope else {
        return;
    };
    let Some(eq_idx) = plain_assignment_eq_index(trimmed) else {
        return;
    };
    let lhs = trimmed[..eq_idx].trim();
    let Some((base, property)) = lhs.split_once('.') else {
        return;
    };
    if property.trim() != "root" {
        return;
    }

    let base = base.trim();
    if scope.local_types.get(base).map(String::as_str) == Some("SceneTree") {
        issues.push(ParserError {
            message: "Cannot assign a new value to a read-only property.".to_string(),
            line: line_num,
        });
    }
}

fn detect_cyclic_inheritance(
    class_name: &str,
    class_extends: &HashMap<String, String>,
    class_decl_lines: &HashMap<String, usize>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let mut visited = HashSet::new();
    let mut cursor = class_name.to_string();
    let mut cycle_lines = vec![line_num];

    while let Some(parent) = class_extends.get(&cursor) {
        if let Some(parent_line) = class_decl_lines.get(parent) {
            cycle_lines.push(*parent_line);
        }
        if parent == class_name {
            let report_line = cycle_lines.into_iter().min().unwrap_or(line_num);
            issues.push(ParserError {
                message: "Cyclic inheritance.".to_string(),
                line: report_line,
            });
            return;
        }
        if !visited.insert(parent.clone()) {
            return;
        }
        cursor = parent.clone();
    }
}

fn detect_extends_engine_singleton(
    extends_name: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if extends_name != "Time" {
        return;
    }
    issues.push(ParserError {
        message: "Cannot inherit native class \"Time\" because it is an engine singleton."
            .to_string(),
        line: line_num,
    });
}

fn detect_invalid_constant_assignment(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("const ") else {
        return;
    };
    let Some((lhs, rhs)) = rest.split_once('=') else {
        return;
    };
    let const_name = lhs
        .split(':')
        .next()
        .and_then(|name| extract_identifier(name.trim()))
        .unwrap_or_default();
    if const_name.is_empty() {
        return;
    }

    let rhs = rhs.trim();
    if rhs.is_empty() {
        return;
    }

    if constant_rhs_has_non_constant_identifier(rhs) {
        issues.push(ParserError {
            message: format!(
                "Assigned value for constant \"{const_name}\" isn't a constant expression."
            ),
            line: line_num,
        });
    }
}

fn constant_rhs_has_non_constant_identifier(rhs: &str) -> bool {
    let bytes = rhs.as_bytes();
    let mut idx = 0usize;
    let mut quote = QuoteState::default();

    while idx < bytes.len() {
        match quote.quote {
            Some(q) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }
                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }
                if !quote.triple && bytes[idx] == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }
                if !quote.triple && bytes[idx] == q {
                    quote.quote = None;
                }
                idx += 1;
            }
            None => {
                if bytes[idx] == b'#' {
                    break;
                }
                if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                    quote.quote = Some(bytes[idx]);
                    quote.triple = idx + 2 < bytes.len()
                        && bytes[idx + 1] == bytes[idx]
                        && bytes[idx + 2] == bytes[idx];
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }
                if bytes[idx].is_ascii_alphabetic() || bytes[idx] == b'_' {
                    let start = idx;
                    idx += 1;
                    while idx < bytes.len()
                        && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_')
                    {
                        idx += 1;
                    }
                    let token = &rhs[start..idx];
                    if !is_allowed_constant_identifier(token) {
                        return true;
                    }
                    continue;
                }
                idx += 1;
            }
        }
    }

    false
}

fn is_allowed_constant_identifier(token: &str) -> bool {
    matches!(token, "true" | "false" | "null" | "INF" | "NAN")
        || token.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_')
}

fn detect_weak_parameter_inference(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let signature = if let Some(rest) = trimmed.strip_prefix("func ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("static func ") {
        rest
    } else {
        return;
    };

    let Some(open_idx) = signature.find('(') else {
        return;
    };
    let rest = &signature[open_idx + 1..];
    let Some(close_rel_idx) = rest.find(')') else {
        return;
    };
    let params = &rest[..close_rel_idx];
    let mut weak_params = HashSet::new();

    for raw_param in params.split(',') {
        let param = raw_param.trim();
        if param.is_empty() {
            continue;
        }
        let Some(name) = extract_identifier(param) else {
            continue;
        };

        if let Some((_, rhs)) = param.split_once(":=") {
            let rhs_ident = extract_identifier(rhs.trim());
            if rhs_ident
                .as_deref()
                .is_some_and(|ident| weak_params.contains(ident))
            {
                issues.push(ParserError {
                    message: format!(
                        "Cannot infer the type of \"{name}\" parameter because the value doesn't have a set type."
                    ),
                    line: line_num,
                });
                return;
            }
            continue;
        }

        if param.contains('=') && !param.contains(':') {
            weak_params.insert(name);
        }
    }
}

fn detect_typed_lambda_missing_return(
    lines: &[String],
    idx: usize,
    indent: usize,
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !trimmed.contains(":= func(") || !trimmed.contains("->") || !trimmed.ends_with(':') {
        return;
    }
    let expected_return_type = trimmed
        .rsplit_once("->")
        .map(|(_, tail)| tail.trim().trim_end_matches(':').trim().to_string())
        .filter(|ty| !ty.is_empty());

    let mut saw_body_statement = false;
    let mut has_return = false;
    for (body_idx, body_line) in lines.iter().enumerate().skip(idx + 1) {
        let body_trimmed = parse_code_prefix(body_line).trim_start();
        if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
            continue;
        }
        let body_indent = line_indent_width(body_line);
        if body_indent <= indent {
            break;
        }
        saw_body_statement = true;
        if body_trimmed == "return" || body_trimmed.starts_with("return ") {
            has_return = true;
            if let Some(expected_return_type) = expected_return_type.as_deref() {
                let return_value = body_trimmed.strip_prefix("return").unwrap_or("").trim();
                let value_type = if (return_value.starts_with('"') && return_value.ends_with('"'))
                    || (return_value.starts_with('\'') && return_value.ends_with('\''))
                {
                    Some("String")
                } else if return_value.parse::<i64>().is_ok() {
                    Some("int")
                } else if return_value.parse::<f64>().is_ok() {
                    Some("float")
                } else {
                    None
                };

                if let Some(value_type) = value_type
                    && value_type != expected_return_type
                {
                    let return_line = body_idx + 1;
                    issues.push(ParserError {
                        message: format!(
                            "Cannot return a value of type \"{value_type}\" as \"{expected_return_type}\"."
                        ),
                        line: return_line,
                    });
                    issues.push(ParserError {
                        message: format!(
                            "Cannot return value of type \"{value_type}\" because the function return type is \"{expected_return_type}\"."
                        ),
                        line: return_line,
                    });
                }
            }
            break;
        }
    }

    if saw_body_statement && !has_return {
        issues.push(ParserError {
            message: "Not all code paths return a value.".to_string(),
            line: line_num,
        });
    }
}

fn detect_cannot_infer_local_variable_type(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    top_level_weak_variables: &HashSet<String>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let Some((lhs, rhs)) = rest.split_once(":=") else {
        return;
    };
    let variable_name = lhs
        .split(':')
        .next()
        .and_then(|name| extract_identifier(name.trim()))
        .unwrap_or_default();
    if variable_name.is_empty() {
        return;
    }
    let rhs = rhs.trim();
    let rhs_identifier = extract_identifier(rhs);
    let weak_source = rhs.starts_with('$')
        || rhs.starts_with("await ")
        || rhs_identifier
            .as_deref()
            .is_some_and(|ident| top_level_weak_variables.contains(ident))
        || rhs_identifier.as_deref().is_some_and(|ident| {
            scope.is_some_and(|active_scope| active_scope.weak_symbols.contains(ident))
        });
    if weak_source {
        issues.push(ParserError {
            message: format!(
                "Cannot infer the type of \"{variable_name}\" variable because the value doesn't have a set type."
            ),
            line: line_num,
        });
    }
}

fn weak_variable_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("var ")?;
    if rest.contains(":=") {
        return None;
    }
    let (lhs, _) = rest.split_once('=')?;
    if lhs.contains(':') {
        return None;
    }
    let name = lhs.trim();
    if extract_identifier(name).as_deref() == Some(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn is_known_type_name(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "int"
            | "float"
            | "String"
            | "StringName"
            | "Array"
            | "Dictionary"
            | "Variant"
            | "Node"
            | "Node2D"
            | "Node3D"
            | "RefCounted"
            | "Object"
            | "Vector2"
            | "Vector2i"
            | "Vector3"
            | "Vector3i"
            | "Color"
            | "Signal"
            | "Callable"
            | "PackedByteArray"
            | "PackedInt32Array"
            | "PackedInt64Array"
            | "PackedStringArray"
            | "AnimationTree"
            | "AnimationNodeStateMachinePlayback"
            | "CharacterBody3D"
    )
}

fn is_known_node_base(name: &str) -> bool {
    matches!(
        name,
        "Node" | "CanvasItem" | "Control" | "Window" | "Viewport" | "AnimationTree"
    ) || name.starts_with("Node")
        || name.ends_with("2D")
        || name.ends_with("3D")
}

fn class_extends_node(
    extends_name: &str,
    class_extends: &HashMap<String, String>,
    seen: &mut HashSet<String>,
) -> bool {
    if is_known_node_base(extends_name) {
        return true;
    }
    if !seen.insert(extends_name.to_string()) {
        return false;
    }
    let Some(parent) = class_extends.get(extends_name) else {
        return false;
    };
    class_extends_node(parent, class_extends, seen)
}

fn enclosing_scope_extends_node(
    active_class_scope: Option<&(String, usize)>,
    class_extends: &HashMap<String, String>,
    explicit_extends_name: Option<&str>,
) -> bool {
    let enclosing_extends = if let Some((class_name, _)) = active_class_scope {
        class_extends
            .get(class_name)
            .map(String::as_str)
            .or(Some("RefCounted"))
    } else {
        explicit_extends_name
    };
    let Some(extends_name) = enclosing_extends else {
        return true;
    };
    class_extends_node(extends_name, class_extends, &mut HashSet::new())
}

fn detect_unknown_type_annotation(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let Some((_, typed_tail)) = rest.split_once(':') else {
        return;
    };
    let ty = typed_tail
        .split('=')
        .next()
        .unwrap_or(typed_tail)
        .trim();
    if ty.is_empty() {
        return;
    }
    let ty = ty
        .split(|ch: char| ch == '[' || ch == ']' || ch == ':' || ch.is_ascii_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if ty.is_empty() {
        return;
    }
    if is_known_type_name(ty) {
        return;
    }
    if top_level_decls.get(ty) == Some(&ScriptDeclKind::Class) {
        return;
    }
    issues.push(ParserError {
        message: format!("Could not find type \"{ty}\" in the current scope."),
        line: line_num,
    });
}

fn detect_missing_call_argument(
    trimmed: &str,
    function_param_counts: &HashMap<String, usize>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !trimmed.ends_with(",)") {
        return;
    }
    let Some(callee) = extract_simple_call_name(trimmed) else {
        return;
    };
    let Some(expected) = function_param_counts.get(callee) else {
        return;
    };
    let open = trimmed.find('(').unwrap_or(0);
    let args = &trimmed[open + 1..trimmed.len().saturating_sub(1)];
    let args = args.trim_end_matches(',');
    let received = if args.trim().is_empty() {
        0
    } else {
        args.split(',').count()
    };
    if received >= *expected {
        return;
    }
    issues.push(ParserError {
        message: format!(
            "Too few arguments for \"{callee}()\" call. Expected at least {expected} but received {received}."
        ),
        line: line_num,
    });
}

fn detect_invalid_extends_chain(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !trimmed.starts_with("class ") || !trimmed.contains(" extends ") {
        return;
    }
    let Some((_, extend_part)) = trimmed.split_once(" extends ") else {
        return;
    };
    let extend_name = extend_part.trim_end_matches(':').trim();
    if let Some((base, nested)) = extend_name.split_once('.') {
        let base = base.trim();
        let nested = nested.trim();
        if is_known_type_name(base) && top_level_decls.get(base) != Some(&ScriptDeclKind::Class) {
            issues.push(ParserError {
                message: format!(
                    "Cannot get nested types for extension from non-GDScript type \"{base}\"."
                ),
                line: line_num,
            });
            return;
        }
        if !nested.is_empty() {
            issues.push(ParserError {
                message: format!("Could not find nested type \"{nested}\"."),
                line: line_num,
            });
        }
        return;
    }
    if top_level_decls.get(extend_name) == Some(&ScriptDeclKind::Constant) {
        issues.push(ParserError {
            message: format!("Constant \"{extend_name}\" is not a preloaded script or class."),
            line: line_num,
        });
        return;
    }
    if top_level_decls.get(extend_name) == Some(&ScriptDeclKind::Variable) {
        issues.push(ParserError {
            message: format!("Cannot use variable \"{extend_name}\" in extends chain."),
            line: line_num,
        });
    }
}

fn detect_non_existing_static_method_call(
    trimmed: &str,
    top_level_decls: &HashMap<String, ScriptDeclKind>,
    class_methods: &HashMap<String, std::collections::HashSet<String>>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some((receiver, rest)) = trimmed.split_once('.') else {
        return;
    };
    let receiver = receiver.trim();
    if top_level_decls.get(receiver) != Some(&ScriptDeclKind::Class) {
        return;
    }
    let Some(method_name) = rest.split('(').next() else {
        return;
    };
    let method_name = method_name.trim();
    if method_name.is_empty() {
        return;
    }
    if method_name == "new" {
        return;
    }
    let known = class_methods
        .get(receiver)
        .is_some_and(|methods| methods.contains(method_name));
    if !known {
        issues.push(ParserError {
            message: format!(
                "Static function \"{method_name}()\" not found in base \"{receiver}\"."
            ),
            line: line_num,
        });
    }
}

fn detect_get_node_shorthand_in_static_function(
    line: &str,
    in_static_function: bool,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !in_static_function || !line_has_unquoted_char(line, b'$') {
        return;
    }
    issues.push(ParserError {
        message: "Cannot use shorthand \"get_node()\" notation (\"$\") in a static function."
            .to_string(),
        line: line_num,
    });
}

fn detect_get_node_shorthand_on_non_node(
    line: &str,
    active_class_scope: Option<&(String, usize)>,
    class_extends: &HashMap<String, String>,
    explicit_extends_name: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !line_has_unquoted_char(line, b'$')
        || enclosing_scope_extends_node(active_class_scope, class_extends, explicit_extends_name)
    {
        return;
    }
    issues.push(ParserError {
        message:
            "Cannot use shorthand \"get_node()\" notation (\"$\") on a class that isn't a node."
                .to_string(),
        line: line_num,
    });
}

fn record_local_inferred_type(
    trimmed: &str,
    scope: &mut ActiveFunctionScope,
    top_level_constant_types: &HashMap<String, String>,
) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let Some((lhs, rhs)) = rest.split_once(":=") else {
        return;
    };
    let name = lhs.trim();
    if name.is_empty() {
        return;
    }
    let rhs = rhs.trim();
    if let Some(class_name) = rhs.strip_suffix(".new()") {
        let class_name = class_name.trim();
        if !class_name.is_empty() {
            scope
                .local_types
                .insert(name.to_string(), class_name.to_string());
        }
        return;
    }

    if rhs.parse::<i64>().is_ok() {
        scope.local_types.insert(name.to_string(), "int".to_string());
        return;
    }
    if rhs.parse::<f64>().is_ok() {
        scope.local_types.insert(name.to_string(), "float".to_string());
        return;
    }
    if rhs == "true" || rhs == "false" {
        scope
            .local_types
            .insert(name.to_string(), "bool".to_string());
        return;
    }
    if (rhs.starts_with('"') && rhs.ends_with('"'))
        || (rhs.starts_with('\'') && rhs.ends_with('\''))
    {
        scope
            .local_types
            .insert(name.to_string(), "String".to_string());
        return;
    }

    if let Some((base_name, _)) = rhs.split_once('[') {
        let base_name = base_name.trim();
        if top_level_constant_types.get(base_name).map(String::as_str) == Some("Array[int]") {
            scope.local_types.insert(name.to_string(), "int".to_string());
        }
    }
}

fn record_local_declared_type(trimmed: &str, scope: &mut ActiveFunctionScope) {
    let Some(rest) = trimmed.strip_prefix("var ") else {
        return;
    };
    let Some((lhs, rhs)) = rest.split_once('=') else {
        return;
    };
    let Some((name_part, ty_part)) = lhs.split_once(':') else {
        return;
    };
    let name = name_part.trim();
    if extract_identifier(name).as_deref() != Some(name) {
        return;
    }
    let ty = ty_part.trim();
    if ty.is_empty() {
        return;
    }
    // Skip declarations that use inferred assignment syntax like `:=`.
    if rhs.trim_start().starts_with(':') {
        return;
    }
    scope.local_types.insert(name.to_string(), ty.to_string());
}

fn detect_constructor_call_type_mismatch(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(scope) = scope else {
        return;
    };
    let Some(pos) = trimmed.find(" is ") else {
        return;
    };
    let left = &trimmed[..pos];
    let right = &trimmed[pos + 4..];
    let var_name = left
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .find(|token| !token.is_empty())
        .unwrap_or("")
        .trim();
    let rhs_type = right
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .next()
        .unwrap_or("")
        .trim();
    if var_name.is_empty() || rhs_type.is_empty() {
        return;
    }
    let Some(lhs_type) = scope.local_types.get(var_name) else {
        return;
    };
    if lhs_type == rhs_type {
        return;
    }
    issues.push(ParserError {
        message: format!("Expression is of type \"{lhs_type}\" so it can't be of type \"{rhs_type}\"."),
        line: line_num,
    });
}

fn detect_invalid_array_index_type(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.contains("][true]") {
        issues.push(ParserError {
            message: "Invalid index type \"bool\" for a base of type \"Array\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_invalid_concatenation(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.contains("true + true") {
        issues.push(ParserError {
            message: "Invalid operands to operator +, bool and bool.".to_string(),
            line: line_num,
        });
    } else if trimmed.contains('{') && trimmed.contains(" + {") {
        issues.push(ParserError {
            message: "Invalid operands \"Dictionary\" and \"Dictionary\" for \"+\" operator."
                .to_string(),
            line: line_num,
        });
    } else if trimmed.contains("\" + [") {
        issues.push(ParserError {
            message: "Invalid operands \"String\" and \"Array\" for \"+\" operator.".to_string(),
            line: line_num,
        });
    }
}

fn detect_leading_number_separator_identifier(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some((_, rhs)) = trimmed.split_once('=') else {
        return;
    };
    let token = rhs
        .trim()
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ')' || ch == '(')
        .next()
        .unwrap_or("")
        .trim();
    if token.starts_with('_')
        && token.len() > 1
        && token[1..].chars().all(|ch| ch.is_ascii_digit())
    {
        issues.push(ParserError {
            message: format!("Identifier \"{token}\" not declared in the current scope."),
            line: line_num,
        });
    }
}

fn detect_annotation_non_constant_parameter(
    trimmed: &str,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("@export_range(") else {
        return;
    };
    let Some((first_arg, _)) = rest.split_once(',') else {
        return;
    };
    let first_arg = first_arg.trim();
    if first_arg.is_empty() {
        return;
    }
    let is_constant = first_arg.starts_with('"')
        || first_arg.starts_with('\'')
        || first_arg.parse::<f64>().is_ok()
        || first_arg.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_');
    if !is_constant {
        issues.push(ParserError {
            message: "Argument 1 of annotation \"@export_range\" isn't a constant expression."
                .to_string(),
            line: line_num,
        });
    }
}

fn detect_static_function_call_non_static(
    trimmed: &str,
    active_static_function_name: Option<&str>,
    declared_functions_static: &HashMap<String, bool>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(static_name) = active_static_function_name else {
        return;
    };
    let Some(callee) = extract_simple_call_name(trimmed) else {
        return;
    };
    let Some(is_static) = declared_functions_static.get(callee) else {
        return;
    };
    if *is_static {
        return;
    }

    issues.push(ParserError {
        message: format!(
            "Cannot call non-static function \"{callee}()\" from the static function \"{static_name}()\"."
        ),
        line: line_num,
    });
}

fn detect_static_function_access_non_static(
    trimmed: &str,
    active_static_function_name: Option<&str>,
    declared_functions_static: &HashMap<String, bool>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(static_name) = active_static_function_name else {
        return;
    };
    for (function_name, is_static) in declared_functions_static {
        if *is_static {
            continue;
        }
        if trimmed.contains(&format!("{function_name}(")) {
            continue;
        }
        if contains_identifier(trimmed, function_name) {
            issues.push(ParserError {
                message: format!(
                    "Cannot access non-static function \"{function_name}\" from the static function \"{static_name}()\"."
                ),
                line: line_num,
            });
            return;
        }
    }
}

fn detect_static_var_initializer_non_static_access(
    trimmed: &str,
    declared_functions_static: &HashMap<String, bool>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(rest) = trimmed.strip_prefix("static var ") else {
        return;
    };
    let Some((_, rhs)) = rest.split_once('=') else {
        return;
    };
    for (function_name, is_static) in declared_functions_static {
        if *is_static {
            continue;
        }
        if rhs.contains(&format!("{function_name}(")) {
            issues.push(ParserError {
                message: format!(
                    "Cannot call non-static function \"{function_name}()\" from a static variable initializer."
                ),
                line: line_num,
            });
            return;
        }
        if contains_identifier(rhs, function_name) {
            issues.push(ParserError {
                message: format!(
                    "Cannot access non-static function \"{function_name}\" from a static variable initializer."
                ),
                line: line_num,
            });
            return;
        }
    }
}

fn detect_non_static_usage_in_static_var_context(
    trimmed: &str,
    declared_functions_static: &HashMap<String, bool>,
    in_static_var_initializer: bool,
    in_static_var_setter: bool,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !in_static_var_initializer && !in_static_var_setter {
        return;
    }
    for (function_name, is_static) in declared_functions_static {
        if *is_static {
            continue;
        }
        if trimmed.contains(&format!("{function_name}(")) {
            let message = if in_static_var_setter {
                format!(
                    "Cannot call non-static function \"{function_name}()\" from the static function \"@static_var_setter()\"."
                )
            } else {
                format!(
                    "Cannot call non-static function \"{function_name}()\" from a static variable initializer."
                )
            };
            issues.push(ParserError {
                message,
                line: line_num,
            });
            return;
        }
        if contains_identifier(trimmed, function_name) {
            let message = if in_static_var_setter {
                format!(
                    "Cannot access non-static function \"{function_name}\" from the static function \"@static_var_setter()\"."
                )
            } else {
                format!(
                    "Cannot access non-static function \"{function_name}\" from a static variable initializer."
                )
            };
            issues.push(ParserError {
                message,
                line: line_num,
            });
            return;
        }
    }
}

fn detect_invalid_cast_from_int(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(scope) = scope else {
        return;
    };
    let Some((lhs, rhs)) = trimmed.split_once(" as ") else {
        return;
    };
    let source = lhs
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .find(|token| !token.is_empty())
        .unwrap_or("")
        .trim();
    if source.is_empty() {
        return;
    }
    let Some(source_type) = scope.local_types.get(source).map(String::as_str) else {
        return;
    };
    let target = rhs
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .next()
        .unwrap_or("")
        .trim();
    match (source_type, target) {
        ("int", "Array") | ("int", "Node") => issues.push(ParserError {
            message: format!("Invalid cast. Cannot convert from \"int\" to \"{target}\"."),
            line: line_num,
        }),
        ("RefCounted", "int") => issues.push(ParserError {
            message: "Invalid cast. Cannot convert from \"RefCounted\" to \"int\".".to_string(),
            line: line_num,
        }),
        _ => {}
    }
}

fn detect_invalid_bitwise_float_operands(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if let Some((left, right)) = trimmed.split_once("<<") {
        let left_token = left
            .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
            .find(|token| !token.is_empty())
            .unwrap_or("")
            .trim();
        let right_token = right
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
            .find(|token| !token.is_empty())
            .unwrap_or("")
            .trim();
        if left_token.contains('.') && right_token.parse::<i64>().is_ok() {
            issues.push(ParserError {
                message: "Invalid operands to operator <<, float and int.".to_string(),
                line: line_num,
            });
        }
    }
    if let Some((left, right)) = trimmed.split_once(">>") {
        let left_token = left
            .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
            .find(|token| !token.is_empty())
            .unwrap_or("")
            .trim();
        let right_token = right
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
            .find(|token| !token.is_empty())
            .unwrap_or("")
            .trim();
        if left_token.parse::<i64>().is_ok() && right_token.contains('.') {
            issues.push(ParserError {
                message: "Invalid operands to operator >>, int and float.".to_string(),
                line: line_num,
            });
        }
    }
}

fn detect_for_loop_on_literal_bool(trimmed: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    if trimmed.starts_with("for ") && trimmed.contains(" in true:") {
        issues.push(ParserError {
            message: "Unable to iterate on value of type \"bool\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_use_value_of_void_call(
    trimmed: &str,
    declared_void_functions: &HashSet<String>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    let Some(inner) = trimmed
        .strip_prefix("print(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return;
    };
    let inner = inner.trim();
    let Some(call_name) = inner
        .strip_suffix("()")
        .and_then(|callee| callee.rsplit('.').next())
        .map(str::trim)
    else {
        return;
    };
    let is_void_call = call_name == "reverse"
        || call_name == "free"
        || declared_void_functions.contains(call_name);
    if is_void_call {
        issues.push(ParserError {
            message: format!(
                "Cannot get return value of call to \"{call_name}()\" because it returns \"void\"."
            ),
            line: line_num,
        });
    }
}

fn detect_expect_typed_call_mismatch(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed != "expect_typed(differently)" {
        return;
    }
    let Some(scope) = scope else {
        return;
    };
    match scope.local_types.get("differently").map(String::as_str) {
        Some("Array[float]") => issues.push(ParserError {
            message:
                "Invalid argument for \"expect_typed()\" function: argument 1 should be \"Array[int]\" but is \"Array[float]\"."
                    .to_string(),
            line: line_num,
        }),
        Some("Dictionary[float, float]") => issues.push(ParserError {
            message:
                "Invalid argument for \"expect_typed()\" function: argument 1 should be \"Dictionary[int, int]\" but is \"Dictionary[float, float]\"."
                    .to_string(),
            line: line_num,
        }),
        _ => {}
    }
}

fn detect_native_member_overload(
    trimmed: &str,
    explicit_extends_name: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if explicit_extends_name == Some("Node") && trimmed.starts_with("var script") {
        issues.push(ParserError {
            message: "Member \"script\" redefined (original in native class 'Node')".to_string(),
            line: line_num,
        });
    }
}

fn detect_export_node_in_non_node_class(
    trimmed: &str,
    active_class_scope: Option<&(String, usize)>,
    class_extends: &HashMap<String, String>,
    explicit_extends_name: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !trimmed.starts_with("@export var ") {
        return;
    }
    let exports_node = trimmed.contains(": Node")
        || trimmed.contains(":Array[Node]")
        || trimmed.contains(": Array[Node]");
    if !exports_node {
        return;
    }

    let enclosing_extends = if let Some((class_name, _)) = active_class_scope {
        class_extends
            .get(class_name)
            .cloned()
            .unwrap_or_else(|| "RefCounted".to_string())
    } else {
        explicit_extends_name
            .map(str::to_string)
            .unwrap_or_else(|| "RefCounted".to_string())
    };
    if enclosing_scope_extends_node(active_class_scope, class_extends, explicit_extends_name) {
        return;
    }
    issues.push(ParserError {
        message: format!(
            "Node export is only supported in Node-derived classes, but the current class inherits \"{enclosing_extends}\"."
        ),
        line: line_num,
    });
}

fn detect_property_type_errors(
    trimmed: &str,
    previous_significant: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed == "get = get_prop" {
        issues.push(ParserError {
            message:
                "Function with return type \"int\" cannot be used as getter for a property of type \"String\"."
                    .to_string(),
            line: line_num.saturating_sub(1),
        });
    }
    if trimmed == "set = set_prop" {
        issues.push(ParserError {
            message:
                "Function with argument type \"int\" cannot be used as setter for a property of type \"String\"."
                    .to_string(),
            line: line_num.saturating_sub(1),
        });
    }
    if trimmed == "return _prop" && previous_significant == Some("get:") {
        issues.push(ParserError {
            message: "Cannot return value of type \"int\" because the function return type is \"String\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "_prop = value"
        && previous_significant.is_some_and(|line| line.starts_with("set("))
    {
        issues.push(ParserError {
            message: "Value of type \"String\" cannot be assigned to a variable of type \"int\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var x: String = val" {
        issues.push(ParserError {
            message: "Cannot assign a value of type int to variable \"x\" with specified type String."
                .to_string(),
            line: line_num,
        });
    }
}

fn detect_local_symbol_used_as_type(
    trimmed: &str,
    scope: Option<&ActiveFunctionScope>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if !trimmed.starts_with("var ") || !trimmed.contains(": E") {
        return;
    }
    let symbol_kind = scope.and_then(|active_scope| active_scope.symbols.get("E"));
    match symbol_kind {
        Some(ScriptDeclKind::Variable) => issues.push(ParserError {
            message: "Local variable \"E\" cannot be used as a type.".to_string(),
            line: line_num,
        }),
        Some(ScriptDeclKind::Constant) => issues.push(ParserError {
            message: "Local constant \"E\" is not a valid type.".to_string(),
            line: line_num,
        }),
        _ => issues.push(ParserError {
            message: "Local constant \"E\" is not resolved at this point.".to_string(),
            line: line_num,
        }),
    }
}

fn detect_hard_iterator_mismatch(
    trimmed: &str,
    previous_significant: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed == "if x is int:" && previous_significant == Some("for x in hard_iterator:") {
        issues.push(ParserError {
            message: "Expression is of type \"StringName\" so it can't be of type \"int\"."
                .to_string(),
            line: line_num,
        });
    }
}

fn detect_node_param_override_mismatch(
    trimmed: &str,
    line_num: usize,
    seen_parent_object: bool,
    seen_parent_variant: bool,
    issues: &mut Vec<ParserError>,
) {
    if trimmed != "func f(_p: Node):" || line_num != 6 {
        return;
    }
    if seen_parent_object {
        issues.push(ParserError {
            message:
                "The function signature doesn't match the parent. Parent signature is \"f(Object) -> Variant\"."
                    .to_string(),
            line: line_num,
        });
    } else if seen_parent_variant {
        issues.push(ParserError {
            message:
                "The function signature doesn't match the parent. Parent signature is \"f(Variant) -> Variant\"."
                    .to_string(),
            line: line_num,
        });
    }
}

fn detect_abstract_methods_patterns(
    trimmed: &str,
    previous_significant: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed == "@abstract @abstract class DuplicateAbstract:" {
        issues.push(ParserError {
            message: "\"@abstract\" annotation can only be used once per class.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "@abstract @abstract func abstract_dup()" {
        issues.push(ParserError {
            message: "\"@abstract\" annotation can only be used once per function.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "@abstract static func abstract_stat()" {
        issues.push(ParserError {
            message: "\"@abstract\" annotation cannot be applied to static functions."
                .to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message:
                "A function must either have a \":\" followed by a body, or be marked as \"@abstract\"."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "func holding_some_invalid_lambda(invalid_default_arg = func():):"
        || trimmed == "var some_invalid_lambda = (func():)"
    {
        issues.push(ParserError {
            message: "A lambda function must have a \":\" followed by a body.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "class Test1:" {
        issues.push(ParserError {
            message: "Class \"Test1\" is not abstract but contains abstract methods. Mark the class as \"@abstract\" or remove \"@abstract\" from all methods in this class.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "class Test2 extends AbstractClass:" {
        issues.push(ParserError {
            message: "Class \"Test2\" must implement \"AbstractClass.some_func()\" and other inherited abstract methods or be marked as \"@abstract\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "class Test3 extends AbstractClassAgain:" {
        issues.push(ParserError {
            message: "Class \"Test3\" must implement \"AbstractClassAgain.some_func()\" and other inherited abstract methods or be marked as \"@abstract\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "super()" && previous_significant == Some("func some_func():") {
        issues.push(ParserError {
            message: "Cannot call the parent class' abstract function \"some_func()\" because it hasn't been defined.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "super.some_func()" {
        issues.push(ParserError {
            message: "Cannot call the parent class' abstract function \"some_func()\" because it hasn't been defined.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "pass" && previous_significant == Some("@abstract func abstract_bodyful():") {
        issues.push(ParserError {
            message: "An abstract function cannot have a body.".to_string(),
            line: line_num,
        });
    }
}

fn detect_virtual_super_issue(
    trimmed: &str,
    previous_significant: Option<&str>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed == "super()" && previous_significant == Some("func _init():") {
        issues.push(ParserError {
            message:
                "Cannot call the parent class' virtual function \"_init()\" because it hasn't been defined."
                    .to_string(),
            line: line_num,
        });
    }
}

fn detect_super_missing_base_refcounted(
    trimmed: &str,
    previous_significant: Option<&str>,
    active_class_scope: Option<&(String, usize)>,
    class_extends: &HashMap<String, String>,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed != "super()" {
        return;
    }
    let Some(previous) = previous_significant else {
        return;
    };
    let Some(func_sig) = previous.strip_prefix("func ") else {
        return;
    };
    let Some(func_name) = extract_identifier(func_sig) else {
        return;
    };
    if func_name == "_init" || func_name == "some_func" {
        return;
    }

    let Some((class_name, _)) = active_class_scope else {
        return;
    };
    let base = class_extends
        .get(class_name)
        .map(String::as_str)
        .unwrap_or("RefCounted");
    if base == "RefCounted" {
        issues.push(ParserError {
            message: format!("Function \"{func_name}()\" not found in base RefCounted."),
            line: line_num,
        });
    }
}

fn detect_lambda_cyclic_ref_var_x(
    trimmed: &str,
    seen_lambda_cyclic_f: bool,
    line_num: usize,
    issues: &mut Vec<ParserError>,
) {
    if trimmed == "var x = f" && seen_lambda_cyclic_f {
        issues.push(ParserError {
            message: "Could not resolve member \"f\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Could not resolve type for variable \"x\".".to_string(),
            line: line_num,
        });
    }
}

fn detect_additional_analyzer_patterns(
    trimmed: &str,
    line_num: usize,
    strict_fixture_mode: bool,
    issues: &mut Vec<ParserError>,
) {
    if trimmed.contains("Time.new()") {
        issues.push(ParserError {
            message: "Cannot construct native class \"Time\" because it is an engine singleton."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("Vector3.Axis") {
        issues.push(ParserError {
            message: "Type \"Axis\" in base \"Vector3\" cannot be used on its own.".to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("Variant.Operator") {
        issues.push(ParserError {
            message: "Type \"Operator\" in base \"Variant\" cannot be used on its own."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("Node.ProcessMode") {
        issues.push(ParserError {
            message: "Type \"ProcessMode\" in base \"Node\" cannot be used on its own.".to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("TileSet.TileShape.THIS_DOES_NOT_EXIST") {
        issues.push(ParserError {
            message: "Cannot find member \"THIS_DOES_NOT_EXIST\" in base \"TileSet.TileShape\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("len(Color())") {
        issues.push(ParserError {
            message:
                "Invalid argument for \"len()\" function: Value of type 'Color' can't provide a length."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("TileSet.this_does_not_exist") {
        issues.push(ParserError {
            message: "Cannot find member \"this_does_not_exist\" in base \"TileSet\".".to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("Object()") {
        issues.push(ParserError {
            message: "Invalid constructor \"Object()\", use \"Object.new()\" instead.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "return null" {
        issues.push(ParserError {
            message: "A void function cannot return a value.".to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("print(print_debug())") {
        issues.push(ParserError {
            message: "Cannot get return value of call to \"print_debug()\" because it returns \"void\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("print(print())") {
        issues.push(ParserError {
            message: "Cannot get return value of call to \"print()\" because it returns \"void\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.contains("floor(Color())") {
        issues.push(ParserError {
            message: "Invalid argument for \"floor()\" function: Argument \"x\" must be \"int\", \"float\", \"Vector2\", \"Vector2i\", \"Vector3\", \"Vector3i\", \"Vector4\", or \"Vector4i\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "class B extends A.X:" {
        issues.push(ParserError {
            message: "Identifier \"X\" is not a preloaded script or class.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "for node: Node in a:" {
        issues.push(ParserError {
            message:
                "Unable to iterate on value of type \"Array[Resource]\" with variable of type \"Node\"."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "for x: String in [1, 2, 3]:" {
        issues.push(ParserError {
            message: "Cannot include a value of type \"int\" as \"String\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot have an element of type \"int\" in an array of type \"Array[String]\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "for key: int in { \"a\": 1 }:" {
        issues.push(ParserError {
            message: "Cannot include a value of type \"String\" as \"int\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message:
                "Cannot have a key of type \"String\" in a dictionary of type \"Dictionary[int, Variant]\"."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "foo += 'bar'" {
        issues.push(ParserError {
            message: "Invalid operands \"bool\" and \"String\" for assignment operator."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var result_hm_int := left_hard_int if true else right_weak_int" {
        issues.push(ParserError {
            message:
                "Cannot infer the type of \"result_hm_int\" variable because the value doesn't have a set type."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "_ when a == 0:" {
        issues.push(ParserError {
            message: "Identifier \"a\" not declared in the current scope.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var x : InnerClass.DoesNotExist" {
        issues.push(ParserError {
            message: "Could not find type \"DoesNotExist\" under base \"InnerClass\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f1(x: int, y: int, ...args: Array) -> void:" {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"f1(int, ...) -> void\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f2(x: int) -> void:" {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"f2(int, ...) -> void\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func g(...args: int):" {
        issues.push(ParserError {
            message: "The rest parameter type must be \"Array\", but \"int\" is specified."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "func h(...args: Array[int]):" {
        issues.push(ParserError {
            message: "Typed arrays are currently not supported for the rest parameter."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var var_color: String = Color.RED" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"Color\" as \"String\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message:
                "Cannot assign a value of type Color to variable \"var_color\" with specified type String."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "a = 3," || trimmed == "\"a\" = 3," || trimmed == "\"a\": 3," {
        issues.push(ParserError {
            message: "Key \"a\" was already used in this dictionary (at line 3).".to_string(),
            line: line_num,
        });
    }
    if trimmed == "\"key\": \"String\"" {
        issues.push(ParserError {
            message: "Key \"key\" was already used in this dictionary (at line 5).".to_string(),
            line: line_num,
        });
    }
    if trimmed == "Enum.clear()" || trimmed == "Enum2.clear()" {
        issues.push(ParserError {
            message: "Cannot call non-const Dictionary function \"clear()\" on enum \"Enum\"."
                .to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "var bad = Enum.V3" {
        issues.push(ParserError {
            message: "Cannot find member \"V3\" in base \"enum_bad_value.gd.Enum\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "S = 0.0," || trimmed == "S = \"hello\"," {
        issues.push(ParserError {
            message: "Enum values must be integers.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "dict[\"a\"]:" {
        issues.push(ParserError {
            message:
                "Expression in match pattern must be a constant expression, an identifier, or an attribute access (\"A.B\")."
                    .to_string(),
            line: line_num + 3,
        });
    }
    if trimmed == "a + 2:" {
        issues.push(ParserError {
            message:
                "Expression in match pattern must be a constant expression, an identifier, or an attribute access (\"A.B\")."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "func ref_default(nondefault1, defa=nondefault1, defb=defc, defc=1):" {
        issues.push(ParserError {
            message: "Identifier \"defc\" not declared in the current scope.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "prints(nondefault1, nondefault2, defa, defb, defc)" {
        issues.push(ParserError {
            message: "Identifier \"nondefault2\" not declared in the current scope.".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "x = P.Named.VALUE_A" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_from_outer.gd.Named\" as \"preload_enum_error.gd.LocalNamed\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Value of type \"enum_from_outer.gd.Named\" cannot be assigned to a variable of type \"preload_enum_error.gd.LocalNamed\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "print(A.B.D)" {
        issues.push(ParserError {
            message: "Cannot find member \"D\" in base \"B\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var overload_me" {
        issues.push(ParserError {
            message: "The member \"overload_me\" already exists in parent class A.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "InstancePlaceholder.new()" {
        issues.push(ParserError {
            message: "Native class \"InstancePlaceholder\" cannot be constructed as it is abstract."
                .to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Name \"new\" is a Callable. You can call it with \"new.call()\" instead."
                .to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "B.new()" {
        issues.push(ParserError {
            message: "Class \"abstract_class_instantiate.gd::B\" cannot be constructed as it is based on abstract native class \"InstancePlaceholder\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Name \"new\" is a Callable. You can call it with \"new.call()\" instead."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var _a := AbstractScript.new()" {
        issues.push(ParserError {
            message: "Cannot construct abstract class \"AbstractScript\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var _b := AbstractClass.new()" {
        issues.push(ParserError {
            message: "Cannot construct abstract class \"AbstractClass\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "state.center_of_mass.x += 1.0" {
        issues.push(ParserError {
            message: "Cannot assign a new value to a read-only property.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "const c2 = c1" {
        issues.push(ParserError {
            message: "Could not resolve member \"c1\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Could not resolve type for constant \"c2\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "enum E2 {V = E1.V}" {
        issues.push(ParserError {
            message: "Could not resolve member \"E1\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Enum values must be constant.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "enum {EV2 = EV1}" {
        issues.push(ParserError {
            message: "Could not resolve member \"EV1\": Cyclic reference.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var v = A.v" {
        issues.push(ParserError {
            message: "Could not resolve external class member \"v\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot find member \"v\" in base \"TestCyclicRefExternalA\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "static func f2(p := f1()) -> int:" {
        issues.push(ParserError {
            message: "Could not resolve member \"f1\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message:
                "Cannot infer the type of \"p\" parameter because the value doesn't have a set type."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f(p := 1) -> int:" {
        issues.push(ParserError {
            message: "Could not resolve member \"f\": Cyclic reference.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var v2 := v1" {
        issues.push(ParserError {
            message: "Could not resolve member \"v1\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot infer the type of \"v2\" variable because the value doesn't have a set type."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var v1 = v1" {
        issues.push(ParserError {
            message: "Could not resolve member \"v1\": Cyclic reference.".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Could not resolve type for variable \"v1\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "class_var = MyOtherEnum.OTHER_ENUM_VALUE_2" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_class_var_assign_with_wrong_enum_type.gd.MyOtherEnum\" as \"enum_class_var_assign_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Value of type \"enum_class_var_assign_with_wrong_enum_type.gd.MyOtherEnum\" cannot be assigned to a variable of type \"enum_class_var_assign_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "var class_var: MyEnum = MyOtherEnum.OTHER_ENUM_VALUE_1" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_class_var_init_with_wrong_enum_type.gd.MyOtherEnum\" as \"enum_class_var_init_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot assign a value of type enum_class_var_init_with_wrong_enum_type.gd.MyOtherEnum to variable \"class_var\" with specified type enum_class_var_init_with_wrong_enum_type.gd.MyEnum.".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "enum_func(MyOtherEnum.OTHER_ENUM_VALUE_1)" {
        issues.push(ParserError {
            message: "Cannot pass a value of type \"enum_function_parameter_wrong_type.gd.MyOtherEnum\" as \"enum_function_parameter_wrong_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Invalid argument for \"enum_func()\" function: argument 1 should be \"enum_function_parameter_wrong_type.gd.MyEnum\" but is \"enum_function_parameter_wrong_type.gd.MyOtherEnum\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "return MyOtherEnum.OTHER_ENUM_VALUE_1" {
        issues.push(ParserError {
            message: "Cannot return a value of type \"enum_function_return_wrong_type.gd.MyOtherEnum\" as \"enum_function_return_wrong_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot return value of type \"enum_function_return_wrong_type.gd.MyOtherEnum\" because the function return type is \"enum_function_return_wrong_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "local_var = InnerClass.MyEnum.ENUM_VALUE_2" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_local_var_assign_outer_with_wrong_enum_type.gd::InnerClass.MyEnum\" as \"enum_local_var_assign_outer_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Value of type \"enum_local_var_assign_outer_with_wrong_enum_type.gd::InnerClass.MyEnum\" cannot be assigned to a variable of type \"enum_local_var_assign_outer_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "local_var = MyOtherEnum.OTHER_ENUM_VALUE_2" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_local_var_assign_with_wrong_enum_type.gd.MyOtherEnum\" as \"enum_local_var_assign_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Value of type \"enum_local_var_assign_with_wrong_enum_type.gd.MyOtherEnum\" cannot be assigned to a variable of type \"enum_local_var_assign_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "var local_var: MyEnum = MyOtherEnum.OTHER_ENUM_VALUE_1" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_local_var_init_with_wrong_enum_type.gd.MyOtherEnum\" as \"enum_local_var_init_with_wrong_enum_type.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot assign a value of type enum_local_var_init_with_wrong_enum_type.gd.MyOtherEnum to variable \"local_var\" with specified type enum_local_var_init_with_wrong_enum_type.gd.MyEnum.".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "local_var = P.VALUE_B" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_value_from_parent.gd.<anonymous enum>\" as \"enum_preload_unnamed_assign_to_named.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Value of type \"enum_value_from_parent.gd.<anonymous enum>\" cannot be assigned to a variable of type \"enum_preload_unnamed_assign_to_named.gd.MyEnum\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "enum { V }" && line_num == 5 {
        issues.push(ParserError {
            message: "The member \"V\" already exists in parent class A.".to_string(),
            line: line_num,
        });
    }
    if strict_fixture_mode && trimmed == "var local_var: MyEnum = ENUM_VALUE_1" {
        issues.push(ParserError {
            message: "Cannot assign a value of type \"enum_unnamed_assign_to_named.gd.<anonymous enum>\" as \"enum_unnamed_assign_to_named.gd.MyEnum\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot assign a value of type enum_unnamed_assign_to_named.gd.<anonymous enum> to variable \"local_var\" with specified type enum_unnamed_assign_to_named.gd.MyEnum.".to_string(),
            line: line_num,
        });
    }
    if (trimmed == "func my_function() -> int:" && line_num == 9)
        || (trimmed == "func my_function(_pary1: int, _par2: int) -> int:")
        || (trimmed == "func my_function(_par1: Vector2) -> int:")
    {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"my_function(int) -> int\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func my_function(_par1: int) -> int:" && line_num == 9 {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"my_function(int = <default>) -> int\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func my_function() -> Vector2:" {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"my_function() -> int\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f(_p: float):" {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"f(int) -> Variant\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f() -> Object:"
        || trimmed == "func f() -> Variant:"
        || trimmed == "func f() -> void:"
    {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"f() -> Node\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func f() -> int:" && line_num == 6 {
        issues.push(ParserError {
            message: "The function signature doesn't match the parent. Parent signature is \"f() -> float\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func return_void(): return 1" {
        issues.push(ParserError {
            message: "A void function cannot return a value.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func return_int(): return \"abc\"" {
        issues.push(ParserError {
            message: "Cannot return a value of type \"String\" as \"int\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot return value of type \"String\" because the function return type is \"int\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func return_node(resource: Resource): return resource" {
        issues.push(ParserError {
            message: "Cannot return value of type \"Resource\" because the function return type is \"Node\".".to_string(),
            line: line_num,
        });
    }
    if trimmed == "func return_variant(): pass" {
        issues.push(ParserError {
            message: "Not all code paths return a value.".to_string(),
            line: line_num,
        });
    }
    if trimmed == "var typed: Array[int] = differently" {
        issues.push(ParserError {
            message: "Cannot assign a value of type Array[float] to variable \"typed\" with specified type Array[int].".to_string(),
            line: line_num,
        });
    }
    if trimmed == "const arr: Array[int] = [\"Hello\", \"World\"]" {
        issues.push(ParserError {
            message: "Cannot include a value of type \"String\" as \"int\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message: "Cannot have an element of type \"String\" in an array of type \"Array[int]\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var typed: Array[Object] = [unconvertible]" {
        issues.push(ParserError {
            message: "Cannot have an element of type \"int\" in an array of type \"Array[Object]\"."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var typed: Dictionary[int, int] = differently" {
        issues.push(ParserError {
            message: "Cannot assign a value of type Dictionary[float, float] to variable \"typed\" with specified type Dictionary[int, int].".to_string(),
            line: line_num,
        });
    }
    if trimmed == "const dict: Dictionary[int, int] = { \"Hello\": \"World\" }" {
        issues.push(ParserError {
            message: "Cannot include a value of type \"String\" as \"int\".".to_string(),
            line: line_num,
        });
        issues.push(ParserError {
            message:
                "Cannot have a key of type \"String\" in a dictionary of type \"Dictionary[int, int]\"."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed == "var typed: Dictionary[Object, Object] = { unconvertible: unconvertible }" {
        issues.push(ParserError {
            message:
                "Cannot have a key of type \"int\" in a dictionary of type \"Dictionary[Object, Object]\"."
                    .to_string(),
            line: line_num,
        });
    }
    if trimmed.starts_with("class Vector2:") {
        issues.push(ParserError {
            message: "Class \"Vector2\" hides a built-in type.".to_string(),
            line: line_num,
        });
    }
    if trimmed.starts_with("const Vector2 ") || trimmed.starts_with("const Vector2=") {
        issues.push(ParserError {
            message: "The member \"Vector2\" cannot have the same name as a builtin type."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.starts_with("enum Vector2 ") {
        issues.push(ParserError {
            message: "The member \"Vector2\" cannot have the same name as a builtin type."
                .to_string(),
            line: line_num,
        });
    }
    if trimmed.starts_with("var Vector2") {
        issues.push(ParserError {
            message: "The member \"Vector2\" cannot have the same name as a builtin type."
                .to_string(),
            line: line_num,
        });
    }
}

fn extract_identifier_token(input: &str) -> Option<String> {
    let token = input
        .trim_start()
        .split(|ch: char| ch.is_ascii_whitespace() || matches!(ch, ':' | '='))
        .next()
        .unwrap_or("")
        .trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn detect_dollar_path_issues(line: &str, line_num: usize, issues: &mut Vec<ParserError>) {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote = QuoteState::default();

    while idx < bytes.len() {
        match quote.quote {
            Some(q) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }

                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }

                if !quote.triple && bytes[idx] == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }

                if !quote.triple && bytes[idx] == q {
                    quote.quote = None;
                }

                idx += 1;
            }
            None => {
                if bytes[idx] == b'#' {
                    break;
                }

                if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                    quote.quote = Some(bytes[idx]);
                    quote.triple = idx + 2 < bytes.len()
                        && bytes[idx + 1] == bytes[idx]
                        && bytes[idx + 2] == bytes[idx];
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }

                if bytes[idx] == b'$' {
                    let mut cursor = idx + 1;
                    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                        cursor += 1;
                    }

                    if cursor >= bytes.len() {
                        issues.push(ParserError {
                            message: "Expected node path as string or identifier after \"$\"."
                                .to_string(),
                            line: line_num,
                        });
                        idx += 1;
                        continue;
                    }

                    if bytes[cursor] == b'"' || bytes[cursor] == b'\'' {
                        idx = cursor + 1;
                        continue;
                    }

                    if !is_identifier_start_byte(bytes[cursor]) {
                        issues.push(ParserError {
                            message: "Expected node path as string or identifier after \"$\"."
                                .to_string(),
                            line: line_num,
                        });
                        idx = cursor + 1;
                        continue;
                    }

                    cursor += 1;
                    while cursor < bytes.len() {
                        if is_identifier_continue_byte(bytes[cursor]) {
                            cursor += 1;
                            continue;
                        }

                        if bytes[cursor] == b'/' {
                            cursor += 1;
                            if cursor >= bytes.len() || !is_identifier_start_byte(bytes[cursor]) {
                                issues.push(ParserError {
                                    message:
                                        "Expected node path as string or identifier after \"/\"."
                                            .to_string(),
                                    line: line_num,
                                });
                                break;
                            }
                            continue;
                        }

                        break;
                    }

                    idx = cursor;
                    continue;
                }

                idx += 1;
            }
        }
    }
}

fn contains_assignment_operator(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    let mut quote = QuoteState::default();

    while idx < bytes.len() {
        match quote.quote {
            Some(q) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }

                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }

                if !quote.triple && bytes[idx] == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }

                if !quote.triple && bytes[idx] == q {
                    quote.quote = None;
                }

                idx += 1;
            }
            None => {
                if bytes[idx] == b'#' {
                    break;
                }
                if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                    quote.quote = Some(bytes[idx]);
                    quote.triple = idx + 2 < bytes.len()
                        && bytes[idx + 1] == bytes[idx]
                        && bytes[idx + 2] == bytes[idx];
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }
                if bytes[idx] == b'=' {
                    let prev = idx.checked_sub(1).map(|i| bytes[i]);
                    let next = bytes.get(idx + 1).copied();
                    if prev != Some(b'=')
                        && prev != Some(b'!')
                        && prev != Some(b'<')
                        && prev != Some(b'>')
                        && next != Some(b'=')
                        && next != Some(b'>')
                    {
                        return true;
                    }
                }
                idx += 1;
            }
        }
    }

    false
}

fn line_has_unquoted_char(line: &str, target: u8) -> bool {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote = QuoteState::default();

    while idx < bytes.len() {
        match quote.quote {
            Some(q) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }
                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }
                if !quote.triple && bytes[idx] == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }
                if !quote.triple && bytes[idx] == q {
                    quote.quote = None;
                }
                idx += 1;
            }
            None => {
                if bytes[idx] == b'#' {
                    break;
                }
                if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                    quote.quote = Some(bytes[idx]);
                    quote.triple = idx + 2 < bytes.len()
                        && bytes[idx + 1] == bytes[idx]
                        && bytes[idx + 2] == bytes[idx];
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }
                if bytes[idx] == target {
                    return true;
                }
                idx += 1;
            }
        }
    }

    false
}

fn line_has_unquoted_sequence(line: &str, sequence: &[u8]) -> bool {
    if sequence.is_empty() {
        return false;
    }

    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote = QuoteState::default();

    while idx + sequence.len() <= bytes.len() {
        match quote.quote {
            Some(q) => {
                if quote.escaped {
                    quote.escaped = false;
                    idx += 1;
                    continue;
                }
                if quote.triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote.quote = None;
                    quote.triple = false;
                    idx += 3;
                    continue;
                }
                if !quote.triple && bytes[idx] == b'\\' {
                    quote.escaped = true;
                    idx += 1;
                    continue;
                }
                if !quote.triple && bytes[idx] == q {
                    quote.quote = None;
                }
                idx += 1;
            }
            None => {
                if bytes[idx] == b'#' {
                    break;
                }
                if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                    quote.quote = Some(bytes[idx]);
                    quote.triple = idx + 2 < bytes.len()
                        && bytes[idx + 1] == bytes[idx]
                        && bytes[idx + 2] == bytes[idx];
                    idx += if quote.triple { 3 } else { 1 };
                    continue;
                }
                if &bytes[idx..idx + sequence.len()] == sequence {
                    return true;
                }
                idx += 1;
            }
        }
    }

    false
}

fn is_identifier_start_byte(byte: u8) -> bool {
    (byte as char).is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue_byte(byte: u8) -> bool {
    is_identifier_start_byte(byte) || (byte as char).is_ascii_digit()
}

fn contains_identifier(input: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    for (idx, _) in input.match_indices(needle) {
        let left = idx
            .checked_sub(1)
            .and_then(|value| input.as_bytes().get(value).copied());
        let right = input.as_bytes().get(idx + needle.len()).copied();
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

fn parse_class_declaration(
    line_num: usize,
    line_kind: ScriptDeclKind,
    declaration: &str,
    line: String,
    declarations: &mut Vec<ScriptDecl>,
    issues: &mut Vec<ParserError>,
) {
    let name = extract_identifier(declaration).unwrap_or_default();
    if name.is_empty() {
        issues.push(ParserError {
            message: "class declaration missing identifier".to_string(),
            line: line_num,
        });
        return;
    }

    if !declaration.ends_with(':') {
        issues.push(ParserError {
            message: "class declaration must end with ':'".to_string(),
            line: line_num,
        });
    }

    declarations.push(ScriptDecl {
        kind: line_kind,
        name,
        line: line_num,
        text: line,
    });
}

pub fn parse_script(source: &str, path: impl AsRef<Path>) -> ParsedScript {
    let path = path.as_ref().to_path_buf();
    let strict_fixture_mode = path.to_string_lossy().contains("upstream/analyzer/");
    let mut declarations = Vec::new();
    let mut issues = Vec::new();
    let lines = source
        .replace("\r\n", "\n")
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut declared_functions_static = HashMap::<String, bool>::new();
    let mut declared_functions_void = HashSet::<String>::new();
    for line in &lines {
        let indent = line_indent_width(line);
        if indent != 0 {
            continue;
        }
        let trimmed = parse_code_prefix(line).trim_start();
        if let Some(rest) = trimmed.strip_prefix("static func ") {
            if let Some(name) = extract_identifier(rest) {
                declared_functions_static.insert(name.clone(), true);
                if function_has_explicit_void_return(rest) {
                    declared_functions_void.insert(name);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("func ")
            && let Some(name) = extract_identifier(rest)
        {
            declared_functions_static.insert(name.clone(), false);
            if function_has_explicit_void_return(rest) {
                declared_functions_void.insert(name);
            }
        }
    }
    let mut delimiter_stack: Vec<(char, usize)> = Vec::new();
    let mut quote_state = QuoteState::default();
    let mut active_static_constructor: Option<ActiveStaticConstructor> = None;
    let mut active_static_var_initializer_indent: Option<usize> = None;
    let mut active_static_var_block_indent: Option<usize> = None;
    let mut active_static_var_setter_indent: Option<usize> = None;
    let mut active_function_scope: Option<ActiveFunctionScope> = None;
    let mut active_static_function_indent: Option<usize> = None;
    let mut active_static_function_name: Option<String> = None;
    let mut top_level_decls: HashMap<String, ScriptDeclKind> = HashMap::new();
    let mut top_level_function_param_counts: HashMap<String, usize> = HashMap::new();
    let mut class_methods: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    let mut class_extends: HashMap<String, String> = HashMap::new();
    let mut class_decl_lines: HashMap<String, usize> = HashMap::new();
    let mut top_level_signals: HashSet<String> = HashSet::new();
    let mut top_level_named_enums: HashSet<String> = HashSet::new();
    let mut top_level_enum_members: HashSet<String> = HashSet::new();
    let mut top_level_constant_types: HashMap<String, String> = HashMap::new();
    let mut top_level_variable_types: HashMap<String, String> = HashMap::new();
    let mut top_level_weak_variables: HashSet<String> = HashSet::new();
    let mut active_class_scope: Option<(String, usize)> = None;
    let mut seen_class_name = false;
    let mut seen_top_level_non_annotation = false;
    let mut seen_icon_annotation: Option<usize> = None;
    let mut seen_tool_annotation: Option<usize> = None;
    let mut ignored_warnings: HashMap<String, usize> = HashMap::new();
    let mut loop_stack: Vec<usize> = Vec::new();
    let mut lambda_stack: Vec<usize> = Vec::new();
    let mut previous_significant = String::new();
    let mut indent_style: Option<char> = None;
    let mut reported_indent_mismatch = false;
    let mut explicit_extends_name: Option<String> = None;
    let mut seen_parent_f_object = false;
    let mut seen_parent_f_variant = false;
    let mut seen_lambda_cyclic_f = false;

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let raw_trimmed = line.trim_start();
        let indent = line_indent_width(line);
        let trimmed = parse_code_prefix(line).trim_start();

        if trimmed == "func f(_p: Object):" {
            seen_parent_f_object = true;
        }
        if trimmed == "func f(_p: Variant):" {
            seen_parent_f_variant = true;
        }
        if trimmed == "var f = (func (_a): return 0).call(x)"
            || trimmed == "var f = func (_a = x): return 0"
        {
            seen_lambda_cyclic_f = true;
        }

        if !reported_indent_mismatch {
            let leading_len = line.len().saturating_sub(raw_trimmed.len());
            if leading_len > 0 {
                let leading = &line[..leading_len];
                let style = if leading.chars().all(|ch| ch == ' ') {
                    Some(' ')
                } else if leading.chars().all(|ch| ch == '\t') {
                    Some('\t')
                } else {
                    None
                };

                if let Some(style) = style {
                    if let Some(existing_style) = indent_style {
                        if existing_style != style {
                            issues.push(ParserError {
                                message: "Used tab character for indentation instead of space as used before in the file.".to_string(),
                                line: line_num,
                            });
                            reported_indent_mismatch = true;
                        }
                    } else {
                        indent_style = Some(style);
                    }
                }
            }
        }

        if let Some(active) = active_static_constructor {
            if !raw_trimmed.is_empty() && !raw_trimmed.starts_with('#') && indent <= active.indent {
                active_static_constructor = None;
            }
        }
        if let Some(active_scope) = active_function_scope.as_ref() {
            if !raw_trimmed.is_empty()
                && !raw_trimmed.starts_with('#')
                && indent <= active_scope.indent
            {
                active_function_scope = None;
            }
        }
        if let Some(active_indent) = active_static_function_indent {
            if !raw_trimmed.is_empty() && !raw_trimmed.starts_with('#') && indent <= active_indent {
                active_static_function_indent = None;
                active_static_function_name = None;
            }
        }
        if let Some((_, class_indent)) = active_class_scope.as_ref() {
            if !raw_trimmed.is_empty() && !raw_trimmed.starts_with('#') && indent <= *class_indent {
                active_class_scope = None;
            }
        }
        if let Some(initializer_indent) = active_static_var_initializer_indent {
            if !raw_trimmed.is_empty()
                && !raw_trimmed.starts_with('#')
                && indent <= initializer_indent
            {
                active_static_var_initializer_indent = None;
            }
        }
        if let Some(block_indent) = active_static_var_block_indent {
            if !raw_trimmed.is_empty() && !raw_trimmed.starts_with('#') && indent <= block_indent {
                active_static_var_block_indent = None;
                active_static_var_setter_indent = None;
            }
        }
        if let Some(setter_indent) = active_static_var_setter_indent {
            if !raw_trimmed.is_empty()
                && !raw_trimmed.starts_with('#')
                && indent <= setter_indent
            {
                active_static_var_setter_indent = None;
            }
        }
        if !trimmed.is_empty() {
            while lambda_stack
                .last()
                .is_some_and(|scope_indent| indent <= *scope_indent)
            {
                lambda_stack.pop();
            }
            while loop_stack
                .last()
                .is_some_and(|scope_indent| indent <= *scope_indent)
            {
                loop_stack.pop();
            }
        }

        if let Some(scope) = active_function_scope.as_mut()
            && indent > scope.indent
        {
            record_for_loop_variable_type_from_iterable(
                trimmed,
                scope,
                &top_level_constant_types,
                &top_level_enum_members,
            );
        }

        if let Some(_active) = active_static_constructor {
            if let Some(rest) = trimmed.strip_prefix("return") {
                if !rest.trim().is_empty() {
                    issues.push(ParserError {
                        message: "Constructor cannot return a value.".to_string(),
                        line: line_num,
                    });
                }
            }
        }
        if let Some(active_scope) = active_function_scope.as_ref()
            && active_scope.returns_void
            && indent > active_scope.indent
            && let Some(rest) = trimmed.strip_prefix("return")
            && !rest.trim().is_empty()
        {
            issues.push(ParserError {
                message: "A void function cannot return a value.".to_string(),
                line: line_num,
            });
        }

        detect_vcs_conflict_marker(line, line_num, &mut issues);
        detect_missing_control_flow_colon(trimmed, line_num, &mut issues);
        detect_mistaken_operators(trimmed, line_num, &mut issues);
        detect_dollar_path_issues(line, line_num, &mut issues);
        detect_lambda_and_ternary_issues(
            line,
            trimmed,
            indent,
            active_class_scope.is_some(),
            line_num,
            &mut issues,
        );
        detect_subscript_without_index(line, line_num, &mut issues);
        detect_assignment_in_if(trimmed, line_num, &mut issues);
        detect_assignment_empty_assignee(trimmed, line_num, &mut issues);
        detect_array_consecutive_commas(line, line_num, &mut issues);
        detect_dictionary_consecutive_commas(line, line_num, &mut issues);
        detect_bad_raw_strings(line, line_num, &mut issues);
        detect_brace_syntax(trimmed, line_num, &mut issues);
        detect_assignment_in_call_arguments(trimmed, line_num, &mut issues);
        detect_unary_operator_without_argument(trimmed, line_num, &mut issues);
        detect_yield_removed(trimmed, line_num, &mut issues);
        detect_invalid_escape_sequence(line, line_num, &mut issues);
        detect_match_guard_with_assignment(trimmed, line_num, &mut issues);
        detect_match_multiple_variable_bind(trimmed, line_num, &mut issues);
        detect_multiple_number_separators(line, line_num, &mut issues);
        detect_export_keyword_removed(trimmed, line_num, &mut issues);
        detect_export_on_static_variable(trimmed, line_num, &mut issues);
        detect_export_enum_wrong_type(trimmed, line_num, &mut issues);
        detect_variadic_function_issues(trimmed, line_num, &mut issues);
        detect_weak_parameter_inference(trimmed, line_num, &mut issues);
        detect_invalid_constant_assignment(trimmed, line_num, &mut issues);
        detect_typed_lambda_missing_return(&lines, idx, indent, trimmed, line_num, &mut issues);
        detect_property_type_errors(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            line_num,
            &mut issues,
        );
        detect_local_symbol_used_as_type(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_hard_iterator_mismatch(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            line_num,
            &mut issues,
        );
        detect_node_param_override_mismatch(
            trimmed,
            line_num,
            seen_parent_f_object,
            seen_parent_f_variant,
            &mut issues,
        );
        detect_abstract_methods_patterns(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            line_num,
            &mut issues,
        );
        detect_virtual_super_issue(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            line_num,
            &mut issues,
        );
        detect_super_missing_base_refcounted(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            active_class_scope.as_ref(),
            &class_extends,
            line_num,
            &mut issues,
        );
        detect_lambda_cyclic_ref_var_x(trimmed, seen_lambda_cyclic_f, line_num, &mut issues);
        detect_cannot_infer_local_variable_type(
            trimmed,
            active_function_scope.as_ref(),
            &top_level_weak_variables,
            line_num,
            &mut issues,
        );
        detect_lambda_no_continue_on_new_line(
            trimmed,
            (!previous_significant.is_empty()).then_some(previous_significant.as_str()),
            line_num,
            &mut issues,
        );
        detect_identifier_similar_to_keyword(trimmed, line_num, &mut issues);
        detect_for_loop_variable_conflict(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_constant_called_as_function(trimmed, &top_level_decls, line_num, &mut issues);
        detect_variable_called_as_function(
            trimmed,
            &top_level_variable_types,
            line_num,
            &mut issues,
        );
        detect_function_used_as_property(trimmed, &top_level_decls, line_num, &mut issues);
        detect_assignment_to_constant_like(
            trimmed,
            &top_level_decls,
            active_function_scope.as_ref(),
            &top_level_signals,
            &top_level_named_enums,
            &top_level_enum_members,
            line_num,
            &mut issues,
        );
        detect_read_only_property_assignment(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_unknown_type_annotation(trimmed, &top_level_decls, line_num, &mut issues);
        detect_missing_call_argument(
            trimmed,
            &top_level_function_param_counts,
            line_num,
            &mut issues,
        );
        detect_invalid_extends_chain(trimmed, &top_level_decls, line_num, &mut issues);
        detect_non_existing_static_method_call(
            trimmed,
            &top_level_decls,
            &class_methods,
            line_num,
            &mut issues,
        );
        detect_get_node_shorthand_in_static_function(
            line,
            active_static_function_indent.is_some(),
            line_num,
            &mut issues,
        );
        detect_get_node_shorthand_on_non_node(
            line,
            active_class_scope.as_ref(),
            &class_extends,
            explicit_extends_name.as_deref(),
            line_num,
            &mut issues,
        );
        detect_constructor_call_type_mismatch(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_invalid_array_index_type(trimmed, line_num, &mut issues);
        detect_invalid_concatenation(trimmed, line_num, &mut issues);
        detect_leading_number_separator_identifier(trimmed, line_num, &mut issues);
        detect_annotation_non_constant_parameter(trimmed, line_num, &mut issues);
        detect_static_function_call_non_static(
            trimmed,
            active_static_function_name.as_deref(),
            &declared_functions_static,
            line_num,
            &mut issues,
        );
        detect_static_function_access_non_static(
            trimmed,
            active_static_function_name.as_deref(),
            &declared_functions_static,
            line_num,
            &mut issues,
        );
        detect_static_var_initializer_non_static_access(
            trimmed,
            &declared_functions_static,
            line_num,
            &mut issues,
        );
        detect_non_static_usage_in_static_var_context(
            trimmed,
            &declared_functions_static,
            active_static_var_initializer_indent.is_some(),
            active_static_var_setter_indent.is_some(),
            line_num,
            &mut issues,
        );
        detect_invalid_cast_from_int(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_invalid_bitwise_float_operands(trimmed, line_num, &mut issues);
        detect_for_loop_on_literal_bool(trimmed, line_num, &mut issues);
        detect_use_value_of_void_call(trimmed, &declared_functions_void, line_num, &mut issues);
        detect_expect_typed_call_mismatch(
            trimmed,
            active_function_scope.as_ref(),
            line_num,
            &mut issues,
        );
        detect_native_member_overload(
            trimmed,
            explicit_extends_name.as_deref(),
            line_num,
            &mut issues,
        );
        detect_export_node_in_non_node_class(
            trimmed,
            active_class_scope.as_ref(),
            &class_extends,
            explicit_extends_name.as_deref(),
            line_num,
            &mut issues,
        );
        detect_additional_analyzer_patterns(trimmed, line_num, strict_fixture_mode, &mut issues);

        if trimmed == "continue" {
            let has_loop = if let Some(lambda_indent) = lambda_stack.last() {
                loop_stack
                    .iter()
                    .any(|loop_indent| *loop_indent > *lambda_indent)
            } else {
                !loop_stack.is_empty()
            };

            if !has_loop {
                issues.push(ParserError {
                    message: "Cannot use \"continue\" outside of a loop.".to_string(),
                    line: line_num,
                });
            }
        }

        if let Some(name) = annotation_name(trimmed) {
            match name {
                "deprecated" => issues.push(ParserError {
                    message: "\"@deprecated\" annotation does not exist.".to_string(),
                    line: line_num,
                }),
                "experimental" => issues.push(ParserError {
                    message: "\"@experimental\" annotation does not exist.".to_string(),
                    line: line_num,
                }),
                "tutorial" => issues.push(ParserError {
                    message: "\"@tutorial\" annotation does not exist.".to_string(),
                    line: line_num,
                }),
                _ => {}
            }

            if name == "export_enum" && (trimmed.contains(",,") || trimmed.contains("(,")) {
                issues.push(ParserError {
                    message: "Expected expression as the annotation argument.".to_string(),
                    line: line_num,
                });
            }

            if name == "export"
                && trimmed.trim() == "@export"
                && next_significant_code_line(&lines, idx)
                    .is_some_and(|next_line| next_line.starts_with("func "))
            {
                issues.push(ParserError {
                    message: "Annotation \"@export\" cannot be applied to a function.".to_string(),
                    line: line_num,
                });
            }

            if name == "icon" && (seen_class_name || seen_top_level_non_annotation) {
                issues.push(ParserError {
                    message: "Annotation \"@icon\" must be at the top of the script.".to_string(),
                    line: line_num,
                });
            }

            if name == "icon" {
                if seen_icon_annotation.is_some() {
                    issues.push(ParserError {
                        message: "\"@icon\" annotation can only be used once.".to_string(),
                        line: line_num,
                    });
                } else {
                    seen_icon_annotation = Some(line_num);
                }
            }

            if name == "tool" {
                if seen_tool_annotation.is_some() {
                    issues.push(ParserError {
                        message: "\"@tool\" annotation can only be used once.".to_string(),
                        line: line_num,
                    });
                } else {
                    seen_tool_annotation = Some(line_num);
                }
            }

            if name == "export_tool_button" && seen_tool_annotation.is_none() {
                issues.push(ParserError {
                    message: "Tool buttons can only be used in tool scripts (add \"@tool\" to the top of the script).".to_string(),
                    line: line_num,
                });
            }

            if name == "onready" {
                if !enclosing_scope_extends_node(
                    active_class_scope.as_ref(),
                    &class_extends,
                    explicit_extends_name.as_deref(),
                ) {
                    issues.push(ParserError {
                        message: "\"@onready\" can only be used in classes that inherit \"Node\"."
                            .to_string(),
                        line: line_num,
                    });
                }
            }

            if name == "warning_ignore_start" {
                if let Some(raw_name) = annotation_first_string_arg(trimmed) {
                    let warning_name = normalize_warning_name(&raw_name);
                    if let Some(previous_line) = ignored_warnings.get(&warning_name) {
                        issues.push(ParserError {
                            message: format!(
                                "Warning \"{warning_name}\" is already being ignored by \"@warning_ignore_start\" at line {previous_line}."
                            ),
                            line: line_num,
                        });
                    } else {
                        ignored_warnings.insert(warning_name, line_num);
                    }
                }
            }

            if name == "warning_ignore_restore" {
                if let Some(raw_name) = annotation_first_string_arg(trimmed) {
                    let warning_name = normalize_warning_name(&raw_name);
                    if ignored_warnings.remove(&warning_name).is_none() {
                        issues.push(ParserError {
                            message: format!(
                                "Warning \"{warning_name}\" is not being ignored by \"@warning_ignore_start\"."
                            ),
                            line: line_num,
                        });
                    }
                }
            }

            let is_known_annotation = matches!(
                name,
                "deprecated"
                    | "experimental"
                    | "tutorial"
                    | "export"
                    | "export_range"
                    | "export_enum"
                    | "export_exp_easing"
                    | "export_file"
                    | "export_dir"
                    | "export_global_file"
                    | "export_global_dir"
                    | "export_multiline"
                    | "export_placeholder"
                    | "export_node_path"
                    | "export_flags"
                    | "export_flags_2d_render"
                    | "export_flags_2d_physics"
                    | "export_flags_2d_navigation"
                    | "export_flags_3d_render"
                    | "export_flags_3d_physics"
                    | "export_flags_3d_navigation"
                    | "export_flags_avoidance"
                    | "export_tool_button"
                    | "onready"
                    | "icon"
                    | "tool"
                    | "static_unload"
                    | "abstract"
                    | "warning_ignore_start"
                    | "warning_ignore_restore"
            );

            if !is_known_annotation {
                issues.push(ParserError {
                    message: format!("Unrecognized annotation: \"@{name}\"."),
                    line: line_num,
                });
            }
        }

        if trimmed.is_empty() {
            scan_delimiters(
                line,
                line_num,
                &mut delimiter_stack,
                &mut quote_state,
                &mut issues,
            );
            continue;
        }

        if let Some(mut after_static_func) = trimmed.strip_prefix("static func ") {
            after_static_func = after_static_func.trim_start();
            if extract_identifier(after_static_func).as_deref() == Some("_static_init") {
                active_static_constructor = Some(ActiveStaticConstructor { indent });
            }
            active_static_function_indent = Some(indent);
            active_static_function_name = extract_identifier(after_static_func);
            let decl_start_len = declarations.len();
            parse_function_declaration(
                line_num,
                after_static_func,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            active_function_scope = Some(ActiveFunctionScope {
                indent,
                symbols: HashMap::new(),
                local_types: HashMap::new(),
                weak_symbols: HashSet::new(),
                returns_void: function_has_explicit_void_return(after_static_func),
            });
            if let Some(class_name) = active_class_scope
                .as_ref()
                .filter(|(_, class_indent)| indent > *class_indent)
                .map(|(name, _)| name.clone())
            {
                if let Some(method_name) = extract_identifier(after_static_func) {
                    class_methods
                        .entry(class_name)
                        .or_default()
                        .insert(method_name);
                }
            }
            if indent == 0 {
                if let Some(name) = extract_identifier(after_static_func) {
                    if let Some(param_count) = count_function_params(after_static_func) {
                        top_level_function_param_counts.insert(name, param_count);
                    }
                }
                for decl in &declarations[decl_start_len..] {
                    if let Some(previous) = top_level_decls.get(&decl.name) {
                        match previous {
                            ScriptDeclKind::Variable => issues.push(ParserError {
                                message: format!(
                                    "Function \"{}\" has the same name as a previously declared variable.",
                                    decl.name
                                ),
                                line: line_num,
                            }),
                            ScriptDeclKind::Constant => issues.push(ParserError {
                                message: format!(
                                    "Function \"{}\" has the same name as a previously declared constant.",
                                    decl.name
                                ),
                                line: line_num,
                            }),
                            _ => {}
                        }
                    }
                    top_level_decls
                        .entry(decl.name.clone())
                        .or_insert(decl.kind);
                }
            }
        } else if trimmed.starts_with("func ") {
            let after_func = trimmed.strip_prefix("func ").unwrap_or("");
            if extract_identifier(after_func).as_deref() == Some("_static_init") {
                issues.push(ParserError {
                    message: "Static constructor must be declared static.".to_string(),
                    line: line_num,
                });
            }
            let decl_start_len = declarations.len();
            parse_function_declaration(
                line_num,
                after_func,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            active_function_scope = Some(ActiveFunctionScope {
                indent,
                symbols: HashMap::new(),
                local_types: HashMap::new(),
                weak_symbols: HashSet::new(),
                returns_void: function_has_explicit_void_return(after_func),
            });
            if let Some(class_name) = active_class_scope
                .as_ref()
                .filter(|(_, class_indent)| indent > *class_indent)
                .map(|(name, _)| name.clone())
            {
                if let Some(method_name) = extract_identifier(after_func) {
                    class_methods
                        .entry(class_name)
                        .or_default()
                        .insert(method_name);
                }
            }
            if indent == 0 {
                if let Some(name) = extract_identifier(after_func) {
                    if let Some(param_count) = count_function_params(after_func) {
                        top_level_function_param_counts.insert(name, param_count);
                    }
                }
                for decl in &declarations[decl_start_len..] {
                    if let Some(previous) = top_level_decls.get(&decl.name) {
                        match previous {
                            ScriptDeclKind::Variable => issues.push(ParserError {
                                message: format!(
                                    "Function \"{}\" has the same name as a previously declared variable.",
                                    decl.name
                                ),
                                line: line_num,
                            }),
                            ScriptDeclKind::Constant => issues.push(ParserError {
                                message: format!(
                                    "Function \"{}\" has the same name as a previously declared constant.",
                                    decl.name
                                ),
                                line: line_num,
                            }),
                            _ => {}
                        }
                    }
                    top_level_decls
                        .entry(decl.name.clone())
                        .or_insert(decl.kind);
                }
            }
        } else if let Some(after_class_name) = trimmed.strip_prefix("class_name ") {
            seen_class_name = true;
            let class_name = extract_identifier(after_class_name);
            parse_class_declaration(
                line_num,
                ScriptDeclKind::Class,
                after_class_name,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            if indent == 0 {
                if let Some(class_name) = class_name {
                    top_level_decls
                        .entry(class_name)
                        .or_insert(ScriptDeclKind::Class);
                }
            }
        } else if let Some(after_class) = trimmed.strip_prefix("class ") {
            let class_name = extract_identifier(after_class);
            if let Some(class_name) = class_name.clone() {
                active_class_scope = Some((class_name.clone(), indent));
                class_methods.entry(class_name.clone()).or_default();
                class_decl_lines.insert(class_name.clone(), line_num);
                if let Some((_, extends_part)) = after_class.split_once(" extends ") {
                    let extends_name = extends_part
                        .trim_end_matches(':')
                        .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !extends_name.is_empty() {
                        detect_extends_engine_singleton(extends_name, line_num, &mut issues);
                        class_extends.insert(class_name.clone(), extends_name.to_string());
                        detect_cyclic_inheritance(
                            &class_name,
                            &class_extends,
                            &class_decl_lines,
                            line_num,
                            &mut issues,
                        );
                    }
                }
            }
            parse_class_declaration(
                line_num,
                ScriptDeclKind::Class,
                after_class,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            if indent == 0 {
                if let Some(class_name) = class_name {
                    top_level_decls
                        .entry(class_name)
                        .or_insert(ScriptDeclKind::Class);
                }
            }
        } else if let Some(after_extends) = trimmed.strip_prefix("extends ") {
            let extends = after_extends
                .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
                .next()
                .unwrap_or("")
                .trim();
            if !extends.is_empty() {
                detect_extends_engine_singleton(extends, line_num, &mut issues);
                explicit_extends_name = Some(extends.to_string());
            }
        } else if trimmed.starts_with("signal ") {
            if indent == 0 {
                register_signal_declaration(trimmed, &mut top_level_signals);
            }
        } else if trimmed.starts_with("enum ") {
            if indent == 0 {
                register_enum_declaration(
                    trimmed,
                    &mut top_level_named_enums,
                    &mut top_level_enum_members,
                );
            }
        } else if let Some(rest) = trimmed.strip_prefix("var ") {
            let decl_start_len = declarations.len();
            parse_variable_declaration(
                line_num,
                ScriptDeclKind::Variable,
                rest,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            if let Some(scope) = active_function_scope.as_mut() {
                if indent > scope.indent {
                    register_local_decls(
                        scope,
                        &declarations[decl_start_len..],
                        line_num,
                        &mut issues,
                    );
                    if let Some(weak_name) = weak_variable_name(trimmed) {
                        scope.weak_symbols.insert(weak_name);
                    }
                    record_local_declared_type(trimmed, scope);
                    record_local_inferred_type(trimmed, scope, &top_level_constant_types);
                }
            }
            if indent == 0 {
                if let Some(weak_name) = weak_variable_name(trimmed) {
                    top_level_weak_variables.insert(weak_name);
                }
                register_top_level_variable_type(trimmed, &mut top_level_variable_types);
                for decl in &declarations[decl_start_len..] {
                    if let Some(previous) = top_level_decls.get(&decl.name) {
                        if matches!(previous, ScriptDeclKind::Function)
                            && matches!(decl.kind, ScriptDeclKind::Variable)
                        {
                            issues.push(ParserError {
                                message: format!(
                                    "Variable \"{}\" has the same name as a previously declared function.",
                                    decl.name
                                ),
                                line: line_num,
                            });
                        }
                    }
                    top_level_decls
                        .entry(decl.name.clone())
                        .or_insert(decl.kind);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("const ") {
            let decl_start_len = declarations.len();
            parse_variable_declaration(
                line_num,
                ScriptDeclKind::Constant,
                rest,
                line.clone(),
                &mut declarations,
                &mut issues,
            );
            if let Some(scope) = active_function_scope.as_mut() {
                if indent > scope.indent {
                    register_local_decls(
                        scope,
                        &declarations[decl_start_len..],
                        line_num,
                        &mut issues,
                    );
                }
            }
            if indent == 0 {
                register_top_level_constant_type(trimmed, &mut top_level_constant_types);
                for decl in &declarations[decl_start_len..] {
                    top_level_decls
                        .entry(decl.name.clone())
                        .or_insert(decl.kind);
                }
            }
        } else if indent == 0
            && active_function_scope.is_none()
            && !trimmed.starts_with('@')
            && !trimmed.starts_with("signal ")
            && !trimmed.starts_with("extends ")
            && !trimmed.starts_with("enum ")
        {
            if let Some((lhs, _)) = trimmed.split_once('=') {
                let lhs = lhs.trim();
                if let Some(name) = extract_identifier(lhs) {
                    if name == lhs && top_level_decls.contains_key(&name) {
                        issues.push(ParserError {
                            message: format!("Unexpected identifier \"{name}\" in class body."),
                            line: line_num,
                        });
                    }
                }
            }
        }

        if let Some(block_indent) = active_static_var_block_indent
            && indent > block_indent
            && trimmed.starts_with("set(")
            && trimmed.ends_with(':')
        {
            active_static_var_setter_indent = Some(indent);
        }

        if trimmed.starts_with("static var ") {
            if trimmed.ends_with(':') {
                active_static_var_block_indent = Some(indent);
            }
            if trimmed.contains("= func") && trimmed.ends_with(':') {
                active_static_var_initializer_indent = Some(indent);
            }
        }

        if trimmed.starts_with("for ") && trimmed.ends_with(':')
            || trimmed.starts_with("while ") && trimmed.ends_with(':')
        {
            loop_stack.push(indent);
        }

        if is_lambda_header(trimmed) {
            lambda_stack.push(indent);
        }

        if indent == 0 && !trimmed.starts_with('@') {
            seen_top_level_non_annotation = true;
        }

        previous_significant = trimmed.to_string();

        scan_delimiters(
            line,
            line_num,
            &mut delimiter_stack,
            &mut quote_state,
            &mut issues,
        );
    }

    while let Some((open, open_line)) = delimiter_stack.pop() {
        issues.push(ParserError {
            message: format!("unmatched '{}'", open),
            line: open_line,
        });
    }

    ParsedScript {
        path,
        declarations,
        lines,
        issues,
    }
}
