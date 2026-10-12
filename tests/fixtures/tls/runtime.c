#define _POSIX_C_SOURCE 200809L
#include "tls.h"
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <openssl/crypto.h>
#include <openssl/ssl.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static atomic_int live_allocations;
void *bhasa_tls_test_malloc(size_t size) {
    void *pointer = malloc(size); if (pointer) atomic_fetch_add(&live_allocations, 1); return pointer;
}
void *bhasa_tls_test_realloc(void *pointer, size_t size) {
    int empty = pointer == NULL;
    void *result = realloc(pointer, size);
    if (result && empty) atomic_fetch_add(&live_allocations, 1);
    return result;
}
void bhasa_tls_test_free(void *pointer) {
    if (pointer) assert(atomic_fetch_sub(&live_allocations, 1) > 0);
    free(pointer);
}
static void *crypto_malloc(size_t size, const char *file, int line) {
    (void)file; (void)line; return bhasa_tls_test_malloc(size);
}
static void *crypto_realloc(void *pointer, size_t size, const char *file, int line) {
    (void)file; (void)line; return bhasa_tls_test_realloc(pointer, size);
}
static void crypto_free(void *pointer, const char *file, int line) {
    (void)file; (void)line; bhasa_tls_test_free(pointer);
}
static atomic_int read_entered;
static atomic_int read_delay;
static atomic_int file_delay;
static void pause_ms(long milliseconds);
int bhasa_tls_test_SSL_read_ex(SSL *ssl, void *data, size_t size, size_t *count) {
    atomic_store(&read_entered, 1);
    int delay = atomic_load(&read_delay); if (delay) pause_ms(delay);
    return SSL_read_ex(ssl, data, size, count);
}
ssize_t bhasa_tls_test_read(int fd, void *data, size_t size) {
    int delay = atomic_exchange(&file_delay, 0); if (delay) pause_ms(delay);
    return read(fd, data, size);
}
static void pause_ms(long milliseconds) {
    struct timespec time = {milliseconds / 1000, milliseconds % 1000 * 1000000};
    while (nanosleep(&time, &time) < 0) assert(errno == EINTR);
}
static bhasa_buffer *path(const char *directory, const char *file) {
    bhasa_buffer *value = bhasa_rt_buffer_from_str(directory);
    bhasa_rt_buffer_append_str(value, "/"); bhasa_rt_buffer_append_str(value, file); return value;
}
static bhasa_buffer *cert, *key, *ca, *other_ca;
static int descriptors(void) {
    int count = 0;
    for (int fd = 0; fd < 4096; ++fd) if (fcntl(fd, F_GETFD) >= 0) ++count;
    return count;
}
static bhasa_tcp_listener *listener(void) {
    bhasa_tcp_listener *value = bhasa_rt_tcp_listen(0);
    if (bhasa_rt_tcp_listener_status(value)) fprintf(stderr, "TLS fixture requires loopback permission: %lld\n", (long long)bhasa_rt_tcp_listener_status(value));
    assert(bhasa_rt_tcp_listener_status(value) == 0); return value;
}
struct accept_work { bhasa_tcp_listener *listener; bhasa_tls_socket *socket; };
static void *accept_peer(void *argument) {
    struct accept_work *work = argument;
    work->socket = bhasa_rt_tls_accept(work->listener, cert, key, 5000); return NULL;
}
static void pair(const char *identity, bhasa_buffer *trust, bhasa_tls_socket **client, bhasa_tls_socket **server) {
    bhasa_tcp_listener *acceptor = listener();
    struct accept_work work = {acceptor, NULL};
    pthread_t thread; assert(pthread_create(&thread, NULL, accept_peer, &work) == 0);
    bhasa_buffer *address = bhasa_rt_buffer_from_str("127.0.0.1");
    bhasa_buffer *name = bhasa_rt_buffer_from_str(identity);
    *client = bhasa_rt_tls_connect(address, bhasa_rt_tcp_local_port(acceptor), name, trust, 5000);
    assert(pthread_join(thread, NULL) == 0); *server = work.socket;
    bhasa_rt_buffer_free(address); bhasa_rt_buffer_free(name); bhasa_rt_tcp_listener_free(acceptor);
}
static void verified_exchange(const char *name) {
    bhasa_tls_socket *client, *server; pair(name, ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == 0 && bhasa_rt_tls_status(server) == 0);
    bhasa_tls_socket *copy = bhasa_rt_tls_socket_clone(client);
    bhasa_rt_tls_socket_free(client); client = copy;
    bhasa_buffer *data = bhasa_rt_buffer_from_str("authenticated data"), *read = bhasa_rt_buffer_from_str("old");
    assert(bhasa_rt_tls_write(client, data, 1000) == 0);
    assert(bhasa_rt_tls_read(server, read, 1024, 1000) == 0 && bhasa_rt_buffer_same(read, data));
    assert(bhasa_rt_tls_write(server, read, 1000) == 0);
    assert(bhasa_rt_tls_read(client, read, 1024, 1000) == 0 && bhasa_rt_buffer_same(read, data));
    assert(bhasa_rt_tls_shutdown(server, 1000) == 0);
    assert(bhasa_rt_tls_write(server, data, 1000) == EPIPE);
    assert(bhasa_rt_tls_read(client, read, 1024, 1000) == 0 && read->len == 0);
    assert(bhasa_rt_tls_shutdown(client, 1000) == 0);
    assert(bhasa_rt_tls_read(server, read, 1024, 1000) == 0 && read->len == 0);
    bhasa_rt_buffer_free(data); bhasa_rt_buffer_free(read);
    bhasa_rt_tls_socket_free(server); bhasa_rt_tls_socket_free(client);
}
static void trust_failures(const char *directory) {
    bhasa_tls_socket *client, *server;
    pair("wrong.example", ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == BHASA_TLS_CERTIFICATE_ERROR);
    assert(bhasa_rt_tls_status(server) != 0);
    bhasa_tls_socket *copy = bhasa_rt_tls_socket_clone(client);
    bhasa_rt_tls_socket_free(client); assert(bhasa_rt_tls_status(copy) == BHASA_TLS_CERTIFICATE_ERROR);
    bhasa_rt_tls_socket_free(copy); bhasa_rt_tls_socket_free(server);
    pair("localhost", other_ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == BHASA_TLS_CERTIFICATE_ERROR);
    assert(bhasa_rt_tls_status(server) != 0);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    pair("127.0.0.2", ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == BHASA_TLS_CERTIFICATE_ERROR);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    bhasa_buffer *original = cert; cert = path(directory, "cn.pem");
    pair("localhost", ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == BHASA_TLS_CERTIFICATE_ERROR);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    bhasa_rt_buffer_free(cert); cert = original;
}
static void timely(int64_t started) {
    int64_t elapsed = bhasa_rt_monotonic_millis() - started;
    assert(elapsed >= 0 && elapsed < 1500);
}
static void handshake_timeouts(void) {
    bhasa_tcp_listener *acceptor = listener();
    bhasa_tls_socket *failed = bhasa_rt_tls_accept(acceptor, cert, key, 0);
    assert(bhasa_rt_tls_status(failed) == ETIMEDOUT); bhasa_rt_tls_socket_free(failed);
    bhasa_tcp_socket *raw = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(acceptor), 1000);
    assert(bhasa_rt_tcp_socket_status(raw) == 0);
    int64_t started = bhasa_rt_monotonic_millis();
    failed = bhasa_rt_tls_accept(acceptor, cert, key, 20);
    assert(bhasa_rt_tls_status(failed) == ETIMEDOUT); timely(started);
    bhasa_rt_tls_socket_free(failed); bhasa_rt_tcp_socket_free(raw);
    bhasa_buffer *address = bhasa_rt_buffer_from_str("127.0.0.1"), *name = bhasa_rt_buffer_from_str("localhost");
    started = bhasa_rt_monotonic_millis();
    failed = bhasa_rt_tls_connect(address, bhasa_rt_tcp_local_port(acceptor), name, ca, 20);
    assert(bhasa_rt_tls_status(failed) == ETIMEDOUT); timely(started);
    bhasa_rt_tls_socket_free(failed);
    raw = bhasa_rt_tcp_accept_timeout(acceptor, 1000);
    assert(bhasa_rt_tcp_socket_status(raw) == 0); bhasa_rt_tcp_socket_free(raw);
    bhasa_rt_buffer_free(address); bhasa_rt_buffer_free(name); bhasa_rt_tcp_listener_free(acceptor);
}
struct read_work { bhasa_tls_socket *socket; int status; };
static void *waiting_read(void *argument) {
    struct read_work *work = argument;
    bhasa_buffer *data = bhasa_rt_buffer_new();
    work->status = (int)bhasa_rt_tls_read(work->socket, data, 10, 1000);
    assert(work->status == 0 && bhasa_rt_buffer_same_str(data, "x"));
    bhasa_rt_buffer_free(data); return NULL;
}
static void operation_timeouts_and_clones(void) {
    bhasa_tls_socket *client, *server; pair("localhost", ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == 0 && bhasa_rt_tls_status(server) == 0);
    bhasa_tls_socket *copy = bhasa_rt_tls_socket_clone(client);
    struct read_work work = {copy, -1}; atomic_store(&read_entered, 0);
    pthread_t thread; assert(pthread_create(&thread, NULL, waiting_read, &work) == 0);
    while (!atomic_load(&read_entered)) pause_ms(1);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("x"), *read = bhasa_rt_buffer_from_str("preserved");
    unsigned char *original = read->data; int64_t capacity = read->cap;
    assert(bhasa_rt_tls_read(client, read, 10, 0) == ETIMEDOUT);
    assert(bhasa_rt_tls_status(client) == 0 && read->data == original && read->cap == capacity);
    assert(bhasa_rt_tls_write(server, data, 1000) == 0);
    assert(pthread_join(thread, NULL) == 0); bhasa_rt_tls_socket_free(copy);
    int64_t started = bhasa_rt_monotonic_millis();
    assert(bhasa_rt_tls_read(client, read, 10, 20) == ETIMEDOUT); timely(started);
    assert(read->data == original && read->cap == capacity && bhasa_rt_buffer_same_str(read, "preserved"));
    assert(bhasa_rt_tls_status(client) == ETIMEDOUT);
    assert(bhasa_rt_tls_write(client, data, 1000) == ETIMEDOUT);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    pair("localhost", ca, &client, &server);
    bhasa_rt_buffer_free(data); data = bhasa_rt_buffer_new();
    data->len = data->cap = 16 * 1024 * 1024;
    data->data = bhasa_tls_test_malloc((size_t)data->len); assert(data->data); memset(data->data, 42, (size_t)data->len);
    started = bhasa_rt_monotonic_millis();
    assert(bhasa_rt_tls_write(client, data, 20) == ETIMEDOUT); timely(started);
    assert(bhasa_rt_tls_status(client) == ETIMEDOUT);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    bhasa_rt_buffer_free(data); bhasa_rt_buffer_free(read);
}
static void abrupt_close(void) {
    bhasa_tls_socket *client, *server; pair("localhost", ca, &client, &server);
    bhasa_rt_tls_socket_free(server);
    bhasa_buffer *read = bhasa_rt_buffer_from_str("preserved");
    assert(bhasa_rt_tls_read(client, read, 10, 1000) == BHASA_TLS_PROTOCOL_ERROR);
    assert(bhasa_rt_buffer_same_str(read, "preserved"));
    bhasa_rt_tls_socket_free(client); bhasa_rt_buffer_free(read);
    pair("localhost", ca, &client, &server); bhasa_rt_tls_socket_free(server);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("x");
    int status = 0;
    for (int index = 0; index < 100 && status == 0; ++index) {
        status = (int)bhasa_rt_tls_write(client, data, 1000); pause_ms(1);
    }
    assert(status != 0); bhasa_rt_tls_socket_free(client); bhasa_rt_buffer_free(data);
}
static void expired_positive_budget(void) {
    bhasa_tls_socket *client, *server; pair("localhost", ca, &client, &server);
    assert(bhasa_rt_tls_status(client) == 0 && bhasa_rt_tls_status(server) == 0);
    bhasa_buffer *data = bhasa_rt_buffer_from_str("ready"), *read = bhasa_rt_buffer_from_str("preserved");
    unsigned char *original = read->data; int64_t capacity = read->cap;
    assert(bhasa_rt_tls_write(server, data, 1000) == 0);
    atomic_store(&read_delay, 30);
    assert(bhasa_rt_tls_read(client, read, 1024, 10) == ETIMEDOUT);
    atomic_store(&read_delay, 0);
    assert(read->data == original && read->cap == capacity && bhasa_rt_buffer_same_str(read, "preserved"));
    assert(bhasa_rt_tls_status(client) == ETIMEDOUT);
    bhasa_rt_tls_socket_free(client); bhasa_rt_tls_socket_free(server);
    bhasa_rt_buffer_free(data); bhasa_rt_buffer_free(read);

    /* Local certificate work exhausting the budget must not consume a queued
     * TCP peer and begin a new handshake after the positive deadline. */
    bhasa_tcp_listener *acceptor = listener();
    bhasa_tcp_socket *raw = bhasa_rt_tcp_connect_timeout_str("127.0.0.1", bhasa_rt_tcp_local_port(acceptor), 1000);
    assert(bhasa_rt_tcp_socket_status(raw) == 0);
    atomic_store(&file_delay, 30);
    server = bhasa_rt_tls_accept(acceptor, cert, key, 10);
    assert(bhasa_rt_tls_status(server) == ETIMEDOUT); bhasa_rt_tls_socket_free(server);
    bhasa_tcp_socket *queued = bhasa_rt_tcp_accept_timeout(acceptor, 0);
    assert(bhasa_rt_tcp_socket_status(queued) == 0);
    bhasa_rt_tcp_socket_free(queued); bhasa_rt_tcp_socket_free(raw); bhasa_rt_tcp_listener_free(acceptor);
}
static void invalid_inputs(const char *directory) {
    bhasa_tcp_listener *acceptor = listener();
    bhasa_buffer *empty = bhasa_rt_buffer_new(), *encrypted = path(directory, "encrypted.key");
    bhasa_buffer *missing = path(directory, "missing.pem"), *wrong = path(directory, "other.key");
    bhasa_tls_socket *value = bhasa_rt_tls_accept(acceptor, cert, empty, 10);
    assert(bhasa_rt_tls_status(value) == EINVAL); bhasa_rt_tls_socket_free(value);
    value = bhasa_rt_tls_accept(acceptor, cert, encrypted, 10);
    assert(bhasa_rt_tls_status(value) == BHASA_TLS_CONFIGURATION_ERROR); bhasa_rt_tls_socket_free(value);
    value = bhasa_rt_tls_accept(acceptor, cert, wrong, 10);
    assert(bhasa_rt_tls_status(value) == BHASA_TLS_CONFIGURATION_ERROR); bhasa_rt_tls_socket_free(value);
    value = bhasa_rt_tls_accept(acceptor, missing, key, 10);
    assert(bhasa_rt_tls_status(value) == BHASA_TLS_CONFIGURATION_ERROR); bhasa_rt_tls_socket_free(value);
    bhasa_buffer *fifo = path(directory, "fifo");
    char *fifo_name = malloc((size_t)fifo->len + 1); assert(fifo_name);
    memcpy(fifo_name, fifo->data, (size_t)fifo->len); fifo_name[fifo->len] = 0;
    assert(mkfifo(fifo_name, 0600) == 0);
    value = bhasa_rt_tls_accept(acceptor, fifo, key, 10);
    assert(bhasa_rt_tls_status(value) == BHASA_TLS_CONFIGURATION_ERROR); bhasa_rt_tls_socket_free(value);
    assert(unlink(fifo_name) == 0); free(fifo_name); bhasa_rt_buffer_free(fifo);
    bhasa_buffer *address = bhasa_rt_buffer_from_str("127.0.0.1"), *name = bhasa_rt_buffer_from_str("localhost");
    value = bhasa_rt_tls_connect(address, 123, empty, ca, 10);
    assert(bhasa_rt_tls_status(value) == EINVAL); bhasa_rt_tls_socket_free(value);
    bhasa_rt_buffer_push(name, 0);
    value = bhasa_rt_tls_connect(address, 123, name, ca, 10);
    assert(bhasa_rt_tls_status(value) == EINVAL); bhasa_rt_tls_socket_free(value);
    value = bhasa_rt_tls_accept(acceptor, cert, key, -1);
    assert(bhasa_rt_tls_status(value) == EINVAL); bhasa_rt_tls_socket_free(value);
    value = bhasa_rt_tls_accept(acceptor, cert, key, INT64_MAX);
    assert(bhasa_rt_tls_status(value) == EINVAL); bhasa_rt_tls_socket_free(value);
    bhasa_rt_buffer_free(address); bhasa_rt_buffer_free(name);
    bhasa_rt_buffer_free(empty); bhasa_rt_buffer_free(encrypted); bhasa_rt_buffer_free(missing); bhasa_rt_buffer_free(wrong);
    bhasa_rt_tcp_listener_free(acceptor);
}
int main(int argc, char **argv) {
    assert(argc == 2); alarm(40);
    assert(CRYPTO_set_mem_functions(crypto_malloc, crypto_realloc, crypto_free) == 1);
    int before = descriptors();
    cert = path(argv[1], "server.pem"); key = path(argv[1], "server.key");
    ca = path(argv[1], "ca.pem"); other_ca = path(argv[1], "other.pem");
    invalid_inputs(argv[1]); verified_exchange("localhost"); verified_exchange("127.0.0.1");
    trust_failures(argv[1]); handshake_timeouts(); operation_timeouts_and_clones(); abrupt_close(); expired_positive_budget();
    bhasa_rt_buffer_free(cert); bhasa_rt_buffer_free(key); bhasa_rt_buffer_free(ca); bhasa_rt_buffer_free(other_ca);
    OPENSSL_cleanup();
    assert(descriptors() == before);
    assert(atomic_load(&live_allocations) == 0);
    puts("TLS verification and ownership checks passed"); return 0;
}
