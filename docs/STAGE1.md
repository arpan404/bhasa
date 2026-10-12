# Runtime and self-hosting implementation contract

This work extends the scalar bootstrap using a hybrid compiler. The Rust host
retains CLI, platform and toolchain integration. Bhasa-written frontend modules
are integrated after comparison tests pass. A separate experimental Bhasa
compiler seed tests independent rebuilding without replacing the supported host.
LLVM and native platform tools remain build dependencies.
This document preserves the initial runtime-extension contract. Later work adds
[records and methods](records.md), [local packages](packages.md), [synchronous TCP](network.md)
and [editor tooling](tooling.md). General borrowing, generics, portable async
I/O and Bhasa GPU kernel compilation remain future work. The
[integration report](status/integration.md) records completed verification of
the methods and explicit-bind milestone.

## Owned compiler data

Add `buffer` for owned bytes and `ints` for owned signed 64-bit vectors. No raw
pointers are exposed. Locals own their allocations and the compiler destroys
them at scope exit, including early returns. Binding or assigning from an
existing value makes a deep copy. Function arguments borrow synchronously;
owned parameters cannot be rebound, but their contents can be changed through
checked operations. Returning a borrowed value makes an owned copy. Temporaries
are destroyed after their use. This is an initial value-semantics design, not a
claim of complete Rust borrowing or optimal copy elimination.

Static `str` remains immutable literal text. Dynamic file contents use `buffer`,
which can contain NUL and arbitrary bytes. Bounds, growth and allocation are
checked by a small C runtime with malloc/free and no collector.

| Builtin | Types and result |
| --- | --- |
| `buffer()` / `buffer(text)` | no argument or `str` -> buffer |
| `ints()` | -> ints |
| `len(value)` | buffer/ints -> int |
| `get(value, index)` | buffer/ints, int -> int |
| `set(value, index, element)` | buffer/ints, int, int -> void |
| `push(value, element)` | buffer/ints, int -> void |
| `append(destination, source)` | buffer, buffer/str -> void |
| `slice(value, start, count)` | buffer, int, int -> buffer |
| `same(a, b)` | buffer, buffer/str -> bool |
| `to_buffer(value)` | int -> buffer, decimal spelling |
| `arg_count()` / `arg(index)` | -> int / int -> buffer |
| `read_file(path, destination)` | buffer/str, buffer -> int |
| `write_file(path, contents)` | buffer/str, buffer -> int |
| `read_stdin(destination)` | buffer -> int |
| `write_stdout(contents)` / `write_stderr(contents)` | buffer -> int |
| `args_push(arguments, value)` | buffer, buffer/str -> void |
| `process_run(arguments)` | buffer -> int |

`print` additionally accepts buffer and writes its bytes followed by a newline.
I/O returns zero on success or a positive errno-style code on failure; callers
must use these status values. File reads replace the destination only on success.
Paths reject embedded NUL. `args_push` builds NUL-separated argument records;
arguments reject embedded NUL. `process_run` invokes an argument vector without
a shell and returns the exit status, or a negative OS error if spawning fails.
Arguments include argv[0]. `arg(0)` is the program path. Runtime startup receives
argc/argv through the generated C main wrapper.

## Structured scalar concurrency

`var job = spawn(worker(value))` calls a defined Bhasa `int -> int` function on
a native thread with a copied integer. `join(job)` consumes the unique handle
and returns the integer result. Initial static rules require a task to be joined
in the same lexical block, before return or block exit. Handles cannot be copied,
reassigned, passed to functions, returned, or consumed within conditional
expressions. The checker rejects worker call graphs reaching trusted extern
functions, whose hidden state has no thread-safety contract. This first interface
does not promise channels, cancellation, async I/O, or arbitrary shared values.

Record methods participate in the worker call graph, including forward and
recursive method calls. A method cannot hide an extern call from this check.
The direct spawn form still accepts a global `int -> int` worker, not a method.

## Record methods

Methods belong inside `struct` declarations and start with an untyped `self`
parameter. The compiler infers the enclosing record type. Other parameters and
returns follow ordinary function rules. `self` borrows mutable record storage;
methods may change its fields but cannot rebind the receiver. Field lookup is
explicit through `self.field`.

Evaluate the receiver once, then explicit arguments left to right. Keep a
temporary receiver alive through the call and acquisition of returned owned
values. Apply existing record/field borrow overlap checks with the receiver as
the first borrowed argument. Method names may repeat across records, but cannot
collide with fields or other methods in the same record. Static methods,
overloads, inheritance, dynamic dispatch and method values remain unsupported.

## Explicit TCP bind addresses

`tcp_listen(port)` binds IPv4 loopback. `tcp_listen_on(host, port)` binds an
explicit numeric IPv4 or IPv6 address supplied as `str` or `buffer`. Wildcards
such as `0.0.0.0` and `::` select all local addresses for their family. Use
`127.0.0.1` or `::1` for local-only listeners. Ports range from 0 to 65535; zero
requests an available port.

The operation does not resolve host names. Invalid ports, malformed addresses,
empty/binary/NUL-containing hosts and OS bind failures return owned error
handles. Call `tcp_listener_status` before using the listener. The native ABI
uses `bhasa_rt_tcp_listen_on_str` and `bhasa_rt_tcp_listen_on` and preserves the
existing clone/free rules. [Networking](network.md) specifies the complete
contract. This addition does not provide TLS, async I/O or timeouts.

## GPU host interface

`gpu_available()` returns bool. `gpu_dispatch(source, entry, data)` accepts
buffer/str source and entry, and a mutable buffer of bytes; it returns a status.
Explicit host backends cover Metal source, CUDA source and Vulkan SPIR-V. Their
source and kernel interfaces are documented separately in `gpu.md`. They wait
for completion before returning, so
Bhasa cannot race host access with a pending dispatch. Missing hardware, invalid
kernels and execution errors are explicit. No CPU fallback is permitted.
These are trusted native GPU interfaces, not yet compilation of Bhasa kernels.

## Implementation ownership

Integration owns shared AST, builtin signatures, parser, LLVM generation,
driver and language integration tests. Runtime worker owns `runtime/core.c`,
`runtime/core.h` and runtime C tests. Concurrency worker owns semantic checking,
`runtime/concurrency.c`, its header and concurrency tests/examples. GPU worker
owns `runtime/metal.m`, its header and GPU tests/examples. Do not change another
worker's files. The stage-0 ownership table is superseded for this work.

Runtime C symbols use the reserved `bhasa_rt_` prefix. Pointer-valued functions
use opaque buffer/ints/task pointers. Integer values and status codes use int64_t.
Boolean runtime results use int64_t and are converted to LLVM i1 by the backend.
