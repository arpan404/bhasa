# Compiler integration status

Verified October 11, 2026 on ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. The [iteration/editor milestone](next-milestone.md) is integrated.
Three existing agents implemented native iteration, editor support and independent
acceptance/review. Root implemented Rust iteration, composed source maps and
lowering, extended examples/self-rebuild and ran combined verification. Prior
completed evidence is preserved in [integration-foundation.md](integration-foundation.md).

## Delivered

- `for name in range(end)` and `range(start, end)` provide ascending signed-int
  iteration with an exclusive upper bound. Bounds evaluate once, left to right;
  reversed/empty ranges perform no iterations. The hidden cursor is independent
  of the mutable binding and cannot overflow.
- `for name in list_expression` accepts `List[T]`, evaluates its source once
  and acquires an owned deep
  snapshot. Each iteration owns an independent element snapshot. Clearing,
  replacing or mutating the source/binding cannot invalidate remaining elements.
  Temporary lists transfer ownership to the loop. Snapshot cost is proportional
  to the stored values; this is not a borrowed view.
- `break`/`continue` target the nearest while/for. Both emitters clean every
  exited owned scope. Continue retains the iterator snapshot; break, return and
  normal completion drop it. The checker rejects outside-loop exits, unreachable
  statements and unjoined tasks in exited scopes. Outer tasks remain in scope;
  existing lexical joins and transitive extern/pool worker restrictions remain.
  Loops never establish a guaranteed function return.
- Completion/hover follows visible declarations, lexical scopes, checked types,
  module aliases and privacy. Typed chained receivers and List/Map methods show
  readable signatures. Template hovers preserve source types or neutral bindings.
  Results are sorted/capped at 128, use UTF-16 edits and bounded plaintext details.
  Partial-source recovery is query-local; mid-word edits replace the whole name.
- VS Code providers reject stale responses, escape hover as untrusted text and
  highlight loop keywords. Runnable iteration examples and five package tests
  demonstrate the new syntax. The self-rebuild gate executes the new range example.

.bh, exactly lowercase bhasa.toml, deterministic no-GC ownership, Rust host
architecture and persistent Cargo/native caches remain intact. Both hosted
frontends use the same module/generic lowering and typed privacy verifier.
Native checking/emission has no fallback. The standalone seed still needs host
lowering for source modules/generics. No installs, cache cleaning, commits,
resets or publishing occurred. The old app-chat heartbeat remains PAUSED.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 398 passed; build/verify-iteration-reference.log |
| Complete full hybrid | 408 passed; build/verify-iteration-hybrid-frontend.log |
| Independent iteration | Five groups pass with both host frontends, including O0/O2 ASan/UBSan and counted allocation cleanup; tests/iteration.rs |
| Native seed/parity | Fresh seed rebuild, identical complete-compiler LLVM, rejection checks and O0/O2 execution; tests/iteration_bhasa.rs |
| Editor protocol/client | 47 LSP tests in both suites; 38 Node tests; build/verify-iteration-lsp-final.log and editors.log |
| Formatting/Clippy | Warnings denied in both configurations; build/verify-iteration-fmt.log, clippy-reference.log, clippy-hybrid.log |
| Refreshed releases | target/release/bhasa and build/hybrid/frontend/aarch64-apple-darwin/release/bhasa; release-reference.log and hybrid suite |
| Release execution | New range/List examples, namespaces/numbers/collections/generics/executor and five native package tests pass with both compilers; release-smoke.log |
| A → B → C | B/C complete compiler LLVM and example outputs match; build/verify-iteration-selfhost.log; build/selfhost/run.0i9MfkYr |
| Cache reuse | 20 native hits across release smoke, four unchanged Rust adapter runs with zero Cargo compilations; two unchanged hybrid builds with zero compilations |

Logs named without a full prefix in the table are under build/verify-iteration-.
All broad compiler suites ran serially after source freeze with two Cargo jobs
and one test thread. Final archive key:
`b2ae9f5fb380dcf392025873346826116006fdc7e0a2ceddcabbd314e0f812d1`.
Its concatenated lexer/emitter source exactly matches the frozen sources. Cache
reuse logs are unchanged-build-1.log and unchanged-build-2.log.

Independent review repaired a synthetic local identifier, parenthesized range
parity, range/keyword reservation through module/generic lowering, a test helper
pipe deadlock, and editor alias collisions, suffix duplication, arbitrary generic
hover specialization and spaced declarations. A broken alias-edit test changed
its import path unintentionally; corrected it to preserve the saved graph.
Clippy prompted a mechanical Option binding repair before broad verification.
Release smoke caught a stale default executable because hybrid bootstrap uses a
separate stage-0 directory. An explicit default release build repaired delivery;
release smoke and A/B/C passed afterward. No language sources changed after the
broad suites.

Real Metal dispatch and TCP/TLS loopback pass. CUDA/Vulkan hardware tests skip
because usable devices/drivers are unavailable; protocol mocks are not hardware
verification. The local Apple Clang has no wasm32 backend. No Linux-device,
VS Code GUI or runnable WASI execution claim is made.

## Remaining limits

The language remains experimental. Descending/stepped ranges, first-class or
borrowed iterators and iteration over Map/buffer/ints are unsupported. List
iteration copies owned values. Fixed-width/unsigned numbers, f32, arrays/slices,
closures/traits, channels, coroutines/async I/O, lifetime parameters and a complete
Rust borrow model remain future work. Native caches reuse complete outputs;
per-module frontend work is still repeated. No C-speed or Go-compilation-time
claim is established. Foreign adapters/kernels remain trusted native code and
must satisfy the application's no-GC rule. Cross-target runtime linking needs
compatible SDKs/libraries; runnable WASI is unimplemented.
