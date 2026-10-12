#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static int read_fault, write_fault, close_fault, flush_fault, writes, closes;
static int live_allocations, live_descriptors;
static int tracked_open(const char *path, int flags, int mode) {
    int fd = open(path, flags, mode);
    if (fd >= 0) ++live_descriptors;
    return fd;
}
static void *tracked_malloc(size_t size) {
    void *p = malloc(size);
    if (p) ++live_allocations;
    return p;
}
static void tracked_free(void *p) {
    if (p) { --live_allocations; assert(live_allocations >= 0); }
    free(p);
}
static ssize_t injected_read(int fd, void *p, size_t n) {
    if (read_fault == 1) { read_fault = 0; errno = EINTR; return -1; }
    if (read_fault == 2) { (void)read(fd, p, n > 2 ? 2 : n); errno = EIO; return -1; }
    return read(fd, p, n);
}
static ssize_t injected_write(int fd, const void *p, size_t n) {
    ++writes;
    if (write_fault == 1 && writes == 1) return write(fd, p, n > 2 ? 2 : n);
    if (write_fault == 1 && writes == 2) { errno = ENOSPC; return -1; }
    if (write_fault == 2) return 0;
    if (write_fault == 3) { write_fault = 0; errno = EINTR; return -1; }
    return write(fd, p, n);
}
static int injected_close(int fd) {
    ++closes;
    int status = close(fd);
    if (!status) --live_descriptors;
    if (close_fault) { errno = EIO; return -1; }
    return status;
}
static int injected_fsync(int fd) {
    if (flush_fault == 1) { flush_fault = 0; errno = EINTR; return -1; }
    if (flush_fault == 2) { errno = ENOSPC; return -1; }
    return fsync(fd);
}
#define BHASA_FILE_READ injected_read
#define BHASA_FILE_OPEN tracked_open
#define BHASA_FILE_WRITE injected_write
#define BHASA_FILE_CLOSE injected_close
#define BHASA_FILE_FSYNC injected_fsync
#define malloc tracked_malloc
#define free tracked_free
#include "../runtime/file.c"
#undef malloc
#undef free
int main(void) {
    char path[] = "/tmp/bhasa-stream-XXXXXX";
    int initial = mkstemp(path);
    assert(initial >= 0); close(initial);
    bhasa_file *f = bhasa_rt_file_open_str(path, 1);
    assert(bhasa_rt_file_status(f) == 0);
    assert(fcntl(f->fd, F_GETFD) & FD_CLOEXEC);
    bhasa_buffer bytes = {6, 6, (unsigned char *)"a\0bcde"};
    assert(bhasa_rt_file_write_chunk(f, &bytes) == 0);
    assert(bhasa_rt_file_position(f) == 6);
    flush_fault = 1; assert(bhasa_rt_file_flush(f) == 0);
    flush_fault = 2; assert(bhasa_rt_file_flush(f) == ENOSPC); flush_fault = 0;
    bhasa_rt_file_free(f);
    f = bhasa_rt_file_open_str(path, 0);
    bhasa_file *copy = bhasa_rt_file_clone(f);
    assert(bhasa_rt_file_status(copy) == 0);
    assert(fcntl(copy->fd, F_GETFD) & FD_CLOEXEC);
    bhasa_buffer out = {0, 0, NULL};
    read_fault = 1;
    assert(bhasa_rt_file_read_chunk(f, &out, 2) == 0 && out.len == 2);
    assert(memcmp(out.data, bytes.data, 2) == 0);
    assert(bhasa_rt_file_position(copy) == 2);
    assert(bhasa_rt_file_read_chunk(copy, &out, 9) == 0 && out.len == 4);
    assert(memcmp(out.data, bytes.data + 2, 4) == 0);
    assert(bhasa_rt_file_read_chunk(f, &out, 9) == 0 && out.len == 0);
    assert(bhasa_rt_file_seek(copy, 0) == 0);
    assert(bhasa_rt_file_read_chunk(f, &out, 6) == 0);
    unsigned char *before = out.data;
    read_fault = 2;
    assert(bhasa_rt_file_seek(f, 0) == 0);
    assert(bhasa_rt_file_read_chunk(f, &out, 6) == EIO);
    assert(out.data == before && out.len == 6 && memcmp(out.data, bytes.data, 6) == 0);
    assert(bhasa_rt_file_position(copy) == 2); read_fault = 0;
    assert(bhasa_rt_file_read_chunk(f, &out, -1) == EINVAL && out.data == before);
    assert(bhasa_rt_file_read_chunk(f, &out, 1048577) == EINVAL && out.data == before);
    assert(bhasa_rt_file_seek(f, -1) == EINVAL);
    assert(bhasa_rt_file_read_chunk(f, &out, 0) == 0 && out.len == 0);
    bhasa_rt_file_free(f);
    assert(bhasa_rt_file_read_chunk(copy, &out, 1) == 0 && out.len == 1);
    f = bhasa_rt_file_clone(copy);
    close_fault = 1;
    int previous_closes = closes;
    assert(bhasa_rt_file_close(f) == EIO);
    assert(bhasa_rt_file_close(f) == EIO);
    bhasa_rt_file_free(f);
    assert(closes == previous_closes + 1); close_fault = 0;
    assert(bhasa_rt_file_read_chunk(copy, &out, 1) == EBADF);
    assert(bhasa_rt_file_close(copy) == EIO);
    assert(bhasa_rt_file_status(copy) == EBADF);
    bhasa_file *dead = bhasa_rt_file_clone(copy);
    assert(bhasa_rt_file_status(dead) == EBADF); bhasa_rt_file_free(dead);
    bhasa_rt_file_free(copy);
    f = bhasa_rt_file_open_str(path, 1);
    writes = 0; write_fault = 1;
    assert(bhasa_rt_file_write_chunk(f, &bytes) == ENOSPC);
    assert(bhasa_rt_file_position(f) == 2);
    write_fault = 2; assert(bhasa_rt_file_write_chunk(f, &bytes) == EIO);
    write_fault = 3; assert(bhasa_rt_file_write_chunk(f, &bytes) == 0);
    bhasa_rt_file_free(f);
    f = bhasa_rt_file_open_str(path, 2);
    assert(bhasa_rt_file_seek(f, 0) == 0);
    assert(bhasa_rt_file_write_chunk(f, &bytes) == 0);
    assert(bhasa_rt_file_position(f) == 14); bhasa_rt_file_free(f);
    f = bhasa_rt_file_open_str(path, 3);
    assert(bhasa_rt_file_seek(f, 2) == 0);
    assert(bhasa_rt_file_read_chunk(f, &out, 6) == 0 && out.len == 6);
    assert(memcmp(out.data, bytes.data, 6) == 0); bhasa_rt_file_free(f);
    f = bhasa_rt_file_open_str(path, 42);
    assert(bhasa_rt_file_status(f) == EINVAL); bhasa_rt_file_free(f);
    bhasa_buffer nul = {3, 3, (unsigned char *)"a\0b"};
    f = bhasa_rt_file_open(&nul, 0);
    assert(bhasa_rt_file_status(f) == EINVAL); bhasa_rt_file_free(f);
    f = bhasa_rt_file_open_str("/", 0);
    assert(bhasa_rt_file_status(f) == EISDIR); bhasa_rt_file_free(f);
    assert(unlink(path) == 0);
    f = bhasa_rt_file_open_str(path, 0);
    assert(bhasa_rt_file_status(f) == ENOENT);
    assert(bhasa_rt_file_read_chunk(f, &out, 1) == ENOENT);
    bhasa_rt_file_free(f);
    tracked_free(out.data);
    assert(live_allocations == 0);
    assert(live_descriptors == 0);
    puts("file stream native tests passed");
    return 0;
}
