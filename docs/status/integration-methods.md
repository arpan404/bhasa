# Compiler integration status

Verified on October 11, 2026, ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. This report supersedes earlier component snapshots. Six dedicated
Codex chats and additional review agents completed the methods and backend
usability milestone. All assigned lanes are frozen and combined checks passed.
No commits, resets, publishing or dependency downloads were performed.

## Delivered compiler

- `.bh` source and exactly lowercase `bhasa.toml`, including on case-insensitive
  filesystems. Local packages, imports, scaffolding and original-file diagnostics.
- Named records, methods, nested fields and owned buffer/integer-vector/TCP fields.
  Deterministic clone/free operations, borrowed parameters and checked field
  replacement. Constructor evaluation and later-argument mutation checks protect
  captured owned fields.
- Methods use inferred mutable-borrowed `self`, qualified static dispatch and
  receiver-before-argument evaluation. Temporary receivers survive return
  acquisition. Method edges participate in recursive worker-to-extern checks;
  direct method spawning remains outside the scalar worker ABI.
- Native scalar workers with mandatory joins, file/console I/O, subprocesses,
  synchronous TCP listeners/sockets, and Metal/CUDA/Vulkan host adapters.
  `tcp_listen_on(str|buffer, port)` accepts explicit numeric IPv4/IPv6 addresses;
  `tcp_listen(port)` retains its IPv4-loopback default.
- Rust staticlib adapters through explicit C ABI exports. Persistent Cargo caches
  reuse unchanged Rust builds, including after Bhasa-only changes.
- `bhasa fmt`, JSON diagnostics, `bhasa lsp`, and a dependency-free VS Code
  extension. Record/field/method outlines and checked method definitions retain
  UTF-16 positions and unsaved module overlays. Each overlaid file is parsed
  separately before type-remapping and checking the loaded graph.
- Optional Bhasa lexer/parser/checker/LLVM emitter inside the Rust host through
  `--frontend bhasa`. The host retains package, toolchain and editor services.
  There is no fallback to the Rust parser/checker when Bhasa is selected.

The host inserts file-boundary tokens between imported sources. They are legal
only between complete declarations, preventing two invalid files from completing
each other's syntax. Bare CR, CRLF, LF and absent final newlines are covered.
The package loader rejects named pipes, directories and oversized inputs.

The runtime repairs macOS descriptor inheritance during process spawning,
implicit shell fallback for executable text without a shebang, and SIGPIPE
termination during fallible writes. Focused fault-injection and sanitizer tests
cover error cleanup and signal-state preservation.

## Verification

| Check | Evidence |
| --- | --- |
| Reference compiler suite | 300 tests passed; `build/verify-methods-reference.log` |
| Full hybrid compiler suite | 307 tests passed; `build/verify-methods-hybrid-frontend.log` |
| Independent method acceptance | Seven groups passed in both modes, including imported type IDs, malformed inputs, worker graphs and native ownership; [acceptance report](acceptance.md) |
| Formatting and Clippy | Passed with warnings denied for default and full hybrid builds; `build/verify-methods-clippy-reference.log`, `build/verify-methods-clippy-hybrid.log` |
| Release binaries | Rebuilt `target/release/bhasa` and `build/hybrid/frontend/aarch64-apple-darwin/release/bhasa` |
| A → B → C self-rebuild | Passed; B/C compiler LLVM and example results match; `build/verify-methods-selfhost.log`, artifacts `build/selfhost/run.ORZKBDfu` |
| Unchanged hybrid build | Reused the native archive, no production Rust compilation; Cargo build steps finished in 0.03s and 0.05s; `build/verify-methods-unchanged-build.log` |
| Release examples and Rust reuse | Method/resource examples passed with both release binaries; two hybrid Rust-library runs printed `42` without Rust recompilation; `build/verify-methods-release-smoke.log` |
| Editor extension | All 31 Node tests and JavaScript syntax checks passed; actual VS Code GUI host unverified |

Both broad suites ran after source freeze, serially with two Cargo build jobs
and one test thread. The final native archive input exactly matches the frozen
lexer and emitter. Its cache directory ends in
`5861ee7a5b8d5f6a027c18d7843bbd39f78154f1aed9d64c6fab70345820d9ae`.
The previous 265/273-test milestone remains recorded in the earlier logs; those
results are not used as proof of the new methods and bind behavior.

Independent review reproduced macOS accepting an IPv6 zone and silently
discarding it during numeric parsing. Scoped inputs now fail before socket
creation. Owner and independent O0/O2 sanitizer regressions verify the repair;
real IPv4 and IPv6 loopback execution passed without an IPv6 skip. The Bhasa
frontend's preliminary brace counts also needed correction to match Rust at
the nesting boundary. CLI and LSP accept 128 empty nested blocks and reject 129.

Sanitizer checks instrument generated LLVM methods, record helpers and C
runtime code at O0/O2; allocation ledgers verify cleanup. Independent safety
review passed 18 probes per frontend and 24 native lifetime/evaluation runs.
No review findings remain open within the milestone's defined scope.

Native archive keys include the bootstrap executable, frontend source, compiler
and archiver binaries, target and preprocessed core runtime. Header changes via
SDK/CPATH invalidate the cache. Repeated preprocessing is stable; changing header
contents at the same path changes the key. `--build-only` skips the test suite
when verifying build reuse. Tests deliberately create fresh Rust fixture crates,
so their compilation is separate from production artifact reuse.

## Platform evidence and remaining work

Metal executed real kernels on this Mac. CUDA and Vulkan mock-driver protocols,
cleanup and CLI selection passed; their hardware tests explicitly skipped
because the drivers/devices were unavailable. This Clang lacks a wasm32 backend,
so Wasm object execution is unverified. Foreign scalar object generation is
covered, but cross-target executables still require compatible SDKs/libraries.
Linux runtime execution is unverified on this host.

Bhasa remains experimental. It has no generics, floating-point types,
closures, general lifetime system, async/TLS/timeouts, package registry, portable
Bhasa GPU kernel language or portable Wasm runtime. Current record borrowing is
conservative and is not Rust's full borrowing model. Rust compatibility uses
explicit adapters, not arbitrary Rust API imports. Bhasa's runtime has no GC;
third-party adapters must independently meet that requirement. No Rust-equivalent
security, C-level performance or Go-level compile-time claim is established by
these tests.

Rust compiler-library APIs and `lsp::serve` use the caller's stack. Near-limit
debug compilation can exhaust a small worker/test-thread stack. The standalone
CLI and LSP pass the tested boundary cases on this host's process main stack;
arbitrary embedding stack sizes remain unverified. See [toolchain](../toolchain.md).

See [README](../../README.md), [language](../language.md),
[self-hosting](../self-hosting.md), [packages](../packages.md),
[caching](../caching.md), [networking](../network.md) and [tooling](../tooling.md).
