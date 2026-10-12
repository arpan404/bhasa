/* Test double for host protocol/cleanup tests. This is never a GPU backend. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
static int live;
static void *current = (void *)(uintptr_t)123;
int mock_live_count(void) { return live; }
static int fail(const char *stage) { const char *value = getenv("BHASA_MOCK_FAIL"); return value && !strcmp(value, stage); }
static void *allocate(size_t size) { void *value = malloc(size); if (value) ++live; return value; }
static void release(void *value) { if (value) { free(value); --live; } }
int cuInit(unsigned flags) { (void)flags; return 0; }
int cuDeviceGet(int *device, int ordinal) { *device = ordinal; return 0; }
int cuDeviceGetAttribute(int *value, int attribute, int device) {
    (void)device; *value = attribute == 75 ? 8 : attribute == 76 ? 0 : attribute == 5 ? 65535 : 256; return 0;
}
int cuCtxGetCurrent(void **context) { *context = current; return 0; }
int cuCtxSetCurrent(void *context) { current = context; return 0; }
int cuCtxCreate_v2(void **context, unsigned flags, int device) {
    (void)flags; (void)device; *context = allocate(1); current = *context; return !*context;
}
int cuCtxDestroy_v2(void *context) { release(context); current = NULL; return 0; }
int mock_context_restored(void) { return current == (void *)(uintptr_t)123; }
int cuCtxSynchronize(void) { return fail("sync"); }
int cuModuleLoadData(void **module, const void *ptx) {
    (void)ptx; if (fail("pipeline")) return 1; *module = allocate(1); return !*module;
}
int cuModuleUnload(void *module) { release(module); return 0; }
int cuModuleGetFunction(void **function, void *module, const char *name) {
    (void)module; if (strcmp(name, "transform")) return 1; *function = (void *)(uintptr_t)10; return 0;
}
int cuMemAlloc_v2(unsigned long long *pointer, size_t size) {
    if (fail("allocation")) return 1; *pointer = (unsigned long long)(uintptr_t)allocate(size); return !*pointer;
}
int cuMemFree_v2(unsigned long long pointer) { release((void *)(uintptr_t)pointer); return 0; }
int cuMemcpyHtoD_v2(unsigned long long pointer, const void *source, size_t size) {
    memcpy((void *)(uintptr_t)pointer, source, size); return 0;
}
int cuMemcpyDtoH_v2(void *destination, unsigned long long pointer, size_t size) {
    if (fail("readback")) { memset(destination, 0, size); return 1; }
    memcpy(destination, (void *)(uintptr_t)pointer, size); return 0;
}
int cuLaunchKernel(void *function, unsigned gx, unsigned gy, unsigned gz,
    unsigned bx, unsigned by, unsigned bz, unsigned shared, void *stream, void **arguments, void **extra) {
    (void)function; (void)shared; (void)stream; (void)extra;
    unsigned count = *(unsigned *)arguments[1];
    if (fail("launch") || gy != 1 || gz != 1 || by != 1 || bz != 1 || (uint64_t)gx * bx < count) return 1;
    unsigned char *bytes = (void *)(uintptr_t)*(unsigned long long *)arguments[0];
    for (unsigned i = 0; i < count; ++i) bytes[i] = (unsigned char)((bytes[i] * 3u + 7u) & 255u);
    return 0;
}
int nvrtcCreateProgram(void **program, const char *source, const char *name,
    int count, const char *const *headers, const char *const *names) {
    (void)source; (void)name; (void)count; (void)headers; (void)names;
    *program = allocate(1); return !*program;
}
int nvrtcDestroyProgram(void **program) { release(*program); *program = NULL; return 0; }
int nvrtcCompileProgram(void *program, int count, const char *const *options) {
    (void)program; return fail("compile") || count != 2 || strcmp(options[0], "--gpu-architecture=compute_80");
}
int nvrtcGetPTXSize(void *program, size_t *size) { (void)program; *size = 5; return 0; }
int nvrtcGetPTX(void *program, char *text) { (void)program; memcpy(text, "mock", 5); return 0; }
int nvrtcGetProgramLogSize(void *program, size_t *size) { (void)program; *size = 19; return 0; }
int nvrtcGetProgramLog(void *program, char *text) { (void)program; memcpy(text, "mock compile error", 19); return 0; }
