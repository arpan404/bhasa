# Compiler integration status

Verified October 11, 2026 on ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. Typed structured tasks and checked agent inspection are integrated.
The user authorized the full backlog in `todo.md` and local progress commits.
Prior iteration evidence is preserved in [integration-iteration.md](integration-iteration.md).

## Delivered

- Global one-argument workers accept supported non-task values and return
  supported non-task results, including void. `join`/`await` infer the result.
  Borrowed owned inputs acquire independent storage before submission; exclusive
  temporaries transfer. Worker results follow ordinary deterministic ownership.
  Buffers, records, enums and concrete List/Map values are covered in both
  frontends. Network handles retain shared endpoint/session semantics.
- Generated typed wrappers use opaque owned frames and type-specific cleanup.
  Integer-to-integer workers retain the scalar ABI. The native runtime uses
  correctly typed function pointers and explicit ABI tags, checks allocation
  sizes, and preserves four workers, 64 waiting jobs and blocking backpressure.
  Every task still requires lexical consumption. Transitive extern/pool worker
  restrictions and nested classic spawn/join behavior remain enforced.
- `bhasa inspect [FILE] [--json]` checks modules and concrete generic APIs, then
  reports deterministic JSON declarations, privacy flags, import/dependency edges,
  package/adapter metadata and original-file locations. It does not invoke
  Clang/Cargo, execute programs or write files. Errors are structured. Unused
  generic templates are explicitly outside the current query result.
- `examples/concurrency/typed.bh` demonstrates caller/worker isolation. Public
  guides and `todo.md` describe current contracts and remaining work. Local
  commits are authorized; installation and publishing are not part of this work.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 411 passed; build/verify-typed-tasks-reference.log |
| Complete full hybrid | 422 passed; build/verify-typed-tasks-hybrid-frontend.log |
| Independent task ownership | O0/O2 ASan/UBSan and atomic allocation counters, imported generics, lexical/call-graph rejection and network error-handle transfer; tests/typed_tasks.rs and typed_tasks_reference.rs |
| Native task runtime | Mixed scalar/owned saturation, blocked producers, 4,096 jobs, cold start, 14 fatal cases and zero tracked allocations; tests/typed_task_runtime.rs; lane probes also pass TSan |
| Native frontend parity | Fresh seed, typed fixture, 22 rejection probes, O0/O2 ownership instrumentation and equal self-rebuilt LLVM; tests/typed_tasks_bhasa.rs |
| Checked inspection | Three CLI groups and independent privacy/generic/location review; tests/agent_inspect.rs |
| Editor | 47 LSP tests in both suites and 38 Node tests; build/verify-typed-tasks-editors.log |
| Formatting/Clippy | Both configurations pass with warnings denied; build/verify-typed-tasks-fmt.log, build/verify-typed-tasks-clippy-reference.log and build/verify-typed-tasks-clippy-hybrid.log |
| Refreshed releases | target/release/bhasa and build/hybrid/frontend/aarch64-apple-darwin/release/bhasa execute typed tasks, previous examples, five package tests and checked inspection; build/verify-typed-tasks-release-smoke.log |
| Cache reuse | 22 native hits, four unchanged Rust adapter runs and two unchanged hybrid builds with zero compilations; build/verify-typed-tasks-unchanged-build-1.log and build/verify-typed-tasks-unchanged-build-2.log |
| A → B → C | Complete compiler LLVM and example outputs match, including typed workers; build/verify-typed-tasks-selfhost.log; build/selfhost/run.alQpR5Im |

Broad suites ran serially after source freeze with two Cargo jobs and one test
thread. The default sandbox blocked loopback HTTP startup; the same fixture and
both complete suites pass with loopback permission. A stale test that rejected
float workers was updated to the new typed-worker contract before final suites.
Independent review repaired malformed extern rejection fixtures and an inspect
privacy false positive, retaining regressions.

Fresh archive key: `eacf7f587e74072324cc9f5b76b01773140d336e3f83ec28530bdc068b5ba50d`.
Its concatenated lexer/emitter source exactly matches the frozen sources.
.bh, lowercase bhasa.toml, deterministic no-GC ownership, Rust host architecture
and existing caches remain intact. The old heartbeat remains paused.

Real Metal dispatch and TCP/TLS loopback pass. CUDA/Vulkan hardware tests skip
because devices/drivers are unavailable. The local Clang lacks a wasm32 backend.
No Linux/Windows-device, VS Code GUI or runnable WASI execution claim is made.

## Remaining work

The language remains experimental. The full backlog is in [todo.md](../../todo.md).
Next: integer bitwise/radix support, descending/stepped ranges and checked editor
references/rename, under [systems-tooling-milestone.md](systems-tooling-milestone.md).
Tasks remain lexical handles; channels, cancellation, shared-state locks,
coroutines and async I/O are unimplemented. General borrowing/lifetimes, owned
parameters, recursive structures, fixed-width/unsigned numbers, closures/traits,
portable GPU kernels, runnable Wasm/embedded runtimes and the wider library/
package/release ecosystem remain open. Foreign adapters remain trusted native
code with no automatic no-GC or thread-safety proof. Per-module incremental
compilation and C/Go performance parity are not established.
