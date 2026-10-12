# GPU and platforms backlog lane

Updated October 12, 2026. Three bounded increments are implemented and locally verified as
isolated drafts. It is ready for coordinator review, not integrated acceptance.
Shared compiler/runtime sources were not edited. No Cargo, broad suites, SDK
installation, cache cleanup or Git mutation ran in this lane.

## Independent adapter review and test-only timing packet

Independent adapter review is complete at
`build/toolchain-process-independent-review/review.md`, SHA256
`fbfa0cb6f5c3e336e510c257b0722ca12fdeabf3c14838a25b57b3c61da795b1`.
Thirteen independent synthetic calls pass. No production correctness defect was
reproduced within documented OS progress, owned wait and in-group assumptions.
Configured tool/CLI acceptance remains root-owned; Linux execution is unverified.

The original copied spaced/metacharacter executable fixture reproducibly timed
out at its 200 ms budget during independent full and exact replay. Those red logs
are preserved alongside the original author passing log. This is a fixture timing
finding, not a change to adapter deadline semantics. The precise loader/kernel
cause was not measured.

A separate source-only packet is ready at
`build/backlog/gpu-platforms/toolchain-process-fixture-correction/`.
`patches/fixture-correction.patch` is the exact independent two-line patch, SHA256
`55c5bc966d55e1bbda704b3b5c7450eaa66b070f636822af758a56ff68f29a2f`.
Only the copied executable test requests the already admitted 2000 ms budget.
Other 200 ms deadline/flood cases and all production timeout/cleanup logic remain
unchanged. The exact corrected harness and dependency copies are under draft/;
old test source is in baseline/tests. Production adapter hash remains
`8bf5fb3758d7a86ee7c1e797a18720076d9fa33dde5b3a1afa29908d3cd84ecc`.

Reviewer corrected author replay passes all six parent groups. Exact reviewer
commands, build/output receipts, original reds, corrected-six output and independent
thirteen-call output are copied to evidence/ with reviewer provenance. No new
executable replay ran in this source packaging turn. `evidence/manifest.json`
distinguishes the review outcomes from packaging checks; source-sha256.txt pins
all copied source files. All 101 original artifacts are verified unchanged in
original-preservation.json. Old module/tests/proofs were not overwritten.

`draft/docs/process-fixture-correction.md` records the narrow change and review
limits. Ordinary OS spawn/SIGKILL/wait progress is still an assumption, not an
absolute kernel-stall bound. Escaped descendants and orphan/subreaper policy are
still outside the proved adapter scope. No configured tool, Cargo, native object,
SDK/network, cache, shared source, Git, publication or new chat occurred. Root
retains production module/CLI/configured-tool wiring after this review handoff.

## Synthetic bounded process adapter handoff

The next separately isolated module is implemented in
`build/backlog/gpu-platforms/toolchain-process-adapter/`. It consumes the frozen
corrected Probe interface and leaves all previous modules, parser proofs, canonical
target policy and original 28 artifact identities unchanged.

`draft/src/toolchain_probe.rs` adds ProcessProbe for macOS/Linux using direct
unsplit OsString/argv, closed null stdin, separate captured pipes and per-stream
byte caps. It validates budgets/config/argument bounds before spawn, reserves
bounded storage fallibly, makes pipes nonblocking and drains one fixed chunk per
stream per pass. It starts no reader/watchdog/reaper thread. Missing executable,
nonzero/signal exit, invalid request, read/setup failure, output overflow and
deadline are explicit statuses; unsupported hosts return SpawnFailed/unknown.

Each child starts its own process group. Failure/deadline signals that group,
kills/waits the direct child and drops both owned pipes. Reaping is delayed while
captured writers remain, pinning the group PID during failure cleanup. Early child
exit with retained descendant pipes becomes a deadline failure. The adapter does
not wait forever for pipe EOF after deadline and does not detach reader threads.

Six serial synthetic parent test groups pass with edition 2021, warnings denied
and one test thread on ARM64 macOS. One ignored fixture is explicitly run as each
synthetic child; fifteen baseline JSON/target tests are filtered. Tests cover
closed stdin, both streams, early/nonzero exit, stdout/stderr/simultaneous flooding,
zero stderr quota, sleep timeout, retained descendant writers, missing/invalid
requests and an executable path containing spaces and shell metacharacters.
A retained writer's scheduled post-deadline marker write is prevented by group
cleanup. Each synthetic probe is checked to finish within two seconds locally.
Probe groups run one at a time; the retained-writer fixture creates its necessary
single descendant in that same group. No configured tool/Clang probe ran.

Exact argv/exit codes are in `evidence/commands.json`; complete compiler/test
output is in build.log and tests.log. `evidence/source-sha256.txt` hashes the
compiled adapter/test/interface/dependency inputs. `evidence/manifest.json`
records root HEAD, source hashes, tested platform and cleanup assumptions.
`patches/adapter-additions.patch` adds only the separate adapter and contract doc,
without driver/CLI wiring. Tests remain an isolated review harness.

`draft/docs/toolchain-process-adapter.md` records material review gaps. Process
spawn and SIGKILL/wait progress are synchronous OS assumptions, not a proved hard
wall-clock bound for stalled kernels/uninterruptible work. Escaping setsid/setpgid
descendants are outside group cleanup. Descendants without captured writers are
not inventoried on successful exit. Direct-child reaping is explicit; orphan
reaping belongs to OS parent/subreaper policy. The host must own its child wait
without a competing reaper. Linux cfg implementation is authored but not executed;
Windows/other hosts have no production implementation and return unknown. The
coordinator must review these assumptions before actual configured tool acceptance.

Preservation receipts verify all 28 original artifact pins and every previous
corrected source hash unchanged in original-preservation.json and
correction-preservation.json. No dependency, target normalization, shared source,
Cargo, native object/linker build, SDK/engine/network, cache, Git, publication or
new chat occurred. No-GC production engine and actual platform execution remain
separate gates. Root retains `bhasa targets --inspect --json` parsing, real
configured Clang acceptance and integrated module/CLI tests.

Exact authorized synthetic reproduction:

```sh
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/toolchain-process-adapter/draft/tests/process_probe_contract.rs -o build/backlog/gpu-platforms/toolchain-process-adapter/evidence/process-probe-tests
build/backlog/gpu-platforms/toolchain-process-adapter/evidence/process-probe-tests --test-threads=1 --skip json:: --skip target::
```

## Independent public-parser findings corrected separately

Both independent red probes are corrected in
`build/backlog/gpu-platforms/toolchain-parser-correction/`. This is a contextual
source delta against the original capability module; toolchain-v1, the previous
target-boundary draft and root-owned canonical target policy remain unchanged.
The original red source/output are copied verbatim into
`evidence/original-red-probes.rs` and `.log`.

The first defect accepted control-bearing registry descriptions as successful
backend evidence. The correction validates description controls before any whole-
line trimming can hide trailing controls. NUL, ESC, trailing tab, DEL and C1
control cases now produce Unknown(malformed_backend_list). Ordinary indented
registered-target output and CRLF descriptions retain their prior behavior.

The second defect was in the direct public parse_resolved_triple API, not an
inspect stdout-cap bypass. Collecting all matching lines amplified allocation
before rejecting duplicates. First/second iterator checks now reject duplicates
without collecting the input. The original 1,400,000-byte duplicate input had
largest allocation 2,097,152 bytes and total 4,194,240 bytes. The corrected public
parser test records largest=0 and total=0 during rejection. Exact valid triples
and existing error-code identity remain unchanged; the metadata ceiling stays
256 bytes and canonical runtime ceiling stays 128 bytes.

Authorized standalone rustc tests run serially with edition 2021, warnings denied
and one test thread. Four public-parser groups pass, including the two original
failures and normal/valid compatibility. Eight original injected cases pass against
the corrected copy. Four canonical capability-boundary groups also pass against
the corrected copy and the unchanged actual root target-128 snapshot. Fifteen
baseline JSON/target cases are filtered out in each focused run. Exact argv/exit
codes are in `evidence/commands.json`; complete outputs are
public-boundary-tests.log, original-eight-tests.log and target-boundary-tests.log.
The public allocation results are printed in public-boundary-tests.log.

`patches/parser-correction.patch` contains only the narrow contextual
src/toolchain_inspect.rs change. `evidence/source-sha256.txt` hashes every compiled
source/dependency; `evidence/manifest.json` records before/after source hashes,
root HEAD, test counts and scope. Tests remain standalone review harnesses until
root wires integrated module tests. No shared source or target parser was edited.

All 28 original artifact pins from the independent review are verified unchanged
in `evidence/original-preservation.json`. No previous module, output, executable,
baseline or test identity was overwritten. New corrected outcomes remain separate
from original failures and prior passing evidence. rustfmt checks pass.

No configured tool probe, Cargo, native object build, SDK/network, cache, Git or
publication ran. Production CLI/process adapter, actual configured tool behavior,
SDK/linker/hardware execution and no-GC engine acceptance remain coordinator gates.

## Canonical 128-byte capability boundary delta

Root's formatted frozen src/target.rs now contains the ratified early 128-byte
ceiling. Inspection confirms that length rejection returns one fixed bounded
diagnostic before split or input formatting. Accepted short and deployment triples
remain byte-preserved. Shared target code was read only.

A separate contextual delta is prepared at
`build/backlog/gpu-platforms/toolchain-target-boundary/`. It snapshots actual frozen
src/target.rs and the exact JSON/string helper dependencies. It imports the
unchanged original capability module, rather than copying any target parser.
The original eight injected tests, their approved-slot output and all executed
source hashes remain intact and verified unchanged.

`draft/tests/capability_target_boundary.rs` authors four narrow injected cases:
127/128-byte exact admission; 129/256-byte observed metadata with canonical runtime
refusal and fixed diagnostic; 257-byte lexical metadata refusal versus 1MiB output
cap refusal; exact deployment triples and byte-counted Unicode limit. Backend
registration remains independent of runtime admission. Oversized tool output
produces unknown instead of pretending hardware absence.

The four new cases have not been compiled or executed. rustfmt passes and
`evidence/baseline.json` records exact source/test hashes and root HEAD.
`evidence/source-checks.log` records inspection only. No configured tool subprocess,
new native build, Cargo, shared source edit, SDK/network, cache or Git operation ran.
The original standalone target_ceiling_refusal.rs remains unchanged as earlier
unrun evidence; this new delta supplies the actual corrected dependency snapshot.

Future coordinator-owned narrow test command, not run in this turn:

```sh
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/toolchain-target-boundary/draft/tests/capability_target_boundary.rs -o build/backlog/gpu-platforms/toolchain-target-boundary/evidence/boundary-tests
build/backlog/gpu-platforms/toolchain-target-boundary/evidence/boundary-tests --test-threads=1 --skip json:: --skip target::
```

This is a test-only contextual handoff. No capability-module patch or CLI/production
adapter behavior changed. Actual configured tool/SDK/linker/runtime/hardware/no-GC
engine evidence remains for root's separate integration and execution gates.

## Capability inspection approved-slot result

The coordinator approved one standalone rustc slot for the eight injected tests.
All eight pass with edition 2021, warnings denied and one test thread. Fifteen
baseline JSON/target tests were filtered out as requested. No configured Clang/tool
subprocess, Cargo, shared source, SDK/network, cache or Git operation ran.

Exact argv, exit codes and output file names are preserved in
`toolchain-v1/evidence/approved-slot-commands.json`. Successful build output is
`approved-slot-build.log`; complete unchanged test output is
`approved-slot-tests.log`. `approved-slot-executed-source-sha256.txt` records the
executed production module, test harness and JSON/target/diagnostic dependencies.
The original submitted draft hashes are retained in
`approved-slot-original-draft-sha256.txt`, and the pre-slot source record is
`source-checks-pre-slot.log`.

Initial compilation exposed a missing standalone harness dependency: existing
JSON calls diagnostic::json_string. The repaired harness imports an exact source
excerpt in baseline/src/diagnostic-json.rs; the original diagnostic source hash is
recorded separately. The capability production module is byte-identical to its
original submitted hash. No product parser, runtime policy or JSON implementation
was changed to pass the tests. The initial build failure is retained in
approved-slot-initial-build.log. Final draft hashes, baseline manifest and doc-only
patch are refreshed to reflect the harness repair and verified evidence.

The coordinator separately ratified a 128-byte ceiling in shared
require_native_posix64. The capability metadata parser keeps its independent
256-byte bound; reporting bounded metadata does not admit it to runtime policy.
`draft/tests/target_ceiling_refusal.rs` separately authors refusals for an overlong
zero-version triple and a 1MiB malformed triple. These two tests have not run and
require the root-owned corrected canonical dependency snapshot. They were not
included in, and do not alter, the approved eight-test evidence. This lane neither
copies a target parser nor changes its canonical snapshot to invent the correction.

Real process adapter execution remains a separate coordinator review gate. This
handoff verifies injected parser/request/JSON contracts, not installed tool/SDK/
linker/hardware availability or no-GC engine acceptance.

## Target/toolchain inspection implementation handoff

The approved bounded assignment is implemented as an isolated draft in
`build/backlog/gpu-platforms/toolchain-v1/`, preserving earlier Wasm/GPU drafts.
Runtime/test execution awaits the coordinator's build slot. No native build,
Cargo, shared source edit, Git mutation, network, SDK/engine acquisition, cache
operation or publication ran in this increment.

`draft/src/toolchain_inspect.rs` implements the proposed host API, pure version/
backend/LLVM metadata parsers, typed observation states, bounded configuration,
three injected read-only probe requests and deterministic schema-v1 JSON using
existing crate::json. It does not execute processes. Failed/timed-out/overflow/
malformed probes yield unknown reason codes, never negative hardware proof.
Successful backend registry absence yields observed false for the resolved triple.
Configured executable names remain one unsplit OsString; empty or above 4096
encoded bytes reject before probing. Tool stderr is not echoed into the report.

Runtime admission calls the approved concurrency correction's
`crate::target::require_native_posix64` verbatim. Reported deployment triples are
preserved. There is no second POSIX normalization implementation. The module
requires that correction before integration; existing target CLI aliases and
run guard remain untouched. Runtime scope is explicitly POSIX64 channels source
admission, with complete runtime support unverified. SDK, object emission, linker
and target execution, and production no-GC engine review remain separate gates.

`draft/docs/toolchain-inspection.md` publishes the API/CLI/JSON/probe contract and
production adapter requirements. Proposed CLI remains
`bhasa targets --inspect --json [--target ALIAS]`. Root owns CLI parsing, the
real bounded process implementation and tracked driver wiring. Version/registry/
empty-stdin LLVM metadata requests create no output artifact. Production adapter
must drain both pipes, enforce deadlines/caps, kill/reap children and bound cleanup;
the injected trait cannot itself prove elapsed time or enforce process deadlines.

`draft/tests/toolchain_inspect_contract.rs` authors eight focused injected tests.
They cover actual-form macOS deployment triples and canonical runtime policy,
explicit target argv, configured executable spaces, Wasm backend absence, five
probe failure states, both output caps, late and invalid UTF-8 output, duplicated/
malformed metadata, backend-count bounds, config/budget rejection, and bounded
escaped JSON. These are authored tests, not executed acceptance evidence.
The standalone harness reads lane-local baseline snapshots of JSON and the
approved corrected target module. After integration, root should import the
actual bhasa modules and retain its existing target correction tests.

Baseline HEAD is `591d1a7e06d9aa328c6def5ee049b7aa600a9ea2`.
`evidence/baseline.json` records exact hashes of baseline/src/driver.rs, json.rs,
target-current.rs and the approved corrected target.rs. It identifies the
canonical correction source path. New shared module/doc paths were absent at
capture. `patches/01-inspection-api.patch` adds only src/toolchain_inspect.rs and
docs/toolchain-inspection.md. It contains no driver/target replacement or CLI wire.
`evidence/draft-sha256.txt` records all final draft hashes. rustfmt formatting and
syntax checks pass for the two new Rust files; no native compilation occurred.

Coordinator build-slot request, small standalone rustc harness only:

```sh
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/toolchain-v1/draft/tests/toolchain_inspect_contract.rs -o build/backlog/gpu-platforms/toolchain-v1/evidence/toolchain-inspect-tests
build/backlog/gpu-platforms/toolchain-v1/evidence/toolchain-inspect-tests --test-threads=1 --skip json:: --skip target::
```

The skip filters select only the eight new injected cases, avoiding re-running
baseline target/JSON tests in this lane's slot. Once compiled, root can run focused
integrated tests under its normal Cargo slot. Actual CLI acceptance still needs
missing/slow/flooding child probes, simultaneous stdout/stderr limits, child/pipe
cleanup and JSON stdout/exit tests in the production adapter. No candidate target
or registered backend is promoted to SDK/hardware/runtime verification.

## Target/toolchain inspection API proposal, October 12

The next isolated increment is `build/backlog/gpu-platforms/toolchain-v1/`.
Proposed CLI is `bhasa targets --inspect --json [--target ALIAS]`, default native.
The coordinator owns CLI/driver integration. Existing `bhasa targets` stays a
candidate listing. No compilation/link/device execution is implied by inspection.

Proposed host API is `inspect(Target, ConfiguredClang, ProbeBudget, &mut Probe)`.
Probe is injected and receives an executable plus argv, empty stdin, independent
stdout/stderr caps and a timeout. It returns bounded output or a structured
missing/failed/timed-out/overflow status. The production adapter must kill/reap
children and drain pipes without deadlock; no production process adapter is added
in this slice. Executable config follows BHASA_CLANG as one unsplit executable
path, with an empty override rejected and clang as PATH default.

Three read-only probes are proposed: --version, --print-targets and the driver's
empty-stdin LLVM triple query with explicit -target only for non-native targets.
JSON schema v1 records candidate alias/triple, configured executable/origin,
version probe, resolved LLVM triple, backend registration, runtime admission and
separate unverified SDK/linker/link/execution gates. Timeout/malformed/overflow
output is unknown or an error, never unavailable hardware evidence. Registered
backend support is not successful object emission. Runtime admission is a source
policy, never verified cross-device execution.

Canonical POSIX admission will call the approved concurrency correction's
`target::require_native_posix64`, preserving deployment triples verbatim. This
lane will not duplicate or modify target normalization. The standalone harness
uses an exact snapshot of that approved correction until root integrates it.
No engine/SDK acquisition, network, cache operation or native/Cargo/shared/Git
mutation is part of this proposal. Frozen Wasm/kernel evidence stays intact.

## No-GC engine gate and object inventory

The coordinator clarified that the user's strict no-GC requirement includes
production host/runtime dependencies. Node remains a verification tool only;
it is not a production host and its fixture cannot satisfy runnable no-GC Bhasa.
The explicit gate and a reference-feature policy correction are isolated under
`build/backlog/gpu-platforms/wasm-v3/`. Earlier drafts remain unchanged.

- `draft/docs/wasm-no-gc-engine-gate.md` specifies dependency/version/checksum/
  transitive-feature review, collector exclusion, engine configuration, binary
  inspection and deterministic trap/exit/cancellation cleanup before adopting
  any Rust engine. Rust language or a GC-disabled guest alone is insufficient.
- `draft/src/wasm_policy.rs` adds required `ModuleFacts.reference_features` and
  PolicyError.Reference. It rejects reference types/instructions/tables separately
  from gc_features. The first scalar profile excludes externref and funcref.
  Facts must come from actual validated binaries; the policy is not a decoder or
  engine configuration. Existing code supplying ModuleFacts must set both fields
  from the reviewed engine inspector, not default them to false.
- `draft/tests/wasm_policy_contract.rs` adds an independent GC/reference rejection
  check. Eight standalone rustc tests pass with warnings denied, recorded in
  `wasm-v3/evidence/wasm-policy-tests.log`.
- `draft/tests/inspect_guest_object.py` checks undefined native symbols against
  the exact three-symbol guest whitelist. O0/O2 strict freestanding objects of
  the v2 C runtime contain only bhasa_host_read/write/close. Evidence is in
  `guest-native-o0-undefined.txt`, `guest-native-o2-undefined.txt` and
  `guest-import-inventory.json`. No native allocator/collector/libc helper appears
  in those objects. This does not attest to a future Wasm object's helpers/imports.

Cargo.toml has no dependencies and Cargo.lock contains only bhasa. No engine
is configured in the project dependency graph. Exact copies and hashes are in
`wasm-v3/baseline/` and `evidence/baseline.json`, captured at HEAD
`4828affc7a6bd5a877f5cb45d506603304004041`. No dependency acquisition or SDK/tool
installation occurred. The Wasm-v2 configured tool inventory remains applicable.

`patches/01-no-reference-profile.patch` is a small patch against the exact v2
submitted policy/test baselines, not a replacement of current tracked sources.
Apply it after the v2 policy, preserving unrelated integration updates.
`patches/02-engine-gate-inventory.patch` adds only the new document/object inspector.
`evidence/draft-sha256.txt` hashes the final v3 draft. No tracked source was edited.

Object probe commands, native only:

```sh
clang -std=c11 -Wall -Wextra -Werror -pedantic -ffreestanding -fno-builtin -fno-stack-protector -O0 -c build/backlog/gpu-platforms/wasm-v2/draft/runtime/wasm_guest.c -o build/backlog/gpu-platforms/wasm-v3/evidence/wasm-guest-native-o0.o
clang -std=c11 -Wall -Wextra -Werror -pedantic -ffreestanding -fno-builtin -fno-stack-protector -O2 -c build/backlog/gpu-platforms/wasm-v2/draft/runtime/wasm_guest.c -o build/backlog/gpu-platforms/wasm-v3/evidence/wasm-guest-native-o2.o
python3 build/backlog/gpu-platforms/wasm-v3/draft/tests/inspect_guest_object.py build/backlog/gpu-platforms/wasm-v3/evidence/wasm-guest-native-o0.o build/backlog/gpu-platforms/wasm-v3/evidence/wasm-guest-native-o2.o
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/wasm-v3/draft/tests/wasm_policy_contract.rs -o build/backlog/gpu-platforms/wasm-v3/evidence/wasm-policy-tests
build/backlog/gpu-platforms/wasm-v3/evidence/wasm-policy-tests --test-threads=1
```

Production gate remains pending an actually available, reviewed no-GC Rust engine
and configured guest compiler/linker, binary feature inspection/import signatures,
both frontend guest lowering and production adapter cleanup tests. No policy
module, Node fixture or native object probe substitutes for those gates. No Cargo,
broad suite, publication, hardware claim or inter-chat message occurred.

## Second bounded Wasm increment

The coordinator approved the memory32 bhasa_host_v1 read/write/close contract for
this draft. New work is isolated under `build/backlog/gpu-platforms/wasm-v2/`.
The first draft remains intact. No tracked shared source, Cargo, Git operation,
SDK/library acquisition or broad suite ran. GPU fixed-width/kernel syntax remains
unratified and the GPU models remain host models, not language support.

Implemented additions with original integration paths:

- `draft/runtime/wasm_guest.c` and `.h`: freestanding bounded arena and exact
  allocation ledger, nonreused addresses, zero/wrong/interior/stale/double-free
  checks, internal buffer cleanup, staged read, partial write and consumed close.
- `draft/src/wasm_policy.rs`: closed import/export/memory/start/GC policy, explicit
  linker argv and typed host namespace status decoding.
- `draft/std/wasm.bh`: HostFailure, private HostError, WasmResult[T] and pure typed
  status decoding. Existing Result.error(Text) and IoResult.error(io.Error) retain
  their identities; they reject HostError payloads.
- `draft/tests/wasm_guest_contract.c`, `wasm_policy_contract.rs`, `wasm_status.bh`
  and `check_wasm_status.py`: native ledger, policy and both frontend checks.
- `draft/tests/generate_wasm_abi_fixture.py` and `wasm_engine_abi.mjs`: SDK-free
  Wasm fixture and a test-only Node engine shim.
- `draft/docs/wasm-guest-runtime.md`: API details, integration requirements and
  precise limits. Export free now returns a status; alloc(0) is canonical zero,
  and positive alloc failure is zero with immediate alloc_status query.

Current baseline HEAD is `e24aa78ba10c146020e48d984d4267dbabe7c630`.
`wasm-v2/evidence/baseline.json` records hashes and exact copies under baseline/
of src/lib.rs, driver.rs, target.rs, codegen.rs, std/io.bh, generic.bh and text.bh.
It records all new integration paths as absent. `patches/01-policy-export.patch`
adds only a wasm_policy module export. `patches/02-guest-and-policy.patch` adds new
files. No driver/target/LLVM allocator change is proposed. Draft hashes are in
`evidence/draft-sha256.txt`. Both patches preserve unrelated integrated updates.

The allocation profile is deliberately bounded by cumulative arena use. Free
releases a ledger slot but does not reclaim addresses. This prevents stale pointer
ABA within one instance; it is not a general-purpose reclaiming allocator. No
reset export exists. Actual guest allocation lowering must use these contracts
explicitly; current native malloc/free and buffer layouts cannot be rebound to
these signatures. Unsafe C metadata pointers are trusted internal calls, not
validated public raw-pointer exports. Rust host range checks remain independent.

The link policy consumes engine-validated binary facts, never sidecar claims.
It requires memory exported as memory, bounded memory32, exact names/signatures,
no automatic start section, no imported nonfunctions and no GC features. LinkPlan
is argv only, with explicit exports and a three-symbol undefined whitelist. It
has not been executed by a linker. Runnable compiler guard remains in place.

Verification completed without Cargo:

- Seven rustc policy tests pass with warnings denied in
  `wasm-v2/evidence/wasm-policy-tests.log`.
- Strict freestanding C11 native ledger/wrapper tests pass at O0/O2 with ASan and
  UBSan in `guest-tests-o0.log` and `guest-tests-o2.log`. The tests exercise both
  live quota and cumulative arena exhaustion and end at zero live allocations/
  bytes. After the final compile-time ledger bound assertion, strict syntax
  validation also passes.
- Existing `target/release/bhasa --frontend rust` and
  `build/hybrid/frontend/aarch64-apple-darwin/debug/bhasa --frontend bhasa` accept
  the status wrapper and reject four privacy/error-identity fixtures. Evidence
  `status-wrappers.log`. The runner takes an explicit repo root and uses a temporary
  package with exactly lowercase bhasa.toml, preserving module-root policy.
- A 227-byte core Wasm fixture executes successfully in installed Node v26.8.1.
  `engine-abi.log` covers imports, unsigned offsets, LE counts, detached old views
  after memory growth, maximum growth rejection, consumed-close fixture behavior
  and a trap. The fixture does not contain the C runtime or compiled Bhasa, and
  its JS shim does not instantiate the Rust Host. Its finally block is fixture
  teardown, not proof of production Rust host cleanup. Node is test-only and no
  JavaScript production runtime library is introduced.

Reproduction from repository root:

```sh
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/wasm-v2/draft/tests/wasm_policy_contract.rs -o build/backlog/gpu-platforms/wasm-v2/evidence/wasm-policy-tests
build/backlog/gpu-platforms/wasm-v2/evidence/wasm-policy-tests --test-threads=1
python3 build/backlog/gpu-platforms/wasm-v2/draft/tests/check_wasm_status.py . target/release/bhasa build/hybrid/frontend/aarch64-apple-darwin/debug/bhasa
python3 build/backlog/gpu-platforms/wasm-v2/draft/tests/generate_wasm_abi_fixture.py build/backlog/gpu-platforms/wasm-v2/evidence/abi-fixture.wasm
node build/backlog/gpu-platforms/wasm-v2/draft/tests/wasm_engine_abi.mjs build/backlog/gpu-platforms/wasm-v2/evidence/abi-fixture.wasm
clang -std=c11 -Wall -Wextra -Werror -pedantic -ffreestanding -O2 -fsanitize=address,undefined -fno-sanitize-recover=all -DBHASA_GUEST_MAX_ALLOCATIONS=16 -DBHASA_GUEST_ARENA_BYTES=512 -I build/backlog/gpu-platforms/wasm-v2/draft/runtime build/backlog/gpu-platforms/wasm-v2/draft/runtime/wasm_guest.c build/backlog/gpu-platforms/wasm-v2/draft/tests/wasm_guest_contract.c -o build/backlog/gpu-platforms/wasm-v2/evidence/guest-tests-o2
build/backlog/gpu-platforms/wasm-v2/evidence/guest-tests-o2
```

Read-only configured/PATH and xcrun inventory is in tool-inventory.json. No
BHASA_CLANG/Wasm engine/linker/shader overrides were configured. Both /usr/bin/clang
and xcrun Clang lack a Wasm backend; no wasm-ld/ld.lld was found by PATH/xcrun.
Node's engine is installed. Xcode Metal compiler is installed via xcrun, correcting
the narrower first-draft inventory. No GPU compiler/device probe ran in this
Wasm-only increment. No SDK/library installation was attempted.

Pending acceptance remains compiled C guest runtime in Wasm, actual linker argv,
Rust engine import adapter and binary inspection, both frontend guest allocation/
clone/drop/I/O lowering, typed I/O wrappers and engine-based allocator/trap cleanup
checks. No WASI, browser production host, mobile, embedded/freestanding startup or
GPU language/hardware completion is claimed. No Cargo build slot is currently
needed; the coordinator can review additive code first and route approved existing
Rust engine/linker prerequisites. This lane has sent no inter-chat messages.

## First increment evidence

The sections below record the earlier first-draft handoff. Its lack of Wasm
execution evidence was accurate at that handoff; the narrow core-engine fixture
above is new evidence and does not establish integrated runnable Wasm support.

## Draft and baseline

Root directory: `build/backlog/gpu-platforms/`.

- `draft/src/wasm_host.rs`: engine-independent bounded Wasm memory32 host I/O.
- `draft/src/portable_gpu.rs`: restricted u32 map representation, WGSL/Metal/CUDA
  generation, checked tensors/pipelines and submission buffer ownership.
- `draft/tests/wasm_host_contract.rs`: eight focused host contract tests.
- `draft/tests/portable_gpu_contract.rs`: eight focused GPU host model tests.
- `draft/docs/wasm-host.md`: proposed runnable ABI, engine adapter rules and gates.
- `draft/docs/gpu-portable.md`: kernel/device ownership contract and next gates.
- `draft/src/lib.rs`: exact baseline copy plus two module exports for integration.

HEAD at capture is `c4f89575fd0f817f1d0cb02915ab841de166d05f`.
`evidence/baseline.json` records the exact hashes of working source files, including
uncommitted frozen error work. `baseline/` contains exact copies of src/lib.rs,
src/target.rs, src/driver.rs, src/codegen.rs, src/builtins.rs and runtime/metal.h.
The new source/test/doc paths were absent at capture. `evidence/draft-sha256.txt`
records draft file hashes. Earlier initial inspection hashes are retained in
baseline-sha256.txt; baseline.json is authoritative for the reviewable patch.

Apply `patches/01-modules.patch`, a two-line export addition, and
`patches/02-host-contracts.patch`, new files only. Do not overwrite shared sources
with the full lib.rs copy. No driver, AST, checker, native emitter, builtin or
native GPU runtime patch is proposed in this increment. Both frontends retain
existing behavior, including contextual try and immutable-binding contracts.

## API contracts

The proposed `bhasa_host_v1` namespace has read/write/close i32 imports with unsigned
u32 memory offsets and lengths. Rust Host accepts owned read/write capabilities,
never reuses handle IDs and drops all remaining resources on instance shutdown.
It validates transfer caps, current memory bounds, rights and data/count overlap
before I/O. Read staging prevents publishing partial/uninitialized bytes on native
error. Write reports partial success. Close consumes the handle even if flush
fails. Native errors can consume input or alter output; no rollback is promised.
Host limits and staging allocation are fallible. No native pointers cross imports.
The host component has no engine dependency and does not execute a Wasm module.

Kernel admits at most 64 Add/Multiply/Xor u32 operations with explicit wrapping
arithmetic and fixed generated identifiers. Translation emits bounds checks and
separate input/output storage. Tensor product/bytes, rank, device, shape and grid
limits are checked. Pipeline rejection returns both owners unchanged. Submission
holds both owners; terminal completion publishes only a complete successful
output. Error/short completion preserves old output and returns both owners.
LE encoding is explicit. CPU oracle checks arithmetic independently of shader text.
The DeviceBuffer is a host ownership model without a hardware allocation. Real
backend fences, async queue/cancellation and Drop safety remain required before
adding device handles. Caps contain only byte/grid limits; adapters must also
query binding/alignment/threadgroup constraints before actual dispatch.

These Rust contracts do not add language syntax, device types or foreign ABI.
Bhasa int remains checked signed i64. Fixed-width u32 and kernel syntax depend on
ownership-types lane contracts. Language async GPU consumption depends on the
concurrency-async lane's scheduler/resource result contracts. Typed host error
wrappers must preserve the approved Result/try error contract. Binding-only let
must not be mistaken for deep immutability or a read-only GPU borrow.

## Verification evidence

Existing Rust 1.98.0 compiled each small standalone test harness with edition 2021
and warnings denied. Both ran with one test thread. No Cargo was used.

```sh
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/draft/tests/wasm_host_contract.rs -o build/backlog/gpu-platforms/evidence/wasm-host-tests
build/backlog/gpu-platforms/evidence/wasm-host-tests --test-threads=1
rustc --edition 2021 --test -D warnings build/backlog/gpu-platforms/draft/tests/portable_gpu_contract.rs -o build/backlog/gpu-platforms/evidence/portable-gpu-tests
build/backlog/gpu-platforms/evidence/portable-gpu-tests --test-threads=1
```

Eight host tests pass in `evidence/wasm-host-tests.log`. Eight GPU tests pass in
`evidence/portable-gpu-tests.log`. Checks include 51,200 small memory intervals,
unsigned memory32 end arithmetic, malformed ranges/overlap with no I/O,
read failure preserving memory, partial output, rights, consumed failed close,
stale IDs, admission/drop cleanup, empty transfers, shape/byte/product overflow,
wrong-device rejection, owner return, short output preservation and grid counts
0/1/63/64/65/1024. rustfmt applied only to these draft Rust files.

Shader tests inspect generated declarations and access guard ordering, without
claiming shader compiler acceptance. No installed tint/naga/glslang/wasm tool was
found by the bounded Homebrew-bin inventory; the usual CommandLineTools metal
compiler path is absent. This does not establish that every possible configured
SDK/tool location is absent. Core Wasm types and WGSL conventions were checked
against their primary specifications, linked in the draft contracts.

## Coordinator requests and remaining gates

No immediate build slot is needed for the standalone tests. After reviewing the
additive modules, coordinator can run formatting and focused Cargo tests in its
serialized slot. Exact post-integration tests:

```sh
CARGO_BUILD_JOBS=2 cargo test --test wasm_host_contract --test portable_gpu_contract -- --test-threads=1
```

Request a coordinated installed-tool inventory before shader compilation or engine
execution. Do not install SDKs or acquire libraries as part of this handoff. No
actual Wasm/WASI/browser execution, portable GPU execution, CUDA/Vulkan hardware,
Linux/Windows target execution, mobile deployment or bare-metal testing is claimed.

Dependency order for next increments:

1. Ratify Wasm host exports and guest allocation ledger, implement target-specific
   guest runtime and checked linker/import/export policy in a new draft. Connect
   a Rust engine adapter only after approved existing dependencies/tool availability.
   Verify both frontends with engine-based trap/memory-growth/I/O cleanup gates.
2. Ratify fixed-width/kernel declarations, add both frontend checks/lowering to
   this representation, then reusable Metal allocation/pipeline/transfer/fences.
   Require actual new-API Metal results against CPU oracles and resource counters.
3. Add asynchronous ownership/cancellation using agreed task contracts, then
   CUDA/Vulkan/WebGPU adapters with mock failure injection before hardware gates.
4. Add explicit target runtime capability manifests and configured SDK/linker
   inspection. Complete Linux/Windows owned runtime and CI on actual target hosts.
5. Define allocation-free/freestanding profiles, startup/linker/MMIO/interrupt
   contracts, then mobile runtime/deployment and compiler distribution artifacts.
   Each needs target-specific execution or explicit unavailable acceptance notes.

Kernel translation is deliberately a restricted first representation. General
kernel syntax, tensors/views, f32/SIMD, Vulkan translation, rendering, debugging,
profiling and performance claims remain open. No todo.md box is marked complete.
The coordinator can inspect all handoff details here. This lane has sent no
messages to other chats.
