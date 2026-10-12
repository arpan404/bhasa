# Native TLS runtime

The runtime implementation is in `runtime/tls.c` and `runtime/tls.h`. It uses
installed OpenSSL 3, with no downloads or package installation. This host has
OpenSSL 3.6.4, discovered through `pkg-config`. Rust/Bhasa frontend wiring and
the HTTP library integration were completed by root and passed final combined
verification. `tests/tls_language.rs` executes verified HTTPS and generic owned
TLS struct/enum payloads at O0/O2 through both frontends, including wrong-host
and untrusted-CA rejection. The complete reference/hybrid suites passed 350/358
tests in `build/verify-next-reference.log` and
`build/verify-next-hybrid-frontend.log`.

## ABI and build contract

All functions use the existing native buffer and integer ABI. Text and path
arguments are buffers, without static-string overloads. Native names prepend
`bhasa_rt_`.

| Function | Arguments | Result |
| --- | --- | --- |
| `tls_connect` | numeric address, port, server identity, CA PEM path, milliseconds | owned `tls_socket` |
| `tls_accept` | TCP listener, certificate-chain PEM path, private-key PEM path, milliseconds | owned `tls_socket` |
| `tls_status` | TLS socket | integer status |
| `tls_read` | TLS socket, destination buffer, byte limit, milliseconds | integer status |
| `tls_write` | TLS socket, source buffer, milliseconds | integer status |
| `tls_shutdown` | TLS socket, milliseconds | integer status |
| `tls_socket_clone` | TLS socket | owned `tls_socket` |
| `tls_socket_free` | TLS socket | void |

Compile `tls.c`, `net.c` and `core.c` with OpenSSL include and link flags and
`-pthread`. OpenSSL flags come from `pkg-config --cflags --libs openssl`; the
host driver owns alternate-prefix and cross-target configuration. Linking a
program that uses TLS requires the installed OpenSSL shared libraries. There
is no TLS implementation for Wasm, Windows or bare metal in this runtime.

The only new TCP seam is `bhasa_rt_tcp_socket_fd`, a C-only borrowed descriptor.
TLS creates and owns its TCP handle internally. Bhasa programs cannot wrap an
existing plaintext socket and accidentally retain a plaintext alias.

## Authentication and ownership

Clients must provide a nonempty explicit CA bundle and expected DNS or IP
identity. Verification requires a matching subjectAltName and a valid chain;
the runtime disables Common Name fallback and partial-label wildcards. DNS
identities also set SNI. Numeric IP identities use IP SAN checks. There is no
verification-disable parameter or permissive verification callback. This follows
OpenSSL's [hostname verification API](https://docs.openssl.org/3.0/man3/SSL_set1_host/).

Servers load an explicit certificate chain and matching unencrypted private
key. Password prompts are disabled. Paths and names reject embedded NULs;
configuration files must be regular files no larger than 1 MiB. Temporary PEM
storage is cleansed when released. Minimum protocol version is TLS 1.2 and
OpenSSL security level is 2. Compression and renegotiation are disabled.

This is server-authenticated TLS. Client certificate authentication, automatic
certificate reload, revocation fetching, OCSP, ALPN and session resumption APIs
are not implemented. The caller manages certificate provisioning and rotation.

A clone shares one reference-counted TLS session. A mutex serializes its SSL
operations, and the final free releases SSL, BIO and TCP resources without a
network wait. No garbage collector is involved. Operations on separate sessions
can run concurrently; clones of one session do not provide simultaneous reads
and writes.

## Errors, deadlines and shutdown

Constructors return owned error handles on failure. Status zero means success.
Argument, allocation, timeout and transport failures use positive errno values.
TLS-specific status codes are stable:

- 10001 means certificate-chain or server-identity validation failed.
- 10002 means a TLS protocol error, including transport EOF without close_notify.
- 10003 means certificate, key or trust configuration could not be loaded.

Milliseconds must be 0 through 2147483647. Zero makes a nonblocking attempt.
One monotonic deadline covers lock acquisition and network progress through
connect/accept, handshake or an I/O operation. Regular-file reads and crypto CPU
work are synchronous; the runtime cannot preempt a stalled filesystem or OS
scheduling. Repeated network readiness waits and interrupted setup retries do
not reset the budget. Positive deadlines are checked after local work and SSL
calls return; an expired constructor does not enter another TCP phase, and an
expired I/O operation cannot report success.

Read failure preserves destination data, length and capacity. Read success
replaces the buffer; an empty result with a positive limit means authenticated
TLS end-of-stream. A zero limit clears the destination without reading. Writes
send all bytes or return an error, possibly after a prefix.

An error or timeout after an SSL operation poisons the shared session; later
operations return its failure status. A timeout while waiting for another
clone's lock leaves the active session intact. This avoids violating OpenSSL's
[same-buffer retry requirements](https://docs.openssl.org/3.0/man3/SSL_write/).

`tls_shutdown` sends close_notify within its deadline. It does not wait for the
peer's shutdown or discard unread data. Reads can continue afterward; further
writes return EPIPE. Free alone closes the transport without claiming a clean
TLS shutdown. An unexpected EOF remains a protocol error, as described by
[OpenSSL error handling](https://docs.openssl.org/3.0/man3/SSL_get_error/).

The custom socket BIO uses MSG_NOSIGNAL on Linux and the TCP runtime's
SO_NOSIGPIPE configuration on macOS. It does not change process signal handlers.

## Focused verification

The native fixture generates temporary CA/server credentials and binds only
ephemeral loopback ports. Strict C11, ASan and UBSan runs passed at
O0 and O2. They covered verified DNS/IP exchanges, wrong DNS/IP identity, an untrusted
CA, invalid/mismatched/encrypted configuration, FIFO rejection, stalled client
and server handshakes, cloned-session lock timeouts, stalled reads, write
backpressure, clean shutdown, unexpected EOF and abrupt-peer writes. Runtime
and OpenSSL allocation counts returned to zero after fixture cleanup, with no
descriptor growth. A certificate containing only a matching Common Name and no
subjectAltName is also rejected.

Injected delays verify positive-deadline completion: an SSL read that returns
data after its deadline instead reports ETIMEDOUT and preserves the destination.
A delayed certificate load expires before TCP accept, leaving its queued peer
available to a subsequent accept. Timeouts before any SSL call leave the session
intact; expiration after an SSL attempt poisons it.

The final PEM-storage cleansing and additional identity cases passed both
sanitizer modes. The focused Cargo wrapper then passed with
`CARGO_BUILD_JOBS=2 cargo test --offline --locked --test tls_runtime --
--test-threads=1`, one test covering both optimization modes. Linux runtime
branches, compiler integration and HTTPS are not claimed verified by this report.
