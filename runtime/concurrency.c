#include "concurrency.h"

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

struct bhasa_task {
    int owned;
    int pooled;
    pthread_t thread;
    union { int64_t (*scalar)(int64_t); void *(*owned)(void *); } worker;
    union { int64_t scalar; void *owned; } argument;
    union { int64_t scalar; void *owned; } result;
    int done;
};

static _Thread_local int in_worker;

static struct {
    pthread_mutex_t mutex;
    pthread_cond_t ready;
    pthread_cond_t space;
    pthread_cond_t finished;
    pthread_t workers[BHASA_ASYNC_WORKERS];
    struct bhasa_task *queue[BHASA_ASYNC_QUEUE_CAPACITY];
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

static void execute_task(struct bhasa_task *task) {
    if (task->owned) {
        void *argument = task->argument.owned;
        task->argument.owned = NULL;
        /* The generated wrapper now owns the argument and returns an owned
         * result frame, or NULL for void. No generic runtime destructor can
         * replace its type-specific cleanup. */
        task->result.owned = task->worker.owned(argument);
    } else {
        task->result.scalar = task->worker.scalar(task->argument.scalar);
    }
}

static void *run_task(void *opaque) {
    in_worker = 1;
    execute_task(opaque);
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
        struct bhasa_task *task = pool.queue[pool.head];
        pool.queue[pool.head] = NULL;
        pool.head = (pool.head + 1) % BHASA_ASYNC_QUEUE_CAPACITY;
        --pool.count;
        task_check("async queue space", pthread_cond_signal(&pool.space));
        task_check("async queue unlock", pthread_mutex_unlock(&pool.mutex));

        execute_task(task);

        task_check("async result lock", pthread_mutex_lock(&pool.mutex));
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

static struct bhasa_task *new_task(int owned) {
    struct bhasa_task *task = malloc(sizeof(*task));
    if (task == NULL) task_error("cannot allocate a task", 0);
    *task = (struct bhasa_task){ .owned = owned };
    return task;
}

static void *submit_pool(struct bhasa_task *task) {
    task_check("async initialization", pthread_once(&pool_once, start_pool));
    task->pooled = 1;
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

static void *submit_thread(struct bhasa_task *task) {
    int status = pthread_create(&task->thread, NULL, run_task, task);
    if (status != 0) {
        free(task);
        task_error("thread creation", status);
    }
    return task;
}

void *bhasa_rt_async_spawn_i64(int64_t (*worker)(int64_t), int64_t argument) {
    if (worker == NULL) task_error("cannot spawn a null worker", 0);
    if (in_worker) task_error("async_spawn is forbidden inside task workers", 0);
    struct bhasa_task *task = new_task(0);
    task->worker.scalar = worker;
    task->argument.scalar = argument;
    return submit_pool(task);
}

void *bhasa_rt_async_spawn_owned(void *(*worker)(void *), void *argument) {
    if (worker == NULL) task_error("cannot spawn a null worker", 0);
    if (in_worker) task_error("async_spawn is forbidden inside task workers", 0);
    struct bhasa_task *task = new_task(1);
    task->worker.owned = worker;
    task->argument.owned = argument;
    return submit_pool(task);
}

void *bhasa_rt_spawn_i64(int64_t (*worker)(int64_t), int64_t argument) {
    if (worker == NULL) task_error("cannot spawn a null worker", 0);
    struct bhasa_task *task = new_task(0);
    task->worker.scalar = worker;
    task->argument.scalar = argument;
    return submit_thread(task);
}

void *bhasa_rt_spawn_owned(void *(*worker)(void *), void *argument) {
    if (worker == NULL) task_error("cannot spawn a null worker", 0);
    struct bhasa_task *task = new_task(1);
    task->worker.owned = worker;
    task->argument.owned = argument;
    return submit_thread(task);
}

static struct bhasa_task *join_task(void *handle, int owned) {
    if (handle == NULL) task_error("cannot join a null task", 0);
    struct bhasa_task *task = handle;
    if (task->owned != owned) task_error("task result ABI mismatch", 0);
    if (task->pooled) {
        if (in_worker) task_error("await is forbidden inside task workers", 0);
        task_check("async await lock", pthread_mutex_lock(&pool.mutex));
        while (!task->done) {
            task_check("async await wait", pthread_cond_wait(&pool.finished, &pool.mutex));
        }
        task_check("async await unlock", pthread_mutex_unlock(&pool.mutex));
    } else {
        task_check("thread join", pthread_join(task->thread, NULL));
    }
    return task;
}

int64_t bhasa_rt_join_i64(void *handle) {
    struct bhasa_task *task = join_task(handle, 0);
    int64_t result = task->result.scalar;
    free(task);
    return result;
}

void *bhasa_rt_join_owned(void *handle) {
    struct bhasa_task *task = join_task(handle, 1);
    void *result = task->result.owned;
    free(task);
    return result;
}

int64_t bhasa_rt_await_i64(void *handle) {
    if (in_worker) task_error("await is forbidden inside task workers", 0);
    return bhasa_rt_join_i64(handle);
}

void *bhasa_rt_await_owned(void *handle) {
    if (in_worker) task_error("await is forbidden inside task workers", 0);
    return bhasa_rt_join_owned(handle);
}

void *bhasa_rt_task_alloc(int64_t bytes) {
    if (bytes < 0 || (uint64_t)bytes > SIZE_MAX) task_error("invalid task frame size", 0);
    void *frame = malloc(bytes == 0 ? 1 : (size_t)bytes);
    if (frame == NULL) task_error("cannot allocate a task frame", 0);
    return frame;
}

void bhasa_rt_task_free(void *frame) { free(frame); }
