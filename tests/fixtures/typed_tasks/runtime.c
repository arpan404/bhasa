#define _POSIX_C_SOURCE 200809L
#include "concurrency.h"
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static _Atomic long allocations;
static _Atomic int created;
static _Atomic int joined;
static _Atomic int inputs_consumed;
static _Atomic int outputs_freed;
static _Atomic int fail_allocation;
static pthread_mutex_t mutex=PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t changed=PTHREAD_COND_INITIALIZER;
static int gate;
static int running;
static int maximum;
static int identities_count;
static pthread_t identities[BHASA_ASYNC_WORKERS];
static int backpressure;
static int submitter_returned;
static _Thread_local int observe_submitter;
static void *shared_task;

void *bhasa_typed_test_malloc(size_t size) {
    if (atomic_load(&fail_allocation)) return NULL;
    void *p=malloc(size);
    if(p) atomic_fetch_add(&allocations,1);
    return p;
}
void bhasa_typed_test_free(void *p) {if(p) atomic_fetch_sub(&allocations,1);free(p);}
int bhasa_typed_test_create(pthread_t *thread,const pthread_attr_t *attributes,void *(*start)(void *),void *argument) {
    int status=pthread_create(thread,attributes,start,argument);
    if(status==0) atomic_fetch_add(&created,1);
    return status;
}
int bhasa_typed_test_join(pthread_t thread,void **result) {
    int status=pthread_join(thread,result);
    if(status==0) atomic_fetch_add(&joined,1);
    return status;
}
int bhasa_typed_test_wait(pthread_cond_t *condition,pthread_mutex_t *lock) {
    if(observe_submitter) {
        assert(pthread_mutex_lock(&mutex)==0);backpressure=1;
        assert(pthread_cond_broadcast(&changed)==0);
        assert(pthread_mutex_unlock(&mutex)==0);
    }
    return pthread_cond_wait(condition,lock);
}

struct frame {int64_t value;double fraction;unsigned char *bytes;};
static struct frame *input(int64_t value) {
    struct frame *frame=bhasa_rt_task_alloc((int64_t)sizeof(*frame));
    assert((uintptr_t)frame % _Alignof(max_align_t)==0);
    frame->value=value;frame->fraction=0.25;
    unsigned char original[8]={'o','w','n','e','d',0,255,0};
    frame->bytes=bhasa_rt_task_alloc(8);
    memcpy(frame->bytes,original,8);
    original[0]='X'; /* The submitted frame already has independent bytes. */
    return frame;
}
static void consume(struct frame *frame) {
    assert(frame && frame->fraction==0.25);
    assert(memcmp(frame->bytes,"owned",5)==0 && frame->bytes[6]==255);
    frame->bytes[0]='m';
    bhasa_rt_task_free(frame->bytes);bhasa_rt_task_free(frame);
    atomic_fetch_add(&inputs_consumed,1);
}
static void finish(void *opaque,int64_t expected) {
    struct frame *frame=opaque;
    assert(frame && frame->value==expected && frame->fraction==1.5);
    assert(memcmp(frame->bytes,"result",7)==0);
    bhasa_rt_task_free(frame->bytes);bhasa_rt_task_free(frame);
    atomic_fetch_add(&outputs_freed,1);
}
static void *owned(void *opaque) {
    struct frame *argument=opaque;
    int64_t value=argument->value*3+1;
    consume(argument);
    struct frame *result=bhasa_rt_task_alloc((int64_t)sizeof(*result));
    result->value=value;result->fraction=1.5;result->bytes=bhasa_rt_task_alloc(7);
    memcpy(result->bytes,"result",7);
    return result;
}
static void *void_owned(void *argument) {consume(argument);return NULL;}
static int64_t scalar(int64_t argument) {return argument*3+1;}
static void wait_gate(void) {
    assert(pthread_mutex_lock(&mutex)==0);
    int found=0;
    for(int i=0;i<identities_count;++i) if(pthread_equal(identities[i],pthread_self())) found=1;
    if(!found) {assert(identities_count<BHASA_ASYNC_WORKERS);identities[identities_count++]=pthread_self();}
    ++running;if(running>maximum) maximum=running;
    assert(running<=BHASA_ASYNC_WORKERS);
    assert(pthread_cond_broadcast(&changed)==0);
    while(!gate) assert(pthread_cond_wait(&changed,&mutex)==0);
    --running;
    assert(pthread_mutex_unlock(&mutex)==0);
}
static int64_t gated_scalar(int64_t value) {wait_gate();return scalar(value);}
static void *gated_owned(void *value) {wait_gate();return owned(value);}
static void *blocked_producer(void *unused) {
    (void)unused;observe_submitter=1;
    void *task=bhasa_rt_async_spawn_owned(gated_owned,input(100));
    observe_submitter=0;
    assert(pthread_mutex_lock(&mutex)==0);submitter_returned=1;assert(pthread_mutex_unlock(&mutex)==0);
    finish(bhasa_rt_await_owned(task),301);return NULL;
}
static void mixed_saturation(void) {
    void *tasks[BHASA_ASYNC_WORKERS+BHASA_ASYNC_QUEUE_CAPACITY];
    for(int i=0;i<BHASA_ASYNC_WORKERS;++i)
        tasks[i]=i%2 ? bhasa_rt_async_spawn_i64(gated_scalar,i) : bhasa_rt_async_spawn_owned(gated_owned,input(i));
    assert(pthread_mutex_lock(&mutex)==0);
    while(running!=BHASA_ASYNC_WORKERS) assert(pthread_cond_wait(&changed,&mutex)==0);
    assert(pthread_mutex_unlock(&mutex)==0);
    for(int i=BHASA_ASYNC_WORKERS;i<BHASA_ASYNC_WORKERS+BHASA_ASYNC_QUEUE_CAPACITY;++i)
        tasks[i]=i%2 ? bhasa_rt_async_spawn_i64(gated_scalar,i) : bhasa_rt_async_spawn_owned(gated_owned,input(i));
    pthread_t producer;
    assert(pthread_create(&producer,NULL,blocked_producer,NULL)==0);
    assert(pthread_mutex_lock(&mutex)==0);
    while(!backpressure) assert(pthread_cond_wait(&changed,&mutex)==0);
    assert(!submitter_returned && maximum==BHASA_ASYNC_WORKERS);
    assert(atomic_load(&created)==BHASA_ASYNC_WORKERS);
    gate=1;assert(pthread_cond_broadcast(&changed)==0);assert(pthread_mutex_unlock(&mutex)==0);
    assert(pthread_join(producer,NULL)==0);
    for(int i=BHASA_ASYNC_WORKERS+BHASA_ASYNC_QUEUE_CAPACITY-1;i>=0;--i) {
        if(i%2) assert(bhasa_rt_join_i64(tasks[i])==i*3+1);
        else finish(bhasa_rt_join_owned(tasks[i]),i*3+1);
    }
    assert(atomic_load(&allocations)==0);
}
static void *producer(void *opaque) {
    int64_t base=*(int64_t *)opaque;
    for(int round=0;round<8;++round) {
        void *tasks[128];
        for(int i=0;i<128;++i) tasks[i]=i%2 ? bhasa_rt_async_spawn_i64(scalar,base+i) : bhasa_rt_async_spawn_owned(owned,input(base+i));
        for(int i=127;i>=0;--i) {
            if(i%2) assert(bhasa_rt_await_i64(tasks[i])==(base+i)*3+1);
            else finish(bhasa_rt_await_owned(tasks[i]),(base+i)*3+1);
        }
    }
    return NULL;
}
static void concurrent_producers(void) {
    pthread_t threads[4];int64_t bases[4]={1000,2000,3000,4000};
    for(int i=0;i<4;++i) assert(pthread_create(&threads[i],NULL,producer,&bases[i])==0);
    for(int i=0;i<4;++i) assert(pthread_join(threads[i],NULL)==0);
    assert(atomic_load(&created)==BHASA_ASYNC_WORKERS);
    assert(atomic_load(&allocations)==0);
}
static void *nested_owned(void *opaque) {
    struct frame *argument=opaque;int64_t value=argument->value;
    consume(argument);
    void *native=bhasa_rt_spawn_i64(scalar,value);
    assert(bhasa_rt_join_i64(native)==value*3+1);
    native=bhasa_rt_spawn_owned(owned,input(value));
    return bhasa_rt_join_owned(native);
}
static int64_t nested_scalar(int64_t value) {
    void *native=bhasa_rt_spawn_owned(owned,input(value));
    struct frame *result=bhasa_rt_join_owned(native);
    int64_t answer=result->value;finish(result,value*3+1);return answer;
}
static void *forbidden_owned_spawn(void *argument) {return bhasa_rt_async_spawn_owned(owned,argument);}
static void *forbidden_scalar_spawn(void *argument) {consume(argument);return bhasa_rt_async_spawn_i64(scalar,1);}
static void *forbidden_owned_await(void *argument) {void *task=bhasa_rt_spawn_owned(owned,argument);return bhasa_rt_await_owned(task);}
static int64_t forbidden_from_scalar(int64_t value) {void *task=bhasa_rt_async_spawn_owned(owned,input(value));bhasa_rt_task_free(task);return 0;}
static void *forbidden_pool_join(void *argument) {consume(argument);return bhasa_rt_join_owned(shared_task);}

static void verify_shutdown(void) {
    assert(atomic_load(&allocations)==0);
    assert(atomic_load(&joined)==atomic_load(&created));
    assert(atomic_load(&inputs_consumed)>=atomic_load(&outputs_freed));
    puts("typed task runtime passed");
}
int main(int argc,char **argv) {
    alarm(30);
    if(argc==2 && strcmp(argv[1],"cold-start")==0) {
        assert(atexit(verify_shutdown)==0);concurrent_producers();return 0;
    }
    if(argc==2) {
        const char *mode=argv[1];
        if(strcmp(mode,"null-worker")==0) bhasa_rt_spawn_owned(NULL,NULL);
        if(strcmp(mode,"null-pool-worker")==0) bhasa_rt_async_spawn_owned(NULL,NULL);
        if(strcmp(mode,"null-handle")==0) bhasa_rt_join_owned(NULL);
        if(strcmp(mode,"negative-size")==0) bhasa_rt_task_alloc(-1);
        if(strcmp(mode,"allocation")==0) {atomic_store(&fail_allocation,1);bhasa_rt_task_alloc(1);}
        if(strcmp(mode,"task-allocation")==0) {atomic_store(&fail_allocation,1);bhasa_rt_spawn_owned(owned,NULL);}
        if(strcmp(mode,"scalar-as-owned")==0) bhasa_rt_join_owned(bhasa_rt_spawn_i64(scalar,1));
        if(strcmp(mode,"owned-as-scalar")==0) bhasa_rt_join_i64(bhasa_rt_async_spawn_owned(owned,input(1)));
        if(strcmp(mode,"nested-owned-pool")==0) bhasa_rt_join_owned(bhasa_rt_async_spawn_owned(forbidden_owned_spawn,input(1)));
        if(strcmp(mode,"nested-owned-thread")==0) bhasa_rt_join_owned(bhasa_rt_spawn_owned(forbidden_owned_spawn,input(1)));
        if(strcmp(mode,"nested-scalar-submit")==0) bhasa_rt_join_owned(bhasa_rt_spawn_owned(forbidden_scalar_spawn,input(1)));
        if(strcmp(mode,"scalar-owned-submit")==0) bhasa_rt_join_i64(bhasa_rt_spawn_i64(forbidden_from_scalar,1));
        if(strcmp(mode,"owned-await-worker")==0) bhasa_rt_join_owned(bhasa_rt_spawn_owned(forbidden_owned_await,input(1)));
        if(strcmp(mode,"owned-join-pool-worker")==0) {
            shared_task=bhasa_rt_async_spawn_owned(owned,input(1));
            bhasa_rt_join_owned(bhasa_rt_spawn_owned(forbidden_pool_join,input(2)));
        }
        return 99;
    }
    assert(atexit(verify_shutdown)==0);
    mixed_saturation();concurrent_producers();
    assert(identities_count==BHASA_ASYNC_WORKERS);
    for(int round=0;round<32;++round) {
        finish(bhasa_rt_await_owned(bhasa_rt_spawn_owned(owned,input(round))),round*3+1);
        finish(bhasa_rt_join_owned(bhasa_rt_async_spawn_owned(owned,input(round))),round*3+1);
    }
    finish(bhasa_rt_join_owned(bhasa_rt_spawn_owned(nested_owned,input(41))),124);
    finish(bhasa_rt_await_owned(bhasa_rt_async_spawn_owned(nested_owned,input(42))),127);
    assert(bhasa_rt_await_i64(bhasa_rt_spawn_i64(nested_scalar,43))==130);
    assert(bhasa_rt_join_i64(bhasa_rt_async_spawn_i64(nested_scalar,44))==133);
    assert(bhasa_rt_join_owned(bhasa_rt_spawn_owned(void_owned,input(1)))==NULL);
    assert(bhasa_rt_await_owned(bhasa_rt_async_spawn_owned(void_owned,input(2)))==NULL);
    void *zero=bhasa_rt_task_alloc(0);assert(zero);bhasa_rt_task_free(zero);bhasa_rt_task_free(NULL);
    assert(atomic_load(&allocations)==0);
    /* Two outer nested inputs and two void jobs have no separately retained result. */
    assert(atomic_load(&inputs_consumed)==atomic_load(&outputs_freed)+4);
    return 0;
}
