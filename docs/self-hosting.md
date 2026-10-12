# Bhasa compiler logic inside the Rust host

Bhasa has two frontend implementations. The Rust frontend is the default and
bootstrap reference. `bootstrap/compiler/lexer.bh` and `emitter.bh` implement
a Bhasa lexer, typed parser and LLVM emitter. They can run directly inside the
Rust host through a native interface. Rust continues to handle the CLI, local
packages, generic specialization, Cargo adapters, targets, native tools and
editor services.

Self-hosting here means the Bhasa frontend can compile its own source. It does
not mean that every host service has been rewritten in Bhasa or that the Rust
implementation should be deleted.

## Build and select a frontend

From the repository root, with Rust, Cargo, Clang and `ar` installed:

```sh
sh bootstrap/build-hybrid.sh --frontend
```

The script builds the Rust bootstrap, compiles the Bhasa scanner and emitter
into an object, adds the C core runtime, and links that archive into a host with
the `bhasa-frontend` feature. It runs the feature-enabled test suite and prints
the resulting executable and archive paths.

On ARM64 macOS:

```sh
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa check examples/records/points.bh --frontend bhasa
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa run examples/records/points.bh --frontend bhasa
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa run examples/records/methods.bh --frontend bhasa
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa run examples/network/loopback.bh --frontend bhasa
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa run examples/enums/main.bh --frontend bhasa
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa check examples/http/main.bh --frontend bhasa
```

Use the printed path on another host. The script supports x64 and ARM64 macOS
and GNU/Linux. `BHASA_CLANG` selects the installed Clang. Building the host for
a different machine is rejected; the resulting compiler can still emit the
normal Bhasa cross-target objects.

`--frontend rust` remains the default, including in a hybrid build. A plain
Cargo build cannot use `--frontend bhasa`; it reports how to build the required
archive. The older scanner-only hybrid remains available:

```sh
sh bootstrap/build-hybrid.sh
```

That command enables `bhasa-lexer`: the Bhasa scanner supplies tokens while the
Rust parser, checker and emitter process them. It writes its executable under
`build/hybrid/compiler/`, separately from the full frontend host under
`build/hybrid/frontend/`.

Both scripts use persistent Cargo build directories and content-addressed
native archives. Unchanged inputs reuse the archive. The key includes frontend
source, the bootstrap executable, runtime source, preprocessed C headers,
native tools, target and compile options. Use `--build-only` to build without
rerunning the test suite. Unchanged builds reuse the production artifacts.

For manual Cargo builds, `BHASA_LEXER_ARCHIVE` must be an absolute archive path
produced by the script. Its adjacent `.target` stamp must match Cargo's target;
the full frontend also needs the `.frontend` stamp. Missing or mismatched
artifacts fail with build guidance. The ordinary Rust bootstrap needs neither.

## Current frontend coverage

The Bhasa implementation handles scalar functions, forward calls, recursion,
branches, while/range/List loops with break/continue, checked arithmetic,
static strings, owned buffers and integer
vectors. It also implements C ABI extern declarations, structured typed tasks,
GPU host calls, named records with nested fields and methods, tagged enums with
exhaustive matches, UTF-8 scalar operations, owned TCP handles with deadlines,
verified TLS sockets and bounded typed task scheduling.
`tcp_listen_on` selects an explicit numeric IPv4 or IPv6 bind address;
`tcp_listen` retains its IPv4-loopback default. Timed connect accepts numeric
addresses only. The Bhasa `std` package builds on those operations to provide
concrete and generic Result/Option types, text, typed collections, JSON and
bounded HTTP. The optional TLS package adds HTTPS through generic transport
helpers.

Record helpers clone and destroy nested owned values. Parameters borrow during
a call; borrowed returns become owned copies. The frontend checks recursive
schemas, constructor types, field access and overlapping record/field borrows.
Methods infer the enclosing record type for their first `self` parameter. The
receiver is evaluated once and borrowed for the call; temporary receivers remain
alive until owned return values have been acquired. Methods resolve by record
type and name, and participate in worker call-graph checks. Direct method spawn,
static methods and method values are unsupported.
TCP copies duplicate descriptor ownership. GPU support calls the same trusted
native adapters as the Rust frontend; it does not compile Bhasa GPU kernels.

Enums use private tags and owned active payloads. A match snapshots its value
once and creates independent case bindings. The checker rejects incomplete or
duplicate cases, recursive schemas, unguarded payload access and unjoined tasks.
The host ABI remains v2, with separate keyword codes for enum/match/case and the
existing source-file boundary token. The standalone seed version is 0.6.
Both hosted frontend choices accept [generics](generics.md): the Rust host
specializes generic source before invoking either parser/checker. The standalone
Bhasa seed does not yet perform that lowering itself.

The [integration report](status/integration.md) records the verified revision,
release artifacts and platform limits. Focused enum and sanitizer evidence is in
[status/enums-bhasa.md](status/enums-bhasa.md); library use is documented in
[backend libraries](backend.md).

The host loads modules and lowercase `bhasa.toml` packages for either frontend.
For the Bhasa path, it lowers module namespaces/privacy and specializes explicit
generic applications. Private member access additionally uses a shared typed
Rust-host verifier. The standalone seed does not perform module privacy lowering.
`bh_compile` receives the expanded source and the host inserts zero-width
file-boundary tokens at the mapped module boundaries. The frontend
accepts that token only between complete declarations, so an imported file
cannot complete another file's unfinished syntax. The host maps returned
diagnostic offsets back to each original file. The CLI does
not use the Rust checker or emitter as a fallback for native frontend failures.
The explicit shared privacy verifier described above is an additional host check.

Frontend diagnostics are not text-identical. The Bhasa implementation uses
`BH200` through `BH216` for syntax, types, names, unsupported features, limits,
control flow, integer ranges, ignored statuses, tasks, extern conflicts and
record/enum rules. Both paths expose ordinary and JSON CLI diagnostics. Comparison
tests check accepted behavior and rejected programs without treating identical
message wording as a requirement.

The current type system supports binary64 floats but still lacks closures, traits,
general lifetime parameters and a full borrow checker. Completing a self-rebuild
is not evidence for those unimplemented features or for a Bhasa-written generic
specialization pass.

## Native compiler interfaces

The scanner entry point is:

```bhasa
def bh_lex(source: buffer, errors: ints) -> ints
```

It returns token triples `kind, start, end`; error triples use
`code, start, end`. Lexical errors 1 through 6 correspond to Rust `L001` through
`L006`; code 7 reports invalid UTF-8 and code 8 corresponds to `L008` for
malformed float literals. The host normally rejects invalid UTF-8
while reading source. `bh_string_value` decodes a validated string token.

The frontend entry point is:

```bhasa
def bh_compile(source: buffer, tokens: ints, errors: ints, target: buffer) -> buffer
```

It emits textual LLVM for the requested target. Native wrappers validate table
shape and span boundaries and free returned allocations. They call the Bhasa
functions directly, with no subprocess for each scan or compilation. The Rust
reference scanner remains available as `lexer::lex_rust` for differential tests.

## Self-rebuild gate

Build the default compiler, then run:

```sh
cargo build --release
sh bootstrap/compiler/check.sh
```

`BHASA_BOOTSTRAP` can select another stage-0 executable and `BHASA_CLANG` selects
the native backend. The script combines `lexer.bh`, `emitter.bh` and the small
`driver.bh` into one frontend runner. This explicit assembly keeps the rebuild
check independent of the host's module loader.

The gate performs these checks:

1. Stage 0 compiles frontend runner A.
2. A emits LLVM for its own source; Clang and the C runtime build B.
3. B emits LLVM for that source; the same native tools build C.
4. B and C must emit identical compiler LLVM.
5. Both compile and run the eight basic examples, including range exits, against
   committed expected output, plus a fixture covering extern calls, tasks, nested record copies and
   methods, tagged enums, exhaustive matches, TCP error-handle copies, explicit-bind failures and
   GPU-unavailable behavior.

Artifacts remain under a fresh `build/selfhost/run.*` directory so a failure
can be inspected. The runner itself implements `check` and stdout-only
`emit-llvm`. Project discovery, safe file publication, `build`, `run` and native
linking belong to the Rust host.

The enum gate passed with artifacts in `build/selfhost/run.sv3dGKDk`.
The [integration report](status/integration.md) records combined frontend
verification. A passing gate proves the tested
frontend can rebuild itself and preserve those outputs. It does not prove full
frontend equivalence, all target support or GPU hardware execution.

## Focused checks

```sh
cargo test --test selfhost_lexer
cargo test --test selfhost_emitter
cargo test --test selfhost_parity
cargo test --test selfhost_records
cargo test --test selfhost_methods
cargo test --test selfhost_enums
```

Scanner tests compare token kinds, spans, decoded strings and lexical failures.
Frontend tests cover accepted and rejected programs, arithmetic traps, owned
values and native execution. Resource tests use sanitizers and allocation
accounting; networking tests need loopback permissions. Hybrid host tests run
when their corresponding Cargo feature and native archive are available.

The original character-class helper checks remain available with
`sh bootstrap/check.sh` and `sh bootstrap/check.sh --release`. They test the
small routines in `bootstrap/lexer.bh` exhaustively over byte inputs. They are
useful bootstrap regressions, not the current complete frontend.
