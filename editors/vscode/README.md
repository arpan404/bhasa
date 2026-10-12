# Bhasa for VS Code

This local extension recognizes `.bh` files and starts `bhasa lsp` over stdio.
It contains plain JavaScript and has no npm dependencies or build step.

## Launch from this checkout

Build the compiler from the repository root:

```sh
cargo build --release
```

In VS Code's user settings, set `bhasa.serverPath` to the absolute path of
`target/release/bhasa`, or `target/release/bhasa.exe` on Windows. The default
`bhasa` uses the extension host's `PATH`. Specify the executable itself without
quotes or arguments. Paths containing spaces work.

Start a development window from the repository root:

```sh
code --extensionDevelopmentPath="$PWD/editors/vscode" "$PWD"
```

Open any `.bh` file. Trust the workspace to enable the server. No npm install
or extension packaging tool is needed for this development launch. The compiler
must be built before opening a Bhasa document.

To keep the extension installed locally, copy `editors/vscode` into VS Code's
local extensions directory as `bhasa-local.bhasa-0.1.0`, then reload VS Code.
On macOS and Linux that directory is `~/.vscode/extensions`; on Windows it is
`%USERPROFILE%\.vscode\extensions`. The development launch above is preferable
while changing extension code. This repository does not publish a Marketplace
extension or include a VSIX.

## Editor support

- Syntax colors, bracket matching, automatic closing pairs, indentation, and
  `#` line comment toggling. Both `#` and `//` comments receive syntax colors.
- Diagnostics from the compiler as you type, including unsaved edits.
- Format Document through the compiler's formatter.
- Document symbols in the outline and Go to Symbol in Editor.
- Go to Definition for checked functions, methods, fields, types and lexical locals.
- Find References across loaded document graphs.
- Rename Symbol with checked, versioned edits when every affected file is open.
- Scoped completion, including module aliases and typed record/collection members.
- Plaintext type and signature hover.

The client also accepts nested record, field, method, enum and variant symbols
and their definition locations from the server. Those features appear only when the
installed compiler supplies them. Client tests cover their conversion, but do
not establish that method analysis is available in a given compiler build.
The grammar recognizes `def total(self)` inside a struct and `point.total()`
calls, colors the `self` receiver, and recognizes `enum`, `match`, `case`,
`for`, `in`, `break` and `continue`.

Set `"[bhasa]": { "editor.defaultFormatter": "bhasa-local.bhasa" }` in settings
if VS Code asks you to choose a formatter. Normal VS Code format-on-save settings
also work. Formatting follows Bhasa's canonical style; tab width preferences do
not change that style.

The extension sends full document text. It passes UTF-16 positions between
VS Code and the server, filters diagnostics for stale document versions, and
discards formatting, completion and hover results if the text changes while the request is running.
Rename checks the entire open Bhasa document inventory, including unchanged importers, secondary versions and close/reopen events. Imported module diagnostics, definitions and references follow the server's supported
scope. Save files after changing import declarations so the server can refresh
the import graph. The extension sends save notifications after synchronizing
the current text. See the [tooling guide](../../docs/tooling.md) for the saved
import graph caveat. Package manifests are named `bhasa.toml`.

Use **Bhasa: Restart language server** after rebuilding the compiler or recovering
from a server error. Changing `bhasa.serverPath` restarts the server. The Bhasa
Output channel contains compiler stderr and request errors. Normal shutdown
sends `shutdown`, then `exit`; a stalled process is terminated after a short
timeout. Broken or oversized protocol messages close the connection.

## Limits

There is no workspace symbol search, semantic highlighting, debugger,
or build/run command in this extension. Completion is bounded to 128 items and
uses the current module graph. Unknown receivers can produce no suggestions. Syntax colors
are a TextMate grammar, so they do not prove code is valid. The server determines
which diagnostics and definitions are available. Unsaved untitled Bhasa documents
can be checked, but imports need a file location.

The extension uses one compiler process per VS Code window. It supports local
file and untitled documents in a trusted workspace. It does not run in the
browser or virtual workspaces. With Remote Development, the extension and
compiler run on the remote host; configure the executable there. Incoming and
outgoing LSP message bodies have a 4 MiB limit, matching the server's request
limit. JSON framing fields and escaping count toward that limit. Most requests
time out after 10 seconds and initialization after 30.

Rename also requires all open Bhasa documents to fit within 128 documents and
32 MiB of UTF-8 source, with URIs no larger than 16 KiB. It checks the encoded
notification sizes and declines if any current version could not be queued for
synchronization. Over-limit documents remain unsent and can be synchronized
after other documents close. Notifications have no acknowledgement.

Rename requires a fully checked graph and refuses unsupported or ambiguous
symbols. It preserves comments, strings, import paths and unrelated bindings.
Open all files receiving edits first. References may include saved files outside
the open editors, but only within graphs rooted at open documents. The server
does not crawl every project for unknown importers. Builtins, foreign ABI names,
`main`, `self`, generated members and unproven generic references cannot be
renamed. See the [tooling guide](../../docs/tooling.md#references-and-rename) for
these limits.

The client implements the fixed capabilities above. It does not implement
general LSP dynamic registration or arbitrary server-to-client requests.

## Validation

Run from `editors/vscode` using Node.js 18 or newer:

```sh
npm run check
npm test
```

Tests use Node's built-in test runner and streams. They exercise framing,
Unicode byte lengths, cancellation, malformed server messages, document sync and saves,
diagnostic version filtering, native provider conversion, stale formatting/completion/hover,
references, rename conversion and stale secondary-document rejection, workspace trust, restart, and graceful shutdown. The VS Code API is mocked in
the provider tests. These tests do not replace trying the extension in a real
VS Code Extension Development Host.

Focused client checks for record and method results:

```sh
node --test --test-name-pattern='hierarchical record|UTF-16 method' tests/extension.test.js
node --test tests/grammar.test.js
```

The grammar checks exercise its regular expressions with Node, not a TextMate
tokenizer. No dependency is installed for these checks.

The implementation follows the [VS Code extension API](https://code.visualstudio.com/api/references/vscode-api)
and the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/).
