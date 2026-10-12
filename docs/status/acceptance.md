# Independent acceptance lane

Methods milestone complete and frozen for integration. All seven groups pass
with the Rust frontend and the current v2 hybrid archive. No concrete compiler
defect or frontend acceptance mismatch was found. No tests remain running.

This lane owns `tests/frontend_differential.rs`,
`tests/method_acceptance/`, `tools/verify-compiler.sh`, and this file. The
completed baseline below is superseded by `docs/status/integration.md` for its
final combined verification.

## Methods milestone contract and status

Independent method tests live in `tests/method_acceptance/mod.rs`, included
by the existing differential target so its child-process deadline and compiler
API paths remain shared. Focused filter: `method_acceptance`. Direct sources
use `compile_to_llvm` and `compile_with_bhasa`; imported cases reload separate
package trees and use `compile_files_with_bhasa`. No new compiler API is needed.

Tests follow `docs/status/next-milestone.md`: inferred first `self`, typed
remaining parameters, mutable borrowed receivers, ordinary return rules, no
method/field name overlap, and unsupported direct method spawning. Coverage
includes reused method names, imported type-ID remapping, temporary
receivers and returned borrows, captured receivers invalidated by later
arguments, recursive method-to-extern worker graphs, and malformed methods.
Native method fixtures use generated LLVM `sanitize_address` attributes
and count both generated record allocations and core container allocations.
Explicit-bind cases test static host/port typing only; network execution
and IPv4/IPv6 hardware/permission skips belong to the network lane.

The focused Rust frontend run passed all seven groups on October 11:

```text
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test frontend_differential method_acceptance -- --nocapture --test-threads=1
7 passed; 0 failed; 9 filtered out; finished in 3.88s
```

Both native fixtures passed at O0/O2 with ASan/UBSan, zero tracked live
allocations, exact stdout, and empty stderr. They exercise independent owned
returns, nested/temporary receivers, receiver mutation during argument
evaluation, detached returned receivers surviving original-parent mutation,
RHS parent replacement, early returns, loops and method/global symbol
separation. A final native output trace independently checks that the receiver
is evaluated exactly once before two explicit arguments in left-to-right
order; its focused rerun passed at both optimization levels. Imported fixtures
reorder local type IDs and repeat imports across
real package files. Malformed UTF-8 method mutations completed with bounded,
valid diagnostics. Worker tests distinguish same-named global/method graph
nodes and parent-evaluated spawn arguments.

An internal read-only review found no blocking fixture, expected-output,
package-isolation, or allocation-tracking issue. `rustfmt --edition 2021
--check tests/frontend_differential.rs tests/method_acceptance/mod.rs` passes.

The focused hybrid run passed all seven groups on October 11 in 4.56s (build
finished in 8.76s), using root's current v2 archive and the existing hybrid
target directory:

```sh
BHASA_LEXER_ARCHIVE=/Users/arpanbhandari/Documents/ChatGPT/bhasa/build/hybrid/native/aarch64-apple-darwin/bhasa-frontend/5861ee7a5b8d5f6a027c18d7843bbd39f78154f1aed9d64c6fab70345820d9ae/libbhasa_lexer.a \
CARGO_BUILD_JOBS=2 cargo test --offline --locked --features bhasa-frontend \
  --target aarch64-apple-darwin --target-dir build/hybrid/frontend \
  --test frontend_differential method_acceptance -- --nocapture --test-threads=1
```

The emitter SHA-256 was verified as
`40a51c63b9c683e61b1b06358b35d2ee8530c3243aa38eb07fe0b8c21a195b3e`, matching
root's frozen archive source report. This run compares both compiler APIs,
including per-file scanner parity and actual package loading. Both frontends'
native method fixtures passed O0/O2 with ASan/UBSan, exact output, empty stderr
and zero tracked live allocations. Bind cases remain compile-only; no sockets
were opened by this lane. No stale archive, broad suite, cache clean, tool
installation, commit, or additional chat was used for this verification.

## Completed baseline

The previous milestone edited only `tests/frontend_differential.rs`,
`tools/verify-compiler.sh`, and this file. Compiler implementation fixes belong
to their owners. All package fixtures and references use lowercase `bhasa.toml`.

## Integration

The test has nine groups, including its child-process entrypoint. It uses
`bhasa::compile_to_llvm(source, target)` as the reference and,
under `cfg(feature = "bhasa-frontend")`, calls
`bhasa::compile_with_bhasa(source, target)`. It does not compare IR spelling.
Feature builds additionally compare the Bhasa and Rust lexers for every source
before compiling. The Rust parser/checker/emitter otherwise uses the selected
lexer in a feature build.

Post-audit module-boundary update: the imported-graph case now reloads isolated
on-disk package trees in each compiler child. Its Rust path uses `project::load`,
semantic checking, and LLVM emission; its Bhasa path uses `load_unparsed` and
`compile_files_with_bhasa(&loaded.files, target)`. Flattened text is retained
only to check the source snapshot and virtual diagnostic spans. Scanner parity
runs on each actual module. File-local loader/parser errors are rebased before
the common diagnostic checks. Root owns the separate cross-file stitching
regressions in `tests/hybrid_frontend.rs`. This update passed rustfmt checking;
per root's instruction no compiler suite was rerun. The new official frontend
archive requires the `bhasa-frontend-v2` stamp; older temporary v1 archives from
the evidence below must not be reused for this check.

Read-only review found no blocking issue in the package-test update. It flagged
one unverified boundary edge case for root's separate regressions: a module
ending in bare `\r` can become `\r\n` when flattening appends `\n`, leaving
the module boundary inside the scanner's newline token. The boundary/token
overlap check in `src/bhasa_lexer.rs` appears able to return `H001` for that
valid input. This is a static observation only; no reproduction, implementation
edit, or overlapping regression test was run here.

Compiler batches run as child test processes with a 30-second deadline. Native
links and executions have 30-second deadlines. A timed
out child is killed and reaped. Output goes to files so pipe capacity cannot
deadlock the timeout checker. Test programs do not launch subprocesses.

The corpus includes fixed-seed arithmetic, task lifetime through branches and
loops, recursive worker graphs that reach externs, owned buffer aliases and
temporaries, arithmetic traps, malformed UTF-8-aware source mutations, and
diagnostic span checks. Native output has an independent expected value and
runs at `-O0` and `-O2`. Generated LLVM functions receive `sanitize_address`
attributes before linking so ASan instruments their memory accesses. ASan/UBSan
also instrument the C runtime, with sanitizer recovery disabled. Native links
already pass `-pthread`, including Linux. These tests do not claim leak freedom
on macOS, where LeakSanitizer is absent. Package tests flatten three lowercase
manifest packages and check a worker graph that crosses both dependency edges.

The refreshed Bhasa emitter now enforces the flat-expression limit too.
The depth test requires both compilers to reject a 256-term addition chain and
300 nested parentheses. There are no diagnostic-code or acceptance mismatch
allowlists. New record comparisons cover accepted copy/temporary/RHS replacement
behavior and rejected schema, allocator collision, and overlapping argument
cases. They also require the safe nested read and copying constructor cases
described below to be accepted.

## Commands and evidence

- `cargo test --offline --locked --test frontend_differential -- --nocapture`
  passed the initial five test groups in 11.77 seconds on ARM64 macOS.
- After adding depth/package cases, generated-LLVM ASan instrumentation, and
  30-second native deadlines, the same test target with `--test-threads=1`
  passed all seven groups in 11.44 seconds.
- `CARGO_BUILD_JOBS=2 cargo test --offline --locked --test frontend_differential
  record_ -- --nocapture --test-threads=1` passed both new record groups in
  1.17 seconds with the then-current conservative record expectation.
- A refreshed full frontend archive compiled from lexer/emitter source hash
  `a99a0e53a9f9537cc48c30127bb70f3d921fa4ea454890d3dda798dd7046c40b` passed
  native scalar/owned/task behavior, all arithmetic traps, native records,
  baseline generated acceptance, malformed inputs, and imported worker graphs.
  Two expectations had become stale during concurrent implementation updates:
  Bhasa now rejects the flat 256-term chain and Rust now accepts safe nested
  record reads. Both test expectations were corrected. A record source was
  also added to the mutation corpus.
- Corrected depth and expanded malformed-input feature tests passed in
  0.03 and 0.21 seconds. After refreshing the archive again, both record groups
  passed in 1.72 seconds, including safe nested reads and constructor copies.
  The concatenated lexer/emitter source SHA-256 was
  `8f2be3443444abc0fb427bf25c80eeada1017c04b86388e07ae66f5f33c0acc3`.
  Focused runs used the existing Cargo target directory, `CARGO_BUILD_JOBS=2`,
  `--features bhasa-frontend --test frontend_differential FILTER`, and
  `-- --nocapture --test-threads=1`.
- `rustfmt --check --edition 2021 tests/frontend_differential.rs` and
  `sh -n tools/verify-compiler.sh` passed. The final combined nine-group run
  against root's official hybrid archive belongs to integration. No remaining
  failure is known from focused checks.
- Full frontend verification requires a current archive produced by
  `sh bootstrap/build-hybrid.sh --frontend`; its Cargo test invocation discovers
  `frontend_differential` automatically.
- Read-only preliminary probes matched on same-buffer argument aliases,
  nested owned temporaries, branch-local task name reuse, recursive worker
  spawning, and outstanding-task returns from unreachable loop bodies.
- An independent seed-compiler probe ran 240 deterministic token mutations,
  seed 7441, with 3-second process deadlines. There were no acceptance
  mismatches, crashes, or timeouts. This used snapshots compiled before the
  currently arriving record and editor-tooling changes.
- The first feature-enabled Cargo run was interrupted by the concurrently
  written `src/lsp.rs` being temporarily incomplete. No frontend comparison
  result is claimed from that run. A temporary full frontend archive is ready.

`tools/verify-compiler.sh` runs offline, locked tests with process-group
timeouts and preserves Cargo and hybrid caches. Modes are `--reference`,
`--hybrid-lexer`, `--hybrid-frontend`, or `--all`. No flag selects the reference
suite. `--timeout SECONDS` sets the per-mode deadline, default 900 seconds.
`--require-gpu` requires all applicable GPU hardware tests instead of permitting
their normal explicit skips. Tests print actual hardware results and skip
reasons; CUDA/Vulkan mock-driver passes are not hardware evidence.

The script passed shell syntax/help checks and isolated fake-tool checks for
mode routing, failure propagation, a missing Clang, timeout validation, and
cleanup of a child ignoring SIGTERM. Full suites remain the integration lane's
run. Root can set `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1` for serial combined
verification.

## Findings

The audit initially found two safe record calls rejected with `S028`:

```bhasa
struct Box { text: buffer }
def consume(b: Box, size: int) { print(size) }
def main() {
    var b = Box(buffer("old"))
    consume(b, len(b.text))
}
```

`len` finishes before `consume` begins and cannot invalidate `b`. A second
case uses `struct Wrap { box: Box }`, a function with `Box, Wrap` parameters,
and `consume(b, Wrap(b))`. The constructor makes an independent copy. The
then-current `argument_accesses` traversal treated both operations like nested
mutable borrowing. Root initially identified this as a deliberate conservative
restriction. The records implementation has since replaced that traversal with
`argument_mutations`. Both exact examples are retained as positive regression
expectations and now pass with the refreshed Rust and Bhasa implementations.
No memory fault was demonstrated.

Confirmed package read finding: making `bhasa.toml` a FIFO and running
`bhasa check --json` blocks on the manifest read. Two ordinary-file controls
completed in 0.0048 and 0.0057 seconds; alternating FIFO cases both hit the
3-second process timeout. Reproduce with `os.mkfifo(root / "bhasa.toml")`, an
otherwise ordinary `main.bh`, and run the CLI with that root as its cwd.
At reproduction time `read_manifest` did not check that the input was a regular
file before reading. Root fixed this with bounded regular-file reads, a 1 MiB
manifest cap, and source reads bounded by the remaining 64 MiB graph budget.
Root reports all three `tests/project_limits.rs` tests passing, including FIFO
manifests/modules, directories, and oversized files. This lane did not duplicate
the implementation or suite.

The record audit found no type-ID remapping failure in a multi-package program
with differing module-local type order, forward record references, duplicate
imports, returned borrows, and copy mutation. It compiled and printed
`42\nvalue\n`. A duplicate imported record diagnostic mapped to the real file
and line. An assignment whose RHS replaces the same record field completed
correctly. These independent record probes did not use sanitizers.

The first 7-group default and feature runs passed all compile acceptance comparisons,
including package/depth cases, but native execution exceeded 10-second
deadlines during concurrent runs. A separate audit saw an ordinary `check`
stall once too, and host load averages exceeded 40. Those runs do not count as
native passes. A later isolated native
group passed in 2.53 seconds with the original 10-second deadline. No generated
program defect was reproduced. Native deadlines are now 30 seconds, and the
next serial default run passed all seven groups. Root now serializes final
integration verification; this lane uses two Cargo jobs for focused tests.

These finite checks are regression evidence, not a production safety claim or
a proof of equivalence.
