#if !defined(_WIN32)
#define _POSIX_C_SOURCE 200809L
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif

#include "core.h"

#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(__linux__)
#include <stdio_ext.h>
#endif
#if defined(__APPLE__)
#include <paths.h>
#endif

#if !defined(_WIN32)
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <spawn.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
extern char **environ;
#endif

static int runtime_argc;
static char **runtime_argv;

static _Noreturn void fail(const char *message) {
    fprintf(stderr, "Bhasa runtime error: %s\n", message);
    exit(1);
}

static void *allocate(size_t size) {
    void *result = malloc(size);
    if (result == NULL) fail("allocation failed");
    return result;
}

static int64_t checked_length(size_t length) {
    if (length > INT64_MAX) fail("capacity overflow");
    return (int64_t)length;
}

static int64_t combined_length(int64_t left, int64_t right) {
    if (left < 0 || right < 0 || left > INT64_MAX - right)
        fail("capacity overflow");
    return left + right;
}

static size_t allocation_size(int64_t capacity, size_t element_size) {
    if (capacity < 0 || (uint64_t)capacity > SIZE_MAX / element_size)
        fail("capacity overflow");
    return (size_t)capacity * element_size;
}

static int64_t grown_capacity(int64_t current, int64_t needed, size_t element_size) {
    allocation_size(needed, element_size);
    int64_t next = current < 16 ? 16 : current;
    while (next < needed) {
        if (next > INT64_MAX / 2 || (uint64_t)next > SIZE_MAX / element_size / 2)
            return needed;
        next *= 2;
    }
    if ((uint64_t)next > SIZE_MAX / element_size) return needed;
    return next;
}

static void buffer_valid(const bhasa_buffer *value) {
    if (value == NULL || value->len < 0 || value->cap < value->len ||
        (value->cap != 0 && value->data == NULL)) fail("invalid buffer");
}

static void ints_valid(const bhasa_ints *value) {
    if (value == NULL || value->len < 0 || value->cap < value->len ||
        (value->cap != 0 && value->data == NULL)) fail("invalid ints");
}

static void buffer_reserve(bhasa_buffer *value, int64_t needed) {
    buffer_valid(value);
    if (needed <= value->cap) return;
    int64_t capacity = grown_capacity(value->cap, needed, 1);
    void *data = realloc(value->data, allocation_size(capacity, 1));
    if (data == NULL) fail("allocation failed");
    value->data = data;
    value->cap = capacity;
}

static void ints_reserve(bhasa_ints *value, int64_t needed) {
    ints_valid(value);
    if (needed <= value->cap) return;
    int64_t capacity = grown_capacity(value->cap, needed, sizeof(int64_t));
    void *data = realloc(value->data, allocation_size(capacity, sizeof(int64_t)));
    if (data == NULL) fail("allocation failed");
    value->data = data;
    value->cap = capacity;
}

static void check_index(int64_t index, int64_t length) {
    if (index < 0 || index >= length) fail("index out of bounds");
}

static unsigned char checked_byte(int64_t byte) {
    if (byte < 0 || byte > 255) fail("byte must be in 0..255");
    return (unsigned char)byte;
}

bhasa_buffer *bhasa_rt_buffer_new(void) {
    bhasa_buffer *value = allocate(sizeof(*value));
    *value = (bhasa_buffer){0, 0, NULL};
    return value;
}

bhasa_buffer *bhasa_rt_buffer_from_str(const char *text) {
    if (text == NULL) fail("invalid string");
    bhasa_buffer *value = bhasa_rt_buffer_new();
    bhasa_rt_buffer_append_str(value, text);
    return value;
}

bhasa_buffer *bhasa_rt_buffer_clone(const bhasa_buffer *value) {
    buffer_valid(value);
    bhasa_buffer *copy = bhasa_rt_buffer_new();
    bhasa_rt_buffer_append(copy, value);
    return copy;
}

void bhasa_rt_buffer_free(bhasa_buffer *value) {
    if (value == NULL) return;
    free(value->data);
    free(value);
}

int64_t bhasa_rt_buffer_len(const bhasa_buffer *value) {
    buffer_valid(value);
    return value->len;
}

int64_t bhasa_rt_buffer_get(const bhasa_buffer *value, int64_t index) {
    buffer_valid(value);
    check_index(index, value->len);
    return value->data[index];
}

void bhasa_rt_buffer_set(bhasa_buffer *value, int64_t index, int64_t byte) {
    buffer_valid(value);
    check_index(index, value->len);
    value->data[index] = checked_byte(byte);
}

void bhasa_rt_buffer_push(bhasa_buffer *value, int64_t byte) {
    buffer_valid(value);
    unsigned char element = checked_byte(byte);
    int64_t length = combined_length(value->len, 1);
    buffer_reserve(value, length);
    value->data[value->len] = element;
    value->len = length;
}

void bhasa_rt_buffer_append(bhasa_buffer *destination, const bhasa_buffer *source) {
    buffer_valid(destination);
    buffer_valid(source);
    int64_t count = source->len;
    int64_t length = combined_length(destination->len, count);
    buffer_reserve(destination, length);
    /* Fetch source->data after realloc: source may be destination itself. */
    if (count != 0)
        memmove(destination->data + destination->len, source->data, (size_t)count);
    destination->len = length;
}

void bhasa_rt_buffer_append_str(bhasa_buffer *destination, const char *source) {
    buffer_valid(destination);
    if (source == NULL) fail("invalid string");
    int64_t count = checked_length(strlen(source));
    int64_t length = combined_length(destination->len, count);
    buffer_reserve(destination, length);
    if (count != 0) memcpy(destination->data + destination->len, source, (size_t)count);
    destination->len = length;
}

bhasa_buffer *bhasa_rt_buffer_slice(const bhasa_buffer *value, int64_t start, int64_t count) {
    buffer_valid(value);
    if (start < 0 || count < 0 || start > value->len || count > value->len - start)
        fail("slice out of bounds");
    bhasa_buffer *result = bhasa_rt_buffer_new();
    buffer_reserve(result, count);
    if (count != 0) memcpy(result->data, value->data + start, (size_t)count);
    result->len = count;
    return result;
}

int64_t bhasa_rt_buffer_same(const bhasa_buffer *left, const bhasa_buffer *right) {
    buffer_valid(left);
    buffer_valid(right);
    return left->len == right->len &&
           (left->len == 0 || memcmp(left->data, right->data, (size_t)left->len) == 0);
}

int64_t bhasa_rt_buffer_same_str(const bhasa_buffer *left, const char *right) {
    buffer_valid(left);
    if (right == NULL) fail("invalid string");
    int64_t length = checked_length(strlen(right));
    return left->len == length &&
           (length == 0 || memcmp(left->data, right, (size_t)length) == 0);
}

bhasa_buffer *bhasa_rt_to_buffer(int64_t value) {
    char digits[32];
    int count = snprintf(digits, sizeof(digits), "%" PRId64, value);
    if (count < 0 || (size_t)count >= sizeof(digits)) fail("integer formatting failed");
    return bhasa_rt_buffer_from_str(digits);
}

bhasa_ints *bhasa_rt_ints_new(void) {
    bhasa_ints *value = allocate(sizeof(*value));
    *value = (bhasa_ints){0, 0, NULL};
    return value;
}

bhasa_ints *bhasa_rt_ints_clone(const bhasa_ints *value) {
    ints_valid(value);
    bhasa_ints *copy = bhasa_rt_ints_new();
    ints_reserve(copy, value->len);
    if (value->len != 0)
        memcpy(copy->data, value->data, allocation_size(value->len, sizeof(int64_t)));
    copy->len = value->len;
    return copy;
}

void bhasa_rt_ints_free(bhasa_ints *value) {
    if (value == NULL) return;
    free(value->data);
    free(value);
}

int64_t bhasa_rt_ints_len(const bhasa_ints *value) {
    ints_valid(value);
    return value->len;
}

int64_t bhasa_rt_ints_get(const bhasa_ints *value, int64_t index) {
    ints_valid(value);
    check_index(index, value->len);
    return value->data[index];
}

void bhasa_rt_ints_set(bhasa_ints *value, int64_t index, int64_t element) {
    ints_valid(value);
    check_index(index, value->len);
    value->data[index] = element;
}

void bhasa_rt_ints_push(bhasa_ints *value, int64_t element) {
    ints_valid(value);
    int64_t length = combined_length(value->len, 1);
    ints_reserve(value, length);
    value->data[value->len] = element;
    value->len = length;
}

void bhasa_rt_init(int argc, char **argv) {
    if (argc < 0 || (argc != 0 && argv == NULL)) fail("invalid process arguments");
    runtime_argc = argc;
    runtime_argv = argv;
}

int64_t bhasa_rt_arg_count(void) { return runtime_argc; }

bhasa_buffer *bhasa_rt_arg(int64_t index) {
    check_index(index, runtime_argc);
    return bhasa_rt_buffer_from_str(runtime_argv[index]);
}

static int io_error(void) { return errno == 0 ? EIO : errno; }

static int read_stream(FILE *stream, bhasa_buffer *destination, int close_stream) {
    buffer_valid(destination);
    bhasa_buffer *incoming = bhasa_rt_buffer_new();
    unsigned char chunk[8192];
    int status = 0;
    for (;;) {
        errno = 0;
        size_t count = fread(chunk, 1, sizeof(chunk), stream);
        if (count != 0) {
            int64_t length = combined_length(incoming->len, (int64_t)count);
            buffer_reserve(incoming, length);
            memcpy(incoming->data + incoming->len, chunk, count);
            incoming->len = length;
        }
        if (ferror(stream)) {
            status = io_error();
            break;
        }
        if (feof(stream)) break;
        if (count == 0) {
            status = EIO;
            break;
        }
    }
    if (close_stream && fclose(stream) != 0 && status == 0) status = io_error();
    if (status == 0) {
        unsigned char *old_data = destination->data;
        *destination = *incoming;
        incoming->data = old_data;
    }
    bhasa_rt_buffer_free(incoming);
    return status;
}

/* A fresh terminator prevents path access beyond the buffer. */
static char *path_string(const bhasa_buffer *path) {
    buffer_valid(path);
    if (path->len != 0 && memchr(path->data, 0, (size_t)path->len) != NULL) return NULL;
    int64_t size = combined_length(path->len, 1);
    char *text = allocate(allocation_size(size, 1));
    if (path->len != 0) memcpy(text, path->data, (size_t)path->len);
    text[path->len] = 0;
    return text;
}

int64_t bhasa_rt_read_file_str(const char *path, bhasa_buffer *destination) {
    buffer_valid(destination);
    if (path == NULL) return EINVAL;
    FILE *stream = fopen(path, "rb");
    if (stream == NULL) return io_error();
    return read_stream(stream, destination, 1);
}

int64_t bhasa_rt_read_file(const bhasa_buffer *path, bhasa_buffer *destination) {
    char *text = path_string(path);
    if (text == NULL) return EINVAL;
    int64_t status = bhasa_rt_read_file_str(text, destination);
    free(text);
    return status;
}

typedef struct pipe_guard {
    int pending_before;
#if !defined(_WIN32)
    sigset_t previous_mask;
    sigset_t signals;
#endif
} pipe_guard;

static int pipe_guard_begin(pipe_guard *guard) {
#if !defined(_WIN32)
    sigemptyset(&guard->signals);
    sigaddset(&guard->signals, SIGPIPE);
    int error = pthread_sigmask(SIG_BLOCK, &guard->signals, &guard->previous_mask);
    if (error != 0) return error;
    sigset_t pending;
    if (sigpending(&pending) != 0) {
        error = io_error();
        pthread_sigmask(SIG_SETMASK, &guard->previous_mask, NULL);
        return error;
    }
    guard->pending_before = sigismember(&pending, SIGPIPE);
#else
    (void)guard;
#endif
    return 0;
}

static int pipe_guard_end(pipe_guard *guard, int saw_pipe_error, int status) {
#if !defined(_WIN32)
    /* Leave a caller's pending signal alone. Only a write's new SIGPIPE is ours. */
    if (saw_pipe_error && !guard->pending_before) {
#if defined(__APPLE__)
        sigset_t pending;
        if (sigpending(&pending) != 0) {
            status = io_error();
        } else if (sigismember(&pending, SIGPIPE)) {
            /* Ignored signals are never pending; waiting for one would hang. */
            int signal_number;
            int error;
            do { error = sigwait(&guard->signals, &signal_number); } while (error == EINTR);
            if (error != 0) status = error;
        }
#else
        struct timespec timeout = {0, 0};
        int result;
        do { result = sigtimedwait(&guard->signals, NULL, &timeout); }
        while (result == -1 && errno == EINTR);
        if (result == -1 && errno != EAGAIN) status = io_error();
#endif
    }
    int error = pthread_sigmask(SIG_SETMASK, &guard->previous_mask, NULL);
    if (status == 0) status = error;
#else
    (void)guard;
    (void)saw_pipe_error;
#endif
    return status;
}

static int write_stream(FILE *stream, const bhasa_buffer *contents) {
    buffer_valid(contents);
    size_t written = 0;
    while (written < (size_t)contents->len) {
        errno = 0;
        size_t count = fwrite(contents->data + written, 1, (size_t)contents->len - written, stream);
        written += count;
        if (ferror(stream)) return io_error();
        if (count == 0) return EIO;
    }
    return 0;
}

static int write_and_finish(FILE *stream, const bhasa_buffer *contents, int close_stream,
                            int newline) {
    buffer_valid(contents);
    pipe_guard guard;
    int status = pipe_guard_begin(&guard);
    if (status != 0) {
        if (close_stream) fclose(stream);
        return status;
    }
#if !defined(_WIN32)
    if (!close_stream) flockfile(stream);
#endif
    status = write_stream(stream, contents);
    if (status == 0 && newline) {
        errno = 0;
        if (fputc('\n', stream) == EOF) status = io_error();
    }
    int saw_pipe_error = status == EPIPE;
    errno = 0;
    if ((close_stream ? fclose(stream) : fflush(stream)) != 0) {
        int error = io_error();
        saw_pipe_error |= error == EPIPE;
        if (status == 0) status = error;
    }
    /* A failed flush can retain bytes and retry them during process exit. */
    if (!close_stream && saw_pipe_error) {
#if defined(__APPLE__)
        fpurge(stream);
#elif defined(__linux__)
        __fpurge(stream);
#endif
    }
#if !defined(_WIN32)
    if (!close_stream) funlockfile(stream);
#endif
    return pipe_guard_end(&guard, saw_pipe_error, status);
}

int64_t bhasa_rt_write_file_str(const char *path, const bhasa_buffer *contents) {
    buffer_valid(contents);
    if (path == NULL) return EINVAL;
    FILE *stream = fopen(path, "wb");
    if (stream == NULL) return io_error();
    return write_and_finish(stream, contents, 1, 0);
}

int64_t bhasa_rt_write_file(const bhasa_buffer *path, const bhasa_buffer *contents) {
    char *text = path_string(path);
    if (text == NULL) return EINVAL;
    int64_t status = bhasa_rt_write_file_str(text, contents);
    free(text);
    return status;
}

int64_t bhasa_rt_read_stdin(bhasa_buffer *destination) {
    return read_stream(stdin, destination, 0);
}

int64_t bhasa_rt_write_stdout(const bhasa_buffer *contents) {
    return write_and_finish(stdout, contents, 0, 0);
}

int64_t bhasa_rt_write_stderr(const bhasa_buffer *contents) {
    return write_and_finish(stderr, contents, 0, 0);
}

void bhasa_rt_print_buffer(const bhasa_buffer *contents) {
    if (write_and_finish(stdout, contents, 0, 1) != 0)
        fail("cannot print buffer");
}

void bhasa_rt_args_push(bhasa_buffer *arguments, const bhasa_buffer *value) {
    buffer_valid(arguments);
    buffer_valid(value);
    if (value->len != 0 && memchr(value->data, 0, (size_t)value->len) != NULL)
        fail("process argument contains NUL");
    int64_t old_length = arguments->len;
    int64_t count = value->len;
    int64_t length = combined_length(combined_length(old_length, count), 1);
    buffer_reserve(arguments, length);
    if (count != 0) memmove(arguments->data + old_length, value->data, (size_t)count);
    arguments->data[length - 1] = 0;
    arguments->len = length;
}

void bhasa_rt_args_push_str(bhasa_buffer *arguments, const char *value) {
    buffer_valid(arguments);
    if (value == NULL) fail("invalid string");
    int64_t count = checked_length(strlen(value));
    int64_t length = combined_length(combined_length(arguments->len, count), 1);
    buffer_reserve(arguments, length);
    memcpy(arguments->data + arguments->len, value, (size_t)count + 1);
    arguments->len = length;
}

#if defined(__APPLE__)
static int spawn_without_shell(pid_t *child, char **argv,
                               const posix_spawn_file_actions_t *actions,
                               const posix_spawnattr_t *attributes) {
    if (strchr(argv[0], '/') != NULL)
        return posix_spawn(child, argv[0], actions, attributes, argv, environ);

    /* Darwin's posix_spawnp falls back to /bin/sh on ENOEXEC. Search PATH
       ourselves so an unrecognized executable is an error, never shell text. */
    const char *directory = getenv("PATH");
    if (directory == NULL) directory = _PATH_DEFPATH;
    size_t name_length = strlen(argv[0]);
    int denied = 0;
    for (;;) {
        const char *end = strchr(directory, ':');
        size_t length = end == NULL ? strlen(directory) : (size_t)(end - directory);
        const char *prefix = length == 0 ? "." : directory;
        if (length == 0) length = 1;
        char candidate[PATH_MAX];
        if (length > sizeof(candidate) - 2 || name_length > sizeof(candidate) - length - 2)
            return ENAMETOOLONG;
        memcpy(candidate, prefix, length);
        candidate[length] = '/';
        memcpy(candidate + length + 1, argv[0], name_length + 1);
        int error = posix_spawn(child, candidate, actions, attributes, argv, environ);
        if (error == EACCES) denied = 1;
        else if (error != ENOENT && error != ENOTDIR && error != ELOOP && error != ENAMETOOLONG)
            return error;
        if (end == NULL) return denied ? EACCES : ENOENT;
        directory = end + 1;
    }
}

static int spawn_child(pid_t *child, char **argv) {
    posix_spawnattr_t attributes;
    int error = posix_spawnattr_init(&attributes);
    if (error != 0) return error;
    error = posix_spawnattr_setflags(&attributes, POSIX_SPAWN_CLOEXEC_DEFAULT);
    if (error != 0) {
        posix_spawnattr_destroy(&attributes);
        return error;
    }
    posix_spawn_file_actions_t actions;
    error = posix_spawn_file_actions_init(&actions);
    if (error != 0) {
        posix_spawnattr_destroy(&attributes);
        return error;
    }
    /* Close other descriptors atomically at spawn, including sockets/files
       another worker has opened but has not yet marked close-on-exec. */
    for (int descriptor = STDIN_FILENO; descriptor <= STDERR_FILENO; ++descriptor) {
        int flags;
        do { flags = fcntl(descriptor, F_GETFD); } while (flags == -1 && errno == EINTR);
        if (flags == -1) {
            if (errno == EBADF) continue;
            error = io_error();
            break;
        }
        /* Preserve the caller's closed/close-on-exec standard streams. */
        if ((flags & FD_CLOEXEC) != 0) continue;
        error = posix_spawn_file_actions_addinherit_np(&actions, descriptor);
        if (error != 0) break;
    }
    if (error == 0)
        error = spawn_without_shell(child, argv, &actions, &attributes);
    posix_spawn_file_actions_destroy(&actions);
    posix_spawnattr_destroy(&attributes);
    return error;
}
#endif

int64_t bhasa_rt_process_run(const bhasa_buffer *arguments) {
    buffer_valid(arguments);
    if (arguments->len == 0 || arguments->data[0] == 0 || arguments->data[arguments->len - 1] != 0)
        return -EINVAL;
#if defined(_WIN32)
    return -ENOSYS;
#else
    size_t count = 0;
    for (int64_t index = 0; index < arguments->len; ++index)
        if (arguments->data[index] == 0) ++count;
    if (count > SIZE_MAX / sizeof(char *) - 1) return -E2BIG;
    char **argv = allocate((count + 1) * sizeof(char *));
    size_t next = 0;
    argv[next++] = (char *)arguments->data;
    for (int64_t index = 0; index < arguments->len - 1; ++index)
        if (arguments->data[index] == 0) argv[next++] = (char *)arguments->data + index + 1;
    argv[count] = NULL;
    pid_t child;
#if defined(__APPLE__)
    int error = spawn_child(&child, argv);
#else
    int error = posix_spawnp(&child, argv[0], NULL, NULL, argv, environ);
#endif
    free(argv);
    if (error != 0) return -(int64_t)error;
    int status;
    pid_t result;
    do { result = waitpid(child, &status, 0); } while (result == -1 && errno == EINTR);
    if (result == -1) return -(int64_t)io_error();
    if (WIFEXITED(status)) return WEXITSTATUS(status);
    if (WIFSIGNALED(status)) return 128 + WTERMSIG(status);
    return -EIO;
#endif
}
