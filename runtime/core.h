#ifndef BHASA_RUNTIME_CORE_H
#define BHASA_RUNTIME_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Compiler-owned allocations. Foreign adapters must not retain borrowed data. */
typedef struct bhasa_buffer {
    int64_t len;
    int64_t cap;
    unsigned char *data;
} bhasa_buffer;

typedef struct bhasa_ints {
    int64_t len;
    int64_t cap;
    int64_t *data;
} bhasa_ints;

bhasa_buffer *bhasa_rt_buffer_new(void);
bhasa_buffer *bhasa_rt_buffer_from_str(const char *text);
bhasa_buffer *bhasa_rt_buffer_clone(const bhasa_buffer *value);
void bhasa_rt_buffer_free(bhasa_buffer *value);
int64_t bhasa_rt_buffer_len(const bhasa_buffer *value);
int64_t bhasa_rt_buffer_get(const bhasa_buffer *value, int64_t index);
void bhasa_rt_buffer_set(bhasa_buffer *value, int64_t index, int64_t byte);
void bhasa_rt_buffer_push(bhasa_buffer *value, int64_t byte);
void bhasa_rt_buffer_append(bhasa_buffer *destination, const bhasa_buffer *source);
void bhasa_rt_buffer_append_str(bhasa_buffer *destination, const char *source);
bhasa_buffer *bhasa_rt_buffer_slice(const bhasa_buffer *value, int64_t start, int64_t count);
int64_t bhasa_rt_buffer_same(const bhasa_buffer *left, const bhasa_buffer *right);
int64_t bhasa_rt_buffer_same_str(const bhasa_buffer *left, const char *right);
bhasa_buffer *bhasa_rt_to_buffer(int64_t value);

bhasa_ints *bhasa_rt_ints_new(void);
bhasa_ints *bhasa_rt_ints_clone(const bhasa_ints *value);
void bhasa_rt_ints_free(bhasa_ints *value);
int64_t bhasa_rt_ints_len(const bhasa_ints *value);
int64_t bhasa_rt_ints_get(const bhasa_ints *value, int64_t index);
void bhasa_rt_ints_set(bhasa_ints *value, int64_t index, int64_t element);
void bhasa_rt_ints_push(bhasa_ints *value, int64_t element);

void bhasa_rt_init(int argc, char **argv);
int64_t bhasa_rt_arg_count(void);
bhasa_buffer *bhasa_rt_arg(int64_t index);

/* I/O: zero means success, positive values are errno codes.
   POSIX writes report EPIPE without changing the process SIGPIPE disposition.
   POSIX consumers must link with pthread support. */
int64_t bhasa_rt_read_file(const bhasa_buffer *path, bhasa_buffer *destination);
int64_t bhasa_rt_read_file_str(const char *path, bhasa_buffer *destination);
int64_t bhasa_rt_write_file(const bhasa_buffer *path, const bhasa_buffer *contents);
int64_t bhasa_rt_write_file_str(const char *path, const bhasa_buffer *contents);
int64_t bhasa_rt_read_stdin(bhasa_buffer *destination);
int64_t bhasa_rt_write_stdout(const bhasa_buffer *contents);
int64_t bhasa_rt_write_stderr(const bhasa_buffer *contents);
void bhasa_rt_print_buffer(const bhasa_buffer *contents);

/* Argument records include argv[0]. Each record is terminated by one NUL. */
void bhasa_rt_args_push(bhasa_buffer *arguments, const bhasa_buffer *value);
void bhasa_rt_args_push_str(bhasa_buffer *arguments, const char *value);
/* Exit code or 128 + signal; negative errno means invalid input/OS failure.
   On macOS only standard descriptors are eligible for inheritance. */
int64_t bhasa_rt_process_run(const bhasa_buffer *arguments);

#ifdef __cplusplus
}
#endif

#endif
