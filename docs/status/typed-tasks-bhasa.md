# Typed structured tasks in the Bhasa frontend

The native Bhasa frontend supports global one-argument workers whose parameter
is any supported non-void, non-task value and whose result is any supported
non-task value, including void. `spawn` and `async_spawn` infer the worker result
type for `join` and `await`. Void joins are statements. A joined owned result is
an independent value; discarding it as a statement releases it immediately.

The inferred task type remains code 6. Each task binding stores its worker
signature alongside the existing pending/consumed state. This preserves lexical
task rules without exposing transferable or manually constructed task values.
Both inputs and results use their ordinary checked concrete types, including
specialized generic records, enums, Lists and Maps.

Int-to-int workers keep the existing scalar C ABI. Other signatures use a
generated wrapper and runtime-owned argument/result frames. The submitting code
copies borrowed owned inputs before submission, or transfers an exclusive
temporary. The wrapper calls the ordinary borrowing worker, drops its input and
frame, then returns a frame containing the worker's independently owned result.
The joining code loads that result and frees its frame. Void wrappers return
null. Frame sizes come from the target LLVM type layout. Aggregate destruction
uses the same generated helpers as ordinary code; there is no tracing GC.

The existing transitive extern and pool-operation restrictions apply to every
typed worker. Classic nested spawn/join remains available inside workers.
Network resource copies retain their documented shared endpoint/session
semantics. Tasks remain confined to their declaring block and cannot be copied,
rebound, shadowed, stored, passed or returned. Typed tasks do not add closures,
multiple worker parameters, cancellation, channels or detached tasks.

Implementation: `bootstrap/compiler/emitter.bh`. No token or lexer change was
needed. The threads runtime marker recognizes scalar/owned submission,
consumption and frame allocation. Runtime symbols are implemented separately in
`runtime/concurrency.c`.

## Verification

Local native probes on arm64 macOS used the existing reference compiler to
build the current Bhasa frontend under `build/typed-tasks-native`. The typed
fixture passed at O0 and O2 with ASan on generated LLVM functions, ASan/UBSan on
runtime code, and an atomic allocation counter covering generated aggregate
allocations and core/collection/task runtime allocations. Every run reported
zero live tracked allocations. The fixture exercises bool, float, str, buffer,
ints, records, enums, List/Map, specialized generic workers, differing argument
and result types, void results, temporary field inputs, nested tasks, independent
input snapshots and discarded owned results. Probe evidence is in
`build/typed-tasks-native/probe.log`.

Twenty-two malformed or unsafe native programs were rejected without LLVM
output; transitive extern cases specifically returned BH209. Evidence is in
`build/typed-tasks-native/invalid.log`. A fresh A-to-B-to-C self rebuild emitted
byte-identical compiler LLVM in `stage-b.ll` and `stage-c.ll`.

`tests/typed_tasks_bhasa.rs` adds integrated reference/native parity, sanitizer
and allocation checks, shared generic expansion, rejection coverage and native
self rebuild. Its optional hybrid test checks the FFI frontend too. Those Cargo
tests pass under coordinator validation with the refreshed hybrid archive.
Complete reference/full-hybrid suites pass 411/422 tests; both Clippy
configurations, formatting and the extended A/B/C gate pass. See integration.md
for final release/cache evidence and platform limits.
