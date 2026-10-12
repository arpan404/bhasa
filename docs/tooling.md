# Editor and agent tooling

Build the compiler with `cargo build --release`. Run `target/release/bhasa lsp`
to start its stdio language server. The command takes no extra arguments. It
reads framed JSON-RPC from stdin and writes only protocol messages to stdout.
Errors in the transport go to stderr.

## VS Code

The local extension in [`editors/vscode`](../editors/vscode/README.md) needs no
npm dependencies or compilation. Set `bhasa.serverPath` in VS Code's user
settings to the absolute compiler executable path, then launch from this checkout:

```sh
code --extensionDevelopmentPath="$PWD/editors/vscode" "$PWD"
```

Open a `.bh` file in the development window. The extension starts `bhasa lsp`
and provides compiler diagnostics, Format Document, an outline of functions and
records with nested fields and methods, enums with nested variants, and Go to Definition for checked calls
and field accesses, local definitions, Find References, checked Rename Symbol, scoped completion, and type/signature hover. The extension README covers local
installation, workspace trust, restart, and validation.

For another LSP client, configure the executable and arguments separately:

```json
{
  "command": "/absolute/path/to/bhasa",
  "args": ["lsp"],
  "filetypes": ["bhasa"]
}
```

This object describes the launch parameters; each editor has its own settings
schema. Select full document synchronization and UTF-16 positions. The server
advertises those choices during `initialize`.

## Supported protocol

| Method | Behavior |
| --- | --- |
| `initialize`, `initialized` | Advertise full sync, UTF-16, formatting, symbols, definitions, references, rename, completion, hover, signature help and semantic tokens |
| `textDocument/didOpen` | Keep source and version in memory and publish diagnostics |
| `textDocument/didChange` | Replace source with each newer full-content update and republish diagnostics |
| `textDocument/didSave` | Reload saved module information and republish diagnostics |
| `textDocument/didClose` | Drop the editor buffer and clear diagnostics no longer needed by open roots |
| `textDocument/formatting` | Return a whole-document edit from `format_source`, or no edits if unchanged |
| `textDocument/documentSymbol` | Return functions, records with fields/methods, enums with variants, and validated type aliases in source order |
| `textDocument/definition` | Resolve checked local bindings, declarations, global/method calls, record/enum constructors, case variants, and field accesses |
| `textDocument/references` | Return checked declaration/use locations across loaded graphs rooted at open documents; honor `includeDeclaration` |
| `textDocument/prepareRename` | Return the complete identifier range and placeholder for a supported checked target |
| `textDocument/rename` | Return versioned `documentChanges` after in-memory checking and binding comparison; every edited file must be open |
| `textDocument/completion` | Return up to 128 sorted visible names or accessible members with UTF-16 replacement edits |
| `textDocument/hover` | Return a plaintext type or declaration signature and the identifier range |
| `textDocument/signatureHelp` | Return checked call signatures and active argument positions |
| `textDocument/semanticTokens/full` | Return checked UTF-16 token sites, including aliases and nominal RHS arguments |
| `shutdown`, `exit` | Respond to shutdown, then exit successfully after the exit notification |

Diagnostics include compiler codes, source `bhasa`, severity, document version
when open, and UTF-16 ranges. Selection ranges cover declaration names; symbol
ranges cover whole declarations. The outline uses LSP Struct, Field, Method,
Function, Enum and EnumMember kinds, with fields/methods and variants inside their enclosing type. Definitions do not link words inside
comments or strings. Locals include parameters, assignment targets, loop bindings and match payloads. Builtins have no
source definition. The current server does not provide workspace symbols. Qualified type annotations
navigate to their checked nominal declarations.

The server processes messages in order. It ignores unsupported notifications,
including cancellation, and returns MethodNotFound for unsupported requests.
It does not advertise incremental edits. Ranged changes, malformed document
notifications, and stale versions leave the current document unchanged.
Formatting failures return RequestFailed without editing the document.

[Type aliases](aliases.md) have their own checked source identity, including
unused declarations. Completion, hover, definition, references, rename and
semantic tokens preserve that identity. Canonical targets retain their nominal
identity, including nested generic arguments on an alias RHS. Qualified aliases
and nominal arguments select only the final identifier; their namespace prefixes
remain separate. `inspect --json` includes declared and canonical target fields.
These identities require a fully checked graph and follow the same privacy,
lock and unsaved-buffer rules as other checked queries.

## References and rename

Find References uses checked declaration identity, including shadowed local
variables, imported declarations and concrete generic specializations. Its scope
is the loaded graphs rooted at open documents. It does not search unopened
projects or unknown importers. References may point to saved files that are not
open. Repeated generic instances produce one location per original token.

Rename Symbol requires every file receiving edits to be open. The server returns
integer document versions and UTF-16 ranges without applying edits or writing
files. It checks the proposed sources in memory and compares reference bindings
before and after, rejecting collisions and silent name capture even when the
renamed program would type-check. Comments, strings and import paths remain
unchanged. Renaming an imported declaration preserves its import alias; a checked
alias can be renamed separately.

The editor validates every open Bhasa document, including importers receiving no
edits, before converting the response to native edits. It drops responses if
any document changes, closes, reopens, or is added during the request, or after
cancellation or server restart. Files outside the returned edits
are not modified.

Before requesting rename, the editor checks that every open Bhasa document fits
the server's synchronization limits: 128 documents, 32 MiB of UTF-8 source in
total, 16 KiB per URI, and 4 MiB per JSON notification including escaping.
Every document needs an integer version and a successfully queued open or change
notification. Over-limit documents and failed sends block rename; they are
retried after capacity becomes available. Notifications have no acknowledgement.

Refactors require completely checked affected graphs. Incomplete-source
completion recovery never authorizes rename. Builtins, generated collection
members, foreign ABI names, `main`, the `self` receiver, generic type parameters,
and ambiguous generic receiver members are unsupported. If any matching token
has no proven checked identity, rename is refused. This includes some nested
generic type arguments and uninstantiated generic bodies. Keyword, builtin,
reserved-prefix and invalid replacement names are rejected. Rename identifiers
are limited to 256 ASCII bytes. Analysis and rebuilt source graphs are bounded
to 64 MiB, with at most 65,536 indexed reference locations and a 2 MiB result.
Oversized operations fail instead of returning partial edits. Global renames
also refuse open documents with unresolved import graphs because their callers
cannot be proven complete. Save changed import paths before refactoring.

## Imports and unsaved source

For a saved file, the server calls the existing project loader to resolve local
`.bh` imports and dependencies in `bhasa.toml`. It replaces the loaded text with
open editor buffers and recalculates source offsets. Module/generic lowering retains file EOF boundaries, so an incomplete file
cannot consume syntax from the next module. Namespace-qualified definitions
retain module identity when different modules export the same spelling. Errors in an imported module use that module's URI and local range.
Closing an imported buffer returns analysis to its saved contents. No language
server operation writes source files or invokes a build tool.

The import graph comes from disk. Save new files and changed import paths to update it. Alias-only edits can use
the resolved saved targets immediately. While an open buffer's import paths
differ from disk, the server checks
its syntax and publishes warning `LSP001` explaining that module checking needs
a save. It does not report unknown imported functions from an incomplete graph.
The same fallback applies when stale disk syntax prevents the loader from
traversing a graph that an editor buffer has repaired. A standalone buffer
without imports can be fully checked before saving, including an `untitled:`
document. Saving imported files outside the editor requires another edit or
save notification to refresh diagnostics; there is no filesystem watcher.

Only the graph rooted at an open source file is available. A module does not
implicitly load the package's entry point or sibling files. When an open
importer already checks an open module, the server avoids also checking that
module as an independent program, which could report false missing globals.
Go to Definition uses the requesting document's graph. Method targets come from
the checker's qualified callee names, including receiver types after module
merging. The server distinguishes same-named methods on different records and
global functions. Chained fields, method-returned receivers, temporary record
receivers, and `self` calls use the checked AST. Field accesses use their checked
receiver type; constructor calls point to the record declaration.
Enum constructors and case patterns point to the corresponding variant declaration.

The graph must pass type checking for method calls, constructor calls, and field
access navigation. If a receiver, argument list, or another part of the graph
has errors, these requests return no location. The server does not guess a
method from its spelling. Parsed declaration names can still link to themselves,
and global-function fallback remains available in incomplete identity module
graphs. Alias/private graphs restrict that fallback to the same file.
Syntax errors can prevent symbol extraction until the document parses again.

## Completion and hover

Completion includes declarations visible from the current module, import aliases,
parameters and preceding local bindings. Inner blocks shadow outer bindings;
locals from other functions, later declarations and completed blocks are excluded.
`for` binders and match payloads belong to their bodies. Hover shows their checked
types when the graph checks, with annotation and literal-type fallback otherwise.
Common builtins and language keywords are also suggested. Generic template
parameters retain their source type names, such as `T`; a local whose template
type cannot be inferred gets a neutral binding hover. The server does not choose
a concrete specialization arbitrarily for a template receiver.

After `alias.`, completion exposes that module's public exports. After a checked
receiver, it exposes fields and methods for that receiver type. Chained calls and
fields retain checked types. `List[T]` and `Map[K,V]` methods show concrete argument
and return types. Private foreign declarations and members are excluded, and
compiler-generated names are never returned. Results use source-level signatures;
there is no completion item resolution or snippet expansion.

One query-local recovery can erase an unfinished name or member suffix and append
at most sixteen missing closing delimiters. This helps with `values.ge` and
unfinished function bodies. It never changes the open document, diagnostics, or
compilation input. When checking still fails, completion falls back to parsed visible
declarations and lexical bindings; unknown receiver members are omitted. Strings
and comments produce no completion or hover. This is bounded recovery, not a
fully error-tolerant parser.

The server returns at most 128 completion items in name order and marks a truncated
list `isIncomplete`. Each detail or hover text is limited to 4096 UTF-8 bytes.
Filtering uses the prefix before the requested UTF-16 position; text edits replace
the complete identifier so completion in the middle of a word does not duplicate its suffix.
Hover uses plaintext, and the VS Code client escapes it as untrusted text. The
client discards completion and hover responses if the document version changes
while the request runs. Imported completion follows the saved graph and unsaved
buffer rules above; there is no workspace-wide symbol index.

## Protocol limits and failures

Headers use CRLF and `Content-Length` counts UTF-8 bytes, following the
[LSP 3.17 base protocol](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#baseProtocol).
Messages have a 4 MiB body limit and an 8 KiB total header limit. The JSON parser
limits nesting to 64 levels, validates surrogate escapes, rejects duplicate
object keys, and preserves request IDs. The server keeps at most 128 open
documents totaling 32 MiB, with URIs up to 16 KiB. Module loading retains the
compiler's 64 MiB source limit. Oversized document notifications leave the existing state unchanged.

Malformed JSON receives ParseError, and the next framed message can still be
processed. A response that would exceed the message limit returns RequestFailed
instead, so later requests can continue. Diagnostic reports fit the same byte
limit and include warning `LSP002` when additional diagnostics must be omitted.
Broken framing, invalid lengths, unsupported content encoding, or truncated messages close the connection with status 1 because the next message
boundary is no longer reliable. Exiting without the shutdown/exit sequence,
including EOF, also returns status 1. After shutdown, further requests receive
InvalidRequest until the exit notification arrives.

## Agent workflows

Use the same source checks and formatter as the editor:

```sh
bhasa check src/main.bh --json
bhasa fmt src/main.bh --check
bhasa fmt src/main.bh
```

`check --json` reports machine-readable compiler diagnostics. The CLI diagnostic
line/column fields are one-based character positions; their byte spans refer
to the original source. LSP positions are zero-based UTF-16 code units. Tools
must not interchange these coordinate systems.

`bhasa inspect [FILE] [--json]` returns a deterministic JSON object containing
the resolved module/import graph, package and Rust-adapter declarations, checked
function signatures, records/fields/methods, enums/payloads, privacy flags and
original file locations. Without FILE it uses the nearest lowercase manifest.
The Rust host checks types without Clang, Cargo, execution or file writes. JSON
is always the output format; `--json` is accepted for consistency with other
agent commands. `schema_version` is 1, `ok` distinguishes success, and failures
carry original-file diagnostics. Exit codes are 0 for success, 1 for source/load
errors and 2 for usage/output-limit errors. Output is capped at 16 MiB.

Inspection includes concrete generic instantiations. Unused generic templates
are omitted, as the `declaration_scope` field explains. It includes private
declarations in the user's loaded graph and labels them; this is an inspection
tool, not a public API export or a privacy bypass for compilation.

## Verification

```sh
cargo test --lib lsp
cargo test --lib json
cargo test --test lsp
cd editors/vscode
npm run check
npm test
```

Rust tests exercise real framed messages through the public server loop and
CLI. The extension tests use Node's built-in runner with mocked VS Code APIs.
A live client/server smoke check covers initialization, Unicode content,
diagnostics, formatting, symbols, definitions, and shutdown. A real VS Code GUI
session has not been verified.

Focused method-tooling checks use the shared checkout and bounded Cargo jobs:

```sh
CARGO_BUILD_JOBS=2 cargo test --test lsp method_ -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test lsp record_symbols_preserve_field_hierarchy_source_order_and_utf16_ranges -- --exact --test-threads=1
```

These cases cover method/field hierarchy, same-name resolution, invalid calls,
UTF-16 ranges, imported record type IDs, unsaved method renames, source URI
preservation, and module EOF diagnostics. They assert that editor requests never
change the saved files.

Generic template symbols retain their original names, including unused and
imported declarations. Checked generic calls navigate to the template, and a
dependent method call with several concrete receiver targets returns all matching
definitions rather than selecting one specialization. Formatting supports explicit
`Type[Argument]` syntax.

Iteration and completion/hover checks are tracked in
[the tooling iteration report](status/tooling-iteration.md).
