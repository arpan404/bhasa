#ifndef BHASA_FILE_STREAM_TRACE_H
#define BHASA_FILE_STREAM_TRACE_H
#include <stddef.h>
void *stream_trace_malloc(size_t);
void *stream_trace_realloc(void *, size_t);
void stream_trace_free(void *);
#define malloc stream_trace_malloc
#define realloc stream_trace_realloc
#define free stream_trace_free
#endif
