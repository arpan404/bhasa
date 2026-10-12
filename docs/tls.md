# TLS and HTTPS

Bhasa's native TLS adapter uses installed OpenSSL 3 on macOS/Linux. Client trust
and DNS/IP subjectAltName verification are mandatory. TLS 1.2 is the minimum;
there is no verification bypass. Supply an explicit PEM CA bundle for a client,
or a certificate chain and unencrypted private key for a server. Connect
addresses are numeric IPv4/IPv6, so DNS cannot exceed the connection budget.

| Operation | Result |
| --- | --- |
| `tls_connect(address, port, server_name, ca_file, milliseconds)` | Owned `tls_socket`, including failures |
| `tls_accept(listener, cert_file, key_file, milliseconds)` | Owned TLS socket; accept and handshake share a budget |
| `tls_status(socket)` | Zero or creation/current session error |
| `tls_read(socket, destination, limit, milliseconds)` | Zero or error; replaces destination only on success |
| `tls_write(socket, contents, milliseconds)` | Zero or error; a failed write may have sent a prefix |
| `tls_shutdown(socket, milliseconds)` | Sends authenticated close_notify; does not wait for peer shutdown |
| `stream_read(socket, destination, limit, milliseconds)` | Transport-neutral timed TCP/TLS read |
| `stream_write(socket, contents, milliseconds)` | Transport-neutral timed TCP/TLS write |

Addresses, identities and paths accept `str` or `buffer`. Timeouts are
0..2147483647 milliseconds; zero attempts without waiting. Positive deadlines
cover lock acquisition and network progress and are checked after synchronous
cryptographic work. Local filesystem and cryptographic operations cannot be
preempted. PEM inputs are bounded regular files, not pipes or password prompts.

TLS-specific statuses are 10001 for certificate/identity verification, 10002
for protocol failure or unauthenticated EOF, and 10003 for configuration errors.
Other errors use positive OS errno values. Use every read/write/shutdown result.
A failure before SSL work leaves the session available; a timeout or error after
SSL work poisons it to prevent unsafe retrying with a different buffer.

Bindings, arguments and aggregate payloads retain deterministic ownership.
Copies share one refcounted TLS session with serialized I/O; last release closes
and frees it without network waits. Copies do not create independent streams.
Explicit shutdown is necessary when authenticated end-of-stream matters.

The optional local package `packages/tls` exposes `https_read`, `https_write`
and `tls_open` returning `Result[tls_socket]`. HTTP's transport helper is generic;
TCP and TLS share the same bounded parser and request deadline handling. Ordinary
`std` applications do not require OpenSSL. Add `tls = "path/to/packages/tls"`
under `[dependencies]` in exactly lowercase `bhasa.toml`, then `import "@tls"`.

Run the localhost HTTPS health example with your certificate/key:

```sh
bhasa run examples/https/main.bh -- chain.pem key.pem 8443 0
```

TLS executable builds locate installed development libraries through pkg-config.
Set `BHASA_OPENSSL_PREFIX` to an installed prefix containing `include/openssl`
and `lib` for a custom/cross target toolchain. Nothing is downloaded. Object/LLVM
emission does not require linking OpenSSL. Target libraries must match the target
architecture; this Mac's libraries do not establish cross-target execution.

The current API provides server-authenticated TLS. Mutual TLS, revocation policy,
ALPN/HTTP2, async socket state machines, Windows and Wasm TLS are not implemented.
The example handles one HTTP/1.x request per connection and processes clients
sequentially. Finite integration tests do not establish production suitability.
