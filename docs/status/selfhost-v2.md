# Bhasa frontend methods and networking

## Current milestone

Complete and frozen. Methods and `tcp_listen_on` are implemented and this lane's
focused verification passed. The earlier
record/network/boundary milestone is recorded separately below. This lane owns
emitter.bh, driver.bh, check.sh, tests/selfhost_methods.rs and this status file.

The public `bh_compile` ABI and zero-width file-boundary token 314 are unchanged.
Methods have a signature registry keyed by nominal record ID and method name.
The inferred `self` parameter borrows mutable storage and cannot be rebound.
Methods use ordinary return checking and ownership acquisition. Fields and
methods cannot collide, while methods may reuse global and builtin names.
Emitted `bhasa.method.<record-id>.<name>` symbols cannot collide with source
identifiers or runtime/extern symbols.

The emitter evaluates the receiver once, then explicit arguments from left to
right. It includes the receiver in overlap checks and frees an owned temporary
receiver after the call has acquired any owned return. Worker safety follows
resolved calls through globals and methods after all bodies are emitted,
including forward and recursive paths. Direct method spawn remains unsupported.
The Rust records handoff in status/records.md matches these rules.

Both `tcp_listen_on` overloads return an owned listener and emit the contracted
str/buffer ABI symbols with core/net runtime markers. Runtime address validation
belongs to the network lane; the frontend checks argument types and ownership.
The standalone seed version is 0.5, while the host frontend ABI remains v2.

Current verification on macOS arm64:

- Stage 0 checked and built the complete frontend. A basic native fixture
  passed mutation, temporary receivers, borrowed record returns and methods
  with builtin names.
- The method acceptance/alias/worker-safety matrix and token-314 method boundary
  test passed. Native method ownership passed Rust and Bhasa at O0/O2 with
  `sanitize_address` on every generated LLVM definition, ASan/UBSan C runtimes
  and zero tracked live allocations.
- The final audit found a depth mismatch in the schema prepass, which counted
  outer record and function braces against the body depth limit. Both
  prepasses now exclude those wrappers. Empty bodies permit 128 nested blocks,
  while combined body and expression depth remains limited to 128.
- After that fix, `sh bootstrap/compiler/check.sh` passed in
  `build/selfhost/run.hz7K3Jwj`. A built B, B built C, their complete compiler LLVM
  matched, and both ran the existing examples plus nested methods, temporary
  record returns and both explicit-bind error-handle forms.
- Both listen-on tests passed after root registered the Rust builtin. Native
  programs ran both frontends at O0/O2 over IPv4 and IPv6 loopback on ephemeral
  ports. IPv6 was available and did not skip. Instrumented LLVM, ASan/UBSan and
  allocation tracking passed for method-returned listener/socket handles,
  temporary receivers, retained clones, binary exchanges and invalid inputs.

All five test functions in `tests/selfhost_methods.rs` have passing focused
runs, with `CARGO_BUILD_JOBS=2` and `--test-threads=1`. The final expanded method
matrix passed in 2.74 seconds, covering 128 nested empty blocks versus 129,
combined expression depth and 127 chained method calls versus 128. Near-limit
Rust references run through explicit `--frontend rust` CLI subprocesses because
the default Rust test-thread stack overflowed at a supported language boundary.
The native receiver-order fixture produces `RAB` and `112`, proving one receiver
evaluation followed by arguments in source order. Method names including
`spawn`, `join`, `gpu_dispatch` and `bhasa_rt_foo` resolve independently of globals.

The integration Clippy finding in the new test is fixed with `as_chunks::<5>()`
and explicit length/remainder assertions. `rustfmt --check` passes. Root will
rerun integration lint and owns the rebuilt frontend archive and combined suite.
No broad suite was run in this lane. Current compiler sources exactly match the
passed A/B/C checkpoint. No source or test edits remain planned.

Frozen SHA-256 values:

```text
40a51c63b9c683e61b1b06358b35d2ee8530c3243aa38eb07fe0b8c21a195b3e  bootstrap/compiler/emitter.bh
107a057af2bb006910a2509048bbb3ac88a39dbd60d801240feb64bec75f0621  bootstrap/compiler/driver.bh
7a156bad0b2245a0fe0bc559fe5967bb7fd6642b485e2455564476214061e079  bootstrap/compiler/check.sh
5215353eb1e7003bf3ead1647529a17b990327c272378991a9771307c487a5ac  tests/selfhost_methods.rs
```

## Completed record/network/boundary baseline

### Implementation

The `bh_compile(buffer, ints, ints, buffer) -> buffer` ABI is unchanged.
`emitter.bh` uses existing stage-0 builtins only. It supports both TCP handle
types, all networking builtins, status-use checks, clone/free and core/net LLVM
markers. Records support forward named schemas, constructors, typed parameters
and returns, nested records, buffer/ints/TCP fields and member reads/writes.
Parameters borrow mutable storage and cannot be rebound.

Each constructor field is acquired before evaluating later arguments. Field
replacement acquires the RHS and then resolves its destination, so calls that
replace an enclosing record cannot leave stale pointers. Owned fields read from
temporaries are cloned before their temporary owner is freed. Generated record
helpers deep-copy owned fields, free them in reverse order, check allocation
failure and emit LLVM types before functions. Only used helpers are generated.

Final borrowed arguments reject equal/ancestor/descendant overlap when a record
is involved. Borrows used only during later-argument evaluation reject a call
that could replace a strict ancestor of an earlier captured handle. This matches
the latest Rust rules: `consume(b, len(b.text))`, constructor copies such as
`consume(b, Wrap(b))`, and mutation of a borrowed record's own descendants are
allowed alongside that record; captured descendant handles remain protected.

Schemas use iterative validation with a 128-level limit. Expression tree depth
is bounded independently of parser recursion, including field and flat binary
chains. Parenthesized field-assignment targets are rejected like the Rust parser.
No raw pointers or garbage collector are introduced into Bhasa source.

### Diagnostics

Existing BH200–BH210 meanings stay intact. BH204 also covers flat expression
trees beyond the Rust frontend's 128-level bound. New codes are BH211 unknown
record type, BH212 invalid/duplicate record schema or name, BH213 recursive or
over-depth record layout, and BH214 overlapping argument borrows.

### Verification

Passed on this host:

- Stage-0 checked and built the complete Bhasa frontend repeatedly.
- Updated `bootstrap/compiler/check.sh` built A, B and C, compared LLVM, ran
  existing examples, and exercised extern/tasks/GPU-unavailable handling plus
  nested record copies and TCP error-handle copies.
- The existing three selfhost_emitter tests passed. After adding tree-depth
  bounds, its focused diagnostic test passed again with the long-chain
  expectation updated to BH204, matching Rust.
- All five selfhost_parity tests passed, including actual TCP loopback and
  copied handle use under ASan/UBSan at O0/O2 for both frontends.
- Record schema/malformed/depth tests passed. The latest focused acceptance
  test passed after the Rust borrowing refinement, including about 30 added
  safe-read, constructor and nested-mutation cases and exact BH210–BH214 checks.
- Native record ownership passed for Rust and Bhasa at O0/O2 with allocation
  tracking, including TCP fields, temporary extraction, self-assignment, early
  returns, loops, constructor ordering and an RHS that replaces its target's
  parent. B and C emitted identical compiler and record LLVM.
- The final focused record ownership rerun passed with `sanitize_address` on
  every generated LLVM definition, including helpers and B/C compiler functions.
  Both Rust/Bhasa native programs passed O0/O2, all tracked allocations were
  freed, stderr stayed empty and B/C LLVM matched. C runtime builds also enable
  ASan/UBSan. The focused run took 5.08 seconds.

Sandbox socket creation returns EPERM; native TCP checks need host socket
permissions. The test passed when run outside the sandbox. Focused reruns use
CARGO_BUILD_JOBS=2 and --test-threads=1. Root owns the final combined suite.
Installed Clang lacks a wasm32 backend; allocator i32 IR checks passed in audit,
but no wasm32 native object verification is claimed.

Changed files are emitter.bh, driver.bh, check.sh, tests/selfhost_emitter.rs,
tests/selfhost_parity.rs, new tests/selfhost_records.rs and this status file.
No root-owned Rust modules or other lane files were edited. Module loading stays
in the host. docs/self-hosting.md still needs its old feature claims updated by
the owning lane.

Root-requested portability fixes: selfhost_emitter now defines
_DARWIN_C_SOURCE before its first system include on Apple; selfhost_records
already did so. Direct core-runtime test links and the A/B/C script now pass
-pthread. The selfhost_emitter focused sanitizer rerun passed in 34.06
seconds after those fixes. Shell syntax and rustfmt checks pass. No source or
test edits remain; this lane is complete and frozen for integration.

File-boundary follow-up changed only emitter.bh, tests/selfhost_emitter.rs and
this status file. Focused regression command passed in 1.04 seconds:
`CARGO_BUILD_JOBS=2 cargo test --offline --test selfhost_emitter host_file_boundaries_separate_complete_declarations_without_joining_syntax -- --test-threads=1`.
The marker is host-only; the scanner does not emit it for ordinary source.
No boundary-aware behavior is needed from the plain compile_with_bhasa path.
The existing bounded-diagnostics test also passed in 1.12 seconds after the
extern prepass change, and rustfmt validation passed. No broad suite was run.
