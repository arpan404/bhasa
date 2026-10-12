#ifndef BHASA_RUNTIME_TLS_H
#define BHASA_RUNTIME_TLS_H
#include "net.h"
#ifdef __cplusplus
extern "C" {
#endif

typedef struct bhasa_tls_socket bhasa_tls_socket;
enum {
    BHASA_TLS_CERTIFICATE_ERROR = 10001,
    BHASA_TLS_PROTOCOL_ERROR = 10002,
    BHASA_TLS_CONFIGURATION_ERROR = 10003
};

/* OpenSSL 3, TLS >= 1.2. Client verification is mandatory: explicit PEM CA
 * bundle and DNS/IP subjectAltName identity. Address is numeric IPv4/IPv6.
 * Server loads an explicit PEM certificate chain and unencrypted private key.
 * This is server-authenticated TLS; client certificates/mTLS are not supported.
 * Every constructor returns an owned handle, including errors. */
bhasa_tls_socket *bhasa_rt_tls_connect(const bhasa_buffer *address, int64_t port,
    const bhasa_buffer *server_name, const bhasa_buffer *ca_file, int64_t milliseconds);
bhasa_tls_socket *bhasa_rt_tls_accept(const bhasa_tcp_listener *listener,
    const bhasa_buffer *cert_file, const bhasa_buffer *key_file, int64_t milliseconds);
int64_t bhasa_rt_tls_status(const bhasa_tls_socket *socket);

/* Milliseconds: 0..INT32_MAX, zero attempts without waiting. One monotonic
 * deadline covers lock acquisition and network progress. Local file loading
 * and cryptographic CPU work are synchronous and cannot be preempted; positive
 * expiration is checked afterward before reporting success or advancing I/O.
 * Read replaces destination only on success; empty success means close_notify
 * or a zero limit. Write sends all bytes or errors, possibly after a prefix.
 * Protocol/I/O failures and timeouts after an SSL call poison the shared session
 * to prevent unsafe retries with changed buffers. A lock timeout does not. */
int64_t bhasa_rt_tls_read(const bhasa_tls_socket *socket, bhasa_buffer *destination,
    int64_t limit, int64_t milliseconds);
int64_t bhasa_rt_tls_write(const bhasa_tls_socket *socket, const bhasa_buffer *contents,
    int64_t milliseconds);
/* Send close_notify; does not wait for peer shutdown or discard unread data.
 * Read may still receive peer data afterward; further writes return EPIPE. */
int64_t bhasa_rt_tls_shutdown(const bhasa_tls_socket *socket, int64_t milliseconds);

/* Clones share one TLS session and serialize I/O, including between threads.
 * Last free closes the transport and releases SSL state without network waits.
 * Call shutdown explicitly when authenticated end-of-stream is needed. */
bhasa_tls_socket *bhasa_rt_tls_socket_clone(const bhasa_tls_socket *socket);
void bhasa_rt_tls_socket_free(bhasa_tls_socket *socket);
#ifdef __cplusplus
}
#endif
#endif
