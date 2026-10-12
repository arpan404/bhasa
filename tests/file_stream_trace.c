#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
static long live;
void *stream_trace_malloc(size_t n) { void *p=malloc(n); if(p)++live; return p; }
void *stream_trace_realloc(void *p,size_t n) {
    int empty=p==NULL; assert(n>0); void *q=realloc(p,n); if(q&&empty)++live; return q;
}
void stream_trace_free(void *p) { if(p){--live;assert(live>=0);}free(p); }
extern long stream_file_live, stream_file_descriptors, stream_file_opened;
extern long stream_file_retained, stream_file_released, stream_file_closed;
extern int stream_guest_main(int,char **);
int main(int argc,char **argv) {
    int status=stream_guest_main(argc,argv);
    assert(status==0); assert(live==0);
    assert(stream_file_live==0); assert(stream_file_descriptors==0);
    assert(stream_file_opened+stream_file_retained==stream_file_released);
    printf("FILE COUNTERS opened=%ld retained=%ld released=%ld closed=%ld allocations=%ld descriptors=%ld\n",
           stream_file_opened,stream_file_retained,stream_file_released,stream_file_closed,live,stream_file_descriptors);
    return 0;
}
