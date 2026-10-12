#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <fcntl.h>
#include <unistd.h>
#include "file_stream_trace.h"
long stream_file_live, stream_file_descriptors, stream_file_opened;
long stream_file_retained, stream_file_released, stream_file_closed;
static int counted_open(const char *path,int flags,int mode) {
    int fd=open(path,flags,mode);if(fd>=0)++stream_file_descriptors;return fd;
}
static int counted_close(int fd) {
    int result=close(fd);if(result==0){--stream_file_descriptors;++stream_file_closed;}return result;
}
#define BHASA_FILE_OPEN counted_open
#define BHASA_FILE_CLOSE counted_close
#define bhasa_rt_file_open stream_impl_open
#define bhasa_rt_file_open_str stream_impl_open_str
#define bhasa_rt_file_clone stream_impl_clone
#define bhasa_rt_file_free stream_impl_free
#include "../runtime/file.c"
#undef bhasa_rt_file_open
#undef bhasa_rt_file_open_str
#undef bhasa_rt_file_clone
#undef bhasa_rt_file_free
bhasa_file *bhasa_rt_file_open(const bhasa_buffer *path,int64_t mode) {
    bhasa_file *file=stream_impl_open(path,mode);++stream_file_opened;++stream_file_live;return file;
}
bhasa_file *bhasa_rt_file_open_str(const char *path,int64_t mode) {
    bhasa_file *file=stream_impl_open_str(path,mode);++stream_file_opened;++stream_file_live;return file;
}
bhasa_file *bhasa_rt_file_clone(const bhasa_file *file) {
    bhasa_file *result=stream_impl_clone(file);++stream_file_retained;return result;
}
void bhasa_rt_file_free(bhasa_file *file) {
    if(file){assert(file->owners>0);++stream_file_released;if(file->owners==1)--stream_file_live;}
    stream_impl_free(file);
}
