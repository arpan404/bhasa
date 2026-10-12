# GPU host backends

Choose `--gpu metal`, `--gpu cuda` or `--gpu vulkan` with `bhasa build` or
`bhasa run`. The native default is Metal on macOS and Vulkan on Linux. The flag
selects the adapter linked into the executable. It does not translate one
backend's shader language into another.

| Backend | Source passed to `gpu_dispatch` | Kernel data contract | Verification here |
| --- | --- | --- | --- |
| Metal | Metal source text | `device uchar*` at buffer 0; one thread per byte | Actual GPU execution verified. |
| CUDA | CUDA C++ source text compiled by NVRTC | `unsigned char* data, unsigned int count`; check index against count | Host protocol and cleanup verified with dynamic-library test doubles; hardware unavailable. |
| Vulkan | SPIR-V 1.0 binary buffer | `uint[]` at set 0/binding 0; one byte value per uint; uint count push constant | Host protocol and cleanup verified with dynamic-library test doubles; hardware unavailable. |

All three implementations allocate device resources, submit compute work, wait
for completion and copy back results. None falls back to a CPU calculation.
Vulkan excludes devices reported as CPU implementations. The test doubles are
isolated test fixtures, and their results are not GPU execution evidence.

These are initial synchronous native host adapters. Native Bhasa GPU kernel
syntax, automatic kernel translation, WebGPU and browser execution remain
unimplemented. Windows loader branches exist in the C sources but the complete
Windows runtime/toolchain path has not been validated.

## Metal

The optional macOS adapter lets a Bhasa program compile trusted Metal source and
run a synchronous byte-buffer kernel. The adapter is Objective-C with explicit
retain/release and uses Apple's Metal and Foundation frameworks. It adds no Rust
runtime dependency or tracing garbage collector. The compiler's Rust host and
Bhasa frontend work are independent of these native platform adapters.

This is a host API for Metal source, not a Bhasa GPU compiler. It does not provide
portable GPU kernels, rendering, tensors, asynchronous submissions or ownership
checking of arbitrary Metal source.

## Metal interface

```bhasa
var source = buffer()
var status = read_file("examples/gpu/transform.metal", source)
if status == 0 {
    var data = buffer("hello")
    status = gpu_dispatch(source, "transform", data)
}
```

- `gpu_available() -> bool` reports whether Metal provides a default device.
- `gpu_dispatch(source, entry, data) -> int` accepts `str` or `buffer` source and
  entry text, plus a mutable `buffer` containing bytes. It compiles the source,
  selects the named compute function and binds data to buffer index zero.

Each dispatch uses exactly `len(data)` invocations in a one-dimensional grid.
The invocation index is Metal's `uint [[thread_position_in_grid]]`. The adapter
waits until the command completes before copying bytes back to the host buffer.
Failures leave the input buffer unchanged. An empty data buffer still validates
the device, source and pipeline, then succeeds without submitting any work.

The source and entry point must be nonempty UTF-8 without NUL bytes. Data may
contain arbitrary bytes. Dispatch length cannot exceed `UINT32_MAX` or the
device's maximum buffer size. Allocation and pipeline limits are checked.

Each call owns its device, library, pipeline, command queue and buffer references
until completion. Calls share no mutable adapter state. Keeping this API
synchronous prevents Bhasa from changing the host buffer during an outstanding
dispatch. Passing the same buffer concurrently through foreign native code still
requires that caller to synchronize access.

## Status codes

| Status | Meaning |
| --- | --- |
| 0 | Completed successfully; host data contains the result. |
| 1 | The selected GPU device, driver or required runtime compiler is unavailable. |
| 2 | Invalid input buffers or backend source/entry encoding. |
| 3 | Kernel compilation or SPIR-V envelope validation failed. |
| 4 | The requested entry point was not found. |
| 5 | Compute pipeline creation failed. |
| 6 | Buffer size, dispatch size or device limits were exceeded. |
| 7 | Buffer, queue, command or encoder allocation failed. |
| 8 | GPU execution/readback failed, or Vulkan output exceeded the byte range. |

Dispatch failures also print the operation and available Metal diagnostics to
stderr. There is no CPU fallback. `gpu_available()` does not guarantee that a
particular kernel compiles or runs.

## Trusted source boundary

Metal source is foreign code. The Bhasa checker does not establish its memory
safety, race freedom, termination or correct argument layout. Kernels must accept
one byte buffer at index zero and respect its length. The example assigns each
invocation its own byte. A kernel that requires more buffers, races on shared
memory or accesses bytes out of bounds violates this interface.

Do not feed untrusted source into this API. A future Bhasa kernel subset needs
typed device buffers, restricted calls, checked indexing and a defined device
memory model before it can promise the language's safety properties.

## Toolchain and verification

The adapter is compiled with Clang as Objective-C using `-fno-objc-arc` and links
Foundation, Metal and CoreGraphics. It invokes Metal's runtime library compiler;
it does not call the offline `metal` or `metallib` commands. Apple documents
runtime source compilation in its [Metal library reference](https://developer.apple.com/documentation/metal/mtllibrary?language=objc)
and the command sequence in its [compute example](https://developer.apple.com/documentation/metal/performing-calculations-on-a-gpu).

Run `cargo test --test gpu -- --nocapture` for adapter tests and
`cargo run -- run examples/gpu/main.bh` for the Bhasa example. Tests print an
explicit skip when a device is unavailable. Only the completed output comparison
establishes GPU execution. An SDK or successful host compilation alone does not.
Set `BHASA_REQUIRE_GPU=1` to make unavailable hardware fail the tests instead of
skipping execution.

On October 11, 2026, the native adapter passed argument validation in the restricted
environment and reported no accessible device. An approved run outside that
environment then compiled and executed the Metal kernel and verified every byte
for lengths 0, 1, 33 and 513. Invalid source and missing entry points returned their
distinct status codes and preserved the host data. The offline Metal Toolchain
component was absent; this test used Metal's runtime source compiler.
The Bhasa builtin test also executed on the GPU in development and release modes,
passing source and entry points as both static strings and owned buffers. Both
calls completed and their combined byte transformation matched the expected
result. `BHASA_REQUIRE_GPU=1` was set for this verification.

## CUDA

The CUDA adapter loads NVIDIA's driver and NVRTC at runtime, compiles CUDA C++
source to PTX, loads the module, allocates a device buffer and launches the named
function. This follows the [NVRTC compilation interface](https://docs.nvidia.com/cuda/nvrtc/index.html)
and the [CUDA Driver API](https://docs.nvidia.com/cuda/cuda-programming-guide/03-advanced/driver-api.html).
No CUDA headers or libraries are required when compiling the host adapter.
A working NVIDIA driver, matching NVRTC library and supported NVIDIA GPU are
required when executing it.

The kernel signature must be:

```cuda
extern "C" __global__ void transform(unsigned char *data, unsigned int count) {
    unsigned int index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index < count) data[index] = (unsigned char)((data[index] * 3u + 7u) & 255u);
}
```

The adapter launches up to 256 threads per group, constrained by the device.
The final group can include indices past `count`, so the bounds check is part of
the kernel contract. Length must fit `uint32` and the device's grid limit.
Each call uses its own CUDA context and restores the calling thread's prior
context. Failed readback does not publish partial host results.

Normal loader library names are tried first unless overridden. Set
`BHASA_CUDA_DRIVER` to a driver library path or `BHASA_NVRTC` to an NVRTC library
path when they are outside the platform loader search path. These paths load
trusted native libraries and must not come from untrusted input. A version
mismatch, such as an NVRTC compiler too old for the GPU's architecture, returns
an explicit error rather than selecting a different backend.

## Vulkan

The Vulkan adapter loads a native Vulkan loader or MoltenVK. Official Khronos
headers are bundled with [license and provenance](../runtime/vendor/README.md),
so host compilation needs no separately installed Vulkan SDK. Execution needs a
hardware Vulkan compute device. `BHASA_VULKAN_LIBRARY` may select a trusted
loader library outside the normal search path.

Pass a binary `buffer` containing SPIR-V 1.0 and the exported compute entry name.
The initial supported shader envelope permits the `Shader` capability and a
literal local workgroup size of `1, 1, 1`. Optional shader capabilities and
specialized workgroup sizes are rejected. This validates the interface envelope;
it is not a full SPIR-V validator.

The byte buffer expands to a `uint32` storage array with one byte value in each
element. This avoids depending on optional 8-bit storage support or creating
races between byte writes packed into one word. The descriptor is set 0, binding
0, and a four-byte push constant contains the element count. Every returned
element must be in `0..255`. Out-of-range output fails without changing the Bhasa
buffer. The checked-in GLSL example describes this layout:

```glsl
layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;
layout(set = 0, binding = 0, std430) buffer Data { uint values[]; } data;
layout(push_constant) uniform Parameters { uint count; } parameters;
```

Dispatch length is limited by the device's maximum X workgroup count and storage
buffer range. The initial implementation uses host-visible coherent device
memory, an explicit compute-to-host memory barrier and queue completion before
readback. These follow Vulkan's [memory properties](https://docs.vulkan.org/refpages/latest/refpages/source/VkMemoryPropertyFlagBits.html)
and [memory-domain synchronization rules](https://docs.vulkan.org/spec/latest/appendices/memorymodel.html).
Each call owns its device and queue. Persistent pipelines, command reuse and
large-grid tiling are future performance work.

Compile the example shader with an installed GLSL compiler:

```sh
glslangValidator -V --target-env vulkan1.0 examples/gpu/transform.comp -o build/transform.spv
```

Then follow the commands in [the GPU examples](../examples/gpu/README.md).

## CUDA and Vulkan verification

`cargo test --test gpu_backends -- --nocapture` builds both adapters with strict
C warnings, loads test driver libraries, and exercises dispatch setup, transfers,
resource destruction and host-buffer preservation on injected failures. CUDA
tests verify restoration of an existing caller context. Vulkan tests verify CPU
device rejection, portability-subset instance dependencies, and failure when a
shader returns a value outside the byte range. The portability regression failed
before the dependency fix and passed afterward. Bhasa CLI tests build and run each backend with deliberately missing
driver paths and verify an unavailable result.

The same test target probes the configured native drivers. When available, it
builds `dispatch.bh` and checks 513 transformed bytes in development and release
modes. Vulkan also needs `glslangValidator` to build its test shader. Missing
hardware or that test tool prints an explicit skip. `BHASA_REQUIRE_CUDA=1` or
`BHASA_REQUIRE_VULKAN=1` makes a missing prerequisite fail its backend test;
`BHASA_REQUIRE_GPU=1` requires both.

These tests passed on October 11, 2026. CUDA and Vulkan hardware execution was
not verified on this Mac because their runtime libraries and devices were not
available. Metal's hardware verification above does not establish CUDA or Vulkan
correctness on other devices.
