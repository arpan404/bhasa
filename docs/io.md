# Typed file I/O

`std/io.bh` wraps file operations in `IoResult[T]`. It keeps the existing integer
status builtins and `Result[T].error(Text)` APIs unchanged.

```bhasa
import "@std" as io

def main() -> int {
    match io.file_read_str("settings.json") {
        case io.IoResult[buffer].ok(contents) { print(contents) }
        case io.IoResult[buffer].error(error) {
            print(error.message().bytes)
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
