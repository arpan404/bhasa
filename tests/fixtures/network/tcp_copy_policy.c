#define _DARWIN_C_SOURCE
#define _GNU_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <netinet/in.h>
#include <pthread.h>
#include <sched.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <unistd.h>

static atomic_int allocations, runtime_fds, closes, duplicates;
static int queued_fd = -1, fail_malloc, fail_socket, allocation_budget = -1;
static void *tracked_malloc(size_t size) {
    if (fail_malloc || allocation_budget == 0) return NULL;
    if (allocation_budget > 0) --allocation_budget;
    void *value = malloc(size);
    if (value) atomic_fetch_add(&allocations, 1);
    return value;
}
static void tracked_free(void *value) {
    if (value) assert(atomic_fetch_sub(&allocations, 1) > 0);
    free(value);
}
static int tracked_close(int fd) {
    assert(fd >= 0);
    assert(atomic_fetch_sub(&runtime_fds, 1) > 0);
    atomic_fetch_add(&closes, 1);
    return close(fd);
}
static int tracked_fcntl(int fd, int command, ...) {
    if (command == F_DUPFD_CLOEXEC || command == F_DUPFD) {
        atomic_fetch_add(&duplicates, 1); errno = EMFILE; return -1;
    }
    if (command == F_GETFD || command == F_GETFL) return fcntl(fd, command);
    va_list arguments; va_start(arguments, command);
    int value = va_arg(arguments, int); va_end(arguments);
    return fcntl(fd, command, value);
}
static int supplied_socket(int family, int kind, int protocol) {
    (void)family; (void)kind; (void)protocol;
    if (fail_socket) { errno = EMFILE; return -1; }
    assert(queued_fd >= 0);
    int fd = queued_fd; queued_fd = -1;
    atomic_fetch_add(&runtime_fds, 1);
    return fd;
}
static int supplied_bind(int fd, const struct sockaddr *address, socklen_t size) {
    (void)fd; (void)address; (void)size; return 0;
}
static int supplied_listen(int fd, int backlog) { (void)fd; (void)backlog; return 0; }
static int supplied_getsockname(int fd, struct sockaddr *address, socklen_t *size) {
    (void)fd; assert(*size >= sizeof(struct sockaddr_in));
    struct sockaddr_in local; memset(&local, 0, sizeof(local));
    local.sin_family = AF_INET; local.sin_port = htons(12345);
    memcpy(address, &local, sizeof(local)); *size = sizeof(local); return 0;
}
static int supplied_connect(int fd, const struct sockaddr *address, socklen_t size) {
    (void)fd; (void)address; (void)size; return 0;
}
static int supplied_accept(int fd, struct sockaddr *address, socklen_t *size) {
    (void)fd; (void)address; (void)size; return supplied_socket(0, 0, 0);
}
#if defined(__linux__)
static int supplied_accept4(int fd, struct sockaddr *address, socklen_t *size, int flags) {
    (void)flags; return supplied_accept(fd, address, size);
}
#endif
#define malloc tracked_malloc
#define free tracked_free
#define close tracked_close
#define fcntl tracked_fcntl
#define socket supplied_socket
#define bind supplied_bind
#define listen supplied_listen
#define getsockname supplied_getsockname
#define connect supplied_connect
#define accept supplied_accept
#if defined(__linux__)
#define accept4 supplied_accept4
#endif
#include "../../../runtime/net.c"
#undef malloc
#undef free
#undef close
#undef fcntl
#undef socket
#undef bind
#undef listen
#undef getsockname
#undef connect
#undef accept
#if defined(__linux__)
#undef accept4
#endif

static int prepare_pair(void) {
    int pair[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);
    assert(queued_fd == -1); queued_fd = pair[0];
    assert(fcntl(pair[1], F_SETFL, O_NONBLOCK) == 0);
    return pair[1];
}
static void failures(void) {
    bhasa_tcp_listener *listener = bhasa_rt_tcp_listen(-1);
    bhasa_tcp_listener *lc = bhasa_rt_tcp_listener_clone(listener);
    assert(bhasa_rt_tcp_listener_status(listener) == EINVAL);
    assert(bhasa_rt_tcp_listener_status(lc) == EINVAL && bhasa_rt_tcp_local_port(lc) == 0);
    bhasa_rt_tcp_listener_free(listener); bhasa_rt_tcp_listener_free(lc);
    bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_str("127.0.0.1", 0);
    bhasa_tcp_socket *sc = bhasa_rt_tcp_socket_clone(socket);
    assert(bhasa_rt_tcp_socket_status(sc) == EINVAL);
    assert(bhasa_rt_tcp_socket_fd(sc) == -1 && errno == EINVAL);
    bhasa_rt_tcp_socket_free(socket); bhasa_rt_tcp_socket_free(sc);
    fail_socket = 1;
    socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
    sc = bhasa_rt_tcp_socket_clone(socket);
    assert(bhasa_rt_tcp_socket_status(sc) == EMFILE);
    bhasa_rt_tcp_socket_free(socket); bhasa_rt_tcp_socket_free(sc);
    listener = bhasa_rt_tcp_listen(0);
    lc = bhasa_rt_tcp_listener_clone(listener);
    assert(bhasa_rt_tcp_listener_status(lc) == EMFILE);
    bhasa_rt_tcp_listener_free(listener); bhasa_rt_tcp_listener_free(lc); fail_socket = 0;
    lc = bhasa_rt_tcp_listener_clone(NULL); sc = bhasa_rt_tcp_socket_clone(NULL);
    assert(bhasa_rt_tcp_listener_status(lc) == EINVAL && bhasa_rt_tcp_socket_status(sc) == EINVAL);
    bhasa_rt_tcp_listener_free(lc); bhasa_rt_tcp_socket_free(sc);
    bhasa_rt_tcp_listener_free(NULL); bhasa_rt_tcp_socket_free(NULL);
    assert(atomic_load(&allocations) == 0 && atomic_load(&runtime_fds) == 0);
}
static void descriptor_exhaustion(void) {
    int peer = prepare_pair(); bhasa_tcp_listener *listener = bhasa_rt_tcp_listen(0);
    assert(bhasa_rt_tcp_listener_status(listener) == 0);
    assert(fcntl(listener->shared->fd, F_GETFD) & FD_CLOEXEC);
    assert(fcntl(listener->shared->fd, F_GETFL) & O_NONBLOCK);
    int socket_peer = prepare_pair(); bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
    assert(bhasa_rt_tcp_socket_status(socket) == 0);
    int accepted_peer = prepare_pair();
    bhasa_tcp_socket *accepted = bhasa_rt_tcp_accept(listener);
    assert(bhasa_rt_tcp_socket_status(accepted) == 0);
    bhasa_tcp_socket *accepted_copy = bhasa_rt_tcp_socket_clone(accepted);
    assert(bhasa_rt_tcp_socket_fd(accepted_copy) == bhasa_rt_tcp_socket_fd(accepted));
    bhasa_rt_tcp_socket_free(accepted); bhasa_rt_tcp_socket_free(accepted_copy);
    assert(close(accepted_peer) == 0);
    int fd = bhasa_rt_tcp_socket_fd(socket);
    assert(fcntl(fd, F_GETFD) & FD_CLOEXEC);
    struct rlimit original, limited; assert(getrlimit(RLIMIT_NOFILE, &original) == 0);
    limited = original; if (limited.rlim_cur > 128) limited.rlim_cur = 128;
    assert(setrlimit(RLIMIT_NOFILE, &limited) == 0);
    int fillers[128], used = 0, filler;
    while ((filler = open("/dev/null", O_RDONLY)) >= 0) {
        assert(used < 128); fillers[used++] = filler;
    }
    assert(errno == EMFILE);
    int before = atomic_load(&runtime_fds);
    for (int i = 0; i < 1000; ++i) {
        bhasa_tcp_listener *lc = bhasa_rt_tcp_listener_clone(listener);
        bhasa_tcp_socket *sc = bhasa_rt_tcp_socket_clone(socket);
        assert(bhasa_rt_tcp_listener_status(lc) == 0 && bhasa_rt_tcp_local_port(lc) == 12345);
        assert(bhasa_rt_tcp_socket_status(sc) == 0 && bhasa_rt_tcp_socket_fd(sc) == fd);
        bhasa_rt_tcp_listener_free(lc); bhasa_rt_tcp_socket_free(sc);
    }
    assert(atomic_load(&runtime_fds) == before && atomic_load(&duplicates) == 0);
    while (used) assert(close(fillers[--used]) == 0);
    assert(setrlimit(RLIMIT_NOFILE, &original) == 0);
    /* Model TLS: retain transport, borrow its fd, release every other owner. */
    bhasa_tcp_socket *transport = bhasa_rt_tcp_socket_clone(socket);
    bhasa_rt_tcp_socket_free(socket);
    assert(bhasa_rt_tcp_socket_fd(transport) == fd && fcntl(fd, F_GETFD) >= 0);
    assert(send(fd, "x", 1, 0) == 1); char byte;
    assert(recv(socket_peer, &byte, 1, 0) == 1 && byte == 'x');
    assert(recv(socket_peer, &byte, 1, 0) == -1 && (errno == EAGAIN || errno == EWOULDBLOCK));
    bhasa_rt_tcp_socket_free(transport);
    assert(recv(socket_peer, &byte, 1, 0) == 0);
    bhasa_rt_tcp_listener_free(listener); assert(close(peer) == 0); assert(close(socket_peer) == 0);
    assert(atomic_load(&allocations) == 0 && atomic_load(&runtime_fds) == 0);
}
typedef struct worker_owner { bhasa_tcp_socket *socket; bhasa_tcp_listener *listener; int status; } worker_owner;
static atomic_int start_workers;
static void *copy_worker(void *argument) {
    worker_owner *owner = argument;
    while (!atomic_load_explicit(&start_workers, memory_order_acquire)) sched_yield();
    for (int i = 0; i < 10000; ++i) {
        bhasa_tcp_socket *socket = bhasa_rt_tcp_socket_clone(owner->socket);
        bhasa_tcp_listener *listener = bhasa_rt_tcp_listener_clone(owner->listener);
        assert(bhasa_rt_tcp_socket_status(socket) == owner->status && bhasa_rt_tcp_listener_status(listener) == owner->status);
        bhasa_rt_tcp_socket_free(socket); bhasa_rt_tcp_listener_free(listener);
    }
    bhasa_rt_tcp_socket_free(owner->socket); bhasa_rt_tcp_listener_free(owner->listener);
    return NULL;
}
static void races(void) {
    for (int round = 0; round < 8; ++round) {
        int peer = prepare_pair(); bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
        int lp = prepare_pair(); bhasa_tcp_listener *listener = bhasa_rt_tcp_listen(0);
        assert(bhasa_rt_tcp_socket_status(socket) == 0 && bhasa_rt_tcp_listener_status(listener) == 0);
        worker_owner owners[4]; pthread_t workers[4]; atomic_store(&start_workers, 0);
        int before = atomic_load(&closes);
        for (int i = 0; i < 4; ++i) {
            owners[i] = (worker_owner){bhasa_rt_tcp_socket_clone(socket), bhasa_rt_tcp_listener_clone(listener), 0};
            assert(pthread_create(&workers[i], NULL, copy_worker, &owners[i]) == 0);
        }
        bhasa_rt_tcp_socket_free(socket); bhasa_rt_tcp_listener_free(listener);
        assert(atomic_load(&closes) == before);
        atomic_store_explicit(&start_workers, 1, memory_order_release);
        for (int i = 0; i < 4; ++i) assert(pthread_join(workers[i], NULL) == 0);
        assert(atomic_load(&closes) == before + 2);
        char byte; assert(recv(peer, &byte, 1, 0) == 0);
        assert(close(peer) == 0 && close(lp) == 0);
        assert(atomic_load(&allocations) == 0 && atomic_load(&runtime_fds) == 0);
    }
    /* Failed owners carry no shared state, but still have independent wrappers. */
    fail_socket = 1;
    bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
    bhasa_tcp_listener *listener = bhasa_rt_tcp_listen(0);
    fail_socket = 0;
    worker_owner owners[4]; pthread_t workers[4]; atomic_store(&start_workers, 0);
    int before = atomic_load(&closes);
    for (int i = 0; i < 4; ++i) {
        owners[i] = (worker_owner){bhasa_rt_tcp_socket_clone(socket), bhasa_rt_tcp_listener_clone(listener), EMFILE};
        assert(pthread_create(&workers[i], NULL, copy_worker, &owners[i]) == 0);
    }
    bhasa_rt_tcp_socket_free(socket); bhasa_rt_tcp_listener_free(listener);
    atomic_store_explicit(&start_workers, 1, memory_order_release);
    for (int i = 0; i < 4; ++i) assert(pthread_join(workers[i], NULL) == 0);
    assert(atomic_load(&closes) == before);
    assert(atomic_load(&allocations) == 0 && atomic_load(&runtime_fds) == 0);
}
int main(int argc, char **argv) {
    if (argc > 1) {
        if (strcmp(argv[1], "allocation") == 0) { fail_malloc = 1; (void)bhasa_rt_tcp_socket_clone(NULL); }
        if (strcmp(argv[1], "allocation-shared") == 0) {
            int peer = prepare_pair(); (void)peer; allocation_budget = 1;
            (void)bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
        }
        if (strcmp(argv[1], "allocation-success-clone") == 0) {
            int peer = prepare_pair(); (void)peer;
            bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
            fail_malloc = 1; (void)bhasa_rt_tcp_socket_clone(socket);
        }
        if (strcmp(argv[1], "overflow") == 0) {
            int peer = prepare_pair(); (void)peer;
            bhasa_tcp_socket *socket = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", 1, 1000);
            atomic_store(&socket->shared->references, UINT_MAX);
            (void)bhasa_rt_tcp_socket_clone(socket);
        }
        abort();
    }
    failures(); descriptor_exhaustion(); races();
    assert(atomic_load(&duplicates) == 0);
    puts("TCP shared copy policy passed"); return 0;
}
