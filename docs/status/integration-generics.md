# Compiler integration status

Verified on October 11, 2026, ARM64 macOS with Rust/Cargo 1.98.0, Apple
Clang 21.0.0 and installed OpenSSL 3.6.4. The approved
[generics, TLS and executor milestone](generics-milestone.md) is integrated and
verified. Three internal agents worked in parallel; root integrated both
frontends, libraries, tooling and final acceptance. The preceding report is
preserved in [integration-backend.md](integration-backend.md).
No installs, dependency downloads, cache cleaning, commits, resets or publishing
were performed. The historical app-chat heartbeat remains paused.

## Delivered behavior

- Explicit generic structs, enums and functions specialize at compile time:
  `Box[buffer]`, `Option[int]`, `identity[int](42)`. Nested applications,
  imported templates and inherited generic record methods work. Concrete
  specializations preserve nominal typing, ownership, aggregate cleanup and
  transitive worker restrictions. Expansion is bounded and diagnostics retain
  original source spans and readable names. LSP outlines include unused
  templates; dependent method navigation returns every concrete target.
- Native verified TLS and the opt-in `tls` package provide a working HTTPS
  health service. Clients require an explicit CA and matching DNS/IP SAN;
  wrong hosts, CN-only identities and untrusted certificates fail. TLS 1.2 or
  newer uses installed OpenSSL 3. Positive total deadlines include lock and
  network progress and are checked after SSL returns. Owned TLS copies share
  a serialized refcounted session, released deterministically without GC.
  Generic HTTP transport helpers work with TCP or TLS. Ordinary `std` programs
  do not link OpenSSL.
- `async_spawn(worker(value))` uses four persistent pthread workers and a FIFO
  queue of 64 waiting jobs. Queue saturation blocks submission. `await(task)`
  consumes the scoped linear task handle and blocks until completion; `join`
  and `await` can consume either task kind. Worker-reachable pooled scheduling,
  waiting and extern calls are rejected through methods, matches, recursion
  and generic specializations. Native guards enforce the same restrictions.

Existing records/methods, tagged enums, exhaustive matching, UTF-8 Text,
collections, JSON, bounded HTTP, file/console I/O, processes, TCP deadlines,
GPU host adapters, local packages and C ABI Rust adapters remain available.
Sources use `.bh` and exactly lowercase `bhasa.toml`. Rust retains CLI,
module loading, generic specialization, toolchain and editor services. The
optional Bhasa frontend performs lexing, parsing, checking and LLVM emission
without a Rust frontend fallback. Bhasa-owned storage has deterministic cleanup.

## Verification

| Check | Evidence |
| --- | --- |
| Complete reference suite | 350 tests passed; `build/verify-next-reference.log` |
| Complete full hybrid suite | 358 tests passed; `build/verify-next-hybrid-frontend.log` |
| Final generic checks | 14 reference / 15 hybrid passed after the final Clippy repair; `build/verify-next-generics-final-reference.log`, `build/verify-next-generics-final-hybrid.log` |
| Formatting and Clippy | Passed, warnings denied in both configurations; `build/verify-next-fmt.log`, `build/verify-next-clippy-reference.log`, `build/verify-next-clippy-hybrid.log` |
| Editor extension | 32 Node tests plus JavaScript syntax and grammar JSON checks passed; `build/verify-next-editor.log`; GUI host unverified |
| Refreshed release compilers | `target/release/bhasa` and `build/hybrid/frontend/aarch64-apple-darwin/release/bhasa`; release build logs use `verify-next-release-*` |
| A → B → C self-rebuild | B/C complete compiler LLVM and example outputs match; `build/verify-next-selfhost.log`, artifacts `build/selfhost/run.NDPkrnQd` |
| Release examples | Both compilers execute generic ownership and pooled workers, plus live HTTPS and certificate/hostname rejection; `build/verify-next-release-smoke.log` |
| Unchanged Rust adapters | Four runs print 42 with zero compilations; Cargo finishes in 0.03s, 0.00s, 0.00s, 0.00s in release smoke log |
| Unchanged hybrid builds | Two builds reuse the native archive with zero compilations; Cargo finishes in 0.02s, 0.01s, 0.00s, 0.00s; `build/verify-next-unchanged-build.log` |

Broad suites ran serially after source freeze, with two Cargo build jobs and one
Rust test thread. The sole subsequent compiler edit changed string slicing to
byte slicing for Clippy; focused generic tests passed in both configurations.
The rebuilt native frontend object is byte-identical to the full-suite object,
and its concatenated source matches the final lexer/emitter. The final archive
key is `7e4882e9e616cb37cd4b421955fc2636fd648b5f0a93fbd55a91d8e4ece56b2c`.
The full suite used key `fe9458e20a9be6e2ab547b9cfc6a353cad9ab49e0b3cadc78bbd5d158c23c69f`;
the host executable change correctly invalidated its content-addressed cache.

Generated generic ownership and native TLS/executor fixtures pass O0/O2
AddressSanitizer and UndefinedBehaviorSanitizer checks. TLS checks account for
runtime/OpenSSL allocations and descriptors after cleanup, preserve buffers on
read failure and cover verification, configuration, backpressure, truncation,
SIGPIPE, lock/read/handshake deadlines and delayed SSL/certificate work. Executor
fixtures verify four concurrent workers, actual queue-full backpressure, 6,144
jobs from four producers, mixed join/await, cold-start races and complete cleanup.
Direct executor ThreadSanitizer normal/cold-start runs also passed. Rust adapter
regressions retain real compiler-invocation checks for unchanged source,
Bhasa-only edits and transitive source/profile/flag invalidation.

Independent review findings were repaired: multiline generic parameter
substitution, readable specialization diagnostics, Unicode/newline/NUL scanner
spans and module EOF isolation, template symbols and multiple method definition
targets, TLS static-library link order and positive-deadline completion. Final
acceptance has no outstanding scoped findings.

## Limits and platform evidence

Generics currently use a shared Rust host lowering pass; the standalone Bhasa
compiler seed does not implement that pass. Type arguments are explicit. There
are no inferred arguments, trait bounds, generic externs or method-specific type
parameters. Unused templates receive syntax/reference validation; dependent body
and ownership checks run per concrete instantiation.

The scheduler provides bounded scalar `int -> int` jobs, with no captures,
coroutines, continuation suspension, event-loop async I/O, cancellation or
channels. Submission and awaiting can block; a blocked worker occupies a pool
slot. Retained completed task handles are caller-owned and not bounded by queue
capacity. TLS has no mutual authentication, revocation policy, ALPN/HTTP2,
Windows, bare-metal or Wasm implementation. Filesystem/crypto work is synchronous
and cannot be preempted, although positive deadline expiry is checked afterward.
The HTTPS example is sequential, with one HTTP/1.x request per connection;
chunking, keepalive and pipelining remain unsupported.

Real Metal kernels executed on this Mac. CUDA/Vulkan mock protocols passed;
hardware execution skipped because drivers/devices are unavailable. This Clang
lacks a wasm32 backend. Linux runtime and Wasm execution remain unverified;
cross-target executables need compatible SDKs, libraries and linkers. OpenSSL
libraries must match the target. There is no portable Bhasa GPU kernel language.

The language remains experimental. Floating-point types, closures, a package
registry and general lifetime parameters remain absent. Borrowing is conservative
and does not establish Rust-equivalent safety. Rust libraries require explicit
C ABI adapters and must independently satisfy the no-GC requirement. Maps use
linear lookup; Text indexes Unicode scalars, not grapheme clusters. Public
library fields allow callers to violate collection/JSON invariants. No C-level
speed, Go-level compile time or production HTTP/TLS security claim follows from
these finite tests. Arbitrary embedding stack sizes remain unverified.

See [generics](../generics.md), [TLS](../tls.md),
[concurrency](../concurrency.md), [backend libraries](../backend.md),
[self-hosting](../self-hosting.md), [tooling](../tooling.md) and
[caching](../caching.md).
