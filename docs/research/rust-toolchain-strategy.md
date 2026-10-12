# Bhasa and a Rust-based toolchain

Historical exploration. The subsequent implementation decision is a Rust-written Bhasa compiler emitting LLVM IR, then a Bhasa-written compiler. See [the implementation record](../IMPLEMENTATION.md). The alternatives evaluated below are retained as research, not the current build plan.

Research checked on October 11, 2026. This evaluates options for Bhasa, whose first workloads are CLI tools and backend services. The user is comparing an original language with Rust library access against a faster Rust compiler. Architecture remains undecided. No compiler implementation, fork, or measured performance gain is established here.

## Recommendation

Start by testing an agent-oriented toolchain around upstream Rust and Cargo. Preserve Rust source semantics and dependency compatibility while improving diagnostics, project defaults, dependency selection, and feedback time. Maintain a small compiler patch only after profiling identifies a bottleneck that existing configuration and upstream work cannot address.

This recommendation is an engineering judgment. A compiler fork can preserve library compatibility when it preserves Rust behavior, but changing ownership, traits, macros, or public type semantics makes that compatibility a continuing implementation obligation. A simpler syntax front end would still need to translate those semantics and explain errors in the user's source.

An original language that generates Rust can reuse Cargo and compatible Rust source dependencies. It also inherits Rust compilation costs, plus its own translation work. A replacement compiler for Rust aims at build performance directly but must reproduce the language behavior required by those dependencies. These are different projects and should have separate success criteria.

Rust does not remove dependency review. The confirmed rule excludes GC anywhere in the running application, including dependencies. Approve libraries and their transitive native components against that rule. A package being written in Rust, or having a C interface, is insufficient evidence. Keep this policy separate from the unresolved treatment of explicit reference-counted ownership.

## What makes Rust compilation expensive

Rust compilation includes macro expansion, name resolution, type inference and trait solving, borrow checking, intermediate optimizations, code generation, and linking. Its query system already supports incremental reuse. Consequently, a backend change only addresses part of a build, and a wrapper that invokes ordinary Rust compilation does not eliminate compiler work. [Compiler overview](https://rustc-dev-guide.rust-lang.org/overview.html)

Rust specializes generic functions for concrete types through monomorphization. This supports runtime performance but increases compilation work and can increase binary size. Removing specialization across arbitrary crates would be a language and code generation research project, not a configuration switch. [Monomorphization](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html)

Distinguish compiler throughput, time to usable diagnostics, and generated-program performance. Cargo explicitly warns that higher optimization can lengthen builds and does not always produce faster programs. Existing profile controls cover optimization, debug information, incremental compilation, link-time optimization, and code generation units. Evaluate settings before changing compiler implementation. [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)

## Backend choices

The Cranelift Rust backend aims to improve debug compilation time. Its current README documents nightly preview installation and Linux, macOS, and x86-64 Windows availability, with a narrower target matrix than Rust overall. It also lists incomplete `std::arch` support and experimental panic unwinding that is unsupported on macOS and Windows. Abort-on-panic is the default. Those differences matter for tests, cleanup behavior, and libraries that contain panics. Treat it as an opt-in experiment whose compatibility must be established, not a production guarantee. [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift)

Use LLVM as the initial release and WebAssembly baseline. Rust's documented LLVM pipeline emits native objects or Wasm before linking. Do not infer Wasm emission from Cranelift's use in a Wasm runtime. Keep backend selection target-specific. [Compiler code generation](https://rustc-dev-guide.rust-lang.org/overview.html#code-generation)

## Ecosystem compatibility includes build behavior

Rust procedural macros consume and produce Rust syntax during compilation. Existing derive and attribute macros therefore constrain a language front end that promises direct crate compatibility. Their source spans also affect diagnostics. [Procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html)

Cargo build scripts compile and run host programs. They can generate source, locate system libraries, compile bundled C code, and configure linking. Compatibility must include their outputs and host-versus-target distinction. Restricting macros or build scripts can improve control, but it narrows the supported dependency set. [Build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)

For the first experiment, leave dependencies unchanged and apply additional policy to Bhasa-authored application code. Report unsupported dependencies explicitly. Do not advertise universal crate support before testing it.

## First experiment and decision gate

1. Select a representative CLI and HTTP service, plus a generic-heavy library and a macro/build-script-heavy case. Pin source revisions, dependencies, compiler versions, hardware, and target.
2. Measure check-only feedback, clean development builds, no-change builds, a single-function edit, a public-signature edit, tests, and optimized release builds. Record wall time, peak memory, binary size, and application latency or throughput. Separate dependency downloads from compilation.
3. Use `cargo build --timings` to find dependency bottlenecks and build-script costs. Use nightly `-Z self-profile` with the documented profiling tools when compiler phases need investigation. [Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html), [Compiler profiling](https://rustc-dev-guide.rust-lang.org/profiling.html)
4. Compare upstream defaults, measured profile changes, and eligible Cranelift development builds. Keep panic policy and test behavior comparable. Repeat runs and report variation.
5. Measure agent completion rate, repair attempts, token use, and time to a tested change on the same tasks. Shorter syntax is not evidence of better agent outcomes.

Only propose a compiler fork when results name the bottleneck, show a reproducible improvement, preserve required behavior, and justify the maintenance cost. Rust already provides a performance-testing workflow for evaluating compiler changes. No speedup target is an achieved result. [Compiler performance testing](https://rustc-dev-guide.rust-lang.org/tests/perf.html)
