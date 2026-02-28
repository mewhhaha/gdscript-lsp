use gdscript_lsp::lsp;
use serde_json::{self, Value};
use std::{collections::HashSet, fs, path::PathBuf};

fn run_lsp(input: &str) -> String {
    let mut out = Vec::new();
    lsp::run_stdio_with(input.as_bytes(), &mut out).expect("lsp run");
    String::from_utf8(out).expect("utf8")
}

fn run_lsp_responses(input: &str) -> Vec<Value> {
    let output = run_lsp(input);
    output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn response_by_id(outputs: &[Value], id: u64) -> Option<&Value> {
    outputs
        .iter()
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(id))
}

fn frame_message(message: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", message.len(), message)
}

fn parse_framed_output(output: &str) -> Vec<Value> {
    let mut values = Vec::new();
    let mut rest = output;

    while let Some(header_end) = rest.find("\r\n") {
        let header = &rest[..header_end];
        let Some(raw_len) = header.strip_prefix("Content-Length: ") else {
            break;
        };
        let Ok(length) = raw_len.trim().parse::<usize>() else {
            break;
        };

        rest = &rest[header_end + 2..];
        if !rest.starts_with("\r\n") {
            break;
        }
        rest = &rest[2..];

        if rest.len() < length {
            break;
        }
        let body = &rest[..length];
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            values.push(value);
        }
        rest = &rest[length..];
    }

    values
}

fn count_notification_errors(outputs: &[Value], notification_id: u64) -> usize {
    outputs
        .iter()
        .filter(|value| {
            value.get("id").and_then(Value::as_u64) == Some(notification_id)
                && value.get("error").is_some()
        })
        .count()
}

fn fixture_text(suite: &str, name: &str, file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(suite)
        .join(name)
        .join(file);

    fs::read_to_string(path).unwrap()
}

fn did_open_message(uri: &str, source: &str) -> String {
    serde_json::to_string(&serde_json::json!({
        "method": "didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "text": source,
            },
        },
    }))
    .unwrap()
}

#[test]
fn lsp_signature_help_supports_line_transport() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-signaturehelp.gd";

    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str("{\"id\":2,\"method\":\"textDocument/signatureHelp\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-signaturehelp.gd\"},\"position\":{\"line\":1,\"character\":6}}}\n");
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let signature_help = response_by_id(&responses, 2).expect("signatureHelp response");
    let signatures = signature_help["result"]["signatures"]
        .as_array()
        .expect("signatures");
    assert_eq!(
        signatures.len(),
        1,
        "signature should resolve declaration: {signature_help:#?}"
    );
    assert_eq!(
        signatures[0]["label"].as_str(),
        Some("func define_value():"),
        "signature label should match declaration text: {signature_help:#?}"
    );
    assert_eq!(
        signatures[0]["documentation"]["value"]
            .as_str()
            .unwrap_or(""),
        "Function `define_value`",
        "signature documentation should match declaration: {signature_help:#?}"
    );
}

#[test]
fn lsp_signature_help_supports_framed_transport() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-signaturehelp.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, &source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/signatureHelp","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-signaturehelp.gd"},"position":{"line":0,"character":5}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let signature_help = response_by_id(&responses, 2).expect("signatureHelp response");
    let signatures = signature_help["result"]["signatures"]
        .as_array()
        .expect("signatures");
    assert_eq!(
        signatures.len(),
        1,
        "signature should resolve declaration: {signature_help:#?}"
    );
    assert_eq!(
        signatures[0]["label"].as_str(),
        Some("func define_value():"),
        "signature label should match declaration text: {signature_help:#?}"
    );
    assert_eq!(
        signatures[0]["documentation"]["value"]
            .as_str()
            .unwrap_or(""),
        "Function `define_value`",
        "signature documentation should match declaration: {signature_help:#?}"
    );
    assert_eq!(
        signature_help["result"]["activeSignature"].as_u64(),
        Some(0),
        "active signature should be first entry: {signature_help:#?}"
    );
    assert_eq!(
        signature_help["result"]["activeParameter"].as_u64(),
        Some(0),
        "active parameter should be zero for function start: {signature_help:#?}"
    );
}

#[test]
fn lsp_document_highlight_supports_line_transport() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-highlight.gd";

    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str("{\"id\":2,\"method\":\"textDocument/documentHighlight\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-highlight.gd\"},\"position\":{\"line\":1,\"character\":6}}}\n");
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let highlight = response_by_id(&responses, 2).expect("documentHighlight response");
    let highlights = highlight["result"].as_array().expect("highlights");
    assert_eq!(
        highlights.len(),
        1,
        "highlight list should include declaration: {highlight:#?}"
    );
    assert_eq!(highlights[0]["range"]["start"]["line"], 1);
    assert_eq!(highlights[0]["range"]["start"]["character"], 6);
    assert_eq!(highlights[0]["range"]["end"]["character"], 18);
    assert_eq!(highlights[0]["kind"], 1);
}

#[test]
fn lsp_document_highlight_supports_framed_transport() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-highlight.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, &source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/documentHighlight","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-highlight.gd"},"position":{"line":0,"character":5}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let highlight = response_by_id(&responses, 2).expect("documentHighlight response");
    let highlights = highlight["result"].as_array().expect("highlights");
    assert_eq!(
        highlights.len(),
        1,
        "highlight list should include declaration: {highlight:#?}"
    );
    assert_eq!(highlights[0]["range"]["start"]["line"], 0);
    assert_eq!(highlights[0]["range"]["start"]["character"], 5);
    assert_eq!(highlights[0]["range"]["end"]["character"], 17);
    assert_eq!(highlights[0]["kind"], 1);
}

#[test]
fn lsp_workspace_symbol_supports_line_transport() {
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(
        "file:///tmp/fixtures/gdscript-lsp-workspace-symbol.gd",
        &fixture_text("lsp", "definition-references", "input.gd"),
    ));
    requests.push('\n');
    requests.push_str(
        "{\"id\":2,\"method\":\"workspace/symbol\",\"params\":{\"query\":\"define_value\"}}\n",
    );
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let workspace_symbol = response_by_id(&responses, 2).expect("workspace/symbol response");
    let symbols = workspace_symbol["result"]
        .as_array()
        .expect("workspace symbols");
    assert_eq!(
        symbols.len(),
        1,
        "query should match one symbol: {workspace_symbol:#?}"
    );
    assert_eq!(symbols[0]["name"], "define_value");
    assert_eq!(
        symbols[0]["location"]["uri"], "file:///tmp/fixtures/gdscript-lsp-workspace-symbol.gd",
        "symbol URI should match opened document"
    );
    assert_eq!(
        symbols[0]["location"]["range"]["start"]["line"], 1,
        "declaration should be on first line"
    );
    assert_eq!(symbols[0]["kind"], 12);
}

#[test]
fn lsp_workspace_symbol_supports_framed_transport() {
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(
        "file:///tmp/fixtures/gdscript-lsp-workspace-symbol.gd",
        &fixture_text("lsp", "definition-references", "input.gd"),
    )));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"workspace/symbol","params":{"query":"define_value"}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let workspace_symbol = response_by_id(&responses, 2).expect("workspace/symbol response");
    let symbols = workspace_symbol["result"]
        .as_array()
        .expect("workspace symbols");
    assert_eq!(
        symbols.len(),
        1,
        "query should match one symbol: {workspace_symbol:#?}"
    );
    assert_eq!(symbols[0]["name"], "define_value");
    assert_eq!(
        symbols[0]["location"]["uri"], "file:///tmp/fixtures/gdscript-lsp-workspace-symbol.gd",
        "symbol URI should match opened document"
    );
    assert_eq!(
        symbols[0]["location"]["range"]["start"]["line"], 0,
        "declaration should be on first line in 0-based coordinates"
    );
    assert_eq!(symbols[0]["kind"], 12);
}

#[test]
fn initialize_reports_core_capabilities() {
    let output = run_lsp("{\"id\":1,\"method\":\"initialize\"}\n{\"method\":\"exit\"}\n");
    assert!(output.contains("hoverProvider"), "output: {output}");
    assert!(output.contains("codeActionProvider"), "output: {output}");
}

#[test]
fn initialize_reports_lsp_317_diagnostic_provider_shape() {
    let responses = run_lsp_responses("{\"id\":1,\"method\":\"initialize\"}\n{\"method\":\"exit\"}\n");
    let init = response_by_id(&responses, 1).expect("initialize response");
    let diagnostic_provider = &init["result"]["capabilities"]["diagnosticProvider"];

    assert_eq!(
        diagnostic_provider["interFileDependencies"].as_bool(),
        Some(false),
        "diagnosticProvider.interFileDependencies should be false: {init:#?}"
    );
    assert_eq!(
        diagnostic_provider["workspaceDiagnostics"].as_bool(),
        Some(false),
        "diagnosticProvider.workspaceDiagnostics should be false: {init:#?}"
    );
}

#[test]
fn code_action_method_returns_actions() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/codeAction\",\"params\":{\"text\":\"a=1 \\n\"}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("Trim trailing whitespace"),
        "output: {output}"
    );
}

#[test]
fn hover_method_returns_builtin_payload() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\",\"line\":2,\"character\":7}}\n{\"method\":\"exit\"}\n",
    );

    assert!(output.contains("print"), "output: {output}");
}

#[test]
fn tracked_document_lifecycle_uses_uri_for_diagnostics_and_edits() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-tracked.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&format!(
        "{{\"method\":\"didOpen\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\",\"text\":\"a=1 \\n\\tprint(\\\\\\\"x\\\\\\\")\\n\"}}}}}}\n"
    ));
    requests.push_str("{\"id\":2,\"method\":\"textDocument/diagnostic\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"}}}\n");
    requests.push_str("{\"id\":3,\"method\":\"textDocument/codeAction\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"}}}\n");
    requests.push_str(
        "{\"method\":\"didChange\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"},\"contentChanges\":[{\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\"}]}}\n",
    );
    requests.push_str("{\"id\":4,\"method\":\"textDocument/diagnostic\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"}}}\n");
    requests.push_str("{\"id\":5,\"method\":\"textDocument/hover\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"},\"position\":{\"line\":2,\"character\":11}}}\n");
    requests.push_str(
        "{\"method\":\"didClose\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-tracked.gd\"}}}\n",
    );
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);

    assert_eq!(
        count_notification_errors(&responses, 0),
        0,
        "didOpen/didChange/didClose should be notifications without responses: {responses:#?}"
    );

    let diagnostic_before = response_by_id(&responses, 2).expect("diagnostic before change");
    let diagnostics_before = diagnostic_before["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        !diagnostics_before.is_empty(),
        "tracked document should carry diagnostics from didOpen text: {diagnostic_before:#?}"
    );

    let code_action_before = response_by_id(&responses, 3).expect("code action before change");
    let actions = code_action_before["result"]
        .as_array()
        .expect("code actions");
    assert!(
        actions.iter().any(|action| action["title"]
            .as_str()
            .unwrap_or("")
            .contains("Trim trailing whitespace")),
        "code actions should track opened document text: {code_action_before:#?}"
    );

    let diagnostic_after = response_by_id(&responses, 4).expect("diagnostic after change");
    let diagnostics_after = diagnostic_after["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        diagnostics_after.is_empty(),
        "clean didChange content should remove diagnostics: {diagnostic_after:#?}"
    );

    let hover = response_by_id(&responses, 5).expect("hover result");
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|contents| contents.contains("print")),
        "hover should resolve builtin against tracked URI document: {hover:#?}"
    );
}

#[test]
fn lsp_handles_standard_hover_and_diagnostic_queries_for_uris() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-uri-query.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":2,\"character\":11}},\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\"}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":3,\"method\":\"textDocument/documentDiagnostic\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\"}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":4,\"method\":\"textDocument/codeAction\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\"}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let hover = response_by_id(&responses, 2).expect("hover response");
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|contents| contents.contains("print")),
        "hover should return print payload: {hover:#?}"
    );

    let diag = response_by_id(&responses, 3).expect("diagnostic response");
    assert!(
        diag["result"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .is_empty(),
        "diagnostic should be empty for clean source: {diag:#?}"
    );

    let actions = response_by_id(&responses, 4).expect("codeAction response");
    assert!(
        actions["result"].as_array().expect("actions").is_empty(),
        "clean source should not suggest fixes: {actions:#?}"
    );
}

#[test]
fn lsp_document_diagnostic_includes_parser_errors() {
    let responses = run_lsp_responses(
        "{\"id\":1,\"method\":\"textDocument/documentDiagnostic\",\"params\":{\"text\":\"func test():\\n    if true\\n        pass\\n\"}}\n{\"method\":\"exit\"}\n",
    );
    let diagnostic = response_by_id(&responses, 1).expect("diagnostic response");
    let diagnostics = diagnostic["result"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");

    assert!(
        diagnostics
            .iter()
            .any(|entry| entry["code"] == "parser-error"),
        "expected parser-error diagnostic payload: {diagnostic:#?}"
    );
    assert!(
        diagnostics.iter().any(|entry| {
            entry["message"]
                .as_str()
                .is_some_and(|message| message.contains("Expected \":\" after \"if\" condition."))
        }),
        "expected missing-colon parser message in payload: {diagnostic:#?}"
    );
}

#[test]
fn framed_transport_roundtrip_initialize() {
    let request = format!(
        "{}{}",
        frame_message(r#"{"id":1,"method":"initialize"}"#),
        frame_message(r#"{"method":"exit"}"#)
    );
    let output = run_lsp(&request);

    assert!(
        output.starts_with("Content-Length: "),
        "expected framed output, got: {output}"
    );

    let framed = parse_framed_output(&output);
    let init = response_by_id(&framed, 1).expect("initialize response");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false),
        "initialize payload missing hover capability: {init:#?}"
    );
}

#[test]
fn framed_transport_roundtrip_handles_notifications() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-framed-notifs.gd";
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&format!(
        "{{\"method\":\"didOpen\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\",\"text\":\"a=1\\n\"}}}}}}"
    )));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let init = response_by_id(&responses, 1).expect("initialize response");
    assert_eq!(count_notification_errors(&responses, 0), 0);
    assert_eq!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false),
        true,
        "initialize payload missing hoverProvider: {init:#?}"
    );
}

#[test]
fn framed_initialized_notification_does_not_emit_method_not_found() {
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(r#"{"method":"initialized","params":{}}"#));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let init = response_by_id(&responses, 1).expect("initialize response");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false),
        "initialize payload missing hoverProvider: {init:#?}"
    );
    assert!(
        responses.iter().all(|value| {
            value.get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(|message| message != "unknown method")
                .unwrap_or(true)
        }),
        "initialized notification should not trigger unknown-method errors: {responses:#?}"
    );
}

#[test]
fn framed_did_open_publishes_lsp_diagnostic_ranges() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-framed-diagnostics.gd";
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&format!(
        "{{\"method\":\"didOpen\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\",\"text\":\"a=1 \\n\\tprint(\\\\\\\"x\\\\\\\")\\n\"}}}}}}"
    )));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let publish = responses
        .iter()
        .find(|value| value["method"] == "textDocument/publishDiagnostics")
        .expect("publishDiagnostics notification");
    let diagnostics = publish["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        diagnostics.iter().any(|diag| diag["range"].is_object()),
        "publishDiagnostics entries should include LSP range fields: {publish:#?}"
    );
}

#[test]
fn framed_transport_supports_completion_and_returns_empty_document_symbol() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-framed-capabilities.gd";
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&format!(
        "{{\"id\":2,\"method\":\"textDocument/completion\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":1,\"character\":1}}}}}}"
    )));
    requests.push_str(&frame_message(&format!(
        "{{\"id\":3,\"method\":\"textDocument/documentSymbol\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}}}}}}"
    )));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let completion = response_by_id(&responses, 2).expect("completion response");
    let document_symbol = response_by_id(&responses, 3).expect("documentSymbol response");
    let labels: HashSet<String> = completion["result"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["label"].as_str().map(ToString::to_string))
        .collect();

    assert!(
        completion["result"]["isIncomplete"].as_bool().is_some(),
        "completion should return LSP completion payload: {completion:#?}"
    );
    assert!(
        labels.contains("cos"),
        "completion should include snapshot builtin candidates: {completion:#?}"
    );
    assert!(
        document_symbol["result"]
            .as_array()
            .is_some_and(|items| items.is_empty()),
        "documentSymbol should currently return empty symbol list: {document_symbol:#?}"
    );
}

#[test]
fn framed_code_action_filter_accepts_fixall_context() {
    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(
        &frame_message(
            r#"{"id":2,"method":"textDocument/codeAction","params":{"text":"a=1 \n","context":{"only":["source.fixAll"]}}}"#,
        ),
    );
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let response = response_by_id(&responses, 2).expect("codeAction response");
    let actions = response["result"].as_array().expect("actions");

    assert!(
        !actions.is_empty(),
        "fixAll context should still produce quick-fix candidates: {response:#?}"
    );
    assert!(
        actions.iter().any(|action| action["title"]
            .as_str()
            .unwrap_or("")
            .contains("Trim trailing whitespace")),
        "expected trim-whitespace action from fixAll context: {actions:#?}"
    );
    assert!(
        actions.iter().any(|action| action["title"]
            .as_str()
            .unwrap_or("")
            .contains("Insert spaces around operator")),
        "expected spacing action from fixAll context: {actions:#?}"
    );
}

#[test]
fn lsp_definition_supports_line_and_framed_transports() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-definition-references.gd";

    let mut line_requests = String::new();
    line_requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    line_requests.push_str(&did_open_message(uri, &source));
    line_requests.push('\n');
    line_requests.push_str("{\"id\":2,\"method\":\"textDocument/definition\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-definition-references.gd\"},\"position\":{\"line\":5,\"character\":6}}}\n");
    line_requests.push_str("{\"method\":\"exit\"}\n");

    let line_outputs = run_lsp_responses(&line_requests);
    let line_definition = response_by_id(&line_outputs, 2).expect("line definition response");
    let line_locations = line_definition["result"].as_array().expect("locations");
    assert_eq!(
        line_locations.len(),
        1,
        "line definition should return one declaration: {line_definition:#?}"
    );
    assert_eq!(
        line_locations[0]["range"]["start"]["line"], 1,
        "definition line should point to declaration"
    );
    assert_eq!(
        line_locations[0]["range"]["start"]["character"], 6,
        "definition column should point to declaration"
    );

    let mut framed_requests = String::new();
    framed_requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    framed_requests.push_str(&frame_message(&did_open_message(uri, &source)));
    framed_requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-definition-references.gd"},"position":{"line":4,"character":5}}}"#,
    ));
    framed_requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let framed_outputs = parse_framed_output(&run_lsp(&framed_requests));
    let framed_definition = response_by_id(&framed_outputs, 2).expect("framed definition response");
    let framed_locations = framed_definition["result"].as_array().expect("locations");
    assert_eq!(
        framed_locations.len(),
        1,
        "framed definition should return one declaration: {framed_definition:#?}"
    );
    assert_eq!(
        framed_locations[0]["range"]["start"]["line"], 0,
        "definition line should match declaration"
    );
    assert_eq!(
        framed_locations[0]["range"]["start"]["character"], 5,
        "definition character should match declaration"
    );
}

#[test]
fn lsp_references_supports_line_and_framed_transports() {
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-references.gd";

    let mut line_requests = String::new();
    line_requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    line_requests.push_str(&did_open_message(uri, &source));
    line_requests.push('\n');
    line_requests.push_str("{\"id\":2,\"method\":\"textDocument/references\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-references.gd\"},\"position\":{\"line\":5,\"character\":6}}}\n");
    line_requests.push_str("{\"method\":\"exit\"}\n");

    let line_outputs = run_lsp_responses(&line_requests);
    let line_references = response_by_id(&line_outputs, 2).expect("line references response");
    let line_locations = line_references["result"].as_array().expect("locations");
    let line_lines: HashSet<u64> = line_locations
        .iter()
        .filter_map(|loc| loc["range"]["start"]["line"].as_u64())
        .collect();
    assert!(
        line_lines.contains(&1),
        "line references should include declaration line: {line_references:#?}"
    );

    let mut framed_requests = String::new();
    framed_requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    framed_requests.push_str(&frame_message(&did_open_message(uri, &source)));
    framed_requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-references.gd"},"position":{"line":4,"character":5}}}"#,
    ));
    framed_requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let framed_outputs = parse_framed_output(&run_lsp(&framed_requests));
    let framed_references = response_by_id(&framed_outputs, 2).expect("framed references response");
    let framed_locations = framed_references["result"].as_array().expect("locations");
    let framed_lines: HashSet<u64> = framed_locations
        .iter()
        .filter_map(|loc| loc["range"]["start"]["line"].as_u64())
        .collect();
    assert!(
        framed_lines.contains(&0),
        "framed references should include declaration line: {framed_references:#?}"
    );
}

#[test]
fn completion_includes_user_symbols_in_framed_mode() {
    let source = fixture_text("lsp", "completion-user-symbols", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-completion-user-symbols.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, &source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-completion-user-symbols.gd"},"position":{"line":7,"character":8}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let completion = response_by_id(&responses, 2).expect("completion response");
    let items = completion["result"]["items"]
        .as_array()
        .expect("completion items");
    let labels: HashSet<String> = items
        .iter()
        .filter_map(|item| item["label"].as_str().map(ToString::to_string))
        .collect();

    assert!(
        labels.contains("calculate_total"),
        "completion should include user declaration labels: {completion:#?}"
    );
}
