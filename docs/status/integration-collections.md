# Compiler integration status

Collection snapshot iteration is integrated on ARM64 macOS with Rust/Cargo 1.98.0
and Apple Clang 21.0.0. The prior numeric/editor increment is preserved in
[integration-systems.md](integration-systems.md). The user authorized the full
backlog, parallel agents, progress commits and pushes to the configured remote.

## Delivered

Both frontends accept `for value in buffer`, `ints`, List and Map expressions.
Buffers yield int bytes 0 through 255; ints preserves integer order; Map yields
its declared int/str key type in unspecified order. Sources evaluate once and
retain independent deep snapshots, including Map values. Original mutation,
replacement and mutable body bindings cannot alter traversal. All normal loop
exits retain owned cleanup, lexical task consumption and worker graph checks.

The internal Map cursor ABI clones a key only on success and leaves output
untouched on exhaustion. It rejects invalid cursors, null/misaligned storage,
address overflow and aliases with Map or cursor storage before mutation. The
compiler loads only successful output and never increments the runtime-managed
Map cursor. Both frontends use explicitly eight-byte-aligned cursor/key slots.

AST/module/generic/editor visitors include the generalized Iterable source.
Original List/range behavior remains checked. The collection example and guides
explain snapshot costs and unspecified Map ordering. Custom/borrowed iterators
and pair destructuring remain outside this increment.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 441 passed; build/verify-collections-reference-final.log |
| Complete hybrid | 454 passed; build/verify-collections-hybrid.log |
| Focused final host/editor | Two imported/generic/task groups and 56 LSP tests pass, including buffer/ints/Map binding hover and rename; build/verify-collections-focused-final.log |
| Reference contract | Four groups cover checked types, corrupt AST, task scopes, worker graph and O0/O2 sequence/key-set oracles; tests/collection_iteration_reference.rs |
| Native parity | Fresh seed, reversed mutation, key-set/owned payload checks and self-rebuild; tests/collection_iteration_bhasa.rs; build/collection-iteration-native/probe.log |
| Independent integration | Imported generics/method sources, typed nested tasks, owned early exits, privacy and transitive worker rejection; tests/collection_iteration_integration.rs |
| Runtime guards | Strict C11 O0/O2 ASan/UBSan, allocation/clone/drop accounting and 20 isolated alias/cursor/storage failures; tests/collection_iteration_runtime.rs |
| Ownership | O0/O2 sanitizers and atomic allocation counters cover Map values, snapshot mutation/free, nested loop exits and typed task frames |
| Editor | 80 Node tests; build/verify-collections-editor.log |
| Formatting/Clippy | Formatting and both Clippy configurations pass; build/verify-collections-fmt.log, verify-collections-clippy-reference.log / -hybrid.log |
| Refreshed releases | Both refreshed releases execute eleven examples and five package tests; build/verify-collections-release-smoke.log |
| Cache reuse | 28 native hits, four unchanged Rust adapter runs and two unchanged hybrid builds with zero compilations; build/verify-collections-release-smoke.log and unchanged-build-1.log / -2.log |
| A → B → C | Compiler LLVM and committed outputs match at build/selfhost/run.nKvvmUf3; build/verify-collections-selfhost.log |

Complete suites ran serially with two Cargo build jobs and one test thread.
Compiler sources remained frozen. The sandbox prevented initial HTTP startup;
the complete suite with local loopback permission passes. An independent test
program initially used an unsupported worker arity and str comparison; the
fixture was corrected to a single owned worker frame and explicit byte equality.
The worker-restriction probe was strengthened to use a valid worker signature,
and final focused/reference and complete hybrid checks pass.

Fresh archive key: `d38354856710fabdc825c0c98f05bb12dab2a48292baf2e9f8e42c76f0c96892`.
Its source exactly matches the frozen scanner/emitter at build/hybrid/run.qoaL3rhV.
The old heartbeat remains paused. Caches, .bh, exactly lowercase bhasa.toml and
the accepted Rust host architecture remain intact.

## Remaining work

The complete remaining backlog is in [todo.md](../../todo.md). Typed file errors,
initializer-only explicit propagation and immediate result-discard detection are
prepared in [errors-milestone.md](errors-milestone.md); drafts stay outside tracked
sources until this increment is committed. Assigned-failure analysis, borrowing,
channels/cancellation, async I/O, fixed-width types, closures/traits, portable GPU
kernels and the wider platform/library/package/release ecosystem remain open.

Map copies values for key iteration and currently uses int/str keys. The C ABI
trusts live foreign pointers and callbacks; it cannot validate arbitrary dangling
pointers or prove native adapters safe. No Rust-equivalent safety proof or C/Go
performance parity is claimed. Real Metal/TCP/TLS pass; CUDA/Vulkan hardware,
Wasm backend, Linux/Windows device execution and real VS Code GUI remain unavailable
or unverified on this host.
