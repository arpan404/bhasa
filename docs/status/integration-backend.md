# Compiler integration status

Verified on October 11, 2026, ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. The backend development milestone is integrated and verified.
Three internal agents and root completed the [approved contract](next-milestone.md).
The preceding methods report is preserved in [integration-methods.md](integration-methods.md).
No commits, resets, publishing, cache cleaning or dependency downloads were performed.

## Delivered compiler and libraries

- Both frontends support named tagged enums, positional variant constructors
  and exhaustive statement matches. Matching evaluates once, snapshots the
  active value and copies case-local payloads. Cleanup touches only the active
  payload. Recursive layouts, task payloads and incomplete cases fail checks.
  Match bodies participate in transitive worker-to-extern validation.
- The local `std` package contains concrete Option/Result types, UTF-8 Text,
  StringList, StringIntMap and StringMap. Collections preserve insertion order
  with linear lookup and independent owned copies. Text indexes Unicode scalars.
- JSON parsing validates UTF-8, escapes, surrogate pairs and number spelling.
  It retains fractions/exponents as source text and checks signed int64
  conversion. Limits are 1 MiB, 65536 nodes and a depth guard of 64 below root.
  Duplicate object lookup returns the first key. Quoting returns validated Text.
- Bounded HTTP/1.0 and HTTP/1.1 parsing supports one request per connection,
  Content-Length bodies and byte-counted responses. It rejects duplicate
  headers, Transfer-Encoding, malformed host authorities and URI targets.
  Limits are 16 KiB headers, 100 fields and 1 MiB bodies. Header/body reads share
  one request deadline. The example serves health, a persistent counter and JSON
  echo; a client reset or response failure does not stop its accept loop.
- Timed TCP accept/connect/read/write use monotonic absolute deadlines across
  interruptions, setup and readiness races. Timed connect uses numeric IPv4/IPv6.
  Descriptor clones share permanently nonblocking endpoints; legacy calls
  retain synchronous behavior using poll without a deadline.
- LSP outlines include enums/variants, with definitions for checked constructors
  and case patterns using UTF-16 ranges. Formatter and VS Code grammar understand
  the new syntax. Runtime-use markers avoid compiling unused text/net adapters.

Existing `.bh`, exactly lowercase `bhasa.toml`, local packages, records/methods,
structured scalar workers, native I/O/processes, GPU host adapters and C ABI Rust
libraries remain available. Rust retains CLI/package/toolchain/editor services;
the optional Bhasa lexer/parser/checker/emitter has no Rust frontend fallback.
Bhasa-owned allocations use deterministic cleanup and no collector.

## Combined verification

| Check | Evidence |
| --- | --- |
| Complete reference suite | 328 tests passed; `build/verify-backend-reference.log` |
| Complete full hybrid suite | 335 tests passed; `build/verify-backend-hybrid-frontend.log` |
| Formatting and Clippy | Passed, warnings denied in both configurations; `build/verify-backend-fmt.log`, `build/verify-backend-clippy-reference.log`, `build/verify-backend-clippy-hybrid.log` |
| Editor extension | 32 Node tests plus both JavaScript syntax checks passed; `build/verify-backend-editor.log`; GUI host unverified |
| Refreshed release compilers | `target/release/bhasa` and `build/hybrid/frontend/aarch64-apple-darwin/release/bhasa` |
| A → B → C self-rebuild | B/C compiler LLVM and example results match; `build/verify-backend-selfhost.log`, artifacts `build/selfhost/run.jVP6qZgH` |
| Release examples | Both compilers run enum and live HTTP health/counter/JSON echo; `build/verify-backend-release-smoke.log` |
| Unchanged Rust adapter | Two hybrid runs print 42 without Rust recompilation; Cargo 0.06s and 0.05s in release-smoke log |
| Unchanged hybrid build | Native archive reused, no Rust compilation; Cargo 0.19s and 0.57s; `build/verify-backend-unchanged-build.log` |

Both broad suites ran serially after source freeze with two Cargo build jobs and
one test thread. No RUST_MIN_STACK override was used. The final native archive
key is `afcb397123732b9a7557395d4405bf3f26e053b0301940058aa56c74104d7c6f`;
its concatenated source exactly matches the frozen lexer and emitter. Cargo
adapter caches also pass source/transitive-dependency/profile/flag invalidation
and Bhasa-only-edit regressions. Caches were retained.

Generated LLVM enum and library code and native runtimes pass O0/O2
AddressSanitizer/UndefinedBehaviorSanitizer checks with zero live allocations.
UTF-8 fixtures round-trip every Unicode scalar. TCP fixtures check descriptor
cleanup, shared-clone races, backpressure, EINTR and real IPv4/IPv6 loopback.
HTTP fixtures check fragmentation, total request deadlines, client resets,
malformed framing and UTF-8 response byte lengths in both frontends.

Review checked 347 JSON mutations and 688 IPv6 authority cases. Reproduced
findings were repaired: malformed Host/targets, listener termination on client
failure or idle timeout, unlimited setup EINTR retries, and unused runtime
overlinking. Full-suite preflight also exposed an old scanner test missing the
new keyword codes and a parser stack regression. Separating recursive parser
frames preserves default-stack 128 acceptance/129 rejection for if/while/match.
The parser's 41 focused tests and final full suites pass. No scoped findings remain.

## Platform evidence and limits

Metal executed real kernels on this Mac. CUDA and Vulkan mock protocols pass;
hardware execution explicitly skips because drivers/devices are unavailable.
This Clang lacks a wasm32 backend. Wasm execution and Linux runtime execution
remain unverified here; cross-target executables require compatible SDKs,
libraries and linkers. This milestone adds no portable GPU kernel language or
portable Wasm runtime.

Bhasa remains experimental. Generics, floating-point types, closures, a general
lifetime system, async scheduling, TLS, chunking, keepalive and a package registry
remain unsupported. Current borrowing is conservative and is not Rust's full
model. Rust libraries need explicit C ABI adapters and must independently meet
the no-GC requirement. No C-level speed, Go-level compile time, Rust-equivalent
security or production HTTP claim follows from these finite tests.

Fields remain public. Use collection factories and treat JSON tables as
parser-owned; arbitrary edits can invalidate their invariants. Maps use linear
lookup, Unicode indexing counts scalars rather than grapheme clusters, and the
example processes requests sequentially. Compiler-library APIs and embedded LSP
use the caller's stack; arbitrary embedding stack sizes remain unverified.

See [backend libraries](../backend.md), [language](../language.md),
[self-hosting](../self-hosting.md), [networking](../network.md),
[tooling](../tooling.md) and [caching](../caching.md).
