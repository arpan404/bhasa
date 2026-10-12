# Iteration editor support

This milestone adds `textDocument/completion` and `textDocument/hover` to the
existing stdio server and local VS Code extension. Source files remain `.bh`;
imports use the saved local `bhasa.toml` graph with unsaved source overlays.

## Contract

Completion lists visible source declarations, aliases, lexical bindings, common
builtins and keywords. Nested bindings shadow outer ones. Function parameters,
match payloads and `for` binders are scoped to their bodies; later locals and
bindings from completed blocks are omitted. Typed receiver completion follows
chained expressions and returns accessible record fields/methods or concrete
List/Map method signatures. Only a bare alias receiver selects module exports.
Foreign private declarations and members, collection implementation fields and
compiler-generated names are excluded.

The response contains at most 128 items in name order. Detail/hover strings and
accepted labels have a 4096-byte bound. A truncated list sets `isIncomplete`.
Completion filters on the prefix before the caret and replaces the whole
identifier, including its suffix. Ranges use UTF-16. Comments and strings do not
produce results. Hover returns plaintext signatures or binding types. Generic
template hovers retain source types instead of picking a concrete specialization.

One query-local recovery removes an unfinished member/name suffix and closes at
most sixteen unmatched delimiters. This recovered copy cannot alter documents,
diagnostics, disk or build inputs. A still-invalid graph falls back to parsed
visible declarations and lexical bindings. Unknown receiver members are omitted.
The server does not implement a full error-tolerant parser, completion resolution,
snippets, rename or a workspace-wide symbol index.

The VS Code client registers native completion and hover providers, converts
LSP kinds and UTF-16 ranges, and discards results for an obsolete document
version. Hover is appended as untrusted text, never executable Markdown.
The grammar adds `for`, `in`, `break` and `continue` with word boundaries.

## Validation

All 38 Node tests passed, including completion/hover conversion, stale-response
rejection, existing transport behavior and loop-keyword regex checks. JavaScript
syntax checking passed. These tests mock VS Code APIs; a real VS Code GUI session
has not been verified.

Focused framed-protocol regressions in `tests/lsp.rs` cover lexical scopes,
shadowing, declaration order, typed collection/member signatures, partial source,
UTF-16 edits, private generic members, unsaved aliases, loop binders, bounded
results, chained alias-name collisions, mid-word edits, generic template hovers
and spaced declarations. All 47 framed LSP tests passed in the final focused
run recorded in `build/verify-iteration-lsp-final.log`. Root also confirmed the
38 Node tests and repository formatting. Combined reference/hybrid verification
is managed by root; no broad-suite outcome is claimed here.

Final combined verification passes all 47 LSP tests in each of the 398-reference
and 408-hybrid suites, plus 38 Node tests and both Clippy configurations.
No VS Code GUI execution claim is made.
