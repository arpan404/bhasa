# Typed tasks in the Rust reference frontend

The reference checker retains `Type::Task`. Each lexical task binding separately
records whether it is live and the worker's result type. `join` and `await`
expressions receive that result type. Existing restrictions still require a
complete task initializer and a join/await initializer or expression statement;
void results can be consumed only as statements. Task copying, storing, passing,
returning, shadowing, reassignment, double consumption, conditional consumption
and unjoined scope/loop exits remain errors.

Workers must be global one-argument Bhasa definitions. Supported non-task values
include scalars, static strings, buffers, integer vectors, records, enums,
concrete List/Map types and native resource handles. Void arguments and task
results are rejected. The checked call graph still rejects transitive extern
calls and pooled submission/await from workers. Nested classic spawn/join remains
supported. Invalid task-returning declarations also diagnose safely rather than
assuming their call has spawn metadata.

## Native lowering

Integer-to-integer workers retain the existing scalar runtime ABI. Other
signatures use the owned-task runtime API and one internal wrapper per worker.
Submission acquires an independent input before dispatch: borrowed owned values
clone, while an exclusive temporary transfers. Resource copies retain their
existing endpoint/session semantics rather than creating independent network
connections.

The input frame stores one LLVM value in checked native allocation. A generated
wrapper loads that value, invokes the normal borrowed worker, drops the input and
frees its frame. Ordinary function-return rules already acquire owned results;
the wrapper transfers that result into a result frame. Join/await uses task-binding
metadata to load the typed value and free the result frame. The resulting owned
value follows ordinary binding/statement cleanup. Void workers return a null
frame. Frame sizes use target LLVM layout expressions rather than assumed scalar
sizes. There is no garbage collector or new source syntax.

No AST or parser changes were necessary. Reference changes are confined to
`src/sema.rs`, `src/codegen.rs` and `tests/typed_tasks_reference.rs`.

## Focused verification

The new test file checks task/result annotations, scalar-versus-owned ABI
selection, lexical and call-graph rejection, and native O0/O2 ownership behavior.
The native fixture instruments emitted LLVM and core/collection/concurrency C
with ASan/UBSan and uses atomic allocation accounting across threads. It covers
bool/float/str, buffer/ints, nested records/enums/List/Map, generic workers,
caller mutation after submission, borrowed return copies, temporary field inputs,
discarded owned/void results, nested classic workers and loop cleanup.

Coordinator verification passes all four focused groups, then complete reference
411/full-hybrid 422 tests, both Clippy configurations and formatting. O0/O2
sanitizers and counted ownership pass in the focused and combined runs. See
`integration.md` for final release/cache evidence and platform limits. This does
not establish production readiness or a complete Rust borrowing model.
