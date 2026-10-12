# Network lane

## Explicit bind addresses, complete and frozen

The new ABI is fixed for integration:

```c
bhasa_tcp_listener *bhasa_rt_tcp_listen_on(const bhasa_buffer *host, int64_t port);
bhasa_tcp_listener *bhasa_rt_tcp_listen_on_str(const char *host, int64_t port);
```

Both implement `tcp_listen_on(host: buffer/str, port: int) -> tcp_listener`.
They parse numeric IPv4/IPv6 literals with `inet_pton`, without DNS. Empty,
malformed, hostname, bracketed, scoped and NUL/binary hosts return an allocated
`EINVAL` handle; ports are 0 through 65535. Wildcards `0.0.0.0` and `::` are
accepted explicitly. The buffer overload borrows its host only during the call.
The existing `tcp_listen(port)` stays IPv4 loopback. Existing status/local-port,
clone/free, runtime markers and linkage apply unchanged. No parser/type change
is needed; root owns builtin mapping and the frontend lane mirrors it.
IPv6 listeners retain the OS's `IPV6_V6ONLY` default. Both variants share the
existing socket configuration and cleanup path. The buffer variant uses a
bounded stack copy and rejects oversized text before scanning/copying it.

Implementation, header and native test sources are frozen for integration.
Root registered both overloads in `src/builtins.rs`, and the focused reference
compiler fixture now passes. The Bhasa frontend lane owns its matching
registration. No new shared-source changes are needed from this lane. Root owns
combined compiler verification.

Passed on macOS arm64 with Apple Clang 21:

```text
CARGO_BUILD_JOBS=2 cargo test --test network_runtime native_tcp_loopback_errors_and_ownership -- --test-threads=1 --nocapture
1 passed; 0 failed; 0 ignored; 1 filtered out; finished in 4.37s
```

This focused run compiled strict C11 with warnings denied, ASan and UBSan at O0
and O2. Both string and buffer overloads exchanged binary data over real IPv4
and IPv6 loopback on ephemeral ports. IPv6 was available, with no skip on this
host. The test reports genuine IPv6 absence explicitly and treats denied
loopback permission as failure. Tool escalation enabled local loopback access.

New checks cover malformed/name/bracket/zone/whitespace/binary/NUL hosts,
inclusive port bounds, no DNS or socket creation for rejected input, bounded
nonterminated host buffers, borrowed-buffer preservation, clone/free, assigned
local ports, CLOEXEC, allocation failures and OS-error cleanup. Wildcards and
non-loopback numeric addresses are checked at an intercepted bind that returns
EACCES before any OS bind; no externally accessible listener was started.
Existing loopback, stream, failure, EOF and resource modes also passed.

NET-SAFE-1 is fixed and its regression passes. The first focused run exposed
Darwin `inet_pton` accepting and discarding a zone suffix. The runtime now
validates numeric-address ASCII characters before parsing, and the repeated
focused run passed `::1%lo0` and `::1%1` rejection for both overloads, with no
socket creation or DNS calls. Buffer
lengths are checked before scanning/copying. Linux execution remains unverified.
`rustfmt --check tests/network_runtime.rs` passed. No broad suite or isolated
Cargo target rebuild was run.

After root's builtin integration, the focused compiled regression also passed:

```text
CARGO_BUILD_JOBS=2 cargo test --test network_runtime compiled_tcp_exchange_and_resource_copies -- --test-threads=1 --nocapture
1 passed; 0 failed; 0 ignored; 1 filtered out; finished in 3.72s
```

This runs three Bhasa fixtures in debug and release modes. The new
`tests/fixtures/network/listen_on.bh` exchanges binary request/response bytes
through both overloads on ephemeral IPv4 loopback, mutates the borrowed host
buffer after construction, and checks errors for hostname, scoped IPv6 and
invalid-port inputs. It prints `TCP explicit bind OK`. The existing worker
loopback and resource-copy fixtures also passed. This run used the reference
compiler; hybrid verification stays with root. Loopback permission was enabled
through tool escalation. Runtime/native-harness sources are unchanged since
the sanitizer pass, so that passing suite was not repeated.

Frozen SHA-256 values:

```text
ce4a6851b9f9a11a79cc2d332b4b8e879ca47ca78d711251d4dfb299bf5abcce  runtime/net.c
63477a5d1e09a9ec96fec1c05c70be3d4d8e2a095a906539e72d072ced823b39  runtime/net.h
eabb86db0204cc22c0e9a1343186098dac9c6d429a4a0e8d89f4f198e7e0f4d9  tests/network_runtime.rs
12a9b82ace6efa3fe422b46c9d6652d399e1a63087ac80b85e9ffcd0ff54af7b  tests/fixtures/network/runtime.c
9091afb3e0a143dac432398b39a040113a13ac224b90d7c3cc5974a2af1f906a  tests/fixtures/network/listen_on.bh
```

## Completed baseline

The native adapter and public header are implemented. Strict C11 compilation,
native O0/O2 sanitizer tests and the compiled Bhasa loopback example pass on
macOS arm64. This file records the ABI and integration evidence.

## Compiler integration contract

Add owned opaque types `tcp_listener` and `tcp_socket`. Arguments borrow them;
binding, assignment and returned borrowed values clone with `dup` semantics.
Each clone owns a separate descriptor for the same kernel endpoint. Copies do
not create independent connections. Scope exit and temporary cleanup call the
matching free function. No raw descriptors or pointers appear in Bhasa source.

| Bhasa builtin | Result | Native symbol |
| --- | --- | --- |
| `tcp_listen(port: int)` | tcp_listener | `bhasa_rt_tcp_listen` |
| `tcp_listen_on(host: buffer, port: int)` | tcp_listener | `bhasa_rt_tcp_listen_on` |
| `tcp_listen_on(host: str, port: int)` | tcp_listener | `bhasa_rt_tcp_listen_on_str` |
| `tcp_connect(host: buffer, port: int)` | tcp_socket | `bhasa_rt_tcp_connect` |
| `tcp_connect(host: str, port: int)` | tcp_socket | `bhasa_rt_tcp_connect_str` |
| `tcp_accept(listener: tcp_listener)` | tcp_socket | `bhasa_rt_tcp_accept` |
| `tcp_listener_status(listener: tcp_listener)` | int | `bhasa_rt_tcp_listener_status` |
| `tcp_socket_status(socket: tcp_socket)` | int | `bhasa_rt_tcp_socket_status` |
| `tcp_local_port(listener: tcp_listener)` | int | `bhasa_rt_tcp_local_port` |
| `tcp_read(socket: tcp_socket, destination: buffer, limit: int)` | int | `bhasa_rt_tcp_read` |
| `tcp_write(socket: tcp_socket, contents: buffer)` | int | `bhasa_rt_tcp_write` |

The owned operations are `bhasa_rt_tcp_listener_clone`,
`bhasa_rt_tcp_listener_free`, `bhasa_rt_tcp_socket_clone`, and
`bhasa_rt_tcp_socket_free`. All scalar arguments and results use `int64_t`.
All resource and buffer values use opaque LLVM pointers. Exact C declarations
are in `runtime/net.h`, which includes `core.h`.

Emit `; bhasa runtime: net` plus the existing core marker for these operations
and for resource clone/free. The executable driver must embed and compile
`runtime/net.c` and `runtime/net.h` beside `core.c` and `core.h`. Networking adds
no link libraries beyond those needed by core. The core SIGPIPE repair needs
`-pthread` for older Linux glibc, as recorded in
[runtime-safety.md](runtime-safety.md). Other targets must reject executable
linking or only emit unlinked objects. The parser recognizes `tcp_listener` and
`tcp_socket` as type names. They can remain identifier tokens in both lexers.

Require use of `tcp_read` and `tcp_write` return statuses like file I/O. A failed
constructor, accept or clone returns a newly allocated error handle, never a
null pointer. The status getters expose that handle's fixed construction status.
Handle-memory allocation failure stops with a runtime error, consistent with core
owned allocations. Normal OS errors are positive errno codes.

`tcp_listen(0)` binds an OS-assigned port on IPv4 loopback, `127.0.0.1`.
`tcp_local_port` returns its assigned port or zero for an error handle. Connect
accepts ports 1 through 65535 and resolves host names. Read performs one receive,
up to the limit, replaces destination only on success, and reports EOF as success
with an empty buffer for a positive limit. Limit zero clears destination without
receiving. Write sends every byte or returns an error after possible partial
output. Every call is synchronous. No TLS, timeouts or async behavior are claimed.

## Verification

`examples/network/loopback.bh` passes compiler checking, native build and execution.
It listens on
port zero, starts an `int -> int` worker with that port, accepts one connection,
exchanges binary six-byte ping/pong messages, joins the client, and exits. Its
`serve_once` helper releases the accepted socket before `join` even on an error.
The expected stdout is `TCP loopback OK\n`. No fixed port or external service is
needed. Both the compiled debug and release examples printed the expected output
and exited zero under a 15-second external timeout with sandbox escalation for
loopback access.

Passed `clang -std=c11 -Wall -Wextra -Werror -pedantic -c runtime/net.c` on macOS
arm64 with Apple Clang 21. The native test runner passed O0/O2
AddressSanitizer + UndefinedBehaviorSanitizer checks across loopback, 1 MiB
streaming, EOF, NUL/binary bytes, listener/socket clone ownership, descriptor
and allocation ledgers, errno injection, partial sends, EINTR, ENOMEM, rejected
inputs and SIGPIPE avoidance after reset. Its five fatal-allocation modes check
listener, connect, accept and both clone handle allocations. All processes have
external timeouts, and the C harness also uses `alarm(30)`.

Loopback in the sandbox explicitly failed with EPERM. The isolated native runner
then passed in 16.05 seconds with escalation. No permission/hardware skip was
treated as success. Linux execution has not been verified on this host.

`tests/network_runtime.rs` also checks the compiled loopback example and
`tests/fixtures/network/resources.bh` in debug/release modes. The latter checks
borrowed resource returns, owned returns, reassignment cleanup and live copied
descriptors. `cargo test --test network_runtime -- --nocapture` passed with
loopback escalation: 2 passed, 0 failed, 0 ignored, in 3.92 seconds. This run
included both compiled programs at debug/release and the native O0/O2 sanitizer
harness. `rustfmt --check tests/network_runtime.rs` also passed.

Independent review found that macOS socket and accept require a later
`fcntl(FD_CLOEXEC)`. The core runtime lane has now fixed Bhasa `process_run` with
`POSIX_SPAWN_CLOEXEC_DEFAULT` and explicit inheritance of eligible standard
descriptors. Its descriptor regression failed before the fix and passed at O0/O2
with ASan/UBSan, along with standard-stream and environment inheritance checks.
The core lane's final focused suite passed in 13.35 seconds with strict C11
warnings and both sanitizers at O0/O2. Core sources are frozen for integration.
See [runtime-safety.md](runtime-safety.md) for the full evidence. Linux networking
uses atomic close-on-exec flags; Linux execution remains unverified.

The remaining macOS warning in `docs/network.md` applies to concurrent foreign
spawning that does not close unrelated descriptors by default. The network lane
did not change shared core implementation or rerun test suites for this docs
update. Root owns final combined verification.
