#if !defined(__APPLE__) && !defined(__linux__)
#error "Bhasa TCP runtime supports macOS and Linux only"
#endif
#if defined(__linux__)
#define _GNU_SOURCE
#else
#define _DARWIN_C_SOURCE
#endif
#define _POSIX_C_SOURCE 200809L

#include "net.h"

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

struct bhasa_tcp_listener {
    int fd;
    int status;
    int64_t port;
};

struct bhasa_tcp_socket {
    int fd;
    int status;
};

static void *new_handle(size_t size) {
    void *value = malloc(size);
    if (value == NULL) {
        fputs("Bhasa runtime error: allocation failed\n", stderr);
        exit(1);
    }
    return value;
}

static bhasa_tcp_listener *new_listener(void) {
    bhasa_tcp_listener *value = new_handle(sizeof(*value));
    *value = (bhasa_tcp_listener){-1, 0, 0};
    return value;
}

static bhasa_tcp_socket *new_socket(void) {
    bhasa_tcp_socket *value = new_handle(sizeof(*value));
    *value = (bhasa_tcp_socket){-1, 0};
    return value;
}

static int os_error(void) {
    return errno > 0 ? errno : EIO;
}

typedef struct net_deadline { int enabled; int64_t end; } net_deadline;

static int monotonic_nanos(int64_t *value) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0) return os_error();
    if (now.tv_sec < 0 || now.tv_nsec < 0 || now.tv_nsec >= 1000000000 ||
        (uint64_t)now.tv_sec > (uint64_t)(INT64_MAX - now.tv_nsec) / 1000000000)
        return EOVERFLOW;
    *value = (int64_t)now.tv_sec * 1000000000 + now.tv_nsec;
    return 0;
}

int64_t bhasa_rt_monotonic_millis(void) {
    int64_t now;
    if (monotonic_nanos(&now) != 0) {
        fputs("Bhasa runtime error: monotonic clock failed\n", stderr);
        exit(1);
    }
    return now / 1000000;
}

static int start_deadline(int64_t milliseconds, net_deadline *deadline) {
    if (milliseconds < 0 || milliseconds > INT32_MAX) return EINVAL;
    int64_t now;
    int status = monotonic_nanos(&now);
    if (status != 0) return status;
    int64_t duration = milliseconds * 1000000;
    if (duration > INT64_MAX - now) return EOVERFLOW;
    *deadline = (net_deadline){1, now + duration};
    return 0;
}

static int deadline_timeout(const net_deadline *deadline, int *milliseconds) {
    if (!deadline->enabled) { *milliseconds = -1; return 0; }
    int64_t now;
    int status = monotonic_nanos(&now);
    if (status != 0) return status;
    if (now >= deadline->end) return ETIMEDOUT;
    int64_t remaining = deadline->end - now;
    *milliseconds = (int)((remaining + 999999) / 1000000);
    return 0;
}

static int wait_socket(int fd, short events, const net_deadline *deadline) {
    for (;;) {
        int timeout;
        int status = deadline_timeout(deadline, &timeout);
        if (status != 0) return status;
        struct pollfd waiter = {fd, events, 0};
        int result = poll(&waiter, 1, timeout);
        if (result < 0) { if (errno == EINTR) continue; return os_error(); }
        if (result == 0) { if (deadline->enabled) return ETIMEDOUT; continue; }
        if (waiter.revents & POLLNVAL) return EBADF;
        /* HUP/ERR are readiness: recv or SO_ERROR determines EOF/error. */
        if (waiter.revents & (events | POLLERR | POLLHUP)) return 0;
    }
}

static void close_socket(int fd) {
    /* Do not retry close on EINTR: the descriptor may already have been freed
     * and reused by another thread. No SO_LINGER is set by this runtime. */
    if (fd >= 0) (void)close(fd);
}

static int retry_setup(const net_deadline *deadline) {
    int remaining;
    int status = deadline_timeout(deadline, &remaining);
    if (status == 0) return 1;
    errno = status;
    return 0;
}

static int fd_control_deadline(int fd, int command, int argument,
                                const net_deadline *deadline) {
    int result;
    do {
        result = fcntl(fd, command, argument);
    } while (result < 0 && errno == EINTR && retry_setup(deadline));
    return result;
}

static int fd_control(int fd, int command, int argument) {
    return fd_control_deadline(fd, command, argument, &(net_deadline){0, 0});
}

static int configure_socket(int fd, const net_deadline *deadline) {
    if (fd_control_deadline(fd, F_SETFD, FD_CLOEXEC, deadline) < 0) return os_error();
    int flags = fd_control_deadline(fd, F_GETFL, 0, deadline);
    if (flags < 0 || fd_control_deadline(fd, F_SETFL, flags | O_NONBLOCK, deadline) < 0)
        return os_error();
#if defined(__APPLE__)
    int enabled = 1;
    int result;
    do {
        result = setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, sizeof(enabled));
    } while (result < 0 && errno == EINTR && retry_setup(deadline));
    if (result < 0) return os_error();
#endif
    return 0;
}

static int create_socket(int family, const net_deadline *deadline) {
    int fd;
    do {
#if defined(__linux__)
        fd = socket(family, SOCK_STREAM | SOCK_CLOEXEC, IPPROTO_TCP);
#else
        fd = socket(family, SOCK_STREAM, IPPROTO_TCP);
#endif
    } while (fd < 0 && errno == EINTR && retry_setup(deadline));
    return fd;
}

static int buffer_valid(const bhasa_buffer *value) {
    return value != NULL && value->len >= 0 && value->cap >= value->len &&
           (uint64_t)value->cap <= SIZE_MAX &&
           (value->cap == 0 || value->data != NULL);
}

static bhasa_tcp_listener *listen_address(bhasa_tcp_listener *value, int family,
                                         const struct sockaddr *address,
                                         socklen_t address_length) {
    int fd = create_socket(family, &(net_deadline){0, 0});
    if (fd < 0) {
        value->status = os_error();
        return value;
    }
    int status = configure_socket(fd, &(net_deadline){0, 0});
    if (status != 0) goto failed;
    int result;
    do {
        result = bind(fd, address, address_length);
    } while (result < 0 && errno == EINTR);
    if (result < 0) { status = os_error(); goto failed; }
    do {
        result = listen(fd, SOMAXCONN);
    } while (result < 0 && errno == EINTR);
    if (result < 0) { status = os_error(); goto failed; }
    struct sockaddr_storage local;
    socklen_t length;
    do {
        length = sizeof(local);
        result = getsockname(fd, (struct sockaddr *)&local, &length);
    } while (result < 0 && errno == EINTR);
    if (result < 0) { status = os_error(); goto failed; }
    if (local.ss_family == AF_INET && length >= sizeof(struct sockaddr_in)) {
        value->port = ntohs(((const struct sockaddr_in *)&local)->sin_port);
    } else if (local.ss_family == AF_INET6 && length >= sizeof(struct sockaddr_in6)) {
        value->port = ntohs(((const struct sockaddr_in6 *)&local)->sin6_port);
    } else {
        status = EIO;
        goto failed;
    }
    value->fd = fd;
    return value;
failed:
    close_socket(fd);
    value->status = status;
    return value;
}

static int numeric_address(const char *host, int64_t port,
                           struct sockaddr_storage *address, socklen_t *length) {
    if (host == NULL || port < 0 || port > 65535) return EINVAL;
    size_t size = strnlen(host, INET6_ADDRSTRLEN);
    if (size == 0 || size >= INET6_ADDRSTRLEN) return EINVAL;
    /* Reject zone suffixes, names, whitespace and platform-specific forms. */
    for (size_t index = 0; index < size; ++index) {
        unsigned char byte = (unsigned char)host[index];
        if ((byte >= '0' && byte <= '9') || (byte >= 'a' && byte <= 'f') ||
            (byte >= 'A' && byte <= 'F') || byte == '.' || byte == ':') continue;
        return EINVAL;
    }
    struct sockaddr_in ipv4;
    memset(&ipv4, 0, sizeof(ipv4));
    int parsed = inet_pton(AF_INET, host, &ipv4.sin_addr);
    if (parsed == 1) {
        ipv4.sin_family = AF_INET;
        ipv4.sin_port = htons((uint16_t)port);
#if defined(__APPLE__)
        ipv4.sin_len = sizeof(ipv4);
#endif
        memcpy(address, &ipv4, sizeof(ipv4));
        *length = sizeof(ipv4);
        return 0;
    }
    if (parsed < 0) return os_error();
    struct sockaddr_in6 ipv6;
    memset(&ipv6, 0, sizeof(ipv6));
    parsed = inet_pton(AF_INET6, host, &ipv6.sin6_addr);
    if (parsed == 1) {
        ipv6.sin6_family = AF_INET6;
        ipv6.sin6_port = htons((uint16_t)port);
#if defined(__APPLE__)
        ipv6.sin6_len = sizeof(ipv6);
#endif
        memcpy(address, &ipv6, sizeof(ipv6));
        *length = sizeof(ipv6);
        return 0;
    }
    return parsed < 0 ? os_error() : EINVAL;
}

static bhasa_tcp_listener *listen_numeric(bhasa_tcp_listener *value,
                                         const char *host, int64_t port) {
    struct sockaddr_storage address;
    socklen_t length;
    value->status = numeric_address(host, port, &address, &length);
    if (value->status != 0) return value;
    return listen_address(value, address.ss_family, (const struct sockaddr *)&address, length);
}

bhasa_tcp_listener *bhasa_rt_tcp_listen_on_str(const char *host, int64_t port) {
    return listen_numeric(new_listener(), host, port);
}

bhasa_tcp_listener *bhasa_rt_tcp_listen_on(const bhasa_buffer *host, int64_t port) {
    bhasa_tcp_listener *value = new_listener();
    if (!buffer_valid(host) || host->len == 0 || host->len >= INET6_ADDRSTRLEN ||
        port < 0 || port > 65535 ||
        memchr(host->data, '\0', (size_t)host->len) != NULL) {
        value->status = EINVAL;
        return value;
    }
    char text[INET6_ADDRSTRLEN];
    memcpy(text, host->data, (size_t)host->len);
    text[host->len] = '\0';
    return listen_numeric(value, text, port);
}

bhasa_tcp_listener *bhasa_rt_tcp_listen(int64_t port) {
    return bhasa_rt_tcp_listen_on_str("127.0.0.1", port);
}

static int resolver_error(int result) {
    if (result == EAI_SYSTEM) return os_error();
    if (result == EAI_MEMORY) return ENOMEM;
    if (result == EAI_AGAIN) return EAGAIN;
    if (result == EAI_NONAME) return EADDRNOTAVAIL;
    return EINVAL;
}

static int connect_socket(int fd, const struct sockaddr *address, socklen_t length,
                          const net_deadline *deadline) {
    int result = connect(fd, address, length);
    if (result == 0) return 0;
    int error = os_error();
    if (error != EINPROGRESS && error != EINTR && error != EALREADY) return error;
    int status = wait_socket(fd, POLLOUT, deadline);
    if (status != 0) return status;
    for (;;) {
        socklen_t error_size = sizeof(error);
        result = getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &error_size);
        if (result >= 0) return error;
        if (errno != EINTR) return os_error();
        int remaining;
        status = deadline_timeout(deadline, &remaining);
        if (status != 0) return status;
    }
}

static bhasa_tcp_socket *connect_host(bhasa_tcp_socket *value, const char *host,
                                      int64_t port) {
    if (host == NULL || host[0] == '\0' || port < 1 || port > 65535) {
        value->status = EINVAL;
        return value;
    }
    char service[6];
    (void)snprintf(service, sizeof(service), "%" PRId64, port);
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_protocol = IPPROTO_TCP;
    hints.ai_flags = AI_NUMERICSERV;
    struct addrinfo *addresses = NULL;
    int result;
    do {
        result = getaddrinfo(host, service, &hints, &addresses);
    } while (result == EAI_SYSTEM && errno == EINTR);
    if (result != 0) {
        value->status = resolver_error(result);
        return value;
    }
    int status = EADDRNOTAVAIL;
    for (const struct addrinfo *item = addresses; item != NULL; item = item->ai_next) {
        int fd = create_socket(item->ai_family, &(net_deadline){0, 0});
        if (fd < 0) { status = os_error(); continue; }
        status = configure_socket(fd, &(net_deadline){0, 0});
        if (status == 0) status = connect_socket(fd, item->ai_addr, item->ai_addrlen, &(net_deadline){0, 0});
        if (status == 0) {
            value->fd = fd;
            break;
        }
        close_socket(fd);
    }
    freeaddrinfo(addresses);
    value->status = status;
    return value;
}

bhasa_tcp_socket *bhasa_rt_tcp_connect_str(const char *host, int64_t port) {
    return connect_host(new_socket(), host, port);
}

bhasa_tcp_socket *bhasa_rt_tcp_connect(const bhasa_buffer *host, int64_t port) {
    bhasa_tcp_socket *value = new_socket();
    if (!buffer_valid(host) || host->len == 0 || port < 1 || port > 65535 ||
        (uint64_t)host->len >= SIZE_MAX ||
        memchr(host->data, '\0', (size_t)host->len) != NULL) {
        value->status = EINVAL;
        return value;
    }
    char *text = malloc((size_t)host->len + 1);
    if (text == NULL) {
        value->status = ENOMEM;
        return value;
    }
    memcpy(text, host->data, (size_t)host->len);
    text[host->len] = '\0';
    connect_host(value, text, port);
    free(text);
    return value;
}

static bhasa_tcp_socket *accept_deadline(bhasa_tcp_socket *value,
                                         const bhasa_tcp_listener *listener,
                                         const net_deadline *deadline) {
    value->status = (int)bhasa_rt_tcp_listener_status(listener);
    if (value->status != 0) return value;
    for (;;) {
        int fd;
#if defined(__linux__)
        fd = accept4(listener->fd, NULL, NULL, SOCK_CLOEXEC | SOCK_NONBLOCK);
#else
        fd = accept(listener->fd, NULL, NULL);
#endif
        if (fd >= 0) {
            value->status = configure_socket(fd, deadline);
            if (value->status != 0) close_socket(fd);
            else value->fd = fd;
            return value;
        }
        int status = os_error();
        if (status != EINTR && status != EAGAIN && status != EWOULDBLOCK) {
            value->status = status;
            return value;
        }
        value->status = wait_socket(listener->fd, POLLIN, deadline);
        if (value->status != 0) return value;
    }
}

bhasa_tcp_socket *bhasa_rt_tcp_accept(const bhasa_tcp_listener *listener) {
    return accept_deadline(new_socket(), listener, &(net_deadline){0, 0});
}

bhasa_tcp_socket *bhasa_rt_tcp_accept_timeout(const bhasa_tcp_listener *listener,
                                            int64_t milliseconds) {
    bhasa_tcp_socket *value = new_socket();
    net_deadline deadline;
    value->status = start_deadline(milliseconds, &deadline);
    if (value->status != 0) return value;
    return accept_deadline(value, listener, &deadline);
}

static bhasa_tcp_socket *connect_numeric_deadline(bhasa_tcp_socket *value,
                                                 const char *host, int64_t port,
                                                 int64_t milliseconds) {
    net_deadline deadline;
    value->status = start_deadline(milliseconds, &deadline);
    if (value->status != 0) return value;
    if (port == 0) { value->status = EINVAL; return value; }
    struct sockaddr_storage address;
    socklen_t length;
    value->status = numeric_address(host, port, &address, &length);
    if (value->status != 0) return value;
    int fd = create_socket(address.ss_family, &deadline);
    if (fd < 0) { value->status = os_error(); return value; }
    value->status = configure_socket(fd, &deadline);
    if (value->status == 0)
        value->status = connect_socket(fd, (const struct sockaddr *)&address, length, &deadline);
    if (value->status != 0) close_socket(fd);
    else value->fd = fd;
    return value;
}

bhasa_tcp_socket *bhasa_rt_tcp_connect_timeout_str(const char *host, int64_t port,
                                                  int64_t milliseconds) {
    return connect_numeric_deadline(new_socket(), host, port, milliseconds);
}

bhasa_tcp_socket *bhasa_rt_tcp_connect_timeout(const bhasa_buffer *host, int64_t port,
                                              int64_t milliseconds) {
    bhasa_tcp_socket *value = new_socket();
    if (!buffer_valid(host) || host->len == 0 || host->len >= INET6_ADDRSTRLEN ||
        memchr(host->data, '\0', (size_t)host->len) != NULL) {
        value->status = EINVAL;
        return value;
    }
    char text[INET6_ADDRSTRLEN];
    memcpy(text, host->data, (size_t)host->len);
    text[host->len] = '\0';
    return connect_numeric_deadline(value, text, port, milliseconds);
}

int64_t bhasa_rt_tcp_listener_status(const bhasa_tcp_listener *listener) {
    if (listener == NULL) return EINVAL;
    return listener->status;
}

int64_t bhasa_rt_tcp_socket_status(const bhasa_tcp_socket *socket) {
    if (socket == NULL) return EINVAL;
    return socket->status;
}

int64_t bhasa_rt_tcp_local_port(const bhasa_tcp_listener *listener) {
    return listener == NULL ? 0 : listener->port;
}

int bhasa_rt_tcp_socket_fd(const bhasa_tcp_socket *socket) {
    int status = (int)bhasa_rt_tcp_socket_status(socket);
    if (status != 0) { errno = status; return -1; }
    return socket->fd;
}

static int64_t read_deadline(const bhasa_tcp_socket *socket,
                             bhasa_buffer *destination, int64_t limit,
                             const net_deadline *deadline) {
    int status = (int)bhasa_rt_tcp_socket_status(socket);
    if (status != 0) return status;
    if (!buffer_valid(destination) || limit < 0 || (uint64_t)limit > SIZE_MAX ||
        (uint64_t)limit > (uint64_t)SSIZE_MAX) return EINVAL;
    unsigned char *bytes = NULL;
    ssize_t count = 0;
    if (limit > 0) {
        bytes = malloc((size_t)limit);
        if (bytes == NULL) return ENOMEM;
        for (;;) {
            count = recv(socket->fd, bytes, (size_t)limit, 0);
            if (count >= 0) break;
            status = os_error();
            if (status == EINTR || status == EAGAIN || status == EWOULDBLOCK)
                status = wait_socket(socket->fd, POLLIN, deadline);
            if (status != 0) { free(bytes); return status; }
        }
    }
    free(destination->data);
    if (count == 0) {
        free(bytes);
        *destination = (bhasa_buffer){0, 0, NULL};
    } else {
        *destination = (bhasa_buffer){(int64_t)count, limit, bytes};
    }
    return 0;
}

int64_t bhasa_rt_tcp_read(const bhasa_tcp_socket *socket,
                          bhasa_buffer *destination, int64_t limit) {
    return read_deadline(socket, destination, limit, &(net_deadline){0, 0});
}

int64_t bhasa_rt_tcp_read_timeout(const bhasa_tcp_socket *socket,
                                  bhasa_buffer *destination, int64_t limit,
                                  int64_t milliseconds) {
    net_deadline deadline;
    int status = start_deadline(milliseconds, &deadline);
    if (status != 0) return status;
    return read_deadline(socket, destination, limit, &deadline);
}

static int64_t write_deadline(const bhasa_tcp_socket *socket,
                              const bhasa_buffer *contents,
                              const net_deadline *deadline) {
    int status = (int)bhasa_rt_tcp_socket_status(socket);
    if (status != 0) return status;
    if (!buffer_valid(contents)) return EINVAL;
    size_t offset = 0;
    while (offset < (size_t)contents->len) {
        if (offset != 0 && deadline->enabled) {
            int remaining;
            status = deadline_timeout(deadline, &remaining);
            if (status != 0) return status;
        }
        size_t count = (size_t)contents->len - offset;
        if (count > (size_t)SSIZE_MAX) count = (size_t)SSIZE_MAX;
#if defined(__linux__)
        ssize_t sent = send(socket->fd, contents->data + offset, count, MSG_NOSIGNAL);
#else
        ssize_t sent = send(socket->fd, contents->data + offset, count, 0);
#endif
        if (sent < 0) {
            status = os_error();
            if (status == EINTR || status == EAGAIN || status == EWOULDBLOCK)
                status = wait_socket(socket->fd, POLLOUT, deadline);
            if (status != 0) return status;
            continue;
        }
        if (sent == 0) return EIO;
        offset += (size_t)sent;
    }
    return 0;
}

int64_t bhasa_rt_tcp_write(const bhasa_tcp_socket *socket,
                           const bhasa_buffer *contents) {
    return write_deadline(socket, contents, &(net_deadline){0, 0});
}

int64_t bhasa_rt_tcp_write_timeout(const bhasa_tcp_socket *socket,
                                   const bhasa_buffer *contents,
                                   int64_t milliseconds) {
    net_deadline deadline;
    int status = start_deadline(milliseconds, &deadline);
    if (status != 0) return status;
    return write_deadline(socket, contents, &deadline);
}

bhasa_tcp_listener *bhasa_rt_tcp_listener_clone(const bhasa_tcp_listener *listener) {
    bhasa_tcp_listener *value = new_listener();
    value->status = (int)bhasa_rt_tcp_listener_status(listener);
    if (value->status != 0) return value;
    value->fd = fd_control(listener->fd, F_DUPFD_CLOEXEC, 0);
    if (value->fd < 0) value->status = os_error();
    else value->port = listener->port;
    return value;
}

bhasa_tcp_socket *bhasa_rt_tcp_socket_clone(const bhasa_tcp_socket *socket) {
    bhasa_tcp_socket *value = new_socket();
    value->status = (int)bhasa_rt_tcp_socket_status(socket);
    if (value->status != 0) return value;
    value->fd = fd_control(socket->fd, F_DUPFD_CLOEXEC, 0);
    if (value->fd < 0) value->status = os_error();
    return value;
}

void bhasa_rt_tcp_listener_free(bhasa_tcp_listener *listener) {
    if (listener == NULL) return;
    close_socket(listener->fd);
    free(listener);
}

void bhasa_rt_tcp_socket_free(bhasa_tcp_socket *socket) {
    if (socket == NULL) return;
    close_socket(socket->fd);
    free(socket);
}
