use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::OnceLock;

use crate::code_actions::{CodeAction, CodeActionKind, code_actions_for_diagnostics_and_mode};
use crate::engine::{BehaviorMode, EngineConfig};
use crate::formatter::format_gdscript;
use crate::hover::hover_at;
use crate::lint::{
    Diagnostic, DiagnosticLevel, LintSettings, check_document_with_settings_and_mode,
};
use crate::parser::{ParsedScript, ScriptDeclKind, parse_script};
use crate::project_godot::load_project_godot_config;

#[derive(Debug, Serialize, Deserialize)]
struct LspRequest {
    pub id: Option<u64>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
struct IndexedDecl {
    line: usize,
    start_character: usize,
    end_character: usize,
}

#[derive(Debug, Clone)]
struct LspDocument {
    source: String,
    parsed: ParsedScript,
    symbol_index: HashMap<String, Vec<IndexedDecl>>,
}

#[derive(Debug, Default)]
struct LspState {
    documents: HashMap<String, LspDocument>,
    shutdown_received: bool,
}

impl LspState {
    fn open_document(&mut self, uri: &str, source: &str) {
        let parsed = parse_script(source, uri);
        self.documents.insert(
            uri.to_string(),
            LspDocument {
                source: source.to_string(),
                parsed: parsed.clone(),
                symbol_index: build_symbol_index(&parsed),
            },
        );
    }

    fn change_document(&mut self, uri: &str, source: &str) {
        self.open_document(uri, source);
    }

    fn close_document(&mut self, uri: &str) {
        self.documents.remove(uri);
    }

    fn source_for_uri(&self, uri: &str) -> Option<&str> {
        self.documents.get(uri).map(|doc| doc.source.as_str())
    }

    fn parsed_for_uri(&self, uri: &str) -> Option<&ParsedScript> {
        self.documents.get(uri).map(|doc| &doc.parsed)
    }

    fn declarations_by_symbol_for_uri(&self, uri: &str, symbol: &str) -> Option<&Vec<IndexedDecl>> {
        self.documents.get(uri)?.symbol_index.get(symbol)
    }
}

#[derive(Debug, Clone, Copy)]
enum Transport {
    Framed,
    LineDelimited,
}

pub fn run_stdio() -> Result<()> {
    let stdin = io::stdin();
    let reader = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    run_stdio_with_engine(EngineConfig::default(), reader, &mut writer)
}

pub fn run_stdio_with_config(engine: EngineConfig) -> Result<()> {
    let stdin = io::stdin();
    let reader = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    run_stdio_with_engine(engine, reader, &mut writer)
}

pub fn run_stdio_with<R: BufRead, W: Write>(reader: R, writer: &mut W) -> Result<()> {
    run_stdio_with_engine(EngineConfig::default(), reader, writer)
}

pub fn run_stdio_with_engine<R: BufRead, W: Write>(
    engine: EngineConfig,
    mut reader: R,
    writer: &mut W,
) -> Result<()> {
    let mut state = LspState::default();
    let mut scratch = String::new();
    let mut transport: Option<Transport> = None;

    while let Some((message, hint)) = read_message(&mut reader, &mut scratch)? {
        if transport.is_none() {
            transport = Some(hint);
        }

        let request: LspRequest = serde_json::from_str(&message)?;
        if request.method == "exit" {
            break;
        }

        let (response, notification) = handle_request(
            request,
            &engine,
            &mut state,
            transport.unwrap_or(Transport::LineDelimited),
        );

        if let Some(payload) = notification {
            let encoded = serde_json::to_string(&payload)?;
            write_message(
                writer,
                &encoded,
                transport.unwrap_or(Transport::LineDelimited),
            )?;
        }

        if let Some(payload) = response {
            let encoded = serde_json::to_string(&payload)?;
            write_message(
                writer,
                &encoded,
                transport.unwrap_or(Transport::LineDelimited),
            )?;
        }
    }

    Ok(())
}

fn read_message<R: BufRead>(
    reader: &mut R,
    scratch: &mut String,
) -> Result<Option<(String, Transport)>> {
    loop {
        scratch.clear();
        let read = reader.read_line(scratch)?;
        if read == 0 {
            return Ok(None);
        }

        let trimmed = scratch.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        if let Some(raw_length) = trimmed.strip_prefix("Content-Length:") {
            let length = raw_length.trim().parse::<usize>()?;

            loop {
                scratch.clear();
                let header_read = reader.read_line(scratch)?;
                if header_read == 0 {
                    break;
                }
                if scratch == "\r\n" || scratch == "\n" {
                    break;
                }
            }

            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body)?;
            let message = String::from_utf8(body)?;
            return Ok(Some((message, Transport::Framed)));
        }

        return Ok(Some((trimmed.to_string(), Transport::LineDelimited)));
    }
}

fn write_message<W: Write>(writer: &mut W, message: &str, transport: Transport) -> Result<()> {
    match transport {
        Transport::Framed => {
            write!(
                writer,
                "Content-Length: {}\r\n\r\n{}",
                message.len(),
                message
            )?;
            writer.flush()?;
        }
        Transport::LineDelimited => {
            writeln!(writer, "{message}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn handle_request(
    req: LspRequest,
    engine: &EngineConfig,
    state: &mut LspState,
    transport: Transport,
) -> (Option<Value>, Option<Value>) {
    let id = req.id;

    let response = match req.method.as_str() {
        "initialize" => id.map(|id| {
            json!({
                "id": id,
                "result": {
                    "capabilities": {
                        "textDocumentSync": 1,
                        "hoverProvider": true,
                        "documentFormattingProvider": true,
                        "diagnosticProvider": {
                            "interFileDependencies": false,
                            "workspaceDiagnostics": false
                        },
                        "codeActionProvider": {
                            "resolveProvider": false,
                            "codeActionKinds": ["quickfix", "refactor"]
                        },
                        "completionProvider": {
                            "resolveProvider": false,
                            "triggerCharacters": ["."]
                        },
                        "signatureHelpProvider": {
                            "triggerCharacters": ["("]
                        },
                        "documentSymbolProvider": true,
                        "documentHighlightProvider": true,
                        "definitionProvider": true,
                        "referencesProvider": true,
                        "workspaceSymbolProvider": true,
                    },
                    "serverInfo": {
                        "name": "gdscript-lsp",
                        "version": "0.1.0"
                    }
                }
            })
        }),
        "initialized"
        | "$/setTrace"
        | "$/cancelRequest"
        | "workspace/didChangeConfiguration"
        | "workspace/didChangeWatchedFiles"
        | "textDocument/didSave" => None,
        "shutdown" => {
            state.shutdown_received = true;
            id.map(|id| json!({"id": id, "result": true}))
        }
        "didOpen" | "textDocument/didOpen" => {
            let params = req.params.unwrap_or_default();
            if let Some(text_document) = params.get("textDocument") {
                if let Some(uri) = text_document.get("uri").and_then(Value::as_str) {
                    let source = text_document
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    state.open_document(uri, source);
                    if matches!(transport, Transport::Framed) {
                        let diagnostics =
                            diagnostics_for_source(source, engine.behavior_mode, transport);
                        return (None, Some(make_publish_diagnostics(uri, diagnostics)));
                    }
                }
            }
            None
        }
        "didChange" | "textDocument/didChange" => {
            let params = req.params.unwrap_or_default();
            if let Some(uri) = extract_uri(&params) {
                if let Some(source) = extract_changed_text(&params) {
                    state.change_document(uri, &source);
                    if matches!(transport, Transport::Framed) {
                        let diagnostics =
                            diagnostics_for_source(&source, engine.behavior_mode, transport);
                        return (None, Some(make_publish_diagnostics(uri, diagnostics)));
                    }
                }
            }
            None
        }
        "didClose" | "textDocument/didClose" => {
            let params = req.params.unwrap_or_default();
            if let Some(uri) = extract_uri(&params) {
                state.close_document(uri);
            }
            None
        }
        "textDocument/hover" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let (line, character) = extract_position(&params, transport);

            let parsed = if let Some(uri) = extract_uri(&params) {
                state
                    .parsed_for_uri(uri)
                    .cloned()
                    .unwrap_or_else(|| parse_script(&source, uri))
            } else {
                parse_script(&source, "stdin://lsp.gd")
            };

            let hover = hover_at(line, character, &parsed);
            let result = hover.map_or(Value::Null, |hover| {
                json!({
                    "contents": {
                        "kind": "markdown",
                        "value": format!("**{}**\n\n{}", hover.title, hover.body)
                    }
                })
            });
            id.map(|id| json!({"id": id, "result": result}))
        }
        "textDocument/formatting" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let formatted = format_gdscript(&source);
            id.map(|id| {
                json!({
                    "id": id,
                    "result": [{
                        "range": full_document_range(&source, transport),
                        "newText": formatted
                    }]
                })
            })
        }
        "textDocument/documentDiagnostic" | "textDocument/diagnostic" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let mode = extract_mode(&params).unwrap_or(engine.behavior_mode);
            let diagnostics = diagnostics_for_source(&source, mode, transport);
            id.map(|id| {
                json!({
                    "id": id,
                    "result": {
                        "kind": "full",
                        "items": diagnostics,
                        "diagnostics": diagnostics
                    }
                })
            })
        }
        "textDocument/codeAction" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let mode = extract_mode(&params).unwrap_or(engine.behavior_mode);
            let diagnostics =
                check_document_with_settings_and_mode(&source, &resolve_lint_settings(), mode);
            let actions = code_actions_for_diagnostics_and_mode(&source, &diagnostics, mode);
            let filtered = filter_actions_by_context(actions, &params);
            id.map(|id| json!({"id": id, "result": filtered}))
        }
        "textDocument/completion" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let uri = extract_uri(&params);
            let prefix = completion_prefix_from_params(&params, &source, transport);
            let local_decls = uri
                .and_then(|uri| state.parsed_for_uri(uri))
                .map(|parsed| parsed.declarations.as_slice());

            let mut items = Vec::new();
            let mut seen = std::collections::HashSet::new();
            items.extend(completion_entries(
                "keyword",
                COMPLETION_KEYWORDS,
                14,
                prefix.as_deref(),
                &mut seen,
            ));
            items.extend(completion_entries(
                "builtin",
                completion_builtin_candidates().as_slice(),
                3,
                prefix.as_deref(),
                &mut seen,
            ));
            if let Some(declarations) = local_decls {
                items.extend(completion_entries_for_declarations(
                    declarations,
                    prefix.as_deref(),
                    &mut seen,
                ));
            }

            id.map(|id| json!({"id": id, "result": {"isIncomplete": false, "items": items}}))
        }
        "textDocument/signatureHelp" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let uri = extract_uri(&params);
            let (line, character) = extract_position(&params, transport);
            let symbol = symbol_at_position(&source, line, character);

            let parsed = uri
                .and_then(|uri| state.parsed_for_uri(uri).cloned())
                .unwrap_or_else(|| parse_script(&source, uri.unwrap_or("stdin://lsp.gd")));
            let signatures = symbol
                .as_deref()
                .map(|symbol| {
                    parsed
                        .declarations
                        .iter()
                        .filter(|decl| {
                            matches!(decl.kind, ScriptDeclKind::Function) && decl.name == symbol
                        })
                        .map(|decl| {
                            json!({
                                "label": decl.text.trim(),
                                "parameters": function_signature_parameters(&decl.text),
                                "documentation": {
                                    "kind": "markdown",
                                    "value": format!("Function `{}`", decl.name)
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let result = if signatures.is_empty() {
                json!({"signatures": []})
            } else {
                json!({"signatures": signatures, "activeSignature": 0, "activeParameter": 0})
            };

            id.map(|id| json!({"id": id, "result": result}))
        }
        "textDocument/documentHighlight" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let uri = extract_uri(&params);
            let (line, character) = extract_position(&params, transport);
            let symbol = symbol_at_position(&source, line, character);
            let symbol_decls = symbol
                .as_deref()
                .map(|symbol| declarations_for_symbol(state, uri, symbol, &source))
                .unwrap_or_default();
            let highlights = symbol_decls
                .into_iter()
                .map(|decl| {
                    json!({
                        "range": range_for_decl(&decl, transport),
                        "kind": 1
                    })
                })
                .collect::<Vec<_>>();

            id.map(|id| json!({"id": id, "result": highlights}))
        }
        "workspace/symbol" => {
            let params = req.params.unwrap_or_default();
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let mut uris: Vec<_> = state.documents.keys().cloned().collect();
            uris.sort_unstable();
            let mut symbols = Vec::new();

            for uri in uris {
                let doc = state.documents.get(&uri).expect("document present");
                for decl in &doc.parsed.declarations {
                    let decl_name = decl.name.to_ascii_lowercase();
                    if !query.is_empty() && !decl_name.contains(&query) {
                        continue;
                    }

                    let line_text = doc.parsed.lines.get(decl.line.saturating_sub(1));
                    let (start_character, end_character) = declaration_name_range(
                        line_text.map(String::as_str).unwrap_or(""),
                        &decl.name,
                    );

                    symbols.push(json!({
                        "name": decl.name,
                        "kind": declaration_kind_to_symbol_kind(&decl.kind),
                        "location": {
                            "uri": uri,
                            "range": lsp_range(
                                decl.line,
                                start_character,
                                decl.line,
                                end_character,
                                transport
                            )
                        }
                    }));
                }
            }

            id.map(|id| json!({"id": id, "result": symbols}))
        }
        "textDocument/definition" => {
            let params = req.params.unwrap_or_default();
            let uri = extract_uri(&params);
            let source = source_from_params(&params, state);
            let (line, character) = extract_position(&params, transport);
            let symbol = symbol_at_position(&source, line, character);
            let locations: Vec<_> = uri
                .and_then(|uri| state.declarations_by_symbol_for_uri(uri, symbol?.as_str()))
                .map(|symbols| {
                    symbols
                        .iter()
                        .map(|symbol| location_for(uri.unwrap_or(""), symbol, transport))
                        .collect()
                })
                .unwrap_or_default();

            let result = if locations.is_empty() {
                serde_json::Value::Null
            } else {
                json!(locations)
            };

            id.map(|id| json!({"id": id, "result": result}))
        }
        "textDocument/references" => {
            let params = req.params.unwrap_or_default();
            let uri = extract_uri(&params);
            let source = source_from_params(&params, state);
            let (line, character) = extract_position(&params, transport);
            let symbol = symbol_at_position(&source, line, character);
            let locations = uri
                .and_then(|uri| state.declarations_by_symbol_for_uri(uri, symbol?.as_str()))
                .map(|symbols| {
                    symbols
                        .iter()
                        .map(|symbol| location_for(uri.unwrap_or(""), symbol, transport))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            id.map(|id| json!({"id": id, "result": locations}))
        }
        "textDocument/documentSymbol" => {
            let params = req.params.unwrap_or_default();
            let uri = extract_uri(&params).unwrap_or("stdin://lsp.gd");
            let source = source_from_params(&params, state);
            let parsed = parse_script(&source, uri);
            let symbols = parsed
                .declarations
                .iter()
                .map(|decl| {
                    let line_text = parsed
                        .lines
                        .get(decl.line.saturating_sub(1))
                        .map(String::as_str)
                        .unwrap_or("");
                    let end_col = line_text.len().saturating_add(1);
                    let start_col = 1;
                    let kind = match decl.kind {
                        crate::parser::ScriptDeclKind::Function => 12,
                        crate::parser::ScriptDeclKind::Class => 5,
                        crate::parser::ScriptDeclKind::Variable => 13,
                        crate::parser::ScriptDeclKind::Constant => 14,
                    };

                    json!({
                        "name": decl.name,
                        "kind": kind,
                        "location": {
                            "uri": uri,
                            "range": lsp_range(
                                decl.line,
                                start_col,
                                decl.line,
                                end_col,
                                transport
                            )
                        }
                    })
                })
                .collect::<Vec<_>>();

            id.map(|id| json!({"id": id, "result": symbols}))
        }
        _ => id.map(|id| {
            json!({
                "id": id,
                "error": {
                    "code": -32601,
                    "message": "unknown method"
                }
            })
        }),
    };

    (response, None)
}

fn make_publish_diagnostics(uri: &str, diagnostics: Vec<Value>) -> Value {
    json!({
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    })
}

fn source_from_params(params: &Value, state: &LspState) -> String {
    if let Some(text) = params.get("text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(text_document) = params.get("textDocument") {
        if let Some(text) = text_document.get("text").and_then(Value::as_str) {
            return text.to_string();
        }
    }

    if let Some(uri) = extract_uri(params) {
        if let Some(source) = state.source_for_uri(uri) {
            return source.to_string();
        }
    }

    String::new()
}

fn extract_uri(params: &Value) -> Option<&str> {
    params
        .get("textDocument")
        .and_then(|td| td.get("uri"))
        .and_then(Value::as_str)
        .or_else(|| params.get("uri").and_then(Value::as_str))
}

fn extract_changed_text(params: &Value) -> Option<String> {
    if let Some(changes) = params.get("contentChanges").and_then(Value::as_array) {
        if let Some(last) = changes.last() {
            if let Some(text) = last.get("text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
        }
    }

    params
        .get("text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn extract_position(params: &Value, transport: Transport) -> (usize, usize) {
    if let Some(position) = params.get("position") {
        let line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let character = position
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        return match transport {
            Transport::Framed => (line.saturating_add(1), character.saturating_add(1)),
            Transport::LineDelimited => (line.max(1), character.max(1)),
        };
    }

    let line = params.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
    let character = params.get("character").and_then(Value::as_u64).unwrap_or(0) as usize;
    match transport {
        Transport::Framed => (line.saturating_add(1), character.saturating_add(1)),
        Transport::LineDelimited => (line.max(1), character.max(1)),
    }
}

fn extract_mode(params: &Value) -> Option<BehaviorMode> {
    params
        .get("context")
        .and_then(|ctx| ctx.get("mode"))
        .and_then(Value::as_str)
        .and_then(BehaviorMode::from_raw)
        .or_else(|| {
            params
                .get("mode")
                .and_then(Value::as_str)
                .and_then(BehaviorMode::from_raw)
        })
}

fn diagnostics_for_source(source: &str, mode: BehaviorMode, transport: Transport) -> Vec<Value> {
    let parsed = parse_script(source, "stdin://lsp.gd");
    let mut diagnostics = parsed
        .issues
        .into_iter()
        .map(|issue| Diagnostic {
            file: None,
            line: issue.line,
            column: 1,
            code: "parser-error".to_string(),
            level: DiagnosticLevel::Error,
            message: issue.message,
        })
        .collect::<Vec<_>>();

    diagnostics.extend(check_document_with_settings_and_mode(
        source,
        &resolve_lint_settings(),
        mode,
    ));

    diagnostics
        .into_iter()
        .map(|diag| diagnostic_payload(diag, transport))
        .collect()
}

fn diagnostic_payload(diag: Diagnostic, transport: Transport) -> Value {
    let line = diag.line.max(1);
    let column = diag.column.max(1);
    let end_column = column.saturating_add(1);
    let mut payload = json!({
        "range": lsp_range(line, column, line, end_column, transport),
        "line": diag.line,
        "column": diag.column,
        "code": diag.code,
        "message": diag.message,
        "level": diag.level,
        "source": "gdscript-lsp",
    });

    if let Some(severity) = lsp_severity(diag.level) {
        if let Some(level) = severity.as_u64() {
            payload["severity"] = level.into();
        }
    }

    payload
}

fn resolve_lint_settings() -> LintSettings {
    let config = load_project_godot_config("project.godot").ok();
    LintSettings::from_project_config(config.as_ref())
}

fn lsp_severity(level: DiagnosticLevel) -> Option<Value> {
    match level {
        DiagnosticLevel::Error => Some(Value::from(1)),
        DiagnosticLevel::Warning => Some(Value::from(2)),
        DiagnosticLevel::Info => Some(Value::from(3)),
        DiagnosticLevel::Off => None,
    }
}

fn completion_prefix_from_params(
    params: &Value,
    source: &str,
    transport: Transport,
) -> Option<String> {
    let (line, character) = extract_position(params, transport);
    let line_idx = line.saturating_sub(1);
    let character_idx = character.max(1);
    let line_text = source.lines().nth(line_idx)?;
    let cursor = line_byte_offset(line_text, character_idx);

    if cursor == 0 {
        return None;
    }

    let bytes = line_text.as_bytes();
    let mut start = cursor;
    while start > 0 {
        if is_identifier_char(bytes[start - 1]) {
            start -= 1;
        } else {
            break;
        }
    }

    let prefix = &line_text[start..cursor];
    if prefix.is_empty() {
        None
    } else {
        Some(prefix.to_string())
    }
}

fn line_byte_offset(line: &str, character: usize) -> usize {
    line.char_indices()
        .map(|(idx, _)| idx)
        .nth(character.saturating_sub(1))
        .unwrap_or(line.len())
}

fn is_identifier_char(byte: u8) -> bool {
    (byte as char).is_ascii_alphanumeric() || byte == b'_'
}

fn completion_entries(
    _kind_name: &str,
    entries: &'static [&'static str],
    lsp_kind: u32,
    prefix: Option<&str>,
    seen: &mut HashSet<String>,
) -> Vec<Value> {
    entries
        .iter()
        .filter(|entry| {
            if let Some(prefix) = prefix {
                entry.starts_with(prefix)
            } else {
                true
            }
        })
        .filter(|entry| {
            if seen.iter().any(|value| value == *entry) {
                false
            } else {
                seen.insert((*entry).to_string());
                true
            }
        })
        .map(|entry| {
            json!({
                "label": entry,
                "kind": lsp_kind,
                "insertText": entry
            })
        })
        .collect()
}

fn completion_entries_for_declarations(
    declarations: &[crate::parser::ScriptDecl],
    prefix: Option<&str>,
    seen: &mut HashSet<String>,
) -> Vec<Value> {
    declarations
        .iter()
        .filter_map(|decl| {
            if let Some(prefix) = prefix {
                if !decl.name.starts_with(prefix) {
                    return None;
                }
            }

            if seen.contains(&decl.name) {
                return None;
            }
            seen.insert(decl.name.clone());

            let kind = match decl.kind {
                crate::parser::ScriptDeclKind::Function => 12,
                crate::parser::ScriptDeclKind::Class => 5,
                crate::parser::ScriptDeclKind::Variable => 13,
                crate::parser::ScriptDeclKind::Constant => 14,
            };

            Some(json!({
                "label": decl.name,
                "kind": kind,
                "insertText": decl.name
            }))
        })
        .collect()
}

fn declarations_for_symbol(
    state: &LspState,
    uri: Option<&str>,
    symbol: &str,
    source: &str,
) -> Vec<IndexedDecl> {
    uri.and_then(|uri| state.declarations_by_symbol_for_uri(uri, symbol).cloned())
        .unwrap_or_else(|| {
            let parsed = parse_script(source, uri.unwrap_or("stdin://lsp.gd"));
            build_symbol_index(&parsed)
                .get(symbol)
                .cloned()
                .unwrap_or_default()
        })
}

fn function_signature_parameters(signature_line: &str) -> Vec<Value> {
    let line = signature_line.trim();
    if let Some(start) = line.find('(') {
        let end = line.find(')').unwrap_or(line.len());
        if start + 1 <= end {
            let raw = line[start + 1..end].trim();
            return raw
                .split(',')
                .map(str::trim)
                .filter(|param| !param.is_empty())
                .map(|param| {
                    let label = param.split('=').next().unwrap_or(param).trim();
                    let label = if label.is_empty() { param } else { label };
                    json!({"label": label})
                })
                .collect();
        }
    }
    Vec::new()
}

fn range_for_decl(decl: &IndexedDecl, transport: Transport) -> Value {
    lsp_range(
        decl.line,
        decl.start_character,
        decl.line,
        decl.end_character,
        transport,
    )
}

fn lsp_range(
    start_line: usize,
    start_character: usize,
    end_line: usize,
    end_character: usize,
    transport: Transport,
) -> Value {
    json!({
        "start": {
            "line": lsp_line(start_line, transport),
            "character": lsp_character(start_character, transport),
        },
        "end": {
            "line": lsp_line(end_line, transport),
            "character": lsp_character(end_character, transport),
        },
    })
}

fn lsp_line(line: usize, transport: Transport) -> usize {
    match transport {
        Transport::Framed => line.saturating_sub(1),
        Transport::LineDelimited => line,
    }
}

fn lsp_character(character: usize, transport: Transport) -> usize {
    match transport {
        Transport::Framed => character.saturating_sub(1),
        Transport::LineDelimited => character,
    }
}

fn declaration_kind_to_symbol_kind(kind: &ScriptDeclKind) -> u32 {
    match kind {
        ScriptDeclKind::Function => 12,
        ScriptDeclKind::Class => 5,
        ScriptDeclKind::Variable => 13,
        ScriptDeclKind::Constant => 14,
    }
}

fn build_symbol_index(parsed: &ParsedScript) -> HashMap<String, Vec<IndexedDecl>> {
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
            let entry = IndexedDecl {
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

fn symbol_at_position(source: &str, line: usize, character: usize) -> Option<String> {
    let line_idx = line.saturating_sub(1);
    let line_text = source.lines().nth(line_idx)?;
    if line_text.is_empty() {
        return None;
    }

    let mut byte_index = character.saturating_sub(1);
    if byte_index >= line_text.len() {
        if line_text.is_empty() {
            return None;
        }
        byte_index = line_text.len().saturating_sub(1);
    }

    let bytes = line_text.as_bytes();
    while byte_index > 0 && !is_identifier_char(bytes[byte_index]) {
        byte_index -= 1;
    }

    if !is_identifier_char(bytes[byte_index]) {
        return None;
    }

    let mut start = byte_index;
    let mut end = byte_index;
    while start > 0 && is_identifier_char(bytes[start - 1]) {
        start -= 1;
    }
    while end + 1 < bytes.len() && is_identifier_char(bytes[end + 1]) {
        end += 1;
    }

    Some(line_text[start..=end].to_string())
}

fn location_for(uri: &str, decl: &IndexedDecl, transport: Transport) -> Value {
    json!({
        "uri": uri,
        "range": lsp_range(
            decl.line,
            decl.start_character,
            decl.line,
            decl.end_character,
            transport
        )
    })
}

fn full_document_range(source: &str, transport: Transport) -> Value {
    let (end_line, end_character) = if source.is_empty() {
        (1, 1)
    } else {
        let lines: Vec<&str> = source.split('\n').collect();
        let line_count = lines.len();
        let last_len = lines.last().map(|line| line.len()).unwrap_or(0);
        (line_count, last_len.saturating_add(1))
    };

    lsp_range(1, 1, end_line, end_character, transport)
}

const COMPLETION_KEYWORDS: &[&str] = &[
    "and",
    "as",
    "await",
    "break",
    "breakpoint",
    "class",
    "class_name",
    "const",
    "elif",
    "else",
    "extends",
    "for",
    "func",
    "if",
    "in",
    "is",
    "match",
    "pass",
    "return",
    "static",
    "super",
    "while",
];

const COMPLETION_BUILTINS: &[&str] = &[
    "abs",
    "Array",
    "Color",
    "Dictionary",
    "Node",
    "Object",
    "PackedStringArray",
    "String",
    "Vector2",
    "Vector3",
    "clamp",
    "clampf",
    "deg_to_rad",
    "draw_circle",
    "get_tree",
    "load",
    "len",
    "max",
    "min",
    "print",
    "print_debug",
    "preload",
    "randf",
    "randi",
    "randi_range",
    "sin",
    "str",
    "yield",
];

fn completion_builtin_candidates() -> &'static Vec<&'static str> {
    static BUILTIN_CANDIDATES: OnceLock<Vec<&'static str>> = OnceLock::new();
    BUILTIN_CANDIDATES.get_or_init(|| {
        let mut out = Vec::new();
        out.extend_from_slice(COMPLETION_BUILTINS);
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
        out.sort_unstable();
        out.dedup();
        out
    })
}

fn filter_actions_by_context(actions: Vec<CodeAction>, params: &Value) -> Vec<CodeAction> {
    let Some(only) = params
        .get("context")
        .and_then(|ctx| ctx.get("only"))
        .and_then(Value::as_array)
    else {
        return actions;
    };

    let quickfix = only
        .iter()
        .filter_map(Value::as_str)
        .any(should_include_quick_fix_action);
    let refactor = only.iter().filter_map(Value::as_str).any(is_refactor_kind);

    actions
        .into_iter()
        .filter(|action| match action.kind {
            CodeActionKind::QuickFix => quickfix,
            CodeActionKind::Refactor => refactor,
        })
        .collect()
}

fn should_include_quick_fix_action(kind: &str) -> bool {
    if kind.eq_ignore_ascii_case("quickfix") {
        return true;
    }

    let normalized = kind.to_ascii_lowercase();
    normalized == "source.fixall" || normalized.starts_with("source.fixall.")
}

fn is_refactor_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("refactor") || kind.to_ascii_lowercase().starts_with("refactor.")
}

pub fn run_with_paths_and_command(path: &Path, source: &str) -> Result<Option<String>> {
    let parsed = parse_script(source, path);
    if parsed.issues.is_empty() {
        Ok(Some(format_gdscript(source)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::run_stdio_with;

    #[test]
    fn handles_unknown_method() {
        let mut out = Vec::new();
        run_stdio_with(
            "{\"id\":1,\"method\":\"unknown\"}\n{\"method\":\"exit\"}\n".as_bytes(),
            &mut out,
        )
        .expect("lsp run");

        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("unknown method"));
    }
}
