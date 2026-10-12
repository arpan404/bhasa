# Bhasa

Bhasa is an experimental compiled language with C/Python-inspired syntax and
`.bh` source files. It emits LLVM IR, then uses an installed Clang for machine
code and linking. The Rust host handles projects, native toolchains and editor
services. An optional build runs the lexer, parser, type checker and LLVM emitter
written in Bhasa.

The language has named records and methods, tagged enums and exhaustive matches,
owned buffers and integer vectors, file and console I/O, subprocesses, structured
typed worker threads, a bounded task executor, synchronous TCP/TLS with deadlines,
explicit compile-time generics, binary64 floats, generic lists/maps, and GPU host adapters. A local
standard library provides UTF-8 text, typed collections, generic Option/Result, JSON and bounded HTTP/HTTPS.
Allocations have deterministic cleanup; the Bhasa runtime has no
garbage collector. This is still a developing language. Its ownership rules do
not provide Rust's complete borrowing model, and no C-like speed or Go-like
compile-time claim has been established. [todo.md](todo.md) tracks the full
implementation backlog and verified progress commits.

```bhasa
struct Point {
    x: int
    y: int
    def total(self) -> int { return self.x + self.y }
}

def add(a: Point, b: Point) -> Point {
    return Point(a.x + b.x, a.y + b.y)
}

def main() {
    var result = add(Point(10, 20), Point(11, 22))
    print(result.total())
}
```

Braces delimit blocks. Newlines or semicolons end simple statements. Parameters
require types except a method's inferred `self`; local variables infer theirs.
An omitted return annotation means `void`. The example prints `63`.

## Build and run

Install Rust with Cargo and have `clang` available on `PATH`. The compiler crate
has no external Rust dependencies. It does not download LLVM, SDKs or toolchains.
From the repository root:

```sh
cargo build --release
./target/release/bhasa check examples/records/points.bh
./target/release/bhasa run examples/records/points.bh
./target/release/bhasa build examples/records/points.bh --release -o build/points
./build/points
```

The compiler executable has an `.exe` suffix on Windows. Owned-data, threading
and networking executables currently use the macOS/Linux runtime. Set
`BHASA_CLANG` to an executable path to select Clang. Checking and LLVM emission
do not need Clang.

Cargo's `--release` optimizes the compiler. Bhasa's `--release` optimizes the
compiled program. Signed arithmetic checks remain active in both program modes.
A prebuilt compiler can build ordinary Bhasa applications without Rust or Cargo;
Clang and the target's native tools are still needed.

## Projects and Rust libraries

Once the compiler is on `PATH`:

```sh
bhasa new hello
cd hello
bhasa check
bhasa run
bhasa fmt src/main.bh --check
bhasa inspect --json
```

The manifest is exactly lowercase `bhasa.toml`. Modules use explicit local
imports such as `import "math.bh"`; declared local libraries use
`import "@math"`. Use `import "@math" as math` for a namespace, then call
`math.double(21)`. Declarations are public by default; `private` limits access
to the defining module. Unaliased conflicting exports are errors. See [packages](docs/packages.md).

`[rust-dependencies]` entries build local Cargo `staticlib` adapters offline and
link their C ABI exports. Persistent Cargo target directories reuse unchanged
Rust artifacts, including after Bhasa-only edits. Rust is needed when building
these adapters, but an already linked executable does not need Rust. This is
explicit adapter interoperability, with no arbitrary Rust API imports. See
[Rust caching](docs/caching.md) and the
[package example](examples/rust-package/README.md).

The bundled `std` is an ordinary local Bhasa package. The HTTP example declares
`std = "../../std"` under `[dependencies]` and uses `import "@std"`. From the
repository root, run its loopback server for three requests:

```sh
./target/release/bhasa run examples/http/main.bh -- 8080 3
```

It implements `/health`, `/counter` and JSON `/echo`, with one request per
connection. See [backend libraries](docs/backend.md) for client commands and
the current protocol and data limits.

## Current scope

| Available | Limit |
| --- | --- |
| `int`, `float`, `bool`, `str`, `void` | Signed 64-bit integers with radix literals/bitwise/shifts, binary64 floats, static UTF-8 strings; no narrow integers or f32 |
| `buffer`, `ints`, `List[T]`, `Map[K,V]` | Owned vectors and specialized typed collections; maps use int/str keys |
| Named `struct` records, methods and nested fields | Borrowed `self` receivers and positional constructors; no inheritance or recursive layouts |
| Explicit generic functions, records and enums | Compile-time specialization; no inferred type arguments, traits or method-specific parameters |
| Tagged `enum` values and exhaustive `match` | Named concrete variants and owned payloads; no guards, wildcards or match expressions |
| `Text`, `StringList`, `StringIntMap`, `StringMap` | Local `std` package; concrete types, scalar UTF-8 indexing and linear map lookup |
| JSON and HTTP/1.0/1.1 library helpers | Bounded input; optional TLS package; one request per connection; no chunking or keepalive |
| Typed functions, forward calls and recursion | No overloads, closures or function values |
| `var`, assignment, branches, `while`, `for`, `break`/`continue` | Half-open int ranges with signed steps and owned List/buffer/ints/Map-key snapshots; see [iteration](docs/iteration.md) |
| File/console I/O, arguments and subprocesses | Native macOS/Linux; explicit status values |
| `spawn(worker(value))`, `join(task)` | One typed input, typed result (including void), independently acquired owned data and mandatory lexical joins |
| `async_spawn(worker(value))`, `await(task)` | Four fixed workers, 64 queued jobs and blocking backpressure; no coroutines or I/O reactor |
| Owned TCP listeners and sockets | Synchronous with optional deadlines; numeric-address timed connect |
| Verified TLS client/server sockets | Optional installed OpenSSL 3; explicit trust and identity checks; no mutual TLS or async I/O |
| Metal, CUDA and Vulkan adapters | Trusted backend-specific kernels; no Bhasa GPU kernel syntax |
| Local modules, aliases and `private` access | No registry or package initialization |
| Rust adapters and explicit native link inputs | Trusted C ABI with integer parameters and integer or void results |
| Native boolean tests | Isolated processes, filters, deadlines and JSON; no coverage or parallel runner |
| Native artifact cache | Reuses unchanged complete outputs; frontend work still runs |
| Formatter, JSON diagnostics, `inspect` and LSP | Checked API/dependency inspection, local definitions, references, versioned rename, completion and hover; see [tooling](docs/tooling.md) |
| Native and cross-target LLVM/object output | Linking needs the target's libraries, SDK and linker |

A target listed by `bhasa targets` is a code-generation candidate. Wasm support
is limited to object emission for pure functions and depends on an installed
backend; runnable WASI applications are not implemented. Metal has native-device
verification. CUDA and Vulkan have host-protocol tests; their hardware execution
has not been verified on the development Mac.

The compiler rejects unsafe record/field borrow combinations covered by its
current rules. General lifetime parameters, shared-memory concurrency, async
I/O and a full borrow checker remain future work. Linked native libraries
and foreign GPU kernels remain trusted and must meet the application's no-GC
requirement.

Record methods use `def method(self, ...)` inside a `struct`; the receiver is
borrowed and fields use `self.field`. `tcp_listen(port)` keeps its IPv4-loopback
default. `tcp_listen_on(host, port)` accepts explicit numeric IPv4/IPv6 addresses,
including wildcard addresses, and returns an owned handle whose status must be
checked. See [records](docs/records.md) and [networking](docs/network.md).
The [backend guide](docs/backend.md) covers concrete Result/Option types,
UTF-8 text, collections and HTTP. The [integration report](docs/status/integration.md)
records verification and platform limits. See [numbers](docs/status/numbers.md),
[collections](docs/collections.md), [native testing](docs/testing.md) and
[caching](docs/caching.md) for these foundations.

## Compiler logic written in Bhasa

Build the complete Bhasa frontend inside the Rust host:

```sh
sh bootstrap/build-hybrid.sh --frontend
```

The script prints the executable path and runs its feature-enabled tests. On an
ARM64 Mac, select the frontend with:

```sh
./build/hybrid/frontend/aarch64-apple-darwin/release/bhasa run examples/records/points.bh --frontend bhasa
```

`--frontend rust` remains the default. The Bhasa implementation supports the
current records and methods, tagged enums and matches, owned data, extern calls,
tasks, UTF-8 operations, TCP deadlines and GPU host calls. The host resolves
imports and maps diagnostics back to original files. Its errors
use separate `BH...` codes, so identical diagnostic text across frontends is not
promised. `sh bootstrap/build-hybrid.sh` without the flag builds only the Bhasa
scanner into the Rust frontend. Both forms cache unchanged native archives.

The separate self-rebuild check is `sh bootstrap/compiler/check.sh`. It builds
three frontend generations and compares their emitted LLVM and program output.
See [self-hosting](docs/self-hosting.md) for the scope of that evidence.

- [Language reference](docs/language.md)
- [Records and ownership](docs/records.md)
- [Commands and targets](docs/toolchain.md)
- [Owned data, I/O and processes](docs/runtime.md)
- [Structured concurrency](docs/concurrency.md)
- [TCP networking](docs/network.md)
- [Text, collections, JSON and HTTP](docs/backend.md)
- [GPU support](docs/gpu.md)
- [Editor and agent tooling](docs/tooling.md)
- [Implementation](docs/IMPLEMENTATION.md)
- [Integration status](docs/status/orchestration.md)

See [generics](docs/generics.md), [TLS/HTTPS](docs/tls.md) and
[bounded scheduling](docs/concurrency.md) for examples and current limits.
