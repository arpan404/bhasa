"use strict";

const vscode = require("vscode");
const { spawn } = require("node:child_process");
const { Buffer } = require("node:buffer");
const { RpcConnection } = require("./transport");

const selector = [{ language: "bhasa", scheme: "file" }, { language: "bhasa", scheme: "untitled" }];
const isBhasa = (document) => document.languageId === "bhasa" && ["file", "untitled"].includes(document.uri.scheme);
let controller;

class LanguageTools {
  constructor(context) {
    this.output = vscode.window.createOutputChannel("Bhasa");
    this.diagnostics = vscode.languages.createDiagnosticCollection("bhasa");
    this.session = null;
    this.starting = null;
    this.disabled = false;
    this.disposed = false;
    this.restartQueue = Promise.resolve();
    this.documentEpochs = new WeakMap();
    context.subscriptions.push(this.output, this.diagnostics,
      vscode.workspace.onDidOpenTextDocument((document) => this.sync(document)),
      vscode.workspace.onDidChangeTextDocument((event) => {
        if (event.contentChanges.length) this.sync(event.document);
      }),
      vscode.workspace.onDidSaveTextDocument((document) => this.saveDocument(document)),
      vscode.workspace.onDidCloseTextDocument((document) => this.closeDocument(document)),
      vscode.workspace.onDidGrantWorkspaceTrust(() => this.restart()),
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration("bhasa.serverPath")) this.restart();
      }),
      vscode.commands.registerCommand("bhasa.restartServer", () => this.restart()),
      vscode.languages.registerDocumentFormattingEditProvider(selector, {
        provideDocumentFormattingEdits: async (document, options, token) => {
          const edits = await this.query(document, "textDocument/formatting", { options }, token);
          return Array.isArray(edits) ? edits.map((edit) => new vscode.TextEdit(range(edit.range), edit.newText)) : [];
        }
      }),
      vscode.languages.registerDocumentSymbolProvider(selector, {
        provideDocumentSymbols: async (document, token) => {
          const symbols = await this.query(document, "textDocument/documentSymbol", {}, token);
          return Array.isArray(symbols) ? symbols.map((item) => symbol(item)) : [];
        }
      }),
      vscode.languages.registerCompletionItemProvider(selector, {
        provideCompletionItems: async (document, position, token) => {
          const result = await this.query(document, "textDocument/completion", {
            position: { line: position.line, character: position.character }
          }, token);
          const items = Array.isArray(result) ? result : result?.items || [];
          return new vscode.CompletionList(items.map((item) => {
            const value = new vscode.CompletionItem(item.label, Number.isInteger(item.kind) ? item.kind - 1 : undefined);
            value.detail = item.detail;
            value.sortText = item.sortText;
            value.filterText = item.filterText;
            if (item.textEdit) {
              value.range = range(item.textEdit.range);
              value.insertText = item.textEdit.newText;
            } else value.insertText = item.insertText || item.label;
            return value;
          }), result?.isIncomplete === true);
        }
      }, "."),
      vscode.languages.registerHoverProvider(selector, {
        provideHover: async (document, position, token) => {
          const result = await this.query(document, "textDocument/hover", {
            position: { line: position.line, character: position.character }
          }, token);
          if (!result?.contents?.value) return null;
          // Compiler source is untrusted text, never trusted Markdown or command links.
          const content = new vscode.MarkdownString();
          content.appendText(result.contents.value);
          content.isTrusted = false;
          return new vscode.Hover(content, result.range ? range(result.range) : undefined);
        }
      }),
      vscode.languages.registerReferenceProvider(selector, {
        provideReferences: async (document, position, context, token) => {
          const result = await this.query(document, "textDocument/references", {
            position: { line: position.line, character: position.character },
            context: { includeDeclaration: context.includeDeclaration === true }
          }, token);
          return Array.isArray(result) ? result.map((item) => new vscode.Location(vscode.Uri.parse(item.uri), range(item.range))) : [];
        }
      }),
      vscode.languages.registerRenameProvider(selector, {
        prepareRename: async (document, position, token) => {
          const result = await this.query(document, "textDocument/prepareRename", {
            position: { line: position.line, character: position.character }
          }, token);
          return result ? { range: range(result.range), placeholder: result.placeholder } : null;
        },
        provideRenameEdits: async (document, position, newName, token) => {
          const result = await this.query(document, "textDocument/rename", {
            position: { line: position.line, character: position.character }, newName
          }, token);
          if (!result) return null;
          const edit = new vscode.WorkspaceEdit();
          for (const change of result.documentChanges) {
            edit.set(vscode.Uri.parse(change.textDocument.uri), change.edits.map((item) => new vscode.TextEdit(range(item.range), item.newText)));
          }
          return edit;
        }
      }),
      vscode.languages.registerDefinitionProvider(selector, {
        provideDefinition: async (document, position, token) => {
          const result = await this.query(document, "textDocument/definition", {
            position: { line: position.line, character: position.character }
          }, token);
          const locations = result == null ? [] : Array.isArray(result) ? result : [result];
          return locations.map((location) => new vscode.Location(
            vscode.Uri.parse(location.uri || location.targetUri),
            range(location.range || location.targetSelectionRange)
          ));
        }
      })
    );
    for (const document of vscode.workspace.textDocuments) this.sync(document);
  }

  log(message) {
    if (!this.disposed) this.output.appendLine(message);
  }

  start() {
    if (this.disposed || this.disabled || !vscode.workspace.isTrusted) return Promise.resolve(null);
    if (this.starting) return this.starting;
    if (this.session?.ready) return Promise.resolve(this.session);
    this.starting = this.launch().finally(() => { this.starting = null; });
    return this.starting;
  }

  async launch() {
    let session;
    try {
      const command = vscode.workspace.getConfiguration("bhasa").get("serverPath", "bhasa");
      if (typeof command !== "string" || !command.trim()) throw new Error("Set bhasa.serverPath to a Bhasa executable.");
      const folders = vscode.workspace.workspaceFolders || [];
      const child = spawn(command, ["lsp"], {
        cwd: folders.find((folder) => folder.uri.scheme === "file")?.uri.fsPath,
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
        shell: false
      });
      const rpc = new RpcConnection(child.stdout, child.stdin);
      session = { child, rpc, ready: false, stopping: false, documents: new Map(), documentBytes: new Map() };
      this.session = session;
      child.stderr.setEncoding("utf8");
      child.stderr.on("data", (data) => { if (!this.disposed) this.output.append(data); });
      child.on("error", (error) => rpc.close(error));
      child.on("close", (code, signal) => rpc.close(new Error(`Bhasa language server exited with ${signal || code}.`)));
      rpc.on("failure", (error) => this.failed(session, error));
      rpc.on("notification", (method, params) => {
        if (session !== this.session || session.stopping) return;
        if (method === "textDocument/publishDiagnostics") this.publishDiagnostics(params);
        else if (method === "window/logMessage" || method === "window/showMessage") this.log(params?.message || "");
      });
      const result = await rpc.request("initialize", {
        processId: process.pid,
        clientInfo: { name: "bhasa-vscode", version: "0.1.0" },
        rootUri: folders[0]?.uri.toString() || null,
        workspaceFolders: folders.map((folder) => ({ uri: folder.uri.toString(), name: folder.name })),
        capabilities: {
          general: { positionEncodings: ["utf-16"] },
          textDocument: {
            synchronization: { dynamicRegistration: false },
            publishDiagnostics: { versionSupport: true },
            documentSymbol: { hierarchicalDocumentSymbolSupport: true },
            definition: { linkSupport: true },
            completion: { completionItem: { snippetSupport: false } },
            hover: { contentFormat: ["plaintext"] },
            references: { dynamicRegistration: false },
            rename: { dynamicRegistration: false, prepareSupport: true },
            formatting: { dynamicRegistration: false }
          }
        }
      }, undefined, 30000);
      if (session !== this.session || session.stopping) return null;
      const capabilities = result?.capabilities;
      const sync = capabilities?.textDocumentSync;
      if (!capabilities || (capabilities.positionEncoding && capabilities.positionEncoding !== "utf-16")) {
        throw new Error("Bhasa language server must use UTF-16 positions.");
      }
      if ((typeof sync === "number" ? sync : sync?.change) !== 1) {
        throw new Error("Bhasa language server must support full document synchronization.");
      }
      session.capabilities = capabilities;
      rpc.notify("initialized", {});
      session.ready = true;
      for (const document of vscode.workspace.textDocuments) this.sendDocument(session, document);
      this.log("Bhasa language server ready.");
      return session;
    } catch (error) {
      if (!session || !session.stopping) this.failed(session, error);
      return null;
    }
  }

  failed(session, error) {
    if (session?.stopping || this.disposed || this.disabled) return;
    this.disabled = true;
    if (session) {
      session.stopping = true;
      session.rpc.close(error);
      session.child.kill();
      const forceKill = setTimeout(() => session.child.kill("SIGKILL"), 1500);
      forceKill.unref();
      session.child.once("close", () => clearTimeout(forceKill));
    }
    this.session = null;
    this.diagnostics.clear();
    this.log(error.message);
    void vscode.window.showErrorMessage(
      `Bhasa: ${error.message} Check bhasa.serverPath, then run "Bhasa: Restart language server".`,
      "Show output"
    ).then((choice) => { if (choice && !this.disposed) this.output.show(true); });
  }

  async sync(document) {
    if (!isBhasa(document)) return;
    const session = await this.start();
    if (!session) return;
    try { this.sendDocument(session, document); }
    catch (error) { this.failed(session, error); }
  }

  sendDocument(session, document) {
    if (!session.ready || session.stopping || document.isClosed || !isBhasa(document)) return false;
    const uri = document.uri.toString();
    const previous = session.documents.get(uri);
    if (previous === document.version) return true;
    if (!Number.isSafeInteger(document.version) || (previous !== undefined && previous >= document.version)) return false;
    const source = document.getText();
    const bytes = Buffer.byteLength(source, "utf8");
    const total = Array.from(session.documentBytes.values()).reduce((sum, size) => sum + size, 0) - (session.documentBytes.get(uri) || 0) + bytes;
    if ((previous === undefined && session.documents.size >= 128) || total > 32 * 1024 * 1024 || !syncFits(document, source)) return false;
    if (previous === undefined) {
      session.rpc.notify("textDocument/didOpen", {
        textDocument: { uri, languageId: "bhasa", version: document.version, text: source }
      });
    } else {
      session.rpc.notify("textDocument/didChange", {
        textDocument: { uri, version: document.version },
        contentChanges: [{ text: source }]
      });
    }
    session.documents.set(uri, document.version);
    session.documentBytes.set(uri, bytes);
    return true;
  }

  closeDocument(document) {
    const session = this.session;
    const uri = document.uri.toString();
    if (session && isBhasa(document)) this.documentEpochs.set(document, (this.documentEpochs.get(document) || 0) + 1);
    this.diagnostics.delete(document.uri);
    session?.documentBytes.delete(uri);
    if (!session?.documents.delete(uri)) return;
    try { session.rpc.notify("textDocument/didClose", { textDocument: { uri } }); }
    catch (error) { this.failed(session, error); }
  }

  async saveDocument(document) {
    if (!isBhasa(document) || document.isClosed) return;
    const session = await this.start();
    if (!session) return;
    try {
      if (!this.sendDocument(session, document)) return;
      const uri = document.uri.toString();
      const save = session.capabilities.textDocumentSync?.save;
      if (!save || session.stopping || session !== this.session || document.isClosed || !session.documents.has(uri)) return;
      session.rpc.notify("textDocument/didSave", {
        textDocument: { uri },
        ...(save.includeText ? { text: document.getText() } : {})
      });
    } catch (error) { this.failed(session, error); }
  }

  publishDiagnostics(params) {
    if (typeof params?.uri !== "string" || !Array.isArray(params.diagnostics)) return;
    const document = vscode.workspace.textDocuments.find((item) => item.uri.toString() === params.uri);
    if (document && typeof params.version === "number" && params.version !== document.version) return;
    const values = params.diagnostics.map((item) => {
      const severity = Number.isInteger(item.severity) && item.severity >= 1 && item.severity <= 4
        ? item.severity - 1 : vscode.DiagnosticSeverity.Error;
      const diagnostic = new vscode.Diagnostic(range(item.range), item.message, severity);
      diagnostic.source = item.source || "bhasa";
      diagnostic.code = item.code;
      return diagnostic;
    });
    this.diagnostics.set(vscode.Uri.parse(params.uri), values);
  }

  async query(document, method, extra, token) {
    const session = await this.start();
    if (!session || token.isCancellationRequested || document.isClosed) return null;
    const capability = {
      "textDocument/formatting": "documentFormattingProvider",
      "textDocument/documentSymbol": "documentSymbolProvider",
      "textDocument/definition": "definitionProvider",
      "textDocument/references": "referencesProvider",
      "textDocument/prepareRename": "renameProvider",
      "textDocument/rename": "renameProvider",
      "textDocument/completion": "completionProvider",
      "textDocument/hover": "hoverProvider"
    }[method];
    if (!session.capabilities[capability]) return null;
    const version = document.version;
    try {
      if (!this.sendDocument(session, document)) return null;
      const rename = method === "textDocument/rename";
      const snapshots = new Map();
      if (rename) {
        const documents = vscode.workspace.textDocuments.filter((open) => isBhasa(open) && !open.isClosed);
        validateRenameDocuments(documents);
        for (const open of documents) {
          if (!this.sendDocument(session, open)) throw new Error("A rename input could not be synchronized.");
          const uri = open.uri.toString();
          snapshots.set(uri, { document: open, version: open.version, epoch: this.documentEpochs.get(open) || 0 });
        }
      }
      const result = await session.rpc.request(method, {
        textDocument: { uri: document.uri.toString() }, ...extra
      }, token);
      // In particular, never apply a formatter response to text edited meanwhile.
      if (document.version !== version || document.isClosed || session !== this.session || token.isCancellationRequested) return null;
      if (rename && result) {
        validateRename(result, extra.newName, snapshots, this.documentEpochs);
      }
      return result;
    } catch (error) {
      if (error.code !== -32800 && !session.stopping) this.log(`${method}: ${error.message}`);
      return null;
    }
  }

  restart() {
    this.restartQueue = this.restartQueue.then(async () => {
      this.disabled = true;
      await this.stop();
      await this.starting;
      this.disabled = false;
      if (!this.disposed) await this.start();
    }).catch((error) => this.log(error.message));
    return this.restartQueue;
  }

  async stop() {
    const session = this.session;
    this.session = null;
    if (!this.disposed) this.diagnostics.clear();
    if (!session) return;
    session.stopping = true;
    if (session.ready && !session.rpc.closed) {
      try {
        await session.rpc.request("shutdown", null, undefined, 1000);
        session.rpc.notify("exit");
      } catch (_) { /* Kill a server that cannot complete shutdown. */ }
    }
    session.rpc.close();
    session.child.stdin.end();
    if (session.child.exitCode !== null || session.child.signalCode !== null) return;
    await new Promise((resolve) => {
      const timeout = setTimeout(() => {
        session.child.kill("SIGKILL");
        resolve();
      }, 1500);
      session.child.once("close", () => { clearTimeout(timeout); resolve(); });
    });
  }
}

// These are the server's admission limits. Notifications have no response:
// never mark an over-limit document as sent and infer an acknowledgement.
function syncFits(document, source) {
  const uri = document.uri.toString();
  if (Buffer.byteLength(uri, "utf8") > 16 * 1024 || !Number.isSafeInteger(document.version)) return false;
  const messages = [
    { jsonrpc: "2.0", method: "textDocument/didOpen", params: { textDocument: { uri, languageId: "bhasa", version: document.version, text: source } } },
    { jsonrpc: "2.0", method: "textDocument/didChange", params: { textDocument: { uri, version: document.version }, contentChanges: [{ text: source }] } }
  ];
  return messages.every((message) => Buffer.byteLength(JSON.stringify(message), "utf8") <= 4 * 1024 * 1024);
}
function validateRenameDocuments(documents) {
  if (documents.length > 128) throw new Error("Rename exceeds the server's 128-document limit.");
  let bytes = 0;
  const seen = new Set();
  for (const document of documents) {
    const uri = document.uri.toString(), source = document.getText();
    bytes += Buffer.byteLength(source, "utf8");
    if (seen.has(uri) || bytes > 32 * 1024 * 1024 || !syncFits(document, source)) throw new Error("Rename inputs exceed synchronization limits.");
    seen.add(uri);
  }
}

// LSP carries versions; native WorkspaceEdit does not. Validate every target
// before converting, without applying edits or opening/writing any file.
function validateRename(result, newName, snapshots, epochs) {
  if (!Array.isArray(result.documentChanges) || result.changes || result.documentChanges.length > 128) throw new Error("Invalid versioned rename result.");
  const open = vscode.workspace.textDocuments.filter((document) => isBhasa(document) && !document.isClosed);
  if (open.length !== snapshots.size || open.some((document) => {
    const uri = document.uri.toString();
    const snapshot = snapshots.get(uri);
    return !snapshot || document !== snapshot.document || document.version !== snapshot.version || (epochs.get(document) || 0) !== snapshot.epoch;
  })) throw new Error("Open document graphs changed during rename.");
  const seen = new Set();
  for (const change of result.documentChanges) {
    const uri = change?.textDocument?.uri;
    const snapshot = snapshots.get(uri);
    const current = vscode.workspace.textDocuments.find((document) => document.uri.toString() === uri);
    if (!snapshot || seen.has(uri) || current !== snapshot.document || current.isClosed ||
        !Number.isSafeInteger(change.textDocument.version) || change.textDocument.version !== snapshot.version ||
        current.version !== snapshot.version || (epochs.get(current) || 0) !== snapshot.epoch ||
        !Array.isArray(change.edits) || !change.edits.length || change.kind) throw new Error("Rename targets changed or are not open.");
    seen.add(uri);
    const source = current.getText();
    const lines = renameLines(source);
    const spans = change.edits.map((edit) => {
      if (edit.newText !== newName) throw new Error("Invalid rename replacement.");
      const start = checkedOffset(source, lines, edit.range?.start);
      const end = checkedOffset(source, lines, edit.range?.end);
      if (start >= end || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(source.slice(start, end))) throw new Error("Rename must replace a complete identifier.");
      if (/[A-Za-z0-9_]/.test(source[start - 1] || "") || /[A-Za-z0-9_]/.test(source[end] || "")) throw new Error("Partial identifier rename.");
      return { start, end };
    }).sort((left, right) => left.start - right.start);
    if (spans.some((span, index) => index && span.start < spans[index - 1].end)) throw new Error("Overlapping rename edits.");
  }
}
function renameLines(source) {
  const starts = [0], ends = [];
  for (let index = 0; index < source.length; index++) {
    const code = source.charCodeAt(index);
    if (code === 10 || code === 13) {
      ends.push(index);
      if (code === 13 && source.charCodeAt(index + 1) === 10) index++;
      starts.push(index + 1);
    }
  }
  ends.push(source.length);
  return { starts, ends };
}
function checkedOffset(source, lines, position) {
  if (!Number.isSafeInteger(position?.line) || position.line < 0 || !Number.isSafeInteger(position?.character) || position.character < 0) throw new Error("Invalid rename position.");
  if (position.line >= lines.starts.length) throw new Error("Rename position exceeds document.");
  const start = lines.starts[position.line];
  if (position.character > lines.ends[position.line] - start) throw new Error("Rename position exceeds line.");
  const at = start + position.character;
  if (at > start && at < lines.ends[position.line] && source.charCodeAt(at - 1) >= 0xd800 && source.charCodeAt(at - 1) <= 0xdbff && source.charCodeAt(at) >= 0xdc00 && source.charCodeAt(at) <= 0xdfff) throw new Error("Rename position splits a surrogate pair.");
  return at;
}

function range(value) {
  for (const position of [value?.start, value?.end]) {
    if (!Number.isInteger(position?.line) || position.line < 0 ||
        !Number.isInteger(position?.character) || position.character < 0) {
      throw new Error("Invalid LSP position.");
    }
  }
  return new vscode.Range(value.start.line, value.start.character, value.end.line, value.end.character);
}

function symbol(value, depth = 0) {
  if (depth > 64) throw new Error("Document symbols are nested too deeply.");
  const kind = Number.isInteger(value.kind) && value.kind >= 1 && value.kind <= 26
    ? value.kind - 1 : vscode.SymbolKind.Function;
  if (value.location) {
    return new vscode.SymbolInformation(value.name, kind, value.containerName || "",
      new vscode.Location(vscode.Uri.parse(value.location.uri), range(value.location.range)));
  }
  const result = new vscode.DocumentSymbol(value.name, value.detail || "", kind, range(value.range), range(value.selectionRange));
  result.children = (value.children || []).map((child) => symbol(child, depth + 1));
  return result;
}

function activate(context) { controller = new LanguageTools(context); }
async function deactivate() {
  if (!controller) return;
  controller.disposed = true;
  await controller.stop();
}

module.exports = { activate, deactivate };
