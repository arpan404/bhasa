#define _POSIX_C_SOURCE 200809L
#include "concurrency.h"
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static _Atomic long allocations;
static _Atomic int created;
static _Atomic int joined;
static pthread_mutex_t state = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t changed = PTHREAD_COND_INITIALIZER;
static pthread_t identities[BHASA_ASYNC_WORKERS];
static int identities_count;
static int running;
static int maximum_running;
static int completed;
static int open_gate;
static int backpressure_seen;
static int submitter_returned;
static int expected_threads = BHASA_ASYNC_WORKERS;
static _Thread_local int observing_submitter;
static void *shared_task;

void *bhasa_async_test_malloc(size_t size) {
    void *result = malloc(size);
    if (result != NULL) atomic_fetch_add(&allocations, 1);
    return result;
}

void bhasa_async_test_free(void *pointer) {
    if (pointer != NULL) atomic_fetch_sub(&allocations, 1);
    free(pointer);
}

int bhasa_async_test_pthread_create(pthread_t *thread, const pthread_attr_t *attributes,
                                  void *(*start)(void *), void *argument) {
    int status = pthread_create(thread, attributes, start, argument);
    if (status == 0) atomic_fetch_add(&created, 1);
    return status;
}

int bhasa_async_test_pthread_join(pthread_t thread, void **result) {
    int status = pthread_join(thread, result);
    if (status == 0) atomic_fetch_add(&joined, 1);
    return status;
}

int bhasa_async_test_cond_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (observing_submitter) {
        assert(pthread_mutex_lock(&state) == 0);
        backpressure_seen = 1;
        assert(pthread_cond_broadcast(&changed) == 0);
        assert(pthread_mutex_unlock(&state) == 0);
    }
    return pthread_cond_wait(condition, mutex);
}

static int64_t work(int64_t argument) {
    assert(pthread_mutex_lock(&state) == 0);
    int found = 0;
    for (int i = 0; i < identities_count; ++i) {
        if (pthread_equal(identities[i], pthread_self())) found = 1;
    }
    if (!found) {
        assert(identities_count < BHASA_ASYNC_WORKERS);
        identities[identities_count++] = pthread_self();
    }
    ++running;
    if (maximum_running < running) maximum_running = running;
    assert(running <= BHASA_ASYNC_WORKERS);
    assert(pthread_cond_broadcast(&changed) == 0);
    while (!open_gate) assert(pthread_cond_wait(&changed, &state) == 0);
    --running;
    ++completed;
    assert(pthread_mutex_unlock(&state) == 0);
    return argument * 3 + 1;
}

static void *blocked_submitter(void *unused) {
    (void)unused;
    observing_submitter = 1;
    void *task = bhasa_rt_async_spawn_i64(work, 900);
    observing_submitter = 0;
    assert(pthread_mutex_lock(&state) == 0);
    submitter_returned = 1;
    assert(pthread_mutex_unlock(&state) == 0);
    assert(bhasa_rt_await_i64(task) == 2701);
    return NULL;
}

static void saturation(void) {
    void *tasks[BHASA_ASYNC_WORKERS + BHASA_ASYNC_QUEUE_CAPACITY];
    for (int i = 0; i < BHASA_ASYNC_WORKERS; ++i) tasks[i] = bhasa_rt_async_spawn_i64(work, i);
    assert(pthread_mutex_lock(&state) == 0);
    while (running != BHASA_ASYNC_WORKERS) assert(pthread_cond_wait(&changed, &state) == 0);
    assert(identities_count == BHASA_ASYNC_WORKERS);
    assert(pthread_mutex_unlock(&state) == 0);
    for (int i = BHASA_ASYNC_WORKERS; i < BHASA_ASYNC_WORKERS + BHASA_ASYNC_QUEUE_CAPACITY; ++i) {
        tasks[i] = bhasa_rt_async_spawn_i64(work, i);
    }
    pthread_t producer;
    assert(pthread_create(&producer, NULL, blocked_submitter, NULL) == 0);
    assert(pthread_mutex_lock(&state) == 0);
    while (!backpressure_seen) assert(pthread_cond_wait(&changed, &state) == 0);
    assert(!submitter_returned);
    assert(running == BHASA_ASYNC_WORKERS);
    assert(atomic_load(&created) == BHASA_ASYNC_WORKERS);
    open_gate = 1;
    assert(pthread_cond_broadcast(&changed) == 0);
    assert(pthread_mutex_unlock(&state) == 0);
    assert(pthread_join(producer, NULL) == 0);
    for (int i = BHASA_ASYNC_WORKERS + BHASA_ASYNC_QUEUE_CAPACITY - 1; i >= 0; --i) {
        int64_t result = i % 2 == 0 ? bhasa_rt_await_i64(tasks[i]) : bhasa_rt_join_i64(tasks[i]);
        assert(result == i * 3 + 1);
    }
    assert(atomic_load(&allocations) == 0);
    assert(maximum_running == BHASA_ASYNC_WORKERS);
    assert(completed == BHASA_ASYNC_WORKERS + BHASA_ASYNC_QUEUE_CAPACITY + 1);
}

static void *producer(void *opaque) {
    int64_t base = *(int64_t *)opaque;
    for (int round = 0; round < 12; ++round) {
        void *tasks[128];
        for (int i = 0; i < 128; ++i) tasks[i] = bhasa_rt_async_spawn_i64(work, base + i);
        for (int i = 127; i >= 0; --i) assert(bhasa_rt_await_i64(tasks[i]) == (base + i) * 3 + 1);
    }
    return NULL;
}

static void concurrent_producers(void) {
    pthread_t producers[4];
    int64_t bases[4] = {1000, 2000, 3000, 4000};
    for (int i = 0; i < 4; ++i) assert(pthread_create(&producers[i], NULL, producer, &bases[i]) == 0);
    for (int i = 0; i < 4; ++i) assert(pthread_join(producers[i], NULL) == 0);
    assert(atomic_load(&created) == BHASA_ASYNC_WORKERS);
    assert(atomic_load(&allocations) == 0);
}

static int64_t increment(int64_t argument) { return argument + 1; }
static int64_t legacy_nested(int64_t argument) {
    void *task = bhasa_rt_spawn_i64(increment, argument);
    return bhasa_rt_join_i64(task);
}
static int64_t forbidden_spawn(int64_t argument) {
    void *task = bhasa_rt_async_spawn_i64(increment, argument);
    return bhasa_rt_join_i64(task);
}
static int64_t forbidden_await(int64_t argument) {
    void *task = bhasa_rt_spawn_i64(increment, argument);
    return bhasa_rt_await_i64(task);
}
static int64_t forbidden_pool_join(int64_t argument) {
    (void)argument;
    return bhasa_rt_join_i64(shared_task);
}

static void verify_shutdown(void) {
    assert(atomic_load(&allocations) == 0);
    assert(atomic_load(&created) == expected_threads);
    assert(atomic_load(&joined) == atomic_load(&created));
    puts("bounded async runtime passed");
}

int main(int argc, char **argv) {
    if (argc == 2) {
        if (strcmp(argv[1], "cold-start") == 0) {
            assert(atexit(verify_shutdown) == 0);
            open_gate = 1;
            concurrent_producers();
            return 0;
        }
        if (strcmp(argv[1], "nested-pool") == 0) {
            void *task = bhasa_rt_async_spawn_i64(forbidden_spawn, 1);
            return (int)bhasa_rt_await_i64(task);
        }
        if (strcmp(argv[1], "nested-thread") == 0) {
            void *task = bhasa_rt_spawn_i64(forbidden_spawn, 1);
            return (int)bhasa_rt_join_i64(task);
        }
        if (strcmp(argv[1], "await-worker") == 0) {
            void *task = bhasa_rt_spawn_i64(forbidden_await, 1);
            return (int)bhasa_rt_join_i64(task);
        }
        if (strcmp(argv[1], "join-pool-worker") == 0) {
            shared_task = bhasa_rt_async_spawn_i64(increment, 1);
            void *task = bhasa_rt_spawn_i64(forbidden_pool_join, 1);
            return (int)bhasa_rt_join_i64(task);
        }
        if (strcmp(argv[1], "null-worker") == 0) bhasa_rt_async_spawn_i64(NULL, 1);
        if (strcmp(argv[1], "null-handle") == 0) bhasa_rt_await_i64(NULL);
        return 99;
    }
    assert(atexit(verify_shutdown) == 0);
    saturation();
    concurrent_producers();
    assert(identities_count == BHASA_ASYNC_WORKERS);
    assert(maximum_running == BHASA_ASYNC_WORKERS);
    void *native = bhasa_rt_spawn_i64(legacy_nested, 41);
    assert(bhasa_rt_await_i64(native) == 42);
    void *pooled = bhasa_rt_async_spawn_i64(legacy_nested, 99);
    assert(bhasa_rt_join_i64(pooled) == 100);
    expected_threads += 3;
    assert(atomic_load(&allocations) == 0);
    return 0;
}
