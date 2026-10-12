#ifndef BHASA_CHANNEL_H
#define BHASA_CHANNEL_H
#include <stdint.h>

/* Compiler-emitted immutable, nominal descriptor. drop destroys the payload and
 * frees its frame. flags == 1 certifies recursively checked thread transfer.
 * Foreign callbacks/pointers are trusted; this is not pointer authentication. */
struct bhasa_channel_type {
    void (*drop)(void *);
    uint64_t flags;
};
#define BHASA_CHANNEL_TRANSFER_CHECKED UINT64_C(1)
enum bhasa_channel_status {
    BHASA_CHANNEL_OK = 0,
    BHASA_CHANNEL_CLOSED = 1,
    BHASA_CHANNEL_WOULD_BLOCK = 2
};
/* capacity must be positive. Returns sender; writes receiver only on success.
 * Each endpoint has one owner. Clone creates another owned endpoint of the same
 * direction. Calls may share state through clones, never race drop on one owner.
 * No payload copies occur in this ABI: generated callers acquire an independent
 * typed frame before send. Success transfers that frame, failure preserves it.
 * Receive success writes an owned frame; all failures leave output untouched. */
void *bhasa_rt_channel_new(const struct bhasa_channel_type *, int64_t capacity,
                           void **receiver);
void *bhasa_rt_channel_clone(void *endpoint);
void bhasa_rt_channel_drop(void *endpoint);
/* Explicit close or last sender drop forbids sends and lets receivers drain.
 * Last receiver drop closes and drops unread payloads exactly once. */
void bhasa_rt_channel_close(void *endpoint);
int64_t bhasa_rt_channel_try_send(void *sender,
    const struct bhasa_channel_type *, void *frame);
int64_t bhasa_rt_channel_try_recv(void *receiver,
    const struct bhasa_channel_type *, void **frame);
/* Blocking calls are forbidden in every Bhasa task worker. Nonblocking calls
 * remain allowed. This conservative first contract prevents pool starvation. */
int64_t bhasa_rt_channel_send(void *sender,
    const struct bhasa_channel_type *, void *frame);
int64_t bhasa_rt_channel_recv(void *receiver,
    const struct bhasa_channel_type *, void **frame);
#endif
