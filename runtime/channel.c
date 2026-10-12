#include "channel.h"
#include "concurrency.h"
#include <pthread.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

struct channel {
    pthread_mutex_t mutex;
    pthread_cond_t ready;
    pthread_cond_t space;
    const struct bhasa_channel_type *type;
    void **queue;
    size_t capacity, head, count, senders, receivers;
    int closed;
};
struct endpoint { struct channel *channel; int sender; };

static _Noreturn void fail(const char *message) {
    fprintf(stderr, "Bhasa runtime: channel %s\n", message);
    abort();
}
static void check(int code) { if (code != 0) fail("synchronization failed"); }
static void *allocate(size_t size) {
    void *value = malloc(size);
    if (value == NULL) fail("allocation failed");
    return value;
}
static struct endpoint *endpoint(void *value) {
    if (value == NULL) fail("null endpoint");
    return value;
}
static void wake(struct channel *channel) {
    check(pthread_cond_broadcast(&channel->ready));
    check(pthread_cond_broadcast(&channel->space));
}
static struct channel *typed(void *value, const struct bhasa_channel_type *type,
                             int sender) {
    struct endpoint *owner = endpoint(value);
    if (owner->sender != sender) fail("endpoint direction mismatch");
    if (type != owner->channel->type) fail("payload type mismatch");
    return owner->channel;
}
void *bhasa_rt_channel_new(const struct bhasa_channel_type *type,
                           int64_t capacity, void **receiver) {
    if (type == NULL || type->drop == NULL ||
        type->flags != BHASA_CHANNEL_TRANSFER_CHECKED) fail("unchecked payload type");
    if (capacity <= 0 || (uint64_t)capacity > SIZE_MAX / sizeof(void *))
        fail("invalid capacity");
    if (receiver == NULL) fail("null receiver output");
    struct channel *channel = allocate(sizeof(*channel));
    *channel = (struct channel){ .type = type, .capacity = (size_t)capacity,
                                .senders = 1, .receivers = 1 };
    channel->queue = allocate(channel->capacity * sizeof(void *));
    check(pthread_mutex_init(&channel->mutex, NULL));
    check(pthread_cond_init(&channel->ready, NULL));
    check(pthread_cond_init(&channel->space, NULL));
    struct endpoint *sender = allocate(sizeof(*sender));
    struct endpoint *reader = allocate(sizeof(*reader));
    *sender = (struct endpoint){channel, 1};
    *reader = (struct endpoint){channel, 0};
    *receiver = reader;
    return sender;
}
void *bhasa_rt_channel_clone(void *value) {
    struct endpoint *owner = endpoint(value);
    struct endpoint *copy = allocate(sizeof(*copy));
    *copy = *owner;
    struct channel *channel = owner->channel;
    check(pthread_mutex_lock(&channel->mutex));
    size_t *count = owner->sender ? &channel->senders : &channel->receivers;
    if (*count == SIZE_MAX) fail("endpoint count overflow");
    ++*count;
    check(pthread_mutex_unlock(&channel->mutex));
    return copy;
}
void bhasa_rt_channel_close(void *value) {
    struct channel *channel = endpoint(value)->channel;
    check(pthread_mutex_lock(&channel->mutex));
    channel->closed = 1;
    wake(channel);
    check(pthread_mutex_unlock(&channel->mutex));
}
void bhasa_rt_channel_drop(void *value) {
    struct endpoint *owner = endpoint(value);
    struct channel *channel = owner->channel;
    void **discard = NULL;
    size_t head = 0, count = 0, capacity = channel->capacity;
    /* Capture the callback before another endpoint can free channel. */
    void (*drop)(void *) = channel->type->drop;
    check(pthread_mutex_lock(&channel->mutex));
    if (owner->sender) {
        if (--channel->senders == 0) channel->closed = 1;
    } else if (--channel->receivers == 0) {
        channel->closed = 1;
        discard = channel->queue;
        channel->queue = NULL;
        head = channel->head;
        count = channel->count;
        channel->count = 0;
    }
    wake(channel);
    int destroy = channel->senders == 0 && channel->receivers == 0;
    check(pthread_mutex_unlock(&channel->mutex));
    free(owner);
    /* Destructors execute without the queue lock. They cannot observe frames
     * still queued or received; the detached queue belongs to this drop only. */
    for (size_t i = 0; i < count; ++i) drop(discard[(head + i) % capacity]);
    free(discard);
    if (destroy) {
        check(pthread_cond_destroy(&channel->ready));
        check(pthread_cond_destroy(&channel->space));
        check(pthread_mutex_destroy(&channel->mutex));
        free(channel->queue);
        free(channel);
    }
}
static int64_t send(void *owner, const struct bhasa_channel_type *type,
                    void *frame, int blocking) {
    struct channel *channel = typed(owner, type, 1);
    if (frame == NULL) fail("null payload frame");
    if (blocking) bhasa_rt_require_nonworker("blocking channel send");
    check(pthread_mutex_lock(&channel->mutex));
    while (!channel->closed && channel->count == channel->capacity && blocking)
        check(pthread_cond_wait(&channel->space, &channel->mutex));
    int64_t status = BHASA_CHANNEL_CLOSED;
    if (!channel->closed) {
        if (channel->count == channel->capacity) status = BHASA_CHANNEL_WOULD_BLOCK;
        else {
            channel->queue[(channel->head + channel->count) % channel->capacity] = frame;
            ++channel->count;
            status = BHASA_CHANNEL_OK;
            check(pthread_cond_signal(&channel->ready));
        }
    }
    check(pthread_mutex_unlock(&channel->mutex));
    return status;
}
static int64_t recv(void *owner, const struct bhasa_channel_type *type,
                    void **frame, int blocking) {
    struct channel *channel = typed(owner, type, 0);
    if (frame == NULL) fail("null payload output");
    if (blocking) bhasa_rt_require_nonworker("blocking channel receive");
    check(pthread_mutex_lock(&channel->mutex));
    while (!channel->closed && channel->count == 0 && blocking)
        check(pthread_cond_wait(&channel->ready, &channel->mutex));
    int64_t status = channel->closed ? BHASA_CHANNEL_CLOSED : BHASA_CHANNEL_WOULD_BLOCK;
    if (channel->count != 0) {
        *frame = channel->queue[channel->head];
        channel->head = (channel->head + 1) % channel->capacity;
        --channel->count;
        status = BHASA_CHANNEL_OK;
        check(pthread_cond_signal(&channel->space));
    }
    check(pthread_mutex_unlock(&channel->mutex));
    return status;
}
int64_t bhasa_rt_channel_try_send(void *owner, const struct bhasa_channel_type *type, void *frame) {
    return send(owner, type, frame, 0);
}
int64_t bhasa_rt_channel_send(void *owner, const struct bhasa_channel_type *type, void *frame) {
    return send(owner, type, frame, 1);
}
int64_t bhasa_rt_channel_try_recv(void *owner, const struct bhasa_channel_type *type, void **frame) {
    return recv(owner, type, frame, 0);
}
int64_t bhasa_rt_channel_recv(void *owner, const struct bhasa_channel_type *type, void **frame) {
    return recv(owner, type, frame, 1);
}
