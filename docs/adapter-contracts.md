# Scalar adapter contracts

A local Cargo adapter may provide `bhasa-adapter.json` at its crate root. The
optional `bhasa-scalar-adapter-v1` sidecar records review identity, SHA-256 of the
exact `src/lib.rs` bytes, a `native-no-gc` runtime claim and each exported scalar
function's ownership, error, unwind and thread contracts. If a sidecar exists,
graph-wide preflight validates every adapter across all loaded Bhasa manifests
before the first Clang or Cargo invocation. The standalone Rust adapter builder
also preflights its complete dependency set before creating caches or running Cargo.
The per-adapter validation remains as a recheck. Stale source review,
unknown fields, duplicate symbols or unsupported contracts fail the build.
Existing adapters without a sidecar retain their explicit trusted C ABI behavior.

`ffi_contract::Adapter::parse` validates the versioned schema.
`verify_source(bytes)` binds a review to the source it names.
`c_header()` generates compatible `int64_t`/`void` declarations.
`to_value()` produces deterministic JSON for read-only inspection.
`read_if_present(crate_root)` loads and validates the optional bounded sidecar.
The included rust-package adapter example records its scalar saturation behavior
as infallible, with reentrant calls and an abort-at-boundary unwind contract.

All parameters use `i64-copy`; results use `i64-copy` or `void`. A scalar copy
transfers no allocation, reference, handle or cleanup obligation across the ABI.
This retains the current signed 64-bit `int` ABI. Pointer strings, buffers,
records, resources and callbacks are rejected in v1. Those need checked ownership
and cleanup adapters before they can be supported.

Errors are either `infallible` or `status-i64` with an explicit signed 64-bit
`success` value encoded as a decimal string. Every other value is an
adapter-defined error code. The sidecar does not convert raw integer returns to
Bhasa Result values, infer typed error domains, or enforce handling of integer
statuses. Typed error wrappers remain explicit library code and must preserve the
current Result/IoResult contracts.

Unwind is `abort` or `contained`. Neither permits unwinding through C ABI frames.
Thread metadata is `caller-only` or `reentrant`. These are review claims; they do
not change either frontend's rule forbidding extern calls in task worker graphs.
The metadata does not inspect transitive Cargo dependencies, prove native safety,
or attest that claimed catch/abort behavior is implemented. A dependency review
must still establish absence of GC. Python and TypeScript GC runtimes cannot be
claimed as `native-no-gc` adapters.

The sidecar currently binds only a crate's default `src/lib.rs`. Custom Cargo
library paths and complete reviewed dependency provenance require a later
format. No Cargo flags, output directories, freshness checks or cache eviction
rules change. Generated headers describe the existing ABI and do not introduce a
stable rich library ABI or registry publication command.

Generated-header symbols must start with an ASCII lowercase letter and use only
ASCII letters, digits and underscores. Leading underscores, double underscores,
C/C++ keywords, stdint typedef namespaces, standard header macros and known
bare compiler/platform macro names are rejected. Header generation revalidates
these rules for publicly mutable Adapter values.

Inspect adds an adapters array recording each alias, owning Bhasa manifest,
resolved Cargo manifest, optional metadata path and normalized contract. It
validates source-review hashes without Cargo, cache creation or downloads.
source_review_verified is true only for a present validated sidecar. The separate
no_gc_proven and worker_extern_allowed fields remain false. Missing sidecars
have contract=null. Invalid or stale sidecars produce located inspection errors
and no successful declaration report. Inspect does not validate Rust syntax,
transitive sources, or actual exported ABI signatures.
