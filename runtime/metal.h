#ifndef BHASA_METAL_H
#define BHASA_METAL_H

#include "core.h"

/* Shared status codes for the synchronous Metal, CUDA and Vulkan adapters. */
enum bhasa_gpu_status {
    BHASA_GPU_OK = 0,
    BHASA_GPU_UNAVAILABLE = 1,
    BHASA_GPU_INVALID_ARGUMENT = 2,
    BHASA_GPU_COMPILE_ERROR = 3,
    BHASA_GPU_ENTRY_NOT_FOUND = 4,
    BHASA_GPU_PIPELINE_ERROR = 5,
    BHASA_GPU_LIMIT = 6,
    BHASA_GPU_ALLOCATION_ERROR = 7,
    BHASA_GPU_EXECUTION_ERROR = 8
};

int64_t bhasa_rt_gpu_available(void);
int64_t bhasa_rt_gpu_dispatch(const bhasa_buffer *source,
                              const bhasa_buffer *entry,
                              bhasa_buffer *data);

#endif
