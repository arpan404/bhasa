#include "concurrency.h"

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

struct bhasa_task_i64 {
    int pooled;
    pthread_t thread;
    int64_t (*worker)(int64_t);
    int64_t argument;
    int64_t result;
    int done;
};

static _Thread_local int in_worker;

static struct {
    pthread_mutex_t mutex;
    pthread_cond_t ready;
    pthread_cond_t space;
    pthread_cond_t finished;
    pthread_t workers[BHASA_ASYNC_WORKERS];
    struct bhasa_task_i64 *queue[BHASA_ASYNC_QUEUE_CAPACITY];
    size_t head;
    size_t count;
    int stopping;
} pool = {
    .mutex = PTHREAD_MUTEX_INITIALIZER,
    .ready = PTHREAD_COND_INITIALIZER,
    .space = PTHREAD_COND_INITIALIZER,
    .finished = PTHREAD_COND_INITIALIZER,
};
static pthread_once_t pool_once = PTHREAD_ONCE_INIT;

static _Noreturn void task_error(const char *operation, int code) {
    if (code != 0) {
        fprintf(stderr, "Bhasa runtime: %s failed with OS error %d\n", operation, code);
    } else {
        fprintf(stderr, "Bhasa runtime: %s\n", operation);
    }
    abort();
}

static void *run_task(void *opaque) {
    struct bhasa_task_i64 *task = opaque;
    in_worker = 1;
    task->result = task->worker(task->argument);
    return NULL;
}

static void task_check(const char *operation, int status) {
    if (status != 0) task_error(operation, status);
}

static void *run_pool_worker(void *unused) {
    (void)unused;
    in_worker = 1;
    for (;;) {
        task_check("async queue lock", pthread_mutex_lock(&pool.mutex));
        while (pool.count == 0 && !pool.stopping) {
            task_check("async queue wait", pthread_cond_wait(&pool.ready, &pool.mutex));
        }
        if (pool.count == 0) {
            task_check("async queue unlock", pthread_mutex_unlock(&pool.mutex));
            return NULL;
        }
        struct bhasa_task_i64 *task = pool.queue[pool.head];
        pool.queue[pool.head] = NULL;
        pool.head = (pool.head + 1) % BHASA_ASYNC_QUEUE_CAPACITY;
        --pool.count;
        task_check("async queue space", pthread_cond_signal(&pool.space));
        task_check("async queue unlock", pthread_mutex_unlock(&pool.mutex));

        int64_t result = task->worker(task->argument);

        task_check("async result lock", pthread_mutex_lock(&pool.mutex));
        task->result = result;
        task->done = 1;
        task_check("async result wake", pthread_cond_broadcast(&pool.finished));
        task_check("async result unlock", pthread_mutex_unlock(&pool.mutex));
    }
}

static void stop_pool(void) {
    if (in_worker) task_error("cannot terminate the process from a task worker", 0);
    task_check("async shutdown lock", pthread_mutex_lock(&pool.mutex));
    pool.stopping = 1;
    task_check("async shutdown wake", pthread_cond_broadcast(&pool.ready));
    task_check("async shutdown space", pthread_cond_broadcast(&pool.space));
    task_check("async shutdown unlock", pthread_mutex_unlock(&pool.mutex));
    for (size_t i = 0; i < BHASA_ASYNC_WORKERS; ++i) {
        task_check("async worker join", pthread_join(pool.workers[i], NULL));
    }
    /* Keep static synchronization objects valid for later atexit callbacks.
     * They see stopping and cannot submit more work. */
}

static void start_pool(void) {
    for (size_t i = 0; i < BHASA_ASYNC_WORKERS; ++i) {
        task_check("async worker creation", pthread_create(&pool.workers[i], NULL, run_pool_worker, NULL));
    }
    if (atexit(stop_pool) != 0) task_error("cannot register async pool shutdown", 0);
}

void *bhasa_rt_async_spawn_i64(int64_t (*worker)(int64_t), int64_t argument) {
    if (worker == NULL) task_error("cannot spawn a null worker", 0);
    if (in_worker) task_error("async_spawn is forbidden inside task workers", 0);
    task_check("async initialization", pthread_once(&pool_once, start_pool));
    struct bhasa_task_i64 *task = malloc(sizeof(*task));
    if (task == NULL) task_error("cannot allocate a task", 0);
    task->pooled = 1;
    task->worker = worker;
    task->argument = argument;
    task->done = 0;
    task_check("async submit lock", pthread_mutex_lock(&pool.mutex));
    while (pool.count == BHASA_ASYNC_QUEUE_CAPACITY && !pool.stopping) {
        task_check("async backpressure", pthread_cond_wait(&pool.space, &pool.mutex));
    }
    if (pool.stopping) {
        free(task);
        task_error("async pool has stopped", 0);
    }
    pool.queue[(pool.head + pool.count) % BHASA_ASYNC_QUEUE_CAPACITY] = task;
    ++pool.count;
    task_check("async submit wake", pthread_cond_signal(&pool.ready));
    task_check("async submit unlock", pthread_mutex_unlock(&pool.mutex));
    return task;
}

static int64_t await_pool_task(struct bhasa_task_i64 *task) {
    if (in_worker) task_error("await is forbidden inside task workers", 0);
    task_check("async await lock", pthread_mutex_lock(&pool.mutex));
    while (!task->done) {
        task_check("async await wait", pthread_cond_wait(&pool.finished, &pool.mutex));
    }
    int64_t result = task->result;
    task_check("async await unlock", pthread_mutex_unlock(&pool.mutex));
    free(task);
    return result;
}

int64_t bhasa_rt_await_i64(void *handle) {
    if (in_worker) task_error("await is forbidden inside task workers", 0);
    return bhasa_rt_join_i64(handle);
}

void *bhasa_rt_spawn_i64(int64_t (*worker)(int64_t), int64_t argument) {
    if (worker == NULL) {
        task_error("cannot spawn a null worker", 0);
    }
    struct bhasa_task_i64 *task = malloc(sizeof(*task));
    if (task == NULL) {
        task_error("cannot allocate a task", 0);
    }
    task->worker = worker;
    task->argument = argument;
    task->pooled = 0;
    int status = pthread_create(&task->thread, NULL, run_task, task);
    if (status != 0) {
        free(task);
        task_error("thread creation", status);
    }
    return task;
}

int64_t bhasa_rt_join_i64(void *handle) {
    if (handle == NULL) {
        task_error("cannot join a null task", 0);
    }
    struct bhasa_task_i64 *task = handle;
    if (task->pooled) return await_pool_task(task);
    int status = pthread_join(task->thread, NULL);
    if (status != 0) {
        task_error("thread join", status);
    }
    int64_t result = task->result;
    free(task);
    return result;
}
