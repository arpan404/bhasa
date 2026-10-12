#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif

#include "core.h"

#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <spawn.h>
#endif

/* The test build redirects only core.c allocations to these functions. */
static int allocation_fails;
static int live_allocations;
void *bhasa_test_malloc(size_t size) {
    void *result = allocation_fails ? NULL : malloc(size);
    if (result != NULL) ++live_allocations;
    return result;
}
void *bhasa_test_realloc(void *pointer, size_t size) {
    int was_empty = pointer == NULL;
    void *result = allocation_fails ? NULL : realloc(pointer, size);
    if (result != NULL && was_empty) ++live_allocations;
    return result;
}
void bhasa_test_free(void *pointer) {
    if (pointer != NULL) {
        assert(live_allocations > 0);
        --live_allocations;
    }
    free(pointer);
}

#if defined(__APPLE__)
/* Fault injection covers each allocated Darwin spawn object and setup step. */
static int spawn_fail_at;
static int spawn_setup_calls;
static int live_spawn_attributes;
static int live_spawn_actions;
static int spawn_setup_fails(void) {
    return ++spawn_setup_calls == spawn_fail_at;
}
int bhasa_test_spawnattr_init(posix_spawnattr_t *attributes) {
    if (spawn_setup_fails()) return ENOMEM;
    int error = posix_spawnattr_init(attributes);
    if (error == 0) ++live_spawn_attributes;
    return error;
}
int bhasa_test_spawnattr_setflags(posix_spawnattr_t *attributes, short flags) {
    if (spawn_setup_fails()) return ENOMEM;
    return posix_spawnattr_setflags(attributes, flags);
}
int bhasa_test_spawnattr_destroy(posix_spawnattr_t *attributes) {
    assert(live_spawn_attributes > 0);
    int error = posix_spawnattr_destroy(attributes);
    assert(error == 0);
    --live_spawn_attributes;
    return error;
}
int bhasa_test_spawn_actions_init(posix_spawn_file_actions_t *actions) {
    if (spawn_setup_fails()) return ENOMEM;
    int error = posix_spawn_file_actions_init(actions);
    if (error == 0) ++live_spawn_actions;
    return error;
}
int bhasa_test_spawn_actions_inherit(posix_spawn_file_actions_t *actions, int descriptor) {
    if (spawn_setup_fails()) return ENOMEM;
    return posix_spawn_file_actions_addinherit_np(actions, descriptor);
}
int bhasa_test_spawn_actions_destroy(posix_spawn_file_actions_t *actions) {
    assert(live_spawn_actions > 0);
    int error = posix_spawn_file_actions_destroy(actions);
    assert(error == 0);
    --live_spawn_actions;
    return error;
}
#endif

static void test_values(void) {
    bhasa_buffer *empty = bhasa_rt_buffer_new();
    bhasa_buffer *copy = bhasa_rt_buffer_clone(empty);
    bhasa_buffer *slice = bhasa_rt_buffer_slice(empty, 0, 0);
    assert(bhasa_rt_buffer_len(empty) == 0);
    assert(bhasa_rt_buffer_same(empty, copy));
    assert(bhasa_rt_buffer_same_str(slice, ""));
    bhasa_rt_buffer_append(empty, empty);
    bhasa_rt_buffer_append_str(empty, "");
    bhasa_rt_buffer_free(copy);
    bhasa_rt_buffer_free(slice);
    for (int64_t index = 0; index < 100000; ++index)
        bhasa_rt_buffer_push(empty, index % 256);
    copy = bhasa_rt_buffer_clone(empty);
    bhasa_rt_buffer_set(empty, 0, 255);
    assert(bhasa_rt_buffer_get(copy, 0) == 0);
    assert(!bhasa_rt_buffer_same(empty, copy));
    bhasa_rt_buffer_append(empty, empty);
    assert(bhasa_rt_buffer_len(empty) == 200000);
    for (int64_t index = 0; index < 100000; ++index)
        assert(bhasa_rt_buffer_get(empty, index) == bhasa_rt_buffer_get(empty, index + 100000));
    slice = bhasa_rt_buffer_slice(empty, 99999, 2);
    assert(bhasa_rt_buffer_get(slice, 0) == 99999 % 256);
    assert(bhasa_rt_buffer_get(slice, 1) == 255);
    bhasa_rt_buffer_free(slice);
    bhasa_rt_buffer_free(copy);
    bhasa_rt_buffer_free(empty);
    bhasa_rt_buffer_free(NULL);

    bhasa_buffer *minimum = bhasa_rt_to_buffer(INT64_MIN);
    bhasa_buffer *maximum = bhasa_rt_to_buffer(INT64_MAX);
    bhasa_buffer *zero = bhasa_rt_to_buffer(0);
    assert(bhasa_rt_buffer_same_str(minimum, "-9223372036854775808"));
    assert(bhasa_rt_buffer_same_str(maximum, "9223372036854775807"));
    assert(bhasa_rt_buffer_same_str(zero, "0"));
    bhasa_rt_buffer_free(minimum);
    bhasa_rt_buffer_free(maximum);
    bhasa_rt_buffer_free(zero);

    bhasa_ints *values = bhasa_rt_ints_new();
    bhasa_ints *other = bhasa_rt_ints_clone(values);
    assert(bhasa_rt_ints_len(other) == 0);
    bhasa_rt_ints_free(other);
    for (int64_t index = 0; index < 100000; ++index)
        bhasa_rt_ints_push(values, index - 50000);
    other = bhasa_rt_ints_clone(values);
    bhasa_rt_ints_set(values, 123, INT64_MIN);
    bhasa_rt_ints_set(values, 124, INT64_MAX);
    assert(bhasa_rt_ints_get(other, 123) == 123 - 50000);
    assert(bhasa_rt_ints_get(values, 123) == INT64_MIN);
    assert(bhasa_rt_ints_get(values, 124) == INT64_MAX);
    assert(bhasa_rt_ints_len(values) == 100000);
    bhasa_rt_ints_free(other);
    bhasa_rt_ints_free(values);
    bhasa_rt_ints_free(NULL);
}

static void test_files(void) {
    bhasa_buffer *path = bhasa_rt_buffer_from_str("a binary file.bin");
    bhasa_buffer *contents = bhasa_rt_buffer_new();
    for (int64_t index = 0; index < 50000; ++index)
        bhasa_rt_buffer_push(contents, index % 256);
    assert(bhasa_rt_write_file(path, contents) == 0);
    bhasa_buffer *read = bhasa_rt_buffer_from_str("replace me");
    assert(bhasa_rt_read_file(path, read) == 0);
    assert(bhasa_rt_buffer_same(read, contents));
    assert(bhasa_rt_read_file_str("not-a-file-98377", read) == ENOENT);
    assert(bhasa_rt_buffer_same(read, contents));
    assert(bhasa_rt_read_file_str(".", read) != 0);
    assert(bhasa_rt_buffer_same(read, contents));
    bhasa_rt_buffer_push(path, 0);
    assert(bhasa_rt_read_file(path, read) == EINVAL);
    assert(bhasa_rt_write_file(path, contents) == EINVAL);
    assert(bhasa_rt_buffer_same(read, contents));
    assert(bhasa_rt_write_file_str("missing-folder/a.bin", contents) == ENOENT);
    bhasa_buffer *empty = bhasa_rt_buffer_new();
    assert(bhasa_rt_write_file_str("empty.bin", empty) == 0);
    assert(bhasa_rt_read_file_str("empty.bin", read) == 0);
    assert(bhasa_rt_buffer_len(read) == 0);
    assert(bhasa_rt_read_file_str("a binary file.bin", read) == 0);
    assert(bhasa_rt_write_file_str("copy.bin", read) == 0);
    bhasa_rt_buffer_free(empty);
    bhasa_rt_buffer_free(read);
    bhasa_rt_buffer_free(contents);
    bhasa_rt_buffer_free(path);
}

static void test_arguments(int argc, char **argv) {
    assert(bhasa_rt_arg_count() == argc);
    bhasa_buffer *executable = bhasa_rt_arg(0);
    assert(bhasa_rt_buffer_same_str(executable, argv[0]));
    bhasa_buffer *again = bhasa_rt_arg(0);
    bhasa_rt_buffer_set(executable, 0, 0);
    assert(bhasa_rt_buffer_same_str(again, argv[0]));
    bhasa_rt_buffer_free(executable);
    bhasa_rt_buffer_free(again);

    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    assert(bhasa_rt_process_run(arguments) == -EINVAL);
    bhasa_rt_buffer_append_str(arguments, "unterminated");
    assert(bhasa_rt_process_run(arguments) == -EINVAL);
    bhasa_rt_buffer_free(arguments);
    arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, "bhasa-nonexistent-executable-173633");
    assert(bhasa_rt_process_run(arguments) == -ENOENT);
    bhasa_rt_buffer_free(arguments);

    arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, argv[0]);
    bhasa_rt_args_push_str(arguments, "child");
    bhasa_rt_args_push_str(arguments, "");
    bhasa_buffer *literal = bhasa_rt_buffer_from_str("$(touch SHELL_EXECUTED); * ' \" \\ space");
    bhasa_rt_args_push(arguments, literal);
    assert(bhasa_rt_process_run(arguments) == 37);
    FILE *marker = fopen("SHELL_EXECUTED", "rb");
    assert(marker == NULL);
    bhasa_rt_buffer_free(literal);
    bhasa_rt_buffer_free(arguments);
}

#if defined(__APPLE__)
static void test_spawn_setup_failures(const char *executable) {
    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, executable);
    bhasa_rt_args_push_str(arguments, "fdcheck");
    bhasa_rt_args_push_str(arguments, "200");
    int expected_allocations = live_allocations;
    for (int step = 1; step <= 6; ++step) {
        spawn_fail_at = step;
        spawn_setup_calls = 0;
        assert(bhasa_rt_process_run(arguments) == -ENOMEM);
        assert(spawn_setup_calls == step);
        assert(live_spawn_attributes == 0 && live_spawn_actions == 0);
        assert(live_allocations == expected_allocations);
    }
    spawn_fail_at = 0;
    spawn_setup_calls = 0;
    bhasa_rt_buffer_free(arguments);
}

static void test_spawn_no_shell(void) {
    bhasa_buffer *script = bhasa_rt_buffer_from_str("exit 29\n");
    assert(bhasa_rt_write_file_str("no-shebang", script) == 0);
    assert(chmod("no-shebang", 0700) == 0);
    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, "./no-shebang");
    assert(bhasa_rt_process_run(arguments) == -ENOEXEC);
    bhasa_rt_buffer_free(arguments);
    bhasa_rt_buffer_free(script);
    assert(unlink("no-shebang") == 0);
}

static void test_spawn_path(const char *executable) {
    const char *original = getenv("PATH");
    char *saved_path = original == NULL ? NULL : strdup(original);
    assert(original == NULL || saved_path != NULL);
    assert(symlink(executable, "path-child") == 0);
    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, "path-child");
    bhasa_rt_args_push_str(arguments, "child");
    bhasa_rt_args_push_str(arguments, "");
    bhasa_rt_args_push_str(arguments, "$(touch SHELL_EXECUTED); * ' \" \\ space");
    const char *paths[] = {"", ":missing", "missing:", "missing::also-missing",
                           "a binary file.bin:."};
    for (size_t index = 0; index < sizeof(paths) / sizeof(paths[0]); ++index) {
        assert(setenv("PATH", paths[index], 1) == 0);
        assert(bhasa_rt_process_run(arguments) == 37);
    }
    bhasa_rt_buffer_free(arguments);
    assert(unlink("path-child") == 0);

    bhasa_buffer *script = bhasa_rt_buffer_from_str("exit 29\n");
    assert(bhasa_rt_write_file_str("path-child", script) == 0);
    arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, "path-child");
    assert(setenv("PATH", ".:missing", 1) == 0);
    assert(chmod("path-child", 0600) == 0);
    assert(bhasa_rt_process_run(arguments) == -EACCES);
    assert(chmod("path-child", 0700) == 0);
    assert(bhasa_rt_process_run(arguments) == -ENOEXEC);
    assert(unlink("path-child") == 0);
    assert(bhasa_rt_process_run(arguments) == -ENOENT);
    bhasa_rt_buffer_free(arguments);
    bhasa_rt_buffer_free(script);

    assert(unsetenv("PATH") == 0);
    arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, "true");
    assert(bhasa_rt_process_run(arguments) == 0);
    bhasa_rt_buffer_free(arguments);
    if (saved_path != NULL) {
        assert(setenv("PATH", saved_path, 1) == 0);
        free(saved_path);
    }
    assert(live_spawn_attributes == 0 && live_spawn_actions == 0);
}

static void test_descriptor_inheritance(const char *executable) {
    FILE *stream = fopen("a binary file.bin", "rb");
    assert(stream != NULL);
    /* Keep the checked descriptor away from the child's loader/sanitizer FDs. */
    int descriptor = fcntl(fileno(stream), F_DUPFD, 200);
    assert(descriptor >= 200);
    assert(fcntl(descriptor, F_GETFD) == 0);
    assert(fclose(stream) == 0);
    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, executable);
    bhasa_rt_args_push_str(arguments, "fdcheck");
    bhasa_buffer *number = bhasa_rt_to_buffer(descriptor);
    bhasa_rt_args_push(arguments, number);
    assert(bhasa_rt_process_run(arguments) == 0);
    /* The parent's descriptor and flags must remain intact. */
    assert(fcntl(descriptor, F_GETFD) == 0);
    assert(close(descriptor) == 0);
    bhasa_rt_buffer_free(number);
    bhasa_rt_buffer_free(arguments);
}
#endif

static int spawn_stdio_mode(const char *executable) {
    bhasa_buffer *arguments = bhasa_rt_buffer_new();
    bhasa_rt_args_push_str(arguments, executable);
    bhasa_rt_args_push_str(arguments, "stdio");
    int64_t status = bhasa_rt_process_run(arguments);
    bhasa_rt_buffer_free(arguments);
    assert(live_allocations == 0);
    return (int)status;
}

static int pipe_file_reader = -1;

#if defined(__APPLE__)
/* Match fopen's SDK assembly alias after the core-only macro substitution. */
FILE *bhasa_test_pipe_fopen(const char *, const char *) __DARWIN_EXTSN(bhasa_test_pipe_fopen);
#endif

/* The core-only wrapper establishes a closed FIFO reader before its first write. */
FILE *bhasa_test_pipe_fopen(const char *path, const char *mode) {
    FILE *stream = fopen(path, mode);
    if (pipe_file_reader >= 0) {
        assert(stream != NULL);
        assert(close(pipe_file_reader) == 0);
        pipe_file_reader = -1;
    }
    return stream;
}

/* Closed readers are established before writing, with no scheduling race. */
static int pipe_mode(const char *mode) {
    int pending_before = strcmp(mode, "pipe-pending") == 0;
    int blocked_before = pending_before || strcmp(mode, "pipe-blocked") == 0;
    int ignored_before = strcmp(mode, "pipe-ignored") == 0;
    assert(signal(SIGPIPE, ignored_before ? SIG_IGN : SIG_DFL) != SIG_ERR);
    sigset_t mask, original_mask;
    assert(sigemptyset(&mask) == 0);
    assert(sigaddset(&mask, SIGUSR1) == 0);
    if (blocked_before) assert(sigaddset(&mask, SIGPIPE) == 0);
    assert(pthread_sigmask(SIG_SETMASK, &mask, &original_mask) == 0);
    if (pending_before) assert(raise(SIGPIPE) == 0);

    int descriptors[2];
    assert(pipe(descriptors) == 0);
    assert(close(descriptors[0]) == 0);
    bhasa_buffer *bytes = bhasa_rt_buffer_from_str("pipe bytes");
    if (strcmp(mode, "pipe-large") == 0) {
        for (int index = 0; index < 32768; ++index) bhasa_rt_buffer_push(bytes, 'x');
    }
    int64_t status;
    if (strcmp(mode, "pipe-file") == 0) {
        const char *path = "pipe-output.fifo";
        assert(mkfifo(path, 0600) == 0);
        pipe_file_reader = open(path, O_RDONLY | O_NONBLOCK);
        assert(pipe_file_reader >= 0);
        status = bhasa_rt_write_file_str(path, bytes);
        assert(pipe_file_reader == -1);
        assert(unlink(path) == 0);
    } else {
        int target = strcmp(mode, "pipe-stderr") == 0 ? STDERR_FILENO : STDOUT_FILENO;
        int saved = dup(target);
        assert(saved >= 0);
        assert(dup2(descriptors[1], target) == target);
        status = target == STDERR_FILENO ? bhasa_rt_write_stderr(bytes)
                                        : bhasa_rt_write_stdout(bytes);
        assert(dup2(saved, target) == target);
        assert(close(saved) == 0);
        clearerr(target == STDERR_FILENO ? stderr : stdout);
    }
    assert(close(descriptors[1]) == 0);
    assert(status == EPIPE);
    bhasa_rt_buffer_free(bytes);
    assert(live_allocations == 0);

    sigset_t after, pending;
    assert(pthread_sigmask(SIG_BLOCK, NULL, &after) == 0);
    assert(sigismember(&after, SIGPIPE) == blocked_before);
    assert(sigismember(&after, SIGUSR1) == 1);
    assert(sigismember(&after, SIGUSR2) == 0);
    assert(sigpending(&pending) == 0);
    assert(sigismember(&pending, SIGPIPE) == pending_before);
    struct sigaction disposition;
    assert(sigaction(SIGPIPE, NULL, &disposition) == 0);
    assert(disposition.sa_handler == (ignored_before ? SIG_IGN : SIG_DFL));
    if (pending_before) {
        int received;
        assert(sigemptyset(&mask) == 0);
        assert(sigaddset(&mask, SIGPIPE) == 0);
        assert(sigwait(&mask, &received) == 0);
        assert(received == SIGPIPE);
    }
    assert(pthread_sigmask(SIG_SETMASK, &original_mask, NULL) == 0);
    return 0;
}

static int stdio_mode(void) {
    const char *environment = getenv("BHASA_CORE_TEST_ENV");
    assert(environment != NULL && strcmp(environment, "inherited value") == 0);
    bhasa_buffer *input = bhasa_rt_buffer_from_str("old");
    assert(bhasa_rt_read_stdin(input) == 0);
    assert(bhasa_rt_write_stdout(input) == 0);
    assert(bhasa_rt_write_stderr(input) == 0);
    bhasa_rt_print_buffer(input);
    bhasa_rt_buffer_free(input);
    assert(live_allocations == 0);
    return 0;
}

int main(int argc, char **argv) {
    bhasa_rt_init(argc, argv);
    assert(argc >= 2);
    if (strcmp(argv[1], "child") == 0) {
        assert(argc == 4);
        assert(strcmp(argv[2], "") == 0);
        assert(strcmp(argv[3], "$(touch SHELL_EXECUTED); * ' \" \\ space") == 0);
        return 37;
    }
    if (strncmp(argv[1], "pipe-", 5) == 0) return pipe_mode(argv[1]);
    if (strcmp(argv[1], "stdio") == 0) return stdio_mode();
    if (strcmp(argv[1], "spawn-stdio") == 0) return spawn_stdio_mode(argv[0]);
#if defined(__APPLE__)
    if (strcmp(argv[1], "spawn-closed-stdio") == 0) {
        assert(close(STDIN_FILENO) == 0);
        assert(fcntl(STDOUT_FILENO, F_SETFD, FD_CLOEXEC) == 0);
        bhasa_buffer *arguments = bhasa_rt_buffer_new();
        bhasa_rt_args_push_str(arguments, argv[0]);
        bhasa_rt_args_push_str(arguments, "closed-stdio-child");
        assert(bhasa_rt_process_run(arguments) == 0);
        assert(fcntl(STDOUT_FILENO, F_GETFD) == FD_CLOEXEC);
        bhasa_rt_buffer_free(arguments);
        assert(live_allocations == 0);
        assert(live_spawn_attributes == 0 && live_spawn_actions == 0);
        return 0;
    }
    if (strcmp(argv[1], "closed-stdio-child") == 0) {
        errno = 0;
        assert(fcntl(STDIN_FILENO, F_GETFD) == -1 && errno == EBADF);
        errno = 0;
        assert(fcntl(STDOUT_FILENO, F_GETFD) == -1 && errno == EBADF);
        assert(fcntl(STDERR_FILENO, F_GETFD) == 0);
        return 0;
    }
    if (strcmp(argv[1], "fdcheck") == 0) {
        assert(argc == 3);
        int descriptor = atoi(argv[2]);
        assert(descriptor >= 200);
        errno = 0;
        assert(fcntl(descriptor, F_GETFD) == -1 && errno == EBADF);
        return 0;
    }
#endif
    if (strcmp(argv[1], "values") == 0) {
        test_values();
        test_files();
        test_arguments(argc, argv);
#if defined(__APPLE__)
        test_descriptor_inheritance(argv[0]);
        test_spawn_no_shell();
        test_spawn_path(argv[0]);
        test_spawn_setup_failures(argv[0]);
#endif
        assert(live_allocations == 0);
        return 0;
    }
    bhasa_buffer *bytes = bhasa_rt_buffer_new();
    bhasa_ints *values = bhasa_rt_ints_new();
    if (strcmp(argv[1], "buffer-empty-index") == 0) bhasa_rt_buffer_get(bytes, 0);
    if (strcmp(argv[1], "buffer-negative-index") == 0) bhasa_rt_buffer_set(bytes, -1, 0);
    if (strcmp(argv[1], "byte-too-large") == 0) bhasa_rt_buffer_push(bytes, 256);
    if (strcmp(argv[1], "byte-negative") == 0) bhasa_rt_buffer_push(bytes, -1);
    if (strcmp(argv[1], "ints-empty-index") == 0) bhasa_rt_ints_get(values, 0);
    if (strcmp(argv[1], "slice-overflow") == 0) bhasa_rt_buffer_slice(bytes, INT64_MAX, INT64_MAX);
    if (strcmp(argv[1], "slice-negative") == 0) bhasa_rt_buffer_slice(bytes, 0, -1);
    if (strcmp(argv[1], "argument-index") == 0) bhasa_rt_arg(argc);
    if (strcmp(argv[1], "argument-nul") == 0) {
        bhasa_rt_buffer_push(bytes, 0);
        bhasa_rt_args_push(bytes, bytes);
    }
    if (strcmp(argv[1], "capacity-overflow") == 0) {
        unsigned char placeholder = 0;
        bhasa_buffer artificial = {INT64_MAX, INT64_MAX, &placeholder};
        bhasa_rt_buffer_push(&artificial, 0);
    }
    if (strcmp(argv[1], "ints-capacity-overflow") == 0) {
        int64_t placeholder = 0;
        bhasa_ints artificial = {INT64_MAX / 2, INT64_MAX / 2, &placeholder};
        bhasa_rt_ints_push(&artificial, 0);
    }
    if (strcmp(argv[1], "allocation-failure") == 0) {
        allocation_fails = 1;
        bhasa_rt_buffer_new();
    }
    if (strcmp(argv[1], "growth-allocation-failure") == 0) {
        allocation_fails = 1;
        bhasa_rt_buffer_push(bytes, 1);
    }
    bhasa_rt_ints_free(values);
    bhasa_rt_buffer_free(bytes);
    assert(!"unknown mode or missing runtime trap");
    return 2;
}
