# GPU compute examples

From the repository root:

```sh
cargo run -- run examples/gpu/main.bh
```

The program reads `transform.metal`, compiles it through Metal, and transforms
513 bytes on a GPU. It checks every byte against the expected result and prints
`Metal GPU verified 513 bytes` only after a completed, correct dispatch.

macOS and an accessible Metal device are required. A missing device prints a
`SKIP` message and exits with status 77. Compilation or execution failure exits
with status 1. There is no CPU fallback.

This calls a Metal kernel through Bhasa's host API. It does not compile Bhasa
functions into GPU kernels. Read the [GPU contract](../../docs/gpu.md) before
supplying custom Metal source.

`dispatch.bh` is a common host program that accepts a kernel file and entry name.
Build it for one backend, then pass the corresponding source format:

```sh
cargo run -- build examples/gpu/dispatch.bh --gpu metal -o build/gpu-metal
./build/gpu-metal examples/gpu/transform.metal transform

cargo run -- build examples/gpu/dispatch.bh --gpu cuda -o build/gpu-cuda
./build/gpu-cuda examples/gpu/transform.cu transform

glslangValidator -V --target-env vulkan1.0 examples/gpu/transform.comp -o build/transform.spv
cargo run -- build examples/gpu/dispatch.bh --gpu vulkan -o build/gpu-vulkan
./build/gpu-vulkan build/transform.spv main
```

CUDA needs an NVIDIA GPU, driver and NVRTC library at execution time. Vulkan
needs a native hardware Vulkan implementation and the GLSL-to-SPIR-V compilation
step shown above. The Vulkan shader uses one uint32 per Bhasa byte; see the
backend contract before writing a shader. The host program verifies every output
byte and returns 77 when the selected backend is unavailable.

Metal execution was verified locally. CUDA and Vulkan adapters have protocol,
cleanup and failure tests using clearly labeled fake libraries. Their hardware
execution is still unverified on this machine.
