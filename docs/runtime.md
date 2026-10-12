# Owned data and native I/O

The native runtime is C, compiled and linked by Clang when a program uses it.
It uses checked allocation and explicit destruction, with no garbage collector
and no Rust runtime. The current executable implementation supports macOS and
Linux. Scalar code that does not use these operations does not link this runtime.

## Containers

```bhasa
def add_suffix(text: buffer) {
    append(text, "!")
}

def main() {
    var text = buffer("hello")
    var copy = text
    add_suffix(text)
    print(text) # hello!
    print(copy) # hello

    var numbers = ints()
    push(numbers, 42)
    print(get(numbers, 0))
}
```

Locals and temporaries are freed automatically, including early returns and loop
iterations. Assignment from an existing container copies its contents. Functions
borrow container arguments and can mutate their contents; they cannot rebind a
borrowed parameter. A returned borrowed value becomes an independent copy.
Containers use no reference counting or tracing, and expose no pointer
arithmetic. Copy elision is not yet optimized, so large value copies have a
real cost. Records and active enum payloads copy their nested containers and
aggregate structure independently; network handle fields follow the resource
rules below.

| Operation | Result |
| --- | --- |
| `buffer()` / `buffer("text")` | Empty byte buffer or a copy of static UTF-8 text |
| `ints()` | Empty integer vector |
| `len(data)` | Element count |
| `get(data, index)` | Element at a checked index |
| `set(data, index, value)` | Replace an existing element |
| `push(data, value)` | Append an element |
| `append(bytes, other)` | Append a buffer or static string |
| `slice(bytes, start, count)` | Independent copy of a checked byte range |
| `same(bytes, other)` | Exact byte comparison with buffer or static string |
| `to_buffer(number)` | Decimal integer text |

Byte values must be in `0..255`. Invalid indices, allocation failure and size
overflow stop the program with a runtime error. Buffers may contain NUL or
invalid UTF-8; byte indexing does not validate Unicode. `str` literals remain
immutable, UTF-8 and NUL-free.

## Files and console streams

```bhasa
def main() -> int {
    var data = buffer()
    var status = read_file("input.txt", data)
    if status != 0 { return status }
    return write_file("output.txt", data)
}
```

`read_file(path, destination)` replaces the destination only on success.
`write_file(path, contents)` creates or truncates a file; a write error can leave
partial output. Paths accept static strings or buffers and reject embedded NUL.
Reads and writes preserve arbitrary bytes.

`read_stdin(destination)` reads until EOF. `write_stdout(contents)` and
`write_stderr(contents)` write and flush bytes without adding a newline.
These functions return zero on success or a positive OS error code. The checker
rejects discarding a status as a standalone expression, but assigning it to an
unused variable is currently permitted. These builtins retain integer status
results. Applications and libraries can use concrete or generic `Result[T]`
enums with exhaustive `match`; [backend libraries](backend.md) use these typed
results. Error propagation is explicit.

## Arguments and subprocesses

`arg_count()` includes the executable name. `arg(index)` returns an owned copy,
with `arg(0)` naming the executable. Use `bhasa run program.bh -- argument ...`
to pass program arguments; flags after `--` belong to the program.

```bhasa
def main() -> int {
    var arguments = buffer()
    args_push(arguments, "/usr/bin/printf")
    args_push(arguments, "%s\n")
    args_push(arguments, "hello from a child")
    return process_run(arguments)
}
```

`args_push` appends one argument record and rejects embedded NUL. The first
record is the executable. `process_run` invokes the argument vector directly,
inherits the environment and standard streams, waits for completion, and never
implicitly invokes a shell. Shell metacharacters in arguments remain literal.
It returns the child's exit code, `128 + signal` after signal termination, or a
negative OS error for invalid input, spawn or wait failure. Output capture,
environment overrides and asynchronous process handles are not yet provided.

## Network resources and tasks

TCP handle copies duplicate owned descriptors for the same kernel endpoint;
they do not open new connections or copy the stream. TLS handle copies retain
one refcounted session with serialized I/O. The last TLS release closes and
frees that session without network waits. Explicit `tls_shutdown` is needed
when authenticated end-of-stream matters. See [TCP](network.md) and
[TLS](tls.md) for deadlines, statuses and ownership details.

`spawn` starts a native thread for a global one-argument typed worker. Owned
arguments copy before scheduling; join/await return its checked result type.
`async_spawn` schedules the same worker shape on four persistent threads with a queue of 64 waiting
jobs; a full queue blocks submission. Task handles have unique lexical
ownership. `join` and `await` each consume one handle and block until the worker
returns. Void-result tasks can be consumed as statements. Worker call graphs cannot reach `async_spawn`, `await` or extern
functions. The [concurrency guide](concurrency.md) specifies the scoped rules.
There are no coroutines, event-loop async I/O or arbitrary shared aggregate references.
