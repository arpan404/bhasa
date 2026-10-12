# Synchronous TCP networking

The native adapter in `runtime/net.c` provides TCP on macOS and Linux.
`tcp_listen(port)` binds to IPv4 loopback, `127.0.0.1`, reachable only from the same
machine. `tcp_listen_on(host, port)` binds an explicit numeric IPv4 or IPv6
address. Ordinary connect supports host names and numeric IPv4 or IPv6 addresses;
timed connect accepts numeric addresses only. Calls remain synchronous, with
optional deadlines for accept, connect, read and write. The optional [TLS adapter](tls.md) adds verified client/server TLS. UDP,
event-loop async I/O and cancellation are not implemented.

## API

| Bhasa operation | Result |
| --- | --- |
| `tcp_listen(port: int)` | Owned `tcp_listener`; port zero asks the OS for an available port |
| `tcp_listen_on(host: buffer/str, port: int)` | Owned `tcp_listener` bound to a numeric IPv4/IPv6 address; port zero asks the OS for an available port |
| `tcp_listener_status(listener: tcp_listener)` | Zero if construction or cloning succeeded; otherwise a positive errno code |
| `tcp_local_port(listener: tcp_listener)` | Actual bound port, or zero for an error handle |
| `tcp_connect(host: buffer/str, port: int)` | Owned `tcp_socket`; port must be 1 through 65535 |
| `tcp_accept(listener: tcp_listener)` | Owned socket for the next connection |
| `tcp_socket_status(socket: tcp_socket)` | Zero if construction or cloning succeeded; otherwise a positive errno code |
| `tcp_read(socket: tcp_socket, destination: buffer, limit: int)` | Zero or positive errno; receives up to `limit` bytes |
| `tcp_write(socket: tcp_socket, contents: buffer)` | Zero or positive errno; sends all bytes on success |
| `tcp_accept_timeout(listener, milliseconds)` | Owned socket; deadline expiry gives an `ETIMEDOUT` error handle |
| `tcp_connect_timeout(host: buffer/str, port, milliseconds)` | Owned socket; numeric IPv4/IPv6 only |
| `tcp_read_timeout(socket, destination, limit, milliseconds)` | Zero or positive errno; one receive within the deadline |
| `tcp_write_timeout(socket, contents, milliseconds)` | Zero or positive errno; sends all bytes within one deadline |
| `monotonic_millis()` | Monotonic milliseconds for elapsed-time and shared-deadline calculations |

Check a resource's status before using it. A failed constructor, accept or clone
still returns an owned handle carrying the error. It holds no open descriptor.
Status getters report the fixed creation result. They do not check whether a
peer is still connected and do not replace checking read/write results.

A successful read replaces the destination. An error leaves all destination
bytes and metadata unchanged. With a positive limit, a successful empty read
means EOF. A zero limit clears the destination without receiving, so its empty
result does not establish EOF. Negative limits, invalid ports and empty or
NUL-containing buffer host names return `EINVAL`.

Explicit listener addresses use numeric presentation syntax, such as
`127.0.0.1` or `::1`. `tcp_listen_on` does not resolve names. It rejects `localhost`,
empty text, embedded NUL or other binary bytes, surrounding whitespace, bracketed
IPv6 addresses and zone identifiers such as `%lo0`. IPv4 requires four decimal
components; abbreviated forms such as `127.1` are rejected. Listener ports must
be in `0..65535`. A valid numeric address can still fail at the OS, for example
if it is not assigned locally or the port is already in use.

`0.0.0.0` and `::` explicitly request all local addresses for their respective
families. IPv6 listeners retain the OS's `IPV6_V6ONLY` default, which determines
whether an IPv6 wildcard listener also accepts IPv4 connections. Use
`127.0.0.1` or `::1` for local-only programs. The examples and native tests bind
only loopback; tests inspect wildcard addresses at an intercepted bind call
without opening an externally accessible listener.

TCP is a byte stream. One write can require several reads, and one read can
contain bytes from several writes. Use a length prefix, delimiter or an agreed
fixed length for message boundaries. Writes preserve NUL and arbitrary binary
bytes. A write error can follow a partially sent message; retrying the entire
buffer can duplicate bytes. A successful write means the OS accepted the data,
not that the remote application processed it.

Host resolution maps `EAI_MEMORY` to `ENOMEM`, `EAI_AGAIN` to `EAGAIN`, and
`EAI_NONAME` to `EADDRNOTAVAIL`. `EAI_SYSTEM` preserves errno; other resolver
failures return `EINVAL`. Errno numbers differ between operating systems.
Allocating a resource handle can stop the program with the core runtime's
allocation error. Failed temporary allocations for host text or receive bytes
return `ENOMEM` through the normal result.

## Deadlines

The timeout range is `0..2147483647` milliseconds. Zero makes an immediate
nonblocking attempt; expiry reports positive `ETIMEDOUT`. Invalid timeouts return
`EINVAL`, directly or in an owned socket error handle. Timed read/write statuses
must be used, as with ordinary read/write. A failed timed read preserves the
destination, and a timed write can fail after sending a prefix.

Each timed operation uses one monotonic deadline, including interrupted waits
and retries after readiness races. It does not reset the timeout on partial
progress. To bound a sequence of calls, record `monotonic_millis() + timeout`
and pass the remaining time to each call. The clock is for durations and cannot
be used as a calendar timestamp.

`tcp_connect_timeout` never invokes hostname resolution. Supply a numeric
IPv4/IPv6 address such as `127.0.0.1` or `::1`, without brackets or a zone suffix.
The ordinary `tcp_connect` retains DNS support and has no application deadline.

All runtime sockets remain nonblocking at the descriptor level. Ordinary calls
wait with `poll` without a deadline; timed calls wait only for their remaining
budget. Cloned descriptors share endpoint state, so operations never temporarily
toggle blocking flags on a shared connection.

The [backend library](backend.md) uses these calls to read a bounded HTTP/1.0 or
HTTP/1.1 request under one deadline and write a response under another. The caller
closes the socket after one request. Its generic transport helpers also support
the optional [TLS package](tls.md). Chunking and keepalive remain unsupported;
the TCP examples use plain connections.

## Ownership

Each local resource owns a descriptor and a small C allocation. Scope exit,
return and temporary cleanup close that descriptor and free the allocation.
The TCP adapter has no garbage collector or reference counter. TLS uses separate
shared-session ownership described in the [TLS guide](tls.md).

Binding or assigning an existing resource duplicates its descriptor. The
descriptors refer to the same kernel listener or connection. Freeing one copy
leaves the other usable; reads through either socket copy consume the same
incoming stream. The peer receives EOF after the final copy closes. Copies do
not open new connections or copy queued bytes. Descriptor exhaustion can make a
copy an error handle, so check its status too.

Function parameters borrow resources for the duration of the call. The C ABI in
`runtime/net.h` keeps listener and socket layouts opaque. Native callers must
free each returned handle exactly once with the matching `_free` function and
must not retain borrowed buffers. `_free(NULL)` is harmless. Destruction does
not call `shutdown`, because that would also affect live descriptor copies.

Sockets use close-on-exec descriptors. On Linux, creation and accept set this
flag atomically. On macOS, creation and accept set it with `fcntl` before
returning. Bhasa's macOS `process_run` excludes descriptors outside the standard
streams from inheritance, including sockets waiting for that flag. It
explicitly preserves eligible standard input, output and error descriptors.
Concurrent foreign spawning that does not use the same policy can still inherit
a socket and delay EOF or listener release.
Both platforms duplicate descriptors atomically with `F_DUPFD_CLOEXEC`. The
[core runtime safety report](status/runtime-safety.md) records the process
inheritance fix and its regression tests.

## A complete local exchange

`examples/network/loopback.bh` binds an available loopback port, starts an
integer-argument worker as the client, exchanges six-byte messages containing
NUL and `255`, joins the worker and exits. The worker receives only the port;
each thread creates its own socket. The helper closes the accepted socket when
it returns, before the main function joins the worker.

```sh
cargo run -- run examples/network/loopback.bh
```

Successful output:

```text
TCP loopback OK
```

This finite example has no timeout. The test harness runs networking programs
under external deadlines so a regression cannot hang the suite indefinitely.

## Native verification and OS references

Run `CARGO_BUILD_JOBS=2 cargo test --test network_runtime -- --test-threads=1 --nocapture`
to compile and exercise the C adapter under address and undefined-behavior
sanitizers. The tests require permission to bind and connect loopback sockets.
IPv6 absence produces an explicit skip with the OS error; denied loopback
permission remains a failure.

Numeric listener parsing uses `inet_pton`; see
[Apple's address conversion documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/inet_pton.3.html).

Interrupted accept/read/write operations retry within the applicable deadline.
Connection setup waits in `poll` and checks `SO_ERROR`; descriptors remain
nonblocking. See the [Linux connect documentation](https://www.man7.org/linux/man-pages/man2/connect.2.html).

The runtime avoids changing process-wide signal handlers. macOS uses
`SO_NOSIGPIPE`, as described in [Apple's socket options documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/setsockopt.2.html).
Linux uses the per-send `MSG_NOSIGNAL` flag in the
[Linux send documentation](https://man7.org/linux/man-pages/man2/send.2.html).
