#ifndef BHASA_RUNTIME_FILE_H
#define BHASA_RUNTIME_FILE_H
#include "core.h"
#ifdef __cplusplus
extern "C" {
#endif
typedef struct bhasa_file bhasa_file;
/* POSIX regular-file draft. Other file kinds return EISDIR/ENOTSUP. Owned
 * even on open/clone failure. Allocation failure is fatal.
 * mode: 0 read, 1 create/truncate write, 2 create/append write, 3 read/write.
 * Copies retain one shared CLOEXEC descriptor with deterministic reference
 * counting. Copying performs no OS call and preserves constructor status.
 * No concurrent cursor operations without external synchronization. */
bhasa_file *bhasa_rt_file_open(const bhasa_buffer *path, int64_t mode);
bhasa_file *bhasa_rt_file_open_str(const char *path, int64_t mode);
bhasa_file *bhasa_rt_file_clone(const bhasa_file *file);
void bhasa_rt_file_free(bhasa_file *file);
int64_t bhasa_rt_file_status(const bhasa_file *file);
/* Zero success, positive errno failure. Read replaces destination on success
 * only; limit is 0..1048576, empty successful positive-limit read means EOF.
 * A failed read may advance the cursor. Write may leave a prefix on failure. */
int64_t bhasa_rt_file_read_chunk(const bhasa_file *file, bhasa_buffer *destination, int64_t limit);
int64_t bhasa_rt_file_write_chunk(const bhasa_file *file, const bhasa_buffer *contents);
/* Absolute nonnegative byte offset; successful seek clears no other state. */
int64_t bhasa_rt_file_seek(const bhasa_file *file, int64_t offset);
/* Nonnegative position or negative errno. */
int64_t bhasa_rt_file_position(const bhasa_file *file);
/* fsync durability request, subject to the filesystem/device guarantees. */
int64_t bhasa_rt_file_flush(const bhasa_file *file);
/* Idempotent for the shared resource; invalidates ALL copies even on error,
 * never retries close. Final free closes a still-open descriptor. */
int64_t bhasa_rt_file_close(bhasa_file *file);
#ifdef __cplusplus
}
#endif
#endif
