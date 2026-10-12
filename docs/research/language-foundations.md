# Bhasa language foundations

Implementation update: the selected bootstrap is written in Rust and emits LLVM IR directly, with installed Clang handling machine-code generation and linking. It does not emit Rust source. See [the implementation record](../IMPLEMENTATION.md) and [the milestone contract](../BOOTSTRAP.md) for current scope. The recommendations below include future work.

Research checked on October 11, 2026. This note separates established precedents from proposed choices for Bhasa. It is an initial design reference, not a completed language specification or evidence of Bhasa's performance.

## Confirmed product direction

The first usable version targets CLI tools and backend services. Embedded systems and WebAssembly remain architectural requirements, with platform support introduced incrementally. Library interoperability is required where it preserves the user's confirmed constraint: no garbage collection anywhere in the running application, including dependencies and supporting runtime processes. This excludes ordinary Python/Node runtime bridges, including separate workers. Native C and Rust integration is the initial priority; neither a language name nor a C ABI establishes that a dependency is GC-free.

## Memory safety without garbage collection

Rust demonstrates compile-time ownership checking without a tracing garbage collector. Values have owners, moving a value invalidates the old binding, and leaving scope drops owned resources. Borrowing permits access without transferring ownership. Rust permits either one mutable reference or multiple immutable references, and references must remain valid. These are semantic restrictions, regardless of how briefly a language spells them. [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html), [Rust borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html).

Recommendation: use owned values, moves, deterministic destruction, and checked borrowing. Infer routine local lifetimes, but keep ownership and borrowing visible in public interfaces. Begin with conservative rules for returned references and prohibit self-referential owned structures until there is a sound design for them. Arenas need checked lifetimes or handles; freeing a whole arena must not leave usable references behind.

Treat "no garbage collection at all" strictly in the initial design. Do not introduce hidden reference counting as an ergonomic shortcut. Specify allocation, destruction order, allocator failure, and cancellation cleanup. Easier syntax cannot eliminate these questions. Graphs and shared mutable structures are useful early tests of whether the ownership model is practical.

## Concurrency requires a narrower promise

Safe Rust prevents data races. Its documentation explicitly permits deadlocks and general race conditions, including incorrect synchronization that produces a logically wrong result without violating memory safety. Ownership plus thread-transfer and thread-sharing rules support the data-race guarantee. [Rustonomicon on races](https://doc.rust-lang.org/nomicon/races.html).

Recommendation: promise data-race freedom in safe code, then define separate policies for task lifetimes, cancellation, resource limits, and deadlocks. Start with scoped tasks and ownership transfer through bounded channels. State whether a parent waits for every child and what happens when one fails. Default backpressure and structured cleanup should be testable language or library behavior. A compiler cannot generally certify arbitrary enterprise business logic just because its memory accesses are safe.

A scheduler does not itself require a tracing collector. However, task state still needs ownership, allocation, and destruction rules. "No GC" should not accidentally become an unsupported promise of zero runtime code.

## Fast builds start with the language

Go's design account identifies dependency handling as a compilation concern. Compiling an importing package reads dependency information from compiled package data instead of repeatedly expanding transitive source headers. The language also rejects unused imports. Go's specification prohibits a package from importing itself directly or indirectly. These are concrete precedents for predictable dependency analysis, not proof that another language will compile at Go's speed. [Go's design account](https://go.dev/talks/2012/splash.article), [Go import specification](https://go.dev/ref/spec#Import_declarations).

Recommendation: use explicit, acyclic package imports, cached interface summaries, and separate checking from machine-code generation. Require declared types at module boundaries and keep inference local. Limit compile-time evaluation, macro expansion, specialization, and overload search until their build costs are measured. Generic code generation needs an explicit policy because broad specialization can multiply work.

Measure clean builds, cached builds, single-function edits, dependency-interface changes, peak compiler memory, and linking separately. Fix the hardware, dependency set, optimization level, and compiler versions. A fast parser or backend alone is not a fast toolchain.

## Backend choice is a target decision

Cranelift accepts an intermediate representation and generates native machine code. Its stated priorities include compiler speed and a smaller optimization scope than LLVM. Its currently listed architectures are x86-64, AArch64, s390x, and RISC-V 64. Its role in Wasmtime is compiling Wasm into machine code. That does not make it a backend that emits Wasm binaries. [Cranelift project](https://cranelift.dev/).

LLVM's WebAssembly toolchain produces Wasm object files, and `wasm-ld` links them. The Bytecode Alliance also provides `wasm-encoder` for directly generating binary modules. [LLVM WebAssembly linker](https://lld.llvm.org/WebAssembly.html), [wasm-encoder](https://github.com/bytecodealliance/wasm-tools/blob/main/crates/wasm-encoder/README.md).

Recommendation: keep a typed, ownership-checked intermediate representation independent of the backend. Evaluate LLVM for the first native and Wasm implementation; evaluate Cranelift for faster development builds once the frontend is stable. A Cranelift-first compiler needs a separate Wasm emission path. Supporting multiple backends adds maintenance and semantic-consistency tests, so do not implement both merely to advertise them. Check the chosen embedded processor before committing to a backend.

## Wasm portability has boundaries

WebAssembly isolates modules from the host, but bounds checks apply to linear memory as a region. An unsafe access inside that region can still corrupt a neighboring object. Wasm sandboxing therefore does not replace source-language ownership and object bounds checks. The security documentation also recognizes race conditions and side channels. [WebAssembly security model](https://webassembly.org/docs/security/).

WASI standardizes host interfaces, and its capability model gives programs only the authority granted by their host. Current WASI documentation describes multiple releases, including async interfaces in 0.3. Runtime and embedding support still matter. [WASI introduction](https://wasi.dev/), [WASI security](https://wasi.dev/security).

Recommendation: define WASI and freestanding target profiles with declared imports, threading models, memory limits, and supported Wasm features. File systems, network operations, UI access, and threads need host-specific implementations. Prefer explicit capabilities for these operations, while recognizing that native capability APIs require enforcement around unsafe code and foreign libraries. A successful Wasm compilation is only one part of a usable browser or server application. Under the confirmed no-GC requirement, Wasm output must not require GC-managed guest objects, and the chosen execution host and its dependencies also need review. Producing a GC-free module does not establish that an entire browser or host deployment is GC-free. Browser deployment therefore cannot currently be included in the compliant target commitment.

## Embedded support needs a small core

Rust's embedded model distinguishes hosted environments from bare metal. Its `no_std` approach provides a core library without ordinary operating-system integration; dynamic allocation requires an allocator and an appropriate allocation library. This is a useful precedent for supporting firmware without forcing server assumptions onto it. [Embedded Rust's core-library model](https://docs.rust-embedded.org/book/intro/no-std.html).

Recommendation: separate core operations, allocation-dependent collections, and hosted services. Core code should not require a heap, thread scheduler, file system, or network stack. Choose one actual board for initial embedded validation. Define interrupt access, atomic availability, stack use, startup, and panic handling for that board. "Embedded support" should eventually mean tested target configurations, not only accepting a target flag.

## Agent tooling needs measurable outcomes

Rust already emits structured JSON diagnostics with diagnostic codes, source spans, suggestions, and suggestion-applicability information. This supplies an existing model for tools that consume compiler output without scraping prose. [rustc JSON diagnostics](https://doc.rust-lang.org/rustc/json.html).

Recommendation: ship versioned diagnostic schemas, a canonical formatter, package-interface queries, ownership explanations, and refactoring commands alongside the compiler. Make patches carry the source revision they were computed against. Report the exact compile and test evidence for a proposed fix; a syntactically applicable edit is not evidence of behavioral correctness.

Measure agent task success, regression rate, repair attempts, total input/output tokens, and time to a validated change. Shorter keywords alone are an untested optimization. Prefer predictable semantics and compact compiler-generated context summaries, then use controlled tasks to determine whether they reduce errors and token use.

## GPU support

GPU programming is a requested capability. Proposed architecture: provide optional host-side GPU libraries and a restricted Bhasa kernel subset. Calling an existing GPU library through a Rust or C adapter is distinct from compiling Bhasa code into GPU instructions. The bootstrap host compiler does not automatically supply a device-code compiler.

Native wgpu is a candidate portable host layer. Its documentation lists Metal, Vulkan, and DirectX 12 backends and WGSL input translated for the selected backend. This is a technical candidate, not a verified no-GC dependency configuration. Review its actual dependencies and resource-management mechanisms before selecting it. Browser WebGPU remains outside the currently compliant deployment commitment because a GC-free module does not establish a GC-free browser host. [wgpu backends and shader inputs](https://wgpu.rs/doc/wgpu/documentation/features/index.html).

Start custom kernel compilation with a small numeric subset targeting WGSL and one native backend. Initially exclude dynamic allocation, arbitrary recursion, operating-system calls, and dynamic interface dispatch. A kernel may call only other device-compatible functions; ordinary Rust package compatibility does not make a Rust library callable from a GPU kernel. Validate numeric types, buffer layouts, device features, memory limits, and workgroup sizes. Unsupported capabilities should produce a clear error or use an explicitly requested CPU implementation. Vendor-specific execution and optimized native math libraries can follow through separate adapters.

Start the safe kernel API with element-wise operations that assign each invocation a distinct output element. General shared-memory kernels need a specified memory model, atomic operations, and uniform barriers. WGSL explicitly assigns shader authors responsibility for avoiding conflicting shared accesses and meeting uniformity requirements; compilation alone does not establish general data-race freedom. [WGSL specification](https://www.w3.org/TR/WGSL/).

Keep transfers and synchronization visible. GPU allocations need owners, and a submission must keep referenced allocations alive until completion, even if the caller cancels its wait. Releasing a handle must not free memory still used by a device. A synchronous convenience operation may wait before returning; an asynchronous operation needs a completion object with explicit lifetime and failure rules. NVIDIA documents that kernel launches may return before execution completes or even begins. [CUDA asynchronous execution](https://docs.nvidia.com/cuda/cuda-programming-guide/02-basics/asynchronous-execution.html).

Proposed sequence: qualify a native GPU adapter; execute an existing kernel with explicit upload, completion, and readback; then implement a Bhasa element-wise kernel. Validate results against a CPU reference and separately measure compilation, transfers, execution, and synchronization. Advanced kernels, rendering, tensor operations, and vendor-specific acceleration follow measured workloads. CLI/backend foundations remain the first language milestone.

## Library interoperability

Recommendation: make the platform C ABI the initial native interoperability contract, supporting both library imports and Bhasa exports. Define calling conventions and data layouts per target; a C ABI is not one binary format shared by all operating systems and processors. Rust documents the C ABI as the default ABI used by the dominant C compiler for a target. [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html#abi).

Generate raw declarations from supported C headers, then build safe Bhasa wrappers from explicit resource contracts. Specify ownership, buffer lengths, nullability, matching deallocators, callback lifetimes, thread restrictions, blocking behavior, and error conversion. Plain C signatures do not establish those contracts. Unknown contracts must remain unsafe or unsupported rather than acquire guessed safety annotations. Foreign allocations must be released by their documented allocator or destruction function. In-process foreign code remains part of the trusted implementation; wrappers cannot contain arbitrary memory corruption inside it. Rust's FFI guidance illustrates these resource and callback obligations. [Rust FFI guide](https://doc.rust-lang.org/nomicon/ffi.html).

Rust libraries should use an adapter crate exposing selected operations through C-compatible functions and types or opaque handles. Compile the adapter as a system static or dynamic library. Rust's native ABI has no stability guarantees; `staticlib` and `cdylib` are intended for integration with other languages. Do not pass ordinary Rust strings, vectors, trait objects, or futures as though their layout were a stable C contract. Generic APIs need concrete exported operations, and async libraries need a specified executor and completion/cancellation protocol. [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html#abi), [Rust library outputs](https://doc.rust-lang.org/reference/linkage.html).

For the initial native bridge, prohibit unwinding across the boundary. Convert ordinary failures into explicit status/results. Rust adapters may contain unwinding panics, but cannot recover from aborting panics; this must remain visible in the adapter contract. [Rust FFI unwinding](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding).

Calling existing Python libraries requires a compatible Python runtime and dependencies. CPython can be embedded through its C API, but it uses reference counting plus cyclic garbage collection. Embedding it would introduce managed memory into the Bhasa process. [Embedding Python](https://docs.python.org/3/extending/embedding.html), [Python garbage collection](https://docs.python.org/3/library/gc.html).

TypeScript normally emits JavaScript and erases type annotations. Reusing a TypeScript or JavaScript package therefore requires its expected runtime and host APIs. Node-API provides a stable C interface for native addons loaded into Node; it is useful for exporting Bhasa functions to Node, but does not turn arbitrary npm packages into standalone native libraries. Node's JavaScript values participate in garbage collection. [TypeScript compilation](https://www.typescriptlang.org/docs/handbook/2/basic-types.html#erased-types), [Node-API](https://nodejs.org/api/n-api.html).

Confirmed policy: exclude Python and Node embedding, worker processes, and deployment through their extension/addon hosts. Moving a collector to another process does not meet the user's requirement. Python/TypeScript package compatibility is limited to reusable native components that can operate independently of those runtimes, or separately implemented ports. Neither bindings nor ahead-of-time packaging generally remove a foreign library's runtime semantics.

Proposed dependency policy: record exact artifacts, transitive dependencies, target configurations, build features, and memory-management requirements. Reject known GC-dependent packages and treat unreviewed foreign binaries as ineligible for a compliant build. Manifest declarations and scans assist review but do not prove that arbitrary native code contains no collector. Qualification requires evidence about the actual implementation and selected build, including runtime-loaded plugins. C, C++, or Rust code can qualify through a native adapter only when its full runtime dependency chain meets the policy. The existing proposal against hidden reference counting in Bhasa remains; explicit reference-counted library types are a separate semantic decision and are not classified solely by their implementation language.

Proposed delivery order: C imports and exports; selected Rust adapters; then additional qualified native libraries and C++ wrappers as needed. Track native artifacts, target triples, headers, adapter versions, and runtime dependencies in package metadata. WebAssembly builds require separately compatible dependency builds or host interfaces; native libraries cannot simply be linked into a Wasm artifact. First acceptance cases should cover a qualified C library, a qualified Rust adapter, ownership and callback misuse rejection, foreign errors, an exported Bhasa function called from C, and rejection of known GC-dependent or unreviewed runtime dependencies.
