use crate::docs_meta::{class_meta_header, node_method_meta_header, validate_metadata_headers};
use crate::parser::{ParsedScript, ScriptDecl, ScriptDeclKind};
use crate::type_system::{
    infer_expression_type as infer_expression_type_ts, infer_literal_type, infer_symbol_type,
    property_signature_for_receiver,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy)]
pub struct HoverWorkspaceDoc<'a> {
    pub uri: &'a str,
    pub script: &'a ParsedScript,
}

#[derive(Debug, Clone)]
struct NodeMethodDoc {
    name: String,
    class_name: String,
    signature: String,
    hover: String,
}

#[derive(Debug, Clone)]
pub struct MethodCompletion {
    pub name: String,
    pub class_name: String,
    pub signature: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct KnownSignature {
    pub label: String,
    pub parameters: Vec<String>,
    pub documentation: String,
}

#[derive(Debug, Clone)]
struct ClassDoc {
    inherits: Vec<String>,
    summary: String,
    note: Option<String>,
}

#[derive(Debug, Default)]
struct HoverReferences {
    by_type: HashMap<String, usize>,
    ordered_uris: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct DeclSummary {
    signature: Option<String>,
    decl_type: Option<String>,
    value: Option<String>,
    comments: Option<String>,
    parameters: Vec<String>,
    return_type: Option<String>,
    base_type: Option<String>,
    enum_members: Vec<String>,
}

#[derive(Debug, Clone)]
struct EnumDecl {
    name: String,
    line: usize,
    members: Vec<String>,
}

pub fn hover_at(line: usize, character: usize, script: &ParsedScript) -> Option<Hover> {
    hover_at_with_workspace(line, character, script, None, &[])
}

pub fn hover_at_with_workspace(
    line: usize,
    character: usize,
    script: &ParsedScript,
    current_uri: Option<&str>,
    workspace: &[HoverWorkspaceDoc<'_>],
) -> Option<Hover> {
    let line_text = script
        .lines
        .get(line.saturating_sub(1))
        .map(String::as_str)?;

    if let Some((symbol, start, _)) = identifier_range_at(line_text, character) {
        let type_context = is_type_context(line_text, start);

        if let Some(hover) = parameter_hover(script, &symbol, line) {
            return Some(hover);
        }

        if let Some(decl) = best_matching_decl(script, &symbol, line) {
            let resolved_type = infer_symbol_type(script, &symbol, line);
            return Some(declaration_hover(
                decl,
                script,
                None,
                resolved_type.as_deref(),
            ));
        }

        if let Some(inline_decl) = best_inline_binding_decl(script, &symbol, line) {
            let resolved_type = infer_symbol_type(script, &symbol, line);
            return Some(declaration_hover(
                &inline_decl,
                script,
                None,
                resolved_type.as_deref(),
            ));
        }

        if let Some(enum_decl) = find_enum_decl(script, &symbol) {
            return Some(enum_hover(&enum_decl, script, None));
        }

        if let Some((uri, decl, doc_script)) =
            best_workspace_decl(&symbol, type_context, current_uri, workspace)
        {
            return Some(declaration_hover(decl, doc_script, Some(uri), None));
        }

        if let Some((uri, enum_decl, doc_script)) =
            best_workspace_enum(&symbol, current_uri, workspace)
        {
            return Some(enum_hover(&enum_decl, doc_script, Some(uri)));
        }

        let receiver_type = receiver_type_for_member_access(line_text, start, script, line);
        if let Some(hover) = known_symbol_hover(&symbol, receiver_type.as_deref()) {
            return Some(hover);
        }
    }

    if let Some((literal_type, literal_value)) = literal_at(line_text, character) {
        return Some(Hover {
            title: format!("literal `{literal_value}`"),
            body: format!(
                "Type: `{literal_type}`\n\nValue: `{literal_value}`\n\nDeclared at `{}`:{}",
                script.path.display(),
                line
            ),
        });
    }

    script
        .declarations
        .iter()
        .find(|decl| decl.line == line)
        .map(|decl| declaration_hover(decl, script, None, None))
}

pub fn definition_uri_for_known_symbol(name: &str) -> Option<String> {
    definition_uris_for_known_symbol(name, None)
        .into_iter()
        .next()
}

pub fn definition_uris_for_known_symbol(name: &str, receiver_type: Option<&str>) -> Vec<String> {
    if crate::type_system::builtin_signature(name).is_some()
        || matches!(name, "print" | "preload" | "len")
    {
        return vec![globalscope_doc_uri(name)];
    }

    let method_candidates = method_candidates_for_hover(receiver_type, name, 5);
    if !method_candidates.is_empty() {
        let mut seen = HashSet::new();
        return method_candidates
            .into_iter()
            .filter_map(|method| {
                let uri = class_method_doc_uri(&method.class_name, &method.name);
                if seen.insert(uri.clone()) {
                    Some(uri)
                } else {
                    None
                }
            })
            .collect();
    }

    if let Some(receiver_type) = receiver_type
        && let Some(property) = property_signature_for_receiver(receiver_type, name)
    {
        return vec![class_property_doc_uri(&property.class_name, &property.name)];
    }

    known_type_doc_uri(name).into_iter().collect()
}

fn declaration_hover(
    decl: &ScriptDecl,
    script: &ParsedScript,
    uri: Option<&str>,
    inferred_type: Option<&str>,
) -> Hover {
    let summary = summarize_declaration(decl, script);
    let source_label = uri
        .map(str::to_string)
        .unwrap_or_else(|| script.path.to_string_lossy().to_string());

    if matches!(
        decl.kind,
        ScriptDeclKind::Variable | ScriptDeclKind::Constant
    ) {
        let mut sections = Vec::new();
        let display_type = inferred_type.or(summary.decl_type.as_deref());
        let snippet = binding_declaration_snippet(
            decl.kind,
            &decl.name,
            display_type,
            summary.value.as_deref(),
        );
        sections.push(format!("```gdscript\n{snippet}\n```"));
        if let Some(comments) = summary.comments {
            sections.push(comments);
        }
        sections.push(format!("Declared at `{source_label}`:{}", decl.line));

        return Hover {
            title: format!("{} '{}'", decl.kind.kind_label(), decl.name),
            body: sections.join("\n\n"),
        };
    }

    let mut sections = Vec::new();
    if let Some(signature) = summary.signature {
        sections.push(format!("Signature: `{signature}`"));
    }
    if let Some(decl_type) = summary.decl_type {
        sections.push(format!("Type: `{decl_type}`"));
    }
    if !summary.parameters.is_empty() {
        sections.push(format!("Parameters: {}", summary.parameters.join(", ")));
    }
    if let Some(return_type) = summary.return_type {
        sections.push(format!("Returns: `{return_type}`"));
    }
    if let Some(base_type) = summary.base_type {
        sections.push(format!("Inherits: `{base_type}`"));
    }
    if !summary.enum_members.is_empty() {
        sections.push(format!("Members: {}", summary.enum_members.join(", ")));
    }
    if let Some(comments) = summary.comments {
        sections.push(format!("Comments: {comments}"));
    }
    sections.push(format!("Declared at `{source_label}`:{}", decl.line));

    Hover {
        title: format!("{} '{}'", decl.kind.kind_label(), decl.name),
        body: sections.join("\n\n"),
    }
}

fn enum_hover(enum_decl: &EnumDecl, script: &ParsedScript, uri: Option<&str>) -> Hover {
    let mut sections = Vec::new();
    sections.push("Type: `enum`".to_string());
    if !enum_decl.members.is_empty() {
        sections.push(format!("Members: {}", enum_decl.members.join(", ")));
    }
    if let Some(comments) = declaration_comments(script, enum_decl.line) {
        sections.push(format!("Comments: {comments}"));
    }

    let source_label = uri
        .map(str::to_string)
        .unwrap_or_else(|| script.path.to_string_lossy().to_string());
    sections.push(format!("Declared at `{source_label}`:{}", enum_decl.line));

    Hover {
        title: format!("enum '{}'", enum_decl.name),
        body: sections.join("\n\n"),
    }
}

fn summarize_declaration(decl: &ScriptDecl, script: &ParsedScript) -> DeclSummary {
    let mut summary = DeclSummary {
        comments: declaration_comments(script, decl.line),
        ..DeclSummary::default()
    };

    match decl.kind {
        ScriptDeclKind::Variable | ScriptDeclKind::Constant => {
            let (_, code, _) = split_code_and_comment(&decl.text);
            let (decl_type, value) = parse_binding_type_and_value(&code);
            summary.decl_type = Some(decl_type.unwrap_or_else(|| "Variant".to_string()));
            summary.value = value;
        }
        ScriptDeclKind::Function => {
            let signature = function_signature(script, decl.line)
                .unwrap_or_else(|| decl.text.trim().to_string());
            if !signature.is_empty() {
                summary.signature = Some(signature.clone());
            }
            let params = parse_function_parameters(&signature);
            summary.parameters = params
                .iter()
                .map(|param| {
                    let mut row = param.name.clone();
                    if let Some(param_type) = &param.param_type {
                        row.push_str(": ");
                        row.push_str(param_type);
                    }
                    if let Some(default) = &param.default_value {
                        row.push_str(" = ");
                        row.push_str(default);
                    }
                    row
                })
                .collect();
            summary.return_type =
                Some(function_return_type(&signature).unwrap_or_else(|| "Variant".to_string()));
            summary.decl_type = Some("callable".to_string());
        }
        ScriptDeclKind::Class => {
            summary.decl_type = Some("type".to_string());
            summary.base_type =
                class_base_type(&decl.text).or_else(|| explicit_extends_in_file(script));
        }
    }

    summary
}

#[derive(Debug, Clone)]
struct FunctionParam {
    name: String,
    param_type: Option<String>,
    default_value: Option<String>,
}

fn parse_function_parameters(signature: &str) -> Vec<FunctionParam> {
    let Some(open_idx) = signature.find('(') else {
        return Vec::new();
    };

    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in signature.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(close_idx) = close_idx else {
        return Vec::new();
    };

    let param_src = signature[open_idx + 1..close_idx].trim();
    if param_src.is_empty() {
        return Vec::new();
    }

    split_top_level_commas(param_src)
        .into_iter()
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let segment = segment.strip_prefix("...").unwrap_or(segment).trim();
            let (left, default_value) = if let Some((left, right)) = segment.split_once('=') {
                (left.trim(), Some(right.trim().to_string()))
            } else {
                (segment, None)
            };

            let (name_part, param_type) = if let Some((name, ty)) = left.split_once(':') {
                (
                    name.trim(),
                    Some(ty.trim().to_string()).filter(|value| !value.is_empty()),
                )
            } else {
                (left.trim(), None)
            };

            let name = extract_identifier(name_part)?;
            Some(FunctionParam {
                name,
                param_type,
                default_value,
            })
        })
        .collect()
}

fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }

    out
}

fn function_signature(script: &ParsedScript, line: usize) -> Option<String> {
    let start = line.saturating_sub(1);
    if start >= script.lines.len() {
        return None;
    }

    let mut depth = 0isize;
    let mut saw_open = false;
    let mut chunks = Vec::new();

    for raw_line in script.lines.iter().skip(start) {
        let code = parse_code_prefix(raw_line).trim();
        if code.is_empty() {
            continue;
        }

        chunks.push(code.to_string());
        for ch in code.chars() {
            match ch {
                '(' => {
                    saw_open = true;
                    depth += 1;
                }
                ')' => depth -= 1,
                _ => {}
            }
        }

        if saw_open && depth <= 0 && code.contains(':') {
            break;
        }
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join(" "))
    }
}

fn class_base_type(class_decl_line: &str) -> Option<String> {
    let code = parse_code_prefix(class_decl_line).trim_start();
    let (_, extends_tail) = code.split_once(" extends ")?;
    let base = extends_tail
        .trim_end_matches(':')
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
        .next()
        .unwrap_or("")
        .trim();
    if base.is_empty() {
        None
    } else {
        Some(base.to_string())
    }
}

fn explicit_extends_in_file(script: &ParsedScript) -> Option<String> {
    script.lines.iter().find_map(|line| {
        let trimmed = parse_code_prefix(line).trim_start();
        let rest = trimmed.strip_prefix("extends ")?;
        let base = rest
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '.')
            .next()
            .unwrap_or("")
            .trim();
        if base.is_empty() {
            None
        } else {
            Some(base.to_string())
        }
    })
}

fn parse_binding_type_and_value(code_line: &str) -> (Option<String>, Option<String>) {
    let trimmed = strip_leading_annotations(code_line.trim_start());
    let rest = if let Some(rest) = trimmed.strip_prefix("var ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("const ") {
        rest
    } else {
        return (None, None);
    };

    let (lhs, rhs) = if let Some((lhs, rhs)) = rest.split_once(":=") {
        (lhs.trim(), Some(rhs.trim()))
    } else if let Some((lhs, rhs)) = rest.split_once('=') {
        (lhs.trim(), Some(rhs.trim()))
    } else {
        (rest.trim(), None)
    };

    let declared_type = lhs
        .split_once(':')
        .map(|(_, ty)| ty.trim())
        .filter(|ty| !ty.is_empty())
        .map(str::to_string);

    let value = rhs
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let inferred_type = declared_type
        .clone()
        .or_else(|| value.as_deref().and_then(infer_literal_type));

    (inferred_type, value)
}

fn binding_declaration_snippet(
    kind: ScriptDeclKind,
    name: &str,
    decl_type: Option<&str>,
    value: Option<&str>,
) -> String {
    let ty = decl_type.unwrap_or("Variant");
    let value = value.filter(|value| !value.is_empty());

    match kind {
        ScriptDeclKind::Variable => {
            if let Some(value) = value {
                format!("var {name}: {ty} = {value}")
            } else {
                format!("var {name}: {ty}")
            }
        }
        ScriptDeclKind::Constant => {
            if let Some(value) = value {
                format!("const {name}: {ty} = {value}")
            } else {
                format!("const {name}: {ty}")
            }
        }
        ScriptDeclKind::Function | ScriptDeclKind::Class => unreachable!(),
    }
}

fn parse_inline_binding_declaration(
    line_text: &str,
    line_num: usize,
) -> Option<crate::parser::ScriptDecl> {
    let code = parse_code_prefix(line_text).trim_start();
    let stripped = strip_leading_annotations(code);
    let (kind, rest) = if let Some(rest) = stripped.strip_prefix("var ") {
        (ScriptDeclKind::Variable, rest)
    } else if let Some(rest) = stripped.strip_prefix("const ") {
        (ScriptDeclKind::Constant, rest)
    } else {
        return None;
    };

    let name = extract_identifier(rest.trim_start())?;
    Some(crate::parser::ScriptDecl {
        kind,
        name,
        line: line_num,
        text: code.to_string(),
    })
}

fn best_inline_binding_decl(
    script: &ParsedScript,
    symbol: &str,
    line: usize,
) -> Option<crate::parser::ScriptDecl> {
    script
        .lines
        .iter()
        .enumerate()
        .take(line)
        .filter_map(|(idx, raw)| parse_inline_binding_declaration(raw, idx + 1))
        .filter(|decl| decl.name == symbol)
        .max_by_key(|decl| decl.line)
}

fn strip_leading_annotations(input: &str) -> &str {
    let mut rest = input.trim_start();
    loop {
        if !rest.starts_with('@') {
            return rest;
        }

        let bytes = rest.as_bytes();
        let mut idx = 1usize;
        while idx < bytes.len() && is_ident_char(bytes[idx]) {
            idx += 1;
        }
        if idx == 1 {
            return rest;
        }

        if idx < bytes.len() && bytes[idx] == b'(' {
            idx += 1;
            let mut depth = 1usize;
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
                        if ch == b'(' {
                            depth += 1;
                            idx += 1;
                            continue;
                        }
                        if ch == b')' {
                            depth = depth.saturating_sub(1);
                            idx += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                        idx += 1;
                    }
                }
            }

            if depth != 0 {
                return rest;
            }
        }

        rest = rest[idx..].trim_start();
    }
}

fn function_return_type(signature_line: &str) -> Option<String> {
    let (_, tail) = signature_line.split_once("->")?;
    let ty = tail.split(':').next()?.trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty.to_string())
    }
}

fn declaration_comments(script: &ParsedScript, line: usize) -> Option<String> {
    if line == 0 || line > script.lines.len() {
        return None;
    }

    let mut leading = Vec::new();
    let mut idx = line.saturating_sub(1);
    while idx > 0 {
        let candidate = script
            .lines
            .get(idx.saturating_sub(1))
            .map(String::as_str)
            .unwrap_or("");
        let trimmed = candidate.trim_start();
        if let Some(comment) = trimmed.strip_prefix('#') {
            leading.push(comment.trim().to_string());
            idx -= 1;
            continue;
        }
        break;
    }
    leading.reverse();

    let declaration_line = script
        .lines
        .get(line.saturating_sub(1))
        .map(String::as_str)
        .unwrap_or("");
    let (_, _, inline_comment) = split_code_and_comment(declaration_line);

    let mut parts = leading
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if let Some(inline_comment) = inline_comment.filter(|comment| !comment.is_empty()) {
        parts.push(inline_comment);
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\\n"))
    }
}

fn split_code_and_comment(line: &str) -> (String, String, Option<String>) {
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
                    let code = line[..idx].trim_end().to_string();
                    let comment = line[idx + 1..].trim().to_string();
                    let original = line.to_string();
                    return (original, code, Some(comment));
                }
                idx += 1;
            }
        }
    }

    (line.to_string(), line.to_string(), None)
}

fn parse_code_prefix(line: &str) -> &str {
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
                    return line[..idx].trim_end();
                }
                idx += 1;
            }
        }
    }

    line.trim_end()
}

fn best_matching_decl<'a>(
    script: &'a ParsedScript,
    symbol: &str,
    line: usize,
) -> Option<&'a ScriptDecl> {
    let target_scope = containing_function_line(script, line);
    let target_indent = script
        .lines
        .get(line.saturating_sub(1))
        .map(|line| line_indent(line))
        .unwrap_or(0);
    let target_scope_indent = target_scope.and_then(|scope_line| {
        script
            .lines
            .get(scope_line.saturating_sub(1))
            .map(|line| line_indent(line))
    });
    let mut best: Option<(&ScriptDecl, usize)> = None;

    for decl in script
        .declarations
        .iter()
        .filter(|decl| decl.name == symbol && decl.line <= line)
    {
        let decl_indent = script
            .lines
            .get(decl.line.saturating_sub(1))
            .map(|line| line_indent(line))
            .unwrap_or(0);
        if line > decl.line && decl_indent > target_indent {
            continue;
        }

        let decl_scope = containing_function_line(script, decl.line);
        if let (Some(target_scope_line), Some(decl_scope_line), Some(scope_indent)) =
            (target_scope, decl_scope, target_scope_indent)
            && target_scope_line == decl_scope_line
            && !decl_visible_in_function_scope(script, decl.line, line, scope_indent)
        {
            continue;
        }
        let scope_score = match (target_scope, decl_scope) {
            (Some(target), Some(candidate)) if target == candidate => 3,
            (None, None) => 2,
            (_, None) => 1,
            _ => 0,
        };

        let score = scope_score * 100_000 + decl.line;
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((decl, score));
        }
    }

    best.map(|(decl, _)| decl).or_else(|| {
        script
            .declarations
            .iter()
            .filter(|decl| decl.name == symbol)
            .min_by_key(|decl| decl.line)
    })
}

fn parameter_hover(script: &ParsedScript, symbol: &str, line: usize) -> Option<Hover> {
    let function_line = containing_function_line(script, line)?;
    let has_local_shadow = script.declarations.iter().any(|decl| {
        decl.name == symbol
            && decl.line < line
            && matches!(
                decl.kind,
                ScriptDeclKind::Variable | ScriptDeclKind::Constant
            )
            && containing_function_line(script, decl.line) == Some(function_line)
    });
    if has_local_shadow {
        return None;
    }

    let signature = function_signature(script, function_line)?;
    let params = parse_function_parameters(&signature);
    let param = params.into_iter().find(|param| param.name == symbol)?;

    let mut sections = Vec::new();
    let param_type = param
        .param_type
        .clone()
        .or_else(|| param.default_value.as_deref().and_then(infer_literal_type))
        .unwrap_or_else(|| "Variant".to_string());
    sections.push(format!("Type: `{param_type}`"));
    if let Some(default_value) = &param.default_value {
        sections.push(format!("Value: `{default_value}`"));
    }
    sections.push(format!("Function: `{}`", signature));
    sections.push(format!(
        "Declared at `{}`:{}",
        script.path.display(),
        function_line
    ));

    Some(Hover {
        title: format!("parameter '{}'", param.name),
        body: sections.join("\n\n"),
    })
}

fn containing_function_line(script: &ParsedScript, target_line: usize) -> Option<usize> {
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for (idx, raw_line) in script.lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = parse_code_prefix(raw_line).trim_start();
        let indent = line_indent(raw_line);

        while let Some((scope_line, scope_indent)) = stack.last().copied() {
            if line_num > scope_line && !trimmed.is_empty() && indent <= scope_indent {
                stack.pop();
            } else {
                break;
            }
        }

        if is_function_header(trimmed) {
            stack.push((line_num, indent));
        }

        if line_num == target_line {
            return stack.last().map(|(scope_line, _)| *scope_line);
        }
    }

    None
}

fn is_function_header(trimmed: &str) -> bool {
    trimmed.starts_with("func ") || trimmed.starts_with("static func ")
}

fn decl_visible_in_function_scope(
    script: &ParsedScript,
    decl_line: usize,
    target_line: usize,
    function_indent: usize,
) -> bool {
    if decl_line > target_line {
        return false;
    }

    let decl_indent = script
        .lines
        .get(decl_line.saturating_sub(1))
        .map(|line| line_indent(line))
        .unwrap_or(0);

    if decl_indent <= function_indent {
        return true;
    }

    for raw_line in script
        .lines
        .iter()
        .skip(decl_line)
        .take(target_line.saturating_sub(decl_line))
    {
        let trimmed = parse_code_prefix(raw_line).trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line_indent(raw_line);
        if indent < decl_indent {
            return false;
        }
    }

    true
}

fn line_indent(line: &str) -> usize {
    line.chars()
        .take_while(|ch| ch.is_ascii_whitespace())
        .count()
}

fn find_enum_decl(script: &ParsedScript, name: &str) -> Option<EnumDecl> {
    enum_declarations(script)
        .into_iter()
        .find(|enum_decl| enum_decl.name == name)
}

fn enum_declarations(script: &ParsedScript) -> Vec<EnumDecl> {
    let mut out = Vec::new();
    let mut idx = 0usize;

    while idx < script.lines.len() {
        let line_num = idx + 1;
        let trimmed = parse_code_prefix(script.lines[idx].as_str()).trim_start();
        if !trimmed.starts_with("enum ") {
            idx += 1;
            continue;
        }

        let rest = trimmed.trim_start_matches("enum ").trim_start();
        if rest.starts_with('{') {
            idx += 1;
            continue;
        }

        let Some(name) = extract_identifier(rest) else {
            idx += 1;
            continue;
        };

        let mut body_text = String::new();
        let mut brace_depth = 0isize;
        let mut saw_open = false;

        for line in script.lines.iter().skip(idx) {
            let code = parse_code_prefix(line);
            if !body_text.is_empty() {
                body_text.push(' ');
            }
            body_text.push_str(code.trim());

            for ch in code.chars() {
                match ch {
                    '{' => {
                        saw_open = true;
                        brace_depth += 1;
                    }
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }

            if saw_open && brace_depth <= 0 {
                break;
            }
        }

        let members = parse_enum_members(&body_text);
        out.push(EnumDecl {
            name,
            line: line_num,
            members,
        });

        idx += 1;
    }

    out
}

fn parse_enum_members(input: &str) -> Vec<String> {
    let Some(open_idx) = input.find('{') else {
        return Vec::new();
    };
    let Some(close_idx) = input.rfind('}') else {
        return Vec::new();
    };
    if close_idx <= open_idx + 1 {
        return Vec::new();
    }

    split_top_level_commas(&input[open_idx + 1..close_idx])
        .into_iter()
        .filter_map(|entry| {
            let token = entry.split('=').next().unwrap_or("").trim();
            if token.is_empty() {
                None
            } else {
                extract_identifier(token)
            }
        })
        .collect()
}

fn best_workspace_decl<'a>(
    symbol: &str,
    type_context: bool,
    current_uri: Option<&str>,
    workspace: &'a [HoverWorkspaceDoc<'a>],
) -> Option<(&'a str, &'a ScriptDecl, &'a ParsedScript)> {
    let mut best: Option<(&str, &ScriptDecl, &ParsedScript, usize)> = None;

    for doc in workspace {
        if current_uri.is_some_and(|uri| uri == doc.uri) {
            continue;
        }

        for decl in doc
            .script
            .declarations
            .iter()
            .filter(|decl| decl.name == symbol)
        {
            let kind_score = match (type_context, decl.kind) {
                (true, ScriptDeclKind::Class) => 4,
                (true, ScriptDeclKind::Constant) => 3,
                (true, ScriptDeclKind::Function) => 2,
                (true, ScriptDeclKind::Variable) => 1,
                (false, ScriptDeclKind::Variable) => 4,
                (false, ScriptDeclKind::Constant) => 3,
                (false, ScriptDeclKind::Function) => 2,
                (false, ScriptDeclKind::Class) => 1,
            };
            let score = kind_score * 100_000 + (100_000usize.saturating_sub(decl.line));
            if best.is_none_or(|(_, _, _, best_score)| score > best_score) {
                best = Some((doc.uri, decl, doc.script, score));
            }
        }
    }

    best.map(|(uri, decl, script, _)| (uri, decl, script))
}

fn best_workspace_enum<'a>(
    symbol: &str,
    current_uri: Option<&str>,
    workspace: &'a [HoverWorkspaceDoc<'a>],
) -> Option<(&'a str, EnumDecl, &'a ParsedScript)> {
    for doc in workspace {
        if current_uri.is_some_and(|uri| uri == doc.uri) {
            continue;
        }

        if let Some(enum_decl) = find_enum_decl(doc.script, symbol) {
            return Some((doc.uri, enum_decl, doc.script));
        }
    }

    None
}

fn is_type_context(line_text: &str, symbol_start: usize) -> bool {
    let prefix = line_text[..symbol_start].trim_end();
    prefix.ends_with(':') || prefix.ends_with("->") || prefix.ends_with(" as")
}

fn known_symbol_hover(name: &str, receiver_type: Option<&str>) -> Option<Hover> {
    if let Some((signature, body)) = crate::type_system::builtin_signature(name) {
        return Some(Hover {
            title: format!("builtin {signature}"),
            body,
        });
    }

    let method_candidates = method_candidates_for_hover(receiver_type, name, 5);
    if let Some(method) = method_candidates.first() {
        if receiver_type.is_none() && method_candidates.len() > 1 {
            let rows = method_candidates
                .iter()
                .map(|candidate| {
                    format!("- `{}` on `{}`", candidate.signature, candidate.class_name)
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Some(Hover {
                title: format!("ambiguous method `{name}`"),
                body: format!(
                    "Multiple Godot methods match `{name}` without a typed receiver.\n\n{rows}\n\nProvide a typed receiver for exact docs."
                ),
            });
        }

        return Some(Hover {
            title: format!("{} method {}", method.class_name, method.signature),
            body: normalize_godot_bbcode(&method.hover),
        });
    }

    if let Some(receiver_type) = receiver_type
        && let Some(property) = property_signature_for_receiver(receiver_type, name)
    {
        let mut body_sections = vec![format!("Type: `{}`", property.property_type)];
        if !property.documentation.trim().is_empty() {
            body_sections.push(normalize_godot_bbcode(&property.documentation));
        }

        return Some(Hover {
            title: format!(
                "{} property {}: {}",
                property.class_name, property.name, property.property_type
            ),
            body: body_sections.join("\n\n"),
        });
    }

    if let Some(class_doc) = class_hover_metadata().get(name) {
        let mut references = HoverReferences::default();
        let mut sections = Vec::new();
        let chain = type_ancestry(name);
        sections.push(class_hierarchy_block(chain.as_slice(), &mut references));

        if !class_doc.summary.is_empty() {
            sections.push(linkify_known_types(
                &normalize_godot_bbcode(&class_doc.summary),
                &mut references,
            ));
        }
        if let Some(note) = &class_doc.note {
            sections.push(format!(
                "Note: {}",
                linkify_known_types(&normalize_godot_bbcode(note), &mut references)
            ));
        }
        if let Some(index) = references.reference_index_for_type(name) {
            sections.push(format!("Docs: [{index}]"));
        }
        if let Some(footer) = references.render_section() {
            sections.push(footer);
        }

        return Some(Hover {
            title: format!("class {name}"),
            body: sections.join("\n\n"),
        });
    }

    if known_type_doc_uri(name).is_some() {
        let mut references = HoverReferences::default();
        let chain = type_ancestry(name);
        let mut sections = Vec::new();
        sections.push(class_hierarchy_block(chain.as_slice(), &mut references));
        if let Some(index) = references.reference_index_for_type(name) {
            sections.push(format!("Docs: [{index}]"));
        }
        if let Some(footer) = references.render_section() {
            sections.push(footer);
        }

        return Some(Hover {
            title: format!("class {name}"),
            body: sections.join("\n\n"),
        });
    }

    match name {
        "print" => Some(Hover {
            title: "builtin print(...)".to_string(),
            body: "GDScript builtin: prints values to output".to_string(),
        }),
        "preload" => Some(Hover {
            title: "builtin preload(path)".to_string(),
            body: "GDScript builtin: loads a resource at parse time".to_string(),
        }),
        "len" => Some(Hover {
            title: "builtin len(value)".to_string(),
            body: "GDScript builtin: returns collection length".to_string(),
        }),
        _ => None,
    }
}

pub fn receiver_type_at_position(
    line: usize,
    character: usize,
    script: &ParsedScript,
) -> Option<String> {
    let line_text = script
        .lines
        .get(line.saturating_sub(1))
        .map(String::as_str)?;
    let cursor = line_byte_offset(line_text, character);
    let bytes = line_text.as_bytes();
    let mut start = cursor.min(bytes.len());
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }

    receiver_type_for_member_access(line_text, start, script, line)
}

fn receiver_type_for_member_access(
    line_text: &str,
    symbol_start: usize,
    script: &ParsedScript,
    line: usize,
) -> Option<String> {
    let receiver_expr = member_access_receiver_expression(line_text, symbol_start)?;
    infer_expression_type_ts(script, &receiver_expr, line)
}

fn member_access_receiver_expression(line_text: &str, symbol_start: usize) -> Option<String> {
    let bytes = line_text.as_bytes();
    if symbol_start == 0 || symbol_start > bytes.len() {
        return None;
    }

    let mut idx = symbol_start;
    while idx > 0 && bytes[idx - 1].is_ascii_whitespace() {
        idx -= 1;
    }
    if idx == 0 || bytes[idx - 1] != b'.' {
        return None;
    }
    let dot_idx = idx - 1;
    let receiver_prefix = &line_text[..dot_idx];
    let expr = trailing_receiver_expression(receiver_prefix)?;
    if expr.is_empty() { None } else { Some(expr) }
}

fn trailing_receiver_expression(prefix: &str) -> Option<String> {
    let bytes = prefix.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }

    let mut start = end;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;

    while start > 0 {
        let ch = bytes[start - 1];

        if ch == b')' {
            paren_depth += 1;
            start -= 1;
            continue;
        }
        if ch == b'(' {
            if paren_depth > 0 {
                paren_depth -= 1;
                start -= 1;
                continue;
            }
            break;
        }
        if ch == b']' {
            bracket_depth += 1;
            start -= 1;
            continue;
        }
        if ch == b'[' {
            if bracket_depth > 0 {
                bracket_depth -= 1;
                start -= 1;
                continue;
            }
            break;
        }
        if paren_depth > 0 || bracket_depth > 0 {
            start -= 1;
            continue;
        }

        if is_ident_char(ch) || matches!(ch, b'.' | b':' | b'$' | b'%') {
            start -= 1;
            continue;
        }
        if ch.is_ascii_whitespace() {
            break;
        }

        break;
    }

    let expr = prefix[start..end].trim();
    if expr.is_empty() {
        None
    } else {
        Some(expr.to_string())
    }
}

fn method_candidates_for_receiver(
    receiver_type: Option<&str>,
    method_name: &str,
) -> Vec<NodeMethodDoc> {
    crate::type_system::method_candidates_for_receiver(receiver_type, method_name)
        .into_iter()
        .map(|method| NodeMethodDoc {
            name: method.name,
            class_name: method.class_name,
            signature: method.signature,
            hover: method.hover,
        })
        .collect()
}

fn method_candidates_for_hover(
    receiver_type: Option<&str>,
    method_name: &str,
    max: usize,
) -> Vec<NodeMethodDoc> {
    let mut methods = method_candidates_for_receiver(receiver_type, method_name);
    methods.truncate(max);
    methods
}

pub fn method_completions_for_receiver(
    receiver_type: &str,
    prefix: Option<&str>,
    max: usize,
) -> Vec<MethodCompletion> {
    let ancestry = type_ancestry(receiver_type);
    let rank = ancestry
        .iter()
        .enumerate()
        .map(|(idx, ty)| (ty.clone(), idx))
        .collect::<HashMap<_, _>>();

    let mut best_by_name: HashMap<String, (usize, NodeMethodDoc)> = HashMap::new();
    for methods in node_method_hover_metadata().values() {
        for method in methods {
            if let Some(prefix) = prefix
                && !method.name.starts_with(prefix)
            {
                continue;
            }
            let Some(depth) = rank.get(&method.class_name).copied() else {
                continue;
            };

            let entry = best_by_name
                .entry(method.name.clone())
                .or_insert_with(|| (usize::MAX, method.clone()));
            if depth < entry.0
                || (depth == entry.0 && method.signature.len() < entry.1.signature.len())
            {
                *entry = (depth, method.clone());
            }
        }
    }

    let mut out = best_by_name
        .into_values()
        .map(|(_, method)| MethodCompletion {
            name: method.name.clone(),
            class_name: method.class_name.clone(),
            signature: method.signature.clone(),
            detail: normalize_godot_bbcode(&method.hover),
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.signature.cmp(&b.signature)));
    out.truncate(max);
    out
}

pub fn known_signatures_for_symbol(
    name: &str,
    receiver_type: Option<&str>,
    max: usize,
) -> Vec<KnownSignature> {
    if max == 0 {
        return Vec::new();
    }

    if let Some((signature, body)) = crate::type_system::builtin_signature(name) {
        return vec![KnownSignature {
            label: signature.clone(),
            parameters: signature_parameter_labels(&signature),
            documentation: body,
        }];
    }

    let mut out = method_candidates_for_hover(receiver_type, name, max)
        .into_iter()
        .map(|method| {
            let label = if receiver_type.is_some() {
                method.signature.clone()
            } else {
                format!("{} [{}]", method.signature, method.class_name)
            };

            KnownSignature {
                label,
                parameters: signature_parameter_labels(&method.signature),
                documentation: normalize_godot_bbcode(&method.hover),
            }
        })
        .collect::<Vec<_>>();

    if out.is_empty() {
        out = fallback_builtin_signature(name)
            .into_iter()
            .take(max)
            .collect();
    }

    out.truncate(max);
    out
}

fn fallback_builtin_signature(name: &str) -> Option<KnownSignature> {
    match name {
        "print" => Some(KnownSignature {
            label: "print(...)".to_string(),
            parameters: vec!["...".to_string()],
            documentation: "GDScript builtin: prints values to output".to_string(),
        }),
        "preload" => Some(KnownSignature {
            label: "preload(path)".to_string(),
            parameters: vec!["path".to_string()],
            documentation: "GDScript builtin: loads a resource at parse time".to_string(),
        }),
        "len" => Some(KnownSignature {
            label: "len(value)".to_string(),
            parameters: vec!["value".to_string()],
            documentation: "GDScript builtin: returns collection length".to_string(),
        }),
        _ => None,
    }
}

fn signature_parameter_labels(signature: &str) -> Vec<String> {
    let Some(open_idx) = signature.find('(') else {
        return Vec::new();
    };

    let mut depth = 0usize;
    let mut close_idx = None;
    for (idx, ch) in signature.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close_idx) = close_idx else {
        return Vec::new();
    };

    let raw = signature[open_idx + 1..close_idx].trim();
    if raw.is_empty() {
        return Vec::new();
    }

    split_top_level_commas(raw)
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let cleaned = segment.strip_prefix("...").unwrap_or(&segment).trim();
            cleaned
                .split_once('=')
                .map(|(left, _)| left.trim().to_string())
                .filter(|left| !left.is_empty())
                .unwrap_or_else(|| cleaned.to_string())
        })
        .collect()
}

fn type_ancestry(ty: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(ty.to_string());

    while let Some(name) = current {
        if !seen.insert(name.clone()) {
            break;
        }
        out.push(name.clone());
        current = class_hover_metadata()
            .get(&name)
            .and_then(|class_doc| class_doc.inherits.first())
            .cloned();
    }

    out
}

fn literal_at(line: &str, character: usize) -> Option<(String, String)> {
    if line.is_empty() {
        return None;
    }

    let mut indices = Vec::new();
    let mut byte_index = character.saturating_sub(1);
    if byte_index >= line.len() {
        byte_index = line.len().saturating_sub(1);
    }
    indices.push(byte_index);
    if byte_index > 0 {
        indices.push(byte_index - 1);
    }

    let spans = literal_spans(line);
    for idx in indices {
        if let Some((literal_type, literal_value, _start, _end)) = spans
            .iter()
            .find(|(_, _, start, end)| idx >= *start && idx < *end)
        {
            return Some((literal_type.clone(), literal_value.clone()));
        }
    }

    None
}

fn literal_spans(line: &str) -> Vec<(String, String, usize, usize)> {
    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        let ch = bytes[idx];
        if ch == b'#' {
            break;
        }

        if ch == b'\'' || ch == b'"' {
            let quote = ch;
            let start = idx;
            idx += 1;
            let mut escaped = false;
            while idx < bytes.len() {
                let current = bytes[idx];
                if escaped {
                    escaped = false;
                    idx += 1;
                    continue;
                }
                if current == b'\\' {
                    escaped = true;
                    idx += 1;
                    continue;
                }
                if current == quote {
                    idx += 1;
                    break;
                }
                idx += 1;
            }
            let end = idx.min(line.len());
            spans.push((
                "String".to_string(),
                line[start..end].to_string(),
                start,
                end,
            ));
            continue;
        }

        if ch.is_ascii_digit() {
            let start = idx;
            idx += 1;
            let mut seen_dot = false;
            while idx < bytes.len() {
                let current = bytes[idx];
                if current == b'_' || current.is_ascii_digit() {
                    idx += 1;
                    continue;
                }
                if current == b'.' && !seen_dot {
                    seen_dot = true;
                    idx += 1;
                    continue;
                }
                break;
            }
            let end = idx;
            let raw = line[start..end].replace('_', "");
            let literal_type = if seen_dot { "float" } else { "int" };
            spans.push((literal_type.to_string(), raw, start, end));
            continue;
        }

        if is_ident_char(ch) {
            let start = idx;
            idx += 1;
            while idx < bytes.len() && is_ident_char(bytes[idx]) {
                idx += 1;
            }
            let end = idx;
            let token = &line[start..end];
            let literal_type = match token {
                "true" | "false" => Some("bool"),
                "null" => Some("Variant"),
                _ => None,
            };
            if let Some(literal_type) = literal_type {
                spans.push((literal_type.to_string(), token.to_string(), start, end));
            }
            continue;
        }

        idx += 1;
    }

    spans
}

fn identifier_range_at(line: &str, character: usize) -> Option<(String, usize, usize)> {
    if line.is_empty() {
        return None;
    }

    let mut byte_index = character.saturating_sub(1);
    if byte_index >= line.len() {
        byte_index = line.len().saturating_sub(1);
    }

    let bytes = line.as_bytes();
    while byte_index > 0 && !is_ident_char(bytes[byte_index]) {
        byte_index -= 1;
    }

    if !is_ident_char(bytes[byte_index]) {
        return None;
    }

    let mut start = byte_index;
    let mut end = byte_index;

    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    while end + 1 < bytes.len() && is_ident_char(bytes[end + 1]) {
        end += 1;
    }

    Some((line[start..=end].to_string(), start, end + 1))
}

fn line_byte_offset(line: &str, character: usize) -> usize {
    line.char_indices()
        .map(|(idx, _)| idx)
        .nth(character.saturating_sub(1))
        .unwrap_or(line.len())
}

fn globalscope_doc_uri(name: &str) -> String {
    let method = name.to_ascii_lowercase().replace('_', "-");
    format!(
        "https://docs.godotengine.org/en/stable/classes/class_@globalscope.html#class-@globalscope-method-{method}"
    )
}

fn class_method_doc_uri(class_name: &str, method_name: &str) -> String {
    let class = class_name.to_ascii_lowercase();
    let method = method_name.to_ascii_lowercase().replace('_', "-");
    format!(
        "https://docs.godotengine.org/en/stable/classes/class_{class}.html#class-{class}-method-{method}"
    )
}

fn class_property_doc_uri(class_name: &str, property_name: &str) -> String {
    let class = class_name.to_ascii_lowercase();
    let property = property_name.to_ascii_lowercase().replace('_', "-");
    format!(
        "https://docs.godotengine.org/en/stable/classes/class_{class}.html#class-{class}-property-{property}"
    )
}

fn known_type_doc_uri(name: &str) -> Option<String> {
    if !is_type_name(name) {
        return None;
    }

    let class = name.to_ascii_lowercase();
    Some(format!(
        "https://docs.godotengine.org/en/stable/classes/class_{class}.html"
    ))
}

fn is_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

impl HoverReferences {
    fn reference_index_for_type(&mut self, name: &str) -> Option<usize> {
        let uri = known_type_doc_uri(name)?;
        if let Some(index) = self.by_type.get(name) {
            return Some(*index);
        }

        let next = self.ordered_uris.len() + 1;
        self.by_type.insert(name.to_string(), next);
        self.ordered_uris.push(uri);
        Some(next)
    }

    fn render_section(&self) -> Option<String> {
        if self.ordered_uris.is_empty() {
            return None;
        }

        let mut lines = Vec::with_capacity(self.ordered_uris.len() + 1);
        lines.push("**References**".to_string());
        for (idx, uri) in self.ordered_uris.iter().enumerate() {
            lines.push(format!("[{}] {}", idx + 1, uri));
        }
        Some(lines.join("\n"))
    }
}

fn linked_type(name: &str, references: &mut HoverReferences) -> String {
    if let Some(index) = references.reference_index_for_type(name) {
        format!("{name}[{index}]")
    } else {
        name.to_string()
    }
}

fn linkify_known_types(text: &str, references: &mut HoverReferences) -> String {
    let known_types = known_doc_type_names();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 32);
    let mut idx = 0usize;

    while idx < bytes.len() {
        if is_ident_char(bytes[idx]) {
            let start = idx;
            idx += 1;
            while idx < bytes.len() && is_ident_char(bytes[idx]) {
                idx += 1;
            }
            let token = &text[start..idx];
            if known_types.contains(token) {
                out.push_str(&linked_type(token, references));
            } else {
                out.push_str(token);
            }
            continue;
        }

        out.push(bytes[idx] as char);
        idx += 1;
    }

    out
}

fn class_hierarchy_block(chain: &[String], references: &mut HoverReferences) -> String {
    let hierarchy = chain
        .iter()
        .map(|class_name| linked_type(class_name, references))
        .collect::<Vec<_>>()
        .join(" < ");
    format!("**Class Hierarchy**\n{hierarchy}")
}

fn normalize_godot_bbcode(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(input.len() + 32);
    let mut idx = 0usize;
    let bytes = input.as_bytes();
    let mut closing_markers: Vec<(String, String)> = Vec::new();

    while idx < bytes.len() {
        if bytes[idx] != b'[' {
            out.push(bytes[idx] as char);
            idx += 1;
            continue;
        }

        let Some(end_rel) = input[idx + 1..].find(']') else {
            out.push('[');
            idx += 1;
            continue;
        };
        let end = idx + 1 + end_rel;
        let tag = input[idx + 1..end].trim();
        idx = end + 1;

        if tag.is_empty() {
            continue;
        }

        if let Some(closing) = tag.strip_prefix('/') {
            if let Some(pos) = closing_markers
                .iter()
                .rposition(|(name, _)| name == closing.trim())
            {
                let (_, close_text) = closing_markers.remove(pos);
                out.push_str(&close_text);
            }
            continue;
        }

        if tag == "b" {
            out.push_str("**");
            closing_markers.push(("b".to_string(), "**".to_string()));
            continue;
        }
        if tag == "i" {
            out.push('*');
            closing_markers.push(("i".to_string(), "*".to_string()));
            continue;
        }
        if tag == "code" {
            out.push('`');
            closing_markers.push(("code".to_string(), "`".to_string()));
            continue;
        }
        if tag == "codeblock" {
            out.push_str("\n```gdscript\n");
            closing_markers.push(("codeblock".to_string(), "\n```\n".to_string()));
            continue;
        }
        if tag == "br" {
            out.push('\n');
            continue;
        }
        if let Some(url) = tag.strip_prefix("url=") {
            closing_markers.push(("url".to_string(), format!(" ({})", url.trim())));
            continue;
        }
        if tag == "url" {
            closing_markers.push(("url".to_string(), String::new()));
            continue;
        }

        let inline_ref = [
            "method", "member", "constant", "enum", "param", "signal", "class",
        ];
        let mut replaced = false;
        for key in inline_ref {
            if let Some(value) = tag.strip_prefix(&format!("{key} ")) {
                out.push('`');
                out.push_str(value.trim());
                out.push('`');
                replaced = true;
                break;
            }
        }
        if replaced {
            continue;
        }

        // Godot docs use bare [TypeName] references heavily.
        if is_type_name(tag) {
            out.push_str(tag);
            continue;
        }
        if tag.starts_with('@') {
            out.push('`');
            out.push_str(tag);
            out.push('`');
            continue;
        }
    }

    while let Some((_, close_text)) = closing_markers.pop() {
        out.push_str(&close_text);
    }

    out = out
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\\n", "\n")
        .replace("[/codeblock]", "")
        .replace("[/code]", "")
        .replace("[/b]", "")
        .replace("[/i]", "");
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out.trim().to_string()
}

fn extract_identifier(input: &str) -> Option<String> {
    let token = input
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if token.is_empty() {
        return None;
    }

    let mut chars = token.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    Some(token)
}

fn node_method_hover_metadata() -> &'static HashMap<String, Vec<NodeMethodDoc>> {
    static NODE_METHOD_META: OnceLock<HashMap<String, Vec<NodeMethodDoc>>> = OnceLock::new();
    NODE_METHOD_META.get_or_init(|| {
        let mut out = HashMap::new();
        let source = include_str!("../data/godot_4_6_node_method_meta.tsv");
        validate_metadata_headers(
            "godot_4_6_node_method_meta.tsv",
            source,
            node_method_meta_header(),
        )
        .unwrap_or_else(|error| panic!("{error}"));

        for line in source.lines().skip(1) {
            let mut fields = line.splitn(4, '\t');
            let Some(name) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(class_name) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(signature) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(hover) = fields.next().map(str::trim) else {
                continue;
            };
            if name.is_empty() || class_name.is_empty() || signature.is_empty() || hover.is_empty()
            {
                continue;
            }
            out.entry(name.to_string())
                .or_insert_with(Vec::new)
                .push(NodeMethodDoc {
                    name: name.to_string(),
                    class_name: class_name.to_string(),
                    signature: signature.to_string(),
                    hover: hover.to_string(),
                });
        }

        for methods in out.values_mut() {
            methods.sort_by(|a, b| a.class_name.cmp(&b.class_name));
            methods.dedup_by(|a, b| {
                a.name == b.name
                    && a.class_name == b.class_name
                    && a.signature == b.signature
                    && a.hover == b.hover
            });
        }
        out
    })
}

fn class_hover_metadata() -> &'static HashMap<String, ClassDoc> {
    static CLASS_META: OnceLock<HashMap<String, ClassDoc>> = OnceLock::new();
    CLASS_META.get_or_init(|| {
        let mut out = HashMap::new();
        let source = include_str!("../data/godot_4_6_class_meta.tsv");
        validate_metadata_headers("godot_4_6_class_meta.tsv", source, class_meta_header())
            .unwrap_or_else(|error| panic!("{error}"));

        for line in source.lines().skip(1) {
            let mut fields = line.splitn(4, '\t');
            let Some(name) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(inherits_raw) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(summary) = fields.next().map(str::trim) else {
                continue;
            };
            let note = fields
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            if name.is_empty() {
                continue;
            }

            let inherits = inherits_raw
                .split('>')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();

            out.insert(
                name.to_string(),
                ClassDoc {
                    inherits,
                    summary: summary.to_string(),
                    note,
                },
            );
        }
        out
    })
}

fn known_doc_type_names() -> &'static HashSet<String> {
    static KNOWN_DOC_TYPES: OnceLock<HashSet<String>> = OnceLock::new();
    KNOWN_DOC_TYPES.get_or_init(|| {
        let mut out = HashSet::new();

        for (name, class_doc) in class_hover_metadata() {
            out.insert(name.to_string());
            for inherited in &class_doc.inherits {
                out.insert(inherited.clone());
            }
        }

        for methods in node_method_hover_metadata().values() {
            for method in methods {
                out.insert(method.class_name.clone());
            }
        }

        out
    })
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

impl ScriptDeclKind {
    fn kind_label(&self) -> &'static str {
        match self {
            ScriptDeclKind::Function => "function",
            ScriptDeclKind::Class => "class",
            ScriptDeclKind::Variable => "variable",
            ScriptDeclKind::Constant => "constant",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HoverWorkspaceDoc, definition_uri_for_known_symbol, hover_at, hover_at_with_workspace,
    };
    use crate::parse_script;

    #[test]
    fn known_symbol_definition_uri_supports_node_methods() {
        let uri = definition_uri_for_known_symbol("queue_free").expect("queue_free docs uri");
        assert!(
            uri.contains("class_node"),
            "queue_free should map to Node docs uri: {uri}"
        );
    }

    #[test]
    fn local_value_hover_includes_type_value_and_comments() {
        let source = "# Movement speed\nvar speed: float = 3.5 # units per second\nfunc _ready():\n    print(speed)\n";
        let parsed = parse_script(source, "hover_local_value.gd");
        let hover = hover_at(4, 11, &parsed).expect("hover response");

        assert!(
            hover
                .body
                .contains("```gdscript\nvar speed: float = 3.5\n```"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("Movement speed"), "hover: {hover:#?}");
        assert!(hover.body.contains("units per second"), "hover: {hover:#?}");
    }

    #[test]
    fn known_type_hover_includes_hierarchy_and_links() {
        let source = "var tree: AnimationTree\n";
        let parsed = parse_script(source, "hover_known_type.gd");
        let hover = hover_at(1, 14, &parsed).expect("hover response");

        assert!(
            hover.body.contains("**Class Hierarchy**"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("AnimationTree[1]"), "hover: {hover:#?}");
        assert!(
            hover.body.contains("< AnimationMixer[2]"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("< Node[3]"), "hover: {hover:#?}");
        assert!(hover.body.contains("AnimationPlayer"), "hover: {hover:#?}");
        assert!(hover.body.contains("**References**"), "hover: {hover:#?}");
        assert!(
            hover.body.contains(
                "[1] https://docs.godotengine.org/en/stable/classes/class_animationtree.html"
            ),
            "hover: {hover:#?}"
        );
        assert!(
            hover.body.contains("class_animationplayer.html"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn fallback_known_type_hover_uses_hierarchy_block() {
        let source = "var bus: AudioServer\n";
        let parsed = parse_script(source, "hover_fallback_type.gd");
        let hover = hover_at(1, 13, &parsed).expect("hover response");

        assert!(
            hover.body.contains("**Class Hierarchy**"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("AudioServer[1]"), "hover: {hover:#?}");
        assert!(hover.body.contains("Docs: [1]"), "hover: {hover:#?}");
        assert!(hover.body.contains("**References**"), "hover: {hover:#?}");
        assert!(
            hover.body.contains(
                "[1] https://docs.godotengine.org/en/stable/classes/class_audioserver.html"
            ),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn typed_receiver_method_hover_uses_method_docs() {
        let source = "func _ready():\n    var _rng: RandomNumberGenerator = RandomNumberGenerator.new()\n    _rng.randomize()\n";
        let parsed = parse_script(source, "hover_typed_receiver_method.gd");
        let hover = hover_at(3, 12, &parsed).expect("hover response");

        assert!(
            hover
                .title
                .contains("RandomNumberGenerator method randomize() -> void"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("time-based seed"), "hover: {hover:#?}");
    }

    #[test]
    fn typed_receiver_method_hover_resolves_inherited_method_docs() {
        let source = "var node_ref: Node3D\nfunc _ready():\n    node_ref.queue_free()\n";
        let parsed = parse_script(source, "hover_inherited_method.gd");
        let hover = hover_at(3, 15, &parsed).expect("hover response");

        assert!(
            hover.title.contains("Node method queue_free() -> void"),
            "hover: {hover:#?}"
        );
        assert!(
            hover.body.contains("Queues this node to be deleted"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn typed_receiver_method_hover_resolves_chained_call_docs() {
        let source = "func _make_rng() -> RandomNumberGenerator:\n    return RandomNumberGenerator.new()\n\nfunc _ready():\n    _make_rng().randomize()\n";
        let parsed = parse_script(source, "hover_chained_call_method.gd");
        let hover = hover_at(5, 19, &parsed).expect("hover response");

        assert!(
            hover
                .title
                .contains("RandomNumberGenerator method randomize() -> void"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn typed_receiver_property_hover_resolves_property_type_and_docs() {
        let source = "func _ready() -> void:\n    var _player: AudioStreamPlayer = AudioStreamPlayer.new()\n    _player.stream.get_length()\n";
        let parsed = parse_script(source, "hover_typed_receiver_property.gd");
        let hover = hover_at(3, 14, &parsed).expect("hover response");

        assert!(
            hover
                .title
                .contains("AudioStreamPlayer property stream: AudioStream"),
            "hover: {hover:#?}"
        );
        assert!(
            hover.body.contains("Type: `AudioStream`"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn chained_property_receiver_method_hover_uses_property_type() {
        let source = "func _ready() -> void:\n    var _player: AudioStreamPlayer = AudioStreamPlayer.new()\n    _player.stream.get_length()\n";
        let parsed = parse_script(source, "hover_property_receiver_method.gd");
        let hover = hover_at(3, 21, &parsed).expect("hover response");

        assert!(
            hover
                .title
                .contains("AudioStream method get_length() -> float"),
            "hover: {hover:#?}"
        );
        assert!(
            !hover.title.contains("ambiguous method"),
            "hover should not be ambiguous: {hover:#?}"
        );
    }

    #[test]
    fn property_hover_inside_call_argument_keeps_member_target() {
        let source = "func _ready() -> void:\n    var _impact_sfx: AudioStreamPlayer = AudioStreamPlayer.new()\n    var max_offset := maxf(_impact_sfx.stream.get_length() - 0.001, 0.0)\n";
        let parsed = parse_script(source, "hover_property_call_argument.gd");
        let hover = hover_at(3, 40, &parsed).expect("hover response");

        assert!(
            hover
                .title
                .contains("AudioStreamPlayer property stream: AudioStream"),
            "hover: {hover:#?}"
        );
        assert!(
            !hover.title.contains("max_offset"),
            "hover should not backtrack to assignment target: {hover:#?}"
        );
    }

    #[test]
    fn unresolved_receiver_method_hover_reports_ambiguity() {
        let source = "func _ready():\n    clear()\n";
        let parsed = parse_script(source, "hover_ambiguous_method.gd");
        let hover = hover_at(2, 7, &parsed).expect("hover response");

        assert!(
            hover.title.contains("ambiguous method `clear`"),
            "hover: {hover:#?}"
        );
        assert!(
            hover.body.contains("Multiple Godot methods match"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn hover_with_inline_annotations_includes_binding_type() {
        let source = "@export_range(0.0, 10.0, 0.1) var speed: float = 3.5\nfunc _ready():\n    print(speed)\n";
        let parsed = parse_script(source, "hover_annotated_value.gd");
        let hover = hover_at(3, 11, &parsed).expect("hover response");

        assert!(
            hover
                .body
                .contains("```gdscript\nvar speed: float = 3.5\n```"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn literal_hover_includes_literal_type() {
        let source = "func _ready():\n    print(42)\n";
        let parsed = parse_script(source, "hover_literal.gd");
        let hover = hover_at(2, 12, &parsed).expect("hover response");

        assert!(hover.body.contains("Type: `int`"), "hover: {hover:#?}");
        assert!(hover.body.contains("Value: `42`"), "hover: {hover:#?}");
    }

    #[test]
    fn local_shadowing_prefers_function_scope_symbol() {
        let source = "var value: int = 1\nfunc test(value: String):\n    print(value)\n";
        let parsed = parse_script(source, "hover_shadowing.gd");
        let hover = hover_at(3, 11, &parsed).expect("hover response");

        assert!(hover.title.contains("parameter"), "hover: {hover:#?}");
        assert!(hover.body.contains("Type: `String`"), "hover: {hover:#?}");
    }

    #[test]
    fn untyped_variable_hover_defaults_type_to_variant() {
        let source = "var target\nfunc _ready():\n    print(target)\n";
        let parsed = parse_script(source, "hover_untyped_var.gd");
        let hover = hover_at(3, 11, &parsed).expect("hover response");

        assert!(
            hover.body.contains("```gdscript\nvar target: Variant\n```"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn untyped_parameter_hover_defaults_type_to_variant() {
        let source = "func consume(value):\n    print(value)\n";
        let parsed = parse_script(source, "hover_untyped_param.gd");
        let hover = hover_at(2, 11, &parsed).expect("hover response");

        assert!(hover.title.contains("parameter"), "hover: {hover:#?}");
        assert!(hover.body.contains("Type: `Variant`"), "hover: {hover:#?}");
    }

    #[test]
    fn untyped_function_hover_reports_variant_return() {
        let source = "func compute(x):\n    return x\nfunc _ready():\n    compute(1)\n";
        let parsed = parse_script(source, "hover_untyped_function.gd");
        let hover = hover_at(4, 7, &parsed).expect("hover response");

        assert!(
            hover.body.contains("Signature: `func compute(x):`"),
            "hover: {hover:#?}"
        );
        assert!(
            hover.body.contains("Returns: `Variant`"),
            "hover: {hover:#?}"
        );
    }

    #[test]
    fn workspace_hover_resolves_external_declaration() {
        let current = parse_script("func test():\n    helper()\n", "a.gd");
        let other = parse_script("func helper() -> int:\n    return 1\n", "b.gd");
        let workspace = [HoverWorkspaceDoc {
            uri: "file:///b.gd",
            script: &other,
        }];

        let hover = hover_at_with_workspace(2, 7, &current, Some("file:///a.gd"), &workspace)
            .expect("hover response");

        assert!(
            hover.title.contains("function 'helper'"),
            "hover: {hover:#?}"
        );
        assert!(hover.body.contains("Returns: `int`"), "hover: {hover:#?}");
    }
}
