#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static _Atomic int64_t live;
static atomic_flag registered = ATOMIC_FLAG_INIT;
static void check_live(void) {
    int64_t count = atomic_load_explicit(&live, memory_order_relaxed);
    if (count != 0) {
        fprintf(stderr, "private storage leaked allocations: %lld\n", (long long)count);
        _Exit(101);
    }
}
static void prepare(void) {
    if (!atomic_flag_test_and_set_explicit(&registered, memory_order_relaxed) && atexit(check_live) != 0)
        abort();
}
void *storage_malloc(size_t size) {
    prepare();
    void *result = malloc(size);
    if (result) atomic_fetch_add_explicit(&live, 1, memory_order_relaxed);
    return result;
}
void *storage_calloc(size_t count, size_t size) {
    prepare();
    void *result = calloc(count, size);
    if (result) atomic_fetch_add_explicit(&live, 1, memory_order_relaxed);
    return result;
}
void *storage_realloc(void *pointer, size_t size) {
    prepare();
    if (size == 0) abort();
    int empty = pointer == NULL;
    void *result = realloc(pointer, size);
    if (result && empty) atomic_fetch_add_explicit(&live, 1, memory_order_relaxed);
    return result;
}
void storage_free(void *pointer) {
    if (pointer && atomic_fetch_sub_explicit(&live, 1, memory_order_relaxed) <= 0) abort();
    free(pointer);
}
int64_t storage_live(int64_t ignored) {
    (void)ignored;
    return atomic_load_explicit(&live, memory_order_relaxed);
}
