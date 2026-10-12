#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#else
#define _GNU_SOURCE
#endif
#define _POSIX_C_SOURCE 200809L
#include "net.h"
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

/* White-box views let the tests check permanent descriptor flags. */
struct bhasa_tcp_socket { int fd; int status; };
struct bhasa_tcp_listener { int fd; int status; int64_t port; };
static atomic_int live_allocations;
void *bhasa_deadline_test_malloc(size_t size) {
    void *result = malloc(size); if (result != NULL) atomic_fetch_add(&live_allocations, 1); return result;
}
void *bhasa_deadline_test_realloc(void *pointer, size_t size) {
    int empty = pointer == NULL;
    void *result = realloc(pointer, size); if (result != NULL && empty) atomic_fetch_add(&live_allocations, 1); return result;
}
void bhasa_deadline_test_free(void *pointer) {
    if (pointer != NULL) assert(atomic_fetch_sub(&live_allocations, 1) > 0);
    free(pointer);
}
static void pause_ms(long milliseconds) {
    struct timespec time = {milliseconds / 1000, milliseconds % 1000 * 1000000};
    while (nanosleep(&time, &time) < 0) assert(errno == EINTR);
}
static int poll_interrupts;
static int socket_interrupts;
static int fcntl_interrupts;
static int setsockopt_interrupts;
static size_t send_limit;
static long send_delay;
static int interrupted(int *remaining) {
    if (*remaining == 0) return 0;
    --*remaining; pause_ms(2); errno = EINTR; return 1;
}
int bhasa_deadline_test_socket(int family, int type, int protocol) {
    if (interrupted(&socket_interrupts)) return -1;
    return socket(family, type, protocol);
}
int bhasa_deadline_test_fcntl(int fd, int command, ...) {
    va_list arguments;
    va_start(arguments, command);
    int argument = va_arg(arguments, int);
    va_end(arguments);
    if (interrupted(&fcntl_interrupts)) return -1;
    return fcntl(fd, command, argument);
}
int bhasa_deadline_test_setsockopt(int fd, int level, int option, const void *value, socklen_t size) {
    if (interrupted(&setsockopt_interrupts)) return -1;
    return setsockopt(fd, level, option, value, size);
}
int bhasa_deadline_test_poll(struct pollfd *items, nfds_t count, int milliseconds) {
    if (poll_interrupts > 0) {
        --poll_interrupts; pause_ms(2); errno = EINTR; return -1;
    }
    return poll(items, count, milliseconds);
}
ssize_t bhasa_deadline_test_send(int fd, const void *data, size_t count, int flags) {
    if (send_limit && count > send_limit) count = send_limit;
    if (send_delay) pause_ms(send_delay);
    return send(fd, data, count, flags);
}
static int descriptors(void) {
    int count = 0;
    for (int fd = 0; fd < 4096; ++fd) if (fcntl(fd, F_GETFD) != -1) ++count;
    return count;
}
static void nonblocking(int fd) {
    assert(fd >= 0 && (fcntl(fd, F_GETFL) & O_NONBLOCK) != 0);
    assert((fcntl(fd, F_GETFD) & FD_CLOEXEC) != 0);
}
static bhasa_tcp_listener *listener(void) {
    bhasa_tcp_listener *value = bhasa_rt_tcp_listen(0);
    if (bhasa_rt_tcp_listener_status(value)) fprintf(stderr, "loopback permission required: %lld\n", (long long)bhasa_rt_tcp_listener_status(value));
    assert(bhasa_rt_tcp_listener_status(value) == 0);
    nonblocking(value->fd);
    return value;
}
static void pair(bhasa_tcp_socket **client, bhasa_tcp_socket **server) {
    bhasa_tcp_listener *value = listener();
    *client = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(value), 500);
    assert(bhasa_rt_tcp_socket_status(*client) == 0);
    *server = bhasa_rt_tcp_accept_timeout(value, 500);
    assert(bhasa_rt_tcp_socket_status(*server) == 0);
    nonblocking((*client)->fd); nonblocking((*server)->fd);
    bhasa_rt_tcp_listener_free(value);
}
static void timely(int64_t started) {
    int64_t elapsed = bhasa_rt_monotonic_millis() - started;
    assert(elapsed >= 0 && elapsed < 1000);
}
static void setup_interruptions(void) {
    bhasa_tcp_listener *value = listener();
    int *counters[] = {&socket_interrupts, &fcntl_interrupts,
#if defined(__APPLE__)
        &setsockopt_interrupts,
#endif
    };
    for (size_t index = 0; index < sizeof(counters) / sizeof(counters[0]); ++index) {
        *counters[index] = 100;
        int64_t started = bhasa_rt_monotonic_millis();
        bhasa_tcp_socket *failed = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(value), 10);
        assert(bhasa_rt_tcp_socket_status(failed) == ETIMEDOUT); timely(started);
        assert(*counters[index] > 0 && *counters[index] < 100);
        *counters[index] = 0;
        bhasa_rt_tcp_socket_free(failed);
    }
    bhasa_tcp_socket *client = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(value), 500);
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    fcntl_interrupts = 100;
    bhasa_tcp_socket *failed = bhasa_rt_tcp_accept_timeout(value, 10);
    assert(bhasa_rt_tcp_socket_status(failed) == ETIMEDOUT);
    assert(fcntl_interrupts > 0 && fcntl_interrupts < 100); fcntl_interrupts = 0;
    bhasa_rt_tcp_socket_free(failed); bhasa_rt_tcp_socket_free(client);
    bhasa_rt_tcp_listener_free(value);
}
static void timeout_and_progress(void) {
    bhasa_tcp_listener *value = listener();
    int64_t started = bhasa_rt_monotonic_millis();
    bhasa_tcp_socket *failed = bhasa_rt_tcp_accept_timeout(value, 20);
    assert(bhasa_rt_tcp_socket_status(failed) == ETIMEDOUT); timely(started);
    bhasa_rt_tcp_socket_free(failed);
    failed = bhasa_rt_tcp_accept_timeout(value, 0);
    assert(bhasa_rt_tcp_socket_status(failed) == ETIMEDOUT); bhasa_rt_tcp_socket_free(failed);
    failed = bhasa_rt_tcp_accept_timeout(value, -1);
    assert(bhasa_rt_tcp_socket_status(failed) == EINVAL); bhasa_rt_tcp_socket_free(failed);
    failed = bhasa_rt_tcp_accept_timeout(value, INT64_MAX);
    assert(bhasa_rt_tcp_socket_status(failed) == EINVAL); bhasa_rt_tcp_socket_free(failed);
    bhasa_rt_tcp_listener_free(value);
    bhasa_tcp_socket *client, *server; pair(&client, &server);
    bhasa_buffer *read = bhasa_rt_buffer_from_str("preserved");
    unsigned char *original = read->data; int64_t capacity = read->cap;
    started = bhasa_rt_monotonic_millis();
    assert(bhasa_rt_tcp_read_timeout(server, read, 8, 20) == ETIMEDOUT); timely(started);
    assert(read->data == original && read->cap == capacity && bhasa_rt_buffer_same_str(read, "preserved"));
    assert(bhasa_rt_tcp_read_timeout(server, read, 8, 0) == ETIMEDOUT);
    assert(bhasa_rt_tcp_read_timeout(server, read, 8, -1) == EINVAL);
    poll_interrupts = 100;
    started = bhasa_rt_monotonic_millis();
    assert(bhasa_rt_tcp_read_timeout(server, read, 8, 20) == ETIMEDOUT); timely(started);
    assert(poll_interrupts > 0 && poll_interrupts < 100); poll_interrupts = 0;
    bhasa_buffer *data = bhasa_rt_buffer_from_str("ready");
    assert(bhasa_rt_tcp_write_timeout(client, data, 0) == 0);
    struct pollfd ready = {server->fd, POLLIN, 0};
    assert(poll(&ready, 1, 1000) == 1);
    assert(bhasa_rt_tcp_read_timeout(server, read, 8, 0) == 0 && bhasa_rt_buffer_same_str(read, "ready"));
    assert(bhasa_rt_tcp_read_timeout(server, read, 0, 0) == 0 && read->len == 0);
    bhasa_rt_buffer_free(data);
    data = bhasa_rt_buffer_new();
    for (int index = 0; index < 128; ++index) bhasa_rt_buffer_push(data, index);
    send_limit = 1; send_delay = 2;
    assert(bhasa_rt_tcp_write_timeout(client, data, 10) == ETIMEDOUT);
    send_limit = 0; send_delay = 0;
    assert(bhasa_rt_tcp_read_timeout(server, read, 128, 100) == 0 && read->len > 0 && read->len < 128);
    /* Backpressure is bounded even when send cannot accept any more bytes. */
    int size = 4096; assert(setsockopt(client->fd, SOL_SOCKET, SO_SNDBUF, &size, sizeof(size)) == 0);
    bhasa_rt_buffer_free(data); data = bhasa_rt_buffer_new();
    data->len = data->cap = 8 * 1024 * 1024;
    data->data = bhasa_deadline_test_malloc((size_t)data->len); assert(data->data);
    memset(data->data, 42, (size_t)data->len);
    started = bhasa_rt_monotonic_millis();
    assert(bhasa_rt_tcp_write_timeout(client, data, 20) == ETIMEDOUT); timely(started);
    nonblocking(client->fd); nonblocking(server->fd);
    bhasa_rt_buffer_free(data); bhasa_rt_buffer_free(read);
    bhasa_rt_tcp_socket_free(client); bhasa_rt_tcp_socket_free(server);
}

struct racing {
    bhasa_tcp_socket *socket;
    bhasa_tcp_listener *listener;
    atomic_int *ready;
    atomic_int *go;
    int status;
};
static void *race(void *argument) {
    struct racing *work = argument;
    atomic_fetch_add(work->ready, 1);
    while (!atomic_load(work->go)) pause_ms(1);
    if (work->listener) {
        bhasa_tcp_socket *socket = bhasa_rt_tcp_accept_timeout(work->listener, 30);
        work->status = (int)bhasa_rt_tcp_socket_status(socket);
        bhasa_rt_tcp_socket_free(socket);
    } else {
        bhasa_buffer *data = bhasa_rt_buffer_new();
        work->status = (int)bhasa_rt_tcp_read_timeout(work->socket, data, 1, 30);
        if (work->status == 0) assert(data->len == 1 && data->data[0] == 'x');
        bhasa_rt_buffer_free(data);
    }
    return NULL;
}
static void run_race(struct racing *left, struct racing *right) {
    atomic_int ready = 0, go = 0;
    left->ready = right->ready = &ready; left->go = right->go = &go;
    pthread_t a, b;
    assert(pthread_create(&a, NULL, race, left) == 0);
    assert(pthread_create(&b, NULL, race, right) == 0);
    while (atomic_load(&ready) != 2) pause_ms(1);
    atomic_store(&go, 1);
    assert(pthread_join(a, NULL) == 0 && pthread_join(b, NULL) == 0);
    assert((left->status == 0 && right->status == ETIMEDOUT) || (right->status == 0 && left->status == ETIMEDOUT));
}
static void clone_races(void) {
    for (int iteration = 0; iteration < 10; ++iteration) {
        bhasa_tcp_socket *client, *server; pair(&client, &server);
        bhasa_tcp_socket *copy = bhasa_rt_tcp_socket_clone(server);
        assert(bhasa_rt_tcp_socket_status(copy) == 0);
        bhasa_buffer *data = bhasa_rt_buffer_from_str("x");
        assert(bhasa_rt_tcp_write(client, data) == 0);
        struct racing left = {.socket = server}, right = {.socket = copy};
        run_race(&left, &right);
        nonblocking(server->fd); nonblocking(copy->fd);
        bhasa_rt_buffer_free(data); bhasa_rt_tcp_socket_free(client);
        bhasa_rt_tcp_socket_free(server); bhasa_rt_tcp_socket_free(copy);
        bhasa_tcp_listener *value = listener(), *other = bhasa_rt_tcp_listener_clone(value);
        client = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(value), 100);
        assert(bhasa_rt_tcp_socket_status(client) == 0);
        left = (struct racing){.listener = value}; right = (struct racing){.listener = other};
        run_race(&left, &right);
        nonblocking(value->fd); nonblocking(other->fd);
        bhasa_rt_tcp_listener_free(value); bhasa_rt_tcp_listener_free(other); bhasa_rt_tcp_socket_free(client);
    }
}
static void invalid_inputs(void) {
    const char *hosts[] = {"localhost", "", "127.1", " 127.0.0.1", "[::1]", "::1%lo0"};
    for (size_t index = 0; index < sizeof(hosts)/sizeof(hosts[0]); ++index) {
        bhasa_tcp_socket *value = bhasa_rt_tcp_connect_timeout_str(hosts[index], 80, 20);
        assert(bhasa_rt_tcp_socket_status(value) == EINVAL); bhasa_rt_tcp_socket_free(value);
    }
    bhasa_tcp_socket *value = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 0, 20);
    assert(bhasa_rt_tcp_socket_status(value) == EINVAL); bhasa_rt_tcp_socket_free(value);
    bhasa_buffer *host = bhasa_rt_buffer_from_str("127.0.0.1"); bhasa_rt_buffer_push(host, 0);
    value = bhasa_rt_tcp_connect_timeout(host, 80, 20);
    assert(bhasa_rt_tcp_socket_status(value) == EINVAL); bhasa_rt_tcp_socket_free(value); bhasa_rt_buffer_free(host);
}
static void ipv6_deadlines(void) {
    bhasa_tcp_listener *value = bhasa_rt_tcp_listen_on_str("::1", 0);
    int status = (int)bhasa_rt_tcp_listener_status(value);
    if (status == EAFNOSUPPORT || status == EPROTONOSUPPORT || status == EADDRNOTAVAIL) {
        printf("SKIP IPv6 deadline loopback: errno=%d\n", status);
        bhasa_rt_tcp_listener_free(value);
        return;
    }
    assert(status == 0);
    bhasa_tcp_socket *client = bhasa_rt_tcp_connect_timeout_str("::1", bhasa_rt_tcp_local_port(value), 500);
    assert(bhasa_rt_tcp_socket_status(client) == 0);
    bhasa_tcp_socket *server = bhasa_rt_tcp_accept_timeout(value, 500);
    assert(bhasa_rt_tcp_socket_status(server) == 0);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("x"), *read = bhasa_rt_buffer_new();
    assert(bhasa_rt_tcp_write_timeout(client, data, 500) == 0);
    assert(bhasa_rt_tcp_read_timeout(server, read, 1, 500) == 0 && bhasa_rt_buffer_same_str(read, "x"));
    bhasa_rt_tcp_socket_free(client);
    assert(bhasa_rt_tcp_read_timeout(server, read, 1, 500) == 0 && read->len == 0);
    bhasa_rt_buffer_free(data); bhasa_rt_buffer_free(read);
    bhasa_rt_tcp_socket_free(server); bhasa_rt_tcp_listener_free(value);
}
int main(void) {
    alarm(30);
    int before = descriptors();
    invalid_inputs(); setup_interruptions(); timeout_and_progress(); clone_races(); ipv6_deadlines();
    assert(descriptors() == before && atomic_load(&live_allocations) == 0);
    puts("TCP deadline and clone-race checks passed");
    return 0;
}
