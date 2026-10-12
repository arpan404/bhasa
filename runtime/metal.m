/* Optional Metal host support. Metal source is trusted foreign code. */
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>

#include "metal.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static int valid_buffer(const bhasa_buffer *value) {
    return value != NULL && value->len >= 0 && value->cap >= value->len &&
           (value->len == 0 || value->data != NULL) &&
           (uint64_t)value->len <= (uint64_t)NSUIntegerMax;
}

static void report_error(const char *operation, NSError *error) {
    const char *detail = error == nil ? NULL : [[error localizedDescription] UTF8String];
    fprintf(stderr, "bhasa GPU: %s%s%s\n", operation,
            detail == NULL ? "" : ": ", detail == NULL ? "" : detail);
}

int64_t bhasa_rt_gpu_available(void) {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        int64_t available = device != nil;
        [device release];
        return available;
    }
}

int64_t bhasa_rt_gpu_dispatch(const bhasa_buffer *source,
                              const bhasa_buffer *entry,
                              bhasa_buffer *data) {
    if (!valid_buffer(source) || !valid_buffer(entry) || !valid_buffer(data) ||
        source->len == 0 || entry->len == 0 ||
        memchr(source->data, 0, (size_t)source->len) != NULL ||
        memchr(entry->data, 0, (size_t)entry->len) != NULL) {
        report_error("invalid source, entry point, or data buffer", nil);
        return BHASA_GPU_INVALID_ARGUMENT;
    }

    @autoreleasepool {
        int64_t status = BHASA_GPU_OK;
        NSString *source_text = nil;
        NSString *entry_text = nil;
        id<MTLDevice> device = nil;
        id<MTLLibrary> library = nil;
        id<MTLFunction> function = nil;
        id<MTLComputePipelineState> pipeline = nil;
        id<MTLBuffer> gpu_data = nil;
        id<MTLCommandQueue> queue = nil;
        id<MTLCommandBuffer> command = nil;
        id<MTLComputeCommandEncoder> encoder = nil;
        NSError *error = nil;

        /* NSString copies the bytes, including when source aliases data. */
        source_text = [[NSString alloc] initWithBytes:source->data
                                               length:(NSUInteger)source->len
                                             encoding:NSUTF8StringEncoding];
        entry_text = [[NSString alloc] initWithBytes:entry->data
                                              length:(NSUInteger)entry->len
                                            encoding:NSUTF8StringEncoding];
        if (source_text == nil || entry_text == nil) {
            report_error("source and entry point must be UTF-8", nil);
            status = BHASA_GPU_INVALID_ARGUMENT;
            goto cleanup;
        }
        device = MTLCreateSystemDefaultDevice();
        if (device == nil) {
            report_error("no Metal device is available", nil);
            status = BHASA_GPU_UNAVAILABLE;
            goto cleanup;
        }
        /* Metal's thread_position_in_grid uses uint for this interface. */
        if ((uint64_t)data->len > UINT32_MAX ||
            (uint64_t)data->len > (uint64_t)[device maxBufferLength]) {
            report_error("data exceeds the device buffer or dispatch limit", nil);
            status = BHASA_GPU_LIMIT;
            goto cleanup;
        }
        library = [device newLibraryWithSource:source_text options:nil error:&error];
        if (library == nil) {
            report_error("Metal source compilation failed", error);
            status = BHASA_GPU_COMPILE_ERROR;
            goto cleanup;
        }
        function = [library newFunctionWithName:entry_text];
        if (function == nil) {
            report_error("Metal entry point was not found", nil);
            status = BHASA_GPU_ENTRY_NOT_FOUND;
            goto cleanup;
        }
        error = nil;
        pipeline = [device newComputePipelineStateWithFunction:function error:&error];
        if (pipeline == nil) {
            report_error("compute pipeline creation failed", error);
            status = BHASA_GPU_PIPELINE_ERROR;
            goto cleanup;
        }
        /* An empty dispatch still validates its device, source and pipeline. */
        if (data->len == 0) {
            goto cleanup;
        }
        gpu_data = [device newBufferWithBytes:data->data
                                      length:(NSUInteger)data->len
                                     options:MTLResourceStorageModeShared];
        queue = [device newCommandQueue];
        if (gpu_data == nil || queue == nil) {
            report_error("buffer or command queue allocation failed", nil);
            status = BHASA_GPU_ALLOCATION_ERROR;
            goto cleanup;
        }
        command = [[queue commandBuffer] retain];
        encoder = [[command computeCommandEncoder] retain];
        if (command == nil || encoder == nil) {
            report_error("command buffer or encoder allocation failed", nil);
            status = BHASA_GPU_ALLOCATION_ERROR;
            goto cleanup;
        }
        NSUInteger width = [pipeline threadExecutionWidth];
        NSUInteger maximum = [pipeline maxTotalThreadsPerThreadgroup];
        if (width == 0 || maximum == 0) {
            report_error("device reported an invalid threadgroup size", nil);
            status = BHASA_GPU_LIMIT;
            goto cleanup;
        }
        if (width > maximum) {
            width = maximum;
        }
        if (width > (NSUInteger)data->len) {
            width = (NSUInteger)data->len;
        }
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:gpu_data offset:0 atIndex:0];
        [encoder dispatchThreads:MTLSizeMake((NSUInteger)data->len, 1, 1)
           threadsPerThreadgroup:MTLSizeMake(width, 1, 1)];
        [encoder endEncoding];
        [encoder release];
        encoder = nil;
        [command commit];
        [command waitUntilCompleted];
        if ([command status] != MTLCommandBufferStatusCompleted) {
            report_error("Metal command execution failed", [command error]);
            status = BHASA_GPU_EXECUTION_ERROR;
            goto cleanup;
        }
        /* Failed commands never publish partially written device bytes. */
        memcpy(data->data, [gpu_data contents], (size_t)data->len);

    cleanup:
        /* All submitted commands have completed before their storage is freed. */
        if (encoder != nil) {
            [encoder endEncoding];
        }
        [encoder release];
        [command release];
        [queue release];
        [gpu_data release];
        [pipeline release];
        [function release];
        [library release];
        [device release];
        [entry_text release];
        [source_text release];
        return status;
    }
}
