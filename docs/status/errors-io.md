# Typed file errors

The typed I/O draft has been applied as `std/io.bh`, exported by `std/lib.bh`,
with `tests/typed_io.rs` and the [I/O guide](../io.md). The coordinator is running
the compiler integration gates. This report records the completed draft probes;
it does not claim final propagation, frontend parity or release acceptance.

## API and ownership

`IoResult[T]` has `ok(value: T)` and `error(error: Error)` variants. It preserves
the existing `Result[T].error(Text)` API and every integer-status native entry
point. The four wrappers are:

| Wrapper | Success payload |
| --- | --- |
| `file_read(path: buffer)` | Owned `buffer` |
| `file_read_str(path: str)` | Owned `buffer` |
| `file_write(path: buffer, contents: buffer)` | Full contents length as `int` |
| `file_write_str(path: str, contents: buffer)` | Full contents length as `int` |

`Error` privately stores `ErrorDomain.os`, the exact positive native status, an
operation string and an owned `Text` message. Its public getters are `domain`,
`code`, `operation` and `message`. The factory remains private to `io.bh`.
Returned messages are independent copies, so editing their public `Text.bytes`
field cannot alter the stored error. Both path forms report `read_file` or
`write_file`; messages are fixed valid UTF-8 and do not decode or expose paths.

Read wrappers own their temporary destination. The native operation replaces
that destination only after reading and closing succeed, so errors return no
partial bytes. Write success reports the input length only after the complete
write and successful close. A write error can follow truncation or a partial
write. There is no rollback, retry, atomic replacement or `fsync` guarantee.
Buffer paths retain native byte semantics; embedded NUL is rejected. Filesystem
acceptance of non-UTF-8 names remains platform-dependent.

Allocation failure and invalid runtime pointers remain fatal. Standard streams,
networking and process execution retain their existing APIs. Compiler propagation
and bare result-discard checks are separate parts of this increment. Assigning a
failure result does not establish that it is eventually handled.

## Draft verification

Before promotion, the ignored `build/errors-draft/io` directory held the module,
a copied `text.bh`, test draft and guide. The existing release compiler accepted
the valid imported program and rejected ten privacy/type cases. Private field
reads and writes, record construction, the error factory and nested message
storage all produced privacy diagnostics. Wrong path, contents and error payload
types were rejected too.

The release compiler emitted LLVM for an independent acceptance program. Its
aggregate allocation/free calls and the native core/text runtime were instrumented
with one counted allocator. Strict C11 runtime builds and native execution passed
at O0 and O2 with ASan/UBSan. Every run ended with zero counted allocations and
no open FILE handles. The program covers:

- A 10 KiB binary round trip, empty files and string/buffer paths.
- Native status comparison for missing files, directory reads, NUL paths and
  non-UTF-8 paths. The tested filesystem rejected the raw byte filename; the
  typed wrapper preserved that status.
- Error domain, code, operation and message values; independent copied messages;
  repeated error creation, matching, copying and cleanup.
- Six injected scenarios: partial read, read-close failure, partial write,
  write-close/flush failure, and combined operation/close failures for reads
  and writes. Read destinations remain unchanged, failed writes can leave the
  observed prefix, and the first operation error takes precedence.

Close-failure injection returns an error after actually closing the FILE handle.
It tests status propagation and cleanup without pretending to reproduce a failing
storage device. These probes ran on macOS ARM64 and used exhaustive matching,
because the existing release compiler predated the new propagation syntax.

The applied propagation code was also reviewed read-only. Both emitters acquire
the selected payload before releasing its source enum, then clean up caller
locals on the error return. Invalid tags are checked before payload access. The
coordinator owns execution checks of those paths and the combined gates.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test typed_io -- --test-threads=1
```

No Cargo or Git operations were run by the I/O lane. Focused coordinator results,
full suites, refreshed releases and self-build/cache evidence remain pending in
this report.
