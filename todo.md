# Bhasa implementation backlog

Updated October 11, 2026. This is the complete backlog from the requested feature
inventory. A checked box means integrated and verified, not merely assigned or
implemented in one frontend. Work proceeds in dependency order with local commits
after verified increments. Existing caches are preserved.

## Current work

- [x] Generalize structured tasks to typed, independently owned arguments/results in both frontends.
- [x] Verify typed task cleanup, lexical consumption, worker call graphs and unchanged scalar behavior.
- [x] Commit the verified typed-task increment and update this checklist with evidence.
- [x] Add read-only checked API/dependency inspection for agents, with JSON output and original-file locations.
- [ ] Add checked integer bitwise/shifts and hexadecimal/binary literals in both frontends.
- [ ] Add stepped/descending ranges with safe cursor advancement and complete host lowering.
- [ ] Add checked local definitions, references and versioned rename with editor providers.

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

Baseline evidence: `docs/status/integration.md`, 398 reference tests, 408 hybrid
tests and 38 editor tests. This evidence predates new work below.

## Ownership and memory

- [ ] Efficient ownership transfer and owned parameters.
- [ ] Explicit read-only and mutable borrows with alias checking.
- [ ] Borrowed returns, lifetime analysis and safe non-owning views.
- [ ] Recursive owned structures with explicit heap indirection.
- [ ] User-defined deterministic resource cleanup.
- [ ] Safe ownership transfer/sharing between threads.
- [ ] Copy elision and allocation/copy benchmarks.
- [ ] Fallible allocation and bounded-memory/custom allocator profiles.

## Systems types and data

- [ ] Fixed-width signed and unsigned integers and target-sized integers.
- [ ] `f32` and explicit checked numeric conversions.
- [ ] Bitwise operators, shifts and hexadecimal/binary literals.
- [ ] Fixed arrays, slices and SIMD/vector types.
- [ ] Tuples and destructuring.
- [ ] Type aliases, immutable declarations and constants.
- [ ] Compile-time evaluation.

## Functions, generics and control flow

- [ ] Function values, callbacks and closures with checked captures.
- [ ] Interfaces/traits, bounds and deliberate dynamic dispatch.
- [ ] Inferred/default generic arguments and method-specific type parameters.
- [ ] Nested match patterns, guards and match expressions.
- [ ] Custom and borrowed iterator protocol.
- [ ] Map/buffer/ints iteration and stepped/descending ranges.
- [ ] Convenient explicit Result propagation and typed runtime errors.
- [ ] Stronger enforcement of unused failure results.

## Collections and text

- [ ] Dynamic Text keys and custom hashing/equality.
- [ ] Capacity controls, fallible lookup and collection algorithms.
- [ ] Adversarial hash-collision protection for untrusted keys.
- [ ] String formatting and richer text/number conversions.
- [ ] Unicode normalization, case folding and grapheme operations.
- [ ] Protect standard-library storage invariants through private APIs.

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

- [ ] Streaming file handles, seek and fallible file operations.
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
- [ ] No-GC dependency policy, provenance and review metadata.
- [ ] Registry, publish/install commands and packaged releases.
- [ ] Version resolution, Bhasa lockfiles and dependency checksums.
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

- [ ] Rename, references and local-variable navigation.
- [ ] Signature help, code actions and semantic highlighting.
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
- Typed tasks: 411 reference / 422 full-hybrid tests, 38 editor tests, both
  Clippy configurations and formatting, refreshed releases and A/B/C pass.
  Sanitizers and atomic allocation counters cover owned frames and results.
  Release smoke records 22 native cache hits; four unchanged Rust adapter runs
  and two unchanged hybrid builds perform zero compilations. Evidence:
  `docs/status/integration.md` and `docs/status/typed-tasks-milestone.md`.
