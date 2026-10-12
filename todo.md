# Bhasa implementation backlog

Updated October 12, 2026. This is the complete backlog from the requested feature
inventory. A checked box means integrated and verified, not merely assigned or
implemented in one frontend. Work proceeds in dependency order with local commits
after verified increments. Existing caches are preserved.

## Current work

- [x] Generalize structured tasks to typed, independently owned arguments/results in both frontends.
- [x] Verify typed task cleanup, lexical consumption, worker call graphs and unchanged scalar behavior.
- [x] Commit the verified typed-task increment and update this checklist with evidence.
- [x] Add read-only checked API/dependency inspection for agents, with JSON output and original-file locations.
- [x] Add checked integer bitwise/shifts and hexadecimal/binary literals in both frontends.
- [x] Add stepped/descending ranges with safe cursor advancement and complete host lowering.
- [x] Add checked local definitions, references and versioned rename with editor providers.
- [x] Snapshot iteration for buffer/ints/Map keys, including cleanup and worker safety.
- [x] Typed file errors, explicit initializer propagation and immediate Result-discard rejection.
- [x] Binding-only immutable locals, both frontends, editor identity, cleanup and cache/self-rebuild acceptance.
- [x] Local pending-result scope/branch/loop checking, preserving error/task/ownership contracts.
- [x] Offline graph locks, scalar adapter review metadata and protected compiler outputs.
- [x] Checked signature help and semantic tokens with guarded editor snapshots.
- [x] Private Text, collection and JSON storage with copy-returning public APIs.
- [x] Transparent nongeneric type aliases, checked alias editor identity and inspection metadata.
- [x] Owned buffer/ints transfer, typed regular-file streams and atomic TCP shared-owner copies; both frontends and release/cache/self-rebuild gates.
- [ ] Eight dedicated chats implement remaining sections in `docs/status/backlog-lanes.md`; draft readiness does not complete a checkbox.

Three workers may run alongside the coordinator in this session. The coordinator
owns integration, checklist updates and commits. Workers have disjoint ownership;
Cargo operations remain serialized with two build jobs and one test thread.

## Verified baseline

- [x] `.bh` source files and exactly lowercase `bhasa.toml`.
- [x] Rust host and selectable Bhasa lexer/parser/checker/LLVM emitter.
- [x] Deterministic no-GC aggregate copies and cleanup on scope/loop/function exits.
- [x] Records, methods, privacy, enums and exhaustive matching.
- [x] Explicit generics, List/Map, binary64 floats and checked integer arithmetic.
- [x] Range/List loops, break and continue.
- [x] File/console I/O, arguments and synchronous subprocess execution.
- [x] Scalar native tasks and bounded scalar worker pool.
- [x] TCP, deadlines, verified server-authenticated TLS and bounded HTTP/JSON.
- [x] Local Bhasa packages and cached local Rust C-ABI adapters.
- [x] Formatter, JSON diagnostics, native test runner, editor completion/hover/navigation.
- [x] Metal/CUDA/Vulkan host adapters; actual Metal execution.
- [x] Persistent Cargo, native output and hybrid frontend archive caches.
- [x] Reference/hybrid acceptance and Bhasa frontend A/B/C self-rebuild.

Baseline evidence: `docs/status/integration-iteration.md`, 398 reference tests, 408 hybrid
tests and 38 editor tests. This evidence predates new work below.

## Ownership and memory

- [ ] Efficient ownership transfer and owned parameters.
  - [x] Zero-clone buffer/ints transfer and owned parameters in synchronous monomorphic free functions.
  - [ ] Aggregate/resource transfer and broader parameter modes.
- [ ] Explicit read-only and mutable borrows with alias checking.
- [ ] Borrowed returns, lifetime analysis and safe non-owning views.
- [ ] Recursive owned structures with explicit heap indirection.
- [ ] User-defined deterministic resource cleanup.
- [ ] Safe ownership transfer/sharing between threads.
  - [x] Atomic TCP shared-descriptor copies with distinct live owners and final-close checks.
- [ ] Copy elision and allocation/copy benchmarks.
  - [x] Exact O0/O2 allocation/copy/drop counters for the bounded owned buffer/ints slice.
  - [ ] Representative allocation/runtime benchmarks and broader elision.
- [ ] Fallible allocation and bounded-memory/custom allocator profiles.

## Systems types and data

- [ ] Fixed-width signed and unsigned integers and target-sized integers.
- [ ] `f32` and explicit checked numeric conversions.
- [x] Bitwise operators, shifts and hexadecimal/binary literals.
- [ ] Fixed arrays, slices and SIMD/vector types.
- [ ] Tuples and destructuring.
- [ ] Type aliases, immutable declarations and constants.
  - [x] Binding-only immutable local `let` declarations.
  - [x] Transparent nongeneric type aliases, including checked editor/API identity.
  - [ ] Generic alias declarations and compile-time constants.
- [ ] Compile-time evaluation.

## Functions, generics and control flow

- [ ] Function values, callbacks and closures with checked captures.
- [ ] Interfaces/traits, bounds and deliberate dynamic dispatch.
- [ ] Inferred/default generic arguments and method-specific type parameters.
- [ ] Nested match patterns, guards and match expressions.
- [ ] Custom and borrowed iterator protocol.
- [x] Stepped/descending ranges with overflow-ending cursors.
- [x] Map/buffer/ints snapshot iteration.
- [ ] Convenient explicit Result propagation and typed runtime errors.
  - [x] Entire-initializer `try` with typed error compatibility, ownership cleanup and task guards.
  - [x] Typed whole-file read/write wrappers with private Error metadata and exact native codes.
  - [ ] Broader propagation positions and consistent typed stream/network/process errors.
- [ ] Stronger enforcement of unused failure results.
  - [x] Reject immediately discarded result-shaped expressions.
  - [ ] Track assigned/passed/aggregated failures through control flow and calls.
  - [x] Local bound-result scope/branch/loop obligations, both frontends and emission guards.
  - [ ] Parameters, aggregate contents and interprocedural handling summaries.

## Collections and text

- [ ] Dynamic Text keys and custom hashing/equality.
- [ ] Capacity controls, fallible lookup and collection algorithms.
- [ ] Adversarial hash-collision protection for untrusted keys.
- [ ] String formatting and richer text/number conversions.
- [ ] Unicode normalization, case folding and grapheme operations.
- [x] Protect standard-library storage invariants through private APIs.

## Concurrency and async

- [x] Typed tasks with independently owned inputs/results.
- [ ] Typed channels, bounded backpressure, close and select.
- [ ] Cancellation, deadlines, task groups and supervision.
- [ ] Checked shared-state locks, atomics and scoped concurrency.
- [ ] Configurable executor capacity and scheduling measurements.
- [ ] Async functions and suspension/resumption lowering.
- [ ] I/O reactor, timers and nonblocking TCP/TLS/file/process operations.
- [ ] Cancellation-safe resource cleanup and shutdown.

## OS and standard library

- [x] Streaming regular-file handles, seek and typed fallible operations; POSIX64 runtime, verified on ARM64 macOS.
- [ ] Paths, directories, metadata and filesystem watching.
- [ ] Environment access and signal handling.
- [ ] Subprocess capture, environment overrides and async process handles.
- [ ] Math, randomness, calendar/timezone, regex and compression libraries.
- [ ] Structured logging, configuration, metrics and tracing.
- [ ] Automatic record serialization and broader typed JSON APIs.

## Backend services and networking

- [ ] HTTP client, keepalive, chunked/streaming bodies and pipelining policy.
- [ ] URL/query decoding, routing and middleware.
- [ ] WebSockets, HTTP/2 and HTTP/3.
- [ ] Concurrent serving, graceful shutdown and connection pooling.
- [ ] Database adapters and maintained backend ecosystem libraries.
- [ ] UDP, Unix-domain sockets and cancellable DNS/timed hostname resolution.
- [ ] Mutual TLS, ALPN, certificate reload and revocation policy.

## C/Rust interoperability and packages

- [ ] Ownership-aware foreign strings, buffers, records, resources and callbacks.
- [ ] Generated C/Rust adapters and useful maintained crate integrations.
- [ ] Explicit foreign thread-safety, error and unwind contracts.
  - [x] Scalar C-ABI review sidecars and generated declarations; worker externs remain forbidden.
  - [ ] Ownership-aware enforcement for foreign resources and callbacks.
- [ ] No-GC dependency policy, provenance and review metadata.
  - [x] Source-bound scalar adapter review metadata, reporting no_gc_proven=false.
  - [ ] Complete transitive dependency review and enforcement.
- [ ] Registry, publish/install commands and packaged releases.
- [ ] Version resolution, Bhasa lockfiles and dependency checksums.
  - [x] Explicit local graph bhasa.lock create/update/check, source/manifest checksums and strict read-only verification.
  - [ ] Version resolution, workspace/multi-entry locks and registry dependencies.
- [ ] Workspaces, feature selection and API documentation generation.
- [ ] Stable library/export ABI and reusable library artifacts.

Ordinary Python/TypeScript runtime libraries remain excluded by the user's strict
no-GC dependency rule. Arbitrary Rust native ABI imports are not promised; rich
compatibility requires adapters with explicit contracts.

## Compiler performance and self-hosting

- [ ] Per-module incremental parsing/checking/emission.
- [ ] Cached generic specialization and measured parallel compiler work.
- [ ] Representative compilation/runtime/memory benchmarks and optimization.
- [ ] Hermetic reproducible builds and cache correctness across supported hosts.
- [ ] Bhasa module/generic lowering and privacy analysis.
- [ ] Further Bhasa CLI/build/package/editor services as the migration proceeds.

The Rust host remains accepted architecture. Completing the migration does not
require deleting useful Rust integration before replacements are verified.

## Platforms, WebAssembly and embedded

- [ ] Distributable compiler/backend/linker and target SDK management.
- [ ] Complete Windows owned-data/thread/network/TLS runtime.
- [ ] Verified Linux/Windows target execution and cross-platform CI.
- [ ] Runnable WASI, browser host bindings, imports/exports and Wasm memory/I/O.
- [ ] Bare-metal targets and a freestanding runtime.
- [ ] Startup/linker support, interrupts and safe volatile/MMIO APIs.
- [ ] Allocator selection and allocation-free embedded profiles.
- [ ] Mobile target/runtime support and deployment examples.

## GPU

- [ ] Bhasa kernel syntax and portable kernel translation.
- [ ] Typed device buffers and checked GPU memory/concurrency rules.
- [ ] Multiple buffers, tensors and reusable pipelines.
- [ ] Async dispatch, transfers, synchronization and resource ownership.
- [ ] WebGPU/browser integration and rendering libraries.
- [ ] Actual CUDA and Vulkan hardware execution.
- [ ] GPU diagnostics, debugging and performance measurements.

## Editor, agent and testing tools

- [x] Checked rename, references and local-variable navigation in loaded graphs rooted at open documents.
- [ ] Signature help, code actions and semantic highlighting.
  - [x] Checked signature help and semantic tokens in both compiler configurations and the client adapter.
  - [ ] Safe code actions and verified application in the editor.
- [ ] Workspace indexing, filesystem watching and broader incomplete-source recovery.
- [ ] Project-wide formatting and API docs.
- [ ] Structured project/type/API queries and dependency graphs for agents.
  - [x] `bhasa inspect` provides checked concrete APIs, privacy flags, module/import edges and package/adapter metadata without build tools.
  - [ ] Include unused generic template APIs and richer symbol/type queries.
- [ ] Safe automated refactors and actionable repair suggestions.
- [ ] Measured agent token efficiency and common-mistake prevention.
- [ ] Assertion library, property testing, coverage and parallel/generic tests.
- [ ] Benchmark runner, source debug information, debugger and profiling integration.
- [ ] Real VS Code GUI verification.

## Production acceptance and releases

- [ ] Fuzzing and independent safety/security review of compiler and native boundaries.
- [ ] Adversarial inputs, failure injection and long-running service soak tests.
- [ ] Stable language/ABI specification, versioning and migration policy.
- [ ] Installers, signed release process, tutorials and reference documentation.
- [ ] Publish measured performance; do not claim C/Go parity before evidence.

Unavailable hardware/SDK verification stays unchecked. No installation or
publication is implied by implementation work. Inheritance, exceptions,
overloads and macros remain separate optional design choices.

## Progress commits

The coordinator records completed increments and their verification here.

- `62afd47`: verified compiler baseline, full backlog, and nested build/cache exclusions.
- `b79b914`: checked agent inspection; three CLI tests pass, including original-file errors,
  concrete generic names, deterministic output, privacy and absent native tools.
  Independent review reproduced a public-declaration privacy false positive;
  repaired it and retained a regression. Evidence is in
  `build/verify-typed-tasks-focused-final.log` and `tests/agent_inspect.rs`.
- `cdd7a5b`: typed tasks; 411 reference / 422 full-hybrid tests, 38 editor tests, both
  Clippy configurations and formatting, refreshed releases and A/B/C pass.
  Sanitizers and atomic allocation counters cover owned frames and results.
  Release smoke records 22 native cache hits; four unchanged Rust adapter runs
  and two unchanged hybrid builds perform zero compilations. Evidence:
  `docs/status/integration-typed-tasks.md` and `docs/status/typed-tasks-milestone.md`.

- `eb88560`: systems/refactoring increment; 433 reference / 445 full-hybrid tests,
  56 LSP and 80 editor tests; both Clippy configurations, formatting, refreshed
  releases and A/B/C pass. Sanitizers and allocation counters cover owned loop
  and task cleanup. Release smoke records 26 native hits; four unchanged Rust
  adapter runs and two unchanged hybrid builds perform zero compilations.
  Evidence: `docs/status/integration-systems.md` and `systems-tooling-milestone.md`.
  Git history records this progress commit's identity.

- `c4f8957`: collection increment; 441 reference / 454 full-hybrid tests, 56 LSP
  and 80 editor tests; both Clippy configurations, formatting, refreshed releases
  and A/B/C pass. Sanitizers/allocation counters cover snapshots and owned task
  exits. Release smoke records 28 native hits; four unchanged Rust adapter runs
  and two hybrid builds perform zero compilations. Evidence:
  `docs/status/integration-collections.md` and `collections-iteration-milestone.md`.

- Typed-error increment: 457 reference / 471 full-hybrid tests, 57 LSP and 81 editor
  tests; both Clippy configurations, formatting, refreshed releases and A/B/C pass.
  O0/O2 sanitizers/allocation counters verify error propagation and typed file I/O,
  including injected read/write/close failures. Release smoke records 30 native
  hits; four unchanged Rust adapter runs and two unchanged hybrid builds perform
  zero compilations. Evidence: `docs/status/integration.md` and `errors-milestone.md`.
  Binding-only let and eight new backlog chats remain unintegrated work.

Local pending-result evidence: 477 reference / 502 hybrid tests, 59 LSP / 82 editor,
both Clippy configurations, thirteen release examples, five package tests,
A/B/C at run.EbbPYcIp and unchanged native/Cargo/archive reuse. See
`docs/status/integration-failures.md`. Parameter/aggregate/interprocedural obligations remain open.

Package/editor/private-storage increment: 558 reference / 583 hybrid tests,
77 LSP / 93 client tests, both Clippy configurations, formatting, thirteen release
examples and five package tests per frontend, A/B/C at run.lm8cj8Lt, 32 native
cache hits and four unchanged Rust adapter runs with zero Rust compilations.
Two unchanged hybrid builds reuse archive ccabbe47a921bfdd320a4844960bfd84f5db48f3f5ae7d3b9013fdffdebff3ec
with zero compilations. See `docs/status/integration-tools.md` for scope and limitations.
Owned transfers, streaming files and channels remain unintegrated drafts.

`591d1a7`: transparent alias increment; 597 complete reference / 622 full-hybrid tests,
93 LSP groups per configuration and 93 Node tests. Both Clippy configurations,
formatting, fourteen release examples and five package tests per frontend pass.
Independent corrected identity review passes 126 groups plus expansion allocation
checks. A/B/C passes at run.DqAvmtmF; 34 native hits, four unchanged Rust adapter
runs and two unchanged hybrid builds perform zero compilations. Matching archive
is 366a75f3c915ffe34aa5529155ad732dc7b5e977320d34a7bc527ad931538b69.
See `docs/status/integration.md`. Generic aliases, constants, ownership transfers,
streaming files, channels and broader failure obligations remain open.

`8316a89`: owned/files/TCP increment; 649 complete reference / 681 full-hybrid tests,
98 LSP groups per configuration and 94 Node tests. Independently reviewed
structural Clippy corrections retain semantics and pass 20 reference / 22 hybrid
focused groups. Both Clippy configurations, formatting, refreshed releases with
16 examples/five package tests per frontend, A/B/C at run.lWdKWpz3, 38 unchanged
native hits, four unchanged Rust adapter runs and two unchanged hybrid builds
pass. Final matching archive is
cdd251ae2afa392702ef70570587f3500c3723875c4c6de2f8ed59419f096a8a.
See docs/status/integration.md for scope, exact instrumentation and retained
failures. Borrows, channels, network/process/TLS errors and broader ownership
remain separate work. Git history records this progress commit's identity.
