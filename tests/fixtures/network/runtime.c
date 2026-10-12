#if defined(__linux__)
#define _GNU_SOURCE
#else
#define _DARWIN_C_SOURCE
#endif
#define _POSIX_C_SOURCE 200809L

#include "net.h"

#include <arpa/inet.h>
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

/* Count core and adapter allocations, including on platforms without LSan. */
static atomic_int live_allocations;
static int allocation_fails;
static int allocations_until_failure = -1;

static int fail_allocation(void) {
    if (allocation_fails || allocations_until_failure == 0) return 1;
    if (allocations_until_failure > 0) --allocations_until_failure;
    return 0;
}

void *bhasa_network_test_malloc(size_t size) {
    void *result = fail_allocation() ? NULL : malloc(size);
    if (result != NULL) atomic_fetch_add(&live_allocations, 1);
    return result;
}

void *bhasa_network_test_realloc(void *pointer, size_t size) {
    int was_empty = pointer == NULL;
    void *result = fail_allocation() ? NULL : realloc(pointer, size);
    if (result != NULL && was_empty) atomic_fetch_add(&live_allocations, 1);
    return result;
}

void bhasa_network_test_free(void *pointer) {
    if (pointer != NULL) assert(atomic_fetch_sub(&live_allocations, 1) > 0);
    free(pointer);
}

/* Redirect only net.c calls. A fault is consumed before the real syscall. */
static const char *fault_call;
static int fault_errno;
static int fault_remaining;
static size_t send_limit;
static int send_calls;
static int last_runtime_fd = -1;
static int send_zero;
static atomic_int runtime_descriptors[8192];
static int resolver_calls;
static int socket_calls;
static int intercepted_bind_family;
static uint16_t intercepted_bind_port;
static unsigned char intercepted_bind_address[16];
static int intercepted_bind_calls;

static int own_descriptor(int fd) {
    if (fd >= 0) {
        assert(fd < (int)(sizeof(runtime_descriptors) / sizeof(runtime_descriptors[0])));
        assert(atomic_exchange(&runtime_descriptors[fd], 1) == 0);
    }
    return fd;
}

static void inject(const char *call, int error, int count) {
    assert(fault_remaining == 0);
    fault_call = call;
    fault_errno = error;
    fault_remaining = count;
}

static int injected(const char *call) {
    if (fault_remaining != 0 && strcmp(call, fault_call) == 0) {
        --fault_remaining;
        errno = fault_errno;
        return 1;
    }
    return 0;
}

int bhasa_network_test_socket(int domain, int type, int protocol) {
    ++socket_calls;
    if (intercepted_bind_family != 0) {
        assert(domain == intercepted_bind_family);
        /* These tests never call real bind. Use an available IPv4 socket even
         * when only the parsing of an IPv6 address is being checked. */
        domain = AF_INET;
    }
    last_runtime_fd = injected("socket") ? -1 : own_descriptor(socket(domain, type, protocol));
    return last_runtime_fd;
}

int bhasa_network_test_bind(int fd, const struct sockaddr *address, socklen_t length) {
    if (intercepted_bind_family != 0) {
        assert(address->sa_family == intercepted_bind_family);
        if (address->sa_family == AF_INET) {
            assert(length == sizeof(struct sockaddr_in));
            const struct sockaddr_in *ipv4 = (const struct sockaddr_in *)address;
            assert(ntohs(ipv4->sin_port) == intercepted_bind_port);
            assert(memcmp(&ipv4->sin_addr, intercepted_bind_address, 4) == 0);
        } else {
            assert(address->sa_family == AF_INET6 && length == sizeof(struct sockaddr_in6));
            const struct sockaddr_in6 *ipv6 = (const struct sockaddr_in6 *)address;
            assert(ntohs(ipv6->sin6_port) == intercepted_bind_port);
            assert(ipv6->sin6_scope_id == 0 && ipv6->sin6_flowinfo == 0);
            assert(memcmp(&ipv6->sin6_addr, intercepted_bind_address, 16) == 0);
        }
        ++intercepted_bind_calls;
        errno = EACCES;
        return -1;
    }
    /* A malformed test or runtime must never bind a wildcard/external address. */
    if (address->sa_family == AF_INET) {
        assert(length == sizeof(struct sockaddr_in));
        assert(((const struct sockaddr_in *)address)->sin_addr.s_addr == htonl(INADDR_LOOPBACK));
    } else {
        assert(address->sa_family == AF_INET6 && length == sizeof(struct sockaddr_in6));
        assert(IN6_IS_ADDR_LOOPBACK(&((const struct sockaddr_in6 *)address)->sin6_addr));
    }
    return injected("bind") ? -1 : bind(fd, address, length);
}

int bhasa_network_test_listen(int fd, int backlog) {
    return injected("listen") ? -1 : listen(fd, backlog);
}

int bhasa_network_test_getsockname(int fd, struct sockaddr *address, socklen_t *length) {
    return injected("getsockname") ? -1 : getsockname(fd, address, length);
}

int bhasa_network_test_connect(int fd, const struct sockaddr *address, socklen_t length) {
    return injected("connect") ? -1 : connect(fd, address, length);
}

int bhasa_network_test_accept(int fd, struct sockaddr *address, socklen_t *length) {
    last_runtime_fd = injected("accept") ? -1 : own_descriptor(accept(fd, address, length));
    return last_runtime_fd;
}

#if defined(__linux__)
int bhasa_network_test_accept4(int fd, struct sockaddr *address, socklen_t *length, int flags) {
    last_runtime_fd = injected("accept") ? -1 : own_descriptor(accept4(fd, address, length, flags));
    return last_runtime_fd;
}
#endif

int bhasa_network_test_fcntl(int fd, int command, ...) {
    va_list arguments;
    va_start(arguments, command);
    int argument = va_arg(arguments, int);
    va_end(arguments);
    const char *name;
    switch (command) {
        case F_DUPFD_CLOEXEC: name = "dup"; break;
        case F_GETFL: name = "getfl"; break;
        case F_SETFL: name = "setfl"; break;
        case F_SETFD: name = "setfd"; break;
        default: assert(!"unexpected runtime fcntl command"); return -1;
    }
    int result = injected(name) ? -1 : fcntl(fd, command, argument);
    if (command == F_DUPFD_CLOEXEC) last_runtime_fd = own_descriptor(result);
    return result;
}

int bhasa_network_test_close(int fd) {
    assert(fd >= 0 && fd < (int)(sizeof(runtime_descriptors) / sizeof(runtime_descriptors[0])));
    assert(atomic_exchange(&runtime_descriptors[fd], 0) == 1);
    return close(fd);
}

int bhasa_network_test_setsockopt(int fd, int level, int option,
                                const void *value, socklen_t length) {
    return injected("setsockopt") ? -1 : setsockopt(fd, level, option, value, length);
}

int bhasa_network_test_getsockopt(int fd, int level, int option,
                                void *value, socklen_t *length) {
    return injected("getsockopt") ? -1 : getsockopt(fd, level, option, value, length);
}

int bhasa_network_test_poll(struct pollfd *descriptors, nfds_t count, int timeout) {
    return injected("poll") ? -1 : poll(descriptors, count, timeout);
}

ssize_t bhasa_network_test_recv(int fd, void *buffer, size_t length, int flags) {
    return injected("recv") ? -1 : recv(fd, buffer, length, flags);
}

ssize_t bhasa_network_test_send(int fd, const void *buffer, size_t length, int flags) {
    ++send_calls;
    if (injected("send")) return -1;
    if (send_zero) return 0;
    if (send_limit != 0 && length > send_limit) length = send_limit;
    return send(fd, buffer, length, flags);
}

int bhasa_network_test_getaddrinfo(const char *host, const char *service,
                                 const struct addrinfo *hints, struct addrinfo **result) {
    ++resolver_calls;
    return injected("getaddrinfo") ? EAI_SYSTEM : getaddrinfo(host, service, hints, result);
}

static int descriptor_count(void) {
    int count = 0;
    for (int fd = 0; fd < 4096; ++fd)
        if (fcntl(fd, F_GETFD) != -1) ++count;
    return count;
}

static void check_descriptor(void) {
    assert(last_runtime_fd >= 0);
    assert((fcntl(last_runtime_fd, F_GETFD) & FD_CLOEXEC) != 0);
    int flags = fcntl(last_runtime_fd, F_GETFL);
    assert(flags >= 0 && (flags & O_NONBLOCK) != 0);
}

static bhasa_tcp_listener *listener(void) {
    bhasa_tcp_listener *value = bhasa_rt_tcp_listen(0);
    assert(value != NULL);
    int64_t status = bhasa_rt_tcp_listener_status(value);
    if (status != 0)
        fprintf(stderr, "loopback listen failed with errno %lld: %s; loopback permission is required\n",
                (long long)status, strerror((int)status));
    assert(status == 0);
    check_descriptor();
    int64_t port = bhasa_rt_tcp_local_port(value);
    assert(port > 0 && port <= 65535);
    return value;
}

static bhasa_tcp_socket *connect_to(bhasa_tcp_listener *server) {
    bhasa_buffer *host = bhasa_rt_buffer_from_str("127.0.0.1");
    bhasa_tcp_socket *value = bhasa_rt_tcp_connect(host, bhasa_rt_tcp_local_port(server));
    /* The adapter must finish using its borrowed host before it returns. */
    bhasa_rt_buffer_free(host);
    assert(value != NULL && bhasa_rt_tcp_socket_status(value) == 0);
    check_descriptor();
    return value;
}

static bhasa_buffer *bytes(size_t size) {
    bhasa_buffer *value = bhasa_rt_buffer_new();
    for (size_t i = 0; i < size; ++i) bhasa_rt_buffer_push(value, (int64_t)(i % 256));
    return value;
}

static void receive_bytes(bhasa_tcp_socket *socket_value, size_t length, int64_t limit) {
    bhasa_buffer *read = bhasa_rt_buffer_from_str("replace on each successful read");
    size_t offset = 0;
    while (offset < length) {
        assert(bhasa_rt_tcp_read(socket_value, read, limit) == 0);
        assert(read->len > 0 && read->len <= limit);
        assert((uint64_t)read->len <= length - offset);
        for (int64_t i = 0; i < read->len; ++i)
            assert(read->data[i] == (unsigned char)((offset + (size_t)i) % 256));
        offset += (size_t)read->len;
    }
    bhasa_rt_buffer_free(read);
}

static void test_loopback(void) {
    int baseline = descriptor_count();
    bhasa_tcp_listener *original = listener();
    int64_t port = bhasa_rt_tcp_local_port(original);
    bhasa_tcp_listener *server = bhasa_rt_tcp_listener_clone(original);
    assert(server != original && bhasa_rt_tcp_listener_status(server) == 0);
    check_descriptor();
    assert(bhasa_rt_tcp_local_port(server) == port);
    bhasa_rt_tcp_listener_free(original);

    bhasa_tcp_socket *client = connect_to(server);
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(server);
    assert(accepted != NULL && bhasa_rt_tcp_socket_status(accepted) == 0);
    check_descriptor();
    bhasa_tcp_socket *copy = bhasa_rt_tcp_socket_clone(client);
    assert(copy != client && bhasa_rt_tcp_socket_status(copy) == 0);
    check_descriptor();
    bhasa_rt_tcp_socket_free(client);

    bhasa_buffer *payload = bytes(1025);
    assert(bhasa_rt_tcp_write(copy, payload) == 0);
    receive_bytes(accepted, 1025, 17);
    assert(bhasa_rt_tcp_write(accepted, payload) == 0);
    receive_bytes(copy, 1025, 23);

    bhasa_buffer *destination = bhasa_rt_buffer_from_str("old");
    assert(bhasa_rt_tcp_read(copy, destination, 0) == 0);
    assert(destination->len == 0);
    assert(bhasa_rt_tcp_write(copy, destination) == 0);
    bhasa_rt_tcp_socket_free(accepted);
    bhasa_rt_buffer_append_str(destination, "must clear at EOF");
    assert(bhasa_rt_tcp_read(copy, destination, 8) == 0);
    assert(destination->len == 0);
    assert(bhasa_rt_tcp_socket_status(copy) == 0);
    bhasa_rt_tcp_socket_free(copy);
    bhasa_rt_buffer_free(destination);
    bhasa_rt_buffer_free(payload);

    /* A listener clone remains usable for more than one accepted connection. */
    for (int i = 0; i < 20; ++i) {
        client = bhasa_rt_tcp_connect_str("localhost", port);
        assert(bhasa_rt_tcp_socket_status(client) == 0);
        accepted = bhasa_rt_tcp_accept(server);
        assert(bhasa_rt_tcp_socket_status(accepted) == 0);
        bhasa_rt_tcp_socket_free(client);
        destination = bhasa_rt_buffer_from_str("EOF");
        assert(bhasa_rt_tcp_read(accepted, destination, 3) == 0);
        assert(destination->len == 0);
        bhasa_rt_buffer_free(destination);
        bhasa_rt_tcp_socket_free(accepted);
    }
    bhasa_rt_tcp_listener_free(server);
    bhasa_rt_tcp_listener_free(NULL);
    bhasa_rt_tcp_socket_free(NULL);
    assert(descriptor_count() == baseline);
}

static void *stream_writer(void *argument) {
    bhasa_tcp_socket *socket_value = argument;
    bhasa_buffer *payload = bytes(1024 * 1024);
    assert(bhasa_rt_tcp_write(socket_value, payload) == 0);
    bhasa_rt_buffer_free(payload);
    bhasa_rt_tcp_socket_free(socket_value);
    return NULL;
}

static void test_stream(void) {
    int baseline = descriptor_count();
    bhasa_tcp_listener *server = listener();
    bhasa_tcp_socket *client = connect_to(server);
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(server);
    assert(bhasa_rt_tcp_socket_status(accepted) == 0);
    bhasa_rt_tcp_listener_free(server);
    pthread_t writer;
    assert(pthread_create(&writer, NULL, stream_writer, client) == 0);
    receive_bytes(accepted, 1024 * 1024, 997);
    assert(pthread_join(writer, NULL) == 0);
    bhasa_buffer *read = bhasa_rt_buffer_from_str("EOF after writer destroys its socket");
    assert(bhasa_rt_tcp_read(accepted, read, 100) == 0 && read->len == 0);
    bhasa_rt_buffer_free(read);
    bhasa_rt_tcp_socket_free(accepted);
    assert(descriptor_count() == baseline);
}

static void failed_socket(bhasa_tcp_socket *value, int64_t expected) {
    assert(value != NULL);
    int64_t status = bhasa_rt_tcp_socket_status(value);
    assert(status > 0);
    if (expected != 0) assert(status == expected);
    bhasa_tcp_socket *copy = bhasa_rt_tcp_socket_clone(value);
    assert(copy != value && bhasa_rt_tcp_socket_status(copy) == status);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("preserved");
    assert(bhasa_rt_tcp_read(value, data, 10) == status);
    assert(bhasa_rt_buffer_same_str(data, "preserved"));
    assert(bhasa_rt_tcp_write(value, data) == status);
    assert(bhasa_rt_tcp_socket_status(value) == status);
    bhasa_rt_buffer_free(data);
    bhasa_rt_tcp_socket_free(value);
    assert(bhasa_rt_tcp_socket_status(copy) == status);
    bhasa_rt_tcp_socket_free(copy);
}

static void failed_listener(bhasa_tcp_listener *value, int64_t expected) {
    assert(value != NULL);
    int64_t status = bhasa_rt_tcp_listener_status(value);
    assert(status > 0);
    if (expected != 0) assert(status == expected);
    assert(bhasa_rt_tcp_local_port(value) == 0);
    bhasa_tcp_listener *copy = bhasa_rt_tcp_listener_clone(value);
    assert(copy != value && bhasa_rt_tcp_listener_status(copy) == status);
    assert(bhasa_rt_tcp_local_port(copy) == 0);
    failed_socket(bhasa_rt_tcp_accept(value), status);
    bhasa_rt_tcp_listener_free(value);
    assert(bhasa_rt_tcp_listener_status(copy) == status);
    bhasa_rt_tcp_listener_free(copy);
}

static bhasa_tcp_listener *listen_numeric(const char *host, int64_t port, int buffer_overload) {
    int previous_resolver_calls = resolver_calls;
    bhasa_tcp_listener *value;
    if (buffer_overload) {
        int64_t length = (int64_t)strlen(host);
        bhasa_buffer borrowed = {length, length, (unsigned char *)host};
        value = bhasa_rt_tcp_listen_on(&borrowed, port);
        assert(borrowed.len == length && borrowed.cap == length);
        assert(borrowed.data == (const unsigned char *)host);
    } else {
        value = bhasa_rt_tcp_listen_on_str(host, port);
    }
    assert(resolver_calls == previous_resolver_calls);
    assert(value != NULL);
    return value;
}

static void invalid_numeric(const char *host, int64_t port, int buffer_overload) {
    int previous_socket_calls = socket_calls;
    failed_listener(listen_numeric(host, port, buffer_overload), EINVAL);
    assert(socket_calls == previous_socket_calls);
}

static int ipv6_unavailable(int64_t status) {
    if (status == EAFNOSUPPORT || status == EPROTONOSUPPORT || status == EADDRNOTAVAIL)
        return 1;
#if defined(EPFNOSUPPORT)
    if (status == EPFNOSUPPORT) return 1;
#endif
    return 0;
}

static void exchange_on_listener(bhasa_tcp_listener *original, const char *host) {
    assert(bhasa_rt_tcp_listener_status(original) == 0);
    check_descriptor();
    int64_t port = bhasa_rt_tcp_local_port(original);
    assert(port > 0 && port <= 65535);
    bhasa_tcp_listener *server = bhasa_rt_tcp_listener_clone(original);
    assert(server != original && bhasa_rt_tcp_listener_status(server) == 0);
    assert(bhasa_rt_tcp_local_port(server) == port);
    check_descriptor();
    bhasa_rt_tcp_listener_free(original);

    bhasa_tcp_socket *client = bhasa_rt_tcp_connect_str(host, port);
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    check_descriptor();
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(server);
    assert(bhasa_rt_tcp_socket_status(accepted) == 0);
    check_descriptor();
    bhasa_rt_tcp_listener_free(server);
    bhasa_tcp_socket *copy = bhasa_rt_tcp_socket_clone(client);
    assert(copy != client && bhasa_rt_tcp_socket_status(copy) == 0);
    check_descriptor();
    bhasa_rt_tcp_socket_free(client);

    bhasa_buffer *payload = bytes(513);
    assert(bhasa_rt_tcp_write(copy, payload) == 0);
    receive_bytes(accepted, 513, 19);
    assert(bhasa_rt_tcp_write(accepted, payload) == 0);
    receive_bytes(copy, 513, 13);
    bhasa_rt_tcp_socket_free(accepted);
    assert(bhasa_rt_tcp_read(copy, payload, 20) == 0 && payload->len == 0);
    bhasa_rt_buffer_free(payload);
    bhasa_rt_tcp_socket_free(copy);
}

static void check_intercepted_bind(const char *host, int family, uint16_t port,
                                   int buffer_overload) {
    int baseline = descriptor_count();
    intercepted_bind_family = family;
    intercepted_bind_port = port;
    intercepted_bind_calls = 0;
    assert(inet_pton(family, host, intercepted_bind_address) == 1);
    bhasa_tcp_listener *value = listen_numeric(host, port, buffer_overload);
    assert(intercepted_bind_calls == 1);
    intercepted_bind_family = 0;
    failed_listener(value, EACCES);
    assert(descriptor_count() == baseline);
}

static void test_listen_on(void) {
    int baseline = descriptor_count();
    for (int overload = 0; overload < 2; ++overload) {
        /* Only the owned handle may allocate; address conversion uses the stack. */
        allocations_until_failure = 1;
        bhasa_tcp_listener *value = listen_numeric("127.0.0.1", 0, overload);
        assert(allocations_until_failure == 0);
        allocations_until_failure = -1;
        exchange_on_listener(value, "127.0.0.1");
        assert(descriptor_count() == baseline);

        const char *invalid_hosts[] = {
            "", "localhost", "127.1", "2130706433", "127.0.0.-1", "256.0.0.1",
            "127.0.0.1:80", "127.0.0.1/8", " 127.0.0.1", "127.0.0.1 ",
            "\t127.0.0.1", "127.0.0.1\n", "[::1]", "::1%lo0", "::1%1", "::1/128",
            "::1 ", " ::1", ":", "1::2::3", "gggg::1", "\377", "127.0.0.1\377",
            "1111111111111111111111111111111111111111111111111111111111111111"
        };
        for (size_t i = 0; i < sizeof(invalid_hosts) / sizeof(invalid_hosts[0]); ++i)
            invalid_numeric(invalid_hosts[i], 0, overload);
        int64_t invalid_ports[] = {-1, 65536, INT64_MIN, INT64_MAX};
        for (size_t i = 0; i < sizeof(invalid_ports) / sizeof(invalid_ports[0]); ++i) {
            invalid_numeric("127.0.0.1", invalid_ports[i], overload);
            invalid_numeric("::1", invalid_ports[i], overload);
        }

        /* Numeric acceptance and the inclusive upper port bound, without binding. */
        check_intercepted_bind("0.0.0.0", AF_INET, 0, overload);
        check_intercepted_bind("0.0.0.0", AF_INET, 65535, overload);
        check_intercepted_bind("::", AF_INET6, 0, overload);
        check_intercepted_bind("::", AF_INET6, 65535, overload);
        check_intercepted_bind("192.0.2.1", AF_INET, 12345, overload);
        check_intercepted_bind("2001:db8::1", AF_INET6, 12345, overload);
        check_intercepted_bind("::ffff:192.0.2.1", AF_INET6, 65535, overload);

        const char *calls[] = {"socket", "setfd", "bind", "listen", "getsockname"};
        for (size_t i = 0; i < sizeof(calls) / sizeof(calls[0]); ++i) {
            inject(calls[i], EIO, 1);
            failed_listener(listen_numeric("127.0.0.1", 0, overload), EIO);
            assert(fault_remaining == 0 && descriptor_count() == baseline);
        }
    }

    int previous_resolver_calls = resolver_calls;
    int previous_socket_calls = socket_calls;
    failed_listener(bhasa_rt_tcp_listen_on(NULL, 0), EINVAL);
    failed_listener(bhasa_rt_tcp_listen_on_str(NULL, 0), EINVAL);
    unsigned char nul_host[] = {'1', '2', '7', '.', '0', '.', '0', '.', '1', 0, 'x'};
    bhasa_buffer rejected[] = {
        {-1, 0, NULL}, {1, 0, NULL}, {0, 1, NULL}, {0, 0, NULL},
        {INT64_MAX, INT64_MAX, nul_host},
        {(int64_t)sizeof(nul_host), (int64_t)sizeof(nul_host), nul_host},
        {10, 10, nul_host}
    };
    for (size_t i = 0; i < sizeof(rejected) / sizeof(rejected[0]); ++i)
        failed_listener(bhasa_rt_tcp_listen_on(&rejected[i], 0), EINVAL);
    assert(socket_calls == previous_socket_calls);
    assert(resolver_calls == previous_resolver_calls);

    /* The buffer overload must not read beyond len for a NUL terminator. */
    unsigned char unterminated[] = {'1', '2', '7', '.', '0', '.', '0', '.', '1'};
    bhasa_buffer borrowed = {9, 9, unterminated};
    bhasa_tcp_listener *value = bhasa_rt_tcp_listen_on(&borrowed, 0);
    assert(resolver_calls == previous_resolver_calls);
    memset(unterminated, 0xff, sizeof(unterminated));
    exchange_on_listener(value, "127.0.0.1");

    bhasa_buffer *owned_host = bhasa_rt_buffer_from_str("127.0.0.1");
    unsigned char *owned_data = owned_host->data;
    int64_t owned_capacity = owned_host->cap;
    previous_resolver_calls = resolver_calls;
    value = bhasa_rt_tcp_listen_on(owned_host, 0);
    assert(resolver_calls == previous_resolver_calls);
    assert(owned_host->data == owned_data && owned_host->cap == owned_capacity);
    assert(bhasa_rt_buffer_same_str(owned_host, "127.0.0.1"));
    bhasa_rt_buffer_free(owned_host);
    exchange_on_listener(value, "127.0.0.1");
    assert(descriptor_count() == baseline);
}

static void test_listen_on_ipv6(void) {
    int baseline = descriptor_count();
    bhasa_tcp_listener *value = listen_numeric("::1", 0, 0);
    int64_t status = bhasa_rt_tcp_listener_status(value);
    if (ipv6_unavailable(status)) {
        failed_listener(value, status);
        assert(descriptor_count() == baseline);
        printf("SKIP native IPv6 loopback: errno=%lld (%s)\n",
               (long long)status, strerror((int)status));
        return;
    }
    if (status != 0)
        fprintf(stderr, "IPv6 loopback listen failed with errno %lld: %s\n",
                (long long)status, strerror((int)status));
    exchange_on_listener(value, "::1");
    for (int overload = 0; overload < 2; ++overload) {
        allocations_until_failure = 1;
        value = listen_numeric("0:0:0:0:0:0:0:1", 0, overload);
        assert(allocations_until_failure == 0);
        allocations_until_failure = -1;
        exchange_on_listener(value, "::1");
        assert(descriptor_count() == baseline);
    }
}

static void test_errors(void) {
    int baseline = descriptor_count();
    assert(bhasa_rt_tcp_listener_status(NULL) == EINVAL);
    assert(bhasa_rt_tcp_socket_status(NULL) == EINVAL);
    assert(bhasa_rt_tcp_local_port(NULL) == 0);
    failed_listener(bhasa_rt_tcp_listener_clone(NULL), EINVAL);
    failed_socket(bhasa_rt_tcp_socket_clone(NULL), EINVAL);
    failed_socket(bhasa_rt_tcp_accept(NULL), EINVAL);
    failed_socket(bhasa_rt_tcp_connect(NULL, 80), EINVAL);
    failed_socket(bhasa_rt_tcp_connect_str(NULL, 80), EINVAL);
    failed_socket(bhasa_rt_tcp_connect_str("", 80), EINVAL);
    int64_t invalid_ports[] = {-1, 65536, INT64_MIN, INT64_MAX};
    for (size_t i = 0; i < sizeof(invalid_ports) / sizeof(invalid_ports[0]); ++i) {
        failed_listener(bhasa_rt_tcp_listen(invalid_ports[i]), EINVAL);
        failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", invalid_ports[i]), EINVAL);
    }
    failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", 0), EINVAL);
    bhasa_buffer *host = bhasa_rt_buffer_from_str("127.0.0.1");
    bhasa_rt_buffer_push(host, 0);
    bhasa_rt_buffer_append_str(host, "not part of a host name");
    failed_socket(bhasa_rt_tcp_connect(host, 80), EINVAL);
    bhasa_rt_buffer_free(host);

    bhasa_tcp_listener *server = listener();
    int64_t port = bhasa_rt_tcp_local_port(server);
    failed_listener(bhasa_rt_tcp_listen(port), EADDRINUSE);
    bhasa_tcp_socket *client = connect_to(server);
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(server);
    assert(bhasa_rt_tcp_socket_status(accepted) == 0);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("preserved");
    assert(bhasa_rt_tcp_read(client, data, -1) == EINVAL);
    assert(bhasa_rt_tcp_read(client, data, INT64_MIN) == EINVAL);
    assert(bhasa_rt_buffer_same_str(data, "preserved"));
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    bhasa_buffer malformed[] = {{-1, 0, NULL}, {1, 0, NULL}, {0, 1, NULL}};
    assert(bhasa_rt_tcp_read(client, NULL, 1) == EINVAL);
    assert(bhasa_rt_tcp_write(client, NULL) == EINVAL);
    for (size_t i = 0; i < sizeof(malformed) / sizeof(malformed[0]); ++i) {
        assert(bhasa_rt_tcp_read(client, &malformed[i], 1) == EINVAL);
        assert(bhasa_rt_tcp_write(client, &malformed[i]) == EINVAL);
        failed_socket(bhasa_rt_tcp_connect(&malformed[i], 80), EINVAL);
    }
    bhasa_rt_buffer_free(data);
    bhasa_rt_tcp_socket_free(client);
    bhasa_rt_tcp_socket_free(accepted);
    bhasa_rt_tcp_listener_free(server);
    /* The just-closed listener supplies a local refusal without external DNS. */
    failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", port), ECONNREFUSED);
    assert(descriptor_count() == baseline);
}

static void test_injected(void) {
    int baseline = descriptor_count();
    const char *listen_calls[] = {"socket", "bind", "listen", "getsockname", "setfd"
#if defined(__APPLE__)
        , "setsockopt"
#endif
    };
    for (size_t i = 0; i < sizeof(listen_calls) / sizeof(listen_calls[0]); ++i) {
        inject(listen_calls[i], EACCES, 1);
        failed_listener(bhasa_rt_tcp_listen(0), EACCES);
        assert(fault_remaining == 0 && descriptor_count() == baseline);
        inject(listen_calls[i], EINTR, 2);
        bhasa_tcp_listener *retried = listener();
        assert(fault_remaining == 0);
        bhasa_rt_tcp_listener_free(retried);
        assert(descriptor_count() == baseline);
    }
    inject("getaddrinfo", EAGAIN, 1);
    failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", 12345), EAGAIN);
    assert(fault_remaining == 0 && descriptor_count() == baseline);

    bhasa_tcp_listener *server = listener();
    int with_listener = descriptor_count();
    const char *connect_calls[] = {"socket", "setfd", "getfl", "setfl", "connect", "poll", "getsockopt"};
    for (size_t i = 0; i < sizeof(connect_calls) / sizeof(connect_calls[0]); ++i) {
        inject(connect_calls[i], EIO, 1);
        failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", bhasa_rt_tcp_local_port(server)), EIO);
        assert(fault_remaining == 0 && descriptor_count() == with_listener);
    }
    /* Failed pending connects may have reached the accept queue. Start fresh. */
    bhasa_rt_tcp_listener_free(server);
    server = listener();
    inject("accept", ECONNABORTED, 1);
    failed_socket(bhasa_rt_tcp_accept(server), ECONNABORTED);
    inject("dup", EMFILE, 1);
    failed_listener(bhasa_rt_tcp_listener_clone(server), EMFILE);
    assert(fault_remaining == 0);
    bhasa_buffer *host = bhasa_rt_buffer_from_str("127.0.0.1");
    allocations_until_failure = 1;
    bhasa_tcp_socket *host_failed = bhasa_rt_tcp_connect(host, bhasa_rt_tcp_local_port(server));
    allocations_until_failure = -1;
    failed_socket(host_failed, ENOMEM);
    bhasa_rt_buffer_free(host);
    inject("connect", ECONNREFUSED, 1);
    failed_socket(bhasa_rt_tcp_connect_str("127.0.0.1", bhasa_rt_tcp_local_port(server)), ECONNREFUSED);
    assert(fault_remaining == 0);

    bhasa_tcp_socket *client = connect_to(server);
    inject("accept", EINTR, 1);
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(server);
    assert(fault_remaining == 0 && bhasa_rt_tcp_socket_status(accepted) == 0);
    inject("dup", EMFILE, 1);
    failed_socket(bhasa_rt_tcp_socket_clone(client), EMFILE);
    assert(fault_remaining == 0);

    bhasa_buffer *payload = bytes(100);
    send_limit = 7;
    send_calls = 0;
    inject("send", EINTR, 2);
    assert(bhasa_rt_tcp_write(client, payload) == 0);
    assert(fault_remaining == 0 && send_calls >= 17);
    send_limit = 0;
    inject("recv", EINTR, 2);
    receive_bytes(accepted, 100, 31);
    assert(fault_remaining == 0);

    bhasa_buffer *data = bhasa_rt_buffer_from_str("preserved after recv failure");
    unsigned char *old_data = data->data;
    int64_t old_capacity = data->cap;
    inject("recv", EIO, 1);
    assert(bhasa_rt_tcp_read(client, data, 100) == EIO);
    assert(data->data == old_data && data->cap == old_capacity);
    assert(bhasa_rt_buffer_same_str(data, "preserved after recv failure"));
    assert(fault_remaining == 0 && bhasa_rt_tcp_socket_status(client) == 0);
    allocation_fails = 1;
    assert(bhasa_rt_tcp_read(client, data, 100) == ENOMEM);
    allocation_fails = 0;
    assert(data->data == old_data && data->cap == old_capacity);
    assert(bhasa_rt_buffer_same_str(data, "preserved after recv failure"));
    inject("send", EIO, 1);
    assert(bhasa_rt_tcp_write(client, data) == EIO);
    assert(fault_remaining == 0 && bhasa_rt_tcp_socket_status(client) == 0);
    send_zero = 1;
    assert(bhasa_rt_tcp_write(client, data) == EIO);
    send_zero = 0;
    bhasa_rt_buffer_free(data);
    bhasa_rt_buffer_free(payload);
    bhasa_rt_tcp_socket_free(client);
    bhasa_rt_tcp_socket_free(accepted);
    bhasa_rt_tcp_listener_free(server);
    assert(descriptor_count() == baseline);
}

static void test_sigpipe(void) {
    int baseline = descriptor_count();
    assert(signal(SIGPIPE, SIG_DFL) != SIG_ERR);
    int raw_listener = socket(AF_INET, SOCK_STREAM, 0);
    assert(raw_listener >= 0);
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    assert(bind(raw_listener, (const struct sockaddr *)&address, sizeof(address)) == 0);
    assert(listen(raw_listener, 1) == 0);
    socklen_t length = sizeof(address);
    assert(getsockname(raw_listener, (struct sockaddr *)&address, &length) == 0);
    bhasa_tcp_socket *client = bhasa_rt_tcp_connect_str("127.0.0.1", ntohs(address.sin_port));
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    int peer = accept(raw_listener, NULL, NULL);
    assert(peer >= 0);
    struct linger reset = {1, 0};
    assert(setsockopt(peer, SOL_SOCKET, SO_LINGER, &reset, sizeof(reset)) == 0);
    assert(close(peer) == 0);
    assert(close(raw_listener) == 0);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("unchanged");
    assert(bhasa_rt_tcp_read(client, data, 100) == ECONNRESET);
    assert(bhasa_rt_buffer_same_str(data, "unchanged"));
    assert(bhasa_rt_tcp_write(client, data) == EPIPE);
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    bhasa_rt_buffer_free(data);
    bhasa_rt_tcp_socket_free(client);
    assert(descriptor_count() == baseline);
}

static void test_allocation_failure(const char *mode) {
    if (strcmp(mode, "allocation-listen-on-str") == 0) {
        allocation_fails = 1;
        bhasa_rt_tcp_listen_on_str("127.0.0.1", 0);
    }
    if (strcmp(mode, "allocation-listen-on-buffer") == 0) {
        unsigned char host[] = {'1', '2', '7', '.', '0', '.', '0', '.', '1'};
        bhasa_buffer borrowed = {9, 9, host};
        allocation_fails = 1;
        bhasa_rt_tcp_listen_on(&borrowed, 0);
    }
    if (strcmp(mode, "allocation-listen") == 0) {
        allocation_fails = 1;
        bhasa_rt_tcp_listen(0);
    }
    if (strcmp(mode, "allocation-connect") == 0) {
        allocation_fails = 1;
        bhasa_rt_tcp_connect_str("127.0.0.1", 1);
    }
    bhasa_tcp_listener *server = listener();
    if (strcmp(mode, "allocation-clone") == 0) {
        allocation_fails = 1;
        bhasa_rt_tcp_listener_clone(server);
    }
    if (strcmp(mode, "allocation-accept") == 0) {
        allocation_fails = 1;
        bhasa_rt_tcp_accept(server);
    }
    if (strcmp(mode, "allocation-socket-clone") == 0) {
        bhasa_tcp_socket *client = connect_to(server);
        allocation_fails = 1;
        bhasa_rt_tcp_socket_clone(client);
    }
    assert(!"missing allocation failure");
}

int main(int argc, char **argv) {
    assert(argc == 2);
    alarm(30);
    /* libc may retain resolver descriptors after its first hostname lookup. */
    struct addrinfo *resolved = NULL;
    assert(getaddrinfo("localhost", "80", NULL, &resolved) == 0);
    freeaddrinfo(resolved);
    if (strncmp(argv[1], "allocation-", 11) == 0) {
        test_allocation_failure(argv[1]);
        return 2;
    }
    if (strcmp(argv[1], "loopback") == 0) test_loopback();
    else if (strcmp(argv[1], "stream") == 0) test_stream();
    else if (strcmp(argv[1], "errors") == 0) test_errors();
    else if (strcmp(argv[1], "injected") == 0) test_injected();
    else if (strcmp(argv[1], "sigpipe") == 0) test_sigpipe();
    else if (strcmp(argv[1], "listen-on") == 0) test_listen_on();
    else if (strcmp(argv[1], "listen-on-ipv6") == 0) test_listen_on_ipv6();
    else assert(!"unknown mode");
    assert(atomic_load(&live_allocations) == 0);
    for (size_t i = 0; i < sizeof(runtime_descriptors) / sizeof(runtime_descriptors[0]); ++i)
        assert(atomic_load(&runtime_descriptors[i]) == 0);
    assert(fault_remaining == 0);
    return 0;
}
