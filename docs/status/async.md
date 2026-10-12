# Bounded task scheduler

`runtime/concurrency.c` adds a fixed worker pool alongside the existing native
thread-per-task implementation. The new ABI is:

```c
void *bhasa_rt_async_spawn_i64(int64_t (*worker)(int64_t), int64_t argument);
int64_t bhasa_rt_await_i64(void *handle);
```

Source integration uses `async_spawn(worker(value))` and `await(task)`.
Both task kinds share the existing opaque linear handle contract. `join` and
`await` may consume either kind exactly once. Existing nested `spawn`/`join`
keeps its thread-per-task behavior.

The pool starts lazily through `pthread_once` and creates four persistent
workers. A FIFO queue holds at most 64 waiting jobs. When full, submission
blocks until a worker removes a queued job. These limits cover pooled execution
and the waiting queue. Completed handles remain caller-owned until consumed,
and retained handles are not bounded by queue capacity. No task is detached and
there is no garbage collector.

Queue state and pooled task completion/result publication use one mutex.
Condition waits loop on their predicates. A worker stops accessing a task before
an awaiting caller may free it. `atexit` stops submissions, drains queued work
and joins all pool workers. Ordinary Bhasa task checking requires every handle
to be consumed before its lexical scope ends.

To avoid queue-full and all-workers-waiting deadlocks, pooled submission and
`await` are forbidden from any Bhasa task worker, including through helpers,
methods, match arms and generic specializations. Both frontends enforce a
transitive call-graph check.
Thread-local runtime guards reject the same operations from native or pooled
worker entry points. `join` on a pooled handle has the same runtime guard;
legacy nested native `spawn`/`join` is allowed. Runtime allocation or pthread
failure remains fatal, matching the existing task runtime.

This is a bounded task executor. It does not add coroutine functions, an I/O
reactor, cancellation, channels or automatic continuation suspension. A blocked
job occupies its worker; four blocked jobs stop queued work until one returns.

## Verification

The owned C fixture is `tests/fixtures/async/runtime.c`, driven by
`tests/async_runtime.rs`. It intercepts runtime allocation and pthread calls to
check exact worker counts and cleanup. It gates all four workers, fills every
queue slot, and observes an actual condition wait by the next producer before
releasing the gate. Four concurrent native producers then submit and consume
6,144 jobs. A separate mode races cold pool initialization. The fixture also
checks mixed join/await consumption, legacy nesting, runtime guard failures and
that every runtime-created thread has been joined before the final exit check.

Direct strict-C11 builds passed at O0 and O2 with AddressSanitizer and
UndefinedBehaviorSanitizer, including zero tracked live task allocations. A
ThreadSanitizer run also passed. Normal operation and concurrent cold-start
passed under all three sanitizer configurations. Root's focused reference
Cargo run passed the runtime test and all three language tests, recorded in
`build/verify-next-focused.log`.

`tests/async_scheduler.rs` adds source-level linear ownership, worker call-graph,
runtime marker and native queue/result checks for the Rust frontend and, when
enabled, the Bhasa frontend. Source checks include 96 outstanding tasks, cross
join/await use, builtin-named methods and scheduling within caller-evaluated
worker arguments. The latest generic additions passed direct reference CLI
checks: six transitive helper rejection cases and an explicitly specialized
integer worker. Final broad reference (350 tests) and full-hybrid (358 tests)
verification passed, including the runtime test and all three source-level
scheduler groups in both configurations. Logs are
`build/verify-next-reference.log` and `build/verify-next-hybrid-frontend.log`.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test async_runtime --test async_scheduler -- --test-threads=1
```
