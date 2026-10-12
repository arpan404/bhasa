#include "channel.h"
#include "concurrency.h"
#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static atomic_long live, drops, waits;
void *bhasa_channel_test_malloc(size_t n) {
    void *p = malloc(n);
    if (p != NULL) atomic_fetch_add(&live, 1);
    return p;
}
void bhasa_channel_test_free(void *p) {
    if (p != NULL) atomic_fetch_sub(&live, 1);
    free(p);
}
int bhasa_channel_test_wait(pthread_cond_t *c, pthread_mutex_t *m) {
    atomic_fetch_add(&waits, 1);
    return pthread_cond_wait(c, m);
}
static void destroy(void *p) {
    atomic_fetch_add(&drops, 1);
    bhasa_channel_test_free(p);
}
static const struct bhasa_channel_type type = {destroy, BHASA_CHANNEL_TRANSFER_CHECKED};
static const struct bhasa_channel_type other = {destroy, BHASA_CHANNEL_TRANSFER_CHECKED};
static const struct bhasa_channel_type unchecked = {destroy, 0};
static void *payload(int64_t value) {
    int64_t *p = bhasa_channel_test_malloc(sizeof(*p));
    assert(p != NULL);
    *p = value;
    return p;
}
static void wait_for_block(long previous) {
    while (atomic_load(&waits) == previous) sched_yield();
}
struct operation { void *endpoint; void *frame; int64_t status; };
static void *blocked_send(void *opaque) {
    struct operation *op = opaque;
    op->status = bhasa_rt_channel_send(op->endpoint, &type, op->frame);
    return NULL;
}
static void *blocked_recv(void *opaque) {
    struct operation *op = opaque;
    op->status = bhasa_rt_channel_recv(op->endpoint, &type, &op->frame);
    return NULL;
}
static void *producer(void *opaque) {
    for (int64_t i = 0; i < 2000; ++i)
        assert(bhasa_rt_channel_send(opaque, &type, payload(i)) == BHASA_CHANNEL_OK);
    bhasa_rt_channel_drop(opaque);
    return NULL;
}
static void *consumer(void *opaque) {
    int64_t expected = 0;
    void *p = NULL;
    while (bhasa_rt_channel_recv(opaque, &type, &p) == BHASA_CHANNEL_OK) {
        assert(*(int64_t *)p == expected++);
        destroy(p);
    }
    assert(expected == 2000);
    bhasa_rt_channel_drop(opaque);
    return NULL;
}
static void basic(void) {
    void *receiver, *p = (void *)(uintptr_t)1;
    void *sender = bhasa_rt_channel_new(&type, 2, &receiver);
    assert(bhasa_rt_channel_try_recv(receiver, &type, &p) == BHASA_CHANNEL_WOULD_BLOCK);
    assert(p == (void *)(uintptr_t)1);
    assert(bhasa_rt_channel_try_send(sender, &type, payload(10)) == BHASA_CHANNEL_OK);
    assert(bhasa_rt_channel_try_send(sender, &type, payload(20)) == BHASA_CHANNEL_OK);
    void *rejected = payload(30);
    assert(bhasa_rt_channel_try_send(sender, &type, rejected) == BHASA_CHANNEL_WOULD_BLOCK);
    assert(*(int64_t *)rejected == 30);
    void *copy = bhasa_rt_channel_clone(sender);
    bhasa_rt_channel_drop(sender);
    assert(bhasa_rt_channel_try_recv(receiver, &type, &p) == BHASA_CHANNEL_OK);
    assert(*(int64_t *)p == 10); destroy(p);
    assert(bhasa_rt_channel_try_send(copy, &type, rejected) == BHASA_CHANNEL_OK);
    bhasa_rt_channel_close(receiver); bhasa_rt_channel_close(copy);
    rejected = payload(40);
    assert(bhasa_rt_channel_try_send(copy, &type, rejected) == BHASA_CHANNEL_CLOSED);
    destroy(rejected);
    bhasa_rt_channel_drop(copy);
    for (int64_t value = 20; value <= 30; value += 10) {
        assert(bhasa_rt_channel_recv(receiver, &type, &p) == BHASA_CHANNEL_OK);
        assert(*(int64_t *)p == value); destroy(p);
    }
    p = (void *)(uintptr_t)1;
    assert(bhasa_rt_channel_recv(receiver, &type, &p) == BHASA_CHANNEL_CLOSED);
    assert(p == (void *)(uintptr_t)1);
    bhasa_rt_channel_drop(receiver);
    assert(atomic_load(&live) == 0);
}
static void last_receiver(void) {
    void *receiver;
    void *sender = bhasa_rt_channel_new(&type, 1, &receiver);
    void *copy = bhasa_rt_channel_clone(receiver);
    assert(bhasa_rt_channel_send(sender, &type, payload(1)) == BHASA_CHANNEL_OK);
    bhasa_rt_channel_drop(receiver);
    long previous = atomic_load(&waits);
    struct operation op = {sender, payload(2), -1};
    pthread_t thread;
    assert(pthread_create(&thread, NULL, blocked_send, &op) == 0);
    wait_for_block(previous);
    long before = atomic_load(&drops);
    bhasa_rt_channel_drop(copy);
    assert(pthread_join(thread, NULL) == 0);
    assert(op.status == BHASA_CHANNEL_CLOSED);
    assert(atomic_load(&drops) == before + 1);
    assert(*(int64_t *)op.frame == 2); destroy(op.frame);
    bhasa_rt_channel_drop(sender);
    assert(atomic_load(&live) == 0);
}
static void backpressure(void) {
    void *receiver, *p;
    void *sender = bhasa_rt_channel_new(&type, 1, &receiver);
    assert(bhasa_rt_channel_send(sender, &type, payload(1)) == BHASA_CHANNEL_OK);
    long previous = atomic_load(&waits);
    struct operation op = {sender, payload(2), -1};
    pthread_t thread;
    assert(pthread_create(&thread, NULL, blocked_send, &op) == 0);
    wait_for_block(previous);
    assert(bhasa_rt_channel_recv(receiver, &type, &p) == BHASA_CHANNEL_OK);
    assert(*(int64_t *)p == 1); destroy(p);
    assert(pthread_join(thread, NULL) == 0);
    assert(op.status == BHASA_CHANNEL_OK);
    assert(bhasa_rt_channel_recv(receiver, &type, &p) == BHASA_CHANNEL_OK);
    assert(*(int64_t *)p == 2); destroy(p);
    previous = atomic_load(&waits);
    op = (struct operation){receiver, (void *)(uintptr_t)1, -1};
    assert(pthread_create(&thread, NULL, blocked_recv, &op) == 0);
    wait_for_block(previous);
    bhasa_rt_channel_drop(sender);
    assert(pthread_join(thread, NULL) == 0);
    assert(op.status == BHASA_CHANNEL_CLOSED);
    assert(op.frame == (void *)(uintptr_t)1);
    bhasa_rt_channel_drop(receiver);
    assert(atomic_load(&live) == 0);
}
struct drop_operation { void *endpoint; atomic_int *gate; };
static void *race_drop(void *opaque) {
    struct drop_operation *op = opaque;
    while (!atomic_load(op->gate)) sched_yield();
    bhasa_rt_channel_drop(op->endpoint);
    return NULL;
}
static void simultaneous_drop(void) {
    for (int i = 0; i < 100; ++i) {
        void *r, *s = bhasa_rt_channel_new(&type, 1, &r);
        assert(bhasa_rt_channel_send(s, &type, payload(i)) == BHASA_CHANNEL_OK);
        atomic_int gate = 0;
        struct drop_operation a = {s, &gate}, b = {r, &gate};
        pthread_t first, second;
        assert(pthread_create(&first, NULL, race_drop, &a) == 0);
        assert(pthread_create(&second, NULL, race_drop, &b) == 0);
        atomic_store(&gate, 1);
        assert(pthread_join(first, NULL) == 0);
        assert(pthread_join(second, NULL) == 0);
        assert(atomic_load(&live) == 0);
    }
}
static int worker_send;
static int64_t worker(int64_t unused) {
    (void)unused;
    void *r, *s = bhasa_rt_channel_new(&type, 1, &r);
    /* Nonblocking operations are safe at the same guarded worker entry. */
    void *p = payload(5);
    assert(bhasa_rt_channel_try_send(s, &type, p) == BHASA_CHANNEL_OK);
    assert(bhasa_rt_channel_try_recv(r, &type, &p) == BHASA_CHANNEL_OK);
    destroy(p);
    if (worker_send) bhasa_rt_channel_send(s, &type, payload(6));
    bhasa_rt_channel_recv(r, &type, &p);
    return 99;
}
int main(int argc, char **argv) {
    if (argc > 1) {
        void *r, *s;
        if (!strcmp(argv[1], "unchecked")) bhasa_rt_channel_new(&unchecked, 1, &r);
        if (!strcmp(argv[1], "zero")) bhasa_rt_channel_new(&type, 0, &r);
        if (!strcmp(argv[1], "negative")) bhasa_rt_channel_new(&type, -1, &r);
        if (!strcmp(argv[1], "overflow")) bhasa_rt_channel_new(&type, INT64_MAX, &r);
        if (!strcmp(argv[1], "null-output")) bhasa_rt_channel_new(&type, 1, NULL);
        if (!strcmp(argv[1], "null-endpoint")) bhasa_rt_channel_drop(NULL);
        if (!strcmp(argv[1], "worker-send")) {
            worker_send = 1;
            bhasa_rt_join_i64(bhasa_rt_spawn_i64(worker, 0));
        }
        if (!strcmp(argv[1], "pool-worker-send")) {
            worker_send = 1;
            bhasa_rt_await_i64(bhasa_rt_async_spawn_i64(worker, 0));
        }
        if (!strcmp(argv[1], "worker")) bhasa_rt_join_i64(bhasa_rt_spawn_i64(worker, 0));
        if (!strcmp(argv[1], "pool-worker")) bhasa_rt_await_i64(bhasa_rt_async_spawn_i64(worker, 0));
        s = bhasa_rt_channel_new(&type, 1, &r);
        if (!strcmp(argv[1], "wrong-type")) bhasa_rt_channel_try_send(s, &other, payload(1));
        if (!strcmp(argv[1], "wrong-direction")) bhasa_rt_channel_try_send(r, &type, payload(1));
        if (!strcmp(argv[1], "null-frame")) bhasa_rt_channel_try_send(s, &type, NULL);
        if (!strcmp(argv[1], "null-recv")) bhasa_rt_channel_try_recv(r, &type, NULL);
        return 99;
    }
    basic(); last_receiver(); backpressure(); simultaneous_drop();
    for (int i = 0; i < 20; ++i) {
        void *r, *s = bhasa_rt_channel_new(&type, 3, &r);
        pthread_t a, b;
        assert(pthread_create(&a, NULL, producer, s) == 0);
        assert(pthread_create(&b, NULL, consumer, r) == 0);
        assert(pthread_join(a, NULL) == 0);
        assert(pthread_join(b, NULL) == 0);
        assert(atomic_load(&live) == 0);
    }
    assert(atomic_load(&drops) == 40108);
    puts("channel runtime passed");
    return 0;
}
