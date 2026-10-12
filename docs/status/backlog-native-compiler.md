# Native compiler backlog lane

Updated October 11, 2026. First draft increment is ready for coordinator review.
It is not integrated and does not mark any todo item complete. Shared sources,
Rust host, runtime ABIs, native/Cargo dependency caches and other lanes are intact.

## Implemented first increment

`build/backlog/native-compiler/draft/src/incremental.rs` implements a deterministic
frontend invalidation planner with exact source-byte equality and separate parse,
check and emit contracts. Snapshot construction rejects duplicate module IDs and
missing resolved imports. The plan identifies current modules to parse, modules
whose file-local parse may be reused, removed modules to evict, and whether the
whole checked/emitted graph must be rebuilt. Empty graphs and cycles terminate
without recursive dependency walks. `ParsedModules<T>` adds actual in-memory
parsed-value storage keyed by module identity, exact source bytes and parser
contract. Failed parses retain the last successful revision but never return it
for changed source. Missing payloads trigger parsing even when the schedule says
reuse; graph retention evicts removed modules. There is no persistent cache, CLI
integration, actual Bhasa AST connection or performance claim yet.

The draft intentionally invalidates graph checking and emission for every source,
import-resolution or module-order edit. Current `src/modules.rs` and
`src/generics.rs` lower the loaded graph before parsing. Module numbering, generic
request order, private enum checks, worker-call safety and virtual source offsets
can change beyond the edited module. Reusing checked modules based only on public
signature hashes would be unsound. Comment edits invalidate parsing/checking for
source locations even when final LLVM remains reusable in the existing artifact
cache. Target/optimization changes invalidate emission; frontend/parser changes
invalidate every stage; builtin/ownership/privacy/generic checker changes
invalidate checking and emission.

API types are `Contracts`, `Module`, `Snapshot`, `Plan`, `InvalidGraph` and
`ParsedModules<T>` and `plan(previous, current)`. Contracts are opaque versioned implementation bytes.
The eventual driver must supply actual implementation identities, frontend choice
and stage-specific options. Module IDs must be stable resolved identities;
ordered imports record actual resolved targets. No timestamps, file sizes or
caller-provided source digests stand in for full source equality. Snapshots have
private fields and are validated at construction. Plan reuse is conditional on
available matching payloads; it is never proof that an artifact exists.

## Approved native parity

Typed errors/initializer try are already in the frozen native emitter and host
bridge. Imported generic error payload changes reject with `S035` in Rust and
`BH219` in Bhasa, at the same original library byte offset, including multibyte
comments and CRLF. Changing an inferred result enum to private rejects implicit
importer deconstruction with `M005` at the original try marker in both modes.
Sixteen focused existing-compiler probes pass without Cargo or native builds.

Binding-only let remains reserved for integration. The prepared native patch at
`build/immutable-draft/native/emitter.bh` stores local reassignability in field7,
keeps ownership in field5 and task-worker metadata in field6, and emits BH220 on
reassignment. `build/immutable-draft/native/tests/immutable_bindings_bhasa.rs`
is the existing parity test. Its README/probe log records 18 invalid programs,
five contextual-name compatibility programs, O0/O2 ownership checks and A/B/C.
Those are prior-worker evidence, not rerun evidence from this lane. The root must
still apply its BH220 bridge mapping and host/Rust/editor metadata changes. This
lane did not duplicate or apply that reserved increment. Aliases, borrows and
pending-failure rules need ratified concrete contracts before native patches;
no syntax or ownership contract is invented here.

## Files and source baseline

All implementation and tests are in `build/backlog/native-compiler/draft`, using
original relative paths. Added files and matching standalone patches:

- `src/incremental.rs`, `build/backlog/native-compiler/incremental.patch`
- `tests/incremental_plan.rs`, `build/backlog/native-compiler/incremental_plan.patch`
- `tests/incremental_frontend_probe.py`, `build/backlog/native-compiler/incremental_frontend_probe.patch`

`build/backlog/native-compiler/baseline.json` records HEAD
`c4f89575fd0f817f1d0cb02915ab841de166d05f`, SHA-256 contents of the actual dirty
frozen lib/bridge/emitter/project/module/generic sources and approved contracts,
and hashes of the reserved let draft. Exact source copies are under
`build/backlog/native-compiler/baseline/`. Do not restore those copies over newer
integration work. These three patches only add files and cannot overwrite shared
changes. The planner is deliberately unexported until the host integration below.

## Verification and next integration increment

Ten standalone Rust planner/storage tests pass with `rustc --edition 2021 --test` and
one test thread. Evidence is `build/backlog/native-compiler/plan-probe.log`.
They cover cold/warm work counts, private-body changes, Unicode/comments, module
order, resolver edits/addition/removal, stage contracts, missing/duplicate modules
and empty/cyclic graphs. Storage tests measure parser invocation counts across
hits, failed revisions, contract changes, absent payloads and graph eviction.
No Cargo suite ran. The existing full hybrid executable
at `build/hybrid/frontend/aarch64-apple-darwin/debug/bhasa` passed 16 clean frontend
oracles, covering both modes. Evidence is `frontend-probe.log` and
`frontend-probe.json` in the lane directory, with compiler SHA-256, statuses,
LLVM hashes and per-invocation elapsed nanoseconds. These samples are correctness
probes, not representative compilation benchmarks.

Coordinator can apply these file additions, then run the focused planner test
with its serialized Cargo slot. No immediate heavy build is requested. Exact
commands, after copying the additions into the repository:

```sh
CARGO_BUILD_JOBS=2 cargo test --test incremental_plan -- --test-threads=1
python3 tests/incremental_frontend_probe.py build/hybrid/frontend/aarch64-apple-darwin/debug/bhasa
```

The next compiler increment must store unlowered per-file tokens/AST with local
spans and stable declaration IDs. Today parser input is the already-lowered graph;
applying this plan directly to current generated ASTs would reuse stale names and
locations. Connect `ParsedModules<T>` to that parser, then connect graph checking and
emission with separate success-only publication. The parsed cache already
checks exact source/contract bytes, invokes parsing for absent payloads and
retains only successful parsed values. A parse-success/check-failure may retain
syntax, but must never publish a successful checked or emitted graph. Persistent
corrupt payloads must force actual work even when this plan says reuse. Keep resolver
loading outside cached syntax and rebuild composed source maps on graph edits.
Then compare incremental and clean output/diagnostics on these exact oracles with
actual stage counters. Unchanged warm runs must report zero parses, while a leaf
edit reports one parse and full graph checking/emission. The existing native
artifact and Rust adapter caches remain separate, and Cargo adapter freshness
must still precede native artifact lookup.

Checked-module reuse needs checked dependencies on private body effects,
privacy, template bodies, specialization request sets and worker call graphs.
Emission reuse needs stable symbol names and deterministic link order. Persistent
cache publication, concurrent/corrupt/symlink entries, reproducible relocation,
representative timing/memory measurements, generic specialization caching,
parallel work and further host migration remain open. Both frontends, no-GC
cleanup and original-file diagnostics remain acceptance requirements. Linux,
Windows, Wasm and hardware execution are not claimed by these local probes.

## Pending failure native parity, priority update

Coordinator routed this contract after typed errors committed as `86a99ed`.
Native pending diagnostic is BH221; Rust is S037. Older safety draft/status S036
references must be changed by that owner, because S036/BH220 are reserved for
immutable reassignment. The incremental work above remains a prerequisite.

Draft `build/backlog/native-compiler/draft/bootstrap/compiler/emitter.bh` is based
exactly on `build/immutable-draft/native/emitter.bh`, with baseline copy at
`pending-baseline/bootstrap/compiler/emitter.bh` and SHA-256/HEAD/contract hashes
in `pending-baseline.json`. Apply `pending-failure-native.patch` contextually
AFTER the reserved immutable emitter patch. It appends field8 pending-result,
separate from field5 ownership, field6 task signature and field7 reassignability.
No context-header, scanner token, type ID or runtime ABI changes are needed.

The patch marks result-shaped Var/let/TryVar successes and match/for payloads.
Parameters and hidden snapshots retain pending=0. Direct named expressions,
including parenthesized names, discharge on exhaustive match, return and try;
field/call/compound expression paths do not discharge their owners. Other live
obligations reject reachable returns and implicit try error returns. Assignment
rejects overwrite and conservatively rejects result assignment to outer loop
bindings. Scope/iteration/break/continue exits reject pending locals. Branches
restore input snapshots before each path and union only continuing paths. Match
payloads receive their own obligations. Loops restore pre-loop outer state to
preserve zero-iteration behavior. Snapshot identities are lexical local nodes,
so shadowing, copying, scalar reads and method calls cannot erase an obligation.

The combined candidate seed at `build/backlog/native-compiler/pending-seed.bh`
passes bounded existing `target/debug/bhasa check ... --frontend rust` and
`emit-llvm ... --frontend rust` probes, producing `pending-seed.ll`.
Evidence is `pending-seed-check.log`. This checks candidate source validity,
not execution of the new native rules. Native execution subsequently passed the authorized slot and probes described
in the executed evidence below. No tracked emitter or other shared file has been modified by this lane.

The coordinator authorized this coordinated slot, with an existing compiler
and no Cargo operation:

```sh
target/debug/bhasa build build/backlog/native-compiler/pending-seed.bh --frontend rust -o build/backlog/native-compiler/pending-stage-a
```

This compiled one candidate native seed. Bounded check probes ran through it. Root still owns full hybrid rebuild,
BH221 bridge text, combined Rust/native gates, test migration for previously
unhandled Result locals, O0/O2 ownership, releases/A/B/C/cache verification.
Requested integration gates will be recorded with the focused fixtures once
native execution establishes candidate behavior.

Focused test additions are `draft/tests/failure_use_bhasa.rs` and
`draft/tests/pending_failure_probe.py`. Ten hybrid acceptance groups compare
Rust S037 and native BH221 on local obligations, control-flow union, loops,
returns/try, nested results, concrete generics/methods/tasks, parenthesized names
and field subjects; they also preserve immutable S036/BH220 precedence.
The seed probe contains 41 concrete cases, with each subprocess bounded to
15 seconds. Its unspecialized List[Result] for-binder fixture is explicitly
reserved for real host lowering and is not presented as seed evidence.
The 41 concrete seed cases have now executed successfully. The ten integrated
hybrid groups remain authored and type-checked, awaiting root integration.
After the seed build slot, run:

```sh
python3 build/backlog/native-compiler/draft/tests/pending_failure_probe.py build/backlog/native-compiler/pending-stage-a
```

After contextual integration, root gates are:

```sh
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test failure_use_bhasa -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test failure_use_reference -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test error_propagation_bhasa --test errors_integration --test immutable_bindings_bhasa -- --test-threads=1
```

The third gate requires migrating previous bound-result discard fixtures through
real direct handling. Authored integration coverage includes imported/generic BH221 exact original
closing-brace spans, List[Result] binder obligations and match-arm union. Those
fixtures, owned cleanup and self-rebuild still require execution.

The focused Rust test source type-checks with existing library
`target/debug/deps/libbhasa-bd69e13e13c63319.rlib` through a bounded standalone
rustc test compilation. No tests from that binary ran, because the linked
library predates pending-failure integration. This catches API/type mistakes;
it does not establish native or Rust behavioral parity. The candidate compiler
source also emits LLVM successfully through the existing Rust frontend.
New test patches are `failure_use_bhasa.patch` and `pending_failure_probe.patch`.

## Executed pending-failure native evidence

Root authorized the single isolated seed build after its immutable Cargo check.
Existing `target/debug/bhasa` built `pending-stage-a` successfully for local
arm64 macOS. The exact seed command above completed in 1.82 seconds. No Cargo,
tracked edits, cache deletion or second native seed build ran in this lane.

`pending_failure_probe.py pending-stage-a` passed all 41 concrete cases. Each
subprocess has a 15-second timeout; the aggregate run completed in about 0.31
seconds. It covers unread/copied/passed/overwritten results, self-assignment,
branch union and returning paths, result/scalar shadowing, zero-iteration loops,
iteration-local results, break/continue, outer/nested-loop assignment rejection,
direct return and try, parenthesized return, other-live-result try errors, nested
success payloads and match payloads, parameter/Option exclusions, owned let/try,
BH220 immutable precedence and existing BH205 unreachable precedence.

Evidence is `build/backlog/native-compiler/pending-probe.log` and
`pending-probe.json`; the JSON records each source label, exit status and full
native diagnostic byte range. Fixtures are retained under `pending-fixtures/`.
Seed SHA-256 is
`0624f13dd4ce31423049611ee6555cecb8e89eaafdb309423422704b7613b993`.
The candidate also passes `pending-stage-a check pending-seed.bh`; log is
`pending-selfcheck.log`. This checks its own compiler source using the new native
rules. No A/B/C executable rebuild or LLVM equality claim follows from that check.

No defect required a source repair during these native probes. The 281-line
contextual emitter patch is ready to integrate after the immutable source.
Root must still map BH221, apply S037 checking/emission, execute the ten hybrid
acceptance groups and migrate affected previously-unhandled bound Results.
Imported/generic original-file locations, real List[Result] binders, method/task
paths, O0/O2 allocation/cleanup and frozen combined/release/cache gates remain
integration criteria. The seed probe explicitly leaves its unspecialized generic
for-binder fixture for real shared host lowering; no syntax failure is counted
as pending-result acceptance.

## File-handle native parity draft

Root routed the std-backend file runtime contract after pending-failure probes.
Pending emitter/source/patch remain frozen. The new isolated original-path draft
is `build/backlog/native-compiler/file-handle/draft/bootstrap/compiler/emitter.bh`.
Its baseline is the exact frozen pending emitter at
`file-handle/baseline/bootstrap/compiler/emitter.bh`; `baseline.json` records its
SHA-256, HEAD, actual tracked emitter hash at review and runtime header hash.
Apply `file-handle/file-handle-native.patch` contextually AFTER pending-failure
and immutable integration. It does not replace shared sources.

Proposed stable native type ID is 11. Current emitter IDs are void/int/bool/str
0..3, buffer/ints/task 4..6, TCP/TLS 7..9 and float 10, with aggregate types
node+100. The host bridge transports source/tokens/diagnostics rather than a
cross-frontend type-ID table, so no scanner keyword, token code or bridge type
code is added. Root must coordinate ID 11 against later primitive/channel types.
The Bhasa Rust AST owner separately adds `Type::FileHandle`.

The draft recognizes identifier-spelled file_handle, lowers it to owned ptr and
uses `bhasa_rt_file_clone`/`bhasa_rt_file_free`. Existing generic aggregate and
enum/list/map element cleanup invokes these hooks. All eight builtin APIs match
std-backend status signatures, including both string/buffer file_open overloads.
All file operations require use like the new Rust builtin table, including
open/status/position. No runtime numeric status, try shape, task metadata,
pending-result field8 or immutable field7 contract changes.

Sequential file_handle aggregates are supported for typed IoResult and ownership.
Every thread-transfer path rejects handles nested in record/enum/List/Map fields,
direct handle arguments or results, helper signatures and method receiver types.
An iterative visited-type walk covers aggregate schemas. File builtins add the
existing forbidden-worker marker to call edges, so transitive file operations
reject even if the worker input/result is int. Existing file-free typed tasks stay
admitted. Native BH209 remains the worker diagnostic. No unsynchronized shared
cursor/refcount or file callback crosses into worker execution.

The runtime clone hook retains shared descriptor/cursor state and has no OS
operation. The native compiler never models close as consuming one alias or as
freeing a value; file_close is a borrowed integer-status call, so runtime close
invalidates all aliases, including those copied into a Bhasa function. Deterministic
free remains required after explicit close and on every cleanup path.

Candidate file seed source passes existing Rust checking/emission and the frozen
pending native seed's source check. Evidence is `file-handle/seed-check.log`,
`native-source-check.log` and `seed.ll`. Five authored hybrid groups in
`draft/tests/file_handle_bhasa.rs` type-check through the existing Rust library.
They cover ABI signatures, sequential ownership, must-use/wrong signatures,
direct/nested/generic transfer and transitive/method worker graphs. The bounded
`draft/tests/file_handle_probe.py` contains 31 concrete check/emission cases;
generic/module/IoResult/runtime execution stays in the root integration gates.
Native file checking/emission has subsequently passed the authorized seed
slot; file runtime/guest execution remains unclaimed. The final marker repair
requires updated native execution as described below.

Requested next coordinated slot, one isolated seed with the existing compiler:

```sh
target/debug/bhasa build build/backlog/native-compiler/file-handle/seed.bh --frontend rust -o build/backlog/native-compiler/file-handle/stage-a
python3 build/backlog/native-compiler/file-handle/draft/tests/file_handle_probe.py build/backlog/native-compiler/file-handle/stage-a
```

Root integration gates after std-backend Rust/runtime/driver and native patches:

```sh
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test file_handle_bhasa -- --test-threads=1
```

Also execute std-backend's typed streaming fixture in both frontends, including
IoResult[file_handle] copy/return/try cleanup, alias close through copied arguments,
private Error, corrupt checked AST worker rejection and O0/O2 allocation/fd
accounting. Root must link and cache-hash file.c/file.h and add supported-target
guards. Existing driver/native cache gates, pending/immutable tests and A/B/C
remain required. This lane did not run Cargo or edit the bridge/driver/shared
runtime. No Linux/Windows file execution is claimed.

The pending-evidence SHA-256 manifest was checked after preparing the file draft;
its frozen source, seed, patch and test files remain byte-identical. The file
candidate adds file runtime declarations alongside the emitter's existing
unconditional declaration block. Root driver linkage/target guards must therefore
recognize actual file use or otherwise preserve file-free unsupported-target
behavior; declarations alone do not demonstrate an executed file operation.

## File-handle executed seed evidence and marker repair

Root approved one isolated file-handle seed build, using existing target/debug
only. The build succeeded for arm64 macOS in 1.38 seconds. All 31 concrete
checking/emission cases passed in about 0.24 seconds. Every API lowers to the
published ptr/i64 ABI; emitted owned paths use file_clone/file_free. Direct,
nested aggregate, enum, pooled, helper and receiver worker paths reject BH209;
file-free owned tasks remain accepted. Must-use rejects with BH207 and invalid
signatures with BH201. No compiled guest/file operation was executed.

The candidate also checks its own source. Re-running all 41 pending-failure
cases through it produces identical statuses and diagnostic ranges to the frozen
pending seed, including immutable BH220 precedence. File evidence is
`file-handle/probe-before-marker.log` / `probe-before-marker.json`,
`selfcheck.log`, `pending-regression.log` and `pending-regression.json`.
The original pending-probe JSON has been retained for the original pending seed;
its frozen source/patch/test SHA manifest still matches.

Subsequent review found the new native runtime declarations were not accompanied
by std-backend's Rust marker `; bhasa runtime: file`. That omission would prevent
normal driver runtime selection. The final draft/patch now emits the marker by
scanning actual definitions after record helpers, matching other native runtime
markers. It does not scan the unconditional declaration block, so file-free
programs remain unmarked. Tests now require the marker for calls and owned
cleanup and its absence for file-free tasks. No type tag or local field changed.
Native file_handle remains tag11, alongside Rust Type::FileHandle; pending field8
and immutable field7 retain their exact layout. Scanner/FFI token numbers remain
unchanged.

The marker-repaired seed passes native source checking and existing Rust LLVM
emission. `marker-source-check.log` and final `seed.ll` record those checks.
The one authorized seed executable predates this final marker hunk; its 31-case
pass must not be attributed to execution of the marker revision. The old source
reconstructed by removing that sole marker line is retained as
`seed-before-marker.bh` and `emitter-before-marker.bh`; `evidence.sha256` records
both final/pre-marker sources, seed and test/patch identities.

Request one additional isolated build of the same stage-a path to execute the
marker assertions, or cover the final patch with root's full hybrid refresh and
`file_handle_bhasa` gate. Exact bounded command is the same build/probe pair
above. No second build was run without a slot. Root's final integration gates
still cover typed IoResult/file alias-close runtime behavior, generic collections,
private Error, malformed checked AST, deterministic O0/O2 resource accounting,
combined suites/self-rebuild and runtime/cache selection on supported targets.

## Final file runtime-marker execution

Root authorized the additional single isolated build of the marker-repaired
candidate. The build completed in 1.14 seconds using existing target/debug/bhasa,
with no Cargo or shared tracked edit. The final 31-case checking/emission probe
passes, now including assertions that operations and clone/free cleanup receive
`; bhasa runtime: file`, while a file-free owned task does not. Final self-source
checking also passes. This supersedes the outstanding marker-build request above.

Final evidence is `file-handle/probe-final.log`, `probe-final.json`,
`selfcheck-final.log` and `final-evidence.sha256`. Final seed SHA-256 is
`8965adbcf41ffaba328d059da222556f77a985bd0c6f447a312f3cdbd61a2afe`. Prior `probe-before-marker.log` and
`probe-before-marker.json` retain the distinct pre-marker observations and seed
hash. `seed-before-marker.bh` / `emitter-before-marker.bh` retain that source.
The stage-a path now contains the final executable; the prior historical hash
manifest was retained as `evidence-before-final-build.sha256`, while the final
manifest matches current artifact paths.

A direct definition comparison confirms be_bind and every pending-failure
helper are byte-identical to the frozen pending source. `layout-check.log`
records this; immutable field7 and pending field8 are unchanged. File scalar
tag11 matches the root's provisional agreement with Rust Type::FileHandle.
The final native file patch is ready for root's driver/runtime/cache review and
hybrid integration gates. No compiled guest file/runtime/hardware acceptance is
claimed by checking/emission and self-source checks.

## Owned buffer/ints parity, early metadata handoff

October 12, 2026. Root routed ratified `owned-bindings-milestone.md` and the
actual host ParamMode/Move API. Work is isolated under
`build/backlog/native-compiler/owned-bindings`, based on the frozen final
file-marker emitter. No pending/file source, patch or tracked file is edited.

Native design retains signature [parameter-token,type] pairs and their existing
stride, pointer ABI and task metadata. Separate mode nodes store signature ID,
parameter index, Owned mode, contextual owned token and declared concrete type;
ctx31 owns that table. Borrowed is the default for absent mode entries. Header
ctx32 tracks controlled execution depth, ctx33 whole-expression move admission,
ctx34/35 expression-operation/source-local facts, and ctx36 active argument loans.
Local fields9/10/11 store initialized, transfer-eligible owner and control depth.
Field5 stays cleanup/task state, field6 task signature, field7 mutability and
field8 pending result. A moved owner retains its cleanup classification but
initialized=0 suppresses cleanup/read. Reinitialization restores initialized=1.

This matches host ParamMode::Borrowed/Owned and ExprKind::Move. It adds no token,
keyword or FFI type/layout code and retains file scalar tag11. Native move is a
contextual source operation, not an ABI intrinsic; pointer adoption avoids clone
and suppresses caller argument release for Owned positions. Mode declarations
and emission validate buffer/ints eligibility, synchronous free-function status
and source-mode identity. Host bridge addition is only BH222 diagnostic text.
Concrete draft and guards are being implemented; no executed parity claim yet.

Owned parity implementation now exists at
`owned-bindings/draft/bootstrap/compiler/emitter.bh`, with bridge draft
`owned-bindings/draft/src/bhasa_lexer.rs` adding BH222 text. Source checking passes
through existing Rust and frozen native file seeds; existing Rust LLVM emission
produces `owned-bindings/seed.ll`. These are checks of candidate compiler source,
not execution of its new ownership rules.

Parameter mode lookup is bounded and rejects inconsistent source-mode/type
metadata during declaration, call and function emission. Initialized reads,
eligible owner/cleanup checks, immediate reservation and active lexical loans
independently guard move emission. Named owned arguments reject; fresh pointer
results and whole moves adopt into the callee. Ordinary parameters and returns
retain borrowing/copy behavior. Reinitialization skips the moved old slot only
in its mutable straight-line scope. Conditional/loop/short-circuit execution
increments control depth; match payloads are ineligible borrowed transfer sources.
Methods/generic/extern owned modes and task targets are excluded. File tag11,
immutable field7 and pending field8 are preserved.

The source test harness `owned-bindings/draft/tests/owned_native_probe.py` retains
actual owner-lane fixture bytes and expected host outcomes in
`fixture-expectations.json`, without rewriting that lane. Additional native
metadata corruption and runtime counter harnesses are being prepared. No Cargo,
shared-source edit, cache cleaning, SDK or Git mutation ran.

The owned draft is ready for the first bounded native build slot. Source fixtures
now retain 41 actual host cases/outcomes. A native counter harness copies the
independent reference counter runtime and four owner fixtures into this lane,
then requires the same exact buffer/ints new/clone/drop counts and outputs at
O0/O2 with ASan/UBSan and nonrecovering sanitizers. It also emits test-only LLVM
wrappers over internal mode/move guards and checks 16 valid/corrupt metadata
cases at both levels. It adds no mutable public native AST API. Corruption cases
cover missing/source-inconsistent modes, type mismatches, owned extern/method,
cycles, initialized/cleanup/eligibility state, controlled execution, expression
position, file exclusions and an active loan. Those harnesses are authored,
not executed yet. Their macOS target is explicit; no cross-platform claim follows.

Requested coordinated bounded slot consists of one isolated owned seed build,
41 source probes, eight small guest builds/runs and two metadata guard builds/runs:

```sh
target/debug/bhasa build build/backlog/native-compiler/owned-bindings/seed.bh --frontend rust -o build/backlog/native-compiler/owned-bindings/stage-a
python3 build/backlog/native-compiler/owned-bindings/draft/tests/owned_native_probe.py build/backlog/native-compiler/owned-bindings/stage-a
python3 build/backlog/native-compiler/owned-bindings/draft/tests/owned_native_counters.py build/backlog/native-compiler/owned-bindings/stage-a
```

No slot or native build was taken without root coordination. Root still owns
combined source/AST/mode/bridge integration, generic/method/alias/editor visitors,
public corrupted checked-AST gates, fresh hybrid acceptance, A/B/C, release/cache
verification and all Cargo/Git work. Existing pending and final file artifacts
remain frozen. The owned draft has no integrated acceptance claim yet.


## Owned native slot completed after app restart

October 12, 2026. Resumed the existing granted slot from its already built seed.
No second seed build ran. All subprocesses ran serially. Original executable
`owned-bindings/stage-a` SHA256 is
`a0cc14445dd83abb3c4bd4e1503f132b89adf19bbff06a1789d5ba2cb2489812`.
Its compiler source and contextual patches are frozen in
`owned-bindings/executed-evidence`, with `manifest.sha256`. That manifest also
records the collected results and reconciled comparison against actual host
counter results. The earlier `draft-evidence.sha256` describes the original
pre-slot authored artifacts; revised harness expectations have their own frozen
execution/correction manifests and do not rewrite that historical manifest.

The 41 source probes passed 40 expected diagnostic outcomes. `owned_task_param`
rejects with BH211 because native type parsing rejects task before mode validation;
expected transfer code is BH222. Acceptance/rejection agrees on all 41 cases.
The borrowed-rebind harness mapping was corrected from BH222 to existing BH201;
the original host S021 diagnostic remains verbatim. The original expectations are
retained in `fixture-expectations-initial.json`.

All eight O0/O2 counter guest executions were collected under ASan/UBSan with
`-fno-sanitize-recover=all`. Actual owner evidence confirms six pass. Both
`transfer_counts` executions have the expected stdout but exit99 with
`allocations=7 copies=0 drops=2 live=5`, instead of 7 drops/live0. Native
`be_block` cleans body locals to its saved parameter head; `be_function` emitted
implicit `ret void` without cleaning adopted parameters. Early explicit-return,
ordinary-copy and try-path counts agree with the host. The initial try test
expected an obsolete stdout/count tuple. Collected native output actually equals
current host `7\nok\n` with 7 allocations, 4 copies and 7 drops/live0.
`counter-results-reconciled.json` retains original executions and actual-host
comparison; no successful rerun is fabricated. The repaired harness uses the
actual host tuple.

All 16 native mode/move corruption guard cases pass at both O0 and O2 with
nonrecovering ASan/UBSan. Test-only wrappers expose internal helpers; they add no
public mutable AST interface. `metadata-results.json` records both runs.

Executed commands after restart were:

```sh
python3 build/backlog/native-compiler/owned-bindings/draft/tests/owned_native_probe.py build/backlog/native-compiler/owned-bindings/stage-a
python3 build/backlog/native-compiler/owned-bindings/draft/tests/owned_native_counters.py build/backlog/native-compiler/owned-bindings/stage-a
```

The source command first stopped at the erroneous borrowed-rebind expectation,
then collected all outcomes after that mapping correction. The counter command
first stopped at transfer cleanup, then collected all eight executions and both
metadata builds after changing the harness to retain mismatches. Logs are kept
separately. Seed build command granted and completed before interruption was:

```sh
target/debug/bhasa build build/backlog/native-compiler/owned-bindings/seed.bh --frontend rust -o build/backlog/native-compiler/owned-bindings/stage-a
```

## Separate owned native correction candidate

Root also routed the independently reproduced match-payload gap. Both Rust
`codegen::match_statement` and native `be_match` acquire buffer/ints payloads
before binding them as owned branch locals. The original native draft then
forcibly cleared transfer eligibility field10. Original seed rejection is
reproduced for both buffer and ints payloads in
`correction/original-payload-reproduction.json`. The correction removes that
forced clear and keeps ordinary type/cleanup/initialized/control-depth checks.
Enum/Result container moves remain excluded and S037/BH221 handling remains.

Three repairs are frozen separately under `owned-bindings/correction`:

- Clean adopted parameters to head -1 before implicit void return, after body
  cleanup has restored the parameter head.
- Diagnose contextual `owned task` with BH222 before unsupported type parsing.
- Retain transfer eligibility for independently acquired buffer/ints payloads.

`correction/emitter-correction.patch` applies to the executed original owned
emitter. `correction/emitter-final.patch` is the complete contextual emitter patch
against the frozen final file-marker baseline. The existing `bhasa_lexer.patch`
remains the BH222 text addition. Patch SHA256 values are respectively
`34e41888f90ebbfef6256b70a2b3b48ba2418832b127ce49b10fd6bd5aa2bbc9`
and `c78b8bfc8f2d3da03b4b28de7a1e6a2b5780512918d2d32bd1401aef180a159d`.
Corrected emitter SHA256 is
`d155c3614da6067942966a9852b22d07168455c1b10a55acadddf00174a73c41`;
corrected seed source SHA256 is
`b7bb009f22845dfb8f1538c6abf8e1a796cb886919f44f404e6dcbfb115756c6`.
`correction/manifest.sha256` freezes source, patches, LLVM and test files.

Corrected seed source passes the existing Rust frontend check, the frozen native
file seed check, and existing Rust LLVM emission. Those commands were:

```sh
target/debug/bhasa check build/backlog/native-compiler/owned-bindings/correction/seed.bh --frontend rust
build/backlog/native-compiler/file-handle/stage-a check build/backlog/native-compiler/owned-bindings/correction/seed.bh
target/debug/bhasa emit-llvm build/backlog/native-compiler/owned-bindings/correction/seed.bh --frontend rust > build/backlog/native-compiler/owned-bindings/correction/seed.ll
```

Correction tests retain the 41 host fixtures and add ten source cases, totaling
51. Added cases cover acquired buffer/ints payload transfer, moved-payload read,
outer conditional transfer, Result buffer payload transfer, forbidden enum
container transfer, and pending Result payload rejection. Their new expected
outcomes come from the ratified correction, not an executed corrected host claim.
A fifth counter guest, `payload_counts`, requires 4 allocations/2 copies/4 drops
and live0 for buffer/ints acquired payload transfers. Correction harnesses retain
O0/O2 and all 16 metadata guards. Python scripts parse successfully.

The granted slot is finished. A next bounded coordinated slot is needed for one
corrected seed build, 51 source probes, ten small counter guest builds/runs and
two metadata guard builds/runs, serially:

```sh
target/debug/bhasa build build/backlog/native-compiler/owned-bindings/correction/seed.bh --frontend rust -o build/backlog/native-compiler/owned-bindings/correction/stage-a
python3 build/backlog/native-compiler/owned-bindings/correction/draft/tests/owned_native_probe.py build/backlog/native-compiler/owned-bindings/correction/stage-a
python3 build/backlog/native-compiler/owned-bindings/correction/draft/tests/owned_native_counters.py build/backlog/native-compiler/owned-bindings/correction/stage-a
```

No corrected seed execution or integrated parity is claimed. Root owns that next
slot, shared integration, public checked-AST validation, hybrid acceptance,
Cargo, git and release/cache work. Existing file and pending SHA256 manifests
both still verify. No shared source, other lane artifact, Cargo, git, cache or
SDK mutation ran.


Final independent host review was read at
`build/owned-bindings-independent-review/review.md`. It reports the same sole P2
payload-owner rejection across 57 source cases, 14 corrupted checked AST cases,
and ten sanitizer executions. Its suggested duplicate payload move, mutable
payload reinitialization and whole-return arm cases were added to the separate
native correction fixture set. That set now has 51 source expectations. These
are authored regressions awaiting the next coordinated corrected seed slot.

Channel handoff was read after the owned slot completed, at
`build/backlog/concurrency-async/source-integration/host-api-handoff.md`.
Native channel implementation remains unassigned. The concrete boundary is the
five-argument `bhasa.fn.bh_compile_channels(source,tokens,errors,target,manifest)`;
existing four-argument entry remains for no-channel units. Manifest v1 header is
[1,nonce,source byte length,family count,21], with whole-declaration anchored
21-word family rows. Manifest RawInts is borrowed immutable storage and cannot
be retained/freed. BH223 is invalid/stale authority and BH224 reachable issued
blocking operations; BH222 remains owned transfer. Native binding must use
issued declaration anchors and validate family schemas/factory shape, never
prefix/name authorization. Scanner codes, primitive tags, ordinary signature
metadata and alias lowering remain unchanged. Root owns additive bridge merging,
manifest lifetime and assignment after corrected host source-authority review.
No channel implementation or registry layout was invented in this lane.


## Corrected owned slot completed

October 12, 2026. Root granted the corrected-owned slot. Built only the separately
frozen correction candidate, then ran all probes serially. Corrected executable
`owned-bindings/correction/stage-a` SHA256 is
`a2b31cce20ed6234b94623c5745672dd15505a142ef20076031617d5b855748e`.
Compiler source and contextual correction/final patches retain the hashes listed
above. `correction/manifest.sha256` still verifies. Original owned executed seed,
file and pending evidence manifests also still verify.

All 51 corrected native source cases pass their expected acceptance and native
diagnostic checks. A serial replay of those exact fixture bytes through the
owner lane's existing corrected host probe agrees on acceptance for all 51.
`correction/host-comparison.json` records full original diagnostics from both
frontends and their executable hashes. The native BH211/BH222 owned-task mismatch
is repaired. Existing S021/BH201 borrowed rebind remains separate. Code families
are intentionally distinct between frontends, including ordinary preexisting
self-declaration/projection diagnostics; no exact byte-span or identical-code
claim follows from agreement on acceptance.

All ten O0/O2 counter guest executions pass with expected stdout and zero live
runtime allocations under nonrecovering address/undefined sanitizers:

| guest | allocations | copies | drops | live |
| --- | --- | --- | --- | --- |
| transfer_counts | 7 | 0 | 7 | 0 |
| early_counts | 3 | 0 | 3 | 0 |
| try_counts | 7 | 4 | 7 | 0 |
| copy_counts | 5 | 3 | 5 | 0 |
| payload_counts | 4 | 2 | 4 | 0 |

Each row passes at both O0 and O2. The four existing counter fixtures exactly
match recorded host outputs/counts. The combined payload guest transfers one
acquired buffer and one acquired ints payload without extra transfer clones;
each requires its existing acquisition clone. This matches the corrected host's
separate 2 allocations/1 acquisition clone/2 drops direct-payload evidence.
Counters cover buffer/ints runtime allocations; these lane probes do not claim
that generated aggregate malloc/free calls were counted. The independent host
review has its own broader aggregate-allocation instrumentation.

All 16 internal metadata guard cases pass at both optimization levels with
nonrecovering sanitizers. Test wrappers remain isolated and add no public AST
interface. Exact run records are `functional-results.json`, `counter-results.json`
and `metadata-results.json`. `execution-evidence.sha256` freezes seed, emitted IR,
guests, metadata guard binaries, logs, patches and result files. Its compiler
inputs are also covered by the unchanged `manifest.sha256`.

Commands executed were the three corrected-slot commands listed above, with
build stdout/stderr saved to `correction/build.log`, source outcomes to
`correction/functional-probe.log`, and counter/guard outcomes to
`correction/counters-probe.log`. Host comparison used the existing binary
`build/backlog/ownership-types/match-payload-correction/probe` against all 51
correction fixture paths, serially, without building or modifying the host lane.
No failures or acceptance mismatches remain in this bounded slot.

The corrected native owned draft is ready for root's combined integration gates.
The slot is stopped. No Cargo, shared-source edit, Git, cache cleaning or SDK
operation ran. Root still owns combined compiler/bridge integration, public AST
validation, hybrid acceptance, refreshed releases and A/B/C verification. Channel
implementation remains unassigned, as instructed.


## Native channel composition audit, implementation unassigned

October 12, 2026. Read actual source-integration host registry producer,
compile_with_channels bridge, checked Operation lookup, helper emission and
approved std source after completing the corrected-owned slot. Root routed the
real driver target rejection to concurrency. This is a composition plan only;
no channel implementation, target policy or frozen owned file was changed.

The actual five-argument bridge constructs a stack RawInts descriptor over the
private NativeManifest Vec, calls synchronously and retains source/receipts.
The vector is [1,nonce,source_bytes,families,21] followed by 21-word rows. Actual
producer ordering matches handoff offsets exactly. Rows anchor whole declarations
for pair/sender/receiver, scalar-or-nominal payload, SendResult/SendFailure,
ChannelSent, ReceiveResult/ReceiveError and factory. Shared ChannelSent and
ReceiveError may repeat. Native IDs are node offsets plus100, not host nominal
IDs, so each row must bind by exact source spans to parsed nodes before dispatch.

Composition sequence when root assigns implementation:

1. Preserve bh_compile's four-argument ABI and extract one common compile path
   used by the added bh_compile_channels entry. Validate manifest shape/length,
   bounded count arithmetic, version/nonce/source length and target policy before
   reading rows. Borrow all incoming descriptors read-only; copy validated facts
   into native-owned metadata. Allocate any new header/table storage separately
   from ctx31..36. Update existing header-floor checks if header size grows;
   keep local fields7..11 and signature pair stride2 unchanged.
2. Bind whole-declaration anchors during existing record/enum/function parsing.
   Current record nodes retain name tokens and body token ranges; signature nodes
   retain parameter pairs/body bounds, but neither is a host whole-span authority
   record. Capture spans in a separate table, preserving node layout. Reject
   duplicate family anchors, wrong declaration roles, endpoint nesting, forbidden
   FileHandle tag11/resource payloads, schema mismatches and altered factory
   signature/body. Registered capacity factories keep Borrowed parameters and
   must pass current owned-mode validation. Names alone never grant dispatch.
3. Route the seven actual Operation kinds Sender/Receiver/Send/TrySend/Recv/
   TryRecv/Close through bound owners/signatures. Close has sender and receiver
   cases. Pair getters return endpoint clones; endpoint clone/drop use channel
   runtime ownership rather than ordinary record field copying. Factory lowering
   builds the native pair via frozen runtime new helper. Pack/unpack callbacks,
   returned send failures and receive payloads must follow actual host helper
   semantics. Issue result enums through existing pending-result/try and acquired
   match-payload handling; do not permit endpoint/container owned moves.
4. Add authenticated endpoint worker-transfer checks and issued blocking edges
   to the existing call graph. Preserve ordinary worker restrictions/BH209 and
   owned-target rejection/BH222. Only bound Send/Recv are blocking; emit BH224 for
   reachability and BH223 for invalid authority. User methods with matching names
   remain ordinary. Recheck bound metadata before helper emission.
5. Request a bounded build/probe slot only after the target correction and actual
   host API authority review pass. Test four/five-argument routes, malformed and
   stale manifests, duplicate/wrong/shared anchors, same-name ordinary methods,
   scalar/nominal payloads, FileHandle denial, worker blocking/nonblocking paths,
   pending results/try, and clone/drop counts. Preserve all owned/file/pending
   fixtures and manifests. Root owns merged bridge diagnostics and runtime/cache
   wiring, combined hybrid/self-build/release gates.

Target gate remains unresolved here. The audited native_manifest and host helper
still list canonical Rust POSIX triples and reject the actual driver spelling
arm64-apple-macosx26.0.0. Native currently accepts target spelling syntactically.
Do not add an independent native allowlist or normalize to a guessed triple.
Consume the separately corrected and reviewed shared policy when supplied.

Authority boundary requires an explicit interpretation in the assigned design.
The five-argument ABI carries borrowed rows and a nonce, without an independent
native issuer handle or source digest. Native can check structure/schema/span
consistency; nonce and byte length alone cannot distinguish forged rows from a
private host-issued vector. Host private constructors/receipt validation and the
trusted synchronous adapter currently provide provenance. If the intended threat
model includes arbitrary untrusted callers of the raw symbol, root must specify
additional issuer authentication before implementation. This lane does not invent
a capability, checksum, cryptographic protocol or alternate manifest layout.

Audited source hashes, before the separate target-policy correction:

- `build/backlog/concurrency-async/source-integration/draft/src/channel_registry.rs` SHA256 `7afcd153343709c11f7dfe8c1714bc6e0c638976bed035fbd41583684ab24cb7`.
- `build/backlog/concurrency-async/source-integration/draft/src/bhasa_lexer.rs` SHA256 `c3aa2aef22ea3dd7a7f5a0b31b2084fff339acfe783c0272a3715f6c7e815312`.
- `build/backlog/concurrency-async/source-integration/draft/src/channel_codegen.rs` SHA256 `fb1fee14e4aba6836d08503cd69c79e39be0f2541fa9526c5e84572fa214b2d3`.
- `build/backlog/concurrency-async/source-integration/draft/src/channels.rs` SHA256 `f1285bc471cc7581d4e45d9f1dcaa51b1e7e68e65bfdb15da50bcb52a783b33e`.
- `build/backlog/native-compiler/owned-bindings/correction/seed.bh` SHA256 `b7bb009f22845dfb8f1538c6abf8e1a796cb886919f44f404e6dcbfb115756c6`.

Frozen corrected-owned manifest, patches and execution evidence remain unchanged.
No Cargo/shared edit/Git or additional native build/probe ran during this audit.


## Isolated native typed-channel draft ready for a focused slot

October 12, 2026. Root assigned actual native parity after independently accepting
`build/channel-source-independent-review/target-correction/review.md`.
Incremental preparation is deferred; reads of host SourceFile, module/generic
expansion, parser Program.merge and native ctx phase boundaries are retained in
conversation context. No incremental cache or speed claim was created.

Root explicitly clarified that the raw C symbol is a trusted synchronous adapter
interface. Private host receipts and exact reviewed std bytes provide authority.
Native nonce/source-length/schema/span checks provide consistency, not
cryptographic authentication against arbitrary malicious raw-symbol callers.
The implementation adds no alternate issuer, checksum or manifest format.

Work is isolated in `build/backlog/native-compiler/channels`, atop the frozen
corrected-owned emitter with FileHandle tag11. `emitter.patch` is contextual to
that exact baseline, and `baseline/bootstrap/compiler/emitter.bh` preserves its
preimage. Four-argument bh_compile is preserved; five-argument
bh_compile_channels shares one compile implementation. Input manifest is borrowed
read-only and copied into native-owned family facts, never retained or freed.
Rows use the exact approved v1/21-word ordering. Whole declaration start/end
anchors bind native IDs, with record/enum/function roles, unique family anchors
and explicitly shared ChannelSent/ReceiveError anchors. Schema and factory body
checks precede function emission and repeat before helper rendering/calls.
Endpoint/pair constructor and field access privileges use only bound family IDs.

Header ctx31..36 and local fields7..11 retain owned semantics. New ctx37/38/39
store family head, operation head and nonce; header floor is40. Standard signature
pairs keep stride2. Move admission now compares exactly mode1, and only mode1
suppresses caller release. Synthetic issued method signatures are kept outside
source function/method emission lists. Ordinary same-name methods remain ordinary.
Registered factory parameters remain Borrowed and pointer ABI remains unchanged.

Native helper rendering uses the existing native flat tagged enum layout, not
host enum IDs/layouts. It acquires payload copies into runtime frames, returns
failure payloads without rereading freed frames, returns receive payloads only on
success, emits endpoint clone/drop/close wrappers and capacity pair construction,
and routes result enums through existing pending/try/acquired payload ownership.
Channel payload checking refuses void/task/FileHandle and nested bound endpoint/
pair types transitively, and validates aggregate identity/cycles/depth. Reachable
issued send/recv adds a distinct blocking graph edge and BH224; foreign/file/pool
restrictions retain BH209. BH223 authority, BH222 owned, BH221 pending and BH220
immutable remain distinct. Target admission mirrors the approved bounded
macOS/Linux component/u32-version policy and preserves exact emitted triples.

Both existing Rust and frozen corrected-native source checks pass for the draft
compiler seed. Existing Rust LLVM emission succeeds. These are candidate-source
checks, not execution of new native channel semantics. No native build ran.
Python verifier scripts parse successfully.

The bounded verifier `channels/draft/tests/verify.py` prepares actual manifests
through the approved host producer using source-integration plus target-correction
sources. It does not invent receipt constructors or manufacture valid authority.
Authored coverage is 13 channel source cases, including all three source handoffs,
ordinary methods, int/ints/buffer/record/enum and multiple families, worker method
blocking and pending/immutable diagnostics. Nine accepted guests require the same
stdout at O0/O2 under explicit LLVM sanitize_address and nonrecovering C ASan/
UBSan. Twenty manifest/metadata refusal probes cover header/nonce/length/width,
anchors/roles, FileHandle tag11, source schema and post-bind family/factory/nonce
corruption. Eighteen target cases include real driver spelling and malformed or
unsupported variants. Old four-argument routing must preserve all51 owned source
cases and eight exact existing O0/O2 buffer/ints transfer/copy/try cleanup oracles.
Tests are authored and await the slot; no successes are claimed for them yet.

Requested focused slot command is:

```sh
python3 build/backlog/native-compiler/channels/draft/tests/verify.py > build/backlog/native-compiler/channels/verification.log 2>&1
```

It compiles one isolated native compiler object from the already emitted seed.ll,
links one sanitizer-enabled C driver, builds one small direct-rustc host exporter,
then runs probes and 26 small guest builds/runs serially. No Cargo, shared compiler
or bridge/runtime source, Git, SDK, cache or new chat operations are involved.
Subprocesses have60-second limits. Original frozen evidence stays unchanged.
Root owns merged source/bridge/runtime installation and integrated acceptance.

Draft seed SHA256 is
`bdffad3f429379aae592b3ede3e2fc1f6a61e4da7ab659028edf34cb9e2bb3c7`;
contextual emitter patch SHA256 is
`442448b2a261aa7014e11506aa4fee5ee5408b83a67b0d70a62bac650e45048b`.
`channels/draft-evidence.sha256` freezes seed/LLVM/patch, implementation helpers,
baseline, fixtures and verification source. Native checker execution parity is
unverified until the focused slot runs. Explicit borrowing remains unassigned.


## Native typed-channel focused slot completed

October 12, 2026. Root granted the exact verifier command. The slot is complete;
no verification subprocess remains running. Shared full-hybrid work may take its
coordinated slot. No shared source/bridge/runtime edit, Cargo, Git, SDK or new chat
ran here. Frozen owned/file/pending manifests all still verify unchanged.

Final native channel contextual patch `channels/emitter.patch` is based only on
the saved corrected-owned/FileHandle baseline, which root has now applied.
Patch SHA256 is
`fa74524c0e6ab05424ff19bc6489d31c6fe0dad7e99e7549f624754c650b4adc`;
final seed source SHA256 is
`981dd2fe457db8bc92255d4a2aff709a1c6db5b31b2d4eb7ad12cdebc42edb97`;
seed LLVM SHA256 is
`ac4b7f85cc5d7a2f034889f2aca28aacbfed83a3761f995923b89b0427485c8d`.
Final isolated compiler object SHA256 is
`23140651a9afece9fcebcebaa30589a574a793d4c84211020988fa15f47517e3`;
C driver SHA256 is
`cef137df1b5d09cf83282d93a70530c1ac38238e363a16740f6f87f5e6596232`;
actual host exporter SHA256 is
`8fb9cdb98668d00ad603521840dd4dc9a4d05a79e26425f7f3e14ce673ccfd06`.

Executed bounded evidence:

- All 13 channel source cases agree with the actual corrected host checker on
  acceptance/rejection and meet expected native diagnostics. These include all
  three source handoffs, same-name ordinary methods, scalar/container nominal
  payloads, multiple families, worker and method-transitive blocking, pending
  results and immutability. `verification/source-results.json` preserves both
  full host and native diagnostics.
- All 18 accepted channel guest runs pass expected stdout/exit at O0/O2. Generated
  LLVM definitions carry explicit sanitize_address; C runtime uses ASan/UBSan,
  nonrecovering checks and frame pointers. The worker/buffer handoff exercises
  source mutation after send, queue full/closed payload return, unread queue drop,
  endpoint cloning, spawn/join, buffer return and typed nested failure matches.
  `verification/guest-results.json` records each executable hash. These are macOS
  executions at actual resolved arm64-apple-macosx26.0.0, not cross-target execution
  or a whole-heap leak claim. macOS leak detection remains disabled.
- All 20 authority/metadata probes pass. Sixteen raw-entry tests reject malformed
  header/length/nonce/rows/anchors, FileHandle tag11 and source endpoint-schema
  mutation with BH223 and empty returned LLVM. Four post-bind internal tests
  deliberately corrupt family ID, factory ID, nonce or factory return type and
  require the repeated family guard to fail before helper emission is called.
  Their C wrapper emits the expected test diagnostic after observing that failure;
  it does not claim public raw-entry exposure of a mutable native AST.
- All 18 target cases pass admission/refusal expectations. Accepted exact resolved
  triples remain unchanged in LLVM. Unsupported/version-overflow cases reject
  BH223; invalid spelling/control bytes may reject existing general BH200 before
  channel-specific admission, matching the existing bridge spelling guard.
- All 51 corrected owned source cases preserve expected acceptance/native codes
  through the old four-argument entry. All 8 existing O0/O2 exact transfer, early
  cleanup, ordinary copy and try buffer/ints counter oracles also pass. Pending
  BH221, immutable BH220 and owned BH222 remain distinct from BH223/BH224.

`verification/summary.json` records totals. `final-draft-evidence.sha256` freezes
final source/patch/fixtures/verifier, while `execution-evidence.sha256` freezes
emitted sources/manifests/boundaries/LLVM, guest binaries, summaries and final log.
`verification-inputs.sha256` identifies the actual host producer and matching
frozen channel/concurrency runtime plus shared core inputs used during the slot.
`draft-evidence.sha256` and `first-failure` remain the original pre-slot snapshot.

First failures are preserved rather than overwritten. The initial raw C asm
symbol names omitted the macOS underscore and failed driver linking. After only
that test adapter correction, basic checking found the draft factory guard used
302, the actual var token, instead of306, the existing return token. That isolated
compiler defect was corrected and required a second compiler-object build within
root's approved repair/rerun instruction. No other compiler semantic change was
needed. A guest runtime link then exposed the verifier's use of shared concurrency
without the separately frozen nonworker guard; it now links the matching reviewed
concurrency draft. An ordinary-method fixture lacked a required field separator;
a blocking-worker fixture nested join inside print instead of using the native
whole-initializer rule. Both fixture corrections preserve existing language rules.
Logs verification.log and verification-second through verification-fifth are
retained, along with dedicated failure copies and exact original hashes.

The final successful command was:

```sh
python3 build/backlog/native-compiler/channels/draft/tests/verify.py --resume-object --reuse-guests > build/backlog/native-compiler/channels/verification-sixth.log 2>&1
```

Resume validates exact saved compiler LLVM/object hashes. Existing guest binaries
are reused only when sanitized emitted LLVM is identical; all are executed and
checked again. Earlier corrections reran only the compiler stage when compiler
source changed, and reused that object for later adapter/fixture-only changes.
The original approved command remains the standalone reproduction path.

Native typed-channel parity is ready for independent review and root combined
integration. Root retains source/private authority issuance, bridge/runtime/cache
merging, public checked-AST gates, fresh hybrid/self-build/release acceptance.
The trusted raw-symbol limitation remains explicit and unchanged. No explicit
borrow implementation or incremental cache was introduced.


## Separate native channel target ceiling, verified

October 12, 2026. Root ratified a 128-byte resolved channel target ceiling after
independent review reproduced excessive splitting/diagnostic allocations and
million-byte zero-version admission in the host. The separate native delta is
`channels/target-bound/emitter-target-bound.patch`, contextual to the completed
channel emitter. It checks len(target)>128 at the channel-required compile entry
before token/target scanning, and at bc_target before splitting into components.
It returns fixed BH223 0..0 without embedding oversized input in diagnostics.
Four-argument no-channel behavior and ordinary target spelling remain unchanged.
Root's host correction is separate; this does not claim its measurements or
verification as native execution evidence.

All 22 native admission tests pass using the isolated sanitizer-enabled compiler
object and driver. They replay all 18 original target cases and add 128/129-byte
boundary cases plus 1 MiB zero-version and malformed targets. A valid exactly 128
byte zero-version spelling accepts and remains exact in emitted LLVM. All three
oversized cases reject BH223 0..0 with no LLVM. The wrapper reads target bytes
from a file to avoid OS argv-size limits. That wrapper's input allocation is not
a measurement of guard allocation. No native performance/speed/allocation count
claim is inferred from these correctness probes.

The ordinary actual-driver target's emitted guest LLVM, after the same test
sanitizer annotations, is byte-identical to the original successful basic guest.
Other old target admission/refusal outcomes are unchanged. Only affected target
probes were rebuilt/rerun; unrelated successful channel guests and owned regressions
retain their original evidence. No shared source, Cargo, Git or runtime change ran.
The original channel final-draft/execution and owned execution manifests still
verify unchanged. The targeted probe process has exited and the slot is complete.

Commands were:

```sh
target/debug/bhasa check build/backlog/native-compiler/channels/target-bound/seed.bh --frontend rust
target/debug/bhasa emit-llvm build/backlog/native-compiler/channels/target-bound/seed.bh --frontend rust > build/backlog/native-compiler/channels/target-bound/seed.ll
python3 build/backlog/native-compiler/channels/target-bound/verify.py > build/backlog/native-compiler/channels/target-bound/verification.log 2>&1
```

`target-bound/results.json` records input lengths/hashes, exact diagnostics and
compiler-object/driver hashes. `target-bound/manifest.sha256` freezes the separate
source, narrow delta, complete emitter patch against corrected-owned, target files,
LLVM, test driver, binaries and results. Apply the narrow target delta after the
original channel patch, or use the complete target-bound emitter-final.patch;
do not apply both. Root retains independent/combined integration and the host
policy correction. Original frozen channel files are not overwritten.


Narrow target delta SHA256 is
`0eed686986c8535b68c3d615e52b8e1a020cb4866def9914b21e91a5adaabd3d`;
complete target-bound emitter patch SHA256 is
`4da31a035e289f3a27992def852b60545f789fa4ae78c6c0f6ec288e4cf64001`;
new seed source SHA256 is
`9f7f6fcf12d102ad755a1b7b897c6130e1864dff4750656448e58042316bb15e`.


## Separate native payload DAG correction, source checks complete

October 12, 2026. Root routed independent P1 evidence in
`build/native-channel-independent-review/dag/results.json`. The original target-
bound driver rechecks repeated nominal children recursively and copies each active
path. Legal authenticated DAG18 took 2.623s in that review; DAG20/24 exceeded its
4s limit. Host accepted them. The reviewer evidence is preserved unchanged and
copied as `channels/payload-memo/independent-baseline-results.json`. These are
reviewer timings, not new lane measurements.

The isolated candidate is `channels/payload-memo`, based on the separately frozen
channel target-bound emitter. `emitter-payload-memo.patch` is a contextual delta;
`emitter-final.patch` includes all channel/target/memo changes against the frozen
corrected-owned/FileHandle baseline. No earlier source, patch or execution manifest
is modified. No new compiler object has been built for this candidate.

Each bc_payload_root call now creates one validation-local table indexed by native
record node. States distinguish missing, active, unvisited and successfully checked
subtree height. Known nominal identity and field bounds are populated once from
the current record chain. A successful nominal node expands at most once per
traversal. Repeated references reuse height only if height fits 128-depth; a bare
positive-membership cache would wrongly erase the remaining depth. Active nodes
still reject cycles. Scalar void/task/FileHandle and bound endpoint/pair exclusions
run before memo reuse. The active-path copying and recursive known-node scans are
removed. A small work counter in that local table lets internal tests assert exact
visits/expansions; it is not stored in ctx or exposed as a product metric.

Every later family/schema/helper check calls a fresh bc_payload_root. No successful
memo survives a mutated context, changed authority, new family or subsequent
emission check. Node IDs, schema/factory rechecks, nonce/anchor checks, BH223/BH224,
mode1 ownership, pending/immutable rules and the128-byte target ceiling are preserved.

Existing Rust source checking and LLVM emission pass. Existing frozen target-bound
native driver also compiles the candidate seed through its four-argument entry.
That check's LLVM/log is retained under payload-memo. Python verifier syntax parses.
These checks validate candidate compiler source, not corrected DAG execution.

Authored bounded probes use one compiler object and two small links:

- Internal DAG4/20/24/100 require exact visits=2*depth+2 and expansions=depth+1.
  This checks linear expansion work rather than relying only on elapsed time.
- Cached height4 is accepted at depth124 and rejected at depth125. A128-nominal
  chain accepts;129 rejects. Active cycles, unknown nodes, FileHandle/task/void,
  issued endpoint exclusion and fresh traversal after field mutation all reject.
- All seven actual authenticated DAG inputs from the reviewer are replayed with
  a4-second subprocess limit and exact source/manifest hashes. New timings will
  be recorded without replacing the original measurements.
- All20 manifest/post-bind authority checks rerun. All nine accepted source handoffs
  must still emit byte-identical LLVM, preserving prior O0/O2 guest evidence.

Requested exact bounded new native-object slot is:

```sh
python3 build/backlog/native-compiler/channels/payload-memo/verify.py > build/backlog/native-compiler/channels/payload-memo/verification.log 2>&1
```

The script builds one isolated O0 ASan-instrumented compiler object from saved
seed.ll, links the existing target-bound C driver and the new internal guard probe,
and runs everything serially. It uses existing approved host-produced DAG inputs;
no new host/compiler authority issuer, Cargo, shared source, Git, SDK, cache or
chat operation is involved. Its general subprocess ceiling is60s and each DAG
execution has the stricter4s limit. Requesting the slot follows source checks and
IR emission as instructed. The candidate's execution is still unverified.

Candidate seed SHA256 is
`03960cb3625a22f987d0a0273dd62a4cf4d5e50551a42d23191eeab949d26315`;
narrow payload-memo patch SHA256 is
`6398db8a2ac8c8ce4218027e3782a5541a98c4a46a34c933a3638f73b21ea708`;
complete contextual patch SHA256 is
`a83556d5ba9ec3bb116b2ec946d408672a5cae206eb5f0b6086cbf6c72f86ba7`.
`payload-memo/draft-evidence.sha256` freezes all candidate inputs/check artifacts.
Root retains independent review, shared integration and coordinated build slots.


## Memo node-bound correction and focused object slot passed

October 12, 2026. Independent source review found that the original memo candidate
computed node+4 before proving node<len(ctx), so a record head/next=i64::MAX would
trap checked arithmetic. The original candidate and its draft manifest are frozen.
The separate correction is `channels/payload-memo/node-bound`. It rejects short
context headers before reading ctx19, then short-circuits node<40, node>=len(ctx),
len(ctx)-node<=4 and the list work ceiling before computing offsets. Negative
list values other than sentinel -1 also reject. Original offending source and
signed-overflow reasoning remain in `original-overflow-evidence.txt`.

The narrow delta is `node-bound/emitter-node-bound.patch`, against the original
memo candidate. Its combined memo delta is `node-bound/emitter-payload-memo.patch`,
against target-bound; `node-bound/emitter-final.patch` includes channels, target
ceiling and corrected memo against corrected-owned/FileHandle. Root should apply
one appropriate composition, never overlapping deltas twice. Original candidate,
target, authority, guest and owned execution manifests remain unchanged.

Both Rust source checking/IR and the existing frozen target-bound native compiler
source check pass. Root then explicitly granted the held one-object slot after
its broad build finished and after the node guard/probes were prepared. Executed:

```sh
python3 build/backlog/native-compiler/channels/payload-memo/node-bound/verify.py > build/backlog/native-compiler/channels/payload-memo/node-bound/verification.log 2>&1
```

One isolated O0 ASan compiler object and two small links were built. All probes
ran serially under nonrecovering sanitizers. The first execution passed; no object
rebuild or repair rerun occurred. The process has exited, so root's next broad
compiler slot may start. No Cargo/shared source/Git/SDK/cache/new chat operation ran.

Internal DAG work is exact: DAG4 has10 visits/5 nominal expansions, DAG20 has42/21,
DAG24 has50/25, and DAG100 has202/101. Each successful nominal subtree expands once
per traversal. Height4 cache reuse accepts depth124 and rejects depth125 without
re-expanding. Depth128 accepts and129 rejects. Cyclic payload fields, unknown
nominal nodes, issued endpoints, task/void/FileHandle and fresh-root validation
after type mutation all reject. Eleven record-list probes cover MAX head/next,
len(ctx) head/next, negative head/next, a short ctx header, a short declaration,
truncated fields, a cyclic list and a node inside the ctx header. They refuse
without checked-arithmetic traps or sanitizer failures. Guard details are in
`node-bound/guard-results.txt`.

All seven actual host-authenticated reviewer DAG source/manifest inputs pass
within the same4s per-process limit. New single-run elapsed times, including
process startup and compiler validation/emission, are:

| DAG depth | seconds | exit |
| --- | --- | --- |
| 4 | 0.260807 | 0 |
| 8 | 0.080107 | 0 |
| 12 | 0.092036 | 0 |
| 16 | 0.096275 | 0 |
| 18 | 0.099388 | 0 |
| 20 | 0.101699 | 0 |
| 24 | 0.111120 | 0 |

These timings are bounded replay observations, not a warmed phase benchmark or a
cross-environment speed ratio. Reviewer baseline timing/timeout results remain
unchanged. New exact source/manifest hashes and emitted LLVM hashes are retained
in `node-bound/results.json`. DAG20/24 no longer exceed the4s correctness gate.

All20 existing raw manifest/post-bind authority checks also pass. All nine
accepted channel source handoffs still emit byte-identical LLVM after the same
sanitizer annotations. Their previous O0/O2 guest evidence remains applicable;
unchanged guest machine programs were not rebuilt merely to repeat that evidence.
Family/schema/factory checks still run, and each payload recheck constructs a fresh
memo. No persistent positive permission cache was introduced.

Final seed source SHA256 is
`90d8cad82e960427e039b0a148787d20f1c61da4f95a93d4b7e8de99988c48c6`;
node guard delta SHA256 is
`ba6d5c1ecaaf4eb5fa33aae7520a340b070b12ca3f5a6ced7d8397fa72909c4e`;
corrected combined memo delta SHA256 is
`9347df43832be3e41ffa0863fdcae4181c9182ded75c5b701fadc2c863b46c4a`;
complete emitter patch SHA256 is
`db06da9bdcb60bc319adbb60f2db8a6c91b5f10644474ca1acbe1743e9d9d774`.
Compiler object SHA256 is
`59f3e6dc69c9b1a2c9bb3b16f42a76bf719d31d0b58377c3393ab3ad908220ac`;
driver SHA256 is
`21ee3809411f2b82dd7af6bf43f68020f2ba683895bce44c31ac926003fb07f7`;
guard binary SHA256 is
`8884315ba5c469b2819e3c890ab878d264d960b13ed74e3b6436812285bf687c`.
`node-bound/draft-evidence.sha256` preserves pre-slot inputs/checks;
`node-bound/execution-evidence.sha256` freezes successful object, links, results,
DAG LLVM, source checks, correction patches and logs. Original memo draft,
target-bound and earlier channel/owned manifests all still verify.

Corrected DAG/depth/resource/cycle/mutation evidence is ready for independent
replay. Shared integration remains with root after that review. The trusted raw
manifest interface limitation,128-byte target ceiling and owned/file/result/task
contracts remain unchanged.


## Separate family/schema range correction prepared, object slot held

October 12, 2026. Root routed the independent corrected-node-bound replay: seven
DAGs, memo/list guards, metadata checks, additional refusals and byte-identical
handoffs pass. That successful source/object/results snapshot remains frozen.
The reviewer separately extracted bc_family checked-addition evidence from its
LLVM, showing n+10 can trap at i64::MAX before range comparison. Its JSON explicitly
claims no runtime execution for that extraction. The original JSON is copied to
`channels/payload-memo/family-bound/original-family-guard.json`, with original
bc_family/bc_known_record/bc_shape/factory guard source and arithmetic reasoning
in `original-guards.txt`. No failing machine execution is invented here.

New work is separate under `channels/payload-memo/family-bound`, contextual to
successful node-bound. Family lookup rejects short headers, then proves
40<=n<len(ctx) and len(ctx)-n>10 before node offsets. Known-record lookup uses the
same range-first ordering with remaining header size4, and compares n==ty-100
instead of n+100. Shape's own node and factory range guards likewise subtract only
after valid range proof. Record/family lookup scans have bounded steps and reject
malformed negative links, duplicates/cycles and truncated nodes. Valid lookup
results stay unchanged. Invalid family metadata returns internal sentinel -2,
separate from valid no-match -1. Payload validation refuses invalid family tables
instead of treating them as absence of an issued endpoint; each root also checks
family-table consistency for scalar payloads. This adds no source diagnostic,
ABI field, persistent cache or authority issuer. Normal shape, depth, File/task,
mode1 and pending/immutable rules are preserved.

Rust source checks and LLVM emission pass. The successful frozen node-bound native
driver also compiles the candidate seed through four-argument routing. Candidate
scripts parse. No new compiler object or guard binary was built; the slot remains
held while root's broad reference work and independent source review run.

Authored tests retain all prior DAG/depth/mutation/resource and11 record-list
cases, extend those malformed records to bc_known_record, and add9 family-list
cases plus4 direct schema-range cases. Family head/next probes use MAX and len(ctx),
negative links, short/truncated tables and cycles, requiring invalid-table -2 and
clean payload/schema refusal. Schema probes use MAX, len(ctx), a negative index
and truncated node. The actual authenticated DAG/20 metadata/nine handoff replay
remains in the verifier. These new guards are not yet claimed as executed.

Exact candidate command for a future explicit one-object slot is:

```sh
python3 build/backlog/native-compiler/channels/payload-memo/family-bound/verify.py > build/backlog/native-compiler/channels/payload-memo/family-bound/verification.log 2>&1
```

It keeps the previous one compiler object/two links/serial bounded probe scope.
Do not run until independent source review and root explicitly approve that slot.
Elapsed time is not approval. No shared/Cargo/Git/source wiring ran here.

Narrow delta `family-bound/emitter-family-bound.patch` SHA256 is
`3a5a93d9e394bc844a912b1dba75e4e929550c5e779009cb684d4ebc202d462e`.
Combined memo/range delta against target-bound SHA256 is
`46f7a31132f716690fff9c7c65bc6b1cec0d447eccb737cd2a0b712ebcaafa18`.
Complete channel emitter patch against corrected-owned SHA256 is
`5837f1f83b0f4644debaa09d34532f795c3fc26d43a60d8e372ab6a72413e8a1`.
Candidate seed SHA256 is
`c74f8c19bf36ec9f29677177162ead8fbce5103b9fa26032e5657626abcdc689`.
`family-bound/draft-evidence.sha256` freezes source, pre-slot checks, tests and
original reviewer/source-math evidence. Node-bound execution, target-bound and
original channel execution manifests still verify unchanged. Root retains shared
integration and the next slot decision.


## Family/schema guard object slot passed

October 12, 2026. Root's reference649/full-hybrid681 gates finished, and independent
source review accepted the separate family-bound guards before root explicitly
granted this slot. Executed exactly:

```sh
python3 build/backlog/native-compiler/channels/payload-memo/family-bound/verify.py > build/backlog/native-compiler/channels/payload-memo/family-bound/verification.log 2>&1
```

One isolated O0 ASan compiler object and two small links were built, followed by
serial bounded probes under nonrecovering checks. The first execution passed.
There was no rebuild/retry. The process has exited; root's release/clippy slot
may start. No shared source, Cargo, Git, SDK or cache work ran in this lane.

All depth/cycle/mutation/resource guards still pass. Exact internal work remains
DAG4 10 visits/5 expansions, DAG20 42/21, DAG24 50/25 and DAG100 202/101. All eleven
record-list probes now pass both payload validation and known-record refusal.
All nine family-list probes pass invalid-table -2 plus clean nominal/scalar
payload and schema refusal. All four direct schema-range probes pass. MAX/end/
negative/truncated/cyclic contexts refuse without arithmetic traps or sanitizer
failures. Valid absent-family -1 behavior and issued endpoint exclusion remain.
`family-bound/guard-results.txt` preserves exact output.

All seven original authenticated DAG inputs pass within4s each. Observed elapsed
process times, including startup/checking/emission, are:

| DAG depth | seconds | exit |
| --- | --- | --- |
| 4 | 0.259321 | 0 |
| 8 | 0.240700 | 0 |
| 12 | 0.088770 | 0 |
| 16 | 0.100338 | 0 |
| 18 | 0.102145 | 0 |
| 20 | 0.110137 | 0 |
| 24 | 0.121476 | 0 |

These are single replay measurements, not a warmed benchmark or phase timing.
All20 raw/post-bind authority checks pass again. All nine accepted channel source
handoffs emit byte-identical LLVM under the same sanitizer annotations, so their
existing guest evidence remains unchanged. No opaque or persistent positive cache
was added. Height/depth validation and authority rechecks remain separate.

Final source/correction hashes remain the pre-slot family-bound hashes listed
above. Compiler object SHA256 is
`f52e0abe4c68bab49868c918cd0544d306c720744d652f99a070be861faf5c44`;
driver SHA256 is
`bc97901d46a19b10b5502ba03313df6e183d9ce8d61b605e661e40f2b3bff544`;
guard binary SHA256 is
`9aebe7cf9e35c613243c6838115f30ab8e001b07221856f9e7bc74a6a02c687d`.
`family-bound/results.json` contains exact authenticated input, LLVM, object and
binary hashes. `family-bound/execution-evidence.sha256` freezes the successful
slot; `verification-inputs.sha256` identifies its core/runtime adapter inputs.
The pre-slot draft manifest, successful node-bound, original memo, target-bound,
original channel and owned execution manifests all verify unchanged.

Corrected family/schema refusal evidence is ready for independent executable
replay. Shared channel integration remains with root after that review, using the
complete family-bound emitter-final.patch or the appropriate non-overlapping
contextual deltas. Trusted raw adapter limitations and all normal ownership,
FileHandle, worker, pending, target and source diagnostic contracts remain intact.

## Explicit readonly/mutable source preparation after family-bound

October 12, 2026. Resumed the assigned native lane and prepared the ratified
explicit buffer/ints borrow source delta under `build/backlog/native-compiler/
explicit-borrows/`, atop the frozen corrected family-bound channel emitter.
No new compiler object/link slot was used. This is source preparation only.

Modes retain Borrowed0/Owned1/Readonly2/Mutable3 and ctx31 six-word records.
Only exact Owned grants moves/adoption/cleanup. Context31–39 and twelve-word
locals retain their meaning. New context40–43 hold data metadata and call frames;
all node/header floors are44. Separate access/provenance metadata tracks acquired
storage, potentially aliasing legacy parameters and shared parameters. ctx36
loans now carry root, provenance, type, kind, frame and original argument token.
Complete source headers and full mode tables are independently rechecked,
including zero-argument declarations and count truncation/MAX corruption.

Builtin effects follow the accepted host positional API plus FileHandle effects.
Argument staging retains candidate loans for nested evaluation, but complete
ordinary signatures and audited positions must validate before frame replay
grants successful emission. Readonly capability widening and explicit overlap
use BH225; existing BH222/BH221/BH220/BH201/BH223/BH224 remain distinct. Copies,
returns, moved storage identities and borrowed temporary cleanup retain their
respective acquisition/transfer behavior in the prepared source.

The existing corrected-owned stage-a checks the prepared seed and emits LLVM
successfully. C harnesses pass syntax checking; the contextual patch applies with
zero fuzz and reproduces exact draft bytes. Nine prior frozen manifests verify
unchanged. All58 concrete fixtures agree when replayed with the frozen host
borrow binary. Native execution remains unrun. Prepared internal tests cover16
loan matrix combinations, provenance/type/frame restoration,9 loan corruption
cases and12 mode corruption cases. Seven counter guests carry14 O0/O2 expected
results, without claiming new runtime evidence.

Patch SHA256: `0d1e237c21435610f87fa9631d9633748acbdbb192a0c5a24b76f8858c329598`.
Seed SHA256: `aa8552c50c481229b36e2c869fd0378dd021ed40a599b719d391dc33d5e577b8`.
Emitter SHA256: `a9bb541f8fa76692db30d7c995b0ff7dc703e3eeceb8b3ec48aa6e1c7c4cb286`.
Draft manifest SHA256: `744f1f1ce5a39fd0eea9323f81211a3531ff70a8999e947c5b7414ff46ae6f1d`.
`README.md`, `source-preparation-results.json`, `inputs.sha256`, `cases.json` and
`counters.json` state exact scope, inputs, checks and open gates.

Root owns independent source review and slot sequencing, then actual source/guard
execution, O0/O2 counters and frozen ownership/pending/task/FileHandle/channel
regression replay. Imported aliases/spans and unused generic rejection need the
real host loader composition. Readonly method escapes still reject as in the
approved host pass; authenticated channel effect adapters alone cannot grant a
readonly method exemption. Header growth changes native nominal node IDs, so
prior channel LLVM byte identity is not claimed for this new delta.
No shared source, other lane artifact, Cargo/Git/SDK/cache operation was changed.

## Separate contextual-name and extern-header borrow correction

Independent source review found two classification errors before a native slot.
The original stacked-mode guard rejected valid contextual parameter names in
readonly mutable:buffer, mutable readonly:buffer and owned readonly:buffer.
The extern schema skipper recognized only owned, causing valid readonly/mutable
extern headers to fail BH200 before their intended unsupported-mode BH225 check.

`build/backlog/native-compiler/explicit-borrows/contextual-names/` preserves a
separate two-condition correction. True stacked modes require a third identifier
and keep BH222/BH225 based on their first mode. Extern skipping now uses the same
modifier/name/colon recognizer; malformed extern colons/types still take BH200.
No provenance, loan, metadata, cleanup, ABI or projection implementation changed.
The requested fresh-record projection fixture rejects with valid struct syntax;
be_meta already clears its inherited Call status.

The corrected manifest has76 source fixtures: original58 plus18 new cases.
All18 extra frozen-host replays agree, including three accepted contextual names,
nine stacked pairs, two unsupported externs, two malformed externs, ordinary
contextual names and fresh projection. Native fixture execution remains unrun.
The existing stage-a checks and emits the corrected seed. C harness syntax and
zero-fuzz exact patch application pass. All84 original frozen files and the
independent reviewer's original pins verify unchanged. No compiler object/link or
runtime execution was started. Initial contextual-only outputs are retained
separately inside the correction directory.

Narrow correction SHA256: `f96723aefac8bbe600b7532089a23a58876de1663076d87d01621dbe35242bfc`.
Complete corrected borrow patch against family-bound SHA256: `4b1d666a4704091e8d526431b8af0020ab4e54ad68735268d89ff6b8befa9535`.
Corrected emitter SHA256: `f8c5aa5f9121cf7bbf3aa1a9795109b68fd19c0396e1138c585ba33c8e0e8bef`.
Corrected seed SHA256: `f0cfbb9910041fa8427c02fadc9997582133f8dd2aecc7a535d1d24e6494bc04`.
Source-only seeded LLVM SHA256: `b99dfbdec26debc650d43860d06c6d4525ab4de6fe7fe215a05256e3f62f9f4f`.
Correction evidence manifest SHA256: `2a61811da3861243fdfc8dc4e6f76ea6b51584bf76dc78df5abca132424afafe`.
Awaiting independent source rereview and root's native slot sequencing.

## Authorized native explicit-borrow source slot completed

After independent contextual/extern source rereview, root authorized exactly
`contextual-names/run_slot.py --coordinated-slot`. Its first execution passes.
One O0 sanitizer compiler object and two links complete; internal guards pass
16 loan matrix combinations, provenance/type/frame restoration,9 malformed
loan/metadata cases and12 mode corruption cases. All76 serial source fixtures
match frozen-host acceptance: 30 accepted and 46 rejected. Prepared BH
diagnostics match, including contextual names, genuinely stacked modes, explicit
extern BH225, malformed extern BH200 and fresh projection BH225.

The first output is preserved in `contextual-names/verification.log`; exact
source outcomes, diagnostics and linked identities are under `verification/`.
`execution-summary.json` records scope and `execution-evidence.sha256` freezes
output bytes. Execution manifest SHA256: `8ee775cd2cd9cc41315589b13b18c6b933a2cbd15e66563620468b4f0164f92c`.
Both original and corrected draft manifests verify unchanged. ASan marks
generated compiler LLVM, C uses ASan/UBSan and leak detection is disabled. This
slot proves source checking/emission and the internal guards; no counter guest,
channel replay, broad suite, Cargo/shared source/Git/SDK/cache operation ran.
The runner has exited with status0 and no native-slot process remains. Root may
resume held compiler/guest work. Native O0/O2 borrow counters and composed loader/
channel/owned/pending/task/FileHandle regression gates remain separately assigned.

## Result-parameter BH221 source preparation after explicit borrows

Prepared a separate native Result-parameter delta under
`build/backlog/native-compiler/result-parameters/`, atop corrected borrow emitter
f8c5aa5f9121cf7bbf3aa1a9795109b68fd19c0396e1138c585ba33c8e0e8bef.
The first complete prior checker/emitter is unchanged in behavior. Only after
all bodies, old local obligations, loan/mode checks, worker graphs, generated
helpers, authority and external symbol validation succeed does a private-context
parameter traversal run. Only source bodies with direct concrete Result-shaped
parameters replay; generated wrappers, aggregate-only and unused templates remain
excluded. New obligations use BH221 and do not seed locals. Copies/calls/methods
do not discharge; direct exhaustive match/return/whole initializer try do.

Context0..43 retains meaning, new44 selects traversal and header/node floors are45.
The owned context copy isolates pending state and the retained first module.
No signature/local/mode/loan stride or borrowed cleanup change was introduced.
Original concrete IDs/spans are resolved from each source compilation; aliases/
generics/import remapping stay with root's actual loader. Root must keep the
approved host delta's native privacy preflight prior-only.

All23 approved host contract fixtures agree. Five host alias/import/privacy/span
probes agree, including UTF-8+CRLF library line4/column1.29 concrete native
acceptance/refusal/precedence fixtures and internal phase/copy-isolation guards
are prepared, not executed against the new implementation. Existing corrected-
borrow native binary establishes prior codes and exact spans; future checks
compare complete prior diagnostic text. Two native test migrations handle
observe, replace the old exemption and match nested cleanup's fallback input
while retaining inner tries. Extracted migrated cleanup program passes host
checking; immutable native forward/count already directly handle inputs.

The existing stage-a checks and emits the new seed; C/Python syntax checks and
zero-fuzz emitter/migration patch replay pass. Five prior native source/execution
manifests verify unchanged. Original source-preparation anchor and fixture
mistakes are preserved and labeled. No compiler object/link/new runtime, Cargo,
shared source, Git, SDK or cache change ran.

Emitter delta SHA256: `339a740c7a0928f222e63bbba5c8810dfa1457b6fd3e396395338e154d28eb31`.
Fixture migration patch SHA256: `aa05c4895caa14db517224b774aedea20fb279feb2334948fc0777a051412201`.
Prepared emitter SHA256: `d65519cd370d355863c5472fbb6d4d704f4702483a51cc019d3c9c5cc9a98b2e`.
Seed SHA256: `3211f4771cf6ff115e7fa6425ace33f3b044ec585a2bbfbdb31d769efaa4b7e7`.
Source-only seed LLVM SHA256: `4dbf02c61f9738ce791769054e80db0aa0bcb6ebe5cf0d44e60263ceed61bc71`.
Draft manifest SHA256: `e0b2edc21a9490ae2a7b19b2763a759d19b670f65a9e6c5a6a4ddbefb7c038cf`.
Independent source review and a separately authorized exact native slot remain
root-sequenced gates. Runtime, counters, channel and full composition are open.

## Separate Result-parameter runner refusal-gate correction

Independent synthetic review identified an acceptance-runner defect: a new
negative could accept BH221 followed by sanitizer stderr or a signal exit.
Compiler semantics and fixtures are unchanged. `result-parameters/runner-correction/`
adopts the exact reviewed patch against original runner d867f4667fc16556622244db6e039d579c793692e3f650a6518e53a5d4e329a8,
with a separately routed execution output directory. Original runner, probes,
sources, fixtures and draft evidence remain frozen.

Refusals now require status1, empty stdout, complete numeric-only BH lines, exact
code vector, no IR and unchanged prior full diagnostics. Success requires status0,
clean stdout/stderr and nonempty regular IR. Guard exit/stderr checks, first
source-result recording, stale-artifact rejection and original timeouts remain.
22 synthetic object/AST-only scenarios pass without subprocess/native execution.
Original 89 frozen draft files and independent reviewer pins verify unchanged.

Separate corrected runner SHA256: `fa1617b0816a8c289137f738d2980e6877ec3eb1438ac52b55eb8ae9c4039655`.
Exact reviewed predicate runner SHA256: `20924913268ee4e6dc3515b8d84928b96aedb142ec2ddad87c8557496fc4ab21`.
Reviewed patch SHA256: `a15fb16798ab5ce89ac36181478bec012217ae0bc347fc45d84d7347f6303d09`.
Complete correction including output routing SHA256: `13fb4afae0e217012392fcf53cfcb7840df7e3644e4238dc7750c4894b5d2ce7`.
Correction evidence SHA256: `079e1fc35b9c9ca72746ee61f7f610cb56a17cf084e75702e9b5fbbb7a0b5df2`.
No Cargo/Clang/compiler-object/link/runtime/shared-source operation ran. Await
independent emitter review, completion of root's channel gates and an exact
root-authorized slot for the separately corrected runner.
