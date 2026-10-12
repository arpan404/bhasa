# Bhasa toolchain

The Rust host uses either the default Rust frontend or an explicitly selected
Bhasa frontend:

```text
Bhasa sources and local packages
  -> Rust host generic specialization
  -> selected lexer, parser and type checker
  -> textual LLVM IR
  -> installed Clang backend and linker
  -> native executable or target object
```

Clang handles LLVM code generation and native linking. The host does not embed
LLVM development libraries, invoke `rustc` on generated application source or
download a compiler backend. Rust builds the host and any declared Rust native
adapters. The Bhasa frontend emits LLVM directly without creating the Rust AST.

## Requirements and building the compiler

Use a Rust toolchain with Cargo. The crate has no external dependencies. Native compilation also requires an installed Clang and the host's linker and system libraries.

```sh
cargo build --release
./target/release/bhasa --version
./target/release/bhasa --help
cargo test
```

The executable is `target/release/bhasa`, or `target/release/bhasa.exe` on Windows. Examples below assume `bhasa` refers to that executable. You can also run it from the repository with `cargo run -- <command>`. `cargo run --release -- <command>` selects an optimized build of the compiler.

By default, Bhasa finds `clang` on `PATH`. `BHASA_CLANG` selects another installed executable:

```sh
BHASA_CLANG=/absolute/path/to/clang bhasa build hello.bh -o build/hello
```

Set the variable to one executable path, not a shell command with arguments. No backend, SDK or dependency is downloaded automatically.

## Commands

```text
bhasa new NAME
bhasa fmt FILE [--check]
bhasa lsp
bhasa check [FILE] [--json]
bhasa inspect [FILE] [--json]
bhasa emit-llvm FILE [--target TARGET] [-o FILE]
bhasa build [FILE] [-o FILE] [--target TARGET] [--release] [--emit object] [--link FILE ...] [--gpu metal|cuda|vulkan]
bhasa run [FILE] [--release] [--link FILE ...] [--gpu metal|cuda|vulkan] [-- ARGS...]
bhasa targets
bhasa --help
bhasa --version
```

| Command | Result |
| --- | --- |
| `new NAME` | Creates a local project with lowercase `bhasa.toml` and `src/main.bh`; refuses an existing directory |
| `fmt FILE [--check]` | Formats source, or reports whether formatting changes are needed without writing |
| `lsp` | Serves editor JSON-RPC over stdio; stdout is protocol-only |
| `check` | Parses and type-checks source without Clang |
| `check --json` | Writes a JSON array of diagnostics to stdout, or `[]` on success |
| `emit-llvm` | Emits checked LLVM IR without Clang; writes stdout unless `-o` is supplied |
| `build` | Uses Clang to create an executable, or an object with `--emit object` |
| `run` | Builds for the native host and runs the result |
| `targets` | Lists recognized target names and their limits |

`check`, `build` and `run` can omit `FILE`. They find the nearest exactly named
`bhasa.toml` in the current directory or its ancestors and use its package entry.
An explicit source file can also use that package's imports and dependencies.
See [packages](packages.md) for local manifests and libraries.

Explicit-file build output defaults to `build/<source-stem>`. Project commands
default to `<project>/build/<package-name>`. Windows executables use `.exe`;
objects use `.o` or `.obj` according to the target. Use distinct `-o` paths to
keep builds for different targets. Output parent directories are created as
needed. Source modules, package manifests and native link inputs are protected
from output replacement.

`check`, `emit-llvm`, `build` and `run` accept `--frontend rust|bhasa`. Rust is the
default. To enable the Bhasa implementation, build with
`sh bootstrap/build-hybrid.sh --frontend` and use the printed host executable.
A host without that feature rejects `--frontend bhasa` with build guidance.
The Bhasa frontend has separate diagnostic codes and does not fall back to the
Rust parser or checker. Both use the same project loader, explicit generic
specialization pass and native driver. The host maps specialization diagnostics
back to the original source; see [generics](generics.md).

```sh
bhasa check hello.bh
bhasa check hello.bh --json
bhasa emit-llvm hello.bh -o build/hello.ll
bhasa build hello.bh -o build/hello
bhasa run hello.bh --release
```

`check` exits with status 0 on success and 1 on errors. Ordinary diagnostics go to stderr. Each JSON diagnostic includes `schema_version`, `severity`, `code`, `message`, `file`, byte offsets in `span`, and a one-based `line` and `column`. LLVM output on stdout contains only the emitted IR. `run` forwards the program's output and exit status.

Use `--` after the optional source and compiler options to forward arguments
unchanged. For a source filename beginning with a hyphen, use `./-name.bh`.

```sh
bhasa run examples/io/copy.bh -- input.txt output.txt
bhasa run examples/concurrency/file_sizes.bh -- first.txt second.txt
bhasa run -- input.txt --verbose
```

Programs using owned containers or I/O automatically link the bundled C runtime.
Thread, TCP, TLS and UTF-8 operations add their native adapters. The current runtime supports
macOS and Linux executables. Runtime sources are embedded in the compiler binary,
so ordinary Bhasa builds need no Rust installation or source checkout. Declared
Rust adapters do require Cargo and Rust when built. Clang, target headers and
native libraries are still required. `emit-llvm` and object emission leave
runtime references unresolved for later linking.

Record methods use the same compiler and build commands as ordinary functions.
For example, run `bhasa run examples/records/methods.bh`, or add
`--frontend bhasa` with a full hybrid host. Both frontends implement borrowed
receivers; completed method verification is recorded in the
[integration report](status/integration.md).

TCP address selection is a source operation, not a compiler flag.
`tcp_listen(port)` binds IPv4 loopback; `tcp_listen_on(host, port)` accepts
numeric IPv4/IPv6 addresses, including explicit wildcards, and returns an owned
listener with a status to check. TCP calls perform synchronous I/O with optional
deadlines. See [networking](network.md) for accepted addresses, status handling
and descriptor-copy semantics. The optional [TLS adapter](tls.md) provides
verified client/server connections using installed OpenSSL 3; ordinary TCP
applications do not require it.

The `std` backend libraries are ordinary `.bh` imports. They require a local
`[dependencies]` entry in lowercase `bhasa.toml`, as shown in
[backend libraries](backend.md). No package download or Rust adapter is needed
for text, typed collections, JSON or HTTP. From the repository root:

```sh
bhasa check examples/http/main.bh
bhasa build examples/http/main.bh --release -o build/http-example
./build/http-example 8080 3
```

The example binds loopback and stops after three requests. Its port and request
count are program arguments. This example uses plain TCP and handles one bounded
HTTP request per connection, without chunking or keepalive. The optional TLS
package and [HTTPS example](tls.md) use the same generic HTTP helpers. A hybrid
host can compile either program with `--frontend bhasa`.

`async_spawn` schedules scalar jobs on four persistent workers with a 64-job
queue and blocking backpressure. `await` consumes a task and blocks until it
finishes. See [concurrency](concurrency.md) for worker restrictions and ownership
rules; this scheduler does not add nonblocking I/O or coroutines.

`--gpu` selects the host adapter for GPU calls. The default is Metal on macOS and
Vulkan on Linux. Explicit selection never silently falls back to another backend
or the CPU. It is an executable-link option and cannot accompany `--emit object`.
Kernel formats, driver requirements and tested hardware are listed in
[GPU support](gpu.md). Bhasa kernel syntax is not yet implemented.

Development builds use Clang `-O0`; `--release` uses `-O2`. Arithmetic traps and type rules are the same in both modes. The Bhasa `--release` flag controls the application, independently of Cargo's `--release` flag for the Rust compiler.

An executable needs an entry point. A Bhasa `main` must have no parameters and return `void` or `int`. A source file containing only callable functions can be checked or emitted as an object without `main`.

## Targets and cross-compilation

| Name | Accepted explicit triple | Current use |
| --- | --- | --- |
| `native` | Host-dependent | Host build and execution |
| `linux-x64` | `x86_64-unknown-linux-gnu` | LLVM IR or objects; linking needs Linux target libraries and tools |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | LLVM IR or objects; linking needs Linux target libraries and tools |
| `windows-x64` | `x86_64-pc-windows-msvc` | LLVM IR or objects; linking needs the Windows SDK and compatible tools |
| `macos-arm64` | `arm64-apple-macosx` | LLVM IR or objects; linking needs the Apple SDK and compatible tools |
| `macos-x64` | `x86_64-apple-macosx` | LLVM IR or objects; linking needs the Apple SDK and compatible tools |
| `wasm32` | `wasm32-unknown-unknown` | Objects for pure functions; no runnable WASI promise |

The command accepts these aliases and exact triples, and rejects unknown targets. Before a build, Bhasa asks the selected Clang frontend to emit LLVM IR for empty C input and reads its resolved target triple. Explicit target selections are passed to that probe. This incorporates SDK and deployment defaults so Bhasa's generated IR and the subsequent compilation agree, including macOS and Windows target versions. The probe needs no source files, headers or linking, and warnings remain enabled. Standalone LLVM emission can work without Clang and maps the Rust compiler host configuration to a native target. If that host has no mapping, pass an explicit target from the table.

`bhasa targets` lists code generation candidates. It does not inspect or certify SDK installation. Even object emission depends on the installed Clang having the requested target backend. A cross-target object is not a runnable executable.

For example, save this as `math.bh`:

```bhasa
def square(value: int) -> int {
    return value * value
}
```

Then emit IR or request an object:

```sh
bhasa emit-llvm math.bh --target linux-x64 -o build/math-linux.ll
bhasa build math.bh --target linux-x64 --emit object -o build/math-linux.o
bhasa build math.bh --target wasm32 --emit object -o build/math-wasm.o
```

These commands do not run their output. `run` only builds for the native host and does not accept a cross-target option. Cross-target executable linking requires the target's libraries, SDK and linker. Bhasa does not substitute the native target when a cross build fails. The bootstrap does not yet manage target SDK packs or provide a bundled LLVM/LLD distribution.

For Wasm, keep the source to pure functions. Printing, host imports and runnable WASI programs are outside the supported subset.

## Native link inputs

Run the complete Rust adapter example from the repository root with:

```sh
sh examples/rust-interop/run.sh
```

The script builds the adapter and links it into the Bhasa caller. Running its
`main.bh` alone leaves `bhasa_rust_double` undefined. An `extern def` declares
the function's signature; it does not build or locate its implementation.

Declare imports with `extern def`, then supply the matching object or archive explicitly:

```sh
bhasa build caller.bh --link path/to/adapter.a -o build/caller
bhasa run caller.bh --link path/to/adapter.a
```

Build the adapter before either command. `run` links for the native host and executes the result. Use additional `--link` inputs when needed, either as `--link first.a second.a` or repeated `--link` options. Put the source file before the link inputs. `--link` cannot be combined with `--emit object`. The source file and all native inputs must target compatible architectures and ABIs. Clang receives LLVM IR with `-x ir`, and the driver resets the input language with `-x none` before native link inputs.

Only integer parameters and integer or void results are supported at the trusted
C ABI boundary. For local Cargo adapters, declare `[rust-dependencies]` entries
in `bhasa.toml`; executable builds and `run` build and link those static archives
automatically. Cargo runs offline in persistent per-adapter target directories.
Unchanged dependencies and Bhasa-only edits reuse Rust compilation artifacts.
`check`, LLVM emission and object emission do not invoke Cargo. See
[Rust dependency caching](caching.md) and the
[package example](../examples/rust-package/README.md).

Foreign APIs are not imported automatically, and linked code must not unwind
through Bhasa. See the [Rust adapter example](interop.md) for the explicit C ABI
procedure. Native library requirements beyond the adapter may still need
additional `--link` inputs and an installed SDK.

The Bhasa scalar subset has no garbage collector. A foreign library can allocate or include a runtime, so its no-GC properties require a separate review. Using Rust for the compiler also does not imply compatibility with every Rust library or Rust's complete safety model.

## Errors and troubleshooting

- If Clang is missing, install it separately or set `BHASA_CLANG` to its executable path. `check` and `emit-llvm` still work without it.
- If the target name is unknown, choose an alias or exact triple shown by `bhasa targets`.
- If an object backend is unavailable, select a Clang installation that supports that target.
- If cross linking reports missing libraries, SDK files or a linker, install the required target tools separately. An object build can help distinguish code generation from linking problems.
- If linking reports an undefined imported symbol, build and supply the matching native object or archive with `--link`. If it is already supplied, verify its exported C ABI symbol, target, and required native dependencies. Installing an SDK alone does not provide a user-defined adapter.
- If linking reports a missing entry point, add a valid `main` or request `--emit object` for a library source.
- If the program traps, inspect its signed arithmetic and divisors. Optimizing the program does not disable these checks.

## Compiler builds and self-rebuilding

`cargo build --release` builds the Rust bootstrap. The hybrid script without
arguments links the Bhasa scanner into it; `--frontend` also links the Bhasa
parser, checker and LLVM emitter. Native archives are cached by their source,
toolchain and target inputs, and Cargo keeps the host build cache.

`sh bootstrap/compiler/check.sh` separately builds three generations of the
Bhasa frontend and compares their emitted LLVM and program behavior. Its current
corpus includes records, methods, enums, exhaustive matches and runtime
operations as well as scalar examples. The [integration report](status/integration.md) records the tested
revision and platform limits.
[Self-hosting](self-hosting.md) describes the exact commands, component boundaries
and verification limits. The Rust host remains part of the chosen architecture.

The public Rust compiler APIs and `lsp::serve` run on the caller's stack. Near
the accepted nesting limit, a debug build can exhaust a small worker or test
thread stack. Embedders must provide sufficient stack space or use the compiler
process; arbitrary embedding stack sizes are not covered by the current safety
claims. The standalone CLI and LSP use the process main thread. On the verified
Mac, both accept 128 empty nested blocks and report a diagnostic at 129.
