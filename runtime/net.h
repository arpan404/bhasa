#ifndef BHASA_RUNTIME_NET_H
#define BHASA_RUNTIME_NET_H

#include "core.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bhasa_tcp_listener bhasa_tcp_listener;
typedef struct bhasa_tcp_socket bhasa_tcp_socket;

/* All constructors return owned handles, including on OS/argument failure.
 * Allocation of the handle itself is fatal on failure, as in core.h.
 * Listen binds IPv4 loopback. Port zero requests an ephemeral port. */
bhasa_tcp_listener *bhasa_rt_tcp_listen(int64_t port);
/* Explicit numeric IPv4/IPv6 bind address, including 0.0.0.0 and ::.
 * No DNS, brackets or zone identifiers. Port zero selects an ephemeral port.
 * IPv6 listeners retain the OS's default IPV6_V6ONLY policy. */
bhasa_tcp_listener *bhasa_rt_tcp_listen_on(const bhasa_buffer *host, int64_t port);
bhasa_tcp_listener *bhasa_rt_tcp_listen_on_str(const char *host, int64_t port);
bhasa_tcp_socket *bhasa_rt_tcp_connect(const bhasa_buffer *host, int64_t port);
bhasa_tcp_socket *bhasa_rt_tcp_connect_str(const char *host, int64_t port);
bhasa_tcp_socket *bhasa_rt_tcp_accept(const bhasa_tcp_listener *listener);

/* Deadlines cover the complete operation and use CLOCK_MONOTONIC. Milliseconds
 * must be 0..2147483647; zero makes an immediate nonblocking attempt. Expiry is
 * ETIMEDOUT. Timed connect accepts numeric IPv4/IPv6 only, never blocking DNS.
 * All descriptors stay nonblocking throughout their lifetime; legacy APIs
 * wait indefinitely with poll instead of changing shared descriptor flags. */
bhasa_tcp_socket *bhasa_rt_tcp_accept_timeout(const bhasa_tcp_listener *listener, int64_t milliseconds);
bhasa_tcp_socket *bhasa_rt_tcp_connect_timeout(const bhasa_buffer *host, int64_t port, int64_t milliseconds);
bhasa_tcp_socket *bhasa_rt_tcp_connect_timeout_str(const char *host, int64_t port, int64_t milliseconds);
int64_t bhasa_rt_tcp_read_timeout(const bhasa_tcp_socket *socket, bhasa_buffer *destination, int64_t limit, int64_t milliseconds);
int64_t bhasa_rt_tcp_write_timeout(const bhasa_tcp_socket *socket, const bhasa_buffer *contents, int64_t milliseconds);
int64_t bhasa_rt_monotonic_millis(void);

/* Fixed constructor/clone status: zero on success, positive errno on failure.
 * Resolver errors map to ENOMEM/EAGAIN/EADDRNOTAVAIL/EINVAL or EAI_SYSTEM errno.
 * Local port is zero for an invalid listener. */
int64_t bhasa_rt_tcp_listener_status(const bhasa_tcp_listener *listener);
int64_t bhasa_rt_tcp_socket_status(const bhasa_tcp_socket *socket);
int64_t bhasa_rt_tcp_local_port(const bhasa_tcp_listener *listener);

/* Native-runtime seam only: borrow the descriptor while this handle remains
 * alive. Do not close it, change its flags, or expose it to Bhasa programs. */
int bhasa_rt_tcp_socket_fd(const bhasa_tcp_socket *socket);

/* One synchronous receive, at most limit bytes. Destination changes only on
 * success. A positive limit and empty result means EOF. Limit zero clears the
 * destination without receiving. Writes send all bytes or return an errno;
 * a failed write may already have sent a prefix. No message framing. */
int64_t bhasa_rt_tcp_read(const bhasa_tcp_socket *socket,
                          bhasa_buffer *destination, int64_t limit);
int64_t bhasa_rt_tcp_write(const bhasa_tcp_socket *socket,
                           const bhasa_buffer *contents);

/* Duplicate descriptor ownership, sharing the same kernel endpoint/stream.
 * Free closes only this handle's descriptor; it never calls shutdown. */
bhasa_tcp_listener *bhasa_rt_tcp_listener_clone(const bhasa_tcp_listener *listener);
bhasa_tcp_socket *bhasa_rt_tcp_socket_clone(const bhasa_tcp_socket *socket);
void bhasa_rt_tcp_listener_free(bhasa_tcp_listener *listener);
void bhasa_rt_tcp_socket_free(bhasa_tcp_socket *socket);

#ifdef __cplusplus
}
#endif

#endif
