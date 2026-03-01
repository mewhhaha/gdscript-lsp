use gdscript_lsp::lsp;
use serde_json::{self, Value, json};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
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
    assert!(output.contains("renameProvider"), "output: {output}");
}

#[test]
fn initialize_reports_lsp_317_diagnostic_provider_shape() {
    let responses =
        run_lsp_responses("{\"id\":1,\"method\":\"initialize\"}\n{\"method\":\"exit\"}\n");
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
fn hover_method_returns_node_method_payload() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"extends Node\\nfunc _ready():\\n    queue_free()\\n\",\"line\":3,\"character\":7}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("queue_free"),
        "hover should include node method symbol: {output}"
    );
    assert!(
        output.contains("Node method"),
        "hover should include node method context: {output}"
    );
}

#[test]
fn hover_on_value_includes_type_value_and_comments() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"# health points\\nvar health: int = 100 # initial hp\\nfunc _ready():\\n    print(health)\\n\",\"line\":4,\"character\":11}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("```gdscript\\nvar health: int = 100\\n```"),
        "output: {output}"
    );
    assert!(output.contains("health points"), "output: {output}");
    assert!(output.contains("initial hp"), "output: {output}");
}

#[test]
fn hover_on_type_uses_local_declaration_context() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"# Player entity\\nclass Player:\\n    pass\\nvar actor: Player\\n\",\"line\":4,\"character\":12}}\n{\"method\":\"exit\"}\n",
    );

    assert!(output.contains("class"), "output: {output}");
    assert!(output.contains("Type: `type`"), "output: {output}");
    assert!(output.contains("Player entity"), "output: {output}");
}

#[test]
fn hover_on_literal_includes_literal_type() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready():\\n    print(42)\\n\",\"line\":2,\"character\":12}}\n{\"method\":\"exit\"}\n",
    );

    assert!(output.contains("Type: `int`"), "output: {output}");
    assert!(output.contains("Value: `42`"), "output: {output}");
}

#[test]
fn hover_on_typed_receiver_method_uses_docs_metadata() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready():\\n    var _rng: RandomNumberGenerator = RandomNumberGenerator.new()\\n    _rng.randomize()\\n\",\"line\":3,\"character\":12}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("RandomNumberGenerator method randomize() -> void"),
        "output: {output}"
    );
    assert!(output.contains("time-based seed"), "output: {output}");
}

#[test]
fn hover_on_typed_receiver_method_uses_inherited_docs_metadata() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"var node_ref: Node3D\\nfunc _ready():\\n    node_ref.queue_free()\\n\",\"line\":3,\"character\":15}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("Node method queue_free() -> void"),
        "output: {output}"
    );
    assert!(
        output.contains("Queues this node to be deleted"),
        "output: {output}"
    );
}

#[test]
fn hover_on_chained_typed_receiver_method_uses_docs_metadata() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _make_rng() -> RandomNumberGenerator:\\n    return RandomNumberGenerator.new()\\n\\nfunc _ready():\\n    _make_rng().randomize()\\n\",\"line\":5,\"character\":19}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("RandomNumberGenerator method randomize() -> void"),
        "output: {output}"
    );
}

#[test]
fn hover_on_property_chain_uses_typed_receiver_for_property_and_method() {
    let property_output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready() -> void:\\n    var _player: AudioStreamPlayer = AudioStreamPlayer.new()\\n    _player.stream.get_length()\\n\",\"line\":3,\"character\":14}}\n{\"method\":\"exit\"}\n",
    );
    assert!(
        property_output.contains("AudioStreamPlayer property stream: AudioStream"),
        "output: {property_output}"
    );
    assert!(
        property_output.contains("Type: `AudioStream`"),
        "output: {property_output}"
    );

    let method_output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready() -> void:\\n    var _player: AudioStreamPlayer = AudioStreamPlayer.new()\\n    _player.stream.get_length()\\n\",\"line\":3,\"character\":21}}\n{\"method\":\"exit\"}\n",
    );
    assert!(
        method_output.contains("AudioStream method get_length() -> float"),
        "output: {method_output}"
    );
    assert!(
        !method_output.contains("ambiguous method"),
        "method hover should not be ambiguous: {method_output}"
    );
}

#[test]
fn hover_on_property_inside_call_argument_keeps_member_symbol() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready() -> void:\\n    var _impact_sfx: AudioStreamPlayer = AudioStreamPlayer.new()\\n    var max_offset := maxf(_impact_sfx.stream.get_length() - 0.001, 0.0)\\n\",\"line\":3,\"character\":40}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("AudioStreamPlayer property stream: AudioStream"),
        "output: {output}"
    );
    assert!(
        !output.contains("var max_offset"),
        "hover should not backtrack to assignment target: {output}"
    );
}

#[test]
fn hover_on_ambiguous_method_reports_candidates() {
    let output = run_lsp(
        "{\"id\":1,\"method\":\"textDocument/hover\",\"params\":{\"text\":\"func _ready():\\n    clear()\\n\",\"line\":2,\"character\":7}}\n{\"method\":\"exit\"}\n",
    );

    assert!(
        output.contains("ambiguous method `clear`"),
        "output: {output}"
    );
    assert!(
        output.contains("Multiple Godot methods match"),
        "output: {output}"
    );
}

#[test]
fn hover_handles_shadowing_nested_scope_and_multiline_function_sections() {
    let source = fixture_text("lsp", "hover-rich", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-hover-rich.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":22,\"character\":19}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":3,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":23,\"character\":15}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":4,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":15,\"character\":11}}}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let inner = response_by_id(&responses, 2).expect("inner shadow hover");
    let outer = response_by_id(&responses, 3).expect("outer shadow hover");
    let function_hover = response_by_id(&responses, 4).expect("function hover");

    assert!(
        inner["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("var score: int = 99")),
        "inner scope hover should resolve nested shadowed variable: {inner:#?}"
    );
    assert!(
        outer["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("var score: int = 10")),
        "outer scope hover should resolve function-level variable: {outer:#?}"
    );
    let function_contents = function_hover["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        function_contents.contains("Parameters: player_name: String, multiplier: float = 1.0"),
        "multiline function hover should include parameter section: {function_hover:#?}"
    );
    assert!(
        function_contents.contains("Returns: `int`"),
        "function hover should include return type section: {function_hover:#?}"
    );
}

#[test]
fn hover_type_sections_include_inheritance_and_enum_members() {
    let source = fixture_text("lsp", "hover-rich", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-hover-rich-type.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":6,\"character\":8}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":3,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":13,\"character\":16}}}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let actor_hover = response_by_id(&responses, 2).expect("Actor hover");
    let enum_hover = response_by_id(&responses, 3).expect("State enum hover");

    assert!(
        actor_hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("Inherits: `BaseEntity`")),
        "class hover should include inheritance section: {actor_hover:#?}"
    );
    assert!(
        enum_hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("Members: IDLE, RUNNING, JUMPING")),
        "enum hover should include enum member list: {enum_hover:#?}"
    );
}

#[test]
fn docs_parity_fixture_covers_typed_hover_and_ambiguous_fallbacks() {
    let source = fixture_text("lsp", "docs-parity", "input.gd");
    let uri = "file:///tmp/fixtures/gdscript-lsp-docs-parity.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":9,\"character\":11}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":3,\"method\":\"textDocument/definition\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":11,\"character\":7}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":4,\"method\":\"textDocument/hover\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":14,\"character\":7}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":5,\"method\":\"textDocument/definition\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"position\":{{\"line\":14,\"character\":7}}}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let typed_hover = response_by_id(&responses, 2).expect("typed hover response");
    let queue_free_def = response_by_id(&responses, 3).expect("queue_free definition response");
    let ambiguous_hover = response_by_id(&responses, 4).expect("ambiguous hover response");
    let ambiguous_def = response_by_id(&responses, 5).expect("ambiguous definition response");

    assert!(
        typed_hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("RandomNumberGenerator method randomize() -> void")),
        "typed hover should match RandomNumberGenerator docs: {typed_hover:#?}"
    );
    assert!(
        queue_free_def["result"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry["uri"]
                    .as_str()
                    .is_some_and(|uri| uri.contains("class_node.html#class-node-method-queue-free"))
            })),
        "definition should resolve inherited Node method docs: {queue_free_def:#?}"
    );
    assert!(
        ambiguous_hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("ambiguous method `clear`")),
        "ambiguous hover should describe unresolved overloads: {ambiguous_hover:#?}"
    );
    assert!(
        ambiguous_def["result"]
            .as_array()
            .is_some_and(|entries| entries.len() > 1),
        "ambiguous definition should return deterministic multi-doc candidates: {ambiguous_def:#?}"
    );
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
fn code_action_includes_explicit_type_annotation_fix() {
    let responses = run_lsp_responses(
        "{\"id\":1,\"method\":\"textDocument/codeAction\",\"params\":{\"text\":\"func _ready():\\n    var speed := 3.5\\n\",\"range\":{\"start\":{\"line\":2,\"character\":9},\"end\":{\"line\":2,\"character\":9}}}}\n{\"method\":\"exit\"}\n",
    );
    let code_action = response_by_id(&responses, 1).expect("codeAction response");
    let actions = code_action["result"].as_array().expect("actions");
    let annotate = actions
        .iter()
        .find(|action| action["title"] == "Add explicit type annotation")
        .expect("explicit type annotation action");

    assert_eq!(
        annotate["edit"]["changes"]["stdin://lsp.gd"][0]["newText"].as_str(),
        Some("    var speed: float = 3.5"),
        "annotation replacement should include inferred float type: {annotate:#?}"
    );
}

#[test]
fn code_action_includes_declaration_context_action() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-code-action-declaration.gd";
    let source = fixture_text("lsp", "definition-references", "input.gd");
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, &source));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/codeAction\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"range\":{{\"start\":{{\"line\":5,\"character\":6}},\"end\":{{\"line\":5,\"character\":6}}}}}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let code_action = response_by_id(&responses, 2).expect("codeAction response");
    let actions = code_action["result"].as_array().expect("actions");
    let declaration_context = actions
        .iter()
        .find(|action| {
            action["title"]
                .as_str()
                .is_some_and(|title| title.contains("Show declaration context"))
        })
        .expect("declaration context action");

    assert_eq!(
        declaration_context["command"]["command"].as_str(),
        Some("gdscript-lsp.showDeclaration"),
        "declaration action should provide executable command metadata: {declaration_context:#?}"
    );
    assert_eq!(
        declaration_context["data"]["symbol"].as_str(),
        Some("define_value"),
        "declaration action should include symbol payload: {declaration_context:#?}"
    );
}

#[test]
fn code_action_includes_new_lint_fixes() {
    let source = "\
@export @onready var value = 1\n\
var player = get_node(\"%Player\")\n\
1 + 2\n\
";
    let request = serde_json::to_string(&json!({
        "id": 1,
        "method": "textDocument/codeAction",
        "params": {
            "text": source
        }
    }))
    .expect("codeAction request");
    let responses = run_lsp_responses(&format!("{request}\n{{\"method\":\"exit\"}}\n"));
    let code_action = response_by_id(&responses, 1).expect("codeAction response");
    let actions = code_action["result"].as_array().expect("actions");

    let remove_onready = actions
        .iter()
        .find(|action| action["title"] == "Remove @onready to keep export precedence")
        .expect("remove onready action");
    assert_eq!(
        remove_onready["edit"]["changes"]["stdin://lsp.gd"][0]["newText"].as_str(),
        Some("@export var value = 1"),
        "onready-with-export action should rewrite decorator order: {remove_onready:#?}"
    );

    let add_onready = actions
        .iter()
        .find(|action| action["title"] == "Add @onready annotation")
        .expect("add onready action");
    assert_eq!(
        add_onready["edit"]["changes"]["stdin://lsp.gd"][0]["newText"].as_str(),
        Some("@onready var player = get_node(\"%Player\")"),
        "get-node-default action should add annotation: {add_onready:#?}"
    );

    let consume_expression = actions
        .iter()
        .find(|action| action["title"] == "Consume standalone expression with discard")
        .expect("consume standalone expression action");
    assert_eq!(
        consume_expression["edit"]["changes"]["stdin://lsp.gd"][0]["newText"].as_str(),
        Some("_ = 1 + 2"),
        "standalone-expression action should insert discard assignment: {consume_expression:#?}"
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
            value
                .get("error")
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
fn lsp_definition_falls_back_to_docs_for_builtin_and_node_methods() {
    let responses = run_lsp_responses(
        "{\"id\":1,\"method\":\"textDocument/definition\",\"params\":{\"text\":\"func _ready():\\n    print(\\\"x\\\")\\n\",\"line\":2,\"character\":7}}\n{\"id\":2,\"method\":\"textDocument/definition\",\"params\":{\"text\":\"extends Node\\nfunc _ready():\\n    queue_free()\\n\",\"line\":3,\"character\":7}}\n{\"method\":\"exit\"}\n",
    );
    let builtin = response_by_id(&responses, 1).expect("builtin definition");
    let node = response_by_id(&responses, 2).expect("node definition");
    let builtin_locations = builtin["result"].as_array().expect("builtin locations");
    let node_locations = node["result"].as_array().expect("node locations");

    assert_eq!(
        builtin_locations.len(),
        1,
        "builtin definition: {builtin:#?}"
    );
    assert!(
        builtin_locations[0]["uri"]
            .as_str()
            .is_some_and(|uri| uri.contains("class_@globalscope")),
        "builtin definition should point to globalscope docs: {builtin:#?}"
    );

    assert_eq!(node_locations.len(), 1, "node definition: {node:#?}");
    assert!(
        node_locations[0]["uri"]
            .as_str()
            .is_some_and(|uri| uri.contains("class_node")),
        "node definition should point to Node docs: {node:#?}"
    );
}

#[test]
fn lsp_definition_returns_multiple_docs_for_ambiguous_method_without_receiver() {
    let responses = run_lsp_responses(
        "{\"id\":1,\"method\":\"textDocument/definition\",\"params\":{\"text\":\"func _ready():\\n    clear()\\n\",\"line\":2,\"character\":7}}\n{\"method\":\"exit\"}\n",
    );
    let definition = response_by_id(&responses, 1).expect("definition response");
    let locations = definition["result"].as_array().expect("locations");

    assert!(
        locations.len() > 1,
        "ambiguous method should provide multiple deterministic docs candidates: {definition:#?}"
    );
}

#[test]
fn lsp_prepare_rename_and_rename_return_workspace_edits() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-rename.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(
        uri,
        "var value := 1\nfunc test():\n    value += 1\n    print(value)\n",
    ));
    requests.push('\n');
    requests.push_str(
        "{\"id\":2,\"method\":\"textDocument/prepareRename\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-rename.gd\"},\"position\":{\"line\":3,\"character\":6}}}\n",
    );
    requests.push_str(
        "{\"id\":3,\"method\":\"textDocument/rename\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-rename.gd\"},\"position\":{\"line\":3,\"character\":6},\"newName\":\"count\"}}\n",
    );
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let prepare = response_by_id(&responses, 2).expect("prepareRename response");
    let rename = response_by_id(&responses, 3).expect("rename response");

    assert_eq!(
        prepare["result"]["placeholder"].as_str(),
        Some("value"),
        "prepareRename placeholder should be symbol under cursor: {prepare:#?}"
    );
    assert_eq!(
        prepare["result"]["range"]["start"]["line"].as_u64(),
        Some(3),
        "prepareRename start line should match target symbol: {prepare:#?}"
    );
    assert_eq!(
        prepare["result"]["range"]["start"]["character"].as_u64(),
        Some(5),
        "prepareRename start character should match symbol start: {prepare:#?}"
    );

    let edits = rename["result"]["changes"][uri]
        .as_array()
        .expect("rename edits");
    assert_eq!(edits.len(), 3, "rename should edit all symbol occurrences");
    assert!(
        edits.iter().all(|edit| edit["newText"] == "count"),
        "rename edits should carry requested symbol: {rename:#?}"
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
fn lsp_definition_and_references_resolve_across_open_documents() {
    let producer = fixture_text("lsp", "cross-file", "a.gd");
    let consumer = fixture_text("lsp", "cross-file", "b.gd");
    let producer_uri = "file:///tmp/fixtures/gdscript-lsp-cross-file-a.gd";
    let consumer_uri = "file:///tmp/fixtures/gdscript-lsp-cross-file-b.gd";

    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(producer_uri, &producer));
    requests.push('\n');
    requests.push_str(&did_open_message(consumer_uri, &consumer));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/definition\",\"params\":{{\"textDocument\":{{\"uri\":\"{consumer_uri}\"}},\"position\":{{\"line\":2,\"character\":21}}}}}}\n"
    ));
    requests.push_str(&format!(
        "{{\"id\":3,\"method\":\"textDocument/references\",\"params\":{{\"textDocument\":{{\"uri\":\"{consumer_uri}\"}},\"position\":{{\"line\":2,\"character\":21}}}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let definition = response_by_id(&responses, 2).expect("definition response");
    let references = response_by_id(&responses, 3).expect("references response");
    let definition_locations = definition["result"]
        .as_array()
        .expect("definition locations");
    let reference_locations = references["result"]
        .as_array()
        .expect("reference locations");
    let reference_uris = reference_locations
        .iter()
        .filter_map(|location| location["uri"].as_str().map(ToString::to_string))
        .collect::<HashSet<_>>();

    assert!(
        definition_locations
            .iter()
            .any(|location| location["uri"] == producer_uri),
        "definition should resolve to producer document: {definition:#?}"
    );
    assert!(
        reference_uris.contains(producer_uri),
        "references should include producer declaration: {references:#?}"
    );
    assert!(
        reference_uris.contains(consumer_uri),
        "references should include consumer call sites: {references:#?}"
    );
    assert!(
        reference_locations.len() >= 3,
        "references should include declaration and call sites: {references:#?}"
    );
}

#[test]
fn lsp_rename_applies_workspace_edits_across_files() {
    let producer = fixture_text("lsp", "cross-file", "a.gd");
    let consumer = fixture_text("lsp", "cross-file", "b.gd");
    let producer_uri = "file:///tmp/fixtures/gdscript-lsp-cross-file-rename-a.gd";
    let consumer_uri = "file:///tmp/fixtures/gdscript-lsp-cross-file-rename-b.gd";

    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(producer_uri, &producer));
    requests.push('\n');
    requests.push_str(&did_open_message(consumer_uri, &consumer));
    requests.push('\n');
    requests.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/rename\",\"params\":{{\"textDocument\":{{\"uri\":\"{consumer_uri}\"}},\"position\":{{\"line\":2,\"character\":21}},\"newName\":\"compute_value\"}}}}\n"
    ));
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let rename = response_by_id(&responses, 2).expect("rename response");
    let producer_edits = rename["result"]["changes"][producer_uri]
        .as_array()
        .expect("producer edits");
    let consumer_edits = rename["result"]["changes"][consumer_uri]
        .as_array()
        .expect("consumer edits");

    assert_eq!(
        producer_edits.len(),
        1,
        "rename should update producer declaration once: {rename:#?}"
    );
    assert_eq!(
        consumer_edits.len(),
        2,
        "rename should update both consumer call sites: {rename:#?}"
    );
    assert!(
        producer_edits
            .iter()
            .chain(consumer_edits.iter())
            .all(|edit| edit["newText"] == "compute_value"),
        "rename edits should use requested symbol: {rename:#?}"
    );
}

#[test]
fn code_action_resolve_hydrates_lazy_declaration_context_edit() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-code-action-resolve.gd";
    let source = fixture_text("lsp", "definition-references", "input.gd");

    let mut first_pass = String::new();
    first_pass.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    first_pass.push_str(&did_open_message(uri, &source));
    first_pass.push('\n');
    first_pass.push_str(&format!(
        "{{\"id\":2,\"method\":\"textDocument/codeAction\",\"params\":{{\"textDocument\":{{\"uri\":\"{uri}\"}},\"range\":{{\"start\":{{\"line\":5,\"character\":6}},\"end\":{{\"line\":5,\"character\":6}}}}}}}}\n"
    ));
    first_pass.push_str("{\"method\":\"exit\"}\n");

    let first_responses = run_lsp_responses(&first_pass);
    let unresolved = response_by_id(&first_responses, 2).expect("codeAction response")["result"]
        .as_array()
        .expect("actions")
        .iter()
        .find(|action| {
            action["title"]
                .as_str()
                .is_some_and(|title| title.contains("Show declaration context"))
        })
        .cloned()
        .expect("lazy declaration action");

    assert!(
        unresolved.get("edit").is_none(),
        "declaration action should be lazily resolved: {unresolved:#?}"
    );

    let resolve_request = serde_json::to_string(&json!({
        "id": 3,
        "method": "codeAction/resolve",
        "params": unresolved
    }))
    .expect("resolve request");

    let mut second_pass = String::new();
    second_pass.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    second_pass.push_str(&did_open_message(uri, &source));
    second_pass.push('\n');
    second_pass.push_str(&resolve_request);
    second_pass.push('\n');
    second_pass.push_str("{\"method\":\"exit\"}\n");

    let second_responses = run_lsp_responses(&second_pass);
    let resolved = response_by_id(&second_responses, 3).expect("resolve response");
    let replacement = resolved["result"]["edit"]["changes"][uri][0]["newText"]
        .as_str()
        .unwrap_or("");
    assert!(
        replacement.contains("func define_value():"),
        "resolved action should carry declaration line edit: {resolved:#?}"
    );
}

#[test]
fn initialize_indexes_workspace_root_for_cross_file_definition() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("gdscript-lsp-workspace-index-{stamp}"));
    fs::create_dir_all(&root).expect("create workspace root");

    let producer_path = root.join("producer.gd");
    let consumer_path = root.join("consumer.gd");
    fs::write(
        &producer_path,
        "func shared_value() -> int:\n    return 7\n",
    )
    .expect("write producer");
    fs::write(
        &consumer_path,
        "func use_value() -> void:\n    var local := shared_value()\n",
    )
    .expect("write consumer");

    let root_uri = file_uri(&root);
    let producer_uri = file_uri(&producer_path);
    let consumer_uri = file_uri(&consumer_path);
    let init = serde_json::to_string(&json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "rootUri": root_uri
        }
    }))
    .expect("init request");

    let mut requests = String::new();
    requests.push_str(&init);
    requests.push('\n');
    requests.push_str(
        &serde_json::to_string(&json!({
            "id": 2,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": consumer_uri },
                "position": { "line": 2, "character": 21 }
            }
        }))
        .expect("definition request"),
    );
    requests.push('\n');
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let definition = response_by_id(&responses, 2).expect("definition response");
    let locations = definition["result"]
        .as_array()
        .expect("definition locations");
    assert!(
        locations
            .iter()
            .any(|location| location["uri"] == producer_uri),
        "workspace indexing should resolve producer definition: {definition:#?}"
    );

    let _ = fs::remove_dir_all(root);
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

#[test]
fn completion_includes_godot4_keywords() {
    let source = "func _ready() -> void:\n    wh\n";
    let uri = "file:///tmp/fixtures/gdscript-lsp-keyword-completion.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-keyword-completion.gd"},"position":{"line":1,"character":6}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let completion = response_by_id(&responses, 2).expect("completion response");
    let labels: HashSet<String> = completion["result"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["label"].as_str().map(ToString::to_string))
        .collect();

    assert!(
        labels.contains("while"),
        "completion should include while keyword: {completion:#?}"
    );
    assert!(
        labels.contains("when"),
        "completion should include when keyword: {completion:#?}"
    );
}

#[test]
fn completion_on_typed_receiver_includes_method_docs_entries() {
    let source = "func _ready():\n    var _rng: RandomNumberGenerator = RandomNumberGenerator.new()\n    _rng.ra\n";
    let uri = "file:///tmp/fixtures/gdscript-lsp-method-completion.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-method-completion.gd"},"position":{"line":2,"character":11}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let completion = response_by_id(&responses, 2).expect("completion response");
    let items = completion["result"]["items"]
        .as_array()
        .expect("completion items");

    assert!(
        items.iter().any(|item| item["label"] == "randomize"
            && item["detail"]
                .as_str()
                .is_some_and(|d| d.contains("RandomNumberGenerator"))),
        "typed receiver completion should include randomize docs entry: {completion:#?}"
    );
}

#[test]
fn completion_on_typed_receiver_uses_snippet_insert_for_method_args() {
    let source = "func _ready():\n    var _rng: RandomNumberGenerator = RandomNumberGenerator.new()\n    _rng.randi_\n";
    let uri = "file:///tmp/fixtures/gdscript-lsp-method-snippet-completion.gd";

    let mut requests = String::new();
    requests.push_str(&frame_message(r#"{"id":1,"method":"initialize"}"#));
    requests.push_str(&frame_message(&did_open_message(uri, source)));
    requests.push_str(&frame_message(
        r#"{"id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///tmp/fixtures/gdscript-lsp-method-snippet-completion.gd"},"position":{"line":2,"character":14}}}"#,
    ));
    requests.push_str(&frame_message(r#"{"method":"exit"}"#));

    let responses = parse_framed_output(&run_lsp(&requests));
    let completion = response_by_id(&responses, 2).expect("completion response");
    let item = completion["result"]["items"]
        .as_array()
        .expect("completion items")
        .iter()
        .find(|entry| entry["label"] == "randi_range")
        .cloned()
        .expect("randi_range completion");

    assert_eq!(
        item["insertTextFormat"].as_u64(),
        Some(2),
        "method completion should return snippet insert format: {item:#?}"
    );
    assert!(
        item["insertText"]
            .as_str()
            .is_some_and(|insert| insert.contains("${1:") && insert.contains("${2:")),
        "method completion should include parameter placeholders: {item:#?}"
    );
}

#[test]
fn signature_help_on_typed_receiver_uses_docs_and_active_parameter() {
    let source = "func _ready():\n    var rng: RandomNumberGenerator = RandomNumberGenerator.new()\n    rng.randi_range(1, 2)\n";
    let uri = "file:///tmp/fixtures/gdscript-lsp-signaturehelp-typed.gd";

    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(uri, source));
    requests.push('\n');
    requests.push_str("{\"id\":2,\"method\":\"textDocument/signatureHelp\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-signaturehelp-typed.gd\"},\"position\":{\"line\":3,\"character\":23}}}\n");
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let signature_help = response_by_id(&responses, 2).expect("signatureHelp response");
    let signatures = signature_help["result"]["signatures"]
        .as_array()
        .expect("signatures");

    assert!(
        signatures.iter().any(|signature| {
            signature["label"]
                .as_str()
                .is_some_and(|label| label.contains("randi_range("))
        }),
        "typed receiver signature help should use docs metadata signatures: {signature_help:#?}"
    );
    assert_eq!(
        signature_help["result"]["activeParameter"].as_u64(),
        Some(1),
        "signature help should track active parameter index from call context: {signature_help:#?}"
    );
}

#[test]
fn initialize_completion_includes_project_class_and_autoload_symbols() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("gdscript-lsp-project-symbols-{stamp}"));
    fs::create_dir_all(&root).expect("create workspace root");

    fs::write(
        root.join("project.godot"),
        "[autoload]\nGameState=\"*res://autoload/game_state.gd\"\n",
    )
    .expect("write project.godot");
    fs::write(
        root.join("typed.gd"),
        "class_name CombatDirector\nextends Node\n",
    )
    .expect("write class file");
    fs::write(
        root.join("consumer.gd"),
        "func _ready() -> void:\n    Ga\n    Co\n",
    )
    .expect("write consumer file");

    let root_uri = file_uri(&root);
    let consumer_uri = file_uri(&root.join("consumer.gd"));
    let init = serde_json::to_string(&json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "rootUri": root_uri
        }
    }))
    .expect("init request");

    let completion_autoload = serde_json::to_string(&json!({
        "id": 2,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": consumer_uri },
            "position": { "line": 2, "character": 7 }
        }
    }))
    .expect("autoload completion request");

    let completion_class = serde_json::to_string(&json!({
        "id": 3,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": consumer_uri },
            "position": { "line": 3, "character": 7 }
        }
    }))
    .expect("class completion request");

    let mut requests = String::new();
    requests.push_str(&init);
    requests.push('\n');
    requests.push_str(&completion_autoload);
    requests.push('\n');
    requests.push_str(&completion_class);
    requests.push('\n');
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let autoload_completion = response_by_id(&responses, 2).expect("autoload completion response");
    let class_completion = response_by_id(&responses, 3).expect("class completion response");
    let autoload_labels = autoload_completion["result"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["label"].as_str().map(ToString::to_string))
        .collect::<HashSet<_>>();
    let class_labels = class_completion["result"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["label"].as_str().map(ToString::to_string))
        .collect::<HashSet<_>>();

    assert!(
        autoload_labels.contains("GameState"),
        "completion should include autoload symbols from project.godot: {autoload_completion:#?}"
    );
    assert!(
        class_labels.contains("CombatDirector"),
        "completion should include class_name symbols from workspace: {class_completion:#?}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn rename_rejects_conflicts_with_builtins() {
    let uri = "file:///tmp/fixtures/gdscript-lsp-rename-conflict.gd";
    let mut requests = String::new();
    requests.push_str("{\"id\":1,\"method\":\"initialize\"}\n");
    requests.push_str(&did_open_message(
        uri,
        "var value := 1\nfunc test() -> void:\n    value += 1\n",
    ));
    requests.push('\n');
    requests.push_str(
        "{\"id\":2,\"method\":\"textDocument/rename\",\"params\":{\"textDocument\":{\"uri\":\"file:///tmp/fixtures/gdscript-lsp-rename-conflict.gd\"},\"position\":{\"line\":3,\"character\":6},\"newName\":\"print\"}}\n",
    );
    requests.push_str("{\"method\":\"exit\"}\n");

    let responses = run_lsp_responses(&requests);
    let rename = response_by_id(&responses, 2).expect("rename response");
    assert_eq!(
        rename["error"]["code"].as_i64(),
        Some(-32602),
        "rename should reject conflicts with builtins: {rename:#?}"
    );
}
