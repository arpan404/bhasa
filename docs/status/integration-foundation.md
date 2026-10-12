# Compiler integration status

Verified October 11, 2026 on ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. The [namespaces/types/tests/native-cache milestone](next-milestone.md)
is integrated. Three existing internal agents implemented independent lanes;
root composed lowering, runtime linkage, test execution and final acceptance.
Prior evidence is preserved in [integration-generics.md](integration-generics.md).
No installs, cache cleaning, commits, resets or publishing occurred. The old
app-chat heartbeat remains paused.

## Delivered

- Aliased file/package imports, qualified functions/types/variants/generics,
  explicit private declarations/fields/methods and isolated module identity.
  Unaliased legacy graphs retain their behavior. Typed privacy survives inferred
  factory returns, chained accesses and List/Map getters. Extern symbols retain
  their C ABI names; worker safety remains transitive.
- IEEE 754 binary64 `float`, finite decimal/exponent literals, arithmetic and
  comparisons, explicit int/float casts with checked float-to-int conversion,
  printing and owned aggregate/generic support in both frontends. No implicit
  coercion or floating remainder.
- `List[T]` and `Map[K,V]` with growable native storage and statically checked
  nominal types. Compiler-generated typed clone/drop callbacks preserve owned
  snapshots and deterministic cleanup. Native storage holds concrete slot bytes.
  Int/str keys, bounds/missing-key failures, nesting and borrow/worker restrictions
  are verified. No GC or runtime type reflection.
- `bhasa test` discovers boolean test functions, compiles native harnesses,
  isolates each test in a process, filters, applies deadlines, bounds captured
  output and emits JSON. False results/traps/timeouts do not stop later tests;
  later compile failures retain completed results. Generic tests are rejected.
  Escaped descendants cannot keep pipe collection hanging; this is not a sandbox.
- Persistent complete native artifact reuse after Cargo adapter freshness checks.
  Keys include emitted LLVM, compiler/toolchain/options/environment, runtimes,
  preprocessed headers and tracked libraries. Atomic digest-verified copies,
  invalidation, corruption recovery, concurrent writers and symlink protection
  are tested. Untrackable inputs compile without reuse.
- LSP module-aware navigation and unsaved alias handling, private symbol outlines,
  float/privacy/collection grammar highlighting, guides and runnable examples.

`.bh` and exactly lowercase `bhasa.toml` remain unchanged. Rust owns projects,
module/generic lowering, native builds/caches and editor services. Member privacy
uses an additional typed Rust-host verifier before either selected frontend.
Native Bhasa lexing/checking/emission has no fallback. The standalone seed does
not independently perform module/privacy/generic lowering.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 384 passed; build/verify-foundation-reference.log |
| Complete full hybrid | 393 passed; build/verify-foundation-hybrid-frontend.log |
| Review regressions | 10 modules, 39 LSP, 8 native runner, 3 collections passed; build/verify-foundation-review.log and modules-hybrid-final.log |
| Cache/SHA after Clippy repair | 3 cache groups + SHA vectors passed; build/verify-foundation-incremental-final.log, sha-final.log |
| Formatting and Clippy | Passed with warnings denied in both configurations; build/verify-foundation-fmt.log, clippy-reference.log, clippy-hybrid.log |
| Editor client/grammar | 33 Node tests passed; build/verify-foundation-editors.log; GUI extension host not exercised |
| Release compilers | Refreshed target/release/bhasa and build/hybrid/frontend/aarch64-apple-darwin/release/bhasa |
| Release execution | Namespaces, numbers, owned collections, prior generics/executor and native tests pass with both compilers; build/verify-foundation-release-smoke.log |
| Native output reuse | 14 hits across repeated release examples/tests/adapters; repeat cases require Reused native artifact |
| Rust dependency reuse | Four unchanged adapter runs print 42 with zero Cargo compilations |
| A → B → C | B/C complete compiler LLVM and outputs match; build/verify-foundation-selfhost.log; build/selfhost/run.75IuC7IS |
| Native frontend archive reuse | Two unchanged hybrid rebuilds perform zero compilations; build/verify-foundation-unchanged-build-1.log and -2.log |

Broad suites ran serially after source freeze with two Cargo jobs and one test
thread. The reference suite preceded a portable SHA chunk API cleanup and test
slice cleanup; focused SHA/cache tests passed again. Full hybrid includes those
changes. Its initial run correctly rejected two inputs but failed assertions
that required Rust message wording. Corrected assertions verify original EOF
spans and native/reference worker codes; the complete rerun passes. No compiler
behavior changed for that correction. Both final Clippy configurations are clean.

Final archive key: `2d994114af47078895ebe44fe861ccc4912b40d3c582716019a91f2955ffb938`.
Its concatenated source exactly matches the frozen lexer/emitter. O0/O2 ASan and
UBSan verify nested collection ownership, alias-safe growth/replacement, collisions,
deletions, snapshots and cleanup, plus numeric casts and existing runtimes.
Real Metal dispatch and TCP/TLS loopback pass. CUDA/Vulkan hardware tests skip
because usable devices/drivers are unavailable; protocol mocks are not hardware
verification. No Linux-device execution or VS Code GUI claim is made. The local
Apple Clang lacks a wasm32 backend; runnable WASI support remains unimplemented.

## Remaining limits

This is still an experimental language. Fixed-width/unsigned integers, f32,
fixed arrays, slices, collection iteration/dynamic keys, general lifetime
parameters and a complete Rust borrow model remain future work. Native caching
skips compilation/linking of unchanged outputs; per-module incremental frontend
work is not implemented. No C-speed or Go-compilation-time claim is established.
The test runner lacks coverage, parallel execution and a sandbox. Hash maps use
deterministic hashing and do not claim resistance to adversarial collisions.
Foreign adapters/kernels remain trusted native code and must satisfy the no-GC
requirement. Runtime cross-target linking still needs compatible SDKs/libraries.
