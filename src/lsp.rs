//! Synchronous stdio LSP with full document synchronization and UTF-16 positions.
use crate::ast::{self, Block, Expr, ExprKind, ForSource, Program, Span, StmtKind, Type};
use crate::diagnostic::Diagnostic;
use crate::json::{self, Value};
use crate::lexer::{Token, TokenKind};
use crate::project::{self, ModuleImport, SourceFile};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const MAX_HEADER: usize = 8 * 1024;
const MAX_DOCUMENTS: usize = 128;
const MAX_OPEN_BYTES: usize = 32 * 1024 * 1024;

/// Run on the process streams. Protocol failures are reported only to stderr.
pub fn run() -> i32 {
    serve(io::stdin().lock(), io::stdout().lock())
}

/// Serve one connection. Exposed for protocol tests and embedders.
/// Exit is successful only after a shutdown request followed by exit notification.
pub fn serve<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> i32 {
    let mut server = Server::default();
    loop {
        let body = match read_frame(&mut reader) {
            Ok(Some(body)) => body,
            Ok(None) => return 1,
            Err(error) => {
                eprintln!("bhasa lsp: {error}");
                return 1;
            }
        };
        let value = match std::str::from_utf8(&body)
            .map_err(|e| e.to_string())
            .and_then(json::parse)
        {
            Ok(value) => value,
            Err(error) => {
                if write_frame(&mut writer, &failure(Value::Null, -32700, &error)).is_err() {
                    return 1;
                }
                continue;
            }
        };
        match server.handle(value, &mut writer) {
            Ok(Some(status)) => return status,
            Ok(None) => {}
            Err(error) => {
                eprintln!("bhasa lsp: {error}");
                return 1;
            }
        }
    }
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut header = Vec::new();
    let mut length = None;
    let mut total = 0;
    loop {
        header.clear();
        let read = reader
            .take((MAX_HEADER + 1 - total) as u64)
            .read_until(b'\n', &mut header)?;
        if read == 0 && total == 0 {
            return Ok(None);
        }
        total += read;
        if total > MAX_HEADER || !header.ends_with(b"\r\n") {
            return Err(invalid("invalid or oversized LSP header"));
        }
        if header == b"\r\n" {
            break;
        }
        if !header.is_ascii() {
            return Err(invalid("LSP headers must be ASCII"));
        }
        let line = std::str::from_utf8(&header[..header.len() - 2])
            .map_err(|_| invalid("invalid header"))?;
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("invalid LSP header field"))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            let value = value.trim();
            if length.is_some() || value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
                return Err(invalid("invalid or duplicate Content-Length"));
            }
            let value: usize = value
                .parse()
                .map_err(|_| invalid("invalid Content-Length"))?;
            if value > MAX_MESSAGE {
                return Err(invalid("Content-Length exceeds 4 MiB"));
            }
            length = Some(value);
        } else if name.eq_ignore_ascii_case("Content-Type") {
            for parameter in value.split(';').skip(1) {
                if let Some((key, encoding)) = parameter.trim().split_once('=') {
                    if key.trim().eq_ignore_ascii_case("charset")
                        && !matches!(
                            encoding.trim().to_ascii_lowercase().as_str(),
                            "utf-8" | "utf8"
                        )
                    {
                        return Err(invalid("only UTF-8 LSP messages are supported"));
                    }
                }
            }
        }
    }
    let mut body = vec![0; length.ok_or_else(|| invalid("missing Content-Length"))?];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn write_frame(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let mut body = message.to_string();
    if body.len() > MAX_MESSAGE {
        let id = message
            .get("id")
            .ok_or_else(|| invalid("outgoing notification exceeds 4 MiB"))?;
        body = failure(
            id.clone(),
            -32803,
            "response exceeds the 4 MiB message limit",
        )
        .to_string();
        if body.len() > MAX_MESSAGE {
            return Err(invalid("outgoing response ID exceeds message limit"));
        }
    }
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()
}
fn success(id: Value, result: Value) -> Value {
    Value::object([("jsonrpc", "2.0".into()), ("id", id), ("result", result)])
}
fn failure(id: Value, code: i64, message: &str) -> Value {
    Value::object([
        ("jsonrpc", "2.0".into()),
        ("id", id),
        (
            "error",
            Value::object([("code", code.into()), ("message", message.into())]),
        ),
    ])
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

#[derive(Clone)]
struct Document {
    source: String,
    version: i64,
    path: Option<PathBuf>,
}
#[derive(Clone, Default)]
struct Server {
    initialized: bool,
    shutdown: bool,
    documents: BTreeMap<String, Document>,
    published: BTreeSet<String>,
}
impl Server {
    fn handle(&mut self, message: Value, writer: &mut impl Write) -> io::Result<Option<i32>> {
        let id = message.get("id").cloned();
        let valid_id = id.as_ref().is_none_or(|id| {
            matches!(id, Value::String(_))
                || id.as_i64().is_some_and(|id| i32::try_from(id).is_ok())
        });
        let method = text(&message, "method");
        if text(&message, "jsonrpc") != Some("2.0") || method.is_none() || !valid_id {
            write_frame(
                writer,
                &failure(
                    if valid_id {
                        id.unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    },
                    -32600,
                    "invalid JSON-RPC request",
                ),
            )?;
            return Ok(None);
        }
        let method = method.unwrap();
        if method == "exit" && id.is_none() {
            return Ok(Some(if self.shutdown { 0 } else { 1 }));
        }
        if self.shutdown {
            if let Some(id) = id {
                write_frame(writer, &failure(id, -32600, "server has shut down"))?;
            }
            return Ok(None);
        }
        if !self.initialized && method != "initialize" {
            if let Some(id) = id {
                write_frame(writer, &failure(id, -32002, "server is not initialized"))?;
            }
            return Ok(None);
        }
        let params = message.get("params").unwrap_or(&Value::Null);
        if let Some(id) = id {
            let result = match method {
                "initialize" if !self.initialized => {
                    self.initialized = true;
                    Ok(Value::object([
                        (
                            "capabilities",
                            Value::object([
                                ("positionEncoding", "utf-16".into()),
                                (
                                    "textDocumentSync",
                                    Value::object([
                                        ("openClose", true.into()),
                                        ("change", 1_i64.into()),
                                        ("save", true.into()),
                                    ]),
                                ),
                                ("documentFormattingProvider", true.into()),
                                ("documentSymbolProvider", true.into()),
                                ("definitionProvider", true.into()),
                                ("referencesProvider", true.into()),
                                (
                                    "renameProvider",
                                    Value::object([("prepareProvider", true.into())]),
                                ),
                                ("hoverProvider", true.into()),
                                (
                                    "completionProvider",
                                    Value::object([
                                        ("triggerCharacters", Value::Array(vec![".".into()])),
                                        ("resolveProvider", false.into()),
                                    ]),
                                ),
                            ]),
                        ),
                        (
                            "serverInfo",
                            Value::object([
                                ("name", "bhasa".into()),
                                ("version", env!("CARGO_PKG_VERSION").into()),
                            ]),
                        ),
                    ]))
                }
                "initialize" => Err((-32600, "initialize may only be called once".to_owned())),
                "shutdown" => {
                    self.shutdown = true;
                    Ok(Value::Null)
                }
                "textDocument/formatting"
                | "textDocument/documentSymbol"
                | "textDocument/definition"
                | "textDocument/references"
                | "textDocument/prepareRename"
                | "textDocument/rename"
                | "textDocument/completion"
                | "textDocument/hover" => self.document_request(method, params),
                _ => Err((-32601, format!("unsupported method '{method}'"))),
            };
            write_frame(
                writer,
                &match result {
                    Ok(result) => success(id, result),
                    Err((code, message)) => failure(id, code, &message),
                },
            )?;
        } else {
            let changed = match method {
                "textDocument/didOpen" => self.open(params),
                "textDocument/didChange" => self.change(params),
                "textDocument/didClose" => {
                    let uri = params.get("textDocument").and_then(|doc| text(doc, "uri"));
                    uri.is_some_and(|uri| self.documents.remove(uri).is_some())
                }
                "textDocument/didSave" => true,
                _ => false,
            };
            if changed {
                self.publish(writer)?;
            }
        }
        Ok(None)
    }
    fn open(&mut self, params: &Value) -> bool {
        let Some(doc) = params.get("textDocument") else {
            return false;
        };
        let (Some(uri), Some(source), Some(version)) = (
            text(doc, "uri"),
            text(doc, "text"),
            doc.get("version").and_then(Value::as_i64),
        ) else {
            return false;
        };
        if uri.len() > 16 * 1024
            || self.documents.contains_key(uri)
            || self.documents.len() >= MAX_DOCUMENTS
            || self.total_bytes() + source.len() > MAX_OPEN_BYTES
        {
            return false;
        }
        self.documents.insert(
            uri.to_owned(),
            Document {
                source: source.to_owned(),
                version,
                path: uri_path(uri),
            },
        );
        true
    }
    fn total_bytes(&self) -> usize {
        self.documents.values().map(|doc| doc.source.len()).sum()
    }
    fn change(&mut self, params: &Value) -> bool {
        let Some(doc) = params.get("textDocument") else {
            return false;
        };
        let (Some(uri), Some(version), Some(changes)) = (
            text(doc, "uri"),
            doc.get("version").and_then(Value::as_i64),
            params.get("contentChanges").and_then(Value::as_array),
        ) else {
            return false;
        };
        let Some(existing) = self.documents.get(uri) else {
            return false;
        };
        if version <= existing.version
            || changes.is_empty()
            || changes
                .iter()
                .any(|change| change.get("range").is_some() || text(change, "text").is_none())
        {
            return false;
        }
        let source = text(changes.last().unwrap(), "text").unwrap();
        if self.total_bytes() - existing.source.len() + source.len() > MAX_OPEN_BYTES {
            return false;
        }
        let existing = self.documents.get_mut(uri).unwrap();
        existing.source = source.to_owned();
        existing.version = version;
        true
    }
    fn document_request(&self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params
            .get("textDocument")
            .and_then(|doc| text(doc, "uri"))
            .ok_or((-32602, "expected textDocument.uri".to_owned()))?;
        let doc = self
            .documents
            .get(uri)
            .ok_or((-32602, "document must be open".to_owned()))?;
        match method {
            "textDocument/formatting" => {
                let formatted = crate::format::format_source(&doc.source).map_err(|errors| {
                    (
                        -32803,
                        errors
                            .iter()
                            .map(|error| error.message.as_str())
                            .collect::<Vec<_>>()
                            .join("; "),
                    )
                })?;
                Ok(Value::Array(if formatted == doc.source {
                    Vec::new()
                } else {
                    vec![Value::object([
                        ("range", range(&doc.source, Span::new(0, doc.source.len()))),
                        ("newText", formatted.into()),
                    ])]
                }))
            }
            "textDocument/documentSymbol" => Ok(Value::Array(
                local_symbols(uri, &doc.source)
                    .into_iter()
                    .map(|symbol| symbol.document_symbol())
                    .collect(),
            )),
            _ => {
                let pos = params
                    .get("position")
                    .ok_or((-32602, "expected position".to_owned()))?;
                let offset = position_offset(&doc.source, pos)
                    .ok_or((-32602, "invalid UTF-16 position".to_owned()))?;
                if matches!(method, "textDocument/completion" | "textDocument/hover") {
                    return Ok(self.editor_query(
                        uri,
                        doc,
                        offset,
                        method == "textDocument/completion",
                    ));
                }
                let analysis = self.analyze(uri, doc);
                if matches!(
                    method,
                    "textDocument/references"
                        | "textDocument/prepareRename"
                        | "textDocument/rename"
                ) {
                    return self.refactor(method, params, uri, offset, analysis);
                }
                if let Some(symbol) = declaration_at(&analysis.symbols, uri, offset) {
                    return Ok(symbol.location());
                }
                let targets: BTreeSet<_> = analysis
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.uri == uri
                            && reference.span.start <= offset
                            && offset < reference.span.end
                    })
                    .map(|reference| reference.target.as_str())
                    .collect();
                if !targets.is_empty() {
                    let mut locations: Vec<_> = targets
                        .iter()
                        .filter_map(|target| find_symbol(&analysis.symbols, target))
                        .map(Symbol::location)
                        .collect();
                    return Ok(match locations.len() {
                        0 => Value::Null,
                        1 => locations.remove(0),
                        _ => Value::Array(locations),
                    });
                }
                // Preserve global-function navigation in incomplete programs, but
                // never infer member-call targets without a successful check.
                let symbol = called_name(&doc.source, offset).and_then(|name| {
                    analysis.symbols.iter().find(|symbol| {
                        symbol.kind == 12
                            && symbol.name == name
                            && (symbol.uri == uri || analysis.global_fallback)
                    })
                });
                Ok(symbol.map_or(Value::Null, Symbol::location))
            }
        }
    }
    fn publish(&mut self, writer: &mut impl Write) -> io::Result<()> {
        let mut diagnostics: BTreeMap<String, Vec<Value>> = self
            .documents
            .keys()
            .map(|uri| (uri.clone(), Vec::new()))
            .collect();
        let analyses: BTreeMap<_, _> = self
            .documents
            .iter()
            .map(|(uri, doc)| (uri, self.analyze(uri, doc)))
            .collect();
        let dependencies: BTreeSet<_> = analyses
            .values()
            .flat_map(|analysis| analysis.dependencies.iter().cloned())
            .collect();
        for (uri, analysis) in analyses {
            // A module shares its importing program's globals. Checking it separately
            // would report missing names that the importing program supplies.
            if dependencies.contains(uri) {
                continue;
            }
            for (target, errors) in analysis.diagnostics {
                let all = diagnostics.entry(target).or_default();
                for error in errors {
                    if !all.contains(&error) {
                        all.push(error);
                    }
                }
            }
        }
        for old in &self.published {
            diagnostics.entry(old.clone()).or_default();
        }
        self.published = diagnostics
            .iter()
            .filter(|(uri, values)| !values.is_empty() || self.documents.contains_key(*uri))
            .map(|(uri, _)| uri.clone())
            .collect();
        for (uri, errors) in diagnostics {
            let version = self.documents.get(&uri).map(|doc| doc.version);
            let overhead = publication(&uri, version, Vec::new()).to_string().len();
            let errors = bounded_diagnostics(errors, MAX_MESSAGE.saturating_sub(overhead));
            write_frame(writer, &publication(&uri, version, errors))?;
        }
        Ok(())
    }
    fn analyze(&self, uri: &str, doc: &Document) -> Analysis {
        let mut analysis = Analysis {
            uri: uri.to_owned(),
            ..Analysis::default()
        };
        let current_imports = match imports(&doc.source) {
            Ok(imports) => imports,
            Err(errors) => {
                analysis.add(uri, &doc.source, errors);
                return analysis;
            }
        };
        let loaded = doc
            .path
            .as_ref()
            .map(|path| project::load_unparsed(Some(path)));
        if let Some(Ok(mut loaded)) = loaded {
            analysis.dependencies = loaded
                .files
                .iter()
                .map(|file| self.file_uri(&file.path))
                .filter(|file_uri| file_uri != uri)
                .collect();
            let mut base = 0;
            for file in &mut loaded.files {
                let overlay = self
                    .documents
                    .iter()
                    .find(|(_, open)| open.path.as_ref() == Some(&file.path));
                if let Some((open_uri, open)) = overlay {
                    let disk_imports = imports(&file.source).unwrap_or_default();
                    let mut open_imports = match imports(&open.source) {
                        Ok(imports) => imports,
                        Err(errors) => {
                            analysis.add(open_uri, &open.source, errors);
                            return analysis;
                        }
                    };
                    if disk_imports
                        .iter()
                        .map(|import| &import.path)
                        .ne(open_imports.iter().map(|import| &import.path))
                    {
                        return self.local_analysis(
                            uri,
                            doc,
                            &current_imports,
                            Some("Save changed import declarations to refresh the module graph"),
                        );
                    }
                    for (open_import, disk_import) in open_imports.iter_mut().zip(&file.imports) {
                        open_import.target = disk_import.target.clone();
                    }
                    file.source = open.source.clone();
                    file.import_spans = open_imports.iter().map(|import| import.span).collect();
                    file.imports = open_imports;
                }
                file.base = base;
                base += file.source.len() + 1;
                if base > 64 * 1024 * 1024 {
                    return self.local_analysis(
                        uri,
                        doc,
                        &current_imports,
                        Some("Module graph exceeds the 64 MiB source limit"),
                    );
                }
                let file_uri = self.file_uri(&file.path);
                analysis
                    .symbols
                    .extend(local_symbols(&file_uri, &file.source));
            }
            // Parse each file before merging, preserving module boundaries and
            // remapping the record IDs used by checked method receivers.
            analysis.global_fallback = crate::modules::expand_files(&loaded.files)
                .is_ok_and(|expanded| expanded.is_identity());
            analysis.files = loaded.files.clone();
            match check_files(&loaded.files) {
                Ok(checked) => {
                    analysis.references = checked_references(
                        &checked.program,
                        &loaded.files,
                        |path| self.file_uri(path),
                        &mut analysis.symbols,
                    );
                    analysis.program = Some(checked.program);
                }
                Err(errors) => {
                    for diagnostic in errors {
                        let located = project::locate(&loaded.files, &diagnostic);
                        analysis.add(
                            &self.file_uri(&located.path),
                            &located.source,
                            vec![located.diagnostic],
                        );
                    }
                }
            }
            return analysis;
        }
        if let Some(Err(errors)) = loaded {
            // The disk loader cannot traverse a file that has unsaved import or syntax
            // repairs. Never present diagnostics for that stale disk text as editor text.
            let stale = errors.iter().any(|error| {
                self.documents.values().any(|open| {
                    open.path.as_ref() == Some(&error.path) && open.source != error.source
                })
            });
            if !stale && !current_imports.is_empty() {
                for error in errors {
                    analysis.add(
                        &self.file_uri(&error.path),
                        &error.source,
                        vec![error.diagnostic],
                    );
                }
                analysis.symbols = local_symbols(uri, &doc.source);
                return analysis;
            }
        }
        self.local_analysis(
            uri,
            doc,
            &current_imports,
            if current_imports.is_empty() {
                None
            } else {
                Some("Save this document and its imports to enable module type checking")
            },
        )
    }
    fn local_analysis(
        &self,
        uri: &str,
        doc: &Document,
        imports: &[ModuleImport],
        notice: Option<&str>,
    ) -> Analysis {
        let mut analysis = Analysis {
            uri: uri.to_owned(),
            symbols: local_symbols(uri, &doc.source),
            ..Analysis::default()
        };
        let source = blank_imports(&doc.source, imports);
        // Unknown imported declarations would create false semantic errors while the
        // graph is unavailable. Syntax errors remain useful until the graph is saved.
        let errors = if notice.is_some() {
            symbol_program(&syntax_source(&doc.source, imports)).err()
        } else {
            match crate::check(&source) {
                Ok(checked) => {
                    let file = SourceFile {
                        path: PathBuf::new(),
                        source: doc.source.clone(),
                        base: 0,
                        import_spans: imports.iter().map(|import| import.span).collect(),
                        imports: Vec::new(),
                    };
                    analysis.references = checked_references(
                        &checked.program,
                        std::slice::from_ref(&file),
                        |_| uri.to_owned(),
                        &mut analysis.symbols,
                    );
                    analysis.files = vec![file];
                    analysis.program = Some(checked.program);
                    None
                }
                Err(errors) => Some(errors),
            }
        };
        if let Some(errors) = errors {
            analysis.add(uri, &doc.source, errors);
        }
        if let Some(message) = notice {
            let mut value = diagnostic(
                &doc.source,
                &Diagnostic::new(
                    "LSP001",
                    message,
                    imports
                        .first()
                        .map_or(Span::default(), |import| import.span),
                ),
            );
            if let Value::Object(fields) = &mut value {
                fields.insert("severity".into(), 2_i64.into());
            }
            analysis
                .diagnostics
                .entry(uri.to_owned())
                .or_default()
                .push(value);
        }
        if analysis.files.is_empty() {
            analysis.files.push(SourceFile {
                path: doc.path.clone().unwrap_or_default(),
                source: doc.source.clone(),
                base: 0,
                import_spans: imports.iter().map(|import| import.span).collect(),
                imports: imports.to_vec(),
            });
        }
        analysis
    }
    fn file_uri(&self, path: &Path) -> String {
        self.documents
            .iter()
            .find(|(_, doc)| doc.path.as_deref() == Some(path))
            .map(|(uri, _)| uri.clone())
            .unwrap_or_else(|| path_uri(path))
    }
}

// Refactors use checked declaration anchors, never the completion recovery tree.
// A name is not an identity: two equal spellings in nested scopes stay distinct.
type Anchor = (String, usize, usize);
type ReferenceIdentity = (Anchor, Anchor);
type RenameEdits = BTreeMap<String, Vec<Span>>;

fn anchor(symbol: &Symbol) -> Anchor {
    (
        symbol.uri.clone(),
        symbol.selection_span.start,
        symbol.selection_span.end,
    )
}
fn rename_error(message: impl Into<String>) -> (i64, String) {
    (-32803, message.into())
}
fn valid_rename_name(name: &str) -> bool {
    if name.len() > 256 {
        return false;
    }
    let Ok(tokens) = crate::lexer::lex(name) else {
        return false;
    };
    matches!(tokens.as_slice(), [Token { kind: TokenKind::Ident(value), span }, Token { kind: TokenKind::Eof, .. }] if value == name && span.start == 0 && span.end == name.len())
        && !crate::builtins::is_builtin(name)
        && !matches!(
            name,
            "private"
                | "import"
                | "as"
                | "self"
                | "print"
                | "spawn"
                | "join"
                | "range"
                | "List"
                | "Map"
                | "task"
                | "tcp_socket"
                | "tcp_listener"
                | "tls_socket"
        )
        && !["bhasa_rt_", "__bhg_", "__bhm_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}
fn bounded_refactor(value: Value) -> Result<Value, (i64, String)> {
    if value.to_string().len() > MAX_MESSAGE / 2 {
        Err(rename_error("Reference result exceeds the response limit"))
    } else {
        Ok(value)
    }
}
fn reference_identities(analysis: &Analysis) -> BTreeSet<ReferenceIdentity> {
    analysis
        .references
        .iter()
        .filter_map(|reference| {
            let target = find_symbol(&analysis.symbols, &reference.target)?;
            Some((
                (
                    reference.uri.clone(),
                    reference.span.start,
                    reference.span.end,
                ),
                anchor(target),
            ))
        })
        .collect()
}
fn rename_offsets(edits: &RenameEdits, replacement: &str) -> BTreeMap<String, Vec<(usize, isize)>> {
    edits
        .iter()
        .map(|(uri, changes)| {
            let mut delta = 0;
            let offsets = changes
                .iter()
                .map(|change| {
                    delta += replacement.len() as isize - (change.end - change.start) as isize;
                    (change.end, delta)
                })
                .collect();
            (uri.clone(), offsets)
        })
        .collect()
}
fn moved_anchor(value: &Anchor, offsets: &BTreeMap<String, Vec<(usize, isize)>>) -> Anchor {
    let move_offset = |position: usize| {
        let delta = offsets
            .get(&value.0)
            .and_then(|changes| {
                changes
                    .partition_point(|(end, _)| *end <= position)
                    .checked_sub(1)
                    .map(|index| changes[index].1)
            })
            .unwrap_or(0);
        (position as isize + delta) as usize
    };
    (value.0.clone(), move_offset(value.1), move_offset(value.2))
}

impl Server {
    fn refactor(
        &self,
        method: &str,
        params: &Value,
        uri: &str,
        offset: usize,
        current: Analysis,
    ) -> Result<Value, (i64, String)> {
        let empty = || {
            if method == "textDocument/references" {
                Value::Array(Vec::new())
            } else {
                Value::Null
            }
        };
        if current.program.is_none() {
            return if method == "textDocument/rename" {
                Err(rename_error(
                    "Rename requires a completely checked document graph",
                ))
            } else {
                Ok(empty())
            };
        }
        let keys: BTreeSet<_> = current
            .references
            .iter()
            .filter(|reference| {
                reference.uri == uri
                    && reference.span.start <= offset
                    && offset < reference.span.end
            })
            .map(|reference| reference.target.clone())
            .collect();
        if keys.len() != 1 {
            return if method == "textDocument/rename" {
                Err(rename_error(
                    "No unambiguous checked symbol at this position",
                ))
            } else {
                Ok(empty())
            };
        }
        let Some(target) = find_symbol(&current.symbols, keys.iter().next().unwrap()).cloned()
        else {
            return Ok(empty());
        };
        let target_anchor = anchor(&target);
        // All open roots participate, including importers of an open library.
        // Unrelated broken graphs do not disable refactoring this graph.
        let mut graphs = vec![current];
        let mut total = graphs[0]
            .files
            .iter()
            .map(|file| file.source.len())
            .sum::<usize>();
        for (other_uri, doc) in &self.documents {
            if other_uri == uri {
                continue;
            }
            let analysis = self.analyze(other_uri, doc);
            let unresolved_imports = analysis.program.is_none()
                && imports(&doc.source).is_ok_and(|imports| !imports.is_empty())
                && (analysis.files.is_empty()
                    || analysis.files.iter().any(|file| {
                        file.imports
                            .iter()
                            .any(|import| import.target.as_os_str().is_empty())
                    }));
            if method != "textDocument/references" && target.kind != 13 && unresolved_imports {
                return if method == "textDocument/prepareRename" {
                    Ok(Value::Null)
                } else {
                    Err(rename_error(
                        "Save and check all open import graphs before renaming a declaration",
                    ))
                };
            }
            let contains = analysis
                .files
                .iter()
                .any(|file| self.file_uri(&file.path) == target.uri)
                || analysis
                    .symbols
                    .iter()
                    .any(|symbol| symbol.uri == target.uri);
            if !contains {
                continue;
            }
            if analysis.program.is_none() {
                return Err(rename_error(
                    "An open importing graph is not completely checked",
                ));
            }
            total += analysis
                .files
                .iter()
                .map(|file| file.source.len())
                .sum::<usize>();
            if total > 64 * 1024 * 1024 {
                return Err(rename_error(
                    "Open reference graphs exceed the 64 MiB analysis limit",
                ));
            }
            graphs.push(analysis);
        }
        let identities: BTreeSet<_> = graphs.iter().flat_map(reference_identities).collect();
        if identities.len() > 65_536 {
            return Err(rename_error(
                "Reference graph exceeds the 65536-location limit",
            ));
        }
        let mut targets_by_site = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        for (site, declaration) in &identities {
            if targets_by_site
                .insert(site.clone(), declaration)
                .is_some_and(|previous| previous != declaration)
            {
                ambiguous.insert(site.clone());
            }
        }
        let sites: BTreeSet<_> = identities
            .iter()
            .filter(|(_, declaration)| declaration == &target_anchor)
            .map(|(site, _)| site.clone())
            .collect();
        let source_at = |file_uri: &str| -> Option<&str> {
            if let Some(doc) = self.documents.get(file_uri) {
                return Some(&doc.source);
            }
            graphs
                .iter()
                .flat_map(|graph| &graph.files)
                .find(|file| self.file_uri(&file.path) == file_uri)
                .map(|file| file.source.as_str())
        };
        if method == "textDocument/references" {
            let include = match params
                .get("context")
                .and_then(|value| value.get("includeDeclaration"))
            {
                Some(Value::Bool(value)) => *value,
                _ => return Err((-32602, "expected context.includeDeclaration".into())),
            };
            return bounded_refactor(Value::Array(
                sites
                    .iter()
                    .filter(|site| include || *site != &target_anchor)
                    .filter_map(|site| {
                        Some(Value::object([
                            ("uri", site.0.clone().into()),
                            (
                                "range",
                                range(source_at(&site.0)?, Span::new(site.1, site.2)),
                            ),
                        ]))
                    })
                    .collect(),
            ));
        }
        // Library ABI names, synthetic names and uninstantiated templates cannot
        // be safely renamed using this checked graph alone.
        let foreign = graphs.iter().any(|graph| {
            graph.program.as_ref().is_some_and(|program| {
                program.functions.iter().any(|function| {
                    function.body.is_none()
                        && graph.files.iter().any(|file| {
                            let file_uri = if file.path.as_os_str().is_empty() {
                                graph.uri.clone()
                            } else {
                                self.file_uri(&file.path)
                            };
                            file_uri == target.uri
                                && function.span.start <= file.base + target.selection_span.start
                                && file.base + target.selection_span.end <= function.span.end
                        })
                })
            })
        });
        // Existing legal spellings can include contextual type names (a local
        // `task`) or builtin-like member names. Restrictions on a proposed new
        // name must not prevent renaming those checked declarations away.
        let unsupported = (target.kind == 13 && target.name == "self")
            || (target.kind == 12 && target.name == "main")
            || foreign
            || sites.is_empty()
            || sites.iter().any(|site| ambiguous.contains(site));
        if unsupported {
            return if method == "textDocument/prepareRename" {
                Ok(Value::Null)
            } else {
                Err(rename_error("This symbol has no safe unambiguous rename"))
            };
        }
        let mut edits: RenameEdits = BTreeMap::new();
        for (file_uri, start, end) in &sites {
            let Some(doc) = self.documents.get(file_uri) else {
                return if method == "textDocument/prepareRename" {
                    Ok(Value::Null)
                } else {
                    Err(rename_error(format!(
                        "Open every affected file before renaming: {file_uri}"
                    )))
                };
            };
            if doc.source.get(*start..*end) != Some(target.name.as_str()) {
                return Err(rename_error(
                    "A reference does not map to a complete original identifier",
                ));
            }
            if let Some(path) = &doc.path {
                if self
                    .documents
                    .values()
                    .filter(|other| other.path.as_ref() == Some(path))
                    .count()
                    != 1
                {
                    return Err(rename_error(
                        "Multiple open documents refer to the same file",
                    ));
                }
            }
            edits
                .entry(file_uri.clone())
                .or_default()
                .push(Span::new(*start, *end));
        }
        // Conservative completeness check. In particular, an unused generic
        // body or nested generic type spelling must never be silently missed.
        for graph in &graphs {
            for file in &graph.files {
                let file_uri = if file.path.as_os_str().is_empty() {
                    graph.uri.clone()
                } else {
                    self.file_uri(&file.path)
                };
                for token in crate::lexer::lex(&file.source).unwrap_or_default() {
                    if matches!(&token.kind, TokenKind::Ident(name) if name == &target.name)
                        && !targets_by_site.contains_key(&(
                            file_uri.clone(),
                            token.span.start,
                            token.span.end,
                        ))
                    {
                        return if method == "textDocument/prepareRename" {
                            Ok(Value::Null)
                        } else {
                            Err(rename_error(
                                "Some matching identifiers have no proven checked identity",
                            ))
                        };
                    }
                }
            }
        }
        if method == "textDocument/prepareRename" {
            let site = sites
                .iter()
                .find(|site| site.0 == uri && site.1 <= offset && offset < site.2)
                .unwrap();
            return Ok(Value::object([
                (
                    "range",
                    range(source_at(uri).unwrap(), Span::new(site.1, site.2)),
                ),
                ("placeholder", target.name.into()),
            ]));
        }
        let name = text(params, "newName").ok_or((-32602, "expected newName".into()))?;
        if !valid_rename_name(name) {
            return Err((
                -32602,
                "newName must be a nonreserved Bhasa identifier".into(),
            ));
        }
        for changes in edits.values_mut() {
            changes.sort_by_key(|span| span.start);
            if changes.windows(2).any(|pair| pair[0].end > pair[1].start) {
                return Err(rename_error("Overlapping rename edits"));
            }
        }
        let offsets = rename_offsets(&edits, name);
        for graph in &graphs {
            let mut files = graph.files.clone();
            let mut base = 0;
            for file in &mut files {
                let file_uri = if file.path.as_os_str().is_empty() {
                    graph.uri.clone()
                } else {
                    self.file_uri(&file.path)
                };
                if let Some(changes) = edits.get(&file_uri) {
                    let removed = changes
                        .iter()
                        .map(|span| span.end - span.start)
                        .sum::<usize>();
                    let length = name
                        .len()
                        .checked_mul(changes.len())
                        .and_then(|added| {
                            file.source.len().checked_sub(removed)?.checked_add(added)
                        })
                        .filter(|length| *length <= 64 * 1024 * 1024)
                        .ok_or_else(|| rename_error("Renamed source exceeds the 64 MiB limit"))?;
                    let mut updated = String::with_capacity(length);
                    let mut cursor = 0;
                    for span in changes {
                        updated.push_str(&file.source[cursor..span.start]);
                        updated.push_str(name);
                        cursor = span.end;
                    }
                    updated.push_str(&file.source[cursor..]);
                    file.source = updated;
                }
                let mut imports = imports(&file.source)
                    .map_err(|_| rename_error("Rename damaged an import declaration"))?;
                if imports.len() != file.imports.len() {
                    return Err(rename_error("Rename changed the import graph"));
                }
                for (new, old) in imports.iter_mut().zip(&file.imports) {
                    if new.path != old.path {
                        return Err(rename_error("Rename changed an import path"));
                    }
                    new.target = old.target.clone();
                }
                file.import_spans = imports.iter().map(|import| import.span).collect();
                file.imports = imports;
                file.base = base;
                base += file.source.len() + 1;
                if base > 64 * 1024 * 1024 {
                    return Err(rename_error("Renamed graph exceeds the 64 MiB limit"));
                }
            }
            let checked = check_files(&files).map_err(|errors| {
                rename_error(format!(
                    "Rename would fail checking: {}",
                    errors
                        .first()
                        .map_or("unknown error", |error| error.message.as_str())
                ))
            })?;
            let mut after = Analysis {
                uri: graph.uri.clone(),
                ..Analysis::default()
            };
            for file in &files {
                let file_uri = if file.path.as_os_str().is_empty() {
                    graph.uri.clone()
                } else {
                    self.file_uri(&file.path)
                };
                after.symbols.extend(local_symbols(&file_uri, &file.source));
            }
            after.references = checked_references(
                &checked.program,
                &files,
                |path| {
                    if path.as_os_str().is_empty() {
                        graph.uri.clone()
                    } else {
                        self.file_uri(path)
                    }
                },
                &mut after.symbols,
            );
            let expected: BTreeSet<_> = reference_identities(graph)
                .iter()
                .map(|(site, declaration)| {
                    (
                        moved_anchor(site, &offsets),
                        moved_anchor(declaration, &offsets),
                    )
                })
                .collect();
            if expected != reference_identities(&after) {
                return Err(rename_error(
                    "Rename would capture or change a symbol binding",
                ));
            }
        }
        bounded_refactor(Value::object([(
            "documentChanges",
            Value::Array(
                edits
                    .into_iter()
                    .map(|(file_uri, changes)| {
                        let doc = &self.documents[&file_uri];
                        Value::object([
                            (
                                "textDocument",
                                Value::object([
                                    ("uri", file_uri.into()),
                                    ("version", doc.version.into()),
                                ]),
                            ),
                            (
                                "edits",
                                Value::Array(
                                    changes
                                        .into_iter()
                                        .map(|span| {
                                            Value::object([
                                                ("range", range(&doc.source, span)),
                                                ("newText", name.into()),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        )]))
    }
}

#[derive(Default)]
struct Analysis {
    uri: String,
    diagnostics: BTreeMap<String, Vec<Value>>,
    symbols: Vec<Symbol>,
    references: Vec<DefinitionReference>,
    dependencies: BTreeSet<String>,
    global_fallback: bool,
    files: Vec<SourceFile>,
    program: Option<Program>,
}
impl Analysis {
    fn add(&mut self, uri: &str, source: &str, errors: Vec<Diagnostic>) {
        self.diagnostics
            .entry(uri.to_owned())
            .or_default()
            .extend(errors.iter().map(|error| diagnostic(source, error)));
    }
}
#[derive(Clone)]
struct Symbol {
    name: String,
    key: String,
    uri: String,
    kind: i64,
    range: Value,
    selection: Value,
    selection_span: Span,
    children: Vec<Symbol>,
    detail: String,
    private: bool,
}
impl Symbol {
    fn location(&self) -> Value {
        Value::object([
            ("uri", self.uri.clone().into()),
            ("range", self.selection.clone()),
        ])
    }
    fn document_symbol(self) -> Value {
        let mut fields = BTreeMap::from([
            ("name".to_owned(), self.name.into()),
            ("kind".to_owned(), self.kind.into()),
            ("range".to_owned(), self.range),
            ("selectionRange".to_owned(), self.selection),
        ]);
        if !self.children.is_empty() {
            fields.insert(
                "children".into(),
                Value::Array(
                    self.children
                        .into_iter()
                        .map(Symbol::document_symbol)
                        .collect(),
                ),
            );
        }
        Value::Object(fields)
    }
}
const MAX_COMPLETIONS: usize = 128;
const MAX_DETAIL_BYTES: usize = 4096;

fn source_signature(source: &str, span: Span, field: bool) -> String {
    let raw = source.get(span.start..span.end).unwrap_or("");
    let raw = if field {
        raw
    } else {
        raw.split('{').next().unwrap_or(raw)
    };
    bounded_detail(&raw.split_whitespace().collect::<Vec<_>>().join(" "))
}
fn bounded_detail(value: &str) -> String {
    let mut end = value.len().min(MAX_DETAIL_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
fn private_at(tokens: &[Token], selection: Span) -> bool {
    let end = tokens.partition_point(|token| token.span.end <= selection.start);
    for token in tokens[..end]
        .iter()
        .rev()
        .filter(|token| token.kind != TokenKind::Newline)
        .take(3)
    {
        match &token.kind {
            TokenKind::Ident(name) if name == "private" => return true,
            TokenKind::Def | TokenKind::Extern | TokenKind::Struct | TokenKind::Enum => {}
            _ => break,
        }
    }
    false
}
#[derive(Clone)]
struct Suggestion {
    label: String,
    detail: String,
    kind: i64,
}
impl Suggestion {
    fn symbol(symbol: &Symbol) -> Self {
        Self {
            label: symbol.name.clone(),
            detail: symbol.detail.clone(),
            kind: match symbol.kind {
                12 => 3,
                6 => 2,
                8 => 5,
                23 => 22,
                10 => 13,
                22 => 20,
                _ => 6,
            },
        }
    }
    fn item(&self, source: &str, replace: Span) -> Value {
        Value::object([
            ("label", self.label.clone().into()),
            ("kind", self.kind.into()),
            ("detail", self.detail.clone().into()),
            ("insertTextFormat", 1_i64.into()),
            (
                "textEdit",
                Value::object([
                    ("range", range(source, replace)),
                    ("newText", self.label.clone().into()),
                ]),
            ),
        ])
    }
}
struct EditorWord {
    span: Span,
    dot: Option<usize>,
    qualifier: Option<String>,
}
fn editor_word(source: &str, offset: usize) -> Option<EditorWord> {
    // The lexer discards comments; scan only the prefix to exclude comments and
    // strings even when the unfinished rest of the document cannot be lexed.
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut string = false;
    let mut comment = false;
    while i < offset {
        let byte = bytes[i];
        if comment {
            if matches!(byte, b'\n' | b'\r') {
                comment = false;
            }
        } else if string {
            if byte == b'\\' {
                i += 1;
            } else if byte == b'"' {
                string = false;
            }
        } else if byte == b'"' {
            string = true;
        } else if byte == b'#' || (byte == b'/' && bytes.get(i + 1) == Some(&b'/')) {
            comment = true;
        }
        i += 1;
    }
    if string || comment {
        return None;
    }
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut start = offset;
    while start > 0 && ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && ident(bytes[end]) {
        end += 1;
    }
    if start < end && bytes[start].is_ascii_digit() {
        return None;
    }
    let mut previous = start;
    while previous > 0 && matches!(bytes[previous - 1], b' ' | b'\t') {
        previous -= 1;
    }
    let dot = (previous > 0 && bytes[previous - 1] == b'.').then(|| previous - 1);
    let qualifier = dot.and_then(|dot| {
        let mut end = dot;
        while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && ident(bytes[start - 1]) {
            start -= 1;
        }
        let before = source[..start].trim_end();
        (start < end && !before.ends_with('.')).then(|| source[start..end].to_owned())
    });
    Some(EditorWord {
        span: Span::new(start, end),
        dot,
        qualifier,
    })
}
fn empty_completion() -> Value {
    Value::object([
        ("isIncomplete", false.into()),
        ("items", Value::Array(Vec::new())),
    ])
}
impl Server {
    fn editor_query(&self, uri: &str, doc: &Document, offset: usize, completion: bool) -> Value {
        let Some(word) = editor_word(&doc.source, offset) else {
            return if completion {
                empty_completion()
            } else {
                Value::Null
            };
        };
        let mut analysis = self.analyze(uri, doc);
        // A single bounded recovery removes the unfinished access/name and closes
        // open blocks. It exists only for this query, never for diagnostics or builds.
        if completion && analysis.program.is_none() {
            if let Some(repaired) = repair_completion(&doc.source, &word) {
                let mut server = self.clone();
                let mut overlay = doc.clone();
                overlay.source = repaired;
                server.documents.insert(uri.to_owned(), overlay.clone());
                let recovered = server.analyze(uri, &overlay);
                if recovered.program.is_some() || analysis.symbols.is_empty() {
                    analysis = recovered;
                }
            }
        }
        let Some(file_index) = analysis.files.iter().position(|file| {
            if let Some(path) = &doc.path {
                &file.path == path
            } else {
                file.path.as_os_str().is_empty()
            }
        }) else {
            return if completion {
                empty_completion()
            } else {
                Value::Null
            };
        };
        let file = &analysis.files[file_index];
        let global_offset = file.base + offset;
        let expansion = crate::expand_files(&analysis.files).ok();
        let mut suggestions = BTreeMap::<String, Suggestion>::new();
        let locals = lexical_bindings(&analysis, file_index, global_offset, expansion.as_ref());
        if let Some(dot) = word.dot {
            let alias = word.qualifier.as_ref().and_then(|qualifier| {
                // Local bindings shadow import aliases in expression position.
                if locals.contains_key(qualifier) {
                    return None;
                }
                file.imports
                    .iter()
                    .find(|import| import.alias.as_ref() == Some(qualifier))
            });
            if let Some(import) = alias {
                if !import.target.as_os_str().is_empty() {
                    let target = &import.target;
                    if let Some(index) = analysis.files.iter().position(|file| &file.path == target)
                    {
                        for symbol in
                            self.module_exports(&analysis, index, false, &mut BTreeSet::new())
                        {
                            suggestions.insert(symbol.name.clone(), Suggestion::symbol(symbol));
                        }
                    }
                }
            } else if let Some(program) = analysis
                .program
                .as_ref()
                .filter(|_| !generic_context(&file.source, offset))
            {
                let dot = file.base + dot;
                let receiver = expression_ending(program, &analysis.files, dot);
                if let Some(ty) = receiver.and_then(|expr| expr.ty) {
                    suggestions.extend(self.member_suggestions(
                        &analysis,
                        uri,
                        ty,
                        expansion.as_ref(),
                    ));
                } else if let Some(qualifier) = &word.qualifier {
                    // Enum variant access has a type qualifier, not a value receiver.
                    for symbol in
                        self.module_exports(&analysis, file_index, true, &mut BTreeSet::new())
                    {
                        if symbol.name == *qualifier && symbol.kind == 10 {
                            for variant in &symbol.children {
                                suggestions
                                    .insert(variant.name.clone(), Suggestion::symbol(variant));
                            }
                        }
                    }
                }
            }
        } else {
            for symbol in self.module_exports(&analysis, file_index, true, &mut BTreeSet::new()) {
                suggestions.insert(symbol.name.clone(), Suggestion::symbol(symbol));
            }
            for import in &file.imports {
                if let Some(alias) = &import.alias {
                    suggestions.insert(
                        alias.clone(),
                        Suggestion {
                            label: alias.clone(),
                            detail: format!("module {alias}: {}", import.path),
                            kind: 9,
                        },
                    );
                }
            }
            for (name, detail) in editor_builtins() {
                suggestions
                    .entry(name.to_owned())
                    .or_insert_with(|| Suggestion {
                        label: name.to_owned(),
                        detail: detail.to_owned(),
                        kind: 3,
                    });
            }
            for keyword in [
                "var", "def", "struct", "enum", "if", "else", "while", "for", "in", "break",
                "continue", "return", "match", "case", "private", "import", "true", "false", "int",
                "float", "bool", "str", "buffer", "ints", "List", "Map",
            ] {
                suggestions
                    .entry(keyword.into())
                    .or_insert_with(|| Suggestion {
                        label: keyword.into(),
                        detail: format!("Bhasa {keyword}"),
                        kind: 14,
                    });
            }
            suggestions.extend(locals);
        }
        suggestions.retain(|name, value| {
            name.len() <= MAX_DETAIL_BYTES
                && !name.starts_with("__bh")
                && !value.detail.contains("__bhm_")
                && !value.detail.contains("__bhg_")
        });
        if completion {
            let prefix = &doc.source[word.span.start..offset];
            let matching: Vec<_> = suggestions
                .values()
                .filter(|item| item.label.starts_with(prefix))
                .collect();
            Value::object([
                ("isIncomplete", (matching.len() > MAX_COMPLETIONS).into()),
                (
                    "items",
                    Value::Array(
                        matching
                            .into_iter()
                            .take(MAX_COMPLETIONS)
                            .map(|item| item.item(&doc.source, word.span))
                            .collect(),
                    ),
                ),
            ])
        } else {
            let name = &doc.source[word.span.start..word.span.end];
            // Checked references disambiguate qualified calls and methods, while
            // lexical bindings take precedence over same-named declarations.
            let value = suggestions
                .get(name)
                .map(|item| item.detail.clone())
                .or_else(|| {
                    analysis
                        .references
                        .iter()
                        .find(|reference| {
                            reference.uri == uri
                                && reference.span.start <= offset
                                && offset < reference.span.end
                        })
                        .and_then(|reference| find_symbol(&analysis.symbols, &reference.target))
                        .filter(|symbol| symbol.uri == uri || !symbol.private)
                        .map(|symbol| symbol.detail.clone())
                });
            value
                .filter(|detail| {
                    !detail.is_empty() && !detail.contains("__bhm_") && !detail.contains("__bhg_")
                })
                .map_or(Value::Null, |detail| {
                    Value::object([
                        (
                            "contents",
                            Value::object([
                                ("kind", "plaintext".into()),
                                ("value", bounded_detail(&detail).into()),
                            ]),
                        ),
                        ("range", range(&doc.source, word.span)),
                    ])
                })
        }
    }
    fn module_exports<'a>(
        &self,
        analysis: &'a Analysis,
        index: usize,
        local: bool,
        seen: &mut BTreeSet<usize>,
    ) -> Vec<&'a Symbol> {
        if !seen.insert(index) {
            return Vec::new();
        }
        let file = &analysis.files[index];
        let uri = if file.path.as_os_str().is_empty() {
            analysis.uri.clone()
        } else {
            self.file_uri(&file.path)
        };
        let mut values: Vec<_> = analysis
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.uri == uri && !matches!(symbol.kind, 3 | 13) && (local || !symbol.private)
            })
            .collect();
        for import in &file.imports {
            if import.alias.is_some() {
                continue;
            }
            if !import.target.as_os_str().is_empty() {
                let target = &import.target;
                if let Some(index) = analysis.files.iter().position(|file| &file.path == target) {
                    values.extend(self.module_exports(analysis, index, false, seen));
                }
            }
        }
        values
    }
    fn member_suggestions(
        &self,
        analysis: &Analysis,
        uri: &str,
        ty: Type,
        expansion: Option<&crate::ExpandedFiles>,
    ) -> BTreeMap<String, Suggestion> {
        let mut result = BTreeMap::new();
        let Some(program) = &analysis.program else {
            return result;
        };
        let Type::Record(id) = ty else {
            return result;
        };
        let Some(record) = program.record(id) else {
            return result;
        };
        if let Ok(Some(schema)) = crate::collections::schema(record) {
            for name in schema.methods() {
                let (args, ret) = schema.method(name).unwrap();
                let args = args
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| {
                        format!(
                            "arg{}: {}",
                            index + 1,
                            display_type(program, *ty, expansion)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                result.insert(
                    (*name).into(),
                    Suggestion {
                        label: (*name).into(),
                        detail: format!(
                            "def {name}({args}) -> {}",
                            display_type(program, ret, expansion)
                        ),
                        kind: 2,
                    },
                );
            }
            return result;
        }
        let original = expansion.map_or_else(
            || record.name.clone(),
            |map| map.original_name(&record.name),
        );
        let Some(file) = analysis
            .files
            .iter()
            .rev()
            .find(|file| file.base <= record.span.start)
        else {
            return result;
        };
        let owner = self.file_uri(&file.path);
        let symbol = analysis.symbols.iter().find(|symbol| {
            symbol.name == original
                && (symbol.uri == owner || file.path.as_os_str().is_empty() && symbol.uri == uri)
        });
        if let Some(symbol) = symbol {
            for member in &symbol.children {
                if member.private && member.uri != uri {
                    continue;
                }
                let mut suggestion = Suggestion::symbol(member);
                if let Some(field) = record.fields.iter().find(|field| field.name == member.name) {
                    suggestion.detail = format!(
                        "{}: {}",
                        field.name,
                        display_type(program, field.ty, expansion)
                    );
                } else if let Some(method) = record
                    .methods
                    .iter()
                    .find(|method| method.name == member.name)
                {
                    suggestion.detail = typed_signature(program, method, expansion, true);
                }
                result.insert(member.name.clone(), suggestion);
            }
        }
        result
    }
}
fn display_type(program: &Program, ty: Type, expansion: Option<&crate::ExpandedFiles>) -> String {
    let name = program.type_name(ty);
    expansion.map_or(name.clone(), |map| {
        map.modules
            .display_names(&map.generics.display_names(&name))
    })
}
fn typed_signature(
    program: &Program,
    function: &ast::Function,
    expansion: Option<&crate::ExpandedFiles>,
    method: bool,
) -> String {
    let name = expansion.map_or_else(
        || function.name.clone(),
        |map| map.original_name(&function.name),
    );
    let args = function
        .params
        .iter()
        .skip(usize::from(method))
        .map(|param| {
            format!(
                "{}: {}",
                param.name,
                display_type(program, param.ty, expansion)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    bounded_detail(&format!(
        "def {name}({args}) -> {}",
        display_type(program, function.return_type, expansion)
    ))
}
fn repair_completion(source: &str, word: &EditorWord) -> Option<String> {
    let mut bytes = source.as_bytes().to_vec();
    let start = word.dot.unwrap_or(word.span.start);
    if start < word.span.end {
        bytes[start..word.span.end].fill(b' ');
        if word.dot.is_none() {
            bytes[start] = b'0';
        }
    }
    let mut repaired = String::from_utf8(bytes).ok()?;
    let tokens = crate::lexer::lex(&repaired).ok()?;
    let mut stack = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::LeftBrace => stack.push('}'),
            TokenKind::LeftParen => stack.push(')'),
            TokenKind::LeftBracket => stack.push(']'),
            TokenKind::RightBrace | TokenKind::RightParen | TokenKind::RightBracket => {
                stack.pop()?;
            }
            _ => {}
        }
    }
    if stack.len() > 16 {
        return None;
    }
    for closing in stack.into_iter().rev() {
        repaired.push(closing);
    }
    (repaired != source).then_some(repaired)
}
fn editor_builtins() -> [(&'static str, &'static str); 13] {
    [
        (
            "print",
            "def print(value: int | float | bool | str | buffer) -> void",
        ),
        ("int", "def int(value: float) -> int"),
        ("float", "def float(value: int) -> float"),
        ("buffer", "def buffer(value: str) -> buffer"),
        ("ints", "def ints() -> ints"),
        ("len", "def len(value: buffer | ints) -> int"),
        ("get", "def get(value: buffer | ints, index: int) -> int"),
        (
            "push",
            "def push(value: buffer | ints, element: int) -> void",
        ),
        (
            "range",
            "range(end: int) | range(start: int, end: int, step: int = 1)",
        ),
        ("spawn", "spawn(worker(value)) -> inferred task"),
        ("async_spawn", "async_spawn(worker(value)) -> inferred task"),
        ("join", "join(value: task) -> worker result"),
        ("await", "await(value: task) -> worker result"),
    ]
}

fn visit_expressions<'a>(block: &'a Block, visit: &mut impl FnMut(&'a Expr)) {
    fn expr<'a>(value: &'a Expr, visit: &mut impl FnMut(&'a Expr)) {
        visit(value);
        match &value.kind {
            ExprKind::Unary { operand, .. } => expr(operand, visit),
            ExprKind::Binary { left, right, .. } => {
                expr(left, visit);
                expr(right, visit);
            }
            ExprKind::Call { args, .. } => {
                for arg in args {
                    expr(arg, visit);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                expr(receiver, visit);
                for arg in args {
                    expr(arg, visit);
                }
            }
            ExprKind::Field { base, .. } => expr(base, visit),
            _ => {}
        }
    }
    for statement in block {
        match &statement.kind {
            StmtKind::Var { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => expr(value, visit),
            StmtKind::FieldAssign { target, value } => {
                expr(target, visit);
                expr(value, visit);
            }
            StmtKind::Return(Some(value)) => expr(value, visit),
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                expr(condition, visit);
                visit_expressions(then_block, visit);
                if let Some(block) = else_block {
                    visit_expressions(block, visit);
                }
            }
            StmtKind::While { condition, body } => {
                expr(condition, visit);
                visit_expressions(body, visit);
            }
            StmtKind::For { source, body, .. } => {
                match source {
                    ForSource::Range { start, end, step } => {
                        expr(start, visit);
                        expr(end, visit);
                        if let Some(step) = step {
                            expr(step, visit);
                        }
                    }
                    ForSource::Iterable(value) => expr(value, visit),
                };
                visit_expressions(body, visit);
            }
            StmtKind::Match { value, arms } => {
                expr(value, visit);
                for arm in arms {
                    visit_expressions(&arm.body, visit);
                }
            }
            _ => {}
        }
    }
}
fn expression_ending<'a>(
    program: &'a Program,
    files: &[SourceFile],
    dot: usize,
) -> Option<&'a Expr> {
    let file = files.iter().rev().find(|file| file.base <= dot)?;
    let mut found = None;
    for function in program
        .functions
        .iter()
        .chain(program.records.iter().flat_map(|record| &record.methods))
    {
        if let Some(body) = &function.body {
            visit_expressions(body, &mut |expr| {
                if expr.span.end <= dot
                    && expr.span.end >= file.base
                    && file
                        .source
                        .get(expr.span.end - file.base..dot - file.base)
                        .is_some_and(|suffix| suffix.trim().is_empty())
                    && found.is_none_or(|previous: &Expr| expr.span.start < previous.span.start)
                {
                    found = Some(expr);
                }
            });
        }
    }
    found
}
// Specializations share original spans. A generic declaration's editor types
// must come from its template, never whichever concrete instance was visited last.
fn generic_context(source: &str, offset: usize) -> bool {
    let Ok(tokens) = crate::lexer::lex(source) else {
        return false;
    };
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(
            token.kind,
            TokenKind::Def | TokenKind::Struct | TokenKind::Enum
        ) || token.span.start > offset
        {
            continue;
        }
        if !tokens
            .get(index + 2)
            .is_some_and(|token| token.kind == TokenKind::LeftBracket)
        {
            continue;
        }
        let Some(open) = tokens[index + 3..]
            .iter()
            .position(|token| token.kind == TokenKind::LeftBrace)
            .map(|next| index + 3 + next)
        else {
            continue;
        };
        let mut depth = 0;
        for token in &tokens[open..] {
            match token.kind {
                TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBrace => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                if offset <= token.span.end {
                    return true;
                }
                break;
            }
        }
    }
    false
}
fn lexical_bindings(
    analysis: &Analysis,
    index: usize,
    offset: usize,
    expansion: Option<&crate::ExpandedFiles>,
) -> BTreeMap<String, Suggestion> {
    let file = &analysis.files[index];
    let syntax_only =
        analysis.program.is_none() || generic_context(&file.source, offset - file.base);
    let expansion = if syntax_only { None } else { expansion };
    let mut fallback;
    let program = if !syntax_only {
        analysis.program.as_ref().unwrap()
    } else {
        let Ok(mut program) = symbol_program(&syntax_source(&file.source, &file.imports)) else {
            return BTreeMap::new();
        };
        // Syntax trees use file-local spans; shift them before comparing the cursor.
        crate::modules::visit_program_spans(&mut program, &|span| {
            Span::new(span.start + file.base, span.end + file.base)
        });
        fallback = program;
        &mut fallback
    };
    let tokens = crate::lexer::lex(&file.source).unwrap_or_default();
    let mut blocks = Vec::new();
    let mut stack = Vec::new();
    for token in &tokens {
        if token.kind == TokenKind::LeftBrace {
            stack.push(token.span.end + file.base);
        }
        if token.kind == TokenKind::RightBrace {
            if let Some(start) = stack.pop() {
                blocks.push(Span::new(start, token.span.start + file.base));
            }
        }
    }
    let scope = |position: usize| {
        blocks
            .iter()
            .filter(|span| span.start <= position && position <= span.end)
            .min_by_key(|span| span.end - span.start)
            .copied()
    };
    let body_scope = |start: usize, end: usize| {
        blocks
            .iter()
            .filter(|span| span.start >= start && span.end < end)
            .max_by_key(|span| span.end - span.start)
            .copied()
    };
    let declaration_span = |name: &str, span: Span| {
        tokens
            .iter()
            .find(|token| {
                token.span.start + file.base >= span.start
                    && token.span.end + file.base <= span.end
                    && matches!(&token.kind, TokenKind::Ident(value) if value == name)
            })
            .map_or(span, |token| {
                Span::new(token.span.start + file.base, token.span.end + file.base)
            })
    };
    let mut bindings: Vec<(usize, Suggestion)> = Vec::new();
    let mut add = |name: &str, ty: Option<Type>, declaration: Span, visible: usize, block: Span| {
        let declaration = declaration_span(name, declaration);
        if (block.start <= offset && offset <= block.end && visible <= offset)
            || (declaration.start <= offset && offset < declaration.end)
        {
            let detail = ty.map_or_else(
                || format!("var {name}"),
                |ty| format!("{name}: {}", display_type(program, ty, expansion)),
            );
            bindings.push((
                block.start,
                Suggestion {
                    label: name.into(),
                    detail,
                    kind: 6,
                },
            ));
        }
    };
    fn block_bindings(
        block: &Block,
        scope: &impl Fn(usize) -> Option<Span>,
        body_scope: &impl Fn(usize, usize) -> Option<Span>,
        add: &mut impl FnMut(&str, Option<Type>, Span, usize, Span),
    ) {
        for statement in block {
            match &statement.kind {
                StmtKind::Var {
                    name,
                    annotation,
                    value,
                } => {
                    if let Some(block) = scope(statement.span.start) {
                        let inferred = value.ty.or(*annotation).or(match value.kind {
                            ExprKind::Int(_) => Some(Type::Int),
                            ExprKind::Float(_) => Some(Type::Float),
                            ExprKind::Bool(_) => Some(Type::Bool),
                            ExprKind::Str(_) => Some(Type::Str),
                            _ => None,
                        });
                        add(
                            name,
                            inferred,
                            Span::new(statement.span.start, value.span.start),
                            value.span.end,
                            block,
                        );
                    }
                }
                StmtKind::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    block_bindings(then_block, scope, body_scope, add);
                    if let Some(block) = else_block {
                        block_bindings(block, scope, body_scope, add);
                    }
                }
                StmtKind::While { body, .. } => block_bindings(body, scope, body_scope, add),
                StmtKind::For {
                    name,
                    name_span,
                    source,
                    element_type,
                    body,
                } => {
                    let start = match source {
                        ForSource::Range { end, step, .. } => {
                            step.as_ref().map_or(end.span.end, |step| step.span.end)
                        }
                        ForSource::Iterable(value) => value.span.end,
                    };
                    if let Some(first) = body_scope(start, statement.span.end) {
                        add(
                            name,
                            element_type.or(if matches!(source, ForSource::Range { .. }) {
                                Some(Type::Int)
                            } else {
                                None
                            }),
                            *name_span,
                            start,
                            first,
                        );
                    }
                    block_bindings(body, scope, body_scope, add);
                }
                StmtKind::Match { arms, .. } => {
                    for arm in arms {
                        if let Some(block) = body_scope(arm.span.start, arm.span.end) {
                            for binding in &arm.bindings {
                                add(&binding.name, binding.ty, binding.span, block.start, block);
                            }
                        }
                        block_bindings(&arm.body, scope, body_scope, add);
                    }
                }
                _ => {}
            }
        }
    }
    for function in program
        .functions
        .iter()
        .chain(program.records.iter().flat_map(|record| &record.methods))
    {
        if function.span.start > offset || function.span.end < offset {
            continue;
        }
        if let Some(body) = &function.body {
            let outer = blocks
                .iter()
                .filter(|span| span.start > function.span.start && span.end < function.span.end)
                .max_by_key(|span| span.end - span.start)
                .copied();
            if let Some(block) = outer {
                for param in &function.params {
                    add(&param.name, Some(param.ty), param.span, block.start, block);
                }
            }
            block_bindings(body, &scope, &body_scope, &mut add);
        }
    }
    bindings.sort_by_key(|(start, _)| *start);
    bindings
        .into_iter()
        .map(|(_, suggestion)| (suggestion.label.clone(), suggestion))
        .collect()
}

fn declaration_name(tokens: &[Token], keyword: TokenKind, name: &str, span: Span) -> Option<Span> {
    tokens
        .windows(2)
        .find(|pair| {
            pair[0].kind == keyword
                && pair[0].span.start >= span.start
                && pair[1].span.end <= span.end
                && matches!(&pair[1].kind, TokenKind::Ident(value) if value == name)
        })
        .map(|pair| pair[1].span)
}
// Declaration shapes retain the original generic names, including unused and
// imported templates. This syntax-only tree is never type checked or emitted.
fn symbol_program(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let tokens = crate::lexer::lex(source)?;
    let mut bytes = source.as_bytes().to_vec();
    let mut start = None;
    let mut depth = 0;
    for token in tokens {
        match token.kind {
            TokenKind::LeftBracket => {
                if depth == 0 {
                    start = Some(token.span.start);
                }
                depth += 1;
            }
            TokenKind::RightBracket if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    // Preserve byte offsets. Erase internal line breaks too:
                    // they were suppressed by the original bracket nesting.
                    bytes[start.take().unwrap()..token.span.end].fill(b' ');
                }
            }
            _ => {}
        }
    }
    let source = String::from_utf8(bytes).expect("blanking token ranges preserves UTF-8");
    crate::parser::parse(&source)
}

// Keep a parser-only declaration shape while an import graph is unavailable.
// This source is never checked or emitted. Names and byte positions remain the
// original ones; namespace prefixes and privacy modifiers are only blanked.
fn syntax_source(source: &str, imports: &[ModuleImport]) -> String {
    let source = blank_imports(source, imports);
    let Ok(tokens) = crate::lexer::lex(&source) else {
        return source;
    };
    let aliases: BTreeSet<_> = imports
        .iter()
        .filter_map(|import| import.alias.as_deref())
        .collect();
    let mut bytes = source.as_bytes().to_vec();
    let blank = |bytes: &mut [u8], span: Span| {
        for byte in &mut bytes[span.start..span.end] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    };
    for (index, token) in tokens.iter().enumerate() {
        let TokenKind::Ident(name) = &token.kind else {
            continue;
        };
        if aliases.contains(name.as_str())
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.kind == TokenKind::Dot)
        {
            blank(&mut bytes, token.span.join(tokens[index + 1].span));
        }
        if name == "private" {
            let mut next = index + 1;
            while tokens
                .get(next)
                .is_some_and(|token| token.kind == TokenKind::Newline)
            {
                next += 1;
            }
            let declaration = tokens.get(next).is_some_and(|token| {
                matches!(
                    token.kind,
                    TokenKind::Def | TokenKind::Extern | TokenKind::Struct | TokenKind::Enum
                )
            });
            let field = tokens
                .get(next)
                .is_some_and(|token| matches!(token.kind, TokenKind::Ident(_)))
                && tokens
                    .get(next + 1)
                    .is_some_and(|token| token.kind == TokenKind::Colon);
            if declaration || field {
                blank(&mut bytes, token.span);
            }
        }
    }
    String::from_utf8(bytes).expect("blanking complete tokens preserves UTF-8")
}

fn local_symbols(uri: &str, source: &str) -> Vec<Symbol> {
    let Ok(imports) = imports(source) else {
        return Vec::new();
    };
    let Ok(program) = symbol_program(&syntax_source(source, &imports)) else {
        return Vec::new();
    };
    let Ok(tokens) = crate::lexer::lex(source) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    for function in &program.functions {
        let Some(selection) =
            declaration_name(&tokens, TokenKind::Def, &function.name, function.span)
        else {
            continue;
        };
        symbols.push((
            function.span.start,
            Symbol {
                name: function.name.clone(),
                key: symbol_key(uri, &function.name),
                uri: uri.to_owned(),
                kind: 12,
                range: range(source, function.span),
                selection: range(source, selection),
                selection_span: selection,
                detail: source_signature(source, function.span, false),
                private: private_at(&tokens, selection),
                children: Vec::new(),
            },
        ));
    }
    for record in &program.records {
        let Some(selection) =
            declaration_name(&tokens, TokenKind::Struct, &record.name, record.span)
        else {
            continue;
        };
        let mut children = Vec::new();
        for field in &record.fields {
            let Some(token) = tokens.iter().find(|token| {
                token.span.start >= field.span.start
                    && token.span.end <= field.span.end
                    && matches!(&token.kind, TokenKind::Ident(name) if name == &field.name)
            }) else {
                continue;
            };
            children.push((
                field.span.start,
                Symbol {
                    name: field.name.clone(),
                    key: symbol_key(uri, &ast::method_key(&record.name, &field.name)),
                    uri: uri.to_owned(),
                    kind: 8,
                    range: range(source, field.span),
                    selection: range(source, token.span),
                    selection_span: token.span,
                    detail: source_signature(source, field.span, true),
                    private: private_at(&tokens, token.span),
                    children: Vec::new(),
                },
            ));
        }
        for method in &record.methods {
            let Some(selection) =
                declaration_name(&tokens, TokenKind::Def, &method.name, method.span)
            else {
                continue;
            };
            children.push((
                method.span.start,
                Symbol {
                    name: method.name.clone(),
                    key: symbol_key(uri, &ast::method_key(&record.name, &method.name)),
                    uri: uri.to_owned(),
                    kind: 6,
                    range: range(source, method.span),
                    selection: range(source, selection),
                    selection_span: selection,
                    detail: source_signature(source, method.span, false),
                    private: private_at(&tokens, selection),
                    children: Vec::new(),
                },
            ));
        }
        children.sort_by_key(|(start, _)| *start);
        let children = children.into_iter().map(|(_, symbol)| symbol).collect();
        symbols.push((
            record.span.start,
            Symbol {
                name: record.name.clone(),
                key: symbol_key(uri, &record.name),
                uri: uri.to_owned(),
                kind: 23,
                range: range(source, record.span),
                selection: range(source, selection),
                selection_span: selection,
                detail: source_signature(
                    source,
                    Span::new(record.span.start, selection.end),
                    false,
                ),
                private: private_at(&tokens, selection),
                children,
            },
        ));
    }
    for declaration in &program.enums {
        let Some(selection) = declaration_name(
            &tokens,
            TokenKind::Enum,
            &declaration.name,
            declaration.span,
        ) else {
            continue;
        };
        let mut children = Vec::new();
        for variant in &declaration.variants {
            let Some(token) = tokens.iter().find(|token| {
                token.span.start >= variant.span.start
                    && token.span.end <= variant.span.end
                    && matches!(&token.kind, TokenKind::Ident(name) if name == &variant.name)
            }) else {
                continue;
            };
            children.push(Symbol {
                name: variant.name.clone(),
                key: symbol_key(uri, &ast::method_key(&declaration.name, &variant.name)),
                uri: uri.to_owned(),
                kind: 22,
                range: range(source, variant.span),
                selection: range(source, token.span),
                selection_span: token.span,
                detail: source_signature(source, variant.span, true),
                private: false,
                children: Vec::new(),
            });
        }
        symbols.push((
            declaration.span.start,
            Symbol {
                name: declaration.name.clone(),
                key: symbol_key(uri, &declaration.name),
                uri: uri.to_owned(),
                kind: 10,
                range: range(source, declaration.span),
                selection: range(source, selection),
                selection_span: selection,
                detail: source_signature(
                    source,
                    Span::new(declaration.span.start, selection.end),
                    false,
                ),
                private: private_at(&tokens, selection),
                children,
            },
        ));
    }
    symbols.sort_by_key(|(start, _)| *start);
    symbols.into_iter().map(|(_, symbol)| symbol).collect()
}
fn declaration_at<'a>(symbols: &'a [Symbol], uri: &str, offset: usize) -> Option<&'a Symbol> {
    for symbol in symbols {
        if symbol.uri == uri
            && symbol.selection_span.start <= offset
            && offset < symbol.selection_span.end
        {
            return Some(symbol);
        }
        if let Some(found) = declaration_at(&symbol.children, uri, offset) {
            return Some(found);
        }
    }
    None
}
fn symbol_key(uri: &str, name: &str) -> String {
    format!("{uri}#{name}")
}

fn find_symbol<'a>(symbols: &'a [Symbol], key: &str) -> Option<&'a Symbol> {
    for symbol in symbols {
        if symbol.key == key {
            return Some(symbol);
        }
        if let Some(found) = find_symbol(&symbol.children, key) {
            return Some(found);
        }
    }
    None
}

/// Parse independently so an incomplete open file cannot consume another file's
/// declarations, then reuse the compiler's type-ID remapping and checker.
fn check_files(files: &[SourceFile]) -> Result<ast::CheckedProgram, Vec<Diagnostic>> {
    crate::check_files(files)
}

#[derive(Clone)]
struct DefinitionReference {
    uri: String,
    span: Span,
    target: String,
}
struct ReferenceFile {
    uri: String,
    base: usize,
    tokens: Vec<Token>,
    source: String,
}
struct ReferenceIndex<'a> {
    program: &'a Program,
    expansion: Option<crate::ExpandedFiles>,
    files: Vec<ReferenceFile>,
    references: Vec<DefinitionReference>,
    targets: BTreeMap<String, String>,
    locals: Vec<Symbol>,
    scopes: Vec<BTreeMap<String, String>>,
}
#[derive(Clone, Copy)]
enum NameContext {
    Call,
    MemberCall,
    Field,
    Declaration,
    Pattern,
    Type,
}
fn checked_references(
    program: &Program,
    files: &[SourceFile],
    uri: impl Fn(&Path) -> String,
    symbols: &mut Vec<Symbol>,
) -> Vec<DefinitionReference> {
    let mut index = ReferenceIndex {
        program,
        expansion: crate::expand_files(files).ok(),
        files: files
            .iter()
            .map(|file| {
                let mut tokens = crate::lexer::lex(&file.source).unwrap_or_default();
                for token in &mut tokens {
                    token.span.start += file.base;
                    token.span.end += file.base;
                }
                ReferenceFile {
                    uri: uri(&file.path),
                    base: file.base,
                    tokens,
                    source: file.source.clone(),
                }
            })
            .collect(),
        references: Vec::new(),
        targets: BTreeMap::new(),
        locals: Vec::new(),
        scopes: Vec::new(),
    };
    for function in &program.functions {
        index.target(&function.name, function.span);
    }
    for record in &program.records {
        index.target(&record.name, record.span);
        for field in &record.fields {
            index.target(&ast::method_key(&record.name, &field.name), field.span);
        }
        for method in &record.methods {
            index.target(&ast::method_key(&record.name, &method.name), method.span);
        }
    }
    for declaration in &program.enums {
        index.target(&declaration.name, declaration.span);
        for variant in &declaration.variants {
            index.target(
                &ast::method_key(&declaration.name, &variant.name),
                variant.span,
            );
        }
    }
    for function in &program.functions {
        index.name(
            function.span,
            function.span.start,
            &function.name,
            &function.name,
            NameContext::Declaration,
        );
        for param in &function.params {
            index.type_reference(param.ty, param.span, TokenKind::Colon);
        }
        index.type_reference(function.return_type, function.span, TokenKind::Arrow);
        if let Some(body) = &function.body {
            index.function(&function.params, body);
        }
    }
    for record in &program.records {
        index.name(
            record.span,
            record.span.start,
            &record.name,
            &record.name,
            NameContext::Declaration,
        );
        for field in &record.fields {
            index.name(
                field.span,
                field.span.start,
                &field.name,
                &ast::method_key(&record.name, &field.name),
                NameContext::Declaration,
            );
            index.type_reference(field.ty, field.span, TokenKind::Colon);
        }
        for method in &record.methods {
            index.name(
                method.span,
                method.span.start,
                &method.name,
                &ast::method_key(&record.name, &method.name),
                NameContext::Declaration,
            );
            for param in &method.params {
                index.type_reference(param.ty, param.span, TokenKind::Colon);
            }
            index.type_reference(method.return_type, method.span, TokenKind::Arrow);
            if let Some(body) = &method.body {
                index.function(&method.params, body);
            }
        }
    }
    for declaration in &program.enums {
        for variant in &declaration.variants {
            for field in &variant.fields {
                index.type_reference(field.ty, field.span, TokenKind::Colon);
            }
        }
    }
    fn declarations(
        symbols: &[Symbol],
        targets: &BTreeMap<String, String>,
        refs: &mut Vec<DefinitionReference>,
    ) {
        for symbol in symbols {
            if targets.values().any(|target| target == &symbol.key) {
                refs.push(DefinitionReference {
                    uri: symbol.uri.clone(),
                    span: symbol.selection_span,
                    target: symbol.key.clone(),
                });
            }
            declarations(&symbol.children, targets, refs);
        }
    }
    declarations(symbols, &index.targets, &mut index.references);
    for local in index.locals {
        if !symbols.iter().any(|symbol| symbol.key == local.key) {
            symbols.push(local);
        }
    }
    for file in files {
        let file_uri = uri(&file.path);
        let tokens = crate::lexer::lex(&file.source).unwrap_or_default();
        for import in &file.imports {
            let Some(alias) = &import.alias else { continue };
            let Some(declaration) = tokens.iter().rev().find(|token| {
                token.span.start >= import.span.start
                    && token.span.end <= import.span.end
                    && matches!(&token.kind, TokenKind::Ident(name) if name == alias)
            }) else {
                continue;
            };
            let key = format!("{file_uri}#alias:{}", declaration.span.start);
            symbols.push(Symbol {
                name: alias.clone(),
                key: key.clone(),
                uri: file_uri.clone(),
                kind: 3,
                range: range(&file.source, import.span),
                selection: range(&file.source, declaration.span),
                selection_span: declaration.span,
                children: Vec::new(),
                detail: format!("module {alias}"),
                private: false,
            });
            index.references.push(DefinitionReference {
                uri: file_uri.clone(),
                span: declaration.span,
                target: key.clone(),
            });
            for pair in tokens.windows(3) {
                if !matches!(&pair[0].kind, TokenKind::Ident(name) if name == alias)
                    || pair[1].kind != TokenKind::Dot
                {
                    continue;
                }
                if index
                    .references
                    .iter()
                    .any(|reference| reference.uri == file_uri && reference.span == pair[0].span)
                {
                    continue;
                }
                // Checked member/type occurrences prove that this prefix resolved
                // as a module, rather than an unresolved receiver in a template.
                if index
                    .references
                    .iter()
                    .any(|reference| reference.uri == file_uri && reference.span == pair[2].span)
                {
                    index.references.push(DefinitionReference {
                        uri: file_uri.clone(),
                        span: pair[0].span,
                        target: key.clone(),
                    });
                }
            }
        }
    }
    index.references
}
impl ReferenceIndex<'_> {
    fn function(&mut self, params: &[ast::Param], body: &Block) {
        self.scopes.clear();
        self.scopes.push(BTreeMap::new());
        for param in params {
            self.local_declaration(&param.name, param.span);
        }
        self.block(body);
        self.scopes.clear();
    }
    fn scoped_block(&mut self, body: &Block) {
        self.scopes.push(BTreeMap::new());
        self.block(body);
        self.scopes.pop();
    }
    fn local_token(&self, name: &str, span: Span) -> Option<(usize, Span)> {
        let index = self
            .files
            .partition_point(|file| file.base <= span.start)
            .checked_sub(1)?;
        let file = &self.files[index];
        let token = file.tokens.iter().find(|token| {
            token.span.start >= span.start
                && token.span.end <= span.end
                && matches!(&token.kind, TokenKind::Ident(value) if value == name)
        })?;
        Some((
            index,
            Span::new(token.span.start - file.base, token.span.end - file.base),
        ))
    }
    fn local_declaration(&mut self, name: &str, span: Span) {
        let Some((index, span)) = self.local_token(name, span) else {
            return;
        };
        let file = &self.files[index];
        let key = format!("{}#local:{}", file.uri, span.start);
        self.locals.push(Symbol {
            name: name.into(),
            key: key.clone(),
            uri: file.uri.clone(),
            kind: 13,
            range: range(&file.source, span),
            selection: range(&file.source, span),
            selection_span: span,
            children: Vec::new(),
            detail: format!("var {name}"),
            private: false,
        });
        self.references.push(DefinitionReference {
            uri: file.uri.clone(),
            span,
            target: key.clone(),
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), key);
        }
    }
    fn local_use(&mut self, name: &str, span: Span) {
        let Some(target) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
        else {
            return;
        };
        if let Some((index, span)) = self.local_token(name, span) {
            self.references.push(DefinitionReference {
                uri: self.files[index].uri.clone(),
                span,
                target,
            });
        }
    }
    fn type_reference(&mut self, ty: Type, span: Span, separator: TokenKind) {
        if !matches!(ty, Type::Record(_) | Type::Enum(_)) {
            return;
        }
        let Some(file_index) = self
            .files
            .partition_point(|file| file.base <= span.start)
            .checked_sub(1)
        else {
            return;
        };
        let file = &self.files[file_index];
        let first = file
            .tokens
            .partition_point(|token| token.span.end <= span.start);
        let Some(start) = file.tokens[first..]
            .iter()
            .take_while(|token| token.span.start < span.end)
            .find(|token| token.kind == separator)
            .map(|token| token.span.end)
        else {
            return;
        };
        let name = self.program.type_name(ty);
        self.name(span, start, &name, &name, NameContext::Type);
    }
    fn target(&mut self, name: &str, span: Span) {
        let Some(index) = self
            .files
            .partition_point(|file| file.base <= span.start)
            .checked_sub(1)
        else {
            return;
        };
        let original = self
            .expansion
            .as_ref()
            .map_or_else(|| name.to_owned(), |expanded| expanded.original_name(name));
        self.targets.insert(
            name.to_owned(),
            symbol_key(&self.files[index].uri, &original),
        );
    }
    fn name(&mut self, span: Span, start: usize, name: &str, target: &str, context: NameContext) {
        let name = self
            .expansion
            .as_ref()
            .map_or_else(|| name.to_owned(), |expanded| expanded.original_name(name));
        let Some(target) = self.targets.get(target).cloned() else {
            return;
        };
        let Some(file_index) = self
            .files
            .partition_point(|file| file.base <= span.start)
            .checked_sub(1)
        else {
            return;
        };
        let file = &self.files[file_index];
        let first = file.tokens.partition_point(|token| token.span.end <= start);
        let token = (first..file.tokens.len())
            .take_while(|index| file.tokens[*index].span.start < span.end)
            .find_map(|index| {
                let token = &file.tokens[index];
                if !matches!(&token.kind, TokenKind::Ident(value) if value == &name) {
                    return None;
                }
                let member = index
                    .checked_sub(1)
                    .is_some_and(|previous| file.tokens[previous].kind == TokenKind::Dot);
                let call = file.tokens.get(index + 1).is_some_and(|next| {
                    matches!(next.kind, TokenKind::LeftParen | TokenKind::LeftBracket)
                });
                let matches = match context {
                    NameContext::Call => call,
                    NameContext::MemberCall => call && member,
                    NameContext::Field => member && !call,
                    NameContext::Declaration => !member,
                    NameContext::Pattern => member,
                    NameContext::Type => true,
                };
                matches.then_some(token)
            });
        if let Some(token) = token {
            self.references.push(DefinitionReference {
                uri: file.uri.clone(),
                span: Span::new(token.span.start - file.base, token.span.end - file.base),
                target: target.to_owned(),
            });
        }
    }
    fn block(&mut self, block: &Block) {
        for statement in block {
            match &statement.kind {
                StmtKind::Var {
                    name,
                    annotation,
                    value,
                } => {
                    if let Some(ty) = annotation {
                        self.type_reference(
                            *ty,
                            Span::new(statement.span.start, value.span.start),
                            TokenKind::Colon,
                        );
                    }
                    self.expression(value);
                    self.local_declaration(name, Span::new(statement.span.start, value.span.start));
                }
                StmtKind::Assign { name, value } => {
                    self.local_use(name, Span::new(statement.span.start, value.span.start));
                    self.expression(value);
                }
                StmtKind::Expr(value) => self.expression(value),
                StmtKind::FieldAssign { target, value } => {
                    self.expression(target);
                    self.expression(value);
                }
                StmtKind::Return(value) => {
                    if let Some(value) = value {
                        self.expression(value);
                    }
                }
                StmtKind::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.expression(condition);
                    self.scoped_block(then_block);
                    if let Some(block) = else_block {
                        self.scoped_block(block);
                    }
                }
                StmtKind::While { condition, body } => {
                    self.expression(condition);
                    self.scoped_block(body);
                }
                StmtKind::For {
                    name,
                    name_span,
                    source,
                    body,
                    ..
                } => {
                    match source {
                        ForSource::Range { start, end, step } => {
                            self.expression(start);
                            self.expression(end);
                            if let Some(step) = step {
                                self.expression(step);
                            }
                        }
                        ForSource::Iterable(value) => self.expression(value),
                    }
                    self.scopes.push(BTreeMap::new());
                    self.local_declaration(name, *name_span);
                    self.block(body);
                    self.scopes.pop();
                }
                StmtKind::Break | StmtKind::Continue => {}
                StmtKind::Match { value, arms } => {
                    self.expression(value);
                    for arm in arms {
                        self.name(
                            arm.span,
                            arm.span.start,
                            &arm.enum_name,
                            &arm.enum_name,
                            NameContext::Type,
                        );
                        self.name(
                            arm.span,
                            arm.span.start,
                            &arm.variant,
                            &ast::method_key(&arm.enum_name, &arm.variant),
                            NameContext::Pattern,
                        );
                        self.scopes.push(BTreeMap::new());
                        for binding in &arm.bindings {
                            self.local_declaration(&binding.name, binding.span);
                        }
                        self.block(&arm.body);
                        self.scopes.pop();
                    }
                }
            }
        }
    }
    fn expression(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call { name, args } => {
                if let Some((owner, method)) = name.rsplit_once('.') {
                    if self
                        .program
                        .enums
                        .iter()
                        .any(|declaration| declaration.name == owner)
                    {
                        self.name(expr.span, expr.span.start, owner, owner, NameContext::Type);
                        self.name(
                            expr.span,
                            expr.span.start,
                            method,
                            name,
                            NameContext::MemberCall,
                        );
                    } else if let Some(receiver) = args.first() {
                        self.name(
                            expr.span,
                            receiver.span.end,
                            method,
                            name,
                            NameContext::MemberCall,
                        );
                    }
                } else {
                    self.name(expr.span, expr.span.start, name, name, NameContext::Call);
                }
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Field { base, name } => {
                if let Some(Type::Record(id)) = base.ty {
                    if let Some(record) = self.program.record(id) {
                        self.name(
                            expr.span,
                            base.span.end,
                            name,
                            &ast::method_key(&record.name, name),
                            NameContext::Field,
                        );
                    }
                }
                self.expression(base);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                // A successful checker normally lowers these. Never guess a method
                // target if a future checker leaves an unresolved call in the tree.
                self.expression(receiver);
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Unary { operand, .. } => self.expression(operand),
            ExprKind::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            ExprKind::Name(name) => self.local_use(name, expr.span),
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {}
        }
    }
}

fn called_name(source: &str, offset: usize) -> Option<String> {
    let tokens = crate::lexer::lex(source).ok()?;
    let index = tokens
        .iter()
        .position(|token| token.span.start <= offset && offset < token.span.end)?;
    if tokens.get(index + 1)?.kind != TokenKind::LeftParen
        || index
            .checked_sub(1)
            .is_some_and(|previous| tokens[previous].kind == TokenKind::Dot)
    {
        return None;
    }
    match &tokens[index].kind {
        TokenKind::Ident(name) => Some(name.clone()),
        _ => None,
    }
}
fn imports(source: &str) -> Result<Vec<ModuleImport>, Vec<Diagnostic>> {
    project::parse_imports(source)
}

fn blank_imports(source: &str, imports: &[ModuleImport]) -> String {
    let file = SourceFile {
        path: PathBuf::new(),
        source: source.to_owned(),
        base: 0,
        import_spans: imports.iter().map(|import| import.span).collect(),
        imports: Vec::new(),
    };
    let mut blanked = project::flattened_source(&[file]);
    blanked.pop(); // Per-file parsing keeps EOF at the original source boundary.
    blanked
}
fn publication(uri: &str, version: Option<i64>, errors: Vec<Value>) -> Value {
    let mut params = BTreeMap::from([
        ("uri".to_owned(), uri.into()),
        ("diagnostics".to_owned(), Value::Array(errors)),
    ]);
    if let Some(version) = version {
        params.insert("version".into(), version.into());
    }
    Value::object([
        ("jsonrpc", "2.0".into()),
        ("method", "textDocument/publishDiagnostics".into()),
        ("params", Value::Object(params)),
    ])
}
fn bounded_diagnostics(errors: Vec<Value>, budget: usize) -> Vec<Value> {
    let mut kept = Vec::new();
    let mut sizes = Vec::new();
    let mut used = 0;
    for error in errors {
        let size = error.to_string().len() + usize::from(!kept.is_empty());
        if used + size > budget {
            let mut notice = diagnostic("", &Diagnostic::new(
                "LSP002", "Additional diagnostics omitted because the report exceeds the 4 MiB message limit", Span::default(),
            ));
            if let Value::Object(fields) = &mut notice {
                fields.insert("severity".into(), 2_i64.into());
            }
            let notice_size = notice.to_string().len();
            while used + notice_size + usize::from(!kept.is_empty()) > budget && !kept.is_empty() {
                kept.pop();
                used -= sizes.pop().unwrap();
            }
            if used + notice_size + usize::from(!kept.is_empty()) <= budget {
                kept.push(notice);
            }
            break;
        }
        used += size;
        sizes.push(size);
        kept.push(error);
    }
    kept
}
fn diagnostic(source: &str, error: &Diagnostic) -> Value {
    Value::object([
        ("range", range(source, error.span)),
        ("severity", 1_i64.into()),
        ("code", error.code.clone().into()),
        ("source", "bhasa".into()),
        ("message", error.message.clone().into()),
    ])
}
fn range(source: &str, span: Span) -> Value {
    Value::object([
        ("start", position(source, span.start)),
        ("end", position(source, span.end.max(span.start))),
    ])
}
fn position(source: &str, offset: usize) -> Value {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let mut line: usize = 0;
    let mut character: usize = 0;
    let mut previous_cr = false;
    for ch in source[..offset].chars() {
        match ch {
            '\r' => {
                line += 1;
                character = 0;
            }
            '\n' => {
                if !previous_cr {
                    line += 1;
                }
                character = 0;
            }
            _ => character += ch.len_utf16(),
        }
        previous_cr = ch == '\r';
    }
    Value::object([
        ("line", Value::from(line)),
        ("character", Value::from(character)),
    ])
}
fn position_offset(source: &str, position: &Value) -> Option<usize> {
    let wanted_line = usize::try_from(position.get("line")?.as_i64()?).ok()?;
    let wanted_character = usize::try_from(position.get("character")?.as_i64()?).ok()?;
    let mut line: usize = 0;
    let mut character: usize = 0;
    let mut chars = source.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if line == wanted_line {
            if character == wanted_character || matches!(ch, '\r' | '\n') {
                return Some(offset);
            }
            if character + ch.len_utf16() > wanted_character {
                return None;
            }
            character += ch.len_utf16();
        }
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                chars.next();
            }
            line += 1;
            character = 0;
        }
    }
    (line == wanted_line).then_some(source.len())
}
fn uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest
        .strip_prefix("localhost/")
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| rest.to_owned());
    if !rest.starts_with('/') || rest.contains(['?', '#']) {
        return None;
    }
    let mut bytes = Vec::new();
    let mut input = rest.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = char::from(input.next()?).to_digit(16)?;
            let low = char::from(input.next()?).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    if path.contains('\0') {
        return None;
    }
    #[cfg(windows)]
    let path = if path.as_bytes().get(2) == Some(&b':') {
        &path[1..]
    } else {
        path.as_str()
    };
    let path = PathBuf::from(path);
    // A new file can be opened before it exists. Resolve its existing ancestors
    // now so a later save through a symlink keeps the same identity as the loader.
    let resolved = path.ancestors().find_map(|ancestor| {
        let parent = ancestor.canonicalize().ok()?;
        Some(parent.join(path.strip_prefix(ancestor).ok()?))
    });
    Some(resolved.unwrap_or(path))
}
fn path_uri(path: &Path) -> String {
    use std::fmt::Write;
    let mut uri = String::from("file://");
    #[cfg(windows)]
    uri.push('/');
    for byte in path.to_string_lossy().replace('\\', "/").bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positions_cover_surrogates_and_all_newline_styles() {
        for newline in ["\n", "\r", "\r\n"] {
            let source = format!("🌍a{newline}हx");
            let pos = position(&source, source.find('x').unwrap());
            assert_eq!(pos.get("line").and_then(Value::as_i64), Some(1));
            assert_eq!(pos.get("character").and_then(Value::as_i64), Some(1));
            assert_eq!(position_offset(&source, &pos), source.find('x'));
            assert_eq!(
                position_offset(
                    &source,
                    &Value::object([("line", 0_i64.into()), ("character", 1_i64.into())])
                ),
                None
            );
        }
    }
    #[test]
    fn file_uris_decode_percent_encoded_utf8() {
        let path = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join("Bhasa files/🌍.bh");
        assert_eq!(uri_path(&path_uri(&path)), Some(path));
        for uri in [
            "https://example.org/a.bh",
            "file://elsewhere/a.bh",
            "file:///tmp/%00",
            "file:///tmp/%zz",
        ] {
            assert_eq!(uri_path(uri), None);
        }
    }
}
