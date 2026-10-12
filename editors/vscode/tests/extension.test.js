"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { EventEmitter } = require("node:events");
const { PassThrough } = require("node:stream");

const tick = () => new Promise((resolve) => setImmediate(resolve));
const locationRange = { start: { line: 0, character: 4 }, end: { line: 0, character: 8 } };

function environment(t, options = {}) {
  const events = {};
  const providers = {};
  const children = [];
  const diagnostics = new Map();
  const errors = [];
  const weakKeys = { writes: 0 };
  const uri = (value) => ({ scheme: value.split(":")[0], fsPath: "/tmp/test", toString: () => value });
  const document = {
    uri: uri(options.documentUri || "file:///tmp/test/main.bh"), languageId: "bhasa", version: 1,
    isClosed: false, text: options.documentText || 'def main() { print("🌍") }',
    getText() { return this.text; }
  };
  const disposable = { dispose() {} };
  const register = (name) => (callback) => { events[name] = callback; return disposable; };
  const provider = (name) => (_, value) => { providers[name] = value; return disposable; };
  const vscode = {
    workspace: {
      isTrusted: options.trusted !== false,
      textDocuments: [document], workspaceFolders: [{ name: "test", uri: uri("file:///tmp/test") }],
      getConfiguration: () => ({ get: () => "/test/bhasa" }),
      onDidOpenTextDocument: register("open"), onDidChangeTextDocument: register("change"),
      onDidSaveTextDocument: register("save"),
      onDidCloseTextDocument: register("close"), onDidGrantWorkspaceTrust: register("trust"),
      onDidChangeConfiguration: register("configuration")
    },
    window: {
      createOutputChannel: () => ({ append() {}, appendLine() {}, show() {}, dispose() {} }),
      showErrorMessage: async (message) => { errors.push(message); }
    },
    languages: {
      createDiagnosticCollection: () => ({
        clear: () => diagnostics.clear(), delete: (key) => diagnostics.delete(key.toString()),
        set: (key, values) => diagnostics.set(key.toString(), values), dispose() {}
      }),
      registerDocumentFormattingEditProvider: provider("format"),
      registerDocumentSymbolProvider: provider("symbols"),
      registerDefinitionProvider: provider("definition"),
      registerCompletionItemProvider: provider("completion"),
      registerHoverProvider: provider("hover"),
      registerReferenceProvider: provider("references"),
      registerRenameProvider: provider("rename")
    },
    commands: { registerCommand: (_, callback) => { events.restart = callback; return disposable; } },
    Uri: { parse: uri }, DiagnosticSeverity: { Error: 0 }, SymbolKind: { Function: 11 },
    Range: class { constructor(a, b, c, d) { this.start = { line: a, character: b }; this.end = { line: c, character: d }; } },
    Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
    CompletionItem: class { constructor(label, kind) { Object.assign(this, { label, kind }); } },
    CompletionList: class { constructor(items, isIncomplete) { Object.assign(this, { items, isIncomplete }); } },
    MarkdownString: class { constructor() { this.value = ""; } appendText(value) { this.value += value; return this; } },
    Hover: class { constructor(contents, range) { Object.assign(this, { contents, range }); } },
    WorkspaceEdit: class { constructor() { this.entries = new Map(); } set(uri, edits) { this.entries.set(uri.toString(), edits); } },
    TextEdit: class { constructor(range, newText) { Object.assign(this, { range, newText }); } },
    Location: class { constructor(uri, range) { Object.assign(this, { uri, range }); } },
    DocumentSymbol: class { constructor(name, detail, kind, range, selectionRange) { Object.assign(this, { name, detail, kind, range, selectionRange }); } },
    SymbolInformation: class { constructor(name, kind, containerName, location) { Object.assign(this, { name, kind, containerName, location }); } }
  };
  const spawn = (command, args, spawnOptions) => {
    assert.equal(command, "/test/bhasa");
    assert.deepEqual(Array.from(args), ["lsp"]);
    assert.equal(spawnOptions.shell, false);
    const child = Object.assign(new EventEmitter(), {
      stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(),
      exitCode: null, signalCode: null, messages: []
    });
    child.kill = (signal) => { child.signalCode = signal || "SIGTERM"; child.stdout.end(); child.emit("close", null, child.signalCode); };
    child.send = (value) => {
      const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...value }));
      child.stdout.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
    };
    child.stdin.on("data", (data) => {
      const message = JSON.parse(data.subarray(data.indexOf("\r\n\r\n") + 4));
      child.messages.push(message);
      queueMicrotask(() => {
        if (message.method === "initialize") {
          child.send({ id: message.id, result: { capabilities: {
            textDocumentSync: { openClose: true, change: 1, save: options.save ?? true }, positionEncoding: "utf-16",
            documentFormattingProvider: true, documentSymbolProvider: true, definitionProvider: true, referencesProvider: true, renameProvider: { prepareProvider: true }, completionProvider: { triggerCharacters: ["."] }, hoverProvider: true
          } } });
        } else if (message.method === "shutdown") child.send({ id: message.id, result: null });
        else if (message.method === "exit") { child.exitCode = 0; child.stdout.end(); child.emit("close", 0, null); }
        else if (message.method === "textDocument/formatting" && !options.delayFormat) {
          child.send({ id: message.id, result: [{ range: locationRange, newText: "main" }] });
        } else if (message.method === "textDocument/documentSymbol") {
          child.send({ id: message.id, result: options.symbols || Array.from({ length: 70 }, (_, index) => ({
            name: `function${index}`, kind: 12, range: locationRange, selectionRange: locationRange
          })) });
        } else if (message.method === "textDocument/completion" && !options.delayCompletion) {
          child.send({ id: message.id, result: options.completion || { isIncomplete: true, items: [{ label: "count", detail: "def count() -> int", kind: 2, textEdit: { range: locationRange, newText: "count" } }] } });
        } else if (message.method === "textDocument/hover" && !options.delayHover) {
          child.send({ id: message.id, result: options.hover || { contents: { kind: "plaintext", value: "var text: buffer [command:unsafe]" }, range: locationRange } });
        } else if (message.method === "textDocument/references") {
          child.send({ id: message.id, result: [{ uri: document.uri.toString(), range: locationRange }] });
        } else if (message.method === "textDocument/prepareRename") {
          child.send({ id: message.id, result: { range: locationRange, placeholder: "main" } });
        } else if (message.method === "textDocument/rename" && !options.delayRename) {
          child.send({ id: message.id, result: options.rename || { documentChanges: [{ textDocument: { uri: document.uri.toString(), version: document.version }, edits: [{ range: locationRange, newText: message.params.newName }] }] } });
        } else if (message.method === "textDocument/definition") {
          child.send({ id: message.id, result: options.definition || { uri: document.uri.toString(), range: locationRange } });
        }
      });
    });
    children.push(child);
    return child;
  };
  const module = { exports: {} };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../extension.js"), "utf8"), {
    require: (name) => name === "vscode" ? vscode : name === "node:child_process" ? { spawn } : name === "node:buffer" ? require("node:buffer") : require("../transport"),
    module, process, setTimeout, clearTimeout,
    WeakMap: class extends WeakMap {
      set(key, value) {
        assert.equal(typeof key, "object");
        weakKeys.writes++;
        return super.set(key, value);
      }
    }
  });
  module.exports.activate({ subscriptions: [] });
  t.after(() => module.exports.deactivate());
  const token = { isCancellationRequested: false, onCancellationRequested: () => disposable };
  return { document, events, providers, vscode, children, diagnostics, errors, token, weakKeys, deactivate: module.exports.deactivate };
}

test("initializes then syncs open/change/close with full unsaved text", async (t) => {
  const env = environment(t);
  await tick();
  const child = env.children[0];
  assert.deepEqual(child.messages.map((m) => m.method), ["initialize", "initialized", "textDocument/didOpen"]);
  assert.deepEqual(child.messages[0].params.capabilities.general.positionEncodings, ["utf-16"]);
  env.document.text = "def main() { unknown() }";
  env.document.version = 2;
  env.events.change({ document: env.document, contentChanges: [{}] });
  await tick();
  assert.deepEqual(child.messages.at(-1).params.contentChanges, [{ text: env.document.text }]);
  assert.equal(child.messages.at(-1).params.textDocument.version, 2);
  env.document.isClosed = true;
  env.events.close(env.document);
  assert.equal(child.messages.at(-1).method, "textDocument/didClose");
  await env.deactivate();
  assert.deepEqual(child.messages.slice(-2).map((m) => m.method), ["shutdown", "exit"]);
  assert.equal(child.exitCode, 0);
  assert.deepEqual(env.errors, []);
});

test("publishes UTF-16 diagnostics, drops stale versions, and clears on close", async (t) => {
  const env = environment(t);
  await tick();
  const child = env.children[0];
  const diagnostic = { range: { start: { line: 0, character: 25 }, end: { line: 0, character: 29 } }, message: "Unknown function", severity: 1, code: "S001" };
  child.send({ method: "textDocument/publishDiagnostics", params: { uri: env.document.uri.toString(), version: 1, diagnostics: [diagnostic] } });
  const found = env.diagnostics.get(env.document.uri.toString());
  assert.equal(found[0].range.start.character, 25);
  assert.equal(found[0].severity, 0);
  env.document.version = 2;
  child.send({ method: "textDocument/publishDiagnostics", params: { uri: env.document.uri.toString(), version: 1, diagnostics: [] } });
  assert.equal(env.diagnostics.get(env.document.uri.toString()).length, 1);
  env.events.close(env.document);
  assert.equal(env.diagnostics.size, 0);
});

test("syncs current text before didSave and sends save even when the version is unchanged", async (t) => {
  const env = environment(t);
  await tick();
  const child = env.children[0];
  env.document.text = 'import "math.bh"\ndef main() { print(answer()) }';
  env.document.version++;
  await env.events.save(env.document);
  assert.deepEqual(child.messages.slice(-2).map((message) => message.method), ["textDocument/didChange", "textDocument/didSave"]);
  assert.equal(child.messages.at(-2).params.contentChanges[0].text, env.document.text);
  assert.deepEqual(child.messages.at(-1).params, { textDocument: { uri: env.document.uri.toString() } });
  const count = child.messages.length;
  await env.events.save(env.document);
  assert.equal(child.messages.length, count + 1);
  assert.equal(child.messages.at(-1).method, "textDocument/didSave");
  env.document.isClosed = true;
  await env.events.save(env.document);
  assert.equal(child.messages.length, count + 1);
});

test("respects save capability and optional saved text", async (t) => {
  const withText = environment(t, { save: { includeText: true } });
  await tick();
  await withText.events.save(withText.document);
  assert.equal(withText.children[0].messages.at(-1).params.text, withText.document.text);
  const withoutSave = environment(t, { save: false });
  await tick();
  await withoutSave.events.save(withoutSave.document);
  assert.equal(withoutSave.children[0].messages.filter((message) => message.method === "textDocument/didSave").length, 0);
});

test("maps formatting, symbols and definitions through native providers", async (t) => {
  const env = environment(t);
  await tick();
  const edits = await env.providers.format.provideDocumentFormattingEdits(env.document, { tabSize: 4, insertSpaces: true }, env.token);
  assert.equal(edits[0].newText, "main");
  const symbols = await env.providers.symbols.provideDocumentSymbols(env.document, env.token);
  assert.equal(symbols.length, 70);
  assert.equal(symbols[69].kind, 11);
  const definitions = await env.providers.definition.provideDefinition(env.document, { line: 2, character: 15 }, env.token);
  assert.equal(definitions[0].range.start.character, 4);
  assert.equal(env.children[0].messages.at(-1).params.position.character, 15);
});

test("preserves hierarchical record, field and method symbols with native kinds and ranges", async (t) => {
  const methodLine = "    def total(self) -> int { return self.x }";
  const recordRange = { start: { line: 0, character: 0 }, end: { line: 3, character: 1 } };
  const recordSelection = { start: { line: 0, character: 7 }, end: { line: 0, character: 12 } };
  const fieldRange = { start: { line: 1, character: 4 }, end: { line: 1, character: 10 } };
  const fieldSelection = { start: { line: 1, character: 4 }, end: { line: 1, character: 5 } };
  const methodRange = { start: { line: 2, character: 4 }, end: { line: 2, character: methodLine.length } };
  const methodSelection = { start: { line: 2, character: 8 }, end: { line: 2, character: 13 } };
  const env = environment(t, {
    documentText: `struct Point {\n    x: int\n${methodLine}\n}\n`,
    symbols: [{
      name: "Point", kind: 23, range: recordRange, selectionRange: recordSelection,
      children: [
        { name: "x", detail: "int", kind: 8, range: fieldRange, selectionRange: fieldSelection },
        { name: "total", detail: "(self) -> int", kind: 6, range: methodRange, selectionRange: methodSelection }
      ]
    }]
  });
  await tick();
  const initialize = env.children[0].messages[0];
  assert.equal(initialize.params.capabilities.textDocument.documentSymbol.hierarchicalDocumentSymbolSupport, true);
  const symbols = await env.providers.symbols.provideDocumentSymbols(env.document, env.token);
  assert.equal(symbols.length, 1);
  assert.equal(symbols[0].name, "Point");
  assert.equal(symbols[0].kind, 22); // VS Code Struct, LSP Struct is 23.
  assert.deepEqual(symbols[0].range.start, recordRange.start);
  assert.deepEqual(symbols[0].range.end, recordRange.end);
  assert.deepEqual(symbols[0].selectionRange.start, recordSelection.start);
  assert.equal(symbols[0].children.length, 2);
  const expectedChildren = [
    { name: "x", detail: "int", kind: 7, range: fieldRange, selection: fieldSelection },
    { name: "total", detail: "(self) -> int", kind: 5, range: methodRange, selection: methodSelection }
  ];
  for (const [index, expected] of expectedChildren.entries()) {
    const actual = symbols[0].children[index];
    assert.equal(actual.name, expected.name);
    assert.equal(actual.kind, expected.kind);
    assert.equal(actual.detail, expected.detail);
    assert.deepEqual(actual.range.start, expected.range.start);
    assert.deepEqual(actual.range.end, expected.range.end);
    assert.deepEqual(actual.selectionRange.start, expected.selection.start);
    assert.deepEqual(actual.selectionRange.end, expected.selection.end);
    assert.equal(actual.children.length, 0);
  }
});

test("passes UTF-16 method definition positions and preserves imported target URIs", async (t) => {
  const targetUri = "file:///tmp/Bhasa%20files/%F0%9F%8C%8D/point.bh";
  const targetSelection = { start: { line: 2, character: 8 }, end: { line: 2, character: 13 } };
  const targetRange = { start: { line: 2, character: 4 }, end: { line: 4, character: 5 } };
  const line = 'def main() { var point = Point(42); print("🌍"); print(point.total()) }';
  const character = line.indexOf("total") + 2;
  assert.notEqual(character, Buffer.byteLength(line.slice(0, character)));
  for (const [name, definition] of [
    ["Location", { uri: targetUri, range: targetSelection }],
    ["LocationLink", [{ targetUri, targetRange, targetSelectionRange: targetSelection }]]
  ]) {
    await t.test(name, async (t) => {
      const documentUri = "file:///tmp/Bhasa%20files/main.bh";
      const env = environment(t, { documentUri, documentText: `import "point.bh"\r\n${line}\r\n`, definition });
      await tick();
      const locations = await env.providers.definition.provideDefinition(env.document, { line: 1, character }, env.token);
      const request = env.children[0].messages.at(-1);
      assert.equal(request.params.textDocument.uri, documentUri);
      assert.deepEqual(request.params.position, { line: 1, character });
      assert.equal(locations.length, 1);
      assert.equal(locations[0].uri.toString(), targetUri);
      assert.deepEqual(locations[0].range.start, targetSelection.start);
      assert.deepEqual(locations[0].range.end, targetSelection.end);
    });
  }
});

test("discards a formatter result when the editor changed meanwhile", async (t) => {
  const env = environment(t, { delayFormat: true });
  await tick();
  const pending = env.providers.format.provideDocumentFormattingEdits(env.document, {}, env.token);
  await tick();
  const request = env.children[0].messages.at(-1);
  env.document.version++;
  env.children[0].send({ id: request.id, result: [{ range: locationRange, newText: "stale" }] });
  assert.equal((await pending).length, 0);
});

test("workspace trust gates spawning and restart reopens documents", async (t) => {
  const env = environment(t, { trusted: false });
  await tick();
  assert.equal(env.children.length, 0);
  env.vscode.workspace.isTrusted = true;
  await env.events.trust();
  assert.equal(env.children.length, 1);
  await env.events.restart();
  assert.equal(env.children[0].exitCode, 0);
  assert.equal(env.children.length, 2);
  assert.equal(env.children[1].messages.at(-1).method, "textDocument/didOpen");
});

test("malformed server output stops the child and permits manual restart", async (t) => {
  const env = environment(t);
  await tick();
  env.children[0].stdout.write("Content-Length: nope\r\n\r\n");
  await tick();
  assert.equal(env.errors.length, 1);
  assert.equal(env.children[0].signalCode, "SIGTERM");
  env.events.change({ document: env.document, contentChanges: [{}] });
  await tick();
  assert.equal(env.children.length, 1);
  await env.events.restart();
  assert.equal(env.children.length, 2);
});


test("completion and hover preserve UTF-16 edits, native kinds and untrusted text", async (t) => {
  const env = environment(t);
  await tick();
  const position = { line: 0, character: 31 };
  const list = await env.providers.completion.provideCompletionItems(env.document, position, env.token);
  assert.equal(list.isIncomplete, true);
  assert.equal(list.items[0].kind, 1);
  assert.equal(list.items[0].detail, "def count() -> int");
  assert.equal(list.items[0].insertText, "count");
  assert.deepEqual(list.items[0].range.start, locationRange.start);
  assert.deepEqual(env.children[0].messages.at(-1).params.position, position);
  const hover = await env.providers.hover.provideHover(env.document, position, env.token);
  assert.equal(hover.contents.value, "var text: buffer [command:unsafe]");
  assert.equal(hover.contents.isTrusted, false);
  assert.deepEqual(hover.range.end, locationRange.end);
});

test("completion and hover discard stale responses", async (t) => {
  for (const method of ["completion", "hover"]) {
    await t.test(method, async (t) => {
      const env = environment(t, { delayCompletion: true, delayHover: true });
      await tick();
      const pending = method === "completion"
        ? env.providers.completion.provideCompletionItems(env.document, { line: 0, character: 1 }, env.token)
        : env.providers.hover.provideHover(env.document, { line: 0, character: 1 }, env.token);
      await tick();
      const request = env.children[0].messages.at(-1);
      env.document.version++;
      env.children[0].send({ id: request.id, result: method === "completion" ? { items: [{ label: "stale" }] } : { contents: { value: "stale" } } });
      const result = await pending;
      if (method === "completion") assert.equal(result.items.length, 0);
      else assert.equal(result, null);
    });
  }
});

test("references and prepareRename use native providers with exact UTF-16 positions", async (t) => {
  const env = environment(t); await tick();
  const refs = await env.providers.references.provideReferences(env.document, { line: 0, character: 5 }, { includeDeclaration: false }, env.token);
  assert.equal(refs.length, 1);
  assert.equal(env.children[0].messages.at(-1).params.context.includeDeclaration, false);
  const prepared = await env.providers.rename.prepareRename(env.document, { line: 0, character: 5 }, env.token);
  assert.equal(prepared.placeholder, "main");
  assert.deepEqual(prepared.range.start, locationRange.start);
});

function renameResponse(documents, name = "renamed") {
  return { documentChanges: documents.map((document) => ({
    textDocument: { uri: document.uri.toString(), version: document.version },
    edits: [{ range: locationRange, newText: name }]
  })) };
}
function secondaryDocument(env) {
  const secondary = { ...env.document, uri: env.vscode.Uri.parse("file:///tmp/test/library.bh"), version: 7 };
  env.vscode.workspace.textDocuments.push(secondary);
  env.events.open(secondary);
  return secondary;
}

test("rename validates and converts every versioned document without applying edits", async (t) => {
  const env = environment(t, { delayRename: true }); await tick();
  const other = secondaryDocument(env);
  const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
  await tick();
  const request = env.children[0].messages.at(-1);
  assert.equal(request.method, "textDocument/rename");
  env.children[0].send({ id: request.id, result: renameResponse([env.document, other]) });
  const edit = await pending;
  assert.equal(edit.entries.size, 2);
  assert.equal(edit.entries.get(other.uri.toString())[0].newText, "renamed");
  assert.equal(other.version, 7);
  assert.equal(env.document.text, 'def main() { print("🌍") }');
});

test("rename discards stale secondary files, close/reopen, cancellation and restart", async (t) => {
  for (const action of ["changed", "closed", "reopened", "same-object-reopened", "canceled", "restart"]) {
    await t.test(action, async (t) => {
      const env = environment(t, { delayRename: true }); await tick();
      const other = secondaryDocument(env); await tick();
      const response = renameResponse([env.document, other]);
      const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
      await tick();
      const child = env.children[0]; const request = child.messages.at(-1);
      if (action === "changed") { other.version++; other.text += "\n"; env.events.change({ document: other, contentChanges: [{}] }); }
      if (["closed", "reopened", "same-object-reopened"].includes(action)) {
        other.isClosed = true; env.events.close(other);
        if (action !== "closed") {
          const reopened = action === "same-object-reopened" ? other : { ...other };
          reopened.isClosed = false;
          env.vscode.workspace.textDocuments[1] = reopened;
          env.events.open(reopened);
        }
      }
      if (action === "canceled") env.token.isCancellationRequested = true;
      if (action === "restart") await env.events.restart();
      else child.send({ id: request.id, result: response });
      assert.equal(await pending, null);
    });
  }
});

test("rename rejects malformed, partial, overlapping, unopened and unversioned edits", async (t) => {
  for (const invalid of ["missing-version", "wrong-version", "unopened", "overlap", "partial", "out-of-range", "replacement", "resource", "duplicate", "legacy-changes"]) {
    await t.test(invalid, async (t) => {
      const env = environment(t, { delayRename: true }); await tick();
      const response = renameResponse([env.document]); const change = response.documentChanges[0];
      if (invalid === "missing-version") change.textDocument.version = null;
      if (invalid === "wrong-version") change.textDocument.version++;
      if (invalid === "unopened") change.textDocument.uri = "file:///tmp/closed.bh";
      if (invalid === "overlap") change.edits.push({ ...change.edits[0] });
      if (invalid === "partial") change.edits[0].range = { start: { line: 0, character: 5 }, end: { line: 0, character: 8 } };
      if (invalid === "out-of-range") change.edits[0].range = { start: { line: 999, character: 0 }, end: { line: 999, character: 4 } };
      if (invalid === "replacement") change.edits[0].newText = "malicious";
      if (invalid === "resource") change.kind = "delete";
      if (invalid === "duplicate") response.documentChanges.push({ ...change });
      if (invalid === "legacy-changes") response.changes = {};
      const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
      await tick();const request = env.children[0].messages.at(-1);
      env.children[0].send({ id: request.id, result: response });
      assert.equal(await pending, null);
    });
  }
});

test("rename rejects changes to a previously unedited importer or the open inventory", async (t) => {
  for (const action of ["changed-unedited", "closed-unedited", "reopened-unedited", "new-open"]) {
    await t.test(action, async (t) => {
      const env = environment(t, { delayRename: true }); await tick();
      const other = action === "new-open" ? null : secondaryDocument(env); await tick();
      const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
      await tick(); const request = env.children[0].messages.at(-1);
      if (action === "changed-unedited") { other.text += "\nfoo()"; other.version++; env.events.change({ document: other, contentChanges: [{}] }); }
      if (action === "closed-unedited" || action === "reopened-unedited") {
        other.isClosed = true; env.events.close(other);
        if (action === "reopened-unedited") { other.isClosed = false; env.events.open(other); }
      }
      if (action === "new-open") secondaryDocument(env);
      env.children[0].send({ id: request.id, result: renameResponse([env.document]) });
      assert.equal(await pending, null);
    });
  }
});

test("rename maps UTF-16 line positions once per document for CRLF and CR sources", async (t) => {
  for (const ending of ["\r\n", "\r"]) {
    await t.test(JSON.stringify(ending), async (t) => {
      const line = 'def main(){print("🌍");var value=1;print(value)}';
      const env = environment(t, { delayRename: true, documentText: `# header${ending}${line}` }); await tick();
      const ranges = [line.indexOf("value"), line.lastIndexOf("value")].map((character) => ({ start: { line: 1, character }, end: { line: 1, character: character + 5 } }));
      const pending = env.providers.rename.provideRenameEdits(env.document, ranges[0].start, "number", env.token);
      await tick(); const request = env.children[0].messages.at(-1);
      env.children[0].send({ id: request.id, result: { documentChanges: [{ textDocument: { uri: env.document.uri.toString(), version: 1 }, edits: ranges.map((range) => ({ range, newText: "number" })) }] } });
      const edit = await pending;
      assert.equal(edit.entries.get(env.document.uri.toString()).length, 2);
      assert.equal(edit.entries.get(env.document.uri.toString())[0].range.start.character, line.indexOf("value"));
    });
  }
});

test("rename refuses snapshots exceeding server count, UTF-8, URI or framed JSON limits", async (t) => {
  for (const limit of ["count", "utf8-total", "escaped-message", "unicode-message", "uri", "version"]) {
    await t.test(limit, async (t) => {
      const env = environment(t, { delayRename: true }); await tick();
      const count = limit === "count" ? 128 : limit === "utf8-total" ? 33 : 1;
      const source = limit === "utf8-total" ? "🌍".repeat(256 * 1024)
        : limit === "escaped-message" ? '"'.repeat(2 * 1024 * 1024)
        : limit === "unicode-message" ? "🌍".repeat(1024 * 1024) : "def main(){}";
      for (let index = 0; index < count; index++) {
        env.vscode.workspace.textDocuments.push({ ...env.document,
          uri: env.vscode.Uri.parse(`file:///tmp/test/${limit === "uri" ? "x".repeat(16 * 1024) : index}.bh`),
          text: source, version: limit === "version" ? NaN : 1
        });
      }
      const result = await env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
      assert.equal(result, null);
      assert.equal(env.children[0].messages.some((message) => message.method === "textDocument/rename"), false);
      assert.equal(env.children[0].messages.filter((message) => message.method === "textDocument/didOpen").length, 1);
    });
  }
});

test("over-limit documents are never marked sent and synchronize once capacity is available", async (t) => {
  const env = environment(t, { delayRename: true }); await tick();
  for (let index = 0; index < 128; index++) {
    const document = { ...env.document, uri: env.vscode.Uri.parse(`file:///tmp/test/extra${index}.bh`) };
    env.vscode.workspace.textDocuments.push(document);
    env.events.open(document);
  }
  await tick();
  const child = env.children[0], last = env.vscode.workspace.textDocuments.at(-1);
  const opens = () => child.messages.filter((message) => message.method === "textDocument/didOpen");
  assert.equal(opens().length, 128);
  assert.equal(opens().some((message) => message.params.textDocument.uri === last.uri.toString()), false);
  const closed = env.vscode.workspace.textDocuments[1];
  closed.isClosed = true; env.events.close(closed);
  const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
  await tick();
  assert.equal(opens().filter((message) => message.params.textDocument.uri === last.uri.toString()).length, 1);
  const request = child.messages.at(-1); assert.equal(request.method, "textDocument/rename");
  child.send({ id: request.id, result: renameResponse([env.document, last]) });
  assert.equal((await pending).entries.size, 2);
});

test("rename rejects failed notification enqueue and retries unsent open or changed versions", async (t) => {
  for (const method of ["didOpen", "didChange"]) {
    for (const failure of ["queue-limit", "write-error"]) {
      await t.test(`${method}/${failure}`, async (t) => {
        const env = environment(t, { delayRename: true }); await tick();
        const other = { ...env.document, uri: env.vscode.Uri.parse("file:///tmp/test/library.bh") };
        env.vscode.workspace.textDocuments.push(other);
        if (method === "didChange") { env.events.open(other); await tick(); other.version++; other.text += "\n"; }
        const child = env.children[0];
        const sent = () => child.messages.filter((message) => message.method === `textDocument/${method}` && message.params.textDocument.uri === other.uri.toString()).length;
        const before = sent();
        const write = child.stdin.write;
        if (failure === "queue-limit") Object.defineProperty(child.stdin, "writableLength", { configurable: true, value: 8 * 1024 * 1024 });
        else child.stdin.write = () => { throw new Error("Simulated notification enqueue failure"); };
        const rejected = await env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
        assert.equal(rejected, null);
        assert.equal(sent(), before);
        assert.equal(child.messages.some((message) => message.method === "textDocument/rename"), false);
        if (failure === "queue-limit") delete child.stdin.writableLength;
        else child.stdin.write = write;
        const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
        await tick();
        assert.equal(sent(), before + 1);
        const request = child.messages.at(-1); assert.equal(request.method, "textDocument/rename");
        child.send({ id: request.id, result: renameResponse([env.document, other]) });
        assert.equal((await pending).entries.size, 2);
      });
    }
  }
});

test("close history uses weak document keys and ignores plaintext or absent sessions", async (t) => {
  const env = environment(t, { trusted: false, delayRename: true }); await tick();
  const closeHistory = (languageId) => {
    for (let index = 0; index < 256; index++) {
      env.events.close({ ...env.document, languageId, isClosed: true,
        uri: env.vscode.Uri.parse(`file:///tmp/test/history${index}.${languageId === "bhasa" ? "bh" : "txt"}`) });
    }
  };
  closeHistory("plaintext"); closeHistory("bhasa");
  assert.equal(env.children.length, 0);
  assert.equal(env.weakKeys.writes, 0);
  env.vscode.workspace.isTrusted = true; await env.events.trust();
  closeHistory("plaintext");
  assert.equal(env.weakKeys.writes, 0);
  const pending = env.providers.rename.provideRenameEdits(env.document, { line: 0, character: 5 }, "renamed", env.token);
  await tick(); const child = env.children[0], request = child.messages.at(-1);
  assert.equal(request.method, "textDocument/rename");
  env.document.isClosed = true; env.events.close(env.document);
  assert.equal(env.weakKeys.writes, 1);
  env.document.isClosed = false; env.events.open(env.document);
  child.send({ id: request.id, result: renameResponse([env.document]) });
  assert.equal(await pending, null);
});
