#ifndef BHASA_CONCURRENCY_H
#define BHASA_CONCURRENCY_H

#include <stdint.h>

/* Handles have exactly one owner. Join/await consume and release either kind.
 * The caller must consume every handle before normal process termination. */
void *bhasa_rt_spawn_i64(int64_t (*worker)(int64_t), int64_t argument);
int64_t bhasa_rt_join_i64(void *handle);

/* Fixed pool: four workers and at most 64 waiting jobs. A full queue blocks the
 * submitting caller until a worker takes a job. Completed, unconsumed handles
 * remain caller-owned and are not included in the queue limit.
 * Async submission/await from any Bhasa worker is forbidden, including calls
 * through helpers. The compiler checks this transitively; the runtime also
 * guards its worker entry points. Legacy nested spawn/join stays supported. */
#define BHASA_ASYNC_WORKERS 4
#define BHASA_ASYNC_QUEUE_CAPACITY 64
void *bhasa_rt_async_spawn_i64(int64_t (*worker)(int64_t), int64_t argument);
int64_t bhasa_rt_await_i64(void *handle);

#endif
