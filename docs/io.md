# Typed file I/O

`std/io.bh` wraps file operations in `IoResult[T]`. It keeps the existing integer
status builtins and `Result[T].error(Text)` APIs unchanged.

```bhasa
import "@std" as io

def main() -> int {
    match io.file_read_str("settings.json") {
        case io.IoResult[buffer].ok(contents) { print(contents) }
        case io.IoResult[buffer].error(error) {
            print(error.message().bytes())
            print(error.code())
            return 1
        }
    }
    return 0
}
```

For early returns from a compatible result function, write
`var contents = try io.file_read_str(path)`. The [error guide](errors.md) defines
propagation and its task/cleanup rules.

The example assumes a local `std` dependency in `bhasa.toml`, as described in
the [package guide](packages.md). The standard library exports these declarations
from `io.bh`. A project with a copied adjacent module can instead use
`import "io.bh" as io`.

| Function | Return value |
| --- | --- |
| `file_read(path: buffer)` | `IoResult[buffer]` |
| `file_read_str(path: str)` | `IoResult[buffer]` |
| `file_write(path: buffer, contents: buffer)` | `IoResult[int]` |
| `file_write_str(path: str, contents: buffer)` | `IoResult[int]` |

`ok` contains independently owned file bytes for a read, or the full contents
length after a successful write and close. Empty reads and zero-byte writes are
successful. File contents are bytes and need not be UTF-8. Buffer paths may also
contain non-UTF-8 bytes; embedded NUL returns an error.

`error` contains an owned `Error`. Its private fields have public getters:

- `domain()` returns `ErrorDomain.os`.
- `code()` preserves the positive native status. Numeric errno values vary by
  platform; this API does not assign portable numeric meanings.
- `operation()` returns `"read_file"` or `"write_file"` for both path forms.
- `message()` returns an independent `Text` copy, containing `"file read failed"`
  or `"file write failed"`. It does not expose or decode the path.

The native read destination changes only after a successful read and close.
The typed read wrapper owns that temporary destination and returns no partial
contents on failure. Writes create or truncate a file and can leave partial
output even when they return an error. A successful close does not promise
`fsync` durability, atomic replacement or rollback. There is no automatic retry.

Errors are ordinary owned values with deterministic cleanup. Returning or copying
an Error preserves independent message storage. Allocation failure and invalid
runtime pointers still stop the process; this module does not make every runtime
failure recoverable. Standard streams, process execution and networking retain
their existing APIs in this increment.

## Streaming regular files

Streaming operations use an owned `file_handle` and return the same
`IoResult[T]` error values. Consume each result with `match`, a compatible
initializer `try`, or a return. Leaving a stored result unhandled is rejected
by the pending-result check.

| Function | Success payload |
| --- | --- |
| `file_stream_open(path: buffer, mode: int)` | `file_handle` |
| `file_stream_open_str(path: str, mode: int)` | `file_handle` |
| `file_stream_read(handle: file_handle, limit: int)` | owned `buffer` |
| `file_stream_write(handle: file_handle, bytes: buffer)` | complete input length |
| `file_stream_seek(handle: file_handle, offset: int)` | requested absolute offset |
| `file_stream_position(handle: file_handle)` | current byte offset |
| `file_stream_flush(handle: file_handle)` | zero |
| `file_stream_close(handle: file_handle)` | zero |

Mode 0 opens an existing regular file for reading; 1 creates or truncates for
writing; 2 creates or appends for writing; 3 opens an existing regular file for
reading and writing. Created permissions use 0666 subject to the process umask.
An open directory reports EISDIR and other unsupported file kinds report
ENOTSUP when detected after open; earlier OS open failures keep their code.
Embedded NUL buffer paths report an error. Descriptors use CLOEXEC.

Copies of a file handle retain the same shared descriptor and cursor. Handle
copying allocates nothing and performs no OS operation. Failed handles preserve
their original open status. Scope cleanup releases each retained reference;
the last release closes a still-open descriptor. `file_handle` and aggregates
containing it are restricted to synchronous single-thread use. File operations
and file-containing worker arguments/results are rejected for both task and
worker-pool submissions, including transitive helper/method calls. Explicit
`owned` parameter modes and `move` do not transfer file handles in this slice.

The wrappers borrow their handle argument. Explicit close invalidates all
aliases, even when close reports an error. It remembers that close result;
repeated close never retries the consumed descriptor. After a successful close,
`file_status` reports EBADF on the aliases, while repeated close remains a
successful no-op. Failed-open handles retain their open error. Concurrent cursor
or lifetime operations are unsupported by this non-atomic file reference count.

Read limits range from 0 through 1048576 bytes; writes accept at most 1048576
bytes per call. A successful positive-limit read returning empty bytes means
EOF. Limit zero returns empty bytes without performing a read. Native read
errors preserve the destination but can advance the shared cursor; the typed
wrapper returns an error without exposing a partial destination. Writes retry
short writes and can leave a prefix on failure. Interrupted read/write/seek/flush
operations retry, but synchronous regular-file I/O has no wall-clock deadline.
Seek uses a nonnegative absolute byte offset. Flush requests `fsync`, subject to
the filesystem and device guarantees; it does not promise atomic replacement.

Errors keep the exact positive native code. `operation()` reports `file_open`,
`file_read_chunk`, `file_write_chunk`, `file_seek`, `file_position`, `file_flush`
or `file_close`. Their message getter returns an independent owned Text copy.
The implementation consumes a descriptor on a reported close failure and does
not retry it. Portable real-device close/EINTR behavior, other platforms and
filesystem durability require their own execution gates.

The read-only [streaming example](../examples/io-stream/main.bh) reads 64 KiB
chunks using typed propagation and explicitly closes the input:

```sh
bhasa run examples/io-stream/main.bh -- INPUT
```

It is a package with lowercase `bhasa.toml` and a local std dependency. It prints
each byte chunk and the total byte count. Allocation failure, invalid runtime
storage and reference-count overflow remain process-fatal errors. There is no
garbage collector.
