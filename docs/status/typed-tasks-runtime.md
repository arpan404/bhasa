# Typed task runtime

`runtime/concurrency.c` now schedules owned wrapper frames alongside the existing
scalar task ABI. Both kinds use the same four-worker pool and 64-entry FIFO queue.
A full queue blocks submission. Native `spawn` still creates one thread per task.
The runtime does not add a garbage collector or change scalar entry points.

## ABI and ownership

The new declarations in `runtime/concurrency.h` are:

```c
void *bhasa_rt_spawn_owned(void *(*worker)(void *), void *argument);
void *bhasa_rt_async_spawn_owned(void *(*worker)(void *), void *argument);
void *bhasa_rt_join_owned(void *handle);
void *bhasa_rt_await_owned(void *handle);
void *bhasa_rt_task_alloc(int64_t bytes);
void bhasa_rt_task_free(void *frame);
```

The compiler acquires a typed argument and stores it in an owned input frame
before submission. The task holds that frame until its wrapper starts. The
wrapper takes ownership, invokes the typed function, drops the input value and
frees the input frame. It returns an owned result frame, or `NULL` for a void
result. Joining consumes the task handle and transfers the result frame to the
caller, which unpacks it and frees the frame. The opaque runtime cannot replace
this generated, type-specific cleanup.

Frame allocation uses `malloc` alignment, checks negative or unrepresentable
sizes and allocation failure, and allocates one byte for size zero.
`task_free(NULL)` is harmless. Allocation and pthread failures are fatal, matching
the existing task runtime. Each handle records its scalar or owned ABI; joining
through the wrong ABI fails explicitly. The compiler remains responsible for
matching concrete input/result types and consuming each handle exactly once.

Function pointers retain their exact C types in separate union members. No
function-pointer cast or integer/pointer conversion carries an owned value.
Native joins synchronize through `pthread_join`. Pooled results are written
before completion is published under the queue mutex; waiting callers acquire
that mutex before retrieving the result. Workers stop accessing a completed
handle before an awaiting caller can free it.

The existing worker restrictions apply to both ABIs. Pooled submission and
`await` are forbidden from all task workers. Joining a pooled handle from a
worker is also forbidden. Native nested `spawn` followed by `join` remains
allowed, including scalar/owned combinations. Normal process shutdown drains
the queue and joins the pool threads. There is no cancellation, detached task
ownership, coroutine suspension, or new thread-safety contract for foreign code.

## Native evidence

`tests/fixtures/typed_tasks/runtime.c` supplies generated-wrapper-shaped workers,
counted allocations, and intercepted thread/condition operations.
`tests/typed_task_runtime.rs` drives it under external deadlines. The fixture:

- Holds all four workers and fills all 64 queue entries with mixed job kinds,
  then observes the next owned producer actually waiting for capacity.
- Starts four concurrent producers and consumes 4,096 mixed scalar/owned jobs;
  a separate process races cold pool initialization.
- Exercises native and pooled owned jobs, cross `join`/`await` consumption,
  nested native scalar/owned jobs, void results and zero-sized frame allocation.
- Tracks input/output frames and nested byte allocations, checks independent
  result contents, and verifies zero remaining allocations and all runtime
  threads joined at shutdown.
- Checks 14 fatal cases covering null inputs, frame/task allocation failures,
  ABI mismatches and mixed worker guard violations.

Direct strict C11 builds passed at O0 and O2 with `-Wall -Wextra -Werror
-pedantic`, AddressSanitizer and UndefinedBehaviorSanitizer. Normal operation,
cold startup and all 14 rejection cases passed at each optimization level.
ThreadSanitizer also passed normal operation and cold startup. The unchanged
scalar fixture in `tests/fixtures/async/runtime.c` passed normal and cold-start
operation at O0/O2 with ASan/UBSan against the updated runtime.

This evidence is from macOS ARM64. Linux source paths are not established by
these probes. Coordinator Cargo verification passes the independent runtime
fixture, unchanged scalar runtime fixture and language-level ownership/call-graph
acceptance, then complete reference/full-hybrid suites at 411/422 tests.
See integration.md for combined evidence and remaining limits.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test typed_task_runtime --test async_runtime -- --test-threads=1
```
