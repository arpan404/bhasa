#define _POSIX_C_SOURCE 200809L
#include "file.h"
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* Override only in the isolated test translation unit. */
#ifndef BHASA_FILE_READ
#define BHASA_FILE_READ read
#endif
#ifndef BHASA_FILE_OPEN
#define BHASA_FILE_OPEN open
#endif
#ifndef BHASA_FILE_WRITE
#define BHASA_FILE_WRITE write
#endif
#ifndef BHASA_FILE_CLOSE
#define BHASA_FILE_CLOSE close
#endif
#ifndef BHASA_FILE_FSYNC
#define BHASA_FILE_FSYNC fsync
#endif
struct bhasa_file { int fd; int status; int close_status; size_t owners; };
static _Noreturn void file_fail(void) {
    fputs("Bhasa runtime error: invalid file storage or allocation failure\n", stderr);
    exit(1);
}
static void valid_buffer(const bhasa_buffer *b) {
    if (!b || b->len < 0 || b->cap < b->len || (b->cap && !b->data)) file_fail();
}
static int error_code(void) { return errno ? errno : EIO; }
static bhasa_file *handle(int fd, int status) {
    bhasa_file *f = malloc(sizeof(*f));
    if (!f) { if (fd >= 0) (void)BHASA_FILE_CLOSE(fd); file_fail(); }
    *f = (bhasa_file){fd, status, 0, 1};
    return f;
}
static int usable(const bhasa_file *f) {
    if (!f) file_fail();
    return f->status ? f->status : f->fd < 0 ? EBADF : 0;
}
bhasa_file *bhasa_rt_file_open_str(const char *path, int64_t mode) {
    int flags;
    switch (mode) {
    case 0: flags = O_RDONLY; break;
    case 1: flags = O_WRONLY | O_CREAT | O_TRUNC; break;
    case 2: flags = O_WRONLY | O_CREAT | O_APPEND; break;
    case 3: flags = O_RDWR; break;
    default: return handle(-1, EINVAL);
    }
    if (!path) return handle(-1, EINVAL);
    int fd;
    do { fd = BHASA_FILE_OPEN(path, flags | O_CLOEXEC | O_NONBLOCK | O_NOCTTY, 0666); } while (fd < 0 && errno == EINTR);
    if (fd >= 0) {
        struct stat info;
        if (fstat(fd, &info) < 0) {
            int status = error_code();
            (void)BHASA_FILE_CLOSE(fd);
            return handle(-1, status);
        }
        if (!S_ISREG(info.st_mode)) {
            int status = S_ISDIR(info.st_mode) ? EISDIR : ENOTSUP;
            (void)BHASA_FILE_CLOSE(fd);
            return handle(-1, status);
        }
    }
    return handle(fd, fd < 0 ? error_code() : 0);
}
bhasa_file *bhasa_rt_file_open(const bhasa_buffer *path, int64_t mode) {
    valid_buffer(path);
    if ((uint64_t)path->len >= SIZE_MAX || (path->len && memchr(path->data, 0, (size_t)path->len)))
        return handle(-1, EINVAL);
    char *p = malloc((size_t)path->len + 1);
    if (!p) file_fail();
    if (path->len) memcpy(p, path->data, (size_t)path->len);
    p[path->len] = 0;
    bhasa_file *f = bhasa_rt_file_open_str(p, mode);
    free(p);
    return f;
}
bhasa_file *bhasa_rt_file_clone(const bhasa_file *f) {
    if (!f || f->owners == SIZE_MAX) file_fail();
    bhasa_file *shared = (bhasa_file *)f;
    ++shared->owners;
    return shared;
}
int64_t bhasa_rt_file_status(const bhasa_file *f) { return usable(f); }
int64_t bhasa_rt_file_read_chunk(const bhasa_file *f, bhasa_buffer *out, int64_t limit) {
    valid_buffer(out);
    int status = usable(f);
    if (status) return status;
    if (limit < 0 || limit > 1048576) return EINVAL;
    unsigned char *data = limit ? malloc((size_t)limit) : NULL;
    if (limit && !data) file_fail();
    ssize_t count = 0;
    if (limit) {
        do { count = BHASA_FILE_READ(f->fd, data, (size_t)limit); } while (count < 0 && errno == EINTR);
    }
    if (count < 0) { status = error_code(); free(data); return status; }
    free(out->data);
    *out = (bhasa_buffer){count, limit, data};
    return 0;
}
int64_t bhasa_rt_file_write_chunk(const bhasa_file *f, const bhasa_buffer *bytes) {
    valid_buffer(bytes);
    int status = usable(f);
    if (status) return status;
    if (bytes->len > 1048576) return EINVAL;
    size_t sent = 0;
    while (sent < (size_t)bytes->len) {
        ssize_t n = BHASA_FILE_WRITE(f->fd, bytes->data + sent, (size_t)bytes->len - sent);
        if (n < 0) { if (errno == EINTR) continue; return error_code(); }
        if (!n) return EIO;
        sent += (size_t)n;
    }
    return 0;
}
int64_t bhasa_rt_file_seek(const bhasa_file *f, int64_t offset) {
    int status = usable(f);
    if (status) return status;
    if (offset < 0 || (int64_t)(off_t)offset != offset) return EINVAL;
    off_t result;
    do { result = lseek(f->fd, (off_t)offset, SEEK_SET); } while (result < 0 && errno == EINTR);
    return result < 0 ? error_code() : 0;
}
int64_t bhasa_rt_file_position(const bhasa_file *f) {
    int status = usable(f);
    if (status) return -status;
    off_t result;
    do { result = lseek(f->fd, 0, SEEK_CUR); } while (result < 0 && errno == EINTR);
    if (result < 0) return -error_code();
    if ((uint64_t)result > INT64_MAX) return -EOVERFLOW;
    return (int64_t)result;
}
int64_t bhasa_rt_file_flush(const bhasa_file *f) {
    int status = usable(f);
    if (status) return status;
    int result;
    do { result = BHASA_FILE_FSYNC(f->fd); } while (result < 0 && errno == EINTR);
    return result < 0 ? error_code() : 0;
}
int64_t bhasa_rt_file_close(bhasa_file *f) {
    if (!f) file_fail();
    if (f->fd < 0) return f->status ? f->status : f->close_status;
    int fd = f->fd;
    f->fd = -1;
    f->close_status = BHASA_FILE_CLOSE(fd) < 0 ? error_code() : 0;
    return f->close_status;
}
void bhasa_rt_file_free(bhasa_file *f) {
    if (f && --f->owners == 0) { (void)bhasa_rt_file_close(f); free(f); }
}
