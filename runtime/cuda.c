/* Dynamically loaded NVIDIA Driver API + NVRTC. No CUDA SDK at build time. */
#include "cuda.h"
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#define GPU_CALL __stdcall
static void *open_library(const char *name) { return (void *)LoadLibraryA(name); }
static void *find_symbol(void *lib, const char *name) { return (void *)(uintptr_t)GetProcAddress((HMODULE)lib, name); }
static void close_library(void *lib) { if (lib) FreeLibrary((HMODULE)lib); }
#else
#include <dlfcn.h>
#define GPU_CALL
static void *open_library(const char *name) { return dlopen(name, RTLD_NOW | RTLD_LOCAL); }
static void *find_symbol(void *lib, const char *name) { return dlsym(lib, name); }
static void close_library(void *lib) { if (lib) dlclose(lib); }
#endif

typedef unsigned long long DevicePtr;
typedef void *Context;
typedef void *Module;
typedef void *Function;
typedef void *Program;
typedef struct {
    void *driver, *nvrtc;
    int (GPU_CALL *Init)(unsigned);
    int (GPU_CALL *DeviceGet)(int *, int);
    int (GPU_CALL *DeviceGetAttribute)(int *, int, int);
    int (GPU_CALL *CtxCreate)(Context *, unsigned, int);
    int (GPU_CALL *CtxDestroy)(Context);
    int (GPU_CALL *CtxGetCurrent)(Context *);
    int (GPU_CALL *CtxSetCurrent)(Context);
    int (GPU_CALL *CtxSynchronize)(void);
    int (GPU_CALL *ModuleLoadData)(Module *, const void *);
    int (GPU_CALL *ModuleUnload)(Module);
    int (GPU_CALL *ModuleGetFunction)(Function *, Module, const char *);
    int (GPU_CALL *MemAlloc)(DevicePtr *, size_t);
    int (GPU_CALL *MemFree)(DevicePtr);
    int (GPU_CALL *MemcpyHtoD)(DevicePtr, const void *, size_t);
    int (GPU_CALL *MemcpyDtoH)(void *, DevicePtr, size_t);
    int (GPU_CALL *LaunchKernel)(Function, unsigned, unsigned, unsigned,
                               unsigned, unsigned, unsigned, unsigned, void *, void **, void **);
    int (GPU_CALL *CreateProgram)(Program *, const char *, const char *, int, const char *const *, const char *const *);
    int (GPU_CALL *DestroyProgram)(Program *);
    int (GPU_CALL *CompileProgram)(Program, int, const char *const *);
    int (GPU_CALL *GetPTXSize)(Program, size_t *);
    int (GPU_CALL *GetPTX)(Program, char *);
    int (GPU_CALL *GetProgramLogSize)(Program, size_t *);
    int (GPU_CALL *GetProgramLog)(Program, char *);
} Cuda;

static void unload(Cuda *api) {
    close_library(api->nvrtc);
    close_library(api->driver);
    memset(api, 0, sizeof(*api));
}

static void *load_any(const char *override, const char *const *names) {
    const char *path = getenv(override);
    if (path && *path) return open_library(path);
    for (size_t i = 0; names[i]; ++i) {
        void *lib = open_library(names[i]);
        if (lib) return lib;
    }
    return NULL;
}

static int load(Cuda *api) {
    memset(api, 0, sizeof(*api));
    static const char *const driver_names[] = {
#ifdef _WIN32
        "nvcuda.dll",
#elif defined(__APPLE__)
        "libcuda.dylib", "/usr/local/cuda/lib/libcuda.dylib",
#else
        "libcuda.so.1", "libcuda.so",
#endif
        NULL};
    static const char *const compiler_names[] = {
#ifdef _WIN32
        "nvrtc64_130_0.dll", "nvrtc64_120_0.dll", "nvrtc64_112_0.dll",
#elif defined(__APPLE__)
        "libnvrtc.dylib", "/usr/local/cuda/lib/libnvrtc.dylib",
#else
        "libnvrtc.so", "libnvrtc.so.13", "libnvrtc.so.12", "libnvrtc.so.11.2",
#endif
        NULL};
    api->driver = load_any("BHASA_CUDA_DRIVER", driver_names);
    api->nvrtc = load_any("BHASA_NVRTC", compiler_names);
    if (!api->driver || !api->nvrtc) goto failed;
#define LOAD(lib, field, symbol) do { \
    void *address = find_symbol(api->lib, symbol); \
    if (!address || sizeof(api->field) != sizeof(address)) goto failed; \
    memcpy(&api->field, &address, sizeof(address)); \
} while (0)
    LOAD(driver, Init, "cuInit");
    LOAD(driver, DeviceGet, "cuDeviceGet");
    LOAD(driver, DeviceGetAttribute, "cuDeviceGetAttribute");
    LOAD(driver, CtxCreate, "cuCtxCreate_v2");
    LOAD(driver, CtxDestroy, "cuCtxDestroy_v2");
    LOAD(driver, CtxGetCurrent, "cuCtxGetCurrent");
    LOAD(driver, CtxSetCurrent, "cuCtxSetCurrent");
    LOAD(driver, CtxSynchronize, "cuCtxSynchronize");
    LOAD(driver, ModuleLoadData, "cuModuleLoadData");
    LOAD(driver, ModuleUnload, "cuModuleUnload");
    LOAD(driver, ModuleGetFunction, "cuModuleGetFunction");
    LOAD(driver, MemAlloc, "cuMemAlloc_v2");
    LOAD(driver, MemFree, "cuMemFree_v2");
    LOAD(driver, MemcpyHtoD, "cuMemcpyHtoD_v2");
    LOAD(driver, MemcpyDtoH, "cuMemcpyDtoH_v2");
    LOAD(driver, LaunchKernel, "cuLaunchKernel");
    LOAD(nvrtc, CreateProgram, "nvrtcCreateProgram");
    LOAD(nvrtc, DestroyProgram, "nvrtcDestroyProgram");
    LOAD(nvrtc, CompileProgram, "nvrtcCompileProgram");
    LOAD(nvrtc, GetPTXSize, "nvrtcGetPTXSize");
    LOAD(nvrtc, GetPTX, "nvrtcGetPTX");
    LOAD(nvrtc, GetProgramLogSize, "nvrtcGetProgramLogSize");
    LOAD(nvrtc, GetProgramLog, "nvrtcGetProgramLog");
#undef LOAD
    return 1;
failed:
    unload(api);
    return 0;
}

static int valid(const bhasa_buffer *buffer) {
    return buffer && buffer->len >= 0 && buffer->cap >= buffer->len &&
           (buffer->len == 0 || buffer->data) && (uint64_t)buffer->len < SIZE_MAX;
}

static char *copy_text(const bhasa_buffer *buffer) {
    char *result = malloc((size_t)buffer->len + 1);
    if (result) {
        memcpy(result, buffer->data, (size_t)buffer->len);
        result[buffer->len] = 0;
    }
    return result;
}

int64_t bhasa_rt_gpu_available(void) {
    Cuda api;
    if (!load(&api)) return 0;
    int device;
    int available = api.Init(0) == 0 && api.DeviceGet(&device, 0) == 0;
    unload(&api);
    return available;
}

int64_t bhasa_rt_gpu_dispatch(const bhasa_buffer *source, const bhasa_buffer *entry,
                              bhasa_buffer *data) {
    if (!valid(source) || !valid(entry) || !valid(data) || !source->len || !entry->len ||
        memchr(source->data, 0, (size_t)source->len) || memchr(entry->data, 0, (size_t)entry->len))
        return BHASA_GPU_INVALID_ARGUMENT;
    if ((uint64_t)data->len > UINT32_MAX) return BHASA_GPU_LIMIT;
    Cuda api;
    if (!load(&api)) {
        fprintf(stderr, "bhasa GPU: CUDA driver or NVRTC library is unavailable\n");
        return BHASA_GPU_UNAVAILABLE;
    }
    int64_t status = BHASA_GPU_UNAVAILABLE;
    int device = 0, major = 0, minor = 0, maximum_grid = 0, maximum_threads = 0;
    Context previous = NULL, context = NULL;
    Module module = NULL;
    Program program = NULL;
    DevicePtr memory = 0;
    char *source_text = NULL, *entry_text = NULL, *ptx = NULL;
    unsigned char *readback = NULL;
    if (api.Init(0) || api.DeviceGet(&device, 0)) goto cleanup;
    status = BHASA_GPU_ALLOCATION_ERROR;
    source_text = copy_text(source);
    entry_text = copy_text(entry);
    if (!source_text || !entry_text) goto cleanup;
    status = BHASA_GPU_PIPELINE_ERROR;
    if (api.CtxGetCurrent(&previous) || api.CtxCreate(&context, 0, device)) goto cleanup;
    /* Attribute values are fixed by NVIDIA's CUdevice_attribute ABI. */
    if (api.DeviceGetAttribute(&major, 75, device) || api.DeviceGetAttribute(&minor, 76, device) ||
        api.DeviceGetAttribute(&maximum_grid, 5, device) || api.DeviceGetAttribute(&maximum_threads, 1, device)) goto cleanup;
    if (maximum_grid < 1 || maximum_threads < 1 || major < 1 || minor < 0) {
        status = BHASA_GPU_LIMIT;
        goto cleanup;
    }
    char architecture[64];
    snprintf(architecture, sizeof(architecture), "--gpu-architecture=compute_%d%d", major, minor);
    const char *options[] = {architecture, "--std=c++11"};
    status = BHASA_GPU_COMPILE_ERROR;
    if (api.CreateProgram(&program, source_text, "bhasa.cu", 0, NULL, NULL)) goto cleanup;
    if (api.CompileProgram(program, 2, options)) {
        size_t size = 0;
        if (!api.GetProgramLogSize(program, &size) && size && size < SIZE_MAX) {
            char *log = calloc(size + 1, 1);
            if (log) {
                if (!api.GetProgramLog(program, log)) fprintf(stderr, "bhasa GPU: NVRTC: %s\n", log);
                free(log);
            }
        }
        goto cleanup;
    }
    size_t ptx_size = 0;
    if (api.GetPTXSize(program, &ptx_size) || !ptx_size) goto cleanup;
    ptx = malloc(ptx_size);
    if (!ptx) { status = BHASA_GPU_ALLOCATION_ERROR; goto cleanup; }
    if (api.GetPTX(program, ptx)) goto cleanup;
    status = BHASA_GPU_PIPELINE_ERROR;
    if (api.ModuleLoadData(&module, ptx)) goto cleanup;
    Function function = NULL;
    status = BHASA_GPU_ENTRY_NOT_FOUND;
    if (api.ModuleGetFunction(&function, module, entry_text)) goto cleanup;
    status = BHASA_GPU_OK;
    if (data->len == 0) goto cleanup;
    unsigned threads = maximum_threads < 256 ? (unsigned)maximum_threads : 256;
    unsigned count = (unsigned)data->len;
    unsigned blocks = (unsigned)(((uint64_t)count + threads - 1) / threads);
    if (blocks > (unsigned)maximum_grid) { status = BHASA_GPU_LIMIT; goto cleanup; }
    status = BHASA_GPU_ALLOCATION_ERROR;
    readback = malloc((size_t)data->len);
    if (!readback || api.MemAlloc(&memory, (size_t)data->len)) goto cleanup;
    status = BHASA_GPU_EXECUTION_ERROR;
    if (api.MemcpyHtoD(memory, data->data, (size_t)data->len)) goto cleanup;
    void *arguments[] = {&memory, &count};
    if (api.LaunchKernel(function, blocks, 1, 1, threads, 1, 1, 0, NULL, arguments, NULL) ||
        api.CtxSynchronize() || api.MemcpyDtoH(readback, memory, (size_t)data->len)) goto cleanup;
    memcpy(data->data, readback, (size_t)data->len);
    status = BHASA_GPU_OK;
cleanup:
    /* Synchronize before freeing even after a launch/synchronization failure. */
    if (context) {
        (void)api.CtxSynchronize();
        if (memory) (void)api.MemFree(memory);
        if (module) (void)api.ModuleUnload(module);
        (void)api.CtxDestroy(context);
        (void)api.CtxSetCurrent(previous);
    }
    if (program) (void)api.DestroyProgram(&program);
    free(readback);
    free(ptx);
    free(entry_text);
    free(source_text);
    unload(&api);
    if (status) fprintf(stderr, "bhasa GPU: CUDA dispatch failed with status %lld\n", (long long)status);
    return status;
}
