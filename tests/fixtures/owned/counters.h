
#define _POSIX_C_SOURCE 200809L
#ifdef __APPLE__
#define _DARWIN_C_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>
static long live, allocations, copies, drops;
static int registered;
static void report(void) {
 fprintf(stderr,"COUNTERS allocations=%ld copies=%ld drops=%ld live=%ld\n",allocations,copies,drops,live);
 if(live != 0) _Exit(99);
}
static void *tracked_malloc(size_t n) {
 if(!registered) {registered=1;atexit(report);}
 void *p=malloc(n); if(p) ++live; return p;
}
static void *tracked_realloc(void *p,size_t n) {
 int was_null=p==NULL; void *q=realloc(p,n);if(q && was_null) ++live;return q;
}
static void tracked_free(void *p) {if(p)--live;free(p);}
/* Generated aggregate helpers allocate directly in LLVM, outside C macros. */
void *bhasa_owned_test_malloc(size_t n) { return tracked_malloc(n); }
void bhasa_owned_test_free(void *p) { tracked_free(p); }
#define malloc tracked_malloc
#define realloc tracked_realloc
#define free tracked_free
