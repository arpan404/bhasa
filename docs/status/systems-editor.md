# Checked editor refactoring

The editor portion of the systems/tooling increment is implemented and frozen.
Focused verification passes 56 LSP tests and 80 Node tests. The combined
reference suite passes 433 tests. Final combined acceptance now also passes 445 full-hybrid tests, refreshed
release execution, self-build and unchanged caches; see integration.md.

## Implemented contract

`src/lsp.rs` provides checked local definitions, Find References,
`prepareRename` and `rename`. Identity uses the original declaration token and
source URI across module and generic lowering. References include declarations,
local reads and writes, members, import aliases, loop bindings, match bindings
and concrete generic occurrences where checked identity is available. Repeated
specializations share one original reference location. Loop source expressions,
including a range step, are visited before introducing the loop binding.

Analysis covers loaded graphs rooted at open documents. References may point to
disk-loaded files. Rename requires every edited file to be open and returns
integer-versioned `documentChanges`; neither server nor extension applies edits
or writes files. The server applies proposed token edits to sources in memory,
checks affected graphs again and compares declaration bindings before and after.
This rejects collisions and silent capture of both renamed and untouched
references. Comments, strings, import paths and import aliases remain unchanged
unless the alias itself is the selected symbol.

The VS Code extension registers native reference and rename providers. It checks
the complete open Bhasa document inventory before converting a rename response
to `WorkspaceEdit`, including importers receiving no edits. A changed version,
close/reopen, newly opened document, cancellation or server restart discards the
response. Returned ranges must select complete identifier tokens, use valid
UTF-16 positions and contain no overlapping edits or resource operations.

## Bounds and conservative refusals

| Input or result | Limit |
| --- | --- |
| Open documents | 128 |
| Total open UTF-8 source | 32 MiB |
| URI | 16 KiB |
| JSON notification body, including escaping | 4 MiB |
| Replacement identifier | 256 ASCII bytes |
| Analysis and rebuilt source graphs | 64 MiB |
| Indexed reference locations | 65,536 |
| Refactor response | 2 MiB |

The client requires integer document versions and successfully queued full-text
notifications for every rename input. It accounts for encoded JSON size before
sending. Over-limit documents and failed sends block rename without marking
those versions as sent. They can be retried when capacity becomes available.
Notifications have no acknowledgement.

Only fully checked, unambiguous identity authorizes a refactor. Incomplete-source
completion recovery and syntax fallback cannot authorize rename. Builtins,
generated collection members, foreign ABI declarations, `main`, the implicit
`self` receiver and generic type parameters are unsupported targets. Ambiguous
generic receiver tokens, some nested generic type arguments and uninstantiated
generic bodies also cause conservative refusal. Replacement names cannot be
keywords, builtins, contextual reserved names or compiler-reserved identifiers.
Legal existing contextual local names can be renamed to valid replacements.

Unknown importers outside loaded graphs are not searched. Global rename refuses
open documents whose unresolved import graphs prevent proving completeness.
Save changed import paths before refactoring. Unrelated broken standalone
documents need not block an otherwise checked rename. There is no workspace-wide
index, disk rewrite or filesystem transaction in this increment.

## Review repairs and evidence

Independent review produced these repairs and regression tests:

- Compare all open document snapshots, including an importer that adds its first
  reference while a request is running. Detect close/reopen even when an editor
  document object and version are reused.
- Refuse unresolved open importer graphs and compare bindings after rename to
  prevent capture that would still type-check.
- Separate replacement-name rules from existing checked names, permitting a
  task binding named `task`, contextual local names and a declared method named
  `get` to be renamed while retaining the implicit receiver restriction.
- Precompute line offsets per edited document instead of scanning the entire
  source for every edit. Preserve CRLF, standalone CR and surrogate boundaries.
- Bound rebuilding and reference indexing. Validate every open source against
  server admission limits and record synchronization state only after a send
  succeeds.
- Keep close/reopen epochs in a `WeakMap` keyed by document objects, so closed
  document history does not retain URIs. Ignore non-Bhasa closes and closes
  without a server session while preserving same-object reopen detection.

The 56 focused LSP tests cover checked lexical identities, generic deduplication,
nominal declarations, aliases, UTF-16 edits, shadowing, capture, unsupported
targets, unresolved import graphs and existing contextual names. The 80 passing
Node tests cover providers, transport and grammar, including stale secondary
documents, malformed edits, source/URI/JSON limits, opening 129 documents then
closing one and recovering, and failed open/change notification enqueue followed
by retry. Queue saturation and write errors are both exercised. Close-history
tests cover non-Bhasa documents, absent sessions and same-object reopen. These client
tests mock the VS Code API; they do not constitute a manual editor UI test.

`npm run check` and `npm test` pass from `editors/vscode`. The coordinator reports
433 passing combined reference tests. Compiler sources stayed frozen during the
final client guard repairs. Final combined acceptance is recorded in integration.md, including both Clippy
configurations and formatting, releases, A/B/C and unchanged cache reuse.
