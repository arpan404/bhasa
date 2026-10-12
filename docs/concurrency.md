# Native tasks and bounded scheduling

Bhasa runs typed functions either on a new operating-system thread with
`spawn`, or on a fixed worker pool with `async_spawn`. Both return a linear task
handle. Joining or awaiting waits for completion, returns the worker's checked
result type and frees the task allocation. There is no garbage collector.

```bhasa
def square(value: int) -> int {
    return value * value
}

def main() {
    var first = spawn(square(6))
    var second = spawn(square(8))
    var a = join(first)
    var b = join(second)
    print(a + b)
}
```

Both tasks may execute concurrently. The program prints `100` after both finish.
Run the checked example with `bhasa run examples/concurrency/tasks.bh`.

Workers require one non-task, non-void parameter and any supported non-task
result. A void result is consumed with a join/await statement. The worker must
be a global Bhasa definition; explicitly specialized generic functions work.
Direct method spawning and function values are unsupported.
Arguments evaluate once in the calling thread before scheduling. Owned arguments
acquire independent copies before submission, so worker mutation of aggregate
storage cannot change the caller's value. Results transfer owned storage back
when consumed. Network handle copies retain their existing shared endpoint or
serialized TLS-session semantics. See `examples/concurrency/typed.bh`.
Workers may
call other Bhasa functions, create local containers, and use legacy `spawn`/`join`
for children that finish before they return. The compiler rejects worker call
graphs reaching `extern def` because those imports have no declared thread-safety
contract.

## Bounded pool

```bhasa
def square(value: int) -> int { return value * value }

def main() {
    var first = async_spawn(square(6))
    var second = async_spawn(square(8))
    var a = await(first)
    var b = await(second)
    print(a + b)
}
```

This also prints `100`. Run the complete
[async example](../examples/async/main.bh) with:

```sh
bhasa run examples/async/main.bh
```

The first pooled submission starts four persistent worker threads. At most four
pooled jobs run at once, and a FIFO queue holds at most 64 waiting jobs. When the
queue is full, `async_spawn` blocks its caller until a worker takes a queued job.
The pool reuses those threads for later jobs and joins them at normal process
shutdown. The current capacities are fixed.

The queue limit does not count completed handles retained by the caller. Those
allocations remain owned until consumed. `join` and `await` can consume either
kind of handle, so `join` works with a pooled task and `await` works with a task
created by `spawn`.

Neither `async_spawn` nor `await` may be reached from any task worker. The checker
follows helper functions, methods, match arms and generic specializations to
enforce this restriction. It prevents workers from blocking on a full pool or
waiting for queued jobs when every worker is occupied. Legacy nested
`spawn`/`join` remains allowed. Scheduling in a worker argument expression runs
in the caller and follows the caller's restrictions.

`await` blocks the calling thread; it does not suspend a coroutine. A blocking
I/O call also occupies its worker. If all four pooled jobs block, queued work
waits for one to return. This scheduler does not add async functions, an I/O
reactor, cancellation or detached tasks.

## Task ownership

Create a task as the complete initializer of a variable. Consume it with a
complete `join(job)` or `await(job)` variable initializer or expression statement
in the same lexical block. Every task must be consumed before that block exits or the function
returns. The compiler rejects copying, rebinding, shadowing, passing, returning,
or joining a task twice. A branch cannot join a task created outside that branch.
Create and join loop-local tasks within each iteration.

```bhasa
var job = spawn(square(6))
var value = join(job)
print(value)
```

A compatible result can also be consumed by `var value = try join(job)` or
`var value = try await(job)`. Propagation rejects any other live task because its
error return would leave that task unconsumed. See [errors](errors.md).

Nested expressions such as `print(join(job))` and `print(await(job))` are rejected. Keeping
consumption at statement boundaries makes control-flow ownership explicit until
the compiler implements richer ownership analysis.

The native runtime uses POSIX threads on macOS and Linux. Allocation, thread
creation, synchronization or join failures stop the process with a diagnostic.
Runtime guards also reject pooled scheduling and awaiting from worker entry
points. Typed task frames use checked native allocations and deterministic
cleanup. Arbitrary shared aggregate references and task-handle transfers remain
unsupported.

## Typed bounded channels

Import `@std` to create `channel[T](capacity)`. Capacity must be positive. The
returned `Channel[T]` owns a sender and receiver; `sender()` and `receiver()`
return additional owned endpoints sharing the same bounded FIFO queue.

```bhasa
import "@std" as ch

def main() {
    let pair = ch.channel[buffer](2)
    let sender = pair.sender()
    let receiver = pair.receiver()
    var message = buffer("hello")

    match sender.try_send(message) {
        case ch.SendResult[buffer].ok(status) {}
        case ch.SendResult[buffer].error(failure) { print("send failed") }
    }
    set(message, 0, 88)
    match receiver.try_recv() {
        case ch.ReceiveResult[buffer].ok(value) { print(value) }
        case ch.ReceiveResult[buffer].error(reason) { print("receive failed") }
    }
    sender.close()
}
```

This prints `hello`. Sending acquires an independent payload copy, so caller
mutation does not change queued data. A successful receive transfers the queued
value to the receiver. A failed send returns that independent value in
`SendFailure[T].closed(value)` or `SendFailure[T].full(value)`; it never consumes
the caller's original value. Both send and receive results must be handled.

| Endpoint operation | Result | Behavior |
| --- | --- | --- |
| `sender.send(value)` | `SendResult[T]` | Waits for space or closure. |
| `sender.try_send(value)` | `SendResult[T]` | Returns immediately; full and closed queues return the value. |
| `receiver.recv()` | `ReceiveResult[T]` | Waits for a value or closure. |
| `receiver.try_recv()` | `ReceiveResult[T]` | Returns immediately; errors are `ReceiveError.empty` or `ReceiveError.closed`. |
| `sender.close()` / `receiver.close()` | `void` | Closes the shared queue and wakes waiting callers. |

Closing prevents further sends while allowing receivers to drain queued values.
Dropping the last sender also closes the queue. Dropping the last receiver closes
it and destroys unread values exactly once. Copies of endpoint variables retain
distinct owners; scope exit releases each owner, and the last owner frees queue
storage. No garbage collector or detached cleanup worker is involved.

Endpoints can be passed to typed task workers. Workers may use `try_send`,
`try_recv` and `close`; blocking `send` and `recv` are forbidden in every task
worker, including through helper functions and methods. The compiler checks the
resolved call graph again before emission, and the runtime also enforces the
worker restriction. The complete [channel example](../examples/channels/main.bh)
uses a receiver task.

Payloads include scalars, owned buffers/ints and concrete records/enums whose
contents satisfy the transfer rules. TCP/TLS handles retain their documented
shared-resource behavior. Tasks, regular-file handles and nested channel
endpoints are refused, including when hidden in aggregate fields or aliases.
Source code cannot construct endpoints or access their private storage.

This first channel runtime admits the shared target layer's supported 64-bit
macOS and Linux targets. Execution is verified on ARM64 macOS; Linux execution
remains unverified. `select`, channel deadlines, cancellation and coroutine-based waiting
remain future work. Invalid capacity, allocation and synchronization failures are
fatal runtime traps. The compiler currently registers the repository's canonical
standard library; relocating an installed library needs a separate registration
contract.

The [async implementation report](status/async.md) records queue saturation,
concurrent execution, allocation cleanup, thread shutdown and sanitizer checks.
