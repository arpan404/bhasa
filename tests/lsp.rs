mod common;

use bhasa::json::{parse, Value};
use common::Workspace;
use std::fs;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

const URI: &str = "untitled:bhasa-lsp-test.bh";

fn request(id: i64, method: &str, params: Value) -> Value {
    Value::object([
        ("jsonrpc", "2.0".into()),
        ("id", id.into()),
        ("method", method.into()),
        ("params", params),
    ])
}

fn notification(method: &str, params: Value) -> Value {
    Value::object([
        ("jsonrpc", "2.0".into()),
        ("method", method.into()),
        ("params", params),
    ])
}

fn initialize() -> Value {
    request(
        1,
        "initialize",
        Value::object([("capabilities", Value::object([]))]),
    )
}

fn start() -> Vec<Value> {
    vec![initialize(), notification("initialized", Value::object([]))]
}

fn finish(messages: &mut Vec<Value>) {
    messages.push(request(999, "shutdown", Value::Null));
    messages.push(notification("exit", Value::Null));
}

fn document(uri: &str) -> Value {
    Value::object([("uri", uri.into())])
}

fn document_params(uri: &str) -> Value {
    Value::object([("textDocument", document(uri))])
}

fn open(uri: &str, source: &str, version: i64) -> Value {
    notification(
        "textDocument/didOpen",
        Value::object([(
            "textDocument",
            Value::object([
                ("uri", uri.into()),
                ("languageId", "bhasa".into()),
                ("version", version.into()),
                ("text", source.into()),
            ]),
        )]),
    )
}

fn change(uri: &str, source: &str, version: i64) -> Value {
    changes(uri, version, vec![Value::object([("text", source.into())])])
}

fn changes(uri: &str, version: i64, changes: Vec<Value>) -> Value {
    notification(
        "textDocument/didChange",
        Value::object([
            (
                "textDocument",
                Value::object([("uri", uri.into()), ("version", version.into())]),
            ),
            ("contentChanges", Value::Array(changes)),
        ]),
    )
}

fn position(line: usize, character: usize) -> Value {
    Value::object([("line", line.into()), ("character", character.into())])
}

fn definition(id: i64, uri: &str, line: usize, character: usize) -> Value {
    request(
        id,
        "textDocument/definition",
        Value::object([
            ("textDocument", document(uri)),
            ("position", position(line, character)),
        ]),
    )
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn encode(messages: &[Value]) -> Vec<u8> {
    messages
        .iter()
        .flat_map(|message| frame(message.to_string().as_bytes()))
        .collect()
}

// Decode independently of the server so stray stdout and incorrect byte lengths fail.
fn decode(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        let end = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .expect("protocol stdout must contain complete CRLF headers");
        let header = std::str::from_utf8(&bytes[..end]).unwrap();
        let length = header
            .strip_prefix("Content-Length: ")
            .expect("unexpected stdout before Content-Length")
            .parse::<usize>()
            .expect("invalid response Content-Length");
        bytes = &bytes[end + 4..];
        assert!(bytes.len() >= length, "truncated response body");
        let body = std::str::from_utf8(&bytes[..length]).expect("response must be UTF-8");
        messages.push(parse(body).expect("response must be valid JSON"));
        bytes = &bytes[length..];
    }
    messages
}

fn exchange_bytes(input: &[u8]) -> (i32, Vec<Value>) {
    // Two-byte buffering splits headers, JSON escapes, and UTF-8 code points.
    let reader = BufReader::with_capacity(2, Cursor::new(input));
    let mut output = Vec::new();
    let status = bhasa::lsp::serve(reader, &mut output);
    (status, decode(&output))
}

fn exchange(mut messages: Vec<Value>) -> Vec<Value> {
    finish(&mut messages);
    let (status, output) = exchange_bytes(&encode(&messages));
    assert_eq!(status, 0, "session did not shut down normally");
    output
}

// Save between protocol frames, after the unsaved-change diagnostic was sent.
struct SaveBetweenFrames {
    before: Cursor<Vec<u8>>,
    after: Cursor<Vec<u8>>,
    save: Option<(PathBuf, String)>,
}
impl BufRead for SaveBetweenFrames {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.before.position() < self.before.get_ref().len() as u64 {
            return self.before.fill_buf();
        }
        if let Some((path, source)) = self.save.take() {
            fs::write(path, source)?;
        }
        self.after.fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        if self.before.position() < self.before.get_ref().len() as u64 {
            self.before.consume(amount);
        } else {
            self.after.consume(amount);
        }
    }
}
impl Read for SaveBetweenFrames {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(output.len());
        output[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

fn response(messages: &[Value], id: i64) -> &Value {
    messages
        .iter()
        .find(|message| message.get("id").and_then(Value::as_i64) == Some(id))
        .unwrap_or_else(|| panic!("missing response {id}: {messages:?}"))
}

fn result(messages: &[Value], id: i64) -> &Value {
    response(messages, id)
        .get("result")
        .unwrap_or_else(|| panic!("request {id} failed: {:?}", response(messages, id)))
}

fn error_code(message: &Value) -> i64 {
    message
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("expected error response: {message:?}"))
}

fn publications<'a>(messages: &'a [Value], uri: &str) -> Vec<&'a Value> {
    messages
        .iter()
        .filter(|message| {
            message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
        })
        .filter_map(|message| message.get("params"))
        .filter(|params| params.get("uri").and_then(Value::as_str) == Some(uri))
        .collect()
}

fn diagnostics(publication: &Value) -> &[Value] {
    publication
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("diagnostics array")
}

fn file_uri(path: &Path) -> String {
    path_uri(&fs::canonicalize(path).unwrap())
}

fn path_uri(path: &Path) -> String {
    let mut result = "file://".to_owned();
    for byte in path.to_str().unwrap().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            result.push(char::from(byte));
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

#[test]
fn initialize_advertises_only_implemented_capabilities_and_handles_multiple_frames() {
    let mut messages = start();
    messages.push(request(2, "workspace/unknown", Value::Null));
    let output = exchange(messages);
    let capabilities = result(&output, 1).get("capabilities").unwrap();
    assert_eq!(
        capabilities.get("positionEncoding").and_then(Value::as_str),
        Some("utf-16")
    );
    assert_eq!(
        capabilities
            .get("textDocumentSync")
            .unwrap()
            .get("change")
            .and_then(Value::as_i64),
        Some(1)
    );
    for name in [
        "documentFormattingProvider",
        "documentSymbolProvider",
        "definitionProvider",
    ] {
        assert_eq!(capabilities.get(name), Some(&Value::Bool(true)));
    }
    assert!(capabilities.get("completionProvider").is_some());
    assert_eq!(capabilities.get("hoverProvider"), Some(&Value::Bool(true)));
    assert_eq!(error_code(response(&output, 2)), -32601);
    assert_eq!(result(&output, 999), &Value::Null);
    assert_eq!(output.len(), 3, "notifications must not receive responses");
}

#[test]
fn initialization_and_shutdown_lifecycle_have_defined_exit_statuses() {
    let cases = [
        (vec![], 1),
        (vec![notification("exit", Value::Null)], 1),
        (vec![initialize()], 1),
        (vec![initialize(), request(2, "shutdown", Value::Null)], 1),
        (
            vec![
                initialize(),
                request(2, "shutdown", Value::Null),
                notification("exit", Value::Null),
            ],
            0,
        ),
    ];
    for (messages, expected) in cases {
        assert_eq!(exchange_bytes(&encode(&messages)).0, expected);
    }
    let output = exchange(vec![
        request(2, "textDocument/documentSymbol", document_params(URI)),
        initialize(),
        request(3, "initialize", Value::Null),
        request(4, "shutdown", Value::Null),
        request(5, "textDocument/documentSymbol", document_params(URI)),
    ]);
    assert_eq!(error_code(response(&output, 2)), -32002);
    assert_eq!(error_code(response(&output, 3)), -32600);
    assert_eq!(result(&output, 4), &Value::Null);
    assert_eq!(error_code(response(&output, 5)), -32600);
}

#[test]
fn malformed_json_and_invalid_utf8_return_parse_errors_and_recover() {
    let mut input = encode(&start());
    let deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    for body in [
        b"{".as_slice(),
        b"\xff",
        b"{\"x\":1,\"x\":2}",
        deep.as_bytes(),
    ] {
        input.extend(frame(body));
    }
    let mut tail = Vec::new();
    finish(&mut tail);
    input.extend(encode(&tail));
    let (status, output) = exchange_bytes(&input);
    assert_eq!(status, 0);
    assert_eq!(output.len(), 6);
    for error in &output[1..5] {
        assert_eq!(error.get("id"), Some(&Value::Null));
        assert_eq!(error_code(error), -32700);
    }
    assert_eq!(result(&output, 999), &Value::Null);
}

#[test]
fn invalid_requests_fail_without_losing_the_connection() {
    let mut input = encode(&start());
    for body in [
        "[]",
        "null",
        "{}",
        r#"{"jsonrpc":"1.0","id":2,"method":"x"}"#,
        r#"{"jsonrpc":"2.0","id":true,"method":"x"}"#,
        r#"{"jsonrpc":"2.0","id":1.5,"method":"x"}"#,
        r#"{"jsonrpc":"2.0","id":2147483648,"method":"x"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":5}"#,
    ] {
        input.extend(frame(body.as_bytes()));
    }
    let mut tail = vec![request(4, "textDocument/definition", Value::Null)];
    finish(&mut tail);
    input.extend(encode(&tail));
    let (status, output) = exchange_bytes(&input);
    assert_eq!(status, 0);
    assert_eq!(output.len(), 11);
    for error in &output[1..9] {
        assert_eq!(error_code(error), -32600);
    }
    assert_eq!(error_code(response(&output, 4)), -32602);
}

#[test]
fn string_ids_and_unicode_messages_use_utf8_byte_lengths() {
    let unicode_request = Value::object([
        ("jsonrpc", "2.0".into()),
        ("id", "request-🌍-ह".into()),
        ("method", "unknown-🌍".into()),
        ("params", Value::Null),
    ]);
    let mut messages = start();
    messages.push(unicode_request);
    let output = exchange(messages);
    let answer = output
        .iter()
        .find(|message| message.get("id").and_then(Value::as_str) == Some("request-🌍-ह"))
        .unwrap();
    assert_eq!(error_code(answer), -32601);
    assert!(answer
        .get("error")
        .unwrap()
        .get("message")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("unknown-🌍"));
}

#[test]
fn bad_framing_is_bounded_and_returns_failure_without_protocol_garbage() {
    let oversized_header = format!(
        "X-Padding: {}\r\nContent-Length: 0\r\n\r\n",
        "a".repeat(8192)
    );
    let frames: Vec<Vec<u8>> = vec![
        b"Content-Length: 2\n\n{}".to_vec(),
        b"Content-Length: 2\r\n".to_vec(),
        b"Content-Length: 20\r\n\r\n{}".to_vec(),
        b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
        b"Content-Type: application/vscode-jsonrpc\r\n\r\n{}".to_vec(),
        b"Content-Length: -2\r\n\r\n{}".to_vec(),
        b"Content-Length: 99999999999999999999999999\r\n\r\n".to_vec(),
        b"Content-Length: 4194305\r\n\r\n".to_vec(),
        b"Content-Length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-16\r\n\r\n{}"
            .to_vec(),
        b"Content-Length: 2\r\nX-Note: \xff\r\n\r\n{}".to_vec(),
        oversized_header.into_bytes(),
    ];
    for input in frames {
        let (status, output) = exchange_bytes(&input);
        assert_eq!(
            status,
            1,
            "accepted invalid frame {:?}",
            String::from_utf8_lossy(&input[..input.len().min(100)])
        );
        assert!(
            output.is_empty(),
            "framing errors must not print text to stdout"
        );
    }
}

#[test]
fn header_names_are_case_insensitive_and_utf8_content_type_is_accepted() {
    let body = initialize().to_string();
    let mut input = format!("content-length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\nX-Client: tests\r\n\r\n{}", body.len(), body).into_bytes();
    let mut tail = Vec::new();
    finish(&mut tail);
    input.extend(encode(&tail));
    let (status, output) = exchange_bytes(&input);
    assert_eq!(status, 0);
    assert!(result(&output, 1).get("capabilities").is_some());
}

#[test]
fn open_change_and_close_publish_utf16_diagnostics_and_clear_them() {
    let source = "def main() { print(\"🌍\"); print(missing) }\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(change(URI, "def main() { print(42) }\r\n", 2));
    messages.push(notification("textDocument/didClose", document_params(URI)));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    let publications = publications(&output, URI);
    assert_eq!(publications.len(), 3);
    let first = diagnostics(publications[0]);
    let missing = first
        .iter()
        .find(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap()
                .contains("missing")
        })
        .unwrap();
    let start = missing.get("range").unwrap().get("start").unwrap();
    assert_eq!(
        start,
        &position(
            0,
            source[..source.find("missing").unwrap()]
                .encode_utf16()
                .count()
        )
    );
    assert_eq!(
        publications[0].get("version").and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        publications[1].get("version").and_then(Value::as_i64),
        Some(2)
    );
    assert!(diagnostics(publications[1]).is_empty());
    assert!(diagnostics(publications[2]).is_empty());
    assert_eq!(error_code(response(&output, 2)), -32602);
}

#[test]
fn stale_ranged_and_invalid_changes_do_not_replace_open_source() {
    let original = "def original() -> int { return 1 }\ndef main() { print(original()) }\n";
    let invalid = "def replacement() { print(missing) }\n";
    let mut messages = start();
    messages.push(open(URI, original, 3));
    messages.push(change(URI, invalid, 2));
    messages.push(change(URI, invalid, 3));
    messages.push(changes(
        URI,
        4,
        vec![Value::object([
            ("text", invalid.into()),
            (
                "range",
                Value::object([("start", position(0, 0)), ("end", position(0, 0))]),
            ),
        ])],
    ));
    messages.push(changes(
        URI,
        5,
        vec![Value::object([("text", true.into())])],
    ));
    messages.push(changes(URI, 6, Vec::new()));
    messages.push(change("untitled:unknown.bh", invalid, 7));
    messages.push(open(URI, invalid, 8));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    let publications = publications(&output, URI);
    assert_eq!(
        publications.len(),
        1,
        "ignored updates must not trigger new diagnostics"
    );
    assert!(diagnostics(publications[0]).is_empty());
    let symbols = result(&output, 2).as_array().unwrap();
    let names: Vec<_> = symbols
        .iter()
        .map(|symbol| symbol.get("name").and_then(Value::as_str).unwrap())
        .collect();
    assert_eq!(names, ["original", "main"]);
}

#[test]
fn full_change_batches_apply_the_last_document_in_order() {
    let mut messages = start();
    messages.push(open(URI, "def main() {}", 1));
    messages.push(changes(
        URI,
        2,
        vec![
            Value::object([("text", "def main() { print(missing) }".into())]),
            Value::object([("text", "def main() { print(42) }".into())]),
        ],
    ));
    let output = exchange(messages);
    let updates = publications(&output, URI);
    assert_eq!(updates.len(), 2);
    assert!(diagnostics(updates[1]).is_empty());
    assert_eq!(updates[1].get("version").and_then(Value::as_i64), Some(2));
}

#[test]
fn formatting_returns_a_valid_whole_document_edit_with_utf16_end() {
    let source = "def main(){print(\"🌍\");print(42)}\r\n# café 🌍";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/formatting",
        Value::object([
            ("textDocument", document(URI)),
            (
                "options",
                Value::object([("tabSize", 4_i64.into()), ("insertSpaces", true.into())]),
            ),
        ]),
    ));
    let output = exchange(messages);
    let edits = result(&output, 2).as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0].get("range"),
        Some(&Value::object([
            ("start", position(0, 0)),
            ("end", position(1, "# café 🌍".encode_utf16().count())),
        ]))
    );
    let formatted = edits[0].get("newText").and_then(Value::as_str).unwrap();
    assert_ne!(formatted, source);
    assert!(
        bhasa::check(formatted).is_ok(),
        "formatted source is invalid: {formatted}"
    );
    assert!(formatted.contains("# café 🌍"));
    assert_eq!(bhasa::format::format_source(formatted).unwrap(), formatted);
    assert_eq!(
        bhasa::compile_to_llvm(source, "x86_64-unknown-linux-gnu"),
        bhasa::compile_to_llvm(formatted, "x86_64-unknown-linux-gnu")
    );
}

#[test]
fn already_formatted_documents_need_no_edit_and_bad_source_reports_failure() {
    let formatted = bhasa::format::format_source("def main(){print(42)}").unwrap();
    let mut messages = start();
    messages.push(open(URI, &formatted, 1));
    messages.push(request(2, "textDocument/formatting", document_params(URI)));
    messages.push(change(URI, "def main() { print(\"bad\\q\") }", 2));
    messages.push(request(3, "textDocument/formatting", document_params(URI)));
    let output = exchange(messages);
    assert!(result(&output, 2).as_array().unwrap().is_empty());
    assert_eq!(error_code(response(&output, 3)), -32803);
}

#[test]
fn symbols_and_definition_ranges_use_utf16_on_crlf_source() {
    let source = "# 🌍\r\ndef answer() -> int { return 42 }\r\ndef main() { print(\"🌍\"); print(answer()) }\r\n";
    let call_line = source.lines().nth(2).unwrap();
    let call_character = call_line[..call_line.find("answer").unwrap()]
        .encode_utf16()
        .count();
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    messages.push(definition(3, URI, 2, call_character + 2));
    let output = exchange(messages);
    let symbols = result(&output, 2).as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    let answer = &symbols[0];
    assert_eq!(answer.get("name").and_then(Value::as_str), Some("answer"));
    assert_eq!(answer.get("kind").and_then(Value::as_i64), Some(12));
    let expected = Value::object([("start", position(1, 4)), ("end", position(1, 10))]);
    assert_eq!(answer.get("selectionRange"), Some(&expected));
    assert_eq!(
        result(&output, 3).get("uri").and_then(Value::as_str),
        Some(URI)
    );
    assert_eq!(result(&output, 3).get("range"), Some(&expected));
}

#[test]
fn definition_ignores_strings_comments_and_resolves_local_names() {
    let source = "def answer() -> int { return 42 }\ndef main() {\n    print(\"answer()\")\n    # answer()\n    var answer = 3\n    print(answer)\n}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(definition(2, URI, 2, 13));
    messages.push(definition(3, URI, 3, 8));
    messages.push(definition(4, URI, 4, 9));
    messages.push(definition(5, URI, 5, 12));
    let output = exchange(messages);
    for id in 2..=3 {
        assert_eq!(
            result(&output, id),
            &Value::Null,
            "definition linked non-code at request {id}"
        );
    }
    for id in [4, 5] {
        assert_selection(result(&output, id), URI, source, "var answer", "answer");
    }
}

#[test]
fn invalid_utf16_positions_fail_safely_including_surrogate_halves() {
    let line = "def main() { print(\"🌍\") }";
    let emoji = line[..line.find('🌍').unwrap()].encode_utf16().count();
    let mut messages = start();
    messages.push(open(URI, &format!("{line}\r\n"), 1));
    messages.push(definition(2, URI, 0, emoji + 1));
    messages.push(definition(3, URI, 9, 0));
    messages.push(definition(4, URI, 0, 999));
    messages.push(request(
        5,
        "textDocument/definition",
        Value::object([
            ("textDocument", document(URI)),
            (
                "position",
                Value::object([("line", (-1_i64).into()), ("character", 0_i64.into())]),
            ),
        ]),
    ));
    let output = exchange(messages);
    for id in [2, 3, 5] {
        assert_eq!(error_code(response(&output, id)), -32602);
    }
    // LSP Position clamps character offsets beyond the line's length.
    assert_eq!(result(&output, 4), &Value::Null);
}

#[test]
fn unsaved_source_overrides_disk_and_formatting_never_writes_files() {
    let workspace = Workspace::new("lsp unsaved 🌍");
    let saved = "def main() { print(stale_disk_name) }\n";
    let path = workspace.write("main file.bh", saved);
    let uri = file_uri(&path);
    let mut messages = start();
    messages.push(open(&uri, "def main(){print(42)}", 1));
    messages.push(request(2, "textDocument/formatting", document_params(&uri)));
    let output = exchange(messages);
    let updates = publications(&output, &uri);
    assert_eq!(updates.len(), 1);
    assert!(
        diagnostics(updates[0]).is_empty(),
        "disk source replaced the open document: {updates:?}"
    );
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
}

#[test]
fn imported_diagnostics_use_real_file_ranges_and_unsaved_entry_body() {
    let workspace = Workspace::new("lsp imports 🌍");
    let saved_main = "import \"math.bh\"\ndef main() { print(stale_entry) }\n";
    let main = workspace.write("main.bh", saved_main);
    let imported = "# module 🌍\r\ndef answer() -> int { print(\"🌍\"); return missing }\r\n";
    let math = workspace.write("math.bh", imported);
    let main_uri = file_uri(&main);
    let math_uri = file_uri(&math);
    let mut messages = start();
    messages.push(open(
        &main_uri,
        "import \"math.bh\"\ndef main() { print(answer()) }\n",
        1,
    ));
    messages.push(definition(2, &main_uri, 1, 20));
    messages.push(notification(
        "textDocument/didClose",
        document_params(&main_uri),
    ));
    let output = exchange(messages);
    let entry_updates = publications(&output, &main_uri);
    assert!(
        entry_updates
            .iter()
            .all(|update| diagnostics(update).is_empty()),
        "unsaved entry was not overlaid: {entry_updates:?}"
    );
    let imported_updates = publications(&output, &math_uri);
    assert_eq!(
        imported_updates.len(),
        2,
        "imported diagnostics were not published and cleared: {output:?}"
    );
    let errors = diagnostics(imported_updates[0]);
    let error = errors
        .iter()
        .find(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap()
                .contains("missing")
        })
        .unwrap();
    let line = imported.lines().nth(1).unwrap();
    let column = line[..line.find("missing").unwrap()].encode_utf16().count();
    assert_eq!(
        error.get("range").unwrap().get("start"),
        Some(&position(1, column))
    );
    assert!(diagnostics(imported_updates[1]).is_empty());
    assert_eq!(
        result(&output, 2).get("uri").and_then(Value::as_str),
        Some(math_uri.as_str())
    );
    assert_eq!(
        result(&output, 2).get("range").unwrap().get("start"),
        Some(&position(1, 4))
    );
    assert_eq!(fs::read_to_string(main).unwrap(), saved_main);
    assert_eq!(fs::read_to_string(math).unwrap(), imported);
}

#[test]
fn open_imported_document_overlays_saved_import_graph_and_rechecks_dependents() {
    let workspace = Workspace::new("lsp import overlay");
    let main_source = "import \"math.bh\"\ndef main() { print(answer()) }\n";
    let main = workspace.write("main.bh", main_source);
    let saved_math = "def answer() -> int { return missing }\n";
    let math = workspace.write("math.bh", saved_math);
    let main_uri = file_uri(&main);
    let math_uri = file_uri(&math);
    let mut messages = start();
    messages.push(open(&main_uri, main_source, 1));
    messages.push(open(&math_uri, "def answer() -> int { return 42 }\n", 1));
    let output = exchange(messages);
    let updates = publications(&output, &math_uri);
    assert!(!diagnostics(updates[0]).is_empty());
    assert!(
        diagnostics(updates.last().unwrap()).is_empty(),
        "saved module overrode its open buffer: {updates:?}"
    );
    let entry_updates = publications(&output, &main_uri);
    assert!(diagnostics(entry_updates.last().unwrap()).is_empty());
    assert_eq!(fs::read_to_string(math).unwrap(), saved_math);
}

#[test]
fn open_imported_module_can_call_globals_supplied_by_its_importer() {
    let workspace = Workspace::new("lsp importer globals");
    let main_source =
        "import \"math.bh\"\ndef helper() -> int { return 21 }\ndef main() { print(answer()) }\n";
    let module_source = "def answer() -> int { return helper() * 2 }\n";
    let main = workspace.write("main.bh", main_source);
    let math = workspace.write("math.bh", module_source);
    let main_uri = file_uri(&main);
    let math_uri = file_uri(&math);
    let mut messages = start();
    messages.push(open(&main_uri, main_source, 1));
    messages.push(open(&math_uri, module_source, 1));
    let output = exchange(messages);
    for uri in [&main_uri, &math_uri] {
        let updates = publications(&output, uri);
        assert!(!updates.is_empty());
        assert!(
            updates.iter().all(|update| diagnostics(update).is_empty()),
            "valid project globals were rejected for {uri}: {updates:?}"
        );
    }
}

#[test]
fn changed_unsaved_imports_warn_until_saved_without_false_semantic_errors() {
    let workspace = Workspace::new("lsp changed imports");
    let saved = "import \"math.bh\"\ndef main() { print(answer()) }\n";
    let main = workspace.write("main.bh", saved);
    workspace.write("math.bh", "def answer() -> int { return 42 }\n");
    workspace.write("other.bh", "def other() -> int { return 21 }\n");
    let uri = file_uri(&main);
    let mut messages = start();
    messages.push(open(&uri, saved, 1));
    messages.push(change(
        &uri,
        "import \"other.bh\"\ndef main() { print(other()) }\n",
        2,
    ));
    let output = exchange(messages);
    let updates = publications(&output, &uri);
    assert!(diagnostics(updates[0]).is_empty());
    let warnings = diagnostics(updates.last().unwrap());
    assert_eq!(
        warnings.len(),
        1,
        "unexpected semantic errors for an unsaved module graph: {warnings:?}"
    );
    assert_eq!(
        warnings[0].get("code").and_then(Value::as_str),
        Some("LSP001")
    );
    assert_eq!(warnings[0].get("severity").and_then(Value::as_i64), Some(2));
    assert!(warnings[0]
        .get("message")
        .and_then(Value::as_str)
        .unwrap()
        .contains("Save"));
    assert_eq!(fs::read_to_string(main).unwrap(), saved);
}

#[test]
fn saving_changed_imports_refreshes_the_graph_and_clears_the_warning() {
    let workspace = Workspace::new("lsp save imports");
    let original = "import \"math.bh\"\ndef main() { print(answer()) }\n";
    let changed = "import \"other.bh\"\ndef main() { print(other()) }\n";
    let main = workspace.write("main.bh", original);
    workspace.write("math.bh", "def answer() -> int { return 42 }\n");
    let other = workspace.write("other.bh", "def other() -> int { return 21 }\n");
    let uri = file_uri(&main);
    let other_uri = file_uri(&other);
    let mut before = start();
    before.push(open(&uri, original, 1));
    before.push(change(&uri, changed, 2));
    let mut after = vec![
        notification("textDocument/didSave", document_params(&uri)),
        definition(2, &uri, 1, 20),
    ];
    finish(&mut after);
    let reader = SaveBetweenFrames {
        before: Cursor::new(encode(&before)),
        after: Cursor::new(encode(&after)),
        save: Some((main.clone(), changed.to_owned())),
    };
    let mut bytes = Vec::new();
    assert_eq!(bhasa::lsp::serve(reader, &mut bytes), 0);
    let output = decode(&bytes);
    let updates = publications(&output, &uri);
    assert_eq!(updates.len(), 3);
    assert_eq!(
        diagnostics(updates[1])[0]
            .get("code")
            .and_then(Value::as_str),
        Some("LSP001")
    );
    assert!(diagnostics(updates[2]).is_empty());
    assert_eq!(
        result(&output, 2).get("uri").and_then(Value::as_str),
        Some(other_uri.as_str())
    );
    assert_eq!(fs::read_to_string(main).unwrap(), changed);
}

#[test]
#[cfg(unix)]
fn newly_saved_file_under_symlink_alias_keeps_its_current_buffer() {
    use std::os::unix::fs::symlink;

    let workspace = Workspace::new("lsp new symlink file");
    let actual = workspace.file("actual");
    let alias = workspace.file("alias");
    fs::create_dir(&actual).unwrap();
    symlink(&actual, &alias).unwrap();
    let path = actual.join("new.bh");
    let uri = path_uri(&alias.join("new.bh"));
    let original = "def original() -> int { return 1 }\ndef main() { print(original()) }\n";
    let saved = "def disk_only() -> int { return 2 }\ndef main() { print(disk_only()) }\n";
    let changed =
        "def live() -> int { return 42 }\ndef main() { print(live()); print(buffer_error) }\n";
    let repaired = "def live() -> int { return 42 }\ndef main() { print(live()) }\n";
    assert!(!path.exists());
    let mut before = start();
    before.push(open(&uri, original, 1));
    let mut after = vec![
        change(&uri, changed, 2),
        definition(2, &uri, 1, 20),
        change(&uri, repaired, 3),
        definition(3, &uri, 1, 20),
    ];
    finish(&mut after);
    let reader = SaveBetweenFrames {
        before: Cursor::new(encode(&before)),
        after: Cursor::new(encode(&after)),
        save: Some((path.clone(), saved.to_owned())),
    };
    let mut bytes = Vec::new();
    assert_eq!(bhasa::lsp::serve(reader, &mut bytes), 0);
    let output = decode(&bytes);
    let updates = publications(&output, &uri);
    assert_eq!(updates.len(), 3);
    assert!(diagnostics(updates[0]).is_empty());
    assert!(
        diagnostics(updates[1]).iter().any(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("buffer_error"))
        }),
        "saved text displaced the current buffer: {updates:?}"
    );
    assert!(diagnostics(updates[2]).is_empty());
    assert_eq!(updates[1].get("version").and_then(Value::as_i64), Some(2));
    assert_eq!(updates[2].get("version").and_then(Value::as_i64), Some(3));
    for id in [2, 3] {
        assert_eq!(
            result(&output, id).get("uri").and_then(Value::as_str),
            Some(uri.as_str())
        );
        assert_eq!(
            result(&output, id).get("range"),
            Some(&Value::object([
                ("start", position(0, 4)),
                ("end", position(0, 8)),
            ]))
        );
    }
    assert_eq!(fs::read_to_string(path).unwrap(), saved);
}

#[test]
fn formatting_response_over_four_mib_fails_without_losing_the_connection() {
    const MAX_MESSAGE: usize = 4 * 1024 * 1024;
    let source = format!(
        "def main(){{{}{}{}}}",
        "if true{".repeat(64),
        "print(0)\n".repeat(18_000),
        "}".repeat(64)
    );
    assert!(
        bhasa::check(&source).is_ok(),
        "large formatting fixture must be valid Bhasa"
    );
    assert!(
        bhasa::format::format_source(&source).unwrap().len() > MAX_MESSAGE,
        "fixture must expand beyond the protocol response limit"
    );
    let open = open(URI, &source, 1);
    assert!(
        open.to_string().len() < MAX_MESSAGE,
        "fixture must fit in a valid input message"
    );
    let mut messages = start();
    messages.push(open);
    messages.push(request(2, "textDocument/formatting", document_params(URI)));
    messages.push(change(URI, "def main(){print(42)}", 2));
    messages.push(request(
        3,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    messages.push(request(4, "textDocument/formatting", document_params(URI)));
    let output = exchange(messages);
    assert_eq!(
        response(&output, 2)
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_i64),
        Some(-32803),
        "oversized formatting result must return RequestFailed"
    );
    assert!(
        output
            .iter()
            .all(|message| message.to_string().len() <= MAX_MESSAGE),
        "all response bodies must fit the protocol message limit"
    );
    let symbols = result(&output, 3).as_array().unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].get("name").and_then(Value::as_str), Some("main"));
    assert_eq!(result(&output, 4).as_array().unwrap().len(), 1);
    assert_eq!(result(&output, 999), &Value::Null);
}

#[test]
fn oversized_diagnostics_are_bounded_with_an_omission_warning_and_session_recovers() {
    const MAX_MESSAGE: usize = 4 * 1024 * 1024;
    let function = "f".repeat(8192);
    let parameters = (0..600)
        .map(|index| format!("p{index}: int"))
        .collect::<Vec<_>>()
        .join(",");
    let arguments = vec!["false"; 600].join(",");
    let source =
        format!("def {function}({parameters}) {{}}\ndef main() {{ {function}({arguments}) }}\n");
    let opening = open(URI, &source, 1);
    assert!(opening.to_string().len() < MAX_MESSAGE);
    let mut messages = start();
    messages.push(opening);
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    assert!(
        output
            .iter()
            .all(|message| message.to_string().len() <= MAX_MESSAGE),
        "diagnostic notifications and responses must fit the protocol message limit"
    );
    let updates = publications(&output, URI);
    assert_eq!(updates.len(), 1);
    let errors = diagnostics(updates[0]);
    assert!(
        errors
            .iter()
            .any(|error| error.get("severity").and_then(Value::as_i64) == Some(1)),
        "the publication must preserve useful compiler diagnostics"
    );
    let omissions: Vec<_> = errors
        .iter()
        .filter(|error| error.get("code").and_then(Value::as_str) == Some("LSP002"))
        .collect();
    assert_eq!(
        omissions.len(),
        1,
        "truncated diagnostics need one omission notice"
    );
    assert_eq!(
        omissions[0].get("severity").and_then(Value::as_i64),
        Some(2)
    );
    assert!(omissions[0]
        .get("message")
        .and_then(Value::as_str)
        .unwrap()
        .starts_with("Additional diagnostics omitted"));
    let symbols = result(&output, 2).as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    assert!(symbols
        .iter()
        .any(|symbol| symbol.get("name").and_then(Value::as_str) == Some("main")));
    assert_eq!(result(&output, 999), &Value::Null);
}

#[test]
fn cli_stdio_route_runs_without_a_stdout_banner() {
    let workspace = Workspace::new("lsp cli");
    let mut messages = start();
    messages.push(open(URI, "def main() { print(\"🌍\") }", 1));
    finish(&mut messages);
    let input = encode(&messages);
    let mut child = workspace
        .command()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    common::succeeds(&output);
    let messages = decode(&output.stdout);
    assert!(result(&messages, 1).get("capabilities").is_some());
    assert_eq!(result(&messages, 999), &Value::Null);
    assert!(
        output.stderr.is_empty(),
        "successful session wrote stderr: {}",
        common::stderr(&output)
    );
}

fn source_position(source: &str, byte: usize) -> Value {
    let prefix = &source[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    position(line, source[line_start..byte].encode_utf16().count())
}

fn definition_at(id: i64, uri: &str, source: &str, needle: &str, name: &str) -> Value {
    let byte = source.find(needle).unwrap() + needle.find(name).unwrap();
    request(
        id,
        "textDocument/definition",
        Value::object([
            ("textDocument", document(uri)),
            ("position", source_position(source, byte)),
        ]),
    )
}

fn assert_selection(location: &Value, uri: &str, source: &str, declaration: &str, name: &str) {
    assert_eq!(location.get("uri").and_then(Value::as_str), Some(uri));
    let byte = source.find(declaration).unwrap() + declaration.find(name).unwrap();
    assert_eq!(
        location.get("range"),
        Some(&Value::object([
            ("start", source_position(source, byte)),
            ("end", source_position(source, byte + name.len())),
        ]))
    );
}

#[test]
fn record_symbols_preserve_field_hierarchy_source_order_and_utf16_ranges() {
    let source = "def before() { print(\"🌍\") } struct Box { label: buffer; point: Point }\r\nstruct Point { x: int; y: int }\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    let symbols = result(&output, 2).as_array().unwrap();
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.get("name").and_then(Value::as_str).unwrap())
            .collect::<Vec<_>>(),
        ["before", "Box", "Point"]
    );
    let record = &symbols[1];
    assert_eq!(record.get("kind").and_then(Value::as_i64), Some(23));
    let byte = source.find("Box").unwrap();
    assert_eq!(
        record.get("selectionRange").unwrap().get("start"),
        Some(&source_position(source, byte))
    );
    let fields = record.get("children").unwrap().as_array().unwrap();
    assert_eq!(fields.len(), 2);
    for (field, name) in fields.iter().zip(["label", "point"]) {
        assert_eq!(field.get("name").and_then(Value::as_str), Some(name));
        assert_eq!(field.get("kind").and_then(Value::as_i64), Some(8));
        let byte = source.find(name).unwrap();
        assert_eq!(
            field.get("selectionRange"),
            Some(&Value::object([
                ("start", source_position(source, byte)),
                ("end", source_position(source, byte + name.len())),
            ]))
        );
    }
}

#[test]
fn method_symbols_nest_methods_and_fields_without_exposing_internal_names() {
    let source = "# 🌍\r\nstruct Point {\r\n x: int\r\n def total(self) -> int { return self.x + self.y }\r\n y: int\r\n def shift(self, amount: int) { self.x = self.x + amount }\r\n}\r\nextern def foreign(value: int) -> int\r\ndef main() {}\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    let symbols = result(&output, 2).as_array().unwrap();
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.get("name").and_then(Value::as_str).unwrap())
            .collect::<Vec<_>>(),
        ["Point", "foreign", "main"]
    );
    let children = symbols[0].get("children").unwrap().as_array().unwrap();
    assert_eq!(
        children
            .iter()
            .map(|symbol| (
                symbol.get("name").and_then(Value::as_str).unwrap(),
                symbol.get("kind").and_then(Value::as_i64).unwrap()
            ))
            .collect::<Vec<_>>(),
        [("x", 8), ("total", 6), ("y", 8), ("shift", 6)]
    );
    for (method, name) in [(&children[1], "total"), (&children[3], "shift")] {
        let byte = source.find(&format!("def {name}")).unwrap() + 4;
        assert_eq!(
            method.get("selectionRange"),
            Some(&Value::object([
                ("start", source_position(source, byte)),
                ("end", source_position(source, byte + name.len())),
            ]))
        );
    }
}

#[test]
fn enum_symbols_and_constructor_pattern_definitions_preserve_utf16() {
    let source = "# 🌍\r\nenum OptionInt { none; some(value: int) }\r\ndef main() { var value = OptionInt.some(42); match value { case OptionInt.none { print(0) } case OptionInt.some(number) { print(number) } } }\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    messages.push(definition_at(3, URI, source, "OptionInt.some(42)", "some"));
    messages.push(definition_at(
        4,
        URI,
        source,
        "case OptionInt.some(number)",
        "some",
    ));
    messages.push(definition_at(5, URI, source, "case OptionInt.none", "none"));
    let output = exchange(messages);
    let symbols = result(&output, 2).as_array().unwrap();
    assert_eq!(symbols[0].get("kind").and_then(Value::as_i64), Some(10));
    let children = symbols[0].get("children").unwrap().as_array().unwrap();
    assert_eq!(
        children
            .iter()
            .map(|s| (
                s.get("name").and_then(Value::as_str).unwrap(),
                s.get("kind").and_then(Value::as_i64).unwrap()
            ))
            .collect::<Vec<_>>(),
        [("none", 22), ("some", 22)]
    );
    for id in [3, 4] {
        assert_selection(result(&output, id), URI, source, "some(value: int)", "some");
    }
    assert_selection(result(&output, 5), URI, source, "none; some", "none");
}

#[test]
fn invalid_enum_constructor_does_not_guess_a_definition() {
    let source = "enum OptionInt { none; some(value: int) }\ndef main() { var value = OptionInt.some(true) }";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(definition_at(
        2,
        URI,
        source,
        "OptionInt.some(true)",
        "some",
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
}

#[test]
fn method_definitions_use_checked_receiver_types_for_names_shared_with_globals() {
    let source = "struct Left { x: int; def total(self) -> int { return self.x }; def twice(self) -> int { return self.total() * 2 } }\nstruct Right { x: int; def total(self) -> int { return self.x + 1 } }\nstruct Box { item: Left }\ndef total() -> int { return 99 }\ndef main() { print(\"🌍\"); var a = Left(1); var b = Right(2); var box = Box(a); print(a.total()); print(b.total()); print(box.item.total()); print(Left(3).total()); print(total()) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    for (id, needle) in [
        (2, "a.total()"),
        (3, "b.total()"),
        (4, "box.item.total()"),
        (5, "Left(3).total()"),
        (6, "self.total()"),
        (7, "print(total())"),
    ] {
        messages.push(definition_at(id, URI, source, needle, "total"));
    }
    let output = exchange(messages);
    for id in [2, 4, 5, 6] {
        assert_selection(
            result(&output, id),
            URI,
            source,
            "def total(self) -> int { return self.x }",
            "total",
        );
    }
    assert_selection(
        result(&output, 3),
        URI,
        source,
        "def total(self) -> int { return self.x + 1 }",
        "total",
    );
    assert_selection(result(&output, 7), URI, source, "def total()", "total");
}

#[test]
fn method_definitions_do_not_guess_for_invalid_receivers_or_unresolved_calls() {
    for source in [
        "struct Point { x: int; def total(self) -> int { return self.x } } def total() -> int { return 3 } def main() { var p = 1; print(p.total()) }",
        "struct Point { x: int; def total(self) -> int { return self.x } } def total() -> int { return 3 } def main() { var p = Point(1); print(p.total(2)) }",
        "struct Point { x: int } def total() -> int { return 3 } def main() { var p = Point(1); print(p.total()) }",
    ] {
        let mut messages = start();
        messages.push(open(URI, source, 1));
        messages.push(definition_at(2, URI, source, "p.total", "total"));
        let output = exchange(messages);
        assert_eq!(result(&output, 2), &Value::Null, "guessed a method for invalid source: {source}");
    }
}

#[test]
fn method_definitions_preserve_imported_uris_and_unsaved_record_overlays() {
    let workspace = Workspace::new("lsp method imports 🌍");
    let left_saved = "struct Seed { value: int }\nstruct Left { x: int; def total(self) -> int { return self.x }; def copy(self) -> Left { return self } }\n";
    let right_saved = "struct Right { x: int; def total(self) -> int { return self.x + 1 } }\n";
    let main_source = "import \"left.bh\"\nimport \"right.bh\"\nstruct Anchor { pad: int }\ndef main() { print(\"🌍\"); var a = Left(1); var b = Right(2); print(a.total()); print(b.total()); print(a.copy().total()); print(a.x) }\n";
    let main = workspace.write("main.bh", main_source);
    let left = workspace.write("left.bh", left_saved);
    let right = workspace.write("right.bh", right_saved);
    let main_uri = file_uri(&main);
    let left_uri = file_uri(&left);
    let right_uri = file_uri(&right);
    let left_open = format!("# unsaved 🧭\r\n{}", left_saved.replace('\n', "\r\n"));
    let renamed = left_open.replace("def total(self)", "def amount(self)");
    let updated_main = main_source
        .replace(".total()", ".amount()")
        .replace("b.amount()", "b.total()");
    let mut messages = start();
    messages.push(open(&main_uri, main_source, 1));
    messages.push(definition_at(
        2,
        &main_uri,
        main_source,
        "a.total()",
        "total",
    ));
    messages.push(open(&left_uri, &left_open, 1));
    messages.push(definition_at(
        3,
        &main_uri,
        main_source,
        "a.total()",
        "total",
    ));
    messages.push(definition_at(
        4,
        &main_uri,
        main_source,
        "b.total()",
        "total",
    ));
    messages.push(definition_at(
        5,
        &main_uri,
        main_source,
        "a.copy().total()",
        "total",
    ));
    messages.push(definition_at(6, &main_uri, main_source, "a.copy()", "copy"));
    messages.push(definition_at(7, &main_uri, main_source, "Left(1)", "Left"));
    messages.push(definition_at(8, &main_uri, main_source, "a.x)", "x"));
    messages.push(change(&left_uri, &renamed, 2));
    messages.push(definition_at(
        9,
        &main_uri,
        main_source,
        "a.total()",
        "total",
    ));
    messages.push(change(&main_uri, &updated_main, 2));
    messages.push(definition_at(
        10,
        &main_uri,
        &updated_main,
        "a.amount()",
        "amount",
    ));
    let output = exchange(messages);
    assert_selection(
        result(&output, 2),
        &left_uri,
        left_saved,
        "def total(self)",
        "total",
    );
    for id in [3, 5] {
        assert_selection(
            result(&output, id),
            &left_uri,
            &left_open,
            "def total(self)",
            "total",
        );
    }
    assert_selection(
        result(&output, 4),
        &right_uri,
        right_saved,
        "def total(self)",
        "total",
    );
    assert_selection(
        result(&output, 6),
        &left_uri,
        &left_open,
        "def copy(self)",
        "copy",
    );
    assert_selection(
        result(&output, 7),
        &left_uri,
        &left_open,
        "struct Left",
        "Left",
    );
    assert_selection(result(&output, 8), &left_uri, &left_open, "x: int", "x");
    assert_eq!(result(&output, 9), &Value::Null);
    assert_selection(
        result(&output, 10),
        &left_uri,
        &renamed,
        "def amount(self)",
        "amount",
    );
    assert!(publications(&output, &main_uri)
        .iter()
        .any(|update| !diagnostics(update).is_empty()));
    assert!(diagnostics(publications(&output, &main_uri).last().unwrap()).is_empty());
    assert_eq!(fs::read_to_string(main).unwrap(), main_source);
    assert_eq!(fs::read_to_string(left).unwrap(), left_saved);
    assert_eq!(fs::read_to_string(right).unwrap(), right_saved);
}

#[test]
fn method_declaration_navigation_stays_local_when_unrelated_errors_block_checking() {
    let source = "struct Point { x: int; def total(self) -> int { return self.x } }\ndef total() -> int { return 7 }\ndef main() { print(missing) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(definition_at(2, URI, source, "def total(self)", "total"));
    messages.push(definition_at(3, URI, source, "x: int", "x"));
    messages.push(definition_at(4, URI, source, "struct Point", "Point"));
    let output = exchange(messages);
    assert_selection(result(&output, 2), URI, source, "def total(self)", "total");
    assert_selection(result(&output, 3), URI, source, "x: int", "x");
    assert_selection(result(&output, 4), URI, source, "struct Point", "Point");
}

#[test]
fn method_import_analysis_preserves_file_boundaries_for_unsaved_syntax_errors() {
    let workspace = Workspace::new("lsp method file boundaries");
    let main_source = "import \"point.bh\"\ndef main() { print(Point(1).value()) }\n";
    let point_source = "struct Point { x: int; def value(self) -> int { return self.x } }\n";
    let main = workspace.write("main.bh", main_source);
    let point = workspace.write("point.bh", point_source);
    let main_uri = file_uri(&main);
    let point_uri = file_uri(&point);
    let broken = "import \"point.bh\"\ndef main() {";
    let mut messages = start();
    messages.push(open(&main_uri, broken, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(&main_uri),
    ));
    let output = exchange(messages);
    let reports = publications(&output, &main_uri);
    let errors = diagnostics(reports.last().unwrap());
    assert!(!errors.is_empty());
    assert_eq!(
        errors[0].get("range").unwrap().get("start"),
        Some(&source_position(broken, broken.len()))
    );
    assert!(
        publications(&output, &point_uri)
            .iter()
            .all(|update| diagnostics(update).is_empty()),
        "entry EOF error leaked into imported file"
    );
    assert!(result(&output, 2).as_array().unwrap().is_empty());
    assert_eq!(fs::read_to_string(main).unwrap(), main_source);
    assert_eq!(fs::read_to_string(point).unwrap(), point_source);
}

#[test]
fn generic_templates_methods_and_calls_keep_original_utf16_navigation() {
    let source = "# 😀\nstruct Box[T] { value: T; def copy(self) -> T { return self.value } }\nenum Option[T] { none; some(value: T) }\ndef identity[T](value: T) -> T { return value }\ndef unused[T](value: T) -> T { return value }\ndef main() { var box = identity[Box[int]](Box[int](42)); var option = Option[int].some(box.copy()); match option { case Option[int].none {} case Option[int].some(value) { print(value) } } }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        10,
        "textDocument/documentSymbol",
        Value::object([("textDocument", document(URI))]),
    ));
    messages.push(definition_at(
        2,
        URI,
        source,
        "identity[Box[int]]",
        "identity",
    ));
    messages.push(definition_at(3, URI, source, "Box[int](42)", "Box"));
    messages.push(definition_at(4, URI, source, "box.copy()", "copy"));
    messages.push(definition_at(5, URI, source, "Option[int].some", "some"));
    let output = exchange(messages);
    let names: Vec<_> = response(&output, 10)
        .get("result")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol.get("name").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(names, ["Box", "Option", "identity", "unused", "main"]);
    for (id, declaration, name) in [
        (2, "def identity", "identity"),
        (3, "struct Box", "Box"),
        (4, "def copy", "copy"),
        (5, "some(value", "some"),
    ] {
        assert_selection(
            response(&output, id).get("result").unwrap(),
            URI,
            source,
            declaration,
            name,
        );
    }
}

#[test]
fn generic_template_member_navigation_reports_every_concrete_receiver() {
    let source = "struct A { def value(self) -> int { return 1 } }\nstruct B { def value(self) -> int { return 2 } }\ndef read[T](receiver: T) -> int { return receiver.value() }\ndef main() { print(read[A](A())); print(read[B](B())) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(definition_at(2, URI, source, "receiver.value()", "value"));
    let output = exchange(messages);
    let locations = result(&output, 2).as_array().unwrap();
    assert_eq!(locations.len(), 2);
    assert_selection(&locations[0], URI, source, "struct A { def value", "value");
    assert_selection(&locations[1], URI, source, "struct B { def value", "value");
}

#[test]
fn namespace_navigation_distinguishes_identical_names_and_unsaved_aliases() {
    let workspace = Workspace::new("lsp namespaces 🌍");
    let left_source = "struct Point { private x: int; def read(self) -> int { return self.x } }\ndef make() -> Point { return Point(21) }\ndef answer() -> int { return 21 }\ndef identity[T](value: T) -> T { return value }\n";
    let right_source = "struct Point { x: int; def read(self) -> int { return self.x } }\ndef make() -> Point { return Point(22) }\ndef answer() -> int { return 22 }\n";
    let main_source = "import \"left.bh\" as left\nimport \"right.bh\" as right\ndef main() { var a: left.Point = left.make(); var b: right.Point = right.Point(22); var c = left.identity[left.Point](a); print(left.answer() + right.answer() + c.read() + b.read()) }\n";
    let main = workspace.write("main.bh", main_source);
    let left = workspace.write("left.bh", left_source);
    let right = workspace.write("right.bh", right_source);
    let main_uri = file_uri(&main);
    let left_uri = file_uri(&left);
    let right_uri = file_uri(&right);
    let renamed = main_source
        .replace("as left", "as math")
        .replace("left.", "math.")
        .replace("import \"math.bh\"", "import \"left.bh\"");
    let left_open = format!("# unsaved 🧭\r\n{}", left_source.replace('\n', "\r\n"));
    let mut messages = start();
    messages.push(open(&main_uri, main_source, 1));
    messages.push(definition_at(
        2,
        &main_uri,
        main_source,
        "left.answer()",
        "answer",
    ));
    messages.push(definition_at(
        3,
        &main_uri,
        main_source,
        "right.answer()",
        "answer",
    ));
    messages.push(definition_at(
        4,
        &main_uri,
        main_source,
        "left.Point =",
        "Point",
    ));
    messages.push(definition_at(
        5,
        &main_uri,
        main_source,
        "right.Point(22)",
        "Point",
    ));
    messages.push(definition_at(6, &main_uri, main_source, "c.read()", "read"));
    messages.push(definition_at(7, &main_uri, main_source, "b.read()", "read"));
    messages.push(definition_at(
        8,
        &main_uri,
        main_source,
        "left.identity[",
        "identity",
    ));
    messages.push(change(&main_uri, &renamed, 2));
    messages.push(open(&left_uri, &left_open, 1));
    messages.push(definition_at(
        9,
        &main_uri,
        &renamed,
        "math.answer()",
        "answer",
    ));
    messages.push(definition_at(
        10,
        &main_uri,
        &renamed,
        "math.Point =",
        "Point",
    ));
    let output = exchange(messages);
    for (id, uri, source, declaration, name) in [
        (2, &left_uri, left_source, "def answer", "answer"),
        (3, &right_uri, right_source, "def answer", "answer"),
        (4, &left_uri, left_source, "struct Point", "Point"),
        (5, &right_uri, right_source, "struct Point", "Point"),
        (6, &left_uri, left_source, "def read", "read"),
        (7, &right_uri, right_source, "def read", "read"),
        (8, &left_uri, left_source, "def identity", "identity"),
        (9, &left_uri, left_open.as_str(), "def answer", "answer"),
        (10, &left_uri, left_open.as_str(), "struct Point", "Point"),
    ] {
        assert_ne!(
            result(&output, id),
            &Value::Null,
            "missing namespace definition for request {id}"
        );
        assert_selection(result(&output, id), uri, source, declaration, name);
    }
    for update in publications(&output, &main_uri) {
        assert!(diagnostics(update).is_empty(), "{update:?}");
    }
    assert_eq!(fs::read_to_string(main).unwrap(), main_source);
    assert_eq!(fs::read_to_string(left).unwrap(), left_source);
}

#[test]
fn namespace_privacy_diagnostics_do_not_navigate_to_same_named_public_members() {
    let workspace = Workspace::new("lsp private members");
    let source="import \"secret.bh\" as secret\nimport \"public.bh\" as public\ndef main() { var a = secret.make(); var b = public.Point(1); print(a.value); print(b.value) }\n";
    let secret_source="private def helper() -> int { return 42 }\nstruct Point { private value: int; private def read(self) -> int { return self.value } }\ndef make() -> Point { return Point(helper()) }\n";
    let main = workspace.write("main.bh", source);
    let secret = workspace.write("secret.bh", secret_source);
    workspace.write("public.bh", "struct Point { value: int }\n");
    let uri = file_uri(&main);
    let secret_uri = file_uri(&secret);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(definition_at(2, &uri, source, "a.value", "value"));
    messages.push(open(&secret_uri, secret_source, 1));
    messages.push(request(
        3,
        "textDocument/documentSymbol",
        document_params(&secret_uri),
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    let updates = publications(&output, &uri);
    let errors = diagnostics(updates.last().unwrap());
    assert!(
        errors
            .iter()
            .any(|error| error.get("code").and_then(Value::as_str) == Some("M005")),
        "{errors:?}"
    );
    assert!(errors.iter().all(|error| !error
        .get("message")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("__bhm_")));
    let symbols = result(&output, 3).as_array().unwrap();
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>(),
        ["helper", "Point", "make"]
    );
    let point = &symbols[1];
    assert_eq!(
        point
            .get("children")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|symbol| symbol.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>(),
        ["value", "read"]
    );
}

#[test]
fn syntax_only_unsaved_namespace_documents_keep_private_symbol_shapes() {
    let source="import \"missing.bh\" as library\nprivate struct Local { private value: int; private def read(self) -> int { return self.value } }\nprivate def helper[T](value: T) -> T { return value }\ndef main() { var value: library.Point = library.Point(1) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(request(
        2,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    let output = exchange(messages);
    let updates = publications(&output, URI);
    let errors = diagnostics(updates.last().unwrap());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].get("code").and_then(Value::as_str),
        Some("LSP001")
    );
    assert_eq!(
        result(&output, 2)
            .as_array()
            .unwrap()
            .iter()
            .map(|symbol| symbol.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Local", "helper", "main"]
    );
}

fn editor_request(id: i64, method: &str, uri: &str, source: &str, byte: usize) -> Value {
    request(
        id,
        method,
        Value::object([
            ("textDocument", document(uri)),
            ("position", source_position(source, byte)),
        ]),
    )
}
fn completion_labels(value: &Value) -> Vec<&str> {
    value
        .get("items")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.get("label").unwrap().as_str().unwrap())
        .collect()
}
fn hover_text(value: &Value) -> Option<&str> {
    value.get("contents")?.get("value")?.as_str()
}

#[test]
fn editor_completion_and_hover_respect_block_scope_shadowing_and_declaration_order() {
    let source = "def other() { var foreign_local = 3 }\ndef work(parameter: float) {\n var value = 1\n if true {\n  var value = buffer(\"inner\")\n  print(value)\n  # inner\n }\n # outer\n var future = 3\n}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        URI,
        source,
        source.find("# inner").unwrap(),
    ));
    messages.push(editor_request(
        3,
        "textDocument/completion",
        URI,
        source,
        source.find("# outer").unwrap(),
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        source,
        source.find("print(value)").unwrap() + 7,
    ));
    messages.push(editor_request(
        5,
        "textDocument/hover",
        URI,
        source,
        source.find("parameter").unwrap() + 1,
    ));
    let output = exchange(messages);
    for id in [2, 3] {
        let labels = completion_labels(result(&output, id));
        assert!(
            labels.contains(&"parameter") && labels.contains(&"value"),
            "{labels:?}"
        );
        assert!(!labels.contains(&"future") && !labels.contains(&"foreign_local"));
    }
    let details = |id| {
        result(&output, id)
            .get("items")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item.get("label").and_then(Value::as_str) == Some("value"))
            .unwrap()
            .get("detail")
            .unwrap()
            .as_str()
            .unwrap()
    };
    assert_eq!(details(2), "value: buffer");
    assert_eq!(details(3), "value: int");
    assert_eq!(hover_text(result(&output, 4)), Some("value: buffer"));
    assert_eq!(hover_text(result(&output, 5)), Some("parameter: float"));
}

#[test]
fn editor_completion_uses_checked_chained_receiver_and_collection_signatures() {
    let source = "struct Entry { score: float; def read(self) -> float { return self.score } }\ndef main() {\n var values = List[Entry]()\n values.push(Entry(2.5))\n print(values.get(0).score)\n var scores = Map[str, float]()\n scores.put(\"x\", 2.5)\n}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        URI,
        source,
        source.find(".score)").unwrap() + 1,
    ));
    messages.push(editor_request(
        3,
        "textDocument/completion",
        URI,
        source,
        source.find("values.push").unwrap() + 7,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        source,
        source.find("scores.put").unwrap() + 8,
    ));
    messages.push(editor_request(
        5,
        "textDocument/hover",
        URI,
        source,
        source.find(".score)").unwrap() + 2,
    ));
    let output = exchange(messages);
    assert_eq!(completion_labels(result(&output, 2)), ["read", "score"]);
    assert_eq!(
        completion_labels(result(&output, 3)),
        ["clear", "count", "get", "push", "set"]
    );
    assert_eq!(
        hover_text(result(&output, 4)),
        Some("def put(arg1: str, arg2: float) -> void")
    );
    assert_eq!(hover_text(result(&output, 5)), Some("score: float"));
    assert!(!result(&output, 3).to_string().contains("__bh"));
}

#[test]
fn editor_completion_recovers_partial_access_and_preserves_utf16_replacement() {
    let source = "def main() {\r\n var values = List[buffer]()\r\n print(\"🌍\"); values.ge";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        URI,
        source,
        source.len(),
    ));
    let output = exchange(messages);
    assert_eq!(completion_labels(result(&output, 2)), ["get"]);
    let item = &result(&output, 2).get("items").unwrap().as_array().unwrap()[0];
    assert_eq!(
        item.get("detail").and_then(Value::as_str),
        Some("def get(arg1: int) -> buffer")
    );
    assert_eq!(
        item.get("textEdit")
            .unwrap()
            .get("range")
            .unwrap()
            .get("start"),
        Some(&source_position(source, source.len() - 2))
    );
    assert_eq!(
        item.get("textEdit")
            .unwrap()
            .get("range")
            .unwrap()
            .get("end"),
        Some(&source_position(source, source.len()))
    );
}

#[test]
fn editor_completion_and_hover_hide_private_module_members_and_follow_unsaved_aliases() {
    let ws = Workspace::new("editor-privacy");
    ws.write("library.bh", "private def hidden() -> int { return 1 }\nstruct Box[T] { private secret: T; shown: T; private def concealed(self) -> T { return self.secret } def read(self) -> T { return self.shown } }\ndef make() -> Box[int] { return Box[int](1, 2) }\n");
    let source = "import \"library.bh\" as library\ndef main() {\n var values = List[library.Box[int]]()\n values.push(library.make())\n print(values.get(0).read())\n}\n";
    let path = ws.write("main.bh", source);
    let uri = file_uri(&path);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        &uri,
        source,
        source.find("library.make").unwrap() + 8,
    ));
    messages.push(editor_request(
        3,
        "textDocument/completion",
        &uri,
        source,
        source.find(".read()").unwrap() + 1,
    ));
    let edited = source
        .replace(" as library", " as api")
        .replace("library.Box", "api.Box")
        .replace("library.make", "api.make");
    messages.push(change(&uri, &edited, 2));
    messages.push(editor_request(
        4,
        "textDocument/completion",
        &uri,
        &edited,
        edited.find("api.make").unwrap() + 4,
    ));
    let private = edited.replace("api.make()", "api.hidden()");
    messages.push(change(&uri, &private, 3));
    messages.push(editor_request(
        5,
        "textDocument/hover",
        &uri,
        &private,
        private.find("api.hidden").unwrap() + 5,
    ));
    let output = exchange(messages);
    assert_eq!(completion_labels(result(&output, 2)), ["Box", "make"]);
    assert_eq!(completion_labels(result(&output, 3)), ["read", "shown"]);
    assert_eq!(completion_labels(result(&output, 4)), ["Box", "make"]);
    assert_eq!(result(&output, 5), &Value::Null);
    for id in [2, 3, 4] {
        assert!(!result(&output, id).to_string().contains("__bh"));
    }
}

#[test]
fn editor_loop_binders_are_typed_and_confined_to_their_bodies() {
    let source = "def main() {\n var values = List[buffer]()\n for item in values {\n  print(item)\n  # list\n }\n for index in range(0, 3) {\n  # range\n }\n # outside\n}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    for (id, marker) in [(2, "# list"), (3, "# range"), (4, "# outside")] {
        messages.push(editor_request(
            id,
            "textDocument/completion",
            URI,
            source,
            source.find(marker).unwrap(),
        ));
    }
    messages.push(editor_request(
        5,
        "textDocument/hover",
        URI,
        source,
        source.find("print(item)").unwrap() + 7,
    ));
    let output = exchange(messages);
    assert!(completion_labels(result(&output, 2)).contains(&"item"));
    assert!(completion_labels(result(&output, 3)).contains(&"index"));
    assert!(!completion_labels(result(&output, 3)).contains(&"item"));
    assert!(!completion_labels(result(&output, 4)).contains(&"index"));
    assert!(!completion_labels(result(&output, 4)).contains(&"item"));
    assert_eq!(hover_text(result(&output, 5)), Some("item: buffer"));
    for (iterable, expected) in [
        ("buffer()", "item: int"),
        ("ints()", "item: int"),
        ("Map[int,buffer]()", "item: int"),
        ("Map[str,buffer]()", "item: str"),
    ] {
        let source = format!("def main(){{for item in {iterable}{{print(item)}};print(1)}}");
        let mut messages = start();
        messages.push(open(URI, &source, 1));
        messages.push(editor_request(
            2,
            "textDocument/hover",
            URI,
            &source,
            source.find("print(item)").unwrap() + 7,
        ));
        messages.push(refactor_at(
            3,
            "textDocument/rename",
            URI,
            &source,
            "item in",
            Some("entry"),
        ));
        let output = exchange(messages);
        assert_eq!(hover_text(result(&output, 2)), Some(expected));
        let changes = result(&output, 3)
            .get("documentChanges")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            changes[0].get("edits").unwrap().as_array().unwrap().len(),
            2
        );
        bhasa::check(&apply_rename(&source, &changes[0])).unwrap();
    }
}

#[test]
fn editor_queries_ignore_comments_strings_and_bound_sorted_results() {
    let mut source = (0..180)
        .map(|n| format!("def user_{n:03}() {{}}\n"))
        .collect::<String>();
    source.push_str("def main() { print(\"user_001\"); user_001() # user_002\n}\n");
    let mut messages = start();
    messages.push(open(URI, &source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        URI,
        &source,
        source.find("; user_").unwrap() + 7,
    ));
    messages.push(editor_request(
        3,
        "textDocument/completion",
        URI,
        &source,
        source.find("\"user_001").unwrap() + 6,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        &source,
        source.find("# user_002").unwrap() + 4,
    ));
    let output = exchange(messages);
    let labels = completion_labels(result(&output, 2));
    assert_eq!(labels.len(), 128);
    assert_eq!(labels[0], "user_000");
    assert_eq!(labels[127], "user_127");
    assert_eq!(
        result(&output, 2).get("isIncomplete"),
        Some(&Value::Bool(true))
    );
    assert!(completion_labels(result(&output, 3)).is_empty());
    assert_eq!(result(&output, 4), &Value::Null);
}

#[test]
fn editor_chained_field_does_not_resolve_same_named_module_alias() {
    let ws = Workspace::new("editor-alias-receiver");
    ws.write(
        "library.bh",
        "def public_fn() -> int { return 1 }\nprivate def hidden_fn() -> int { return 2 }\n",
    );
    let source = "import \"library.bh\" as lib\nstruct Leaf { member: int }\nstruct Outer { lib: Leaf }\ndef main() { var obj = Outer(Leaf(1)); obj.lib. }\n";
    let path = ws.write("main.bh", source);
    let uri = file_uri(&path);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/completion",
        &uri,
        source,
        source.find("obj.lib.").unwrap() + 8,
    ));
    let output = exchange(messages);
    assert_eq!(completion_labels(result(&output, 2)), ["member"]);
}

#[test]
fn editor_midword_completion_replaces_suffix_and_generic_hover_keeps_template_types() {
    let source = "def identity[T](parameter: T) -> T { var local = parameter; return local }\ndef greeting() {}\ndef main() { var          banana = 1; print(identity[int](banana)); print(identity[float](2.5)); greeting() }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    let call = source.rfind("greeting()").unwrap();
    messages.push(editor_request(
        2,
        "textDocument/completion",
        URI,
        source,
        call + 2,
    ));
    messages.push(editor_request(
        3,
        "textDocument/hover",
        URI,
        source,
        source.find("parameter: T").unwrap() + 2,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        source,
        source.find("return local").unwrap() + 9,
    ));
    messages.push(editor_request(
        5,
        "textDocument/hover",
        URI,
        source,
        source.find("banana =").unwrap() + 2,
    ));
    let output = exchange(messages);
    let item = &result(&output, 2).get("items").unwrap().as_array().unwrap()[0];
    assert_eq!(item.get("label").and_then(Value::as_str), Some("greeting"));
    assert_eq!(
        item.get("textEdit")
            .unwrap()
            .get("range")
            .unwrap()
            .get("end"),
        Some(&source_position(source, call + "greeting".len()))
    );
    assert_eq!(hover_text(result(&output, 3)), Some("parameter: T"));
    assert_eq!(hover_text(result(&output, 4)), Some("var local"));
    assert_eq!(hover_text(result(&output, 5)), Some("banana: int"));
}

fn refactor_at(
    id: i64,
    method: &str,
    uri: &str,
    source: &str,
    needle: &str,
    name: Option<&str>,
) -> Value {
    let mut params = BTreeMap::from([
        ("textDocument".to_owned(), document(uri)),
        (
            "position".to_owned(),
            source_position(source, source.find(needle).unwrap()),
        ),
    ]);
    if method == "textDocument/references" {
        params.insert(
            "context".into(),
            Value::object([("includeDeclaration", true.into())]),
        );
    }
    if let Some(name) = name {
        params.insert("newName".into(), name.into());
    }
    request(id, method, Value::Object(params))
}
fn apply_rename(source: &str, change: &Value) -> String {
    fn byte(source: &str, pos: &Value) -> usize {
        let line = pos.get("line").unwrap().as_i64().unwrap() as usize;
        let character = pos.get("character").unwrap().as_i64().unwrap() as usize;
        let start = source
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>();
        let mut units = 0;
        for (offset, ch) in source[start..].char_indices() {
            if units == character {
                return start + offset;
            }
            units += ch.len_utf16();
        }
        assert_eq!(units, character);
        source.len()
    }
    let mut source = source.to_owned();
    let mut edits = change
        .get("edits")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| {
            let span = edit.get("range").unwrap();
            (
                byte(&source, span.get("start").unwrap()),
                byte(&source, span.get("end").unwrap()),
                edit.get("newText").unwrap().as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    edits.sort_by_key(|edit| edit.0);
    for (start, end, text) in edits.into_iter().rev() {
        source.replace_range(start..end, text);
    }
    source
}
use std::collections::BTreeMap;

#[test]
fn checked_local_navigation_and_references_follow_binding_identity() {
    let source = "def work(input:int)->int { var value=input; value=value+1; if true { var value=value+10; print(value) }; return value }\ndef main(){print(work(1))}\n";
    let mut messages = start();
    messages.push(open(URI, source, 4));
    messages.push(refactor_at(
        2,
        "textDocument/references",
        URI,
        source,
        "value=input",
        None,
    ));
    messages.push(definition_at(3, URI, source, "var value=value+10", "value"));
    messages.push(definition_at(4, URI, source, "print(value)", "value"));
    messages.push(refactor_at(
        5,
        "textDocument/references",
        URI,
        source,
        "input:int",
        None,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 5); // declaration, write, two reads, return
    assert_eq!(result(&output, 5).as_array().unwrap().len(), 2);
    assert_selection(
        result(&output, 3),
        URI,
        source,
        "var value=value+10",
        "value",
    );
    assert_selection(
        result(&output, 4),
        URI,
        source,
        "var value=value+10",
        "value",
    );
}

#[test]
fn checked_rename_returns_versioned_utf16_edits_without_changing_comments_or_strings() {
    let source = "# 🌍 value\r\ndef main(){print(\"🌍 value\"); var value=1; value=value+1; print(value)}\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 8));
    messages.push(refactor_at(
        2,
        "textDocument/prepareRename",
        URI,
        source,
        "value=1",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        URI,
        source,
        "value=1",
        Some("number"),
    ));
    let mut refs = refactor_at(4, "textDocument/references", URI, source, "value=1", None);
    if let Value::Object(fields) = &mut refs {
        if let Some(Value::Object(params)) = fields.get_mut("params") {
            params.insert(
                "context".into(),
                Value::object([("includeDeclaration", false.into())]),
            );
        }
    }
    messages.push(refs);
    let output = exchange(messages);
    assert_eq!(
        result(&output, 2).get("placeholder").unwrap().as_str(),
        Some("value")
    );
    assert_eq!(result(&output, 4).as_array().unwrap().len(), 3);
    let changes = result(&output, 3)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0]
            .get("textDocument")
            .unwrap()
            .get("version")
            .unwrap()
            .as_i64(),
        Some(8)
    );
    let changed = apply_rename(source, &changes[0]);
    assert_eq!(changed,"# 🌍 value\r\ndef main(){print(\"🌍 value\"); var number=1; number=number+1; print(number)}\r\n");
    bhasa::check(&changed).unwrap();
}

#[test]
fn rename_rejects_collisions_capture_reserved_names_and_unchecked_sources() {
    for source in [
        "def main(){var x=1;var y=2;print(x+y)}",
        "def main(){var x=1;if true{var y=2;print(x);print(y)}}",
        "def main(){var y=1;if true{var x=2;print(y);print(x)}}",
    ] {
        let mut messages = start();
        messages.push(open(URI, source, 1));
        messages.push(refactor_at(
            2,
            "textDocument/rename",
            URI,
            source,
            "x=",
            Some("y"),
        ));
        assert_eq!(
            response(&exchange(messages), 2)
                .get("error")
                .unwrap()
                .get("code")
                .unwrap()
                .as_i64(),
            Some(-32803)
        );
    }
    let source = "def main(){var value=1;print(value)}";
    for name in [
        "if",
        "print",
        "self",
        "private",
        "import",
        "range",
        "List",
        "task",
        "tcp_socket",
        "file_handle",
        "__bhg_1",
        "__bhm_2",
        "bhasa_rt_value",
        "bad name",
        "name\n",
        "🌍",
        "",
    ] {
        let mut messages = start();
        messages.push(open(URI, source, 1));
        messages.push(refactor_at(
            2,
            "textDocument/rename",
            URI,
            source,
            "value=",
            Some(name),
        ));
        assert_eq!(
            response(&exchange(messages), 2)
                .get("error")
                .unwrap()
                .get("code")
                .unwrap()
                .as_i64(),
            Some(-32602),
            "{name}"
        );
    }
    let broken = "def main(){var value=missing;print(value)}";
    let mut messages = start();
    messages.push(open(URI, broken, 1));
    messages.push(refactor_at(
        2,
        "textDocument/prepareRename",
        URI,
        broken,
        "value=",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        URI,
        broken,
        "value=",
        Some("number"),
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(response(&output, 3).get("error").is_some());
}

#[test]
fn loaded_graph_rename_finds_importers_requires_open_targets_and_preserves_aliases() {
    let ws = Workspace::new("rename-graphs");
    let library = "def answer()->int{return 42}\n";
    let source = "import \"library.bh\" as api\ndef main(){print(api.answer())}\n";
    let library_path = ws.write("library.bh", library);
    let main_path = ws.write("main.bh", source);
    let lib = path_uri(&library_path);
    let main = path_uri(&main_path);
    let mut messages = start();
    messages.push(open(&main, source, 2));
    messages.push(refactor_at(
        2,
        "textDocument/references",
        &main,
        source,
        "answer()",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        &main,
        source,
        "answer()",
        Some("response"),
    ));
    messages.push(open(&lib, library, 7));
    messages.push(refactor_at(
        4,
        "textDocument/rename",
        &lib,
        library,
        "answer()",
        Some("response"),
    ));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        &main,
        source,
        "api\ndef",
        Some("library"),
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 2);
    assert!(response(&output, 3).get("error").is_some());
    let changes = result(&output, 4)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 2);
    for change in changes {
        let doc = change.get("textDocument").unwrap();
        let uri = doc.get("uri").unwrap().as_str().unwrap();
        let original = if uri == lib { library } else { source };
        let changed = apply_rename(original, change);
        assert!(changed.contains("response()"));
        if uri == main {
            assert!(changed.contains("as api"));
            assert!(changed.contains("api.response()"));
        }
    }
    let alias = result(&output, 5)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(alias.len(), 1);
    assert_eq!(
        apply_rename(source, &alias[0]),
        source
            .replace("as api", "as library")
            .replace("api.answer", "library.answer")
    );
    assert_eq!(fs::read_to_string(library_path).unwrap(), library);
    assert_eq!(fs::read_to_string(main_path).unwrap(), source);
}

#[test]
fn generic_rename_deduplicates_instances_and_rejects_ambiguous_receiver_tokens() {
    let source="def identity[T](value:T)->T{return value}\ndef main(){print(identity[int](1));print(identity[float](2.0))}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/rename",
        URI,
        source,
        "identity[T]",
        Some("keep"),
    ));
    messages.push(refactor_at(
        3,
        "textDocument/references",
        URI,
        source,
        "value:T",
        None,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 3).as_array().unwrap().len(), 2);
    let changes = result(&output, 2)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        changes[0].get("edits").unwrap().as_array().unwrap().len(),
        3
    );
    bhasa::check(&apply_rename(source, &changes[0])).unwrap();
    let source="struct A{def value(self)->int{return 1}}\nstruct B{def value(self)->int{return 2}}\ndef read[T](item:T)->int{return item.value()}\ndef main(){print(read[A](A()));print(read[B](B()))}";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/prepareRename",
        URI,
        source,
        "value()",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        URI,
        source,
        "value(self)",
        Some("number"),
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(response(&output, 3).get("error").is_some());
}

#[test]
fn stepped_loop_and_match_bindings_keep_sources_outside_the_new_scope() {
    let source="enum E{value(n:int)}\ndef main(){var step=2;for step in range(0,6,step){print(step)};match E.value(step){case E.value(step){print(step)}};print(step)}";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/references",
        URI,
        source,
        "step=2",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/references",
        URI,
        source,
        "step in",
        None,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/references",
        URI,
        source,
        "step){print(step)}}",
        None,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 4);
    assert_eq!(result(&output, 3).as_array().unwrap().len(), 2);
    assert_eq!(result(&output, 4).as_array().unwrap().len(), 2);
}

#[test]
fn rename_refuses_an_open_importer_with_unsaved_unknown_import_paths() {
    let ws = Workspace::new("rename-unsaved-import-path");
    let library = "def answer()->int{return 42}\n";
    let lib_path = ws.write("library.bh", library);
    ws.write("other.bh", "def answer()->int{return 1}\n");
    let saved = "import \"other.bh\" as api\ndef main(){}\n";
    let edited = "import \"library.bh\" as api\ndef main(){print(api.answer())}\n";
    let main = path_uri(&ws.write("main.bh", saved));
    let lib = path_uri(&lib_path);
    let mut messages = start();
    messages.push(open(&lib, library, 1));
    messages.push(open(&main, edited, 2));
    messages.push(refactor_at(
        2,
        "textDocument/prepareRename",
        &lib,
        library,
        "answer()",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        &lib,
        library,
        "answer()",
        Some("response"),
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(response(&output, 3).get("error").is_some());
    assert_eq!(fs::read_to_string(lib_path).unwrap(), library);
}

#[test]
fn rename_preserves_member_and_enum_identities_and_bounds_name_expansion() {
    let source = "struct Item{private value:int;def read(self)->int{return self.value}}\nenum Choice{some(item:Item);none}\ndef main(){var item=Item(42);print(item.read());match Choice.some(item){case Choice.some(value){print(value.read())}case Choice.none{}}}\n";
    for (needle, name) in [
        ("Item{", "Entry"),
        ("read(self)", "get"),
        ("Choice{", "Result"),
        ("some(item:", "present"),
    ] {
        let mut messages = start();
        messages.push(open(URI, source, 1));
        messages.push(refactor_at(
            2,
            "textDocument/rename",
            URI,
            source,
            needle,
            Some(name),
        ));
        let output = exchange(messages);
        // Builtin names are rejected even for members; use a nonbuiltin below.
        if name == "get" {
            assert!(response(&output, 2).get("error").is_some());
            continue;
        }
        let changes = result(&output, 2)
            .get("documentChanges")
            .unwrap()
            .as_array()
            .unwrap();
        bhasa::check(&apply_rename(source, &changes[0])).unwrap();
    }
    let source = "def main(){var value=1;print(value)}";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/rename",
        URI,
        source,
        "value=",
        Some(&"n".repeat(257)),
    ));
    assert_eq!(
        response(&exchange(messages), 2)
            .get("error")
            .unwrap()
            .get("code")
            .unwrap()
            .as_i64(),
        Some(-32602)
    );
}

#[test]
fn rename_accepts_existing_contextual_locals_and_builtin_named_members() {
    for name in ["task", "private", "as", "import"] {
        let source = format!("def main(){{var {name}=1;print({name})}}");
        let mut messages = start();
        messages.push(open(URI, &source, 1));
        messages.push(refactor_at(
            2,
            "textDocument/rename",
            URI,
            &source,
            &format!("{name}="),
            Some("changed"),
        ));
        let output = exchange(messages);
        let changes = result(&output, 2)
            .get("documentChanges")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            changes[0].get("edits").unwrap().as_array().unwrap().len(),
            2,
            "{name}"
        );
        bhasa::check(&apply_rename(&source, &changes[0])).unwrap();
    }
    let source="def work(value:buffer)->buffer{return value}\ndef main(){var task=spawn(work(buffer()));var value=join(task);print(value)}";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/rename",
        URI,
        source,
        "task=spawn",
        Some("job"),
    ));
    let output = exchange(messages);
    let changes = result(&output, 2)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    bhasa::check(&apply_rename(source, &changes[0])).unwrap();
    let source = "struct Item{def get(self)->int{return 42}}\ndef main(){print(Item().get())}";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/rename",
        URI,
        source,
        "get(self)",
        Some("read"),
    ));
    messages.push(refactor_at(
        3,
        "textDocument/prepareRename",
        URI,
        source,
        "self)",
        None,
    ));
    let output = exchange(messages);
    let changes = result(&output, 2)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    bhasa::check(&apply_rename(source, &changes[0])).unwrap();
    assert_eq!(result(&output, 3), &Value::Null);
}

#[test]
fn propagated_success_bindings_have_checked_hover_references_and_rename() {
    let source="enum R{ok(payload:buffer);error(problem:buffer)}\ndef read()->R{return R.ok(buffer(\"x\"))}\ndef run()->R{var value=try read();append(value,\"!\");return R.ok(value)}\ndef main(){}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/hover",
        URI,
        source,
        source.find("append(value").unwrap() + 8,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/references",
        URI,
        source,
        "value=try",
        None,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/rename",
        URI,
        source,
        "value=try",
        Some("contents"),
    ));
    let output = exchange(messages);
    assert_eq!(hover_text(result(&output, 2)), Some("value: buffer"));
    assert_eq!(result(&output, 3).as_array().unwrap().len(), 3);
    let changes = result(&output, 4)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    bhasa::check(&apply_rename(source, &changes[0])).unwrap();
}
// Append these tests to tests/lsp.rs; they use that file's protocol helpers.

#[test]
fn immutable_binding_named_let_has_identity_only_on_binder_and_reads() {
    for (source, detail, read_needle, references) in [
        (
            "# 🌍 let\r\ndef main(){print(\"🌍 let\"); let let = 1; print(let)}\r\n",
            "let let: int",
            "print(let)",
            2,
        ),
        (
            "# 🌍 let\r\nenum R{ok(payload:buffer);error(problem:buffer)}\r\ndef read()->R{return R.ok(buffer(\"x\"))}\r\ndef run()->R{print(\"🌍 let\"); let let = try read();append(let,\"!\");return R.ok(let)}\r\ndef main(){}\r\n",
            "let let: buffer",
            "append(let,",
            3,
        ),
    ] {
        let marker = source.find("let let").unwrap();
        let binder = source.find("let =").unwrap();
        let read = source.find(read_needle).unwrap() + read_needle.find("let").unwrap();
        let mut messages = start();
        messages.push(open(URI, source, 7));
        for (id, method) in [
            (2, "textDocument/definition"),
            (3, "textDocument/hover"),
            (4, "textDocument/prepareRename"),
        ] {
            messages.push(editor_request(id, method, URI, source, marker));
        }
        messages.push(refactor_at(5, "textDocument/references", URI, source, "let let", None));
        messages.push(refactor_at(6, "textDocument/rename", URI, source, "let let", Some("contents")));
        for (id, method) in [
            (7, "textDocument/definition"),
            (8, "textDocument/hover"),
            (9, "textDocument/prepareRename"),
        ] {
            messages.push(editor_request(id, method, URI, source, binder));
        }
        messages.push(refactor_at(10, "textDocument/references", URI, source, "let =", None));
        messages.push(refactor_at(11, "textDocument/rename", URI, source, "let =", Some("contents")));
        messages.push(editor_request(12, "textDocument/definition", URI, source, read));
        messages.push(editor_request(13, "textDocument/hover", URI, source, read));
        let output = exchange(messages);
        let updates = publications(&output, URI);
        assert!(diagnostics(updates.last().unwrap()).is_empty(), "{output:?}");
        for id in [2, 3, 4] {
            assert_eq!(result(&output, id), &Value::Null, "marker query {id}: {source}");
        }
        assert!(result(&output, 5).as_array().unwrap().is_empty());
        assert_eq!(error_code(response(&output, 6)), -32803);
        for id in [7, 12] {
            assert_selection(result(&output, id), URI, source, "let =", "let");
        }
        for id in [8, 13] {
            assert_eq!(hover_text(result(&output, id)), Some(detail), "{source}");
        }
        let prepared = result(&output, 9);
        assert_eq!(prepared.get("placeholder").and_then(Value::as_str), Some("let"));
        assert_eq!(prepared.get("range"), Some(&Value::object([
            ("start", source_position(source, binder)),
            ("end", source_position(source, binder + 3)),
        ])));
        let locations = result(&output, 10).as_array().unwrap();
        assert_eq!(locations.len(), references, "{source}: {locations:?}");
        assert!(locations.iter().all(|location| {
            location.get("uri").and_then(Value::as_str) == Some(URI)
                && location.get("range").and_then(|range| range.get("start")) != Some(&source_position(source, marker))
        }));
        for byte in [binder, read] {
            assert!(locations.iter().any(|location| location.get("range") == Some(&Value::object([
                ("start", source_position(source, byte)),
                ("end", source_position(source, byte + 3)),
            ]))), "missing reference at {byte}: {locations:?}");
        }
        let changes = result(&output, 11).get("documentChanges").unwrap().as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].get("textDocument").unwrap().get("version").and_then(Value::as_i64), Some(7));
        assert_eq!(changes[0].get("edits").unwrap().as_array().unwrap().len(), references);
        let changed = apply_rename(source, &changes[0]);
        let expected = source.replacen("let =", "contents =", 1)
            .replace("print(let)", "print(contents)")
            .replace("append(let,", "append(contents,")
            .replace("R.ok(let)", "R.ok(contents)");
        assert_eq!(changed, expected);
        assert!(changed.contains("let contents ="));
        bhasa::check(&changed).unwrap();
    }
}

#[test]
fn function_named_let_remains_navigable_and_rename_preserves_declaration_markers() {
    let source = "# 🌍 let\r\ndef let(value:int)->int{return value+1}\r\ndef main(){print(\"🌍 let\"); let answer = let(41);print(answer)}\r\n";
    let marker = source.find("let answer").unwrap();
    let call = source.find("let(41)").unwrap();
    let mut messages = start();
    messages.push(open(URI, source, 3));
    for (id, method) in [
        (2, "textDocument/definition"),
        (3, "textDocument/hover"),
        (4, "textDocument/prepareRename"),
    ] {
        messages.push(editor_request(id, method, URI, source, marker));
    }
    messages.push(refactor_at(
        5,
        "textDocument/references",
        URI,
        source,
        "let answer",
        None,
    ));
    messages.push(editor_request(
        6,
        "textDocument/definition",
        URI,
        source,
        call,
    ));
    messages.push(editor_request(7, "textDocument/hover", URI, source, call));
    messages.push(refactor_at(
        8,
        "textDocument/prepareRename",
        URI,
        source,
        "let(41)",
        None,
    ));
    messages.push(refactor_at(
        9,
        "textDocument/references",
        URI,
        source,
        "let(41)",
        None,
    ));
    messages.push(refactor_at(
        10,
        "textDocument/rename",
        URI,
        source,
        "let(41)",
        Some("increment"),
    ));
    let output = exchange(messages);
    let updates = publications(&output, URI);
    assert!(
        diagnostics(updates.last().unwrap()).is_empty(),
        "{output:?}"
    );
    for id in [2, 3, 4] {
        assert_eq!(
            result(&output, id),
            &Value::Null,
            "contextual marker resolved to global function: {id}"
        );
    }
    assert!(result(&output, 5).as_array().unwrap().is_empty());
    assert_selection(result(&output, 6), URI, source, "def let", "let");
    assert!(hover_text(result(&output, 7)).is_some_and(|text| text.contains("def let(")));
    assert_eq!(
        result(&output, 8)
            .get("placeholder")
            .and_then(Value::as_str),
        Some("let")
    );
    assert_eq!(result(&output, 9).as_array().unwrap().len(), 2);
    let changes = result(&output, 10)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].get("edits").unwrap().as_array().unwrap().len(),
        2
    );
    let changed = apply_rename(source, &changes[0]);
    assert_eq!(changed, "# 🌍 let\r\ndef increment(value:int)->int{return value+1}\r\ndef main(){print(\"🌍 let\"); let answer = increment(41);print(answer)}\r\n");
    bhasa::check(&changed).unwrap();
}

fn signature_at(id: i64, uri: &str, source: &str, byte: usize) -> Value {
    editor_request(id, "textDocument/signatureHelp", uri, source, byte)
}
fn signature_label(value: &Value) -> Option<&str> {
    value
        .get("signatures")?
        .as_array()?
        .first()?
        .get("label")?
        .as_str()
}

#[test]
fn checked_signature_help_selects_nested_arguments_and_excludes_string_commas() {
    let source = "def add(left: int, right: int) -> int { return left + right }\ndef use(text: str, value: int) {}\ndef main() { use(\"🌍, comma\", add(1, 2)) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(signature_at(2, URI, source, source.find("2))").unwrap()));
    messages.push(signature_at(3, URI, source, source.find("add(1").unwrap()));
    messages.push(signature_at(4, URI, source, source.find("comma").unwrap()));
    messages.push(signature_at(
        5,
        URI,
        source,
        source.find("use(text").unwrap() + 4,
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def add(left: int, right: int) -> int")
    );
    assert_eq!(
        result(&output, 2)
            .get("activeParameter")
            .and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def use(text: str, value: int) -> void")
    );
    assert_eq!(
        result(&output, 3)
            .get("activeParameter")
            .and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(result(&output, 4), &Value::Null);
    assert_eq!(result(&output, 5), &Value::Null);
    let capability = result(&output, 1)
        .get("capabilities")
        .unwrap()
        .get("signatureHelpProvider")
        .unwrap();
    assert_eq!(
        capability.get("triggerCharacters"),
        Some(&Value::Array(vec!["(".into(), ",".into()]))
    );
}

#[test]
fn checked_signature_help_handles_methods_generics_zero_args_and_checked_builtins() {
    let source = "struct Entry { value: int; def plus(self, amount: int) -> int { return self.value + amount } }\ndef identity[T](value: T) -> T { return value }\ndef empty() {}\ndef main() { var entry = Entry(2); print(entry.plus(identity[int](3))); empty(); print(len(buffer(\"a\"))) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(signature_at(
        2,
        URI,
        source,
        source.find("identity[int](3)").unwrap(),
    ));
    messages.push(signature_at(3, URI, source, source.find("3)))").unwrap()));
    messages.push(signature_at(
        4,
        URI,
        source,
        source.rfind("empty()").unwrap() + 6,
    ));
    messages.push(signature_at(
        5,
        URI,
        source,
        source.find("buffer(\"a").unwrap(),
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def plus(amount: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def identity(value: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 4)),
        Some("def empty() -> void")
    );
    assert_eq!(
        result(&output, 4)
            .get("signatures")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("parameters")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        signature_label(result(&output, 5)),
        Some("def len(arg1: buffer) -> int")
    );
    assert!(!result(&output, 3).to_string().contains("__bh"));
}

#[test]
fn checked_signature_help_rechecks_unsaved_text_and_returns_null_for_invalid_graphs() {
    let good = "def value(item: int) -> int { return item }\ndef main() { print(value(1)) }\n";
    let changed = good
        .replace("item: int", "item: float")
        .replace("-> int", "-> float")
        .replace("value(1)", "value(1.5)");
    let bad = changed.replace("value(1.5)", "value(\"bad\")");
    let incomplete = "def main() { print(";
    let mut messages = start();
    messages.push(open(URI, good, 1));
    messages.push(signature_at(
        2,
        URI,
        good,
        good.find("value(1)").unwrap() + 6,
    ));
    messages.push(change(URI, &changed, 2));
    messages.push(signature_at(
        3,
        URI,
        &changed,
        changed.find("value(1.5)").unwrap() + 6,
    ));
    messages.push(change(URI, &bad, 3));
    messages.push(signature_at(
        4,
        URI,
        &bad,
        bad.find("value(\"bad").unwrap() + 6,
    ));
    messages.push(change(URI, incomplete, 4));
    messages.push(signature_at(5, URI, incomplete, incomplete.len()));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def value(item: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def value(item: float) -> float")
    );
    assert_eq!(result(&output, 4), &Value::Null);
    assert_eq!(result(&output, 5), &Value::Null);
}

#[test]
fn checked_signature_help_uses_imported_identity_and_unsaved_dependency_overlay() {
    let workspace = Workspace::new("signature imports");
    let library = "private def hidden(value: int) -> int { return value }\ndef convert(value: int) -> int { return hidden(value) }\n";
    let source = "import \"library.bh\" as lib\ndef main() { print(lib.convert(2)) }\n";
    let main = workspace.write("main.bh", source);
    let library_path = workspace.write("library.bh", library);
    let main_uri = file_uri(&main);
    let library_uri = file_uri(&library_path);
    let renamed = library
        .replace("convert(value: int)", "convert(amount: int)")
        .replace("hidden(value) }", "hidden(amount) }");
    let mut messages = start();
    messages.push(open(&main_uri, source, 1));
    messages.push(signature_at(
        2,
        &main_uri,
        source,
        source.find("convert(2)").unwrap() + 8,
    ));
    messages.push(open(&library_uri, &renamed, 1));
    messages.push(signature_at(
        3,
        &main_uri,
        source,
        source.find("convert(2)").unwrap() + 8,
    ));
    let private = source.replace("lib.convert", "lib.hidden");
    messages.push(change(&main_uri, &private, 2));
    messages.push(signature_at(
        4,
        &main_uri,
        &private,
        private.find("hidden(2)").unwrap() + 7,
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def convert(value: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def convert(amount: int) -> int")
    );
    assert_eq!(result(&output, 4), &Value::Null);
}

#[test]
fn checked_signature_help_peels_grouped_calls_and_ignores_nested_comment_commas() {
    let source = "def add(left: int, right: int) -> int { return left + right }\ndef main() { print((add(1, # ignored , comma\n 2))) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(signature_at(2, URI, source, source.find("2)))").unwrap()));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def add(left: int, right: int) -> int")
    );
    assert_eq!(
        result(&output, 2)
            .get("activeParameter")
            .and_then(Value::as_i64),
        Some(1)
    );
}

#[test]
fn checked_signature_help_does_not_choose_a_concrete_instance_inside_a_generic_template() {
    let source = "def identity[T](value: T) -> T { return value }\ndef wrap[T](value: T) -> T { return identity[T](value) }\ndef main() { print(wrap[int](1)); print(wrap[float](2.5)) }\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(signature_at(
        2,
        URI,
        source,
        source.find("identity[T](value)").unwrap() + 12,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
}

#[test]
fn checked_signature_help_handles_new_unsaved_file_uri_documents_before_and_after_save() {
    let workspace = Workspace::new("signature new unsaved file");
    let path = workspace.file("new.bh");
    assert!(!path.exists());
    let uri = format!("{}/new.bh", file_uri(workspace.path()));
    let source =
        "def add(left:int,right:int)->int{return left+right}\ndef main(){print(add(1,2))}\n";
    let byte = source.find("2))").unwrap();
    let mut before = start();
    before.push(open(&uri, source, 1));
    before.push(signature_at(2, &uri, source, byte));
    before.push(editor_request(
        3,
        "textDocument/hover",
        &uri,
        source,
        source.rfind("add(").unwrap(),
    ));
    let mut after = vec![
        notification("textDocument/didSave", document_params(&uri)),
        signature_at(4, &uri, source, byte),
    ];
    finish(&mut after);
    let reader = SaveBetweenFrames {
        before: Cursor::new(encode(&before)),
        after: Cursor::new(encode(&after)),
        save: Some((path.clone(), source.to_owned())),
    };
    let mut bytes = Vec::new();
    assert_eq!(bhasa::lsp::serve(reader, &mut bytes), 0);
    let output = decode(&bytes);
    for id in [2, 4] {
        assert_eq!(
            signature_label(result(&output, id)),
            Some("def add(left: int, right: int) -> int")
        );
        assert_eq!(
            result(&output, id)
                .get("activeParameter")
                .and_then(Value::as_i64),
            Some(1)
        );
    }
    assert!(hover_text(result(&output, 3)).is_some_and(|text| text.contains("def add(")));
    assert!(path.exists());
}

#[test]
fn checked_signature_help_covers_externs_chained_receivers_collections_and_crlf_positions() {
    let source = "extern def foreign(left:int,right:int)->int\r\nstruct Entry{value:int;def add(self,amount:int)->int{return self.value+amount}}\r\ndef make()->Entry{return Entry(2)}\r\ndef main(){print(\"🌍\");print(foreign(1,2));print(make().add(3));var list=List[Entry]();list.push(make());print(list.get(0).add(4));var map=Map[str,int]();map.put(\"x\",5)}\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(signature_at(
        2,
        URI,
        source,
        source.find("2));print(make").unwrap(),
    ));
    messages.push(signature_at(
        3,
        URI,
        source,
        source.find("add(3)").unwrap() + 4,
    ));
    messages.push(signature_at(
        4,
        URI,
        source,
        source.find("add(4)").unwrap() + 4,
    ));
    messages.push(signature_at(
        5,
        URI,
        source,
        source.find("get(0)").unwrap() + 4,
    ));
    messages.push(signature_at(
        6,
        URI,
        source,
        source.find("put(\"x\",5)").unwrap() + 8,
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def foreign(left: int, right: int) -> int")
    );
    assert_eq!(
        result(&output, 2)
            .get("activeParameter")
            .and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def add(amount: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 4)),
        Some("def add(amount: int) -> int")
    );
    assert_eq!(
        signature_label(result(&output, 5)),
        Some("def get(arg1: int) -> Entry")
    );
    assert_eq!(
        signature_label(result(&output, 6)),
        Some("def put(arg1: str, arg2: int) -> void")
    );
    assert_eq!(
        result(&output, 6)
            .get("activeParameter")
            .and_then(Value::as_i64),
        Some(1)
    );
}

fn semantic_request(id: i64, uri: &str) -> Value {
    request(id, "textDocument/semanticTokens/full", document_params(uri))
}
fn decoded_semantic(value: &Value) -> Vec<(usize, usize, usize, usize, usize)> {
    let data = value.get("data").unwrap().as_array().unwrap();
    assert_eq!(data.len() % 5, 0);
    let mut line = 0;
    let mut character = 0;
    data.as_chunks::<5>()
        .0
        .iter()
        .map(|chunk| {
            let values: Vec<_> = chunk
                .iter()
                .map(|value| usize::try_from(value.as_i64().unwrap()).unwrap())
                .collect();
            line += values[0];
            character = if values[0] == 0 {
                character + values[1]
            } else {
                values[1]
            };
            assert!(values[2] > 0);
            assert!(values[3] < 11);
            assert!(values[4] < 4);
            (line, character, values[2], values[3], values[4])
        })
        .collect()
}
fn semantic_site(value: &Value, source: &str, byte: usize) -> Option<(usize, usize)> {
    let at = source_position(source, byte);
    let line = at.get("line").unwrap().as_i64().unwrap() as usize;
    let character = at.get("character").unwrap().as_i64().unwrap() as usize;
    decoded_semantic(value)
        .into_iter()
        .find(|site| site.0 == line && site.1 == character)
        .map(|site| (site.3, site.4))
}

#[test]
fn checked_semantic_tokens_distinguish_let_markers_binders_reads_and_calls() {
    let source = "def let(value:int)->int{return value}\ndef main(){let answer=let(2);var let=1;print(let+answer)}\n";
    let marker = source.find("let answer").unwrap();
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(semantic_request(2, URI));
    messages.push(editor_request(3, "textDocument/hover", URI, source, marker));
    let output = exchange(messages);
    let tokens = result(&output, 2);
    assert_eq!(
        semantic_site(tokens, source, source.find("let(value").unwrap()),
        Some((4, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("var let").unwrap() + 4),
        Some((8, 1))
    );
    assert_eq!(semantic_site(tokens, source, marker), Some((9, 0)));
    assert_eq!(
        semantic_site(tokens, source, source.find("answer=").unwrap()),
        Some((8, 3))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("let(2)").unwrap()),
        Some((4, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("let+answer").unwrap()),
        Some((8, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.rfind("answer").unwrap()),
        Some((8, 2))
    );
    assert_eq!(result(&output, 3), &Value::Null);
    let provider = result(&output, 1)
        .get("capabilities")
        .unwrap()
        .get("semanticTokensProvider")
        .unwrap();
    assert_eq!(provider.get("full"), Some(&Value::Bool(true)));
    assert_eq!(provider.get("range"), Some(&Value::Bool(false)));
}

#[test]
fn checked_semantic_tokens_keep_try_markers_separate_from_ordinary_try_names() {
    let source = "enum R{ok(value:int);error(message:buffer)}\ndef try(value:R)->R{return value}\ndef run(input:R)->R{let try=try try(input);return R.ok(try)}\ndef main(){}\n";
    let marker = source.find("try try(input)").unwrap();
    let binder = source.find("let try=").unwrap() + 4;
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(semantic_request(2, URI));
    messages.push(editor_request(3, "textDocument/hover", URI, source, marker));
    let output = exchange(messages);
    let tokens = result(&output, 2);
    assert_eq!(
        semantic_site(tokens, source, source.find("let try=").unwrap()),
        Some((9, 0))
    );
    assert_eq!(semantic_site(tokens, source, binder), Some((8, 3)));
    assert_eq!(semantic_site(tokens, source, marker), Some((9, 0)));
    assert_eq!(semantic_site(tokens, source, marker + 4), Some((4, 0)));
    assert_eq!(
        semantic_site(tokens, source, source.find("R.ok(try)").unwrap() + 5),
        Some((8, 2))
    );
    assert_eq!(result(&output, 3), &Value::Null);
}

#[test]
fn checked_semantic_tokens_encode_utf16_crlf_and_mutability_of_shadowed_bindings() {
    let source = "# 🌍 let\r\ndef main(){print(\"🌍 let\"); let let=1; if true {var let=2;print(let)} print(let)}\r\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(semantic_request(2, URI));
    let output = exchange(messages);
    let tokens = result(&output, 2);
    assert_eq!(
        semantic_site(tokens, source, source.find("let let=").unwrap()),
        Some((9, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("let=1").unwrap()),
        Some((8, 3))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("let=2").unwrap()),
        Some((8, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("print(let)").unwrap() + 6),
        Some((8, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.rfind("print(let)").unwrap() + 6),
        Some((8, 2))
    );
    let sites = decoded_semantic(tokens);
    assert!(sites.iter().all(|site| site.0 == 1));
    for pair in sites.windows(2) {
        assert!(pair[0].0 < pair[1].0 || pair[0].1 + pair[0].2 <= pair[1].1);
    }
    assert!(semantic_site(tokens, source, source.find("let\r\n").unwrap()).is_none());
    assert!(semantic_site(tokens, source, source.find("let\"").unwrap()).is_none());
}

#[test]
fn checked_semantic_tokens_classify_records_fields_methods_parameters_and_enums() {
    let source = "struct Entry{value:int;def get(self,amount:int)->int{return self.value+amount}}\nenum Choice{item(value:int);empty}\ndef main(){let entry=Entry(1);print(entry.get(2));var choice=Choice.item(3);match choice{case Choice.item(value){print(value)}case Choice.empty{}}}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(semantic_request(2, URI));
    let output = exchange(messages);
    let tokens = result(&output, 2);
    assert_eq!(
        semantic_site(tokens, source, source.find("Entry{").unwrap()),
        Some((1, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("value:int").unwrap()),
        Some((6, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("get(self").unwrap()),
        Some((5, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("amount:int").unwrap()),
        Some((7, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("self.value").unwrap() + 5),
        Some((6, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("Choice{").unwrap()),
        Some((2, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.find("Choice.item(3)").unwrap() + 7),
        Some((3, 0))
    );
}

#[test]
fn checked_semantic_tokens_follow_unsaved_changes_and_drop_invalid_graphs() {
    let mutable = "def main(){var value=1;print(value)}\n";
    let immutable = mutable.replace("var value", "let value");
    let invalid = immutable.replace("print(value)", "missing(value)");
    let mut messages = start();
    messages.push(open(URI, mutable, 1));
    messages.push(semantic_request(2, URI));
    messages.push(change(URI, &immutable, 2));
    messages.push(semantic_request(3, URI));
    messages.push(change(URI, &invalid, 3));
    messages.push(semantic_request(4, URI));
    messages.push(change(URI, "def main(){let value=", 4));
    messages.push(semantic_request(5, URI));
    let output = exchange(messages);
    assert_eq!(
        semantic_site(result(&output, 2), mutable, mutable.rfind("value").unwrap()),
        Some((8, 0))
    );
    assert_eq!(
        semantic_site(
            result(&output, 3),
            &immutable,
            immutable.rfind("value").unwrap()
        ),
        Some((8, 2))
    );
    assert!(decoded_semantic(result(&output, 4)).is_empty());
    assert!(decoded_semantic(result(&output, 5)).is_empty());
}

#[test]
fn checked_semantic_tokens_use_qualified_module_identity_without_highlighting_comments() {
    let workspace = Workspace::new("semantic module");
    let library = "def read(value:int)->int{return value}\n";
    let source = "import \"library.bh\" as lib\ndef main(){# lib.read let\nprint(lib.read(1))}\n";
    let main = workspace.write("main.bh", source);
    workspace.write("library.bh", library);
    let uri = file_uri(&main);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(semantic_request(2, &uri));
    let output = exchange(messages);
    let tokens = result(&output, 2);
    assert_eq!(
        semantic_site(tokens, source, source.find("as lib").unwrap() + 3),
        Some((0, 1))
    );
    assert_eq!(
        semantic_site(tokens, source, source.rfind("lib.read").unwrap()),
        Some((0, 0))
    );
    assert_eq!(
        semantic_site(tokens, source, source.rfind("lib.read").unwrap() + 4),
        Some((4, 0))
    );
    assert!(semantic_site(tokens, source, source.find("lib.read").unwrap()).is_none());
}

#[cfg(unix)]
#[test]
fn checked_editor_tools_refuse_duplicate_canonical_document_aliases() {
    let workspace = Workspace::new("editor-duplicate-aliases");
    let disk = "def work(value:int)->int{return value}\ndef main(){print(work(2))}\n";
    let real = workspace.write("z.bh", disk);
    let alias = workspace.file("a.bh");
    std::os::unix::fs::symlink("z.bh", &alias).unwrap();
    let real_uri = file_uri(&real);
    let alias_uri = path_uri(&workspace.path().canonicalize().unwrap().join("a.bh"));
    let alpha = disk.replace("value", "alpha");
    let bravo = disk.replace("value", "bravo");
    let mut messages = start();
    messages.push(open(&alias_uri, &alpha, 1));
    messages.push(open(&real_uri, &bravo, 2));
    for (id, uri, source) in [(2, &alias_uri, &alpha), (5, &real_uri, &bravo)] {
        let call = source.find("work(2)").unwrap();
        messages.push(signature_at(id, uri, source, call + 5));
        messages.push(semantic_request(id + 1, uri));
        messages.push(editor_request(
            id + 2,
            "textDocument/hover",
            uri,
            source,
            call,
        ));
    }
    messages.push(notification(
        "textDocument/didClose",
        document_params(&alias_uri),
    ));
    messages.push(signature_at(
        8,
        &real_uri,
        &bravo,
        bravo.find("work(2)").unwrap() + 5,
    ));
    messages.push(semantic_request(9, &real_uri));
    let output = exchange(messages);
    for id in [2, 4, 5, 7] {
        assert_eq!(result(&output, id), &Value::Null);
    }
    for id in [3, 6] {
        assert!(decoded_semantic(result(&output, id)).is_empty());
    }
    assert_eq!(
        signature_label(result(&output, 8)),
        Some("def work(bravo: int) -> int")
    );
    assert!(!decoded_semantic(result(&output, 9)).is_empty());
    let importing = "import \"z.bh\" as lib\ndef main(){print(lib.work(2))}\n";
    let main = workspace.write("main.bh", importing);
    let main_uri = file_uri(&main);
    let mut messages = start();
    messages.push(open(&alias_uri, &alpha, 1));
    messages.push(open(&real_uri, &bravo, 2));
    messages.push(open(&main_uri, importing, 1));
    messages.push(signature_at(
        2,
        &main_uri,
        importing,
        importing.find("work(2)").unwrap() + 5,
    ));
    messages.push(semantic_request(3, &main_uri));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
}

fn create_editor_lock(entry: &Path, lock_path: &Path) {
    let loaded = bhasa::project::load_unparsed(Some(entry)).unwrap();
    let root = bhasa::package_lock::root_manifest(&loaded).unwrap();
    let lock = bhasa::package_lock::Snapshot::capture(&loaded, root).unwrap();
    fs::write(lock_path, lock.encode()).unwrap();
}

fn locked_editor_queries(path: &Path, source: &str, overlay: Option<(&Path, &str)>) -> Vec<Value> {
    let uri = file_uri(path);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    if let Some((path, source)) = overlay {
        messages.push(open(&file_uri(path), source, 2));
    }
    messages.push(signature_at(
        2,
        &uri,
        source,
        source.find("second(2)").unwrap() + 7,
    ));
    messages.push(semantic_request(3, &uri));
    exchange(messages)
}

#[test]
fn checked_editor_locked_imports_tests_and_unsaved_overlays_keep_checked_identity() {
    let workspace = Workspace::new("locked-editor-graphs");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"editor-lock\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    let main = workspace.write("main.bh", "import \"a.bh\"\ndef main(){print(first())}\n");
    let source = "import \"b.bh\"\ndef first()->int{return second(2)}\n";
    let module = workspace.write("a.bh", source);
    let dependency = workspace.write("b.bh", "def second(value:int)->int{return value}\n");
    create_editor_lock(&main, &workspace.file("bhasa.lock"));
    let unchanged_lock = fs::read(workspace.file("bhasa.lock")).unwrap();
    let output = locked_editor_queries(&module, source, None);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def second(value: int) -> int")
    );
    assert!(!decoded_semantic(result(&output, 3)).is_empty());
    // Exact-entry compilation remains strict; only read-only analysis re-roots.
    assert!(bhasa::project::load_unparsed(Some(&module)).is_err());
    let overlay = "def second(amount:int)->int{return amount}\n";
    let output = locked_editor_queries(&module, source, Some((&dependency, overlay)));
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def second(amount: int) -> int")
    );
    assert!(!decoded_semantic(result(&output, 3)).is_empty());
    fs::create_dir(workspace.file("tests")).unwrap();
    let test_source = "import \"../b.bh\"\ndef sample()->int{return second(2)}\n";
    let test = workspace.write("tests/sample.bh", test_source);
    let output = locked_editor_queries(&test, test_source, None);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def second(value: int) -> int")
    );
    assert!(!decoded_semantic(result(&output, 3)).is_empty());
    assert_eq!(
        fs::read(workspace.file("bhasa.lock")).unwrap(),
        unchanged_lock
    );
}

#[test]
fn checked_editor_locked_graph_refuses_saved_changes_and_unpinned_sources() {
    let workspace = Workspace::new("locked-editor-stale");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"editor-lock\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    let main = workspace.write("main.bh", "import \"a.bh\"\nimport \"leaf.bh\" as leaf\ndef main(){print(first());print(leaf.leaf())}\n");
    let source = "import \"b.bh\"\ndef first()->int{return second(2)}\n";
    let module = workspace.write("a.bh", source);
    workspace.write("b.bh", "def second(value:int)->int{return value}\n");
    let leaf_source =
        "def second(value:int)->int{return value}\ndef leaf()->int{return second(2)}\n";
    let leaf = workspace.write("leaf.bh", leaf_source);
    create_editor_lock(&main, &workspace.file("bhasa.lock"));
    let other = workspace.write("extra.bh", source);
    let output = locked_editor_queries(&other, source, None);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
    let import_free = workspace.write("extra-local.bh", leaf_source);
    let output = locked_editor_queries(&import_free, leaf_source, None);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
    workspace.write("b.bh", "def second(value:int)->int{return value+1}\n");
    let output = locked_editor_queries(&module, source, None);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
    let output = locked_editor_queries(&leaf, leaf_source, None);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
}

#[cfg(unix)]
#[test]
fn checked_editor_entry_symlink_retains_requested_outer_package_lock() {
    let workspace = Workspace::new("locked-editor-entry-alias");
    fs::create_dir(workspace.file("nested")).unwrap();
    workspace.write("bhasa.toml", "[package]\nname=\"outer\"\nversion=\"0.1.0\"\nentry=\"entry.bh\"\n[dependencies]\ninner=\"nested\"\n");
    workspace.write(
        "nested/bhasa.toml",
        "[package]\nname=\"inner\"\nversion=\"0.1.0\"\nentry=\"lib.bh\"\n",
    );
    let source = "import \"@inner\"\ndef main(){print(second(2))}\n";
    workspace.write("nested/main.bh", source);
    workspace.write(
        "nested/lib.bh",
        "def second(value:int)->int{return value}\n",
    );
    let entry = workspace.file("entry.bh");
    std::os::unix::fs::symlink("nested/main.bh", &entry).unwrap();
    create_editor_lock(&entry, &workspace.file("bhasa.lock"));
    assert!(!workspace.file("nested/bhasa.lock").exists());
    // Build the URI from the requested alias, without canonicalizing its final component.
    let uri = format!(
        "file://{}",
        workspace
            .path()
            .canonicalize()
            .unwrap()
            .join("entry.bh")
            .display()
    );
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(signature_at(
        2,
        &uri,
        source,
        source.find("second(2)").unwrap() + 7,
    ));
    messages.push(semantic_request(3, &uri));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def second(value: int) -> int")
    );
    assert!(!decoded_semantic(result(&output, 3)).is_empty());
    workspace.write(
        "nested/lib.bh",
        "def second(value:int)->int{return value+1}\n",
    );
    let mut messages = start();
    messages.push(open(&uri, source, 2));
    messages.push(signature_at(
        2,
        &uri,
        source,
        source.find("second(2)").unwrap() + 7,
    ));
    messages.push(semantic_request(3, &uri));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
}

#[test]
fn checked_alias_editor_distinguishes_unused_aliases_nominal_targets_and_type_names() {
    let source="struct Point{value:int}\ntype Item=Point\ntype Other=Item\ntype Unused=List[int]\ndef type(value:int)->int{return value}\ndef main(){var type=7;let item:Other=Item(1);print(item.value+type)}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        URI,
        source,
        source.find("Other=Item").unwrap() + 6,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        URI,
        source,
        source.find("=Item(1)").unwrap() + 1,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        source,
        source.find("item:Other").unwrap() + 5,
    ));
    messages.push(request(
        5,
        "textDocument/documentSymbol",
        document_params(URI),
    ));
    messages.push(semantic_request(6, URI));
    messages.push(editor_request(
        7,
        "textDocument/hover",
        URI,
        source,
        source.find("type Item").unwrap(),
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), URI, source, "type Item", "Item");
    assert_selection(result(&output, 3), URI, source, "type Item", "Item");
    assert_eq!(hover_text(result(&output, 4)), Some("type Other = Point"));
    let names: Vec<_> = result(&output, 5)
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"Unused"));
    assert!(names.contains(&"Point"));
    assert!(names.contains(&"type"));
    assert_eq!(
        semantic_site(
            result(&output, 6),
            source,
            source.find("type Item").unwrap()
        ),
        Some((9, 0))
    );
    assert_eq!(
        semantic_site(
            result(&output, 6),
            source,
            source.find("Item=Point").unwrap()
        ),
        Some((10, 1))
    );
    assert_eq!(
        semantic_site(
            result(&output, 6),
            source,
            source.find("=Item(1)").unwrap() + 1
        ),
        Some((10, 0))
    );
    assert_eq!(
        semantic_site(
            result(&output, 6),
            source,
            source.find("var type").unwrap() + 4
        ),
        Some((8, 1))
    );
    assert_eq!(result(&output, 7), &Value::Null);
}

#[test]
fn checked_alias_editor_qualified_navigation_and_rename_preserve_namespaces_and_utf16() {
    let workspace = Workspace::new("alias editor graph");
    let library =
        "# 🌍\r\nstruct Point{value:int}\r\ntype Item=Point\r\nprivate type Hidden=buffer\r\n";
    let source="import \"library.bh\" as lib\ndef main(){let item:lib.Item=lib.Item(1);print(item.value)}\n";
    let main = workspace.write("main.bh", source);
    let lib = workspace.write("library.bh", library);
    let main_uri = file_uri(&main);
    let lib_uri = file_uri(&lib);
    let mut messages = start();
    messages.push(open(&main_uri, source, 4));
    messages.push(open(&lib_uri, library, 9));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &main_uri,
        source,
        source.find("lib.Item=lib").unwrap() + 4,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/references",
        &lib_uri,
        library,
        "Item=Point",
        None,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/rename",
        &lib_uri,
        library,
        "Item=Point",
        Some("Entry"),
    ));
    messages.push(editor_request(
        5,
        "textDocument/completion",
        &main_uri,
        source,
        source.find("lib.Item=lib").unwrap() + 4,
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), &lib_uri, library, "type Item", "Item");
    assert_eq!(result(&output, 3).as_array().unwrap().len(), 3);
    let changes = result(&output, 4)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    let main_change = changes
        .iter()
        .find(|change| {
            change
                .get("textDocument")
                .unwrap()
                .get("uri")
                .and_then(Value::as_str)
                == Some(main_uri.as_str())
        })
        .unwrap();
    let renamed = apply_rename(source, main_change);
    assert!(renamed.contains("item:lib.Entry=lib.Entry(1)"));
    assert!(!completion_labels(result(&output, 5)).contains(&"Hidden"));
}

#[test]
fn checked_alias_editor_shadows_and_capture_rejection_preserve_identity() {
    let source = "type Count=int\ndef main(){var Count=2;var value:Count=Count;print(value)}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/references",
        URI,
        source,
        "Count=int",
        None,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        URI,
        source,
        "Count=int",
        Some("Number"),
    ));
    messages.push(editor_request(
        4,
        "textDocument/definition",
        URI,
        source,
        source.find("=Count;").unwrap() + 1,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 2);
    let changes = result(&output, 3)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    let renamed = apply_rename(source, &changes[0]);
    assert!(renamed.contains("var Count = 2") || renamed.contains("var Count=2"));
    assert!(renamed.contains("value:Number=Count"));
    assert_selection(result(&output, 4), URI, source, "var Count", "Count");
    let collision = "type Count=int\ntype Number=bool\ndef main(){var value:Count=1}\n";
    let mut messages = start();
    messages.push(open(URI, collision, 1));
    messages.push(refactor_at(
        2,
        "textDocument/rename",
        URI,
        collision,
        "Count=int",
        Some("Number"),
    ));
    assert_eq!(error_code(response(&exchange(messages), 2)), -32803);
}

#[test]
fn checked_alias_formatter_preserves_declared_spelling_comments_and_canonical_types() {
    let source="# alias docs\nstruct Box[T]{value:T}\ntype Number=int;type Entry=Box[Number,] # target\ndef type(value:int)->int{return value}\ndef main(){let type=2;var entry:Entry=Entry(type);print(entry.value)}\n";
    let formatted = bhasa::format::format_source(source).unwrap();
    assert!(formatted.contains("type Number = int"));
    assert!(formatted.contains("Box[Number,]"));
    assert!(formatted.contains("# target"));
    assert_eq!(bhasa::format::format_source(&formatted).unwrap(), formatted);
    let original = bhasa::check(source).unwrap();
    let updated = bhasa::check(&formatted).unwrap();
    assert_eq!(
        original.program.records.len(),
        updated.program.records.len()
    );
    assert_eq!(
        original.program.functions.len(),
        updated.program.functions.len()
    );
    assert_eq!(
        bhasa::codegen::emit_llvm(&original, "aarch64-apple-darwin").unwrap(),
        bhasa::codegen::emit_llvm(&updated, "aarch64-apple-darwin").unwrap()
    );
}

#[test]
fn checked_alias_editor_nominal_rhs_navigation_and_rename_preserve_alias_declarations() {
    let source="struct Point{value:int}\ntype Item=Point;type Other=Item\ndef main(){var item:Other=Item(1);print(item.value)}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        URI,
        source,
        source.find("=Point").unwrap() + 1,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/rename",
        URI,
        source,
        "Point{",
        Some("Coordinate"),
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), URI, source, "struct Point", "Point");
    let changes = result(&output, 3)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    let renamed = apply_rename(source, &changes[0]);
    assert!(renamed.contains("type Item=Coordinate"));
    assert!(renamed.contains("type Other=Item"));
    assert!(renamed.contains("item:Other=Item(1)"));
}

#[test]
fn checked_alias_editor_same_names_in_different_modules_remain_distinct() {
    let workspace = Workspace::new("alias editor identical names");
    let source="import \"left.bh\" as left\nimport \"right.bh\" as right\ndef main(){var a:left.Item=left.Item(1);var b:right.Item=right.Item(2);print(a.value+b.value)}\n";
    let library = "struct Point{value:int}\ntype Item=Point\n";
    let main = workspace.write("main.bh", source);
    let left = workspace.write("left.bh", library);
    let right = workspace.write("right.bh", library);
    let main_uri = file_uri(&main);
    let left_uri = file_uri(&left);
    let right_uri = file_uri(&right);
    let mut messages = start();
    messages.push(open(&main_uri, source, 1));
    messages.push(open(&left_uri, library, 1));
    messages.push(open(&right_uri, library, 1));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &main_uri,
        source,
        source.find("left.Item=").unwrap() + 5,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        &main_uri,
        source,
        source.find("right.Item=").unwrap() + 6,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/rename",
        &left_uri,
        library,
        "Item=Point",
        Some("Entry"),
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), &left_uri, library, "type Item", "Item");
    assert_selection(result(&output, 3), &right_uri, library, "type Item", "Item");
    let changes = result(&output, 4)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!changes.iter().any(|change| change
        .get("textDocument")
        .unwrap()
        .get("uri")
        .and_then(Value::as_str)
        == Some(right_uri.as_str())));
    let main_change = changes
        .iter()
        .find(|change| {
            change
                .get("textDocument")
                .unwrap()
                .get("uri")
                .and_then(Value::as_str)
                == Some(main_uri.as_str())
        })
        .unwrap();
    let renamed = apply_rename(source, main_change);
    assert!(renamed.contains("left.Entry"));
    assert!(renamed.contains("right.Item"));
}

#[test]
fn checked_alias_editor_generic_type_shadows_never_become_alias_uses() {
    let source="type Value=int\ndef echo[Value](value:Value)->Value{return value}\ndef main(){print(echo[bool](true))}\n";
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(refactor_at(
        2,
        "textDocument/references",
        URI,
        source,
        "Value=int",
        None,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        URI,
        source,
        source.find("value:Value").unwrap() + 6,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/prepareRename",
        URI,
        source,
        "Value=int",
        None,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2).as_array().unwrap().len(), 1);
    assert_eq!(result(&output, 3), &Value::Null);
    // The existing completeness policy refuses unindexed generic template
    // identifiers rather than editing a generic parameter by name.
    assert_eq!(result(&output, 4), &Value::Null);
}

#[test]
fn checked_alias_editor_type_annotations_win_over_value_shadows_only_at_alias_sites() {
    for name in ["Count", "type"] {
        let source = format!(
            "type {name}=int\ndef main(){{var {name}=2;var value:{name}={name};print(value)}}\n"
        );
        let mut messages = start();
        messages.push(open(URI, &source, 1));
        messages.push(editor_request(
            2,
            "textDocument/hover",
            URI,
            &source,
            source.find(&format!("value:{name}")).unwrap() + 6,
        ));
        messages.push(editor_request(
            3,
            "textDocument/hover",
            URI,
            &source,
            source.find(&format!("={name};")).unwrap() + 1,
        ));
        messages.push(refactor_at(
            4,
            "textDocument/references",
            URI,
            &source,
            &format!("{name}=int"),
            None,
        ));
        let output = exchange(messages);
        assert_eq!(
            hover_text(result(&output, 2)),
            Some(format!("type {name} = int").as_str())
        );
        assert_eq!(
            hover_text(result(&output, 3)),
            Some(format!("{name}: int").as_str())
        );
        assert_eq!(result(&output, 4).as_array().unwrap().len(), 2);
    }
}

#[test]
fn checked_alias_editor_enum_owners_keep_alias_identity_and_private_uses_fail_closed() {
    let workspace = Workspace::new("alias enum privacy");
    let library =
        "enum Status{some(value:int);none}\ntype Public=Status\nprivate type Hidden=int\n";
    let source="import \"library.bh\" as lib\ndef main(){var result=lib.Public.some(1);match result{case lib.Public.some(value){print(value)}case lib.Public.none{}}}\n";
    let main = workspace.write("main.bh", source);
    let lib = workspace.write("library.bh", library);
    let main_uri = file_uri(&main);
    let lib_uri = file_uri(&lib);
    let mut messages = start();
    messages.push(open(&main_uri, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &main_uri,
        source,
        source.find("lib.Public.some(1)").unwrap() + 4,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        &main_uri,
        source,
        source.find("lib.Public.some(value)").unwrap() + 11,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        &main_uri,
        source,
        source.find("lib.Public.some(1)").unwrap() + 4,
    ));
    let private = "import \"library.bh\" as lib\ndef main(){var value:lib.Hidden=1}\n";
    messages.push(change(&main_uri, private, 2));
    messages.push(editor_request(
        5,
        "textDocument/hover",
        &main_uri,
        private,
        private.find("Hidden").unwrap(),
    ));
    messages.push(semantic_request(6, &main_uri));
    let output = exchange(messages);
    assert_selection(
        result(&output, 2),
        &lib_uri,
        library,
        "type Public",
        "Public",
    );
    assert_selection(result(&output, 3), &lib_uri, library, "some(value", "some");
    assert_eq!(hover_text(result(&output, 4)), Some("type Public = Status"));
    assert_eq!(result(&output, 5), &Value::Null);
    assert!(decoded_semantic(result(&output, 6)).is_empty());
}

#[test]
fn checked_alias_editor_locked_module_metadata_follows_unsaved_dependency_versions() {
    let workspace = Workspace::new("locked-alias-editor");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"locked-alias\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    let main = workspace.write("main.bh", "import \"a.bh\" as a\ndef main(){}\n");
    let source = "import \"b.bh\" as lib\ndef first(value:lib.Count)->lib.Count{return value}\n";
    let module = workspace.write("a.bh", source);
    let library = "# 🌍\r\ntype Count=int\r\nprivate type Hidden=buffer\r\n";
    let dependency = workspace.write("b.bh", library);
    create_editor_lock(&main, &workspace.file("bhasa.lock"));
    let lock = fs::read(workspace.file("bhasa.lock")).unwrap();
    let module_uri = file_uri(&module);
    let dependency_uri = file_uri(&dependency);
    let overlay = library.replace("Count=int", "Count=bool");
    let mut messages = start();
    messages.push(open(&module_uri, source, 4));
    messages.push(open(&dependency_uri, &overlay, 9));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &module_uri,
        source,
        source.find("lib.Count").unwrap() + 4,
    ));
    messages.push(editor_request(
        3,
        "textDocument/hover",
        &module_uri,
        source,
        source.find("lib.Count").unwrap() + 4,
    ));
    messages.push(semantic_request(4, &module_uri));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        &dependency_uri,
        &overlay,
        "Count=bool",
        Some("Number"),
    ));
    let output = exchange(messages);
    assert_selection(
        result(&output, 2),
        &dependency_uri,
        &overlay,
        "type Count",
        "Count",
    );
    assert_eq!(hover_text(result(&output, 3)), Some("type Count = bool"));
    assert_eq!(
        semantic_site(
            result(&output, 4),
            source,
            source.find("lib.Count").unwrap() + 4
        ),
        Some((10, 0))
    );
    let changes = result(&output, 5)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 2);
    for change in changes {
        let document = change.get("textDocument").unwrap();
        let target = document.get("uri").and_then(Value::as_str).unwrap();
        assert!(target == dependency_uri || target == module_uri);
        let version = if target == dependency_uri { 9 } else { 4 };
        assert_eq!(
            document.get("version").and_then(Value::as_i64),
            Some(version)
        );
    }
    assert_eq!(fs::read(workspace.file("bhasa.lock")).unwrap(), lock);
    assert!(bhasa::project::load_unparsed(Some(&module)).is_err());
}

#[test]
fn checked_alias_editor_locked_saved_unpinned_and_stale_alias_graphs_refuse_metadata() {
    let workspace = Workspace::new("locked-alias-refusal");
    workspace.write(
        "bhasa.toml",
        "[package]\nname=\"locked-alias\"\nversion=\"0.1.0\"\nentry=\"main.bh\"\n",
    );
    let source = "type Count=int\ndef first(value:Count)->Count{return value}\n";
    let leaf = workspace.write("leaf.bh", source);
    let other = workspace.write("other.bh", "def other()->int{return 1}\n");
    let main = workspace.write(
        "main.bh",
        "import \"leaf.bh\" as leaf\nimport \"other.bh\" as other\ndef main(){}\n",
    );
    create_editor_lock(&main, &workspace.file("bhasa.lock"));
    let unpinned = workspace.write("extra.bh", source);
    for path in [&unpinned, &leaf] {
        if path == &leaf {
            fs::write(&other, "def other()->int{return 2}\n").unwrap();
        }
        let uri = file_uri(path);
        let mut messages = start();
        messages.push(open(&uri, source, 1));
        messages.push(editor_request(
            2,
            "textDocument/definition",
            &uri,
            source,
            source.find("value:Count").unwrap() + 6,
        ));
        messages.push(editor_request(
            3,
            "textDocument/hover",
            &uri,
            source,
            source.find("value:Count").unwrap() + 6,
        ));
        messages.push(semantic_request(4, &uri));
        messages.push(request(
            5,
            "textDocument/documentSymbol",
            document_params(&uri),
        ));
        let output = exchange(messages);
        assert_eq!(result(&output, 2), &Value::Null);
        assert_eq!(result(&output, 3), &Value::Null);
        assert!(decoded_semantic(result(&output, 4)).is_empty());
        assert!(result(&output, 5)
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item.get("kind").and_then(Value::as_i64) != Some(26)));
    }
}

#[cfg(unix)]
#[test]
fn checked_alias_editor_duplicate_canonical_overlays_refuse_then_recover_alias_identity() {
    let workspace = Workspace::new("alias-duplicate-overlays");
    let source = "type Count=int\ndef first(value:Count)->Count{return value}\n";
    let real = workspace.write("z.bh", source);
    let alias = workspace.file("a.bh");
    std::os::unix::fs::symlink("z.bh", &alias).unwrap();
    let real_uri = file_uri(&real);
    let alias_uri = path_uri(&workspace.path().canonicalize().unwrap().join("a.bh"));
    let overlay = source.replace("Count=int", "Count=bool");
    let mut messages = start();
    messages.push(open(&real_uri, source, 4));
    messages.push(open(&alias_uri, &overlay, 9));
    messages.push(editor_request(
        2,
        "textDocument/hover",
        &real_uri,
        source,
        source.find("value:Count").unwrap() + 6,
    ));
    messages.push(semantic_request(3, &real_uri));
    messages.push(notification(
        "textDocument/didClose",
        document_params(&alias_uri),
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        &real_uri,
        source,
        source.find("value:Count").unwrap() + 6,
    ));
    messages.push(editor_request(
        5,
        "textDocument/definition",
        &real_uri,
        source,
        source.find("value:Count").unwrap() + 6,
    ));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
    assert_eq!(hover_text(result(&output, 4)), Some("type Count = int"));
    assert_selection(result(&output, 5), &real_uri, source, "type Count", "Count");
}

#[test]
fn checked_alias_rhs_nested_unused_nominals_have_definition_semantics_and_complete_rename() {
    let source="# 🌍\r\nstruct Point{value:int}\r\nstruct Box[T]{value:T}\r\nstruct Pair[A,B]{left:A;right:B}\r\ntype Item=Pair[Box[Point],Point]\r\ndef main(){}\r\n";
    let nested = source.find("Box[Point]").unwrap() + 4;
    let repeated = source.find("],Point]").unwrap() + 2;
    let mut messages = start();
    messages.push(open(URI, source, 9));
    for (id, site) in [
        (2, nested),
        (3, repeated),
        (4, source.find("=Pair").unwrap() + 1),
        (5, source.find("Pair[Box").unwrap() + 5),
    ] {
        messages.push(editor_request(
            id,
            "textDocument/definition",
            URI,
            source,
            site,
        ));
    }
    messages.push(semantic_request(6, URI));
    messages.push(refactor_at(
        7,
        "textDocument/references",
        URI,
        source,
        "Point{",
        None,
    ));
    messages.push(refactor_at(
        8,
        "textDocument/rename",
        URI,
        source,
        "Point{",
        Some("Coordinate"),
    ));
    let output = exchange(messages);
    for id in [2, 3] {
        assert_selection(result(&output, id), URI, source, "struct Point", "Point");
    }
    assert_selection(result(&output, 4), URI, source, "struct Pair", "Pair");
    assert_selection(result(&output, 5), URI, source, "struct Box", "Box");
    for site in [nested, repeated] {
        assert_eq!(
            semantic_site(result(&output, 6), source, site),
            Some((1, 0))
        );
    }
    assert_eq!(result(&output, 7).as_array().unwrap().len(), 3);
    let changes = result(&output, 8)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0]
            .get("textDocument")
            .unwrap()
            .get("version")
            .and_then(Value::as_i64),
        Some(9)
    );
    let renamed = apply_rename(source, &changes[0]);
    assert!(renamed.contains("type Item=Pair[Box[Coordinate],Coordinate]"));
    bhasa::check(&renamed).unwrap();
}

#[test]
fn checked_alias_rhs_chains_preserve_alias_identity_inside_generic_arguments() {
    let source="struct Point{value:int}\nstruct Box[T]{value:T}\ntype Node=Point\ntype Item=Box[Node]\ntype Again=Item\ndef main(){}\n";
    let mut messages = start();
    messages.push(open(URI, source, 2));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        URI,
        source,
        source.find("Box[Node]").unwrap() + 4,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        URI,
        source,
        source.find("=Point").unwrap() + 1,
    ));
    messages.push(semantic_request(4, URI));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        URI,
        source,
        "Node=",
        Some("Vertex"),
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), URI, source, "type Node", "Node");
    assert_selection(result(&output, 3), URI, source, "struct Point", "Point");
    assert_eq!(
        semantic_site(
            result(&output, 4),
            source,
            source.find("Box[Node]").unwrap() + 4
        ),
        Some((10, 0))
    );
    let renamed = apply_rename(
        source,
        &result(&output, 5)
            .get("documentChanges")
            .unwrap()
            .as_array()
            .unwrap()[0],
    );
    assert!(renamed.contains("type Vertex=Point"));
    assert!(renamed.contains("type Item=Box[Vertex]"));
    assert!(renamed.contains("type Again=Item"));
    bhasa::check(&renamed).unwrap();
}

#[test]
fn checked_alias_rhs_qualified_nominals_distinguish_same_names_and_private_local_targets() {
    let workspace = Workspace::new("alias-nested-qualified");
    let left_source = "struct Point{value:int}\nstruct Box[T]{value:T}\n";
    let right_source = "struct Point{value:int}\n";
    let left = workspace.write("left.bh", left_source);
    let right = workspace.write("right.bh", right_source);
    let source="import \"left.bh\" as left\nimport \"right.bh\" as right\nstruct Point{value:int}\nprivate struct Hidden{value:int}\ntype Item=left.Box[right.Point]\nprivate type Local=left.Box[Hidden]\ndef main(){}\n";
    let main = workspace.write("main.bh", source);
    let uri = file_uri(&main);
    let right_uri = file_uri(&right);
    let mut messages = start();
    messages.push(open(&uri, source, 4));
    messages.push(open(&file_uri(&left), left_source, 5));
    messages.push(open(&right_uri, right_source, 6));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &uri,
        source,
        source.find("right.Point]").unwrap() + 6,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        &uri,
        source,
        source.find("Box[Hidden]").unwrap() + 4,
    ));
    messages.push(semantic_request(4, &uri));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        &right_uri,
        right_source,
        "Point{",
        Some("Coordinate"),
    ));
    let output = exchange(messages);
    assert_selection(
        result(&output, 2),
        &right_uri,
        right_source,
        "struct Point",
        "Point",
    );
    assert_selection(result(&output, 3), &uri, source, "struct Hidden", "Hidden");
    assert_eq!(
        semantic_site(
            result(&output, 4),
            source,
            source.find("right.Point]").unwrap() + 6
        ),
        Some((1, 0))
    );
    let changes = result(&output, 5)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 2);
    let change = changes
        .iter()
        .find(|c| {
            c.get("textDocument")
                .unwrap()
                .get("uri")
                .and_then(Value::as_str)
                == Some(uri.as_str())
        })
        .unwrap();
    let renamed = apply_rename(source, change);
    assert!(renamed.contains("left.Box[right.Coordinate]"));
    assert!(renamed.contains("struct Point{"));
    assert!(!changes.iter().any(|c| c
        .get("textDocument")
        .unwrap()
        .get("uri")
        .and_then(Value::as_str)
        == Some(file_uri(&left).as_str())));
    let invalid = source.replace("private type Local", "type Local");
    let mut messages = start();
    messages.push(open(&uri, &invalid, 7));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        &uri,
        &invalid,
        invalid.find("Box[Hidden]").unwrap() + 4,
    ));
    messages.push(semantic_request(3, &uri));
    let output = exchange(messages);
    assert_eq!(result(&output, 2), &Value::Null);
    assert!(decoded_semantic(result(&output, 3)).is_empty());
}

#[test]
fn checked_alias_rhs_generic_named_type_shadow_keeps_conservative_rename_refusal() {
    let source="struct Point{value:int}\nstruct Box[Point]{value:Point}\ntype Item=Box[Point]\ndef main(){}\n";
    let rhs = source.rfind("Box[Point]").unwrap() + 4;
    let formal = source.find("Box[Point]").unwrap() + 4;
    let mut messages = start();
    messages.push(open(URI, source, 1));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        URI,
        source,
        rhs,
    ));
    messages.push(semantic_request(3, URI));
    messages.push(refactor_at(
        4,
        "textDocument/prepareRename",
        URI,
        source,
        "Point{",
        None,
    ));
    messages.push(editor_request(
        5,
        "textDocument/definition",
        URI,
        source,
        formal,
    ));
    let output = exchange(messages);
    assert_selection(result(&output, 2), URI, source, "struct Point", "Point");
    assert_eq!(semantic_site(result(&output, 3), source, rhs), Some((1, 0)));
    assert_eq!(result(&output, 4), &Value::Null);
    assert_eq!(result(&output, 5), &Value::Null);
}

#[test]
fn checked_owned_tooling_signature_and_parameter_hints_keep_ordinary_names() {
    let source="def take(owned value:buffer)->buffer{return move value}\ndef owned(owned:int)->int{return owned}\ndef move(value:int)->int{return value}\ndef main(){let input=buffer(\"x\");let output=take(move input);print(owned(move(1)))}\n";
    let mut messages = start();
    messages.push(open(URI, source, 4));
    messages.push(signature_at(
        2,
        URI,
        source,
        source.find("take(move input)").unwrap() + 5,
    ));
    messages.push(signature_at(
        3,
        URI,
        source,
        source.find("owned(move(1))").unwrap() + 6,
    ));
    messages.push(editor_request(
        4,
        "textDocument/hover",
        URI,
        source,
        source.find("return move value").unwrap() + 12,
    ));
    messages.push(editor_request(
        5,
        "textDocument/completion",
        URI,
        source,
        source.find("return move value").unwrap() + 12,
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def take(owned value: buffer) -> buffer")
    );
    assert_eq!(
        result(&output, 2)
            .get("signatures")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("parameters")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .get("label")
            .and_then(Value::as_str),
        Some("owned value: buffer")
    );
    assert_eq!(
        signature_label(result(&output, 3)),
        Some("def owned(owned: int) -> int")
    );
    assert_eq!(
        result(&output, 4)
            .get("contents")
            .unwrap()
            .get("value")
            .and_then(Value::as_str),
        Some("owned value: buffer")
    );
    assert!(result(&output, 5)
        .get("items")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.get("label").and_then(Value::as_str) == Some("value")
            && v.get("detail").and_then(Value::as_str) == Some("owned value: buffer")));
}
#[test]
fn checked_owned_tooling_marks_only_checked_mode_and_move_tokens() {
    let source="# 🌍\r\ndef owned(owned:int)->int{return owned}\r\ndef move(value:int)->int{return value}\r\ndef take(owned owned:buffer)->buffer{return move owned}\r\ndef main(){let move=buffer(\"x\");let output=take(move move);print(owned(1))}\r\n";
    let mode = source.find("owned owned:buffer").unwrap();
    let binder = mode + 6;
    let transfer = source.find("return move owned").unwrap() + 7;
    let read = transfer + 5;
    let mut messages = start();
    messages.push(open(URI, source, 7));
    messages.push(semantic_request(2, URI));
    for (id, at) in [(3, mode), (5, transfer)] {
        messages.push(editor_request(id, "textDocument/hover", URI, source, at));
        messages.push(editor_request(
            id + 1,
            "textDocument/completion",
            URI,
            source,
            at,
        ));
    }
    messages.push(editor_request(7, "textDocument/hover", URI, source, binder));
    let output = exchange(messages);
    let semantic = result(&output, 2);
    assert_eq!(semantic_site(semantic, source, mode), Some((9, 0)));
    assert_eq!(semantic_site(semantic, source, transfer), Some((9, 0)));
    assert_eq!(semantic_site(semantic, source, binder), Some((7, 1)));
    assert_eq!(semantic_site(semantic, source, read), Some((7, 0)));
    assert_eq!(
        semantic_site(semantic, source, source.find("def owned").unwrap() + 4),
        Some((4, 1))
    );
    assert_eq!(
        semantic_site(semantic, source, source.find("def move").unwrap() + 4),
        Some((4, 1))
    );
    assert_eq!(
        semantic_site(semantic, source, source.find("let move=").unwrap() + 4),
        Some((8, 3))
    );
    for id in [3, 5] {
        assert_eq!(result(&output, id), &Value::Null);
    }
    for id in [4, 6] {
        assert!(result(&output, id)
            .get("items")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        result(&output, 7)
            .get("contents")
            .unwrap()
            .get("value")
            .and_then(Value::as_str),
        Some("owned owned: buffer")
    );
}
#[test]
fn checked_owned_tooling_move_identity_and_rename_preserve_contextual_markers() {
    let source="def take(owned owned:buffer)->buffer{return move owned}\ndef move()->int{return 1}\ndef main(){let source=buffer(\"x\");let target=take(move source);print(move())}\n";
    let operand = source.find("move source").unwrap() + 5;
    let mut messages = start();
    messages.push(open(URI, source, 3));
    messages.push(editor_request(
        2,
        "textDocument/definition",
        URI,
        source,
        operand,
    ));
    messages.push(refactor_at(
        3,
        "textDocument/references",
        URI,
        source,
        "source);",
        None,
    ));
    messages.push(refactor_at(
        4,
        "textDocument/rename",
        URI,
        source,
        "source);",
        Some("payload"),
    ));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        URI,
        source,
        "move()->int",
        Some("ordinary"),
    ));
    messages.push(refactor_at(
        6,
        "textDocument/rename",
        URI,
        source,
        "owned:buffer",
        Some("value"),
    ));
    let output = exchange(messages);
    assert_eq!(
        result(&output, 2).get("range").unwrap().get("start"),
        Some(&source_position(
            source,
            source.find("let source").unwrap() + 4
        ))
    );
    assert_eq!(result(&output, 3).as_array().unwrap().len(), 2);
    for (id, expected) in [
        (4, "take(move payload)"),
        (5, "print(ordinary())"),
        (6, "owned value:buffer"),
    ] {
        let changes = result(&output, id)
            .get("documentChanges")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].get("textDocument").unwrap().get("version"),
            Some(&3_i64.into())
        );
        let repaired = apply_rename(source, &changes[0]);
        assert!(repaired.contains(expected), "{repaired}");
        assert!(bhasa::check(&repaired).is_ok(), "{repaired}");
        if id == 5 {
            assert!(
                repaired.contains("return move owned") && repaired.contains("take(move source)")
            );
        }
    }
}
#[test]
fn checked_owned_tooling_imported_alias_modes_preserve_nested_nominal_metadata() {
    let workspace = Workspace::new("owned-editor-aliases");
    let library="struct Point{value:int}\nstruct Box[T]{value:T}\ntype Item=Box[Point]\ntype Bytes=buffer\ndef take(owned value:Bytes)->Bytes{return move value}\n";
    let lib = workspace.write("lib.bh", library);
    let lib_uri = file_uri(&lib);
    let source="import \"lib.bh\" as lib\ndef main(){let input=buffer(\"x\");let output=lib.take(move input)}\n";
    let main = workspace.write("main.bh", source);
    let uri = file_uri(&main);
    let mut messages = start();
    messages.push(open(&uri, source, 1));
    messages.push(open(&lib_uri, library, 2));
    messages.push(signature_at(
        2,
        &uri,
        source,
        source.find("take(move input)").unwrap() + 5,
    ));
    messages.push(editor_request(
        3,
        "textDocument/definition",
        &lib_uri,
        library,
        library.find("Box[Point]").unwrap() + 4,
    ));
    messages.push(semantic_request(4, &lib_uri));
    messages.push(editor_request(
        5,
        "textDocument/definition",
        &uri,
        source,
        source.find("move input").unwrap() + 5,
    ));
    let output = exchange(messages);
    assert_eq!(
        signature_label(result(&output, 2)),
        Some("def take(owned value: buffer) -> buffer")
    );
    assert_eq!(
        result(&output, 3).get("uri").and_then(Value::as_str),
        Some(lib_uri.as_str())
    );
    assert_eq!(
        result(&output, 3).get("range").unwrap().get("start"),
        Some(&source_position(library, 7))
    );
    assert_eq!(
        semantic_site(
            result(&output, 4),
            library,
            library.find("Box[Point]").unwrap() + 4
        ),
        Some((1, 0))
    );
    assert_eq!(
        result(&output, 5).get("range").unwrap().get("start"),
        Some(&source_position(
            source,
            source.find("let input").unwrap() + 4
        ))
    );
}

#[test]
fn checked_owned_tooling_grouped_moves_keep_exact_keyword_and_operand_identity() {
    let source="def take(owned value:buffer)->buffer{return ((move value))}\ndef move()->int{return 1}\ndef main(){let source=buffer();let result=take((move source));print(move())}\n";
    let keyword = source.find("move source").unwrap();
    let mut messages = start();
    messages.push(open(URI, source, 9));
    messages.push(semantic_request(2, URI));
    messages.push(editor_request(
        3,
        "textDocument/hover",
        URI,
        source,
        keyword,
    ));
    messages.push(editor_request(
        4,
        "textDocument/definition",
        URI,
        source,
        keyword + 5,
    ));
    messages.push(refactor_at(
        5,
        "textDocument/rename",
        URI,
        source,
        "move()->int",
        Some("ordinary"),
    ));
    let output = exchange(messages);
    assert_eq!(
        semantic_site(result(&output, 2), source, keyword),
        Some((9, 0))
    );
    assert_eq!(
        semantic_site(
            result(&output, 2),
            source,
            source.find("move value").unwrap()
        ),
        Some((9, 0))
    );
    assert_eq!(result(&output, 3), &Value::Null);
    assert_selection(result(&output, 4), URI, source, "let source", "source");
    let changes = result(&output, 5)
        .get("documentChanges")
        .unwrap()
        .as_array()
        .unwrap();
    let renamed = apply_rename(source, &changes[0]);
    assert!(
        renamed.contains("print(ordinary())")
            && renamed.contains("move source")
            && renamed.contains("move value")
    );
    assert!(bhasa::check(&renamed).is_ok(), "{renamed}");
}
