# Compiler integration status

Typed file errors and explicit propagation are integrated on ARM64 macOS with
Rust/Cargo 1.98.0 and Apple Clang 21.0.0. Collection evidence is preserved in
[integration-collections.md](integration-collections.md). The user authorized all
remaining backlog work, parallel chats, progress commits and remote pushes.

## Delivered

Both frontends accept `var value = try operation()` and `var value = try result`
as entire variable initializers. Compatible two-variant ok/error enums can have
different success types, identities and declaration orders; error payloads must
have identical concrete nominal types. Operands evaluate once, success payloads
are independent owned values and error returns perform normal owned cleanup,
including hidden loop/match snapshots. Corrupt tags trap before payload access.
Direct join/await consumes its supplying task; any other live task rejects the
possible early return. Worker call graphs and private enum opacity remain checked.

`std/io.bh` exports IoResult[T] and an Error with private storage and public
domain/code/operation/message getters. Read/write wrappers preserve exact native
status codes and existing Result[T].error(Text) and integer runtime ABIs. Read
failure preserves the native destination; write failure can leave partial output.
Messages are independent owned copies. Standard streams/network/process retain
their existing APIs. See [errors](../errors.md), [I/O](../io.md) and the runnable
[example](../../examples/errors/main.bh).

Immediately discarded result-shaped expressions fail compilation. Assigned or
passed failures can still go unhandled; full pending-failure analysis remains
open. Ordinary calls, generic functions, fields and variables named try retain
their meaning. Propagation is not a general expression or thrown exception.
Fatal faults stop without unwinding/normal cleanup. Public panic() and catchable
panic APIs are absent.

Host visitors, generic/module lowering, original-file diagnostics and checked
editor binding identity include propagated operands and success values. Review
repaired multiline generic annotation marker protection and bounded malformed
header discovery. A 4000-header malformed case fell from about 2.7 seconds to
24 ms in a local debug check; this is a regression probe, not a compiler benchmark.
Deterministic binder-count and valid call/range/match regressions accompany it.
Collection and typed-worker documentation drift was corrected too.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 457 passed; build/verify-errors-reference-final.log |
| Complete hybrid | 471 passed; build/verify-errors-hybrid-final.log |
| Focused reference contract | Six groups cover shapes, nominal errors/tags, contextual syntax, pending tasks/worker graphs, corrupted checked AST and O0/O2 owned propagation; tests/error_propagation_reference.rs |
| Native parity | Fresh seed, actual shared generic lowering, 23 rejections, compatibility and self-rebuild; tests/error_propagation_bhasa.rs; build/errors-draft/native/probe.log |
| Independent host integration | Four groups cover imported generics, aliases, private inferred enums, real std I/O, owned loop/match exits, formatter stability and exact original CRLF/UTF-8 diagnostics; tests/errors_integration.rs |
| Typed I/O | Binary/empty files, status/privacy/message isolation, six injected read/close/write/flush combinations, first-error preservation and zero open FILE/owned allocations; tests/typed_io.rs |
| Ownership | O0/O2 ASan/UBSan and atomic allocation counters cover typed results, collections, loops/matches and joined task results |
| Editor | 57 LSP / 81 Node tests; complete suites and build/verify-errors-editor-final.log |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-errors-fmt.log and clippy-reference.log / -hybrid.log |
| Refreshed releases | Both refreshed binaries execute twelve examples and five package tests; build/verify-errors-release-smoke.log |
| Cache reuse | 30 native hits, four unchanged Rust adapter runs and two unchanged hybrid builds with zero compilations; build/verify-errors-release-smoke.log and unchanged-build-1.log / -2.log |
| A → B → C | Complete compiler LLVM and committed example outputs match at build/selfhost/run.jhI8MeqT; build/verify-errors-selfhost.log |

Complete suites ran serially with two Cargo build jobs and one test thread,
using local loopback permission for networking fixtures. After the final bounds
repair, both complete suites were rerun against frozen sources. No scoped
acceptance criteria remain open within the typed-error increment.

Final archive key: `3e95a9e4f6b732970b6bb9bb8d42fbd33723adbc43efb080cf7b482cac1e559f`.
Concatenated scanner/emitter source exactly matches build/hybrid/run.IHQjCDCp.
All caches and the accepted Rust host remain intact.

## Remaining work and active delegation

The complete remaining backlog is [todo.md](../../todo.md). Eight new dedicated
chats own its sections in [backlog-lanes.md](backlog-lanes.md), with IDs/cursors
in [backlog-state.json](backlog-state.json). Their implementations stay in
isolated drafts until reviewed and integrated. Binding-only let is also drafted
under [immutable-bindings-milestone.md](immutable-bindings-milestone.md); it is
not yet applied or accepted. The historical six chats remain completed. The existing heartbeat is repurposed
for the eight new lanes, active every five minutes with quiet unchanged checks.

Ownership transfer/read-only borrows, richer types/functions/generics, channels
and cancellation, true async I/O, broader standard/backend libraries, package
resolution/distribution, compiler migration/performance, portable GPU kernels
and production acceptance remain open. The foreign C ABI trusts live pointers
and callbacks and cannot prove arbitrary adapters safe. No Rust-equivalent safety
proof or C/Go performance parity is claimed. Actual Metal/TCP/TLS pass; CUDA/Vulkan
hardware, runnable Wasm, Linux/Windows device execution and real VS Code GUI
remain unavailable or unverified on this host.
