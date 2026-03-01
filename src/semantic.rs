use crate::parser::{ParsedScript, parse_script};
use std::collections::HashMap;

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

#[derive(Debug, Clone)]
pub struct SemanticDocument {
    pub uri: String,
    pub source: String,
    pub parsed: ParsedScript,
    declaration_index: HashMap<String, Vec<SymbolSpan>>,
    occurrence_index: HashMap<String, Vec<SymbolSpan>>,
}

impl SemanticDocument {
    pub fn declarations_for_symbol(&self, symbol: &str) -> Option<&Vec<SymbolSpan>> {
        self.declaration_index.get(symbol)
    }

    pub fn occurrences_for_symbol(&self, symbol: &str) -> Option<&Vec<SymbolSpan>> {
        self.occurrence_index.get(symbol)
    }
}

#[derive(Debug, Default)]
pub struct WorkspaceSemanticIndex {
    documents: HashMap<String, SemanticDocument>,
}

impl WorkspaceSemanticIndex {
    pub fn upsert_document(&mut self, uri: &str, source: &str) {
        let parsed = parse_script(source, uri);
        let declaration_index = build_declaration_index(&parsed);
        let occurrence_index = build_occurrence_index(source);

        self.documents.insert(
            uri.to_string(),
            SemanticDocument {
                uri: uri.to_string(),
                source: source.to_string(),
                parsed,
                declaration_index,
                occurrence_index,
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

fn is_identifier_char(byte: u8) -> bool {
    (byte as char).is_ascii_alphanumeric() || byte == b'_'
}
