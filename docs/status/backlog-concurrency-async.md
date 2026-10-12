# Concurrency and async lane

Updated October 12, 2026. The isolated host channel implementation is ready for
coordinator review and native routing. Actual module/generic lowering, trusted
registry binding, host checking and LLVM emission now execute against source
fixtures. The five-argument native bridge type-checks; native parser/checker/
emitter execution and shared integration remain coordinator-owned. No tracked
shared source edits, Cargo, broad suites, Git mutations, SDK installation or
cache cleaning were performed. Runtime remains frozen.

## Frozen host implementation and handoff

The ratified registry correction is implemented under
`build/backlog/concurrency-async/source-integration`. The early API contract is
`host-api-handoff.md`; its fixed native layout and diagnostic allocation are
now frozen. `channel-host.patch` contains contextual source changes against
saved current sources plus the frozen alias module/generic overlay.
`channel-additions.patch` contains the four helper modules and approved
`std/channels.bh`. Individual patches are in `patches/`; `patch-baselines.json`,
`baseline.json` and `draft.sha256` identify exact preimages and final bytes.
Preserve newer root edits and apply narrow hunks, never overwrite shared files
with these copied snapshots.

Production module lowering calls private registration for the canonical
repository `std/channels.bh` location and exact compiled-in reviewed bytes.
Modified unsaved SourceFile bytes reject before issuance. Identical bytes at
an unregistered path grant no authority. Installed-library relocation needs an
explicit approved registration; basename or prefix matching cannot grant it.
Private module/expansion receipts retain exact transformed source/boundaries,
actual export bindings and canonical generic family identities.

Annotation-first Sender/Receiver use interns the entire Channel/payload/results/
factory family in the existing specialization table. Frozen alias TypeRoots are
preserved. Final host binding follows nominal enum normalization and original
span mapping. Program stores an opaque BoundAuthority with private constructors;
raw parser programs have none. Type/factory mutation invalidates authority.
No public issuer, serializer or caller-created ID list grants channel behavior.

Checked operations retain existing Call nodes. Actual issued owner identity
resolves sender/receiver getters, send/try_send, recv/try_recv and close into
exact signatures and helper names. Endpoint/pair fields and direct construction
are forbidden. Only the reviewed factory body can construct a Channel from
one int. Source getter methods are forbidden. Unrelated user send methods keep
ordinary behavior. Strict payload and authenticated worker transfer contexts
remain separate; bounded DAG/depth checks are retained.

Host checking and public emission recheck resolved worker method/extern/pool
call graphs, global target identity, transfer signatures and registered blocking
effects. Existing public failure-use rechecking preserves discarded/pending
result obligations. Native diagnostics are reserved as BH223 for invalid/stale
channel authority, matching Rust C101, and BH224 for reachable registered
blocking worker operations, matching Rust S020. BH222 remains owned transfer.
Root will reserve readonly diagnostics separately.

The host native branch produces a private NativeManifest and calls new symbol
`bhasa.fn.bh_compile_channels(source,tokens,errors,target,manifest)`. Manifest
words are borrowed read-only for one synchronous call, with source/context held
alive. Version 1 has five header words and 21 words per concrete family, using
whole expanded declaration spans and scalar/nominal payload tags. The native
lane must bind these anchors to its own node/type IDs and validate schemas and
factory shape. The ordinary four-argument entry remains for no-channel units.
The new symbol is a required link dependency, so old native prefix dispatch
cannot silently handle authenticated channel units. Native execution parity
has not been claimed. Existing native chat finishes owned parity first; root
routes this frozen channel handoff afterward. No new native chat was created.

The driver patch extracts channel.c/h and links channel.c; channels imply the
concurrency runtime and pthread flags. Existing cache command hashing includes
source bytes and preprocessed headers. No build_cache change is needed.
Root still applies the frozen runtime/concurrency patches and performs combined
FileHandle tag11, owned ParamMode/Move, driver/FFI and editor/loader integration.
FileHandle must be explicitly rejected in channel/worker transfer and handled
in exhaustive type matches. Registered factory parameters remain Borrowed.

## Executed evidence

`source-integration/verify.py` and `verification.log` reproduce direct rustc
host pipeline probes, native bridge metadata checking, and six small Clang
guests. No Cargo or native frontend archive build was used.

Source checks exercise actual reviewed-module registration and module/generic
producer, host registry/checker and LLVM consumer. They cover constructors and
private fields, wrong payloads, discarded/pending results, task/nested endpoint
payload refusal, modified reviewed bytes, stale manifest bytes/unsupported
targets, declaration/factory mutation, annotation-first interning, alternate and
unaliased imports, unrelated send methods, worker endpoint transfer, transitive
helper/method blocking through spawn and async_spawn, and post-check blocking/
pool call mutation. Identical bytes at a separate module path grant no authority.

The blocking, owned-worker and try source guests each print 42 and exit zero
at O0/O2 with address/undefined behavior sanitizers and nonrecovering checks.
Allocation instrumentation covers C runtime allocations and generated LLVM
malloc/free calls; all six report zero live allocations at exit. Cases include
empty/closed receive, close then queue drain, copied buffer isolation, full/closed
send failure payload ownership, worker endpoint/result ownership, unread queue
cleanup on last owners, and typed try propagation. macOS general leak detection
is unsupported; only the explicit allocation accounting is claimed.

The earlier frozen runtime probe passed 40,000 FIFO transfers, simultaneous
endpoint drops, ownership/drop counters and native guard cases at O0/O2 under
ASan/UBSan. Earlier standalone helper probes passed 16 registry/transfer cases
against two AST snapshots and LLVM ownership accounting. Those artifacts remain
in the lane root and original `draft/`; new production host evidence is in
`source-integration/`. `runtime-frozen.sha256` still matches channel.c/h and
concurrency.c/h exactly. Root's independent-review report is untouched.

## Coordinator acceptance still required

Native parser/checker/emitter parity, its new entrypoint and refreshed archive,
real project-loader/LSP overlays, combined owned/file AST rebasing, native driver
execution, dual-frontend source acceptance, A/B/C, release/cache and broader
suites remain coordinator gates. Draft completion is not integrated acceptance.
All other Bhasa thread sources and status files were left to their owners.

Select, cooperative cancellation/deadlines, supervision/task groups, locks/
atomics, executor changes and coroutine/reactor I/O remain later slices with the
prerequisites recorded in `draft/docs/channels-contract.md`. No backlog checkbox
or unsupported platform/hardware completion is asserted.

## Independent-review target-path correction

Root reproduced a P1 defect in the frozen wiring: its hardcoded Darwin target
proof hid rejection of actual Clang `arm64-apple-macosx26.0.0` and documented
macOS targets. A separate correction is ready under
`build/backlog/concurrency-async/target-correction`, with handoff.md,
`target-correction.patch`, exact preimage/final hashes, verification.log and
patch-verification.log. Original frozen source-integration evidence is preserved.

The existing target layer owns one strict resolved-triple admission function,
called by both channel LLVM helpers and manifest production. It validates complete
architecture/vendor/OS/ABI/version components, preserves the emitted deployment
triple, and leaves CLI aliases and existing run guards unchanged. Fifteen target
positives, including the actual driver frontend query and four documented POSIX
aliases, agree across emission/manifest. Thirty-two malformed/platform/ABI cases
reject. Thirteen focused target-layer tests pass. The actual-host LLVM guest
prints 42 at O0/O2 with ASan/UBSan; no target override warning occurs.

Runtime, manifest layout and BH223/BH224 remain frozen. No shared source, Cargo,
Git or native assignment occurred. Root applies this three-file contextual
correction after the frozen channel patches before native routing.

Root's independent replay accepts the separate target correction for native
implementation. `build/channel-source-independent-review/target-correction/review.md`
records the five original targets passing with exact LLVM triples and twelve
additional malformed/platform/ABI cases rejecting consistently as C001/C101.
The original failing evidence remains frozen. Root is routing the accepted
shared target policy and five-argument source-bound manifest to the existing
native chat. Runtime, source authority, payload/manifest rows, operation guards,
BH223/BH224 and original patches are preserved. This review does not establish
native parity or complete any integration checkbox.
