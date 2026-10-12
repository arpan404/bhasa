#if !defined(__APPLE__) && !defined(__linux__)
#error "Bhasa TLS runtime supports macOS and Linux only"
#endif
#define _POSIX_C_SOURCE 200809L
#include "tls.h"
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509v3.h>
#include <poll.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#if OPENSSL_VERSION_MAJOR < 3
#error "Bhasa TLS requires OpenSSL 3 or newer"
#endif

typedef struct tls_shared {
    atomic_uint references;
    atomic_int status;
    pthread_mutex_t lock;
    bhasa_tcp_socket *transport;
    SSL *ssl;
    BIO_METHOD *method;
    int fd;
    int sent_shutdown;
} tls_shared;

struct bhasa_tls_socket { tls_shared *shared; int status; };
typedef struct tls_deadline { int64_t end; int immediate; } tls_deadline;

static int os_error(void) { return errno > 0 ? errno : EIO; }
static void *allocate(size_t size) {
    void *value = malloc(size);
    if (value == NULL) { fputs("Bhasa runtime error: allocation failed\n", stderr); exit(1); }
    return value;
}
static bhasa_tls_socket *new_socket(void) {
    bhasa_tls_socket *value = allocate(sizeof(*value));
    *value = (bhasa_tls_socket){NULL, 0};
    return value;
}
static int buffer_valid(const bhasa_buffer *value) {
    return value != NULL && value->len >= 0 && value->cap >= value->len &&
        (uint64_t)value->cap <= SIZE_MAX && (value->cap == 0 || value->data != NULL);
}
static char *text_copy(const bhasa_buffer *value, size_t maximum) {
    if (!buffer_valid(value) || value->len < 1 || (uint64_t)value->len > maximum ||
        memchr(value->data, 0, (size_t)value->len) != NULL) return NULL;
    char *text = allocate((size_t)value->len + 1);
    memcpy(text, value->data, (size_t)value->len); text[value->len] = 0;
    return text;
}
static int clock_nanos(int64_t *value) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return os_error();
    if (now.tv_sec < 0 || now.tv_nsec < 0 || now.tv_nsec >= 1000000000 ||
        (uint64_t)now.tv_sec > (uint64_t)(INT64_MAX - now.tv_nsec) / 1000000000) return EOVERFLOW;
    *value = (int64_t)now.tv_sec * 1000000000 + now.tv_nsec; return 0;
}
static int start_deadline(int64_t milliseconds, tls_deadline *deadline) {
    if (milliseconds < 0 || milliseconds > INT32_MAX) return EINVAL;
    int64_t now;
    int status = clock_nanos(&now); if (status) return status;
    if (milliseconds * 1000000 > INT64_MAX - now) return EOVERFLOW;
    deadline->end = now + milliseconds * 1000000;
    deadline->immediate = milliseconds == 0; return 0;
}
static int remaining(const tls_deadline *deadline, int *milliseconds) {
    int64_t now;
    int status = clock_nanos(&now); if (status) return status;
    if (now >= deadline->end) { *milliseconds = 0; return ETIMEDOUT; }
    *milliseconds = (int)((deadline->end - now + 999999) / 1000000); return 0;
}
/* A zero budget permits one immediate operation, but never a readiness wait.
 * Positive budgets must still be checked after non-preemptible local work. */
static int check_budget(const tls_deadline *deadline) {
    int milliseconds;
    return deadline->immediate ? 0 : remaining(deadline, &milliseconds);
}
static int wait_fd(int fd, short events, const tls_deadline *deadline) {
    for (;;) {
        int milliseconds;
        int status = remaining(deadline, &milliseconds); if (status) return status;
        struct pollfd waiter = {fd, events, 0};
        int result = poll(&waiter, 1, milliseconds);
        if (result < 0) { if (errno == EINTR) continue; return os_error(); }
        if (result == 0) return ETIMEDOUT;
        if (waiter.revents & POLLNVAL) return EBADF;
        if (waiter.revents & (events | POLLHUP | POLLERR)) return 0;
    }
}
static int lock_session(tls_shared *shared, const tls_deadline *deadline) {
    for (;;) {
        int status = pthread_mutex_trylock(&shared->lock);
        if (status == 0) {
            status = check_budget(deadline);
            if (status) (void)pthread_mutex_unlock(&shared->lock);
            return status;
        }
        if (status != EBUSY) return status;
        int milliseconds;
        status = remaining(deadline, &milliseconds); if (status) return status;
        struct timespec delay = {0, 1000000};
        if (nanosleep(&delay, NULL) != 0 && errno != EINTR) return os_error();
    }
}

/* Socket BIO callbacks never raise SIGPIPE. Each session owns its BIO method;
 * SSL owns the BIO, while the TCP handle owns the descriptor. */
static int socket_create(BIO *bio) { BIO_set_init(bio, 1); BIO_set_data(bio, NULL); return 1; }
static int socket_destroy(BIO *bio) { BIO_set_data(bio, NULL); return 1; }
static int socket_read(BIO *bio, char *destination, int limit) {
    tls_shared *shared = BIO_get_data(bio);
    BIO_clear_retry_flags(bio);
    if (shared == NULL || destination == NULL || limit <= 0) return 0;
    int result = (int)recv(shared->fd, destination, (size_t)limit, 0);
    if (result < 0 && (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR)) BIO_set_retry_read(bio);
    return result;
}
static int socket_write(BIO *bio, const char *source, int length) {
    tls_shared *shared = BIO_get_data(bio);
    BIO_clear_retry_flags(bio);
    if (shared == NULL || source == NULL || length <= 0) return 0;
#if defined(__linux__)
    int result = (int)send(shared->fd, source, (size_t)length, MSG_NOSIGNAL);
#else
    int result = (int)send(shared->fd, source, (size_t)length, 0);
#endif
    if (result < 0 && (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR)) BIO_set_retry_write(bio);
    return result;
}
static long socket_control(BIO *bio, int command, long argument, void *pointer) {
    (void)bio; (void)argument; (void)pointer;
    return command == BIO_CTRL_FLUSH ? 1 : 0;
}
static int attach_transport(tls_shared *shared) {
    shared->fd = bhasa_rt_tcp_socket_fd(shared->transport);
    if (shared->fd < 0) return os_error();
    shared->method = BIO_meth_new(BIO_TYPE_SOURCE_SINK, "Bhasa nonblocking TCP");
    if (shared->method == NULL) return ENOMEM;
    if (!BIO_meth_set_create(shared->method, socket_create) ||
        !BIO_meth_set_destroy(shared->method, socket_destroy) ||
        !BIO_meth_set_read(shared->method, socket_read) ||
        !BIO_meth_set_write(shared->method, socket_write) ||
        !BIO_meth_set_ctrl(shared->method, socket_control)) return ENOMEM;
    BIO *bio = BIO_new(shared->method);
    if (bio == NULL) return ENOMEM;
    BIO_set_data(bio, shared);
    SSL_set_bio(shared->ssl, bio, bio);
    return 0;
}
static void shared_free(tls_shared *shared) {
    SSL_free(shared->ssl);
    BIO_meth_free(shared->method);
    bhasa_rt_tcp_socket_free(shared->transport);
    (void)pthread_mutex_destroy(&shared->lock);
    free(shared);
}
static tls_shared *shared_new(void) {
    tls_shared *shared = allocate(sizeof(*shared));
    memset(shared, 0, sizeof(*shared)); shared->fd = -1;
    atomic_init(&shared->references, 1); atomic_init(&shared->status, 0);
    int status = pthread_mutex_init(&shared->lock, NULL);
    if (status != 0) { free(shared); errno = status; return NULL; }
    return shared;
}
static void clear_bytes(unsigned char *bytes, size_t size) {
    OPENSSL_cleanse(bytes, size); free(bytes);
}

/* Parse only bounded regular files, never pipes/devices or password prompts. */
static BIO *file_bio(const char *path, const tls_deadline *deadline) {
    int fd;
    for (;;) {
        fd = open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC);
        if (fd >= 0 || errno != EINTR) break;
        int milliseconds; int status = remaining(deadline, &milliseconds);
        if (status) { errno = status; return NULL; }
    }
    if (fd < 0) return NULL;
    struct stat metadata;
    if (fstat(fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) ||
        metadata.st_size <= 0 || metadata.st_size > 1048576) {
        (void)close(fd); errno = EINVAL; return NULL;
    }
    size_t size = (size_t)metadata.st_size;
    unsigned char *bytes = allocate(size);
    size_t offset = 0;
    while (offset < size) {
        ssize_t count = read(fd, bytes + offset, size - offset);
        if (count > 0) {
            offset += (size_t)count;
            if (offset < size) {
                int milliseconds; int status = remaining(deadline, &milliseconds);
                if (status) { clear_bytes(bytes, size); (void)close(fd); errno = status; return NULL; }
            }
        }
        else if (count < 0 && errno == EINTR) {
            int milliseconds; int status = remaining(deadline, &milliseconds);
            if (status) { clear_bytes(bytes, size); (void)close(fd); errno = status; return NULL; }
        } else { clear_bytes(bytes, size); (void)close(fd); errno = EIO; return NULL; }
    }
    (void)close(fd);
    /* Secure-memory BIOs cleanse their storage even when no secure heap was
     * configured by the embedding application. PEM keys must not linger. */
    BIO *bio = BIO_new(BIO_s_secmem());
    if (bio == NULL || BIO_write(bio, bytes, (int)size) != (int)size) { BIO_free(bio); bio = NULL; }
    clear_bytes(bytes, size);
    return bio;
}
static int no_password(char *buffer, int size, int writing, void *context) {
    (void)buffer; (void)size; (void)writing; (void)context; return 0;
}
static int load_trust(SSL_CTX *context, const char *path, const tls_deadline *deadline) {
    BIO *bio = file_bio(path, deadline);
    if (bio == NULL) return BHASA_TLS_CONFIGURATION_ERROR;
    STACK_OF(X509_INFO) *items = PEM_X509_INFO_read_bio(bio, NULL, no_password, NULL);
    BIO_free(bio);
    if (items == NULL) return BHASA_TLS_CONFIGURATION_ERROR;
    int count = 0, status = 0;
    X509_STORE *store = SSL_CTX_get_cert_store(context);
    for (int index = 0; index < sk_X509_INFO_num(items); ++index) {
        X509_INFO *item = sk_X509_INFO_value(items, index);
        if (item->x509 != NULL) {
            if (X509_STORE_add_cert(store, item->x509) != 1) { status = BHASA_TLS_CONFIGURATION_ERROR; break; }
            ++count;
        }
    }
    sk_X509_INFO_pop_free(items, X509_INFO_free);
    return status ? status : (count > 0 ? 0 : BHASA_TLS_CONFIGURATION_ERROR);
}
static int load_server(SSL_CTX *context, const char *cert_path, const char *key_path,
                        const tls_deadline *deadline) {
    BIO *bio = file_bio(cert_path, deadline);
    if (bio == NULL) return BHASA_TLS_CONFIGURATION_ERROR;
    X509 *cert = PEM_read_bio_X509_AUX(bio, NULL, no_password, NULL);
    int status = cert != NULL && SSL_CTX_use_certificate(context, cert) == 1 ? 0 : BHASA_TLS_CONFIGURATION_ERROR;
    X509_free(cert);
    while (status == 0 && (cert = PEM_read_bio_X509(bio, NULL, no_password, NULL)) != NULL) {
        if (SSL_CTX_add_extra_chain_cert(context, cert) != 1) { X509_free(cert); status = BHASA_TLS_CONFIGURATION_ERROR; }
    }
    BIO_free(bio); ERR_clear_error();
    if (status) return status;
    bio = file_bio(key_path, deadline);
    if (bio == NULL) return BHASA_TLS_CONFIGURATION_ERROR;
    EVP_PKEY *key = PEM_read_bio_PrivateKey(bio, NULL, no_password, NULL);
    status = key != NULL && SSL_CTX_use_PrivateKey(context, key) == 1 &&
        SSL_CTX_check_private_key(context) == 1 ? 0 : BHASA_TLS_CONFIGURATION_ERROR;
    EVP_PKEY_free(key); BIO_free(bio); return status;
}
static int identity(SSL *ssl, const char *name) {
    unsigned char address[16];
    if (inet_pton(AF_INET, name, address) == 1 || inet_pton(AF_INET6, name, address) == 1)
        return X509_VERIFY_PARAM_set1_ip_asc(SSL_get0_param(ssl), name) == 1 ? 0 : EINVAL;
    size_t size = strlen(name), label = 0;
    if (size == 0 || size > 253) return EINVAL;
    for (size_t index = 0; index < size; ++index) {
        unsigned char c = (unsigned char)name[index];
        if (c == '.') {
            if (label == 0 || name[index - 1] == '-') return EINVAL;
            label = 0;
        } else {
            if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                (c >= '0' && c <= '9') || (c == '-' && label > 0))) return EINVAL;
            if (++label > 63) return EINVAL;
        }
    }
    if (label == 0 || name[size - 1] == '-') return EINVAL;
    SSL_set_hostflags(ssl, X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS | X509_CHECK_FLAG_NEVER_CHECK_SUBJECT);
    if (SSL_set1_host(ssl, name) != 1 || SSL_set_tlsext_host_name(ssl, name) != 1) return EINVAL;
    return 0;
}
static SSL_CTX *new_context(int server) {
    SSL_CTX *context = SSL_CTX_new(server ? TLS_server_method() : TLS_client_method());
    if (context == NULL) return NULL;
    if (SSL_CTX_set_min_proto_version(context, TLS1_2_VERSION) != 1) { SSL_CTX_free(context); return NULL; }
    SSL_CTX_set_security_level(context, 2);
    SSL_CTX_set_options(context, SSL_OP_NO_COMPRESSION | SSL_OP_NO_RENEGOTIATION);
    SSL_CTX_set_mode(context, SSL_MODE_ENABLE_PARTIAL_WRITE);
    SSL_CTX_set_session_cache_mode(context, SSL_SESS_CACHE_OFF);
    if (!server) SSL_CTX_set_verify(context, SSL_VERIFY_PEER, NULL);
    return context;
}
static int ssl_failure(tls_shared *shared, int error, int saved_errno) {
    if (SSL_get_verify_result(shared->ssl) != X509_V_OK) return BHASA_TLS_CERTIFICATE_ERROR;
    if (error == SSL_ERROR_SYSCALL && saved_errno > 0) return saved_errno;
    return BHASA_TLS_PROTOCOL_ERROR;
}
static int handshake(tls_shared *shared, int server, const tls_deadline *deadline) {
    for (;;) {
        int status = check_budget(deadline); if (status) return status;
        ERR_clear_error(); errno = 0;
        int result = server ? SSL_accept(shared->ssl) : SSL_connect(shared->ssl);
        int saved_errno = errno;
        int error = SSL_get_error(shared->ssl, result);
        status = check_budget(deadline); if (status) return status;
        if (result == 1) return 0;
        if (error != SSL_ERROR_WANT_READ && error != SSL_ERROR_WANT_WRITE)
            return ssl_failure(shared, error, saved_errno);
        status = wait_fd(shared->fd, error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT, deadline);
        if (status) return status;
    }
}
static bhasa_tls_socket *finish_socket(bhasa_tls_socket *value, tls_shared *shared, int status) {
    if (status) { shared_free(shared); value->status = status; }
    else value->shared = shared;
    ERR_clear_error(); return value;
}
bhasa_tls_socket *bhasa_rt_tls_connect(const bhasa_buffer *address, int64_t port,
    const bhasa_buffer *server_name, const bhasa_buffer *ca_file, int64_t milliseconds) {
    bhasa_tls_socket *value = new_socket(); tls_deadline deadline;
    value->status = start_deadline(milliseconds, &deadline); if (value->status) return value;
    char *name = text_copy(server_name, 253), *path = text_copy(ca_file, 4096);
    if (name == NULL || path == NULL) { free(name); free(path); value->status = EINVAL; return value; }
    tls_shared *shared = shared_new();
    if (shared == NULL) { free(name); free(path); value->status = os_error(); return value; }
    SSL_CTX *context = new_context(0);
    int status = context == NULL ? BHASA_TLS_CONFIGURATION_ERROR : load_trust(context, path, &deadline);
    free(path);
    if (status == 0) { shared->ssl = SSL_new(context); if (shared->ssl == NULL) status = ENOMEM; }
    SSL_CTX_free(context);
    if (status == 0) status = identity(shared->ssl, name);
    free(name);
    if (status == 0) {
        int budget = 0; int clock_status = remaining(&deadline, &budget);
        if (clock_status != 0 && !(clock_status == ETIMEDOUT && deadline.immediate)) status = clock_status;
        else {
            shared->transport = bhasa_rt_tcp_connect_timeout(address, port, budget);
            status = (int)bhasa_rt_tcp_socket_status(shared->transport);
        }
    }
    if (status == 0) status = check_budget(&deadline);
    if (status == 0) status = attach_transport(shared);
    if (status == 0) status = handshake(shared, 0, &deadline);
    return finish_socket(value, shared, status);
}
bhasa_tls_socket *bhasa_rt_tls_accept(const bhasa_tcp_listener *listener,
    const bhasa_buffer *cert_file, const bhasa_buffer *key_file, int64_t milliseconds) {
    bhasa_tls_socket *value = new_socket(); tls_deadline deadline;
    value->status = start_deadline(milliseconds, &deadline); if (value->status) return value;
    char *cert = text_copy(cert_file, 4096), *key = text_copy(key_file, 4096);
    if (cert == NULL || key == NULL) { free(cert); free(key); value->status = EINVAL; return value; }
    tls_shared *shared = shared_new();
    if (shared == NULL) { free(cert); free(key); value->status = os_error(); return value; }
    SSL_CTX *context = new_context(1);
    int status = context == NULL ? BHASA_TLS_CONFIGURATION_ERROR : load_server(context, cert, key, &deadline);
    free(cert); free(key);
    if (status == 0) { shared->ssl = SSL_new(context); if (shared->ssl == NULL) status = ENOMEM; }
    SSL_CTX_free(context);
    if (status == 0) {
        int budget = 0; int clock_status = remaining(&deadline, &budget);
        if (clock_status != 0 && !(clock_status == ETIMEDOUT && deadline.immediate)) status = clock_status;
        else {
            shared->transport = bhasa_rt_tcp_accept_timeout(listener, budget);
            status = (int)bhasa_rt_tcp_socket_status(shared->transport);
        }
    }
    if (status == 0) status = check_budget(&deadline);
    if (status == 0) status = attach_transport(shared);
    if (status == 0) status = handshake(shared, 1, &deadline);
    return finish_socket(value, shared, status);
}
int64_t bhasa_rt_tls_status(const bhasa_tls_socket *socket) {
    if (socket == NULL) return EINVAL;
    return socket->shared == NULL ? socket->status : atomic_load(&socket->shared->status);
}
static int operation_begin(const bhasa_tls_socket *socket, int64_t milliseconds,
                            tls_deadline *deadline, tls_shared **shared) {
    int status = start_deadline(milliseconds, deadline); if (status) return status;
    status = (int)bhasa_rt_tls_status(socket); if (status) return status;
    if (socket->shared == NULL) return EINVAL;
    *shared = socket->shared;
    status = lock_session(*shared, deadline); if (status) return status;
    status = atomic_load(&(*shared)->status);
    if (status) (void)pthread_mutex_unlock(&(*shared)->lock);
    return status;
}
static int operation_end(tls_shared *shared, int status, int poison) {
    if (status && poison) atomic_store(&shared->status, status);
    (void)pthread_mutex_unlock(&shared->lock); ERR_clear_error(); return status;
}
int64_t bhasa_rt_tls_read(const bhasa_tls_socket *socket, bhasa_buffer *destination,
    int64_t limit, int64_t milliseconds) {
    if (!buffer_valid(destination) || limit < 0 || (uint64_t)limit > SIZE_MAX) return EINVAL;
    tls_deadline deadline; tls_shared *shared = NULL;
    int status = operation_begin(socket, milliseconds, &deadline, &shared); if (status) return status;
    unsigned char *bytes = limit > 0 ? malloc((size_t)limit) : NULL;
    if (limit > 0 && bytes == NULL) return operation_end(shared, ENOMEM, 0);
    size_t count = 0;
    int attempted = 0;
    while (limit > 0) {
        status = check_budget(&deadline);
        if (status) { free(bytes); return operation_end(shared, status, attempted); }
        ERR_clear_error(); errno = 0;
        attempted = 1;
        int result = SSL_read_ex(shared->ssl, bytes, (size_t)limit, &count);
        int saved_errno = errno; int error = SSL_get_error(shared->ssl, result);
        status = check_budget(&deadline);
        if (status) { free(bytes); return operation_end(shared, status, 1); }
        if (result == 1 || error == SSL_ERROR_ZERO_RETURN) break;
        if (error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE)
            status = wait_fd(shared->fd, error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT, &deadline);
        else status = ssl_failure(shared, error, saved_errno);
        if (status) { free(bytes); return operation_end(shared, status, 1); }
    }
    free(destination->data);
    if (count == 0) { free(bytes); *destination = (bhasa_buffer){0, 0, NULL}; }
    else *destination = (bhasa_buffer){(int64_t)count, limit, bytes};
    return operation_end(shared, 0, 0);
}
int64_t bhasa_rt_tls_write(const bhasa_tls_socket *socket, const bhasa_buffer *contents,
    int64_t milliseconds) {
    if (!buffer_valid(contents)) return EINVAL;
    tls_deadline deadline; tls_shared *shared = NULL;
    int status = operation_begin(socket, milliseconds, &deadline, &shared); if (status) return status;
    if (shared->sent_shutdown) return operation_end(shared, EPIPE, 0);
    size_t offset = 0;
    int attempted = 0;
    while (offset < (size_t)contents->len) {
        if (offset > 0) { int budget; status = remaining(&deadline, &budget); if (status) return operation_end(shared, status, 1); }
        status = check_budget(&deadline); if (status) return operation_end(shared, status, attempted);
        size_t count = 0;
        ERR_clear_error(); errno = 0;
        attempted = 1;
        int result = SSL_write_ex(shared->ssl, contents->data + offset, (size_t)contents->len - offset, &count);
        int saved_errno = errno; int error = SSL_get_error(shared->ssl, result);
        status = check_budget(&deadline); if (status) return operation_end(shared, status, 1);
        if (result == 1) { if (count == 0) return operation_end(shared, EIO, 1); offset += count; continue; }
        if (error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE)
            status = wait_fd(shared->fd, error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT, &deadline);
        else status = ssl_failure(shared, error, saved_errno);
        if (status) return operation_end(shared, status, 1);
    }
    return operation_end(shared, 0, 0);
}
int64_t bhasa_rt_tls_shutdown(const bhasa_tls_socket *socket, int64_t milliseconds) {
    tls_deadline deadline; tls_shared *shared = NULL;
    int status = operation_begin(socket, milliseconds, &deadline, &shared); if (status) return status;
    if (shared->sent_shutdown) return operation_end(shared, 0, 0);
    int attempted = 0;
    for (;;) {
        status = check_budget(&deadline); if (status) return operation_end(shared, status, attempted);
        ERR_clear_error(); errno = 0;
        attempted = 1;
        int result = SSL_shutdown(shared->ssl); int saved_errno = errno;
        /* SSL_get_error must immediately follow an unsuccessful SSL call. */
        int error = result < 0 ? SSL_get_error(shared->ssl, result) : SSL_ERROR_NONE;
        status = check_budget(&deadline); if (status) return operation_end(shared, status, 1);
        if (result >= 0) { shared->sent_shutdown = 1; return operation_end(shared, 0, 0); }
        if (error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE)
            status = wait_fd(shared->fd, error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT, &deadline);
        else status = ssl_failure(shared, error, saved_errno);
        if (status) return operation_end(shared, status, 1);
    }
}
bhasa_tls_socket *bhasa_rt_tls_socket_clone(const bhasa_tls_socket *socket) {
    bhasa_tls_socket *copy = new_socket();
    if (socket == NULL) { copy->status = EINVAL; return copy; }
    copy->status = socket->status; copy->shared = socket->shared;
    if (copy->shared != NULL && atomic_fetch_add(&copy->shared->references, 1) == UINT_MAX) {
        fputs("Bhasa runtime error: TLS reference count overflow\n", stderr); exit(1);
    }
    return copy;
}
void bhasa_rt_tls_socket_free(bhasa_tls_socket *socket) {
    if (socket == NULL) return;
    if (socket->shared != NULL && atomic_fetch_sub(&socket->shared->references, 1) == 1) shared_free(socket->shared);
    free(socket);
}
