# Compiler implementation

Bhasa uses a Rust host with selectable language frontends. The default Cargo
build contains the Rust implementation. A hybrid build also links Bhasa-written
compiler components as a native archive. Both emit textual LLVM IR; installed
Clang performs optimization, machine-code generation and linking. Applications
are not translated to Rust.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| `src/driver.rs`, `src/target.rs` | CLI, frontend selection, native tools, runtime linkage, target checks and artifact publication |
| `src/project.rs` | Lowercase `bhasa.toml`, local dependencies, deterministic imports and original-file diagnostic mapping |
| `src/rust_deps.rs` | Offline Cargo adapter builds and persistent per-adapter caches |
| `src/generics.rs` | Explicit compile-time specialization shared by both hosted frontends, with original-source diagnostic mapping |
| `src/lexer.rs`, `src/parser.rs`, `src/sema.rs`, `src/codegen.rs` | Rust reference frontend and LLVM generation |
| `bootstrap/compiler/lexer.bh`, `emitter.bh` | Bhasa lexer, typed parser and direct LLVM emitter |
| `src/bhasa_lexer.rs` | Native Bhasa compiler interface, token/error validation and allocation cleanup |
| `src/format.rs`, `src/lsp.rs`, `src/json.rs` | Formatting, diagnostics and editor protocol |
| `runtime/core.c`, `concurrency.c`, `net.c`, `text.c`, `tls.c` | Owned storage, native I/O, processes, scalar threads and bounded scheduling, TCP/TLS deadlines and strict UTF-8 |
| `std/*.bh`, `packages/tls` | Concrete and generic Result/Option, Text, string collections, JSON and bounded HTTP/HTTPS |
| GPU runtime adapters | Metal, CUDA and Vulkan host integration |

The host owns module loading and explicit generic specialization for both
frontends. It blanks imports, preserves module boundaries and specializes
generic functions, records and enums before ordinary parsing and checking.
The Rust path merges parsed modules; the Bhasa path invokes `bh_compile` on the
expanded source. Diagnostics from either path map back to the original file and
local span. The standalone Bhasa seed does not yet implement generic lowering.

The `bhasa-lexer` feature replaces scanning inside the Rust frontend.
`bhasa-frontend` includes that feature and enables `--frontend bhasa` for checking,
LLVM emission, building and running. Selecting the Bhasa frontend does not
silently fall back to Rust. Rust remains the default selection and the host
implementation. See [self-hosting](self-hosting.md) for build and rebuild checks.

## Language and runtime rules

Records have named fields, methods, nominal types and positional constructors. The
compiler emits allocation, clone and cleanup helpers for nested records and
owned fields. Existing buffers, integer vectors and records copy when bound or
assigned; parameters borrow for a synchronous call. Returning borrowed owned
values produces a copy. Cleanup covers scope exit, early return, reassignment
and temporary expressions.

Methods are declared inside records with an inferred `self` parameter. The
checker resolves each call from its receiver's nominal type and lowers it to an
internal qualified function, keeping it separate from source global and extern
symbols. The receiver is evaluated once before explicit arguments and follows
the existing borrowed-parameter rules. Temporary receivers stay alive through
the call and owned return acquisition. Worker call graphs include resolved
methods, so an indirect extern call cannot bypass the worker restriction.
Static methods, overloads, method values and dynamic dispatch are not supported.

Named enums own one active payload. Exhaustive match snapshots the scrutinee
once and copies payloads into case-local bindings. Only active payloads are
initialized, cloned or freed. Recursive schemas and task payloads are rejected.
The frontends use different private enum layouts, so enum values cannot cross
the scalar C ABI. Match cases participate in worker call-graph validation.

Text and protocol libraries use checked native storage and deterministic copies.
TCP deadline operations keep descriptors nonblocking and wait with monotonic
absolute deadlines. Runtime-use markers link only the required adapters;
unused builtin declarations do not request extra C compilation. The
[backend guide](backend.md) lists library contracts and limits.

TCP copies duplicate descriptor ownership for the same operating-system
endpoint. TLS copies instead retain a refcounted session with serialized I/O;
its last release closes and frees it without a network wait. Neither copy
creates an independent connection. Buffers and aggregate structure still copy
independently; fields containing network handles follow these handle rules.

Task handles are unique and must be consumed with `join` or `await` in their
lexical scope. Both operations block and can consume either task kind.
`spawn` creates a native thread per task; `async_spawn` uses four persistent
workers and a FIFO queue holding 64 waiting jobs, blocking submission when full.
Workers receive and return integers, so aggregate data is not implicitly shared
between threads. Worker call graphs cannot reach `async_spawn`, `await` or
extern functions. This scheduler provides no coroutines or event-loop async I/O.

`tcp_listen` keeps the IPv4-loopback convenience behavior. `tcp_listen_on`
accepts an explicit numeric IPv4/IPv6 address, including wildcards, using
`bhasa_rt_tcp_listen_on_str` or `bhasa_rt_tcp_listen_on` for static strings or
buffers. It returns the same owned listener/error-handle representation as
`tcp_listen`. The runtime rejects name resolution and malformed bind input;
OS errors remain observable through `tcp_listener_status`.

The checker rejects recursive record layouts and record argument combinations
that can invalidate borrowed fields. These are targeted rules for the current
value model, not a complete Rust borrow checker. C ABI adapters, GPU kernel
source and the native runtime are trusted code. There is no garbage collector.
TLS session references are counted for deterministic shared-resource ownership;
buffers and aggregate allocations do not use reference counting.

## Builds and caching

`bhasa.toml` can declare local Bhasa packages and local Rust static-library
adapters. Cargo owns Rust dependency freshness and runs offline against stable
cache directories under the entry project's `build/rust/`. Bhasa-only edits do
not force unchanged Rust crates to compile again. Native adapter target/profile
separation and invalidation are described in [caching](caching.md).

`bootstrap/build-hybrid.sh` also caches native compiler archives. Its cache key
covers the frontend source, bootstrap binary, core runtime, tool binaries,
target and build options. Repeating an unchanged build reuses the archive and
Cargo's host build cache. Changing those inputs creates a different archive.

Cross-target code generation and cross linking remain separate capabilities.
Targets need installed backend support; linked programs also need matching
headers, libraries, SDKs and linkers. The compiler does not download them.

## Loop control and snapshots

The checked AST retains range bounds or a typed List source, plus the inferred
loop binding type. Module/generic source maps visit the source, binder and body.
The worker call graph visits both bounds and List expressions. Checker flow
tracks fallthrough, return and loop exits separately; loops cannot establish a
function return. Task exits check exactly the scopes left by break/continue.

Both LLVM emitters allocate loop cursors in the function entry block. The cursor
is separate from the mutable source binding. A hidden owned scope retains the
List snapshot across continue; each iteration binds an owned element snapshot.
Nearest-loop targets record their retained scopes. Exits clean inner scopes and
branch to the condition, increment or common exit as appropriate. The common
exit drops the List snapshot, and function returns drop it with all locals.

## Verification and remaining work

Tests cover parsing/checking, native execution, malformed inputs, diagnostic
locations, resource cleanup, frontend comparisons, Rust-cache reuse, TCP and
editor protocol behavior. Native ownership tests use address and
undefined-behavior sanitizers and allocation counters. GPU mocks verify host
protocols separately from real hardware runs. Self-rebuild checks compare LLVM
from successive Bhasa frontend generations.

Recorded runs apply to their tested revisions and environments. Component
handoffs and current integration work are in [status/orchestration.md](status/orchestration.md).
The methods and explicit-bind addition follows its
[integration contract](status/next-milestone.md). The current
[integration report](status/integration.md) records the completed combined
checks, refreshed release binaries and self-rebuild. Neither finite tests nor self-rebuilding establish production
readiness, complete safety or performance parity with another language.

Floating-point types, closures, trait bounds, inferred generic arguments, a
package registry, general lifetime parameters, event-loop async I/O, portable
Bhasa GPU kernels and runnable WASI applications remain outside the implemented
scope. TLS currently requires installed OpenSSL 3 and supports server
authentication; mutual TLS and revocation policy are not implemented.
[BOOTSTRAP.md](BOOTSTRAP.md) and [STAGE1.md](STAGE1.md) preserve the earlier
implementation contracts; the public language/runtime guides describe the
current interfaces.
