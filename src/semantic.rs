use crate::parser::{ParsedScript, parse_script};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolSpan {
    pub line: usize,
    pub start_character: usize,
    pub end_character: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub uri: String,
    pub span: SymbolSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedBinding {
    pub line: usize,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct SemanticDocument {
    pub uri: String,
    pub source: String,
    pub parsed: ParsedScript,
    declaration_index: HashMap<String, Vec<SymbolSpan>>,
    occurrence_index: HashMap<String, Vec<SymbolSpan>>,
    typed_binding_index: HashMap<String, Vec<TypedBinding>>,
    class_names: HashSet<String>,
    extends_name: Option<String>,
    source_hash: u64,
}

impl SemanticDocument {
    pub fn declarations_for_symbol(&self, symbol: &str) -> Option<&Vec<SymbolSpan>> {
        self.declaration_index.get(symbol)
    }

    pub fn occurrences_for_symbol(&self, symbol: &str) -> Option<&Vec<SymbolSpan>> {
        self.occurrence_index.get(symbol)
    }

    pub fn type_for_symbol_at_line(&self, symbol: &str, line: usize) -> Option<&str> {
        self.typed_binding_index
            .get(symbol)
            .and_then(|bindings| {
                bindings
                    .iter()
                    .filter(|binding| binding.line <= line)
                    .max_by_key(|binding| binding.line)
            })
            .map(|binding| binding.ty.as_str())
    }

    pub fn class_names(&self) -> &HashSet<String> {
        &self.class_names
    }

    pub fn extends_name(&self) -> Option<&str> {
        self.extends_name.as_deref()
    }

    pub fn source_hash(&self) -> u64 {
        self.source_hash
    }
}

#[derive(Debug, Default)]
pub struct WorkspaceSemanticIndex {
    documents: HashMap<String, SemanticDocument>,
}

impl WorkspaceSemanticIndex {
    pub fn upsert_document(&mut self, uri: &str, source: &str) {
        let source_hash = hash_source(source);
        if self
            .documents
            .get(uri)
            .is_some_and(|existing| existing.source_hash() == source_hash)
        {
            return;
        }

        let parsed = parse_script(source, uri);
        let declaration_index = build_declaration_index(&parsed);
        let occurrence_index = build_occurrence_index(source);
        let typed_binding_index = build_typed_binding_index(source);
        let class_names = extract_class_names(source);
        let extends_name = extract_extends_name(source);

        self.documents.insert(
            uri.to_string(),
            SemanticDocument {
                uri: uri.to_string(),
                source: source.to_string(),
                parsed,
                declaration_index,
                occurrence_index,
                typed_binding_index,
                class_names,
                extends_name,
                source_hash,
            },
        );
    }

    pub fn remove_document(&mut self, uri: &str) {
        self.documents.remove(uri);
    }

    pub fn get_document(&self, uri: &str) -> Option<&SemanticDocument> {
        self.documents.get(uri)
    }

    pub fn documents(&self) -> impl Iterator<Item = &SemanticDocument> {
        self.documents.values()
    }

    pub fn declarations_for_symbol_in_uri(&self, uri: &str, symbol: &str) -> Vec<SymbolLocation> {
        self.documents
            .get(uri)
            .and_then(|doc| doc.declarations_for_symbol(symbol))
            .into_iter()
            .flatten()
            .map(|span| SymbolLocation {
                uri: uri.to_string(),
                span: *span,
            })
            .collect()
    }

    pub fn occurrences_for_symbol_in_uri(&self, uri: &str, symbol: &str) -> Vec<SymbolLocation> {
        self.documents
            .get(uri)
            .and_then(|doc| doc.occurrences_for_symbol(symbol))
            .into_iter()
            .flatten()
            .map(|span| SymbolLocation {
                uri: uri.to_string(),
                span: *span,
            })
            .collect()
    }

    pub fn workspace_declarations_for_symbol(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.documents
            .iter()
            .flat_map(|(uri, doc)| {
                doc.declarations_for_symbol(symbol)
                    .into_iter()
                    .flatten()
                    .map(move |span| SymbolLocation {
                        uri: uri.clone(),
                        span: *span,
                    })
            })
            .collect()
    }

    pub fn workspace_occurrences_for_symbol(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.documents
            .iter()
            .flat_map(|(uri, doc)| {
                doc.occurrences_for_symbol(symbol)
                    .into_iter()
                    .flatten()
                    .map(move |span| SymbolLocation {
                        uri: uri.clone(),
                        span: *span,
                    })
            })
            .collect()
    }

    pub fn has_workspace_declaration(&self, symbol: &str) -> bool {
        self.documents.values().any(|doc| {
            doc.declarations_for_symbol(symbol)
                .is_some_and(|hits| !hits.is_empty())
        })
    }

    pub fn workspace_class_names(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for doc in self.documents.values() {
            out.extend(doc.class_names().iter().cloned());
        }
        out
    }
}

fn build_declaration_index(parsed: &ParsedScript) -> HashMap<String, Vec<SymbolSpan>> {
    parsed
        .declarations
        .iter()
        .fold(HashMap::new(), |mut acc, declaration| {
            let line_text = parsed
                .lines
                .get(declaration.line.saturating_sub(1))
                .map(String::as_str)
                .unwrap_or("");
            let (start_character, end_character) =
                declaration_name_range(line_text, &declaration.name);
            let entry = SymbolSpan {
                line: declaration.line,
                start_character,
                end_character,
            };
            acc.entry(declaration.name.clone()).or_default().push(entry);
            acc
        })
}

fn declaration_name_range(line_text: &str, name: &str) -> (usize, usize) {
    if name.is_empty() {
        return (1, 1);
    }

    if let Some(byte_offset) = line_text.find(name) {
        let start_character = byte_offset.saturating_add(1);
        (start_character, start_character.saturating_add(name.len()))
    } else {
        (1, 1 + name.len())
    }
}

fn build_occurrence_index(source: &str) -> HashMap<String, Vec<SymbolSpan>> {
    let mut out: HashMap<String, Vec<SymbolSpan>> = HashMap::new();
    let mut quote = None::<u8>;
    let mut escaped = false;
    let mut triple = false;

    for (line_idx, line_text) in source.lines().enumerate() {
        let bytes = line_text.as_bytes();
        let mut idx = 0usize;

        while idx < bytes.len() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                    idx += 1;
                    continue;
                }
                if triple
                    && idx + 2 < bytes.len()
                    && bytes[idx] == q
                    && bytes[idx + 1] == q
                    && bytes[idx + 2] == q
                {
                    quote = None;
                    triple = false;
                    idx += 3;
                    continue;
                }
                if bytes[idx] == b'\\' && !triple {
                    escaped = true;
                } else if bytes[idx] == q && !triple {
                    quote = None;
                }
                idx += 1;
                continue;
            }

            if bytes[idx] == b'#' {
                break;
            }

            if bytes[idx] == b'\'' || bytes[idx] == b'"' {
                quote = Some(bytes[idx]);
                triple = idx + 2 < bytes.len()
                    && bytes[idx + 1] == bytes[idx]
                    && bytes[idx + 2] == bytes[idx];
                idx += if triple { 3 } else { 1 };
                continue;
            }

            if is_identifier_char(bytes[idx]) {
                let start = idx;
                idx += 1;
                while idx < bytes.len() && is_identifier_char(bytes[idx]) {
                    idx += 1;
                }
                let name = line_text[start..idx].to_string();
                out.entry(name).or_default().push(SymbolSpan {
                    line: line_idx.saturating_add(1),
                    start_character: start.saturating_add(1),
                    end_character: idx.saturating_add(1),
                });
                continue;
            }

            idx += 1;
        }

        if quote.is_some() && !triple {
            quote = None;
            escaped = false;
        }
    }

    out
}

fn build_typed_binding_index(source: &str) -> HashMap<String, Vec<TypedBinding>> {
    let mut out: HashMap<String, Vec<TypedBinding>> = HashMap::new();

    for (line_idx, raw_line) in source.lines().enumerate() {
        let line = line_idx.saturating_add(1);
        let code = parse_code_prefix(raw_line).trim();
        if code.is_empty() {
            continue;
        }

        if let Some((name, ty)) = parse_var_or_const_binding(code) {
            out.entry(name).or_default().push(TypedBinding { line, ty });
            continue;
        }

        if let Some((name, expr)) = parse_simple_assignment(code)
            && let Some(ty) = infer_type_from_expression(expr)
        {
            out.entry(name).or_default().push(TypedBinding { line, ty });
        }
    }

    for bindings in out.values_mut() {
        bindings.sort_by_key(|binding| binding.line);
        bindings.dedup_by(|a, b| a.line == b.line && a.ty == b.ty);
    }

    out
}

fn parse_var_or_const_binding(code: &str) -> Option<(String, String)> {
    let tail = code
        .strip_prefix("var ")
        .or_else(|| code.strip_prefix("const "))?
        .trim();

    if let Some((lhs, rhs)) = tail.split_once(":=") {
        let name = extract_identifier(lhs.trim())?;
        let ty = infer_type_from_expression(rhs.trim())?;
        return Some((name, ty));
    }

    if let Some((lhs, rhs)) = tail.split_once('=') {
        let lhs = lhs.trim();
        let explicit = lhs
            .split_once(':')
            .map(|(_, ty)| ty.trim())
            .and_then(non_empty)
            .map(ToString::to_string);
        let name = extract_identifier(lhs.split(':').next().unwrap_or(lhs).trim())?;
        let ty = explicit.or_else(|| infer_type_from_expression(rhs.trim()))?;
        return Some((name, ty));
    }

    let (name_part, ty_part) = tail.split_once(':')?;
    let name = extract_identifier(name_part.trim())?;
    let ty = non_empty(ty_part.trim())?.to_string();
    Some((name, ty))
}

fn parse_simple_assignment(code: &str) -> Option<(String, &str)> {
    if code.starts_with("func ") || code.starts_with("class ") {
        return None;
    }

    if let Some((lhs, rhs)) = code.split_once(":=") {
        let lhs = lhs.trim();
        if lhs.contains('.') || lhs.contains('[') {
            return None;
        }
        let name = extract_identifier(lhs)?;
        if name != lhs {
            return None;
        }
        return Some((name, rhs.trim()));
    }

    let (lhs, rhs) = code.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    if lhs.ends_with('!')
        || lhs.ends_with('<')
        || lhs.ends_with('>')
        || lhs.ends_with('=')
        || lhs.ends_with('+')
        || lhs.ends_with('-')
        || lhs.ends_with('*')
        || lhs.ends_with('/')
        || lhs.ends_with('%')
        || rhs.starts_with('=')
        || lhs.contains('.')
        || lhs.contains('[')
    {
        return None;
    }

    let name = extract_identifier(lhs)?;
    if name != lhs {
        return None;
    }
    Some((name, rhs))
}

fn infer_type_from_expression(expr: &str) -> Option<String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    if expr.parse::<i64>().is_ok() {
        return Some("int".to_string());
    }
    if expr.parse::<f64>().is_ok() {
        return Some("float".to_string());
    }
    if matches!(expr, "true" | "false") {
        return Some("bool".to_string());
    }
    if (expr.starts_with('"') && expr.ends_with('"'))
        || (expr.starts_with('\'') && expr.ends_with('\''))
    {
        return Some("String".to_string());
    }
    if expr.starts_with('[') && expr.ends_with(']') {
        return Some("Array".to_string());
    }
    if expr.starts_with('{') && expr.ends_with('}') {
        return Some("Dictionary".to_string());
    }

    if let Some(class_name) = expr.strip_suffix(".new()") {
        let class_name = class_name.trim();
        if is_type_name(class_name) {
            return Some(class_name.to_string());
        }
    }

    if let Some((constructor, _)) = expr.split_once('(')
        && is_type_name(constructor.trim())
    {
        return Some(constructor.trim().to_string());
    }

    None
}

fn extract_class_names(source: &str) -> HashSet<String> {
    let mut out = HashSet::new();

    for raw_line in source.lines() {
        let code = parse_code_prefix(raw_line).trim();
        if let Some(rest) = code.strip_prefix("class_name ")
            && let Some(name) = extract_identifier(rest.trim())
        {
            out.insert(name);
        }
    }

    out
}

fn extract_extends_name(source: &str) -> Option<String> {
    for raw_line in source.lines() {
        let code = parse_code_prefix(raw_line).trim();
        if let Some(rest) = code.strip_prefix("extends ")
            && let Some(name) = extract_identifier(rest.trim())
        {
            return Some(name);
        }
    }

    None
}

fn hash_source(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn parse_code_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut triple = false;

    while idx < bytes.len() {
        let ch = bytes[idx];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                idx += 1;
                continue;
            }
            if ch == b'\\' && !triple {
                escaped = true;
                idx += 1;
                continue;
            }
            if triple
                && idx + 2 < bytes.len()
                && bytes[idx] == q
                && bytes[idx + 1] == q
                && bytes[idx + 2] == q
            {
                quote = None;
                triple = false;
                idx += 3;
                continue;
            }
            if ch == q && !triple {
                quote = None;
            }
            idx += 1;
            continue;
        }

        if ch == b'#' {
            return line[..idx].trim_end();
        }

        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            triple = idx + 2 < bytes.len() && bytes[idx + 1] == ch && bytes[idx + 2] == ch;
            idx += if triple { 3 } else { 1 };
            continue;
        }

        idx += 1;
    }

    line.trim_end()
}

fn extract_identifier(input: &str) -> Option<String> {
    let token = input
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();

    let mut chars = token.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    Some(token)
}

fn is_identifier_char(byte: u8) -> bool {
    (byte as char).is_ascii_alphanumeric() || byte == b'_'
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

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceSemanticIndex, infer_type_from_expression};

    #[test]
    fn infers_basic_expression_types() {
        assert_eq!(infer_type_from_expression("1"), Some("int".to_string()));
        assert_eq!(
            infer_type_from_expression("RandomNumberGenerator.new()"),
            Some("RandomNumberGenerator".to_string())
        );
        assert_eq!(
            infer_type_from_expression("Vector2(1.0, 2.0)"),
            Some("Vector2".to_string())
        );
    }

    #[test]
    fn workspace_index_tracks_types_and_classes() {
        let mut index = WorkspaceSemanticIndex::default();
        let uri = "file:///tmp/semantic-types.gd";
        let source = "extends Node\nclass_name DemoType\nvar rng := RandomNumberGenerator.new()\nfunc _ready():\n    rng = RandomNumberGenerator.new()\n";
        index.upsert_document(uri, source);

        let doc = index.get_document(uri).expect("document");
        assert_eq!(
            doc.type_for_symbol_at_line("rng", 3),
            Some("RandomNumberGenerator")
        );
        assert_eq!(doc.extends_name(), Some("Node"));
        assert!(doc.class_names().contains("DemoType"));
    }

    #[test]
    fn upsert_skips_unchanged_sources() {
        let mut index = WorkspaceSemanticIndex::default();
        let uri = "file:///tmp/semantic-cache.gd";
        let source = "var count := 1\n";
        index.upsert_document(uri, source);
        let first_hash = index.get_document(uri).expect("document").source_hash();

        index.upsert_document(uri, source);
        let second_hash = index.get_document(uri).expect("document").source_hash();

        assert_eq!(first_hash, second_hash);
    }
}
