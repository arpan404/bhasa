# Compiler integration status

Immutable local bindings are integrated and verified on ARM64 macOS with
Rust/Cargo 1.98.0 and Apple Clang 21.0.0. Prior typed-error evidence is preserved
in [integration-errors.md](integration-errors.md). The user authorized remaining
backlog implementation, dedicated chats, progress commits and pushes.

## Delivered

Both frontends accept `let name[:Type] = expression`, including initializer-only
`try` and inferred task handles. Reassigning a let fails with S036 in Rust and
BH220 in Bhasa. Owned copies, normal scope/loop/return cleanup, field/container
mutation, synchronous borrowed calls and task consumption retain existing rules.
This is binding immutability; read-only borrows, constants and aliases remain
separate work. Ordinary functions, methods, generic parameters and variables
named let remain valid; scanner token codes and runtime ABIs are unchanged.

AST and both local-state implementations carry reassignability separately from
ownership/task metadata. Rust emission rejects reassignment after checked-AST
mutation. Shared module/generic lowering protects contextual let/try markers and
keeps original private and source identities. Imported generic diagnostics retain
exact original CRLF/UTF-8 byte positions.

Editor definitions/references/hover/prepareRename/rename identify the binder and
reads in `let let = 1` and `let let = try read()`. Contextual markers have no
ordinary symbol identity, including when a function is named let. Versioned
rename preserves markers, comments and strings. Lexical grammar adds contextual
let highlighting; no real VS Code GUI claim is made. See [language](../language.md)
and the [example](../../examples/basics/bindings.bh).

Independent review found and root repaired marker hover resolving an ordinary
symbol and native sanitizer acceptance permitting recoverable UBSan reports.
The imported-path fixture now canonicalizes macOS paths. The complete example
suite caught unsupported range spelling in the new example; it now uses the
existing range(1, 4), followed by a complete frozen-source rerun.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 468 passed; build/verify-immutable-reference-final.log |
| Complete hybrid | 483 passed; build/verify-immutable-hybrid-final.log |
| Focused language contract | Six reference groups, two native groups and two imported/generic host groups; tests/immutable_bindings_reference.rs, immutable_bindings_bhasa.rs and immutable_bindings_integration.rs |
| Independent acceptance | 24 original-source cases pass in each frontend; build/verify-immutable-independent-reference.log and -native.log |
| Owned cleanup | O0/O2 ASan/UBSan with nonrecover flags and atomic allocation counters; mutation, copies, try payloads, loop/match exits and task results |
| Editor | 59 LSP and 82 Node tests pass; build/verify-immutable-focused-reference.log and editor-final.log |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-immutable-fmt.log and clippy-reference.log / -hybrid.log |
| Refreshed releases | Both compilers execute thirteen examples and five package tests; build/verify-immutable-release-smoke.log |
| Cache reuse | 32 native hits, four unchanged Rust adapter runs, zero Rust compilations; two unchanged hybrid builds reuse the same archive with zero compilations |
| A → B → C | Complete compiler LLVM and committed example outputs agree at build/selfhost/run.V33q6Djk; build/verify-immutable-selfhost.log |

Complete reference and hybrid suites ran serially with two Cargo build jobs and
one test thread; loopback tests used authorized local networking. Final archive
key is 89752ec3eb1d4387292584272e0b7eae37705041b0aa085299abc15f52a04998.
Its scanner/emitter source exactly matches build/hybrid/run.4w9SLwHa/frontend.bh;
release hashes are recorded in build/verify-immutable-artifact-identity.json.
Both unchanged rebuild logs confirm that same archive and zero Compiling lines.
No scoped immutable-binding criteria remain open. Caches remain intact.

## Active backlog

[todo.md](../../todo.md) and [backlog-lanes.md](backlog-lanes.md) track remaining
work. All eight chats delivered initial isolated drafts and received concrete
second increments. Cursors/status are in [backlog-state.json](backlog-state.json).
Drafts are not completed language features. The next integration is conservative
local pending-result obligations (S037/BH221), with matching isolated native and
independent review evidence plus migrations preserving owned-copy assertions.
Full assigned/passed/aggregate failure safety remains outside that slice.

Alias review routed allocation-bound, root alias main and trailing-comma defects.
Package/FFI review routed test-graph locks, symlink entry identity, graph-wide
preflight and C/C++ reserved-symbol defects. Editor review routed new unsaved
file identity and stale dependency signature replies. Corrected drafts await
integration gates. Streaming files, channels, Wasm guest runtime and portable GPU
work are also isolated until their full frontend/runtime contracts are verified.

Ownership/borrows, richer types/functions, async/concurrency, standard/backend
libraries, packaging, compiler performance and production acceptance remain open.
No Rust-equivalent safety or C/Go performance parity is claimed. Actual prior
Metal/TCP/TLS tests pass; CUDA/Vulkan hardware, compiled guest Wasm execution,
Linux/Windows device execution and real VS Code GUI remain unverified or gated.
