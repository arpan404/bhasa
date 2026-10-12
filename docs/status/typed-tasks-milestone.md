# Typed structured tasks

User authorization on October 11, 2026 expands work to the complete `todo.md`
backlog and explicitly permits local progress commits. Prior milestone
restrictions on commits are superseded. No publishing, SDK installation or cache
cleaning is requested.

The first dependency-ordered increment generalizes tasks while preserving the
current lexical linear-handle rules, no-GC ownership, both frontends and worker
call-graph restrictions. `spawn(worker(value))` and `async_spawn(worker(value))`
accept a global Bhasa function with one supported non-task argument and return a
task. The argument is acquired as an independent owned value before submission.
`join(task)`/`await(task)` return the worker's checked result type. Void results
may be consumed only as statements. Network resource copies retain their
documented shared-endpoint/session semantics. Tasks remain inferred and cannot
be copied, stored, passed, returned or consumed outside their declaring block.

Runtime addition: `bhasa_rt_spawn_owned(void *(*worker)(void *), void *argument)`
and `bhasa_rt_async_spawn_owned` submit generated wrappers.
`bhasa_rt_join_owned`/`bhasa_rt_await_owned` consume the handle and return an
opaque owned result frame. Generated wrappers acquire/drop typed input and pack
typed results; callers unpack/free result frames. `bhasa_rt_task_alloc(int64_t)`
and `bhasa_rt_task_free(void *)` provide checked frame allocation/destruction.
Keep existing scalar native ABI entry points for compatibility. Runtime worker
guards apply equally to new submissions.

Ownership: packages owns Rust AST/parser/checker/emitter changes and its
focused tests; frontend_parity owns Bhasa frontend parity changes and its tests;
rust_cache owns C concurrency runtime/header plus independent runtime tests;
root owns host/lowering/builtins integration, examples, docs, combined
verification and commits. Workers may read all files but must request ownership
before editing another lane. Serialize Cargo checks through root.

Keep the scalar i64 runtime ABI for int-to-int workers. Other signatures use
owned frames; a void-result wrapper returns null and statement consumption has
no result frame to unpack. Worker result types are stored with lexical task
bindings; task handles retain their current type/encoding and transfer limits.

Acceptance: scalar compatibility; bool/float/str/buffer/ints/record/enum and
concrete generic collection input/results; worker-local mutation never mutates
caller-owned aggregate storage; cleanup at O0/O2 with sanitizers; lexical task
and transitive extern/pool rejection; both compiler suites, Clippy, both releases,
A/B/C and unchanged Rust/native cache reuse. Broad suites after source freeze.
