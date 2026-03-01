use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::code_actions::{CodeAction, CodeActionKind, code_actions_for_diagnostics_and_mode};
use crate::engine::{BehaviorMode, EngineConfig};
use crate::formatter::format_gdscript;
use crate::hover::{HoverWorkspaceDoc, definition_uri_for_known_symbol, hover_at_with_workspace};
use crate::lint::{
    Diagnostic, DiagnosticLevel, LintSettings, check_document_with_settings_and_mode,
};
use crate::parser::{ParsedScript, ScriptDeclKind, parse_script};
use crate::project_godot::load_project_godot_config;
use crate::semantic::{SemanticDocument, SymbolLocation, SymbolSpan, WorkspaceSemanticIndex};

#[derive(Debug, Serialize, Deserialize)]
struct LspRequest {
    pub id: Option<u64>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Default)]
struct LspState {
    workspace_index: WorkspaceSemanticIndex,
    workspace_roots: Vec<PathBuf>,
    shutdown_received: bool,
}

impl LspState {
    fn open_document(&mut self, uri: &str, source: &str) {
        self.workspace_index.upsert_document(uri, source);
    }

    fn change_document(&mut self, uri: &str, source: &str) {
        self.open_document(uri, source);
    }

    fn close_document(&mut self, uri: &str) {
        if !self.reindex_document_from_disk(uri) {
            self.workspace_index.remove_document(uri);
        }
    }

    fn source_for_uri(&self, uri: &str) -> Option<&str> {
        self.workspace_index
            .get_document(uri)
            .map(|doc| doc.source.as_str())
    }

    fn parsed_for_uri(&self, uri: &str) -> Option<&ParsedScript> {
        self.workspace_index
            .get_document(uri)
            .map(|doc| &doc.parsed)
    }

    fn declarations_by_symbol_for_uri(&self, uri: &str, symbol: &str) -> Vec<SymbolLocation> {
        self.workspace_index
            .declarations_for_symbol_in_uri(uri, symbol)
    }

    fn workspace_declarations_for_symbol(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.workspace_index
            .workspace_declarations_for_symbol(symbol)
    }

    fn workspace_occurrences_for_symbol(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.workspace_index
            .workspace_occurrences_for_symbol(symbol)
    }

    fn has_workspace_declaration(&self, symbol: &str) -> bool {
        self.workspace_index.has_workspace_declaration(symbol)
    }

    fn workspace_documents(&self) -> impl Iterator<Item = &SemanticDocument> {
        self.workspace_index.documents()
    }

    fn configure_workspace_roots(&mut self, params: &Value) {
        let mut roots = Vec::new();

        if let Some(root_uri) = params.get("rootUri").and_then(Value::as_str)
            && let Some(path) = file_uri_to_path(root_uri)
        {
            roots.push(path);
        }

        if let Some(workspace_folders) = params.get("workspaceFolders").and_then(Value::as_array) {
            for folder in workspace_folders {
                if let Some(uri) = folder.get("uri").and_then(Value::as_str)
                    && let Some(path) = file_uri_to_path(uri)
                {
                    roots.push(path);
                }
            }
        }

        roots.sort();
        roots.dedup();
        self.workspace_roots = roots;
    }

    fn index_workspace_files(&mut self) {
        let mut files = Vec::new();
        for root in &self.workspace_roots {
            collect_gd_files(root, &mut files);
        }

        files.sort();
        files.dedup();

        for path in files {
            if let Ok(source) = fs::read_to_string(&path) {
                let uri = path_to_file_uri(&path);
                self.workspace_index.upsert_document(&uri, &source);
            }
        }
    }

    fn apply_watched_file_changes(&mut self, params: &Value) {
        let Some(changes) = params.get("changes").and_then(Value::as_array) else {
            return;
        };

        for change in changes {
            let Some(uri) = change.get("uri").and_then(Value::as_str) else {
                continue;
            };
            let change_type = change.get("type").and_then(Value::as_u64).unwrap_or(2);
            if change_type == 3 {
                self.workspace_index.remove_document(uri);
                continue;
            }
            self.reindex_document_from_disk(uri);
        }
    }

    fn reindex_document_from_disk(&mut self, uri: &str) -> bool {
        let Some(path) = file_uri_to_path(uri) else {
            return false;
        };

        if !path.exists() || path.extension().and_then(|ext| ext.to_str()) != Some("gd") {
            return false;
        }

        match fs::read_to_string(&path) {
            Ok(source) => {
                self.workspace_index.upsert_document(uri, &source);
                true
            }
            Err(_) => false,
        }
    }
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    if raw.is_empty() {
        return None;
    }

    if raw.starts_with('/') {
        Some(PathBuf::from(raw))
    } else {
        Some(PathBuf::from(format!("/{raw}")))
    }
}

fn path_to_file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

fn collect_gd_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| matches!(name, ".git" | ".godot" | "target"))
            {
                continue;
            }
            collect_gd_files(&path, out);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("gd") {
            out.push(path);
        }
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
        "initialize" => {
            let init_params = req.params.unwrap_or_default();
            state.configure_workspace_roots(&init_params);
            state.index_workspace_files();
            id.map(|id| {
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
                                "resolveProvider": true,
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
                            "renameProvider": {
                                "prepareProvider": true
                            },
                            "workspaceSymbolProvider": true,
                            "executeCommandProvider": {
                                "commands": ["gdscript-lsp.showDeclaration"]
                            }
                        },
                        "serverInfo": {
                            "name": "gdscript-lsp",
                            "version": "0.1.0"
                        }
                    }
                })
            })
        }
        "initialized"
        | "$/setTrace"
        | "$/cancelRequest"
        | "workspace/didChangeConfiguration"
        | "textDocument/didSave" => None,
        "workspace/didChangeWatchedFiles" => {
            let params = req.params.unwrap_or_default();
            state.apply_watched_file_changes(&params);
            None
        }
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
            let request_uri = extract_uri(&params);

            let parsed = if let Some(uri) = request_uri {
                state
                    .parsed_for_uri(uri)
                    .cloned()
                    .unwrap_or_else(|| parse_script(&source, uri))
            } else {
                parse_script(&source, "stdin://lsp.gd")
            };

            let workspace = state
                .workspace_documents()
                .map(|doc| HoverWorkspaceDoc {
                    uri: doc.uri.as_str(),
                    script: &doc.parsed,
                })
                .collect::<Vec<_>>();

            let hover = hover_at_with_workspace(
                line,
                character,
                &parsed,
                request_uri,
                workspace.as_slice(),
            );
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
            let mut actions = code_actions_for_diagnostics_and_mode(&source, &diagnostics, mode);
            let (line, character) = extract_action_position(&params, transport);

            if let Some(action) = explicit_type_annotation_action(&source, line) {
                actions.push(action);
            }
            if let Some(action) =
                declaration_context_action(state, &params, &source, line, character)
            {
                actions.push(action);
            }

            let filtered = filter_actions_by_context(actions, &params);
            let action_uri = extract_uri(&params).unwrap_or("stdin://lsp.gd");
            let lsp_actions = to_lsp_code_actions(&filtered, action_uri, &source, transport);
            id.map(|id| json!({"id": id, "result": lsp_actions}))
        }
        "codeAction/resolve" | "textDocument/codeAction/resolve" => {
            let params = req.params.unwrap_or_default();
            let resolved = resolve_code_action(state, params, transport);
            id.map(|id| json!({"id": id, "result": resolved}))
        }
        "workspace/executeCommand" => {
            let params = req.params.unwrap_or_default();
            let command = params.get("command").and_then(Value::as_str).unwrap_or("");
            let result = if command == "gdscript-lsp.showDeclaration" {
                params
                    .get("arguments")
                    .and_then(Value::as_array)
                    .and_then(|arguments| arguments.first())
                    .cloned()
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            id.map(|id| json!({ "id": id, "result": result }))
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
            items.extend(completion_entries_for_workspace_declarations(
                state.workspace_documents(),
                prefix.as_deref(),
                &mut seen,
            ));

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
            let mut docs = state.workspace_documents().collect::<Vec<_>>();
            docs.sort_by(|a, b| a.uri.cmp(&b.uri));
            let mut symbols = Vec::new();

            for doc in docs {
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
                            "uri": doc.uri,
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
            let mut locations = symbol
                .as_deref()
                .map(|symbol_name| {
                    definition_locations(state, uri, symbol_name, &source, transport)
                })
                .unwrap_or_default();

            if locations.is_empty()
                && let Some(symbol) = symbol.as_deref()
                && let Some(doc_uri) = definition_uri_for_known_symbol(symbol)
            {
                locations.push(virtual_location_for_uri(&doc_uri, transport));
            }

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
            let include_declaration = params
                .get("context")
                .and_then(|ctx| ctx.get("includeDeclaration"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let locations = symbol
                .as_deref()
                .map(|symbol_name| {
                    reference_locations(
                        state,
                        uri,
                        symbol_name,
                        &source,
                        transport,
                        include_declaration,
                    )
                })
                .unwrap_or_default();

            id.map(|id| json!({"id": id, "result": locations}))
        }
        "textDocument/prepareRename" => {
            let params = req.params.unwrap_or_default();
            let source = source_from_params(&params, state);
            let (line, character) = extract_position(&params, transport);
            let result = symbol_range_at_position(&source, line, character)
                .and_then(|(symbol, start_character, end_character)| {
                    if !is_valid_identifier_name(&symbol) {
                        return None;
                    }
                    if definition_uri_for_known_symbol(&symbol).is_some()
                        && !state.has_workspace_declaration(&symbol)
                        && !source_declares_symbol(&source, &symbol)
                    {
                        return None;
                    }
                    Some(json!({
                        "range": lsp_range(line, start_character, line, end_character, transport),
                        "placeholder": symbol,
                    }))
                })
                .unwrap_or(Value::Null);

            id.map(|id| json!({"id": id, "result": result}))
        }
        "textDocument/rename" => {
            let params = req.params.unwrap_or_default();
            let uri = extract_uri(&params);
            let source = source_from_params(&params, state);
            let (line, character) = extract_position(&params, transport);
            let new_name = params
                .get("newName")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            if !is_valid_identifier_name(&new_name) {
                let error = json!({
                    "code": -32602,
                    "message": "Invalid params: newName must be a valid identifier"
                });
                return (id.map(|id| json!({"id": id, "error": error})), None);
            }

            let result = symbol_range_at_position(&source, line, character)
                .and_then(|(symbol, _, _)| {
                    if definition_uri_for_known_symbol(&symbol).is_some()
                        && !state.has_workspace_declaration(&symbol)
                        && !source_declares_symbol(&source, &symbol)
                    {
                        return None;
                    }

                    let mut occurrences = state.workspace_occurrences_for_symbol(&symbol);
                    if occurrences.is_empty() {
                        let fallback_uri = uri.unwrap_or("stdin://lsp.gd");
                        occurrences = collect_symbol_occurrences(&source, &symbol)
                            .into_iter()
                            .map(|span| SymbolLocation {
                                uri: fallback_uri.to_string(),
                                span,
                            })
                            .collect();
                    }

                    if occurrences.is_empty() {
                        return None;
                    }

                    let mut changes = serde_json::Map::new();
                    for (target_uri, spans) in group_locations_by_uri(occurrences) {
                        let edits = spans
                            .into_iter()
                            .map(|span| {
                                json!({
                                    "range": lsp_range(
                                        span.line,
                                        span.start_character,
                                        span.line,
                                        span.end_character,
                                        transport
                                    ),
                                    "newText": new_name.clone()
                                })
                            })
                            .collect::<Vec<_>>();
                        if !edits.is_empty() {
                            changes.insert(target_uri, Value::Array(edits));
                        }
                    }

                    if changes.is_empty() {
                        return None;
                    }

                    Some(json!({
                        "changes": Value::Object(changes)
                    }))
                })
                .unwrap_or(Value::Null);

            id.map(|id| json!({"id": id, "result": result}))
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

fn extract_action_position(params: &Value, transport: Transport) -> (usize, usize) {
    if let Some(start) = params.get("range").and_then(|range| range.get("start")) {
        let line = start.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let character = start.get("character").and_then(Value::as_u64).unwrap_or(0) as usize;
        return match transport {
            Transport::Framed => (line.saturating_add(1), character.saturating_add(1)),
            Transport::LineDelimited => (line.max(1), character.max(1)),
        };
    }
    extract_position(params, transport)
}

fn declaration_context_action(
    state: &LspState,
    params: &Value,
    source: &str,
    line: usize,
    character: usize,
) -> Option<CodeAction> {
    let symbol = symbol_at_position(source, line, character)?;
    if !is_valid_identifier_name(&symbol) {
        return None;
    }

    let uri = extract_uri(params).unwrap_or("stdin://lsp.gd");
    let target = declarations_for_symbol(state, Some(uri), &symbol, source)
        .into_iter()
        .min_by_key(|decl| decl.line)?;
    let replacement = state
        .source_for_uri(uri)
        .and_then(|doc_source| doc_source.lines().nth(target.line.saturating_sub(1)))
        .unwrap_or_else(|| {
            source
                .lines()
                .nth(target.line.saturating_sub(1))
                .unwrap_or_default()
        })
        .to_string();

    Some(CodeAction {
        title: format!("Show declaration context for `{symbol}`"),
        kind: CodeActionKind::Refactor,
        patch: crate::code_actions::CodeActionPatch {
            line: target.line.max(1),
            replacement: replacement.clone(),
        },
        command: Some("gdscript-lsp.showDeclaration".to_string()),
        data: Some(json!({
            "resolver": "line-replacement",
            "uri": uri,
            "symbol": symbol,
            "line": target.line,
            "start_character": target.start_character,
            "end_character": target.end_character,
            "replacement": replacement
        })),
    })
}

fn explicit_type_annotation_action(source: &str, line: usize) -> Option<CodeAction> {
    let current = source.lines().nth(line.saturating_sub(1))?;
    let (prefix, code, suffix_comment) = split_code_and_comment(current);
    let trimmed = code.trim_start();
    let rest = trimmed.strip_prefix("var ")?;
    let (lhs, rhs) = rest.split_once(":=")?;
    let name = lhs.trim();
    if name.contains(':') || !is_valid_identifier_name(name) {
        return None;
    }
    let rhs = rhs.trim();
    let inferred_type = infer_type_for_annotation(rhs)?;
    let mut replacement = format!("{prefix}var {name}: {inferred_type} = {rhs}");
    if let Some(comment) = suffix_comment.filter(|comment| !comment.is_empty()) {
        replacement.push_str(" # ");
        replacement.push_str(comment.as_str());
    }
    if replacement == current {
        return None;
    }

    Some(CodeAction {
        title: "Add explicit type annotation".to_string(),
        kind: CodeActionKind::QuickFix,
        patch: crate::code_actions::CodeActionPatch { line, replacement },
        command: None,
        data: None,
    })
}

fn split_code_and_comment(line: &str) -> (String, String, Option<String>) {
    let indent = line
        .chars()
        .take_while(|ch| ch.is_ascii_whitespace())
        .collect::<String>();
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
                    return (indent, code, Some(comment));
                }
                idx += 1;
            }
        }
    }
    (indent, line.trim_end().to_string(), None)
}

fn infer_type_for_annotation(expr: &str) -> Option<String> {
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
    if expr == "true" || expr == "false" {
        return Some("bool".to_string());
    }
    if (expr.starts_with('"') && expr.ends_with('"'))
        || (expr.starts_with('\'') && expr.ends_with('\''))
    {
        return Some("String".to_string());
    }
    if (expr.starts_with("&\"") && expr.ends_with('"'))
        || (expr.starts_with("&'") && expr.ends_with('\''))
    {
        return Some("StringName".to_string());
    }
    if expr.starts_with('[') && expr.ends_with(']') {
        return Some("Array".to_string());
    }
    if expr.starts_with('{') && expr.ends_with('}') {
        return Some("Dictionary".to_string());
    }
    if let Some(class_name) = expr.strip_suffix(".new()")
        && is_valid_identifier_name(class_name.trim())
    {
        return Some(class_name.trim().to_string());
    }
    None
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

fn completion_entries_for_workspace_declarations<'a>(
    documents: impl Iterator<Item = &'a SemanticDocument>,
    prefix: Option<&str>,
    seen: &mut HashSet<String>,
) -> Vec<Value> {
    let mut out = Vec::new();
    for doc in documents {
        out.extend(completion_entries_for_declarations(
            &doc.parsed.declarations,
            prefix,
            seen,
        ));
    }
    out
}

fn to_lsp_code_actions(
    actions: &[CodeAction],
    uri: &str,
    source: &str,
    transport: Transport,
) -> Vec<Value> {
    actions
        .iter()
        .map(|action| {
            let line = action.patch.line.max(1);
            let line_text = source
                .lines()
                .nth(line.saturating_sub(1))
                .unwrap_or_default();
            let end_column = line_text.len().saturating_add(1);
            let mut changes = serde_json::Map::new();
            changes.insert(
                uri.to_string(),
                Value::Array(vec![json!({
                    "range": lsp_range(line, 1, line, end_column, transport),
                    "newText": action.patch.replacement
                })]),
            );

            let kind = match action.kind {
                CodeActionKind::QuickFix => "quickfix",
                CodeActionKind::Refactor => "refactor.rewrite",
            };

            let lazy_resolve = action
                .data
                .as_ref()
                .and_then(|data| data.get("resolver"))
                .is_some();

            let mut payload = json!({
                "title": action.title,
                "kind": kind
            });

            if !lazy_resolve {
                payload["edit"] = json!({
                    "changes": Value::Object(changes)
                });
            }

            if let Some(command_name) = &action.command {
                let arguments = action
                    .data
                    .clone()
                    .map(|data| vec![data])
                    .unwrap_or_default();
                payload["command"] = json!({
                    "title": action.title,
                    "command": command_name,
                    "arguments": arguments
                });
            }

            if let Some(data) = &action.data {
                payload["data"] = data.clone();
            }

            payload
        })
        .collect()
}

fn declarations_for_symbol(
    state: &LspState,
    uri: Option<&str>,
    symbol: &str,
    source: &str,
) -> Vec<SymbolSpan> {
    if let Some(uri) = uri {
        let local = state.declarations_by_symbol_for_uri(uri, symbol);
        if !local.is_empty() {
            return local.into_iter().map(|loc| loc.span).collect();
        }
    }

    let parsed = parse_script(source, uri.unwrap_or("stdin://lsp.gd"));
    parsed
        .declarations
        .iter()
        .filter(|decl| decl.name == symbol)
        .map(|decl| {
            let line_text = parsed
                .lines
                .get(decl.line.saturating_sub(1))
                .map(String::as_str)
                .unwrap_or("");
            let (start_character, end_character) = declaration_name_range(line_text, &decl.name);
            SymbolSpan {
                line: decl.line,
                start_character,
                end_character,
            }
        })
        .collect()
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

fn range_for_decl(decl: &SymbolSpan, transport: Transport) -> Value {
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
    symbol_range_at_position(source, line, character).map(|(symbol, _, _)| symbol)
}

fn symbol_range_at_position(
    source: &str,
    line: usize,
    character: usize,
) -> Option<(String, usize, usize)> {
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

    let start_character = start.saturating_add(1);
    let end_character = end.saturating_add(2);
    Some((
        line_text[start..=end].to_string(),
        start_character,
        end_character,
    ))
}

fn collect_symbol_occurrences(source: &str, symbol: &str) -> Vec<SymbolSpan> {
    if symbol.is_empty() {
        return Vec::new();
    }

    let mut occurrences = Vec::new();
    let symbol_bytes = symbol.as_bytes();
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
                if &bytes[start..idx] == symbol_bytes {
                    let start_character = start.saturating_add(1);
                    let end_character = idx.saturating_add(1);
                    occurrences.push(SymbolSpan {
                        line: line_idx.saturating_add(1),
                        start_character,
                        end_character,
                    });
                }
                continue;
            }

            idx += 1;
        }

        if quote.is_some() && !triple {
            quote = None;
            escaped = false;
        }
    }

    occurrences
}

fn is_valid_identifier_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn location_for(uri: &str, decl: &SymbolSpan, transport: Transport) -> Value {
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

fn definition_locations(
    state: &LspState,
    uri: Option<&str>,
    symbol: &str,
    source: &str,
    transport: Transport,
) -> Vec<Value> {
    let mut locations = if let Some(uri) = uri {
        let mut local = state.declarations_by_symbol_for_uri(uri, symbol);
        if local.is_empty() {
            local = state.workspace_declarations_for_symbol(symbol);
        }
        local
    } else {
        state.workspace_declarations_for_symbol(symbol)
    };

    if locations.is_empty() {
        let fallback_uri = uri.unwrap_or("stdin://lsp.gd").to_string();
        locations = declarations_for_symbol(state, uri, symbol, source)
            .into_iter()
            .map(|span| SymbolLocation {
                uri: fallback_uri.clone(),
                span,
            })
            .collect();
    }

    locations.sort_by(|a, b| {
        let preferred_a = uri.is_some_and(|current| current == a.uri);
        let preferred_b = uri.is_some_and(|current| current == b.uri);
        preferred_b
            .cmp(&preferred_a)
            .then_with(|| a.uri.cmp(&b.uri))
            .then_with(|| a.span.line.cmp(&b.span.line))
            .then_with(|| a.span.start_character.cmp(&b.span.start_character))
    });

    locations
        .into_iter()
        .map(|location| location_for(&location.uri, &location.span, transport))
        .collect()
}

fn reference_locations(
    state: &LspState,
    uri: Option<&str>,
    symbol: &str,
    source: &str,
    transport: Transport,
    include_declaration: bool,
) -> Vec<Value> {
    let mut refs = state.workspace_occurrences_for_symbol(symbol);

    if refs.is_empty() {
        let fallback_uri = uri.unwrap_or("stdin://lsp.gd");
        refs = collect_symbol_occurrences(source, symbol)
            .into_iter()
            .map(|span| SymbolLocation {
                uri: fallback_uri.to_string(),
                span,
            })
            .collect();
    }

    if !include_declaration {
        let declaration_keys = state
            .workspace_declarations_for_symbol(symbol)
            .into_iter()
            .map(|decl| {
                (
                    decl.uri,
                    decl.span.line,
                    decl.span.start_character,
                    decl.span.end_character,
                )
            })
            .collect::<HashSet<_>>();
        refs.retain(|location| {
            !declaration_keys.contains(&(
                location.uri.clone(),
                location.span.line,
                location.span.start_character,
                location.span.end_character,
            ))
        });
    }

    refs.sort_by(|a, b| {
        a.uri
            .cmp(&b.uri)
            .then_with(|| a.span.line.cmp(&b.span.line))
            .then_with(|| a.span.start_character.cmp(&b.span.start_character))
    });

    refs.into_iter()
        .map(|location| location_for(&location.uri, &location.span, transport))
        .collect()
}

fn group_locations_by_uri(locations: Vec<SymbolLocation>) -> HashMap<String, Vec<SymbolSpan>> {
    let mut out: HashMap<String, Vec<SymbolSpan>> = HashMap::new();
    for location in locations {
        out.entry(location.uri).or_default().push(location.span);
    }
    for spans in out.values_mut() {
        spans.sort_by(|a, b| {
            a.line
                .cmp(&b.line)
                .then_with(|| a.start_character.cmp(&b.start_character))
                .then_with(|| a.end_character.cmp(&b.end_character))
        });
    }
    out
}

fn source_declares_symbol(source: &str, symbol: &str) -> bool {
    let parsed = parse_script(source, "stdin://source-check.gd");
    parsed.declarations.iter().any(|decl| decl.name == symbol)
}

fn resolve_code_action(state: &LspState, mut action: Value, transport: Transport) -> Value {
    let data = action.get("data").cloned();
    if action.get("edit").is_some() {
        return action;
    }

    let Some(data) = data else {
        return action;
    };
    let resolver = data.get("resolver").and_then(Value::as_str).unwrap_or("");
    if resolver != "line-replacement" {
        return action;
    }

    let uri = data
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or("stdin://lsp.gd");
    let line = data.get("line").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
    let replacement = data
        .get("replacement")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            state.source_for_uri(uri).map(|source| {
                source
                    .lines()
                    .nth(line.saturating_sub(1))
                    .unwrap_or_default()
                    .to_string()
            })
        })
        .unwrap_or_default();

    if replacement.is_empty() {
        return action;
    }

    let source_line = state
        .source_for_uri(uri)
        .and_then(|source| source.lines().nth(line.saturating_sub(1)))
        .unwrap_or_default();
    let end_column = source_line.len().saturating_add(1);
    let mut changes = serde_json::Map::new();
    changes.insert(
        uri.to_string(),
        Value::Array(vec![json!({
            "range": lsp_range(line, 1, line, end_column, transport),
            "newText": replacement
        })]),
    );
    action["edit"] = json!({
        "changes": Value::Object(changes)
    });
    action
}

fn virtual_location_for_uri(uri: &str, transport: Transport) -> Value {
    let (line, character) = match transport {
        Transport::Framed => (0, 0),
        Transport::LineDelimited => (1, 1),
    };
    json!({
        "uri": uri,
        "range": {
            "start": {
                "line": line,
                "character": character,
            },
            "end": {
                "line": line,
                "character": character,
            }
        }
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
