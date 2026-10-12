# Safety and errors backlog lane

Updated October 11, 2026. First implementation is ready for contract review and
native parity work. It is a draft, not integrated acceptance. This lane has not
changed tracked compiler/runtime sources, run Cargo or broad suites, changed
caches, installed SDKs, committed, pushed or claimed hardware verification.

## First increment and contract

The existing immediate-discard rule accepts `var ignored = make_result()`.
The draft adds conservative obligations for local result-shaped values. An enum
qualifies through the existing `EnumDecl.result_shape()` helper, preserving
exactly two one-payload variants named `ok` and `error`, arbitrary declaration
order, concrete nominal payload types and non-task payloads. Result, IoResult,
initializer-only try, binding-only let and runtime ABIs retain their contracts.
The pass matches Var/TryVar with `..`, so the reserved reassignable field is
compatible. No AST fields or scanner tokens are added.

A checked local Var initializer, assignment, result-valued TryVar success,
result-valued match payload or result-valued for binder creates a pending
obligation. Direct exhaustive `match binding`, `return binding`, or whole
initializer `try binding` discharges that binding. Every other pending local
rejects a reachable return or implicit try error return. Pending locals reject
scope exit, iteration exit, break and continue. Assignment rejects overwriting a
pending value, even self-assignment. Every new assigned result creates a fresh
obligation. Ordinary reads, argument passing, copies, method calls and storing
inside aggregates do not discharge a local. A copied Result has its own
obligation; handling the copy leaves the original pending. This deliberately
rejects some programs that could be proved safe by future alias/call analysis.

Branch joins union pending state from continuing paths only. Returning paths
validate their own obligations and do not contaminate following statements.
Shadowing uses lexical scope identity, including non-result binders that mask
outer names. Checked exhaustive match discharges its direct subject before
analyzing payload binders and arm exits. Loops preserve pre-loop outer state
because they may execute zero times. Result assignment to an outer binding from
any loop body rejects conservatively. Local results remain allowed in each
iteration. This restriction avoids dropping obligations on backedges, breaks or
nested-loop exits before a full fixed-point analysis exists.

Function parameters retain the old contract. Result expressions embedded directly
inside call arguments, aggregate storage or arbitrary nested expressions remain
outside this slice, as do alias equivalence, field/container obligation tracking,
interprocedural handling summaries and scalar native status obligations. This is
stronger enforcement of local bound results. Complete assigned/passed/aggregate
failure safety remains open and must not be marked complete by this slice.
Fatal arithmetic, bounds, allocation and corrupt-tag traps remain fatal traps.
No catching, unwinding or trap-to-return conversion exists in this increment.

## Draft and baseline

All code/tests use original relative paths under
`build/backlog/safety-errors/draft/`:

- `src/failure_use.rs` adds the checker over typed ASTs.
- `src/sema.rs` runs it after ordinary checking succeeds.
- `src/codegen.rs` rechecks obligations at public emit_llvm entry, including a
  caller-mutated CheckedProgram.
- `src/lib.rs` registers the private module.
- `tests/failure_use_reference.rs` adds eight focused acceptance groups.
- `tests/error_propagation_reference.rs` changes the previous assigned-discard
  acceptance to require S037, preserving the rest of the existing error tests.

`build/backlog/safety-errors/failure-use.patch` contains the small edits plus new
files. Existing-source snapshots are in `baseline/`, with SHA-256 manifests
`baseline.sha256` and `baseline-files.sha256`. Base Git HEAD at capture was
`c4f89575fd0f817f1d0cb02915ab841de166d05f`; source included the uncommitted frozen
error/try increment. Use contextual patches, not whole-file replacement, when
integrating after immutable bindings or other shared changes. Diagnostic S037
points to the discard/exit. Messages include the binding name without raw
generated/global byte offsets. S036 remains reserved for immutable binding
reassignment. The imported generic original-file regression below passes; CLI
and native diagnostic parity still require integration verification.

## Executed evidence

`build/backlog/safety-errors/probe.rs` imports the existing compiler library's
AST, builtin schemas, diagnostics and collections, and compiles the actual draft
sema, failure-use checker and code generator with rustc. Existing host checking
supplies expanded concrete typed ASTs before the draft sema rechecks them. The
harness runs only its eight failure_acceptance groups; 39 source-local existing
unit tests compiled but were filtered out. This is a bounded source probe, not a
Cargo suite or release/native frontend verification.

Exact commands from the repository root:

```sh
CARGO_MANIFEST_DIR=/Users/arpanbhandari/Documents/ChatGPT/bhasa rustc --edition 2021 --test build/backlog/safety-errors/probe.rs --extern bhasa=target/debug/deps/libbhasa-bd69e13e13c63319.rlib -L dependency=target/debug/deps -o build/backlog/safety-errors/probe
build/backlog/safety-errors/probe failure_acceptance:: --test-threads=1
```

Eight groups passed, zero failed, in 0.69s. Log is
`build/backlog/safety-errors/probe.log`; library identity is pinned in
`probe-library.sha256`. Coverage includes overwritten/unread locals, argument
passing and alias copies, both-branch handling, early return, shadowing,
zero-iteration loops, break/continue, nested loop assignments, try's implicit
error exit, nested Result payloads, concrete Result/IoResult generics, methods,
joined typed tasks and emitter rejection after checked-AST mutation. The new
imported generic regression uses multibyte UTF-8 comments and CRLFs, follows the
host module/generic diagnostic pipeline, and verifies the original library path,
exact `return 0` span, line 5/column 1 JSON position and exact offset-free message.
The first
probe exposed a fixture mistake: unreachable statements already reject with
S015. The corrected fixture preserves that existing diagnostic.

## Coordinator requests and remaining acceptance

1. Review this exact conservative contract and route it to native-compiler for
   parity. A distinct native diagnostic, suggested BH221 after reserved BH220
   for immutable assignment, should cover these same paths. Native state must
   remain separate from local mutability, ownership, task metadata and type IDs.
   Native checks must join branch paths and preserve outer loop obligations;
   a global "was read" flag is insufficient. The eight host groups provide
   source fixtures and expected acceptance, not native evidence.
2. Typed errors are now committed and pushed as `86a99ed`, per coordinator
   handoff. Integrate the contextual patch only with matching native checking. Request a serialized root build slot for
   `CARGO_BUILD_JOBS=2 cargo test --test failure_use_reference -- --test-threads=1`
   and the existing error-propagation tests. Check both compiler configurations.
   Existing tests that intentionally discard bound Result values need real
   match/return/try handling rather than disabling the check.
3. Require imported/generic original-file diagnostics, private opaque result
   APIs, let/try interaction, worker restrictions, checked-AST corruption,
   bounded native parity, owned O0/O2 cleanup and coordinator-owned final
   suite/release/self-rebuild/cache gates before claiming integration.

No verification slot is running or held by this lane.

## Preserved prerequisites for later increments

Typed file I/O already returns IoResult with private Error fields while legacy
Result uses Text. Keep those separate identities. Streaming I/O belongs to
std-backend; broader typed errors need shared domain/code/operation/message and
partial-side-effect contracts before adding wrappers. No fallible wrapper may
label a fatal trap as a recoverable failure.

Library storage invariants remain open. `std/text.bh` explicitly exposes mutable
`Text.bytes`; `std/result.bh` still constructs Text directly. Privatizing storage
requires an audited migration of constructors/getters/mutation, caller changes
and both-frontends privacy acceptance. List/Map storage is compiler-generated;
there are no std/list.bh or std/map.bh files. Do not infer completion from Error's
already-private fields.

Foreign error/unwind/thread metadata belongs to a joint contract with
packages-ffi. Current extern worker rejection remains in place, scalar native
statuses remain unchanged, and adapters must not unwind through the C ABI.
No metadata declaration alone proves pointer validity, ownership or thread
safety. Later independent acceptance should cover malicious metadata, worker
call-graph paths, error output preservation, panic/trap behavior and deterministic
resource release. These prerequisites are recorded; their implementation is
still pending and the backlog remains unchecked.

## Immediate fixture migration handoff

The coordinator's independent reviewer found 22 S037 diagnostics across the four
existing owned-fixture constants. This lane prepared an isolated 86-line patch at
`build/backlog/safety-errors/fixture-migrations/fixture-migrations.patch` against
the current root-applied immutable-binding tests. Exact original files are under
that directory's `baseline/tests`, with `baseline.sha256` and `baseline-head.txt`.
The original pending checker source and patch remain frozen.

Draft test files retain original relative paths under
`build/backlog/safety-errors/fixture-migrations/draft/tests/`:

- `error_propagation_reference.rs` retains mutation of the propagated success
  bytes and error message/List/Map, and rechecks the original source payload.
  It records scalar success/error flags and sizes until matching the original
  source, then performs the same early-return assertions.
- `error_propagation_bhasa.rs` handles each original good/bad result after take
  and before creating the next pending result. It retains the subsequent owned
  payload checks and expected helper output. A joined worker result now matches
  directly after recording read_output's scalar result, preserving that helper's
  error-message and detail-count checks.
- `immutable_bindings_reference.rs` records scalar failure statuses until the
  immutable original success/failure has been matched and checked after
  propagation. Buffer mutation and independent-original assertions remain.
- `immutable_bindings_bhasa.rs` handles the original success before creating the
  failure, handles the original failure after forwarding, and adds a check that
  its message remains `failure`. All task/loop/let mutation checks remain.

All four extracted constants passed the already-built isolated reviewer checker
`build/backlog/safety-errors/review-probes/check`, zero diagnostics. Evidence is
`fixture-migrations/check-results.json`, with the exact extracted .bh sources in
that directory. No runtime/sanitizer or native parity gate was run for these
migrations. Root must run the existing O0/O2 ownership/allocation tests and native
checker acceptance with this patch before integrating it.

The conservative opaque-result restriction is explicit. A caller binding a
private result enum returned by `lib.make()` cannot discharge it by passing the
binding to `lib.consume(value)`, even when consume matches internally. Caller
match/try is also unavailable because the enum is private. Inline
`lib.consume(lib.make())` remains outside the local-binding slice and accepted.
This means opaque-result APIs are not all source compatible with S037. It does
not establish interprocedural or full assigned/passed/aggregate failure safety.

`draft/tests/failure_use_opaque.rs` and the separate
`opaque-contract-test.patch` record this regression. The existing isolated
reviewer project pipeline rejects the bound form with original-file S037 and
accepts the inline form; evidence is `fixture-migrations/opaque-results.json`.
The Rust test syntax compiles against the existing library but its CLI execution
requires coordinator integration. No native evidence is claimed for this case.

## Private Text storage draft

A separate `build/backlog/safety-errors/text-storage.patch` prepares Text storage
privacy and caller migration. Exact snapshots of all changed existing files are
under `text-baseline/`; `text-baseline.sha256` and `text-baseline-head.txt` identify
the original sources. Text baseline HEAD was
`e24aa78ba10c146020e48d984d4267dbabe7c630`. The patch must be applied contextually,
independently of pending-failure integration or fixture-migration patches.

The public API delta is deliberate:

- `Text.bytes: buffer` becomes private `Text.storage: buffer`. Direct external
  field reads, assignments, borrowed mutation and positional `Text(buffer)`
  construction reject. This is a source-breaking change.
- `Text.bytes() -> buffer` returns an independent owned copy. Mutating it cannot
  change Text. `byte_count() -> int` avoids copying just to read byte length.
- `text_from_bytes(buffer) -> Text` is a new checked, copying constructor for
  already-validated/trusted bytes. It uses utf8_slice's full validation and
  fatal invalid-UTF-8 path. The extra owned intermediate follows existing
  deterministic cleanup; no allocation or copy-elision claim is made.
- `text_decode(buffer) -> ResultText` remains the fallible untrusted boundary,
  preserving error text and independently copied success payloads. It calls the
  checked constructor after validation. Result and IoResult identities remain.
- `text(str)`, valid/count/codepoint/part/equals/append/append_codepoint keep their
  signatures. New replace(Text) and clear() mutate Text through valid Text or an
  empty buffer. Invalid scalar append returns the existing status and leaves
  storage unchanged. part's bad-range failures remain fatal.

Changed original-relative files under `draft/` are `std/text.bh`, result.bh,
collections.bh, json.bh and http.bh; `packages/tls/lib.bh`; HTTP/HTTPS/errors
example main.bh files; backend_stdlib.rs, typed_io.rs, errors_integration.rs and
the existing text library fixture; docs/backend.md and docs/io.md. New files are
`tests/text_storage.rs` and `tests/fixtures/text/private-storage.bh`.

The audit found every external Text constructor in std/result, collections,
JSON, HTTP and the HTTP example. All are migrated. All Text byte-field callers
in standard modules, TLS package, examples and current tests/docs are migrated
without changing unrelated user-defined record bytes fields. Only two direct
Text constructors remain, both inside its defining text.bh module. JSON.string
now returns text_decode's typed failure if externally mutable JsonDoc storage
produces invalid decoded UTF-8. JSON quoting snapshots bytes once before its
loop; collection set snapshots once; HTTP response length uses byte_count.

The original invalid-Text fixture now corrupts a detached byte copy, verifies
Text remains valid, and feeds the corrupted copy to text_decode. Typed-I/O and
propagation fixtures use message.clear() to check independent returned messages;
this avoids introducing an unqualified text factory into aliased import scopes.
Historical docs/status evidence remains historical and was not rewritten.

Existing binaries were used for bounded probes, sequentially, with no Cargo:
`target/debug/bhasa --frontend rust` and
`build/hybrid/frontend/aarch64-apple-darwin/release/bhasa --frontend bhasa`.
Compiler SHA-256 identities are in `text-probe/compiler.sha256`. The initial debug
binary lacks the native frontend, so the native release binary was selected.

Both frontends checked, compiled and executed the positive .bh fixture, printing
`private Text storage passed`. It covers Unicode/scalar/byte counts, NUL,
constructor/getter/copy independence, temporary getters, mutators, invalid scalar
status preservation, fallible decode, collection extraction, JSON and HTTP head
parsing. Both reject five private constructor/storage access cases with M005
and old `.bytes` field access with S026. Native privacy rejection uses the shared
host verifier before native emission; positive execution uses native emission.
Both compiled and executed the invalid constructor fixture, exiting 1 with
`Bhasa runtime error: invalid UTF-8`, empty stdout and no returned failure.

HTTP/HTTPS/errors examples, the typed-I/O fixture, the text/collection fixture
and the aliased I/O propagation fixture pass checked acceptance in both modes.
The HTTPS fixture copies the local TLS package and points its lowercase
bhasa.toml at the copied std package. The alias audit exposed an unqualified text
factory in the first mutation draft; clear() fixes that source error.

All evidence is under `text-probe/`: check-results.json, run-results.json,
privacy-results.json, trap-results.json, caller-results.json and
additional-caller-results.json. Three new integration test groups are authored
and syntax-compiled with rustc against the existing library, but were not run
as a Cargo suite against the unintegrated std changes. No TLS socket, live HTTP
server, sanitizer, allocation-counter, release/self-rebuild or cache gate is
claimed for Text. Root should request native/host O0/O2 ownership acceptance,
privacy/source-location regressions and its combined gates after review.

This draft protects Text's UTF-8 storage within checked Bhasa code. StringList,
StringMap, StringIntMap and JsonDoc still have public storage and separate
invariant work remains open. Arbitrary foreign dangling pointers or corruption
are outside this source-privacy guarantee. No broader library-invariant backlog
item is marked complete.

## Private collection and JSON storage follow-up

The coordinator reports pending S037/BH221 root-applied after immutable bindings
commit `4828aff`. It repaired two negative worker fixtures to handle the joined
Result before asserting BH209; this lane did not alter that integration or its
pending checker drafts. The following independent library increment is ready
for review atop the private Text draft.

Patch is `build/backlog/safety-errors/storage-invariants/storage-invariants.patch`.
Original-relative sources/tests are under that directory's `draft/`; exact
parent snapshots, SHA-256 manifest and baseline description are in `baseline/`,
`baseline.sha256` and `baseline-description.txt`. Parent source is this lane's
Text draft, not tracked std sources. Apply text-storage.patch first, then this
follow-up patch contextually. Captured root HEAD was
`4828affc7a6bd5a877f5cb45d506603304004041`.

StringList now has private packed storage and offsets. StringIntMap and StringMap
have private keys and values. External positional constructors consequently
reject; string_list/string_int_map/string_map remain the public constructors.
The original count/valid/push/get/set/find/has/put/value_at signatures and
insertion-order/replacement behavior remain. StringList.clear establishes empty
bytes and the zero sentinel offset. Both maps add clear, key_at(index) and
keys_snapshot(). key_at returns independent Text; keys_snapshot returns an
independent owned StringList. Mutating or clearing either snapshot cannot alter
the map. clear releases the prior owned contents and leaves a valid empty
collection; subsequent push/put remains supported.

JsonDoc now has private source, nodes, cursor and error fields, and its positional
constructor is inaccessible outside json.bh. json_parse remains its public
constructor and publishes a document only after complete validated parsing.
Its eight parser methods white/peek/hex/scan_string/digits/number/literal/value
are private, as are json_hex and json_unquote. Private fields alone would have
left public parser methods able to mutate a previously published document.
Public count/kind/raw/string/integer/find/item signatures remain, and the new
bytes() getter returns an owned source copy. No node-table/cursor getter or
mutable parser API is exposed. Number token spellings, JSON size/depth/node
bounds and invalid-index behavior remain unchanged.

These are deliberate source restrictions on formerly public fields,
constructors and helper functions/methods. External callers must use factories,
public accessors/mutators and snapshots. The complete caller audit found no
repository callers of collection raw storage/positional constructors or JsonDoc
parser internals outside their defining modules; no additional caller rewrite
was needed beyond the parent Text migration. docs/backend.md now documents
private storage, factories, snapshots, clear and private JSON parsing helpers.
No unrelated user-defined record fields or generic List/Map layouts changed.

New `draft/tests/storage_invariants.rs` has two integration test groups and
`draft/tests/fixtures/storage_invariants.bh` is the runtime fixture. Existing
binaries were used sequentially without Cargo or tracked changes:
`target/debug/bhasa --frontend rust` and
`build/hybrid/frontend/aarch64-apple-darwin/release/bhasa --frontend bhasa`.
Their captured identities are in `storage-invariants/probe/compiler.sha256`.

Both frontends checked, compiled and executed the positive fixture, printing
`private collection and JSON storage passed` with empty stderr. Coverage includes
empty/multibyte list values, length-changing replacement, independent copies and
extracted Text, clearing/reusing lists and maps, duplicate-key replacement and
insertion order, detached key snapshots, source/raw/Text copies from JSON,
original input mutation after parse, copied documents, invalid node access and
re-parsing the owned source getter.

Twenty-four negative fixtures each pass in both modes, 48 checks total. They
exercise all four collection/document constructors, raw storage reads/mutations,
key-snapshot representation access, all four JsonDoc fields, eight parser methods
and both decoding helpers. Field/constructor/method access rejects with M005;
private global helper calls reject with M003. Every diagnostic reports the
original importing file at line 3 after a multibyte UTF-8 comment. As with Text,
private-access rejection uses the shared host privacy verifier before native
emission; successful native execution uses the Bhasa emitter.

Fourteen checked compatibility probes pass across the parent Text runtime
fixture, HTTP/HTTPS/errors examples, typed-I/O fixture, aliased I/O propagation
fixture and text/collection fixture, using the new private collection/JSON
modules. Evidence is in `storage-invariants/probe/check-results.json`,
`run-results.json`, `privacy-results.json`, `privacy-cases.json` and
`caller-results.json`. The authored Rust test syntax also compiled with rustc
against the existing library; no Cargo suite was run against unintegrated files.

This increment preserves deterministic owned copies and fatal bounds/allocation
traps. It introduces no catch/unwind behavior, returned partial parser state,
fallible allocation guarantee or thread sharing. The guarantee applies to checked
Bhasa clients of these APIs; foreign memory corruption remains outside it.
No broad library-safety or production-acceptance backlog box is marked complete.

Coordinator verification request, after Text review and integration order are
settled: run the new storage_invariants integration groups in both compiler
configurations, existing text/backend/I/O tests and private-API original-file
checks. Add root-owned O0/O2 sanitizer/allocation checks for clear, snapshots,
parser success/failure and scope exits before the combined release/self-rebuild/
cache gates. The draft tests author both optimization modes, but the executed
bounded native probes used default optimization only. No build slot is held.

The private-storage guide was corrected after independent review. It now states
that explicit try is limited to whole var/let initializers and links errors.md,
and describes JsonDoc's private state and public copy-returning accessors. The
library implementation and pending-result checker remain frozen. The refreshed
full patch is storage-invariants.patch; documentation-correction.patch contains
only these two guide edits. documentation-correction.sha256 pins before/after
text and both patch identities; draft.sha256 pins all current follow-up files.
Original baseline hashes remain unchanged. No new runtime gate was needed for
this documentation correction; root owns combined acceptance before promotion.

## TCP copy policy contract, October 12

New isolated draft under build/backlog/safety-errors/tcp-copy-policy. Private std
storage and pending-result work remain frozen. TCP copies will allocate a fresh
opaque owner handle and atomically retain one immutable shared descriptor state,
without dup/fcntl or any fallible OS operation. Listener handles keep constructor
status and port; socket handles keep constructor status. Failed handles retain
exact error statuses and no descriptor state. Successful constructors attach a
shared state holding one nonblocking CLOEXEC fd and an atomic reference count.
Last-owner release closes once and frees the state; every owner wrapper frees
independently. Free never invokes shutdown. Retain overflow traps before changing
the counter; allocation failures retain the existing fatal runtime policy.

The opaque C signatures remain unchanged. Independent owners share the exact fd
number internally, instead of separate descriptors for the same kernel endpoint.
Stream/listener state was already shared under dup. Code borrowing an fd must
not close it or alter flags. Native consumers that assumed different fd numbers
per clone must stop doing so. Concurrent operation/clone/free on distinct live
owners is allowed; one owner must not be operated on, cloned or dropped
concurrently with its own free. Shared state contains no mutable IO state beyond
its atomic count. This does not add IO ordering or message framing. EOF follows
final-owner release, with no shutdown of another owner. TLS retains its owning
TCP transport while borrowing the fd, so its borrowed descriptor stays valid.

Implementation and evidence are pending. Root owns integration, loopback gate,
existing TCP/TLS/task regressions and full O0/O2 sanitizer/allocation acceptance.

### TCP copy implementation and bounded acceptance

The isolated TCP draft is ready for root review. No shared runtime, TLS source,
compiler, std-storage or pending-checker source was edited. Original-relative
files are under tcp-copy-policy/draft: runtime/net.c and net.h, the migrated
existing tests/fixtures/network/runtime.c, new
tests/fixtures/network/tcp_copy_policy.c and tests/tcp_copy_policy.rs. The
contextual patch is tcp-copy-policy/tcp-copy-policy.patch; source snapshots and
SHA-256 identities are baseline/runtime, baseline/tests, baseline.sha256,
baseline-files.sha256, baseline-head.txt and draft.sha256. Additional root
core/TCP/TLS/typed-task fixture identities are regression-baseline.sha256.

Actual internal state is `tcp_shared { atomic_uint references; int fd; }`.
Each listener wrapper has shared/status/port; each socket wrapper has
shared/status. Descriptor state attaches only after successful setup. Successful
listen, legacy connect, timed connect and accept paths all attach it. Failed
handles have no state. Retain uses a relaxed compare-exchange loop with a checked
UINT_MAX boundary, so overflow never wraps. Release uses acq_rel fetch-sub; only
the owner observing one closes and frees the shared state. Wrapper allocation
remains per copy. A successful constructor now also allocates one shared block;
its allocation failure is fatal, including after OS setup. This changes native
allocation counts and may require explicit fault-injection budget updates; the
legacy temporary hostname ENOMEM return remains unchanged.

All C signatures, opaque names, constructor statuses, listener ports,
nonblocking/CLOEXEC flags and native fd-borrow signature remain. No duplicate fd
is created by copying; no shutdown call was added. Distinct live wrapper owners
have shared stream/listener semantics and independent lifetimes. Dropping one
cannot close the fd held by another owner. A release-induced peer EOF occurs
after final local release, not on intermediate drops. Other causes of peer EOF
or IO error remain possible. The internal fd number is the same across copies;
foreign consumers must obey the documented borrowed-fd rule. No mutation of
per-handle/shared IO status, IO serialization or concurrent use/free of the same
owner is added. Independent retained owners can be used across threads, and a
source owner must remain alive throughout clone.

TLS needs no source change. Its tls_shared owns transport and stores its borrowed
fd; that transport reference holds the TCP shared state until TLS destroys it.
The bounded probe models this exact ownership seam by retaining a transport,
borrowing its fd, dropping every other owner and then successfully sending.
This is not a TLS handshake or session regression result.

Existing TCP injected-dup tests previously demanded an EMFILE error clone.
Their draft migration now arms the same fault, asserts clone success, identical
status/port or fd, unchanged descriptor count and an unconsumed dup fault, then
removes that unused fault before subsequent injections. It preserves all other
runtime faults, stream, EOF, inheritance and descriptor/allocation checks. There
is no silent success-to-error conversion or disabled fault assertion.

The new C probe uses real AF_UNIX socket pairs with injected successful TCP
constructor setup, so no loopback listener is needed. It tests invalid/null and
OS-EMFILE failed handles, successful listen/connect/accept, status-preserving
copies, nonblocking CLOEXEC, actual process-local RLIMIT_NOFILE exhaustion to
EMFILE, 2,000 copies while exhausted with zero dup attempts and no new runtime
fds, borrowed transport lifetime, intermediate EAGAIN versus final EOF, and
exact last-close/fd/allocation balance. Four independent-owner threads perform
720,000 clone/drop operations per optimization across success and failed handles.
Main drops its original owners before the threads proceed; every success round
closes exactly two shared states after final thread releases. Failed owners
allocate independent wrappers and never close a descriptor.

Executed strict C11 -Wall -Wextra -Werror -pedantic O0/O2 ASan/UBSan checks pass.
The positive modes print `TCP shared copy policy passed`, with zero outstanding
tracked native owner/shared allocations and runtime fds. Separate subprocesses
for wrapper allocation failure, shared-state allocation failure, successful-copy
allocation failure and reference-count overflow exit 1 with the exact fatal
message and no stdout. Results are tcp-copy-policy/acceptance-results.json.
Both positive stress runs finish within the 30-second bound. ASan leak detection
was disabled in these manual probes; explicit counters verify normal cleanup.
No ThreadSanitizer claim is made. The draft runtime also compiles as a standalone
strict C object. The Rust acceptance test syntax compiled with the existing
library but no Cargo test was run.

Reproducible bounded compile command from the root, once per -O0/-O2:

```sh
clang -std=c11 -Wall -Wextra -Werror -pedantic -pthread -O0 -fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer -I runtime build/backlog/safety-errors/tcp-copy-policy/draft/tests/fixtures/network/tcp_copy_policy.c runtime/core.c -o build/backlog/safety-errors/tcp-copy-policy/probe-o0
```

Run the resulting executable normally and with allocation, allocation-shared,
allocation-success-clone and overflow arguments. Root should integrate only after
independent review and its coordinated loopback gate. Requested gates are new
`tcp_copy_policy`, existing `network_runtime`, `network_deadlines`, `tls_runtime`,
`tls_language`, `typed_task_runtime`, language resource-copy and typed-task tests
in both frontends, with O0/O2 sanitizer/allocation/descriptor accounting. Existing
native network-runtime C fixture changes are included in this patch. Actual
IPv4/IPv6 TCP exchange, TLS handshake, inheritance across exec and compiled Bhasa
task transfer were not executed against this draft by this lane. No build slot
is held; no full acceptance, release or hardware claim is made.

### Independent-review corrections, October 12

The reviewer found two existing listener tests that still permitted only one
allocation. Both exact IPv4/IPv6 success-path budgets in the migrated native
runtime fixture now allow two allocations, owner plus shared state. Both require
budget exhaustion, constructor success and exactly two additional outstanding
tracked allocations before exchange cleanup. Their comments explicitly retain
the address-conversion stack-only requirement. The budget is not disabled or
made unlimited. Other intentional one-allocation failure probes remain intact.

Strict O0/O2 ASan/UBSan builds of core plus the frozen draft net runtime and
migrated existing network fixture pass both listen-on and listen-on-ipv6 modes.
All four return zero with empty stdout/stderr, including actual IPv4 and IPv6
loopback exchange and descriptor/allocation balance. Initial sandbox runs were
denied socket creation; the scoped rerun with loopback access passed. Evidence
is tcp-copy-policy/listener-budget-probes/results.json; sandbox denials remain
separate in sandbox-results.json. Only these corrected modes were rerun; no
duplicate stress or broad Cargo gate ran.

Contextual documentation drafts now replace duplicated-descriptor claims in
network.md, runtime.md, records.md, language.md, IMPLEMENTATION.md and
self-hosting.md, all under tcp-copy-policy/draft/docs. IMPLEMENTATION.md is in
docs, not the repository root. network.md explains independent owner wrappers,
shared atomic descriptor state, fixed copy status, fatal allocation/overflow,
final close/EOF, cross-thread owner rules, unchanged CLOEXEC and the borrowed-fd
restriction. It removes the obsolete claim that TCP has no reference counter
and that clone can acquire an EMFILE error handle. Failed resources have no
shared descriptor state. Other guides state the same shared endpoint/lifetime
semantics without implying a new connection or fallible dup.

The full contextual patch and draft.sha256 are refreshed. The separate
tcp-copy-policy-docs.patch contains only the six guide updates; exact original
guide snapshots and hashes are baseline/docs and documentation-baseline.sha256.
Reviewed runtime/net.c and net.h remain byte-identical to
reviewed-runtime.sha256. No runtime repair was made and no shared files changed.

## First Result-parameter contract — contract handoff only

The proposed next increment begins every direct Result-shaped parameter pending
in a source-defined function body. It reuses S037/BH221 and the existing exact
ok/error, one non-task payload per variant shape, including reversed declaration
order. Concrete generic specializations and ordinary method arguments follow the
same rule. Record self, aggregate-contained Results, unused generic templates,
extern declarations and compiler-generated channel wrappers are outside this
increment. No checker implementation has been drafted or integrated for this
contract. TCP runtime and guide drafts remain frozen for root integration gates.

Scope and semantics:

- Seed each recognized concrete parameter independently at function entry.
  Apply this to every source body, including functions used as dedicated or
  pooled workers: they can also be called synchronously. A worker frame owns
  its copied argument, while its function parameter remains a borrowed slot.
- An exhaustive direct `match parameter`, direct `return parameter`, or whole
  initializer `var/let payload = try parameter` discharges that parameter.
  Return and try acquire the existing independent owned result/payload copies.
  The implicit error-return path from try must check every other pending
  binding, including other parameters. Two sequential tries can therefore fail
  when the first propagates before the second parameter was handled.
- Copies create separate obligations. Matching a copy, passing the parameter to
  a helper, reading it, or storing it does not discharge the original parameter.
  Passing a bound caller Result still leaves that caller binding pending even
  when the callee handles its own borrowed parameter. No call summaries, owned
  transfer, last-use analysis or alias-based discharge are introduced.
- Use the existing conservative branch joins, reachable exit checks and zero-
  iteration loop treatment. Handling only inside a loop cannot clear the entry
  obligation after that loop. Include explicit return, fallthrough and implicit
  try error exits. Existing local shadowing rules must preserve outer obligations.
- Pending state is independent of ownership and binding mutability. Host owned
  parameters remain in borrowed_params; native parameter ownership stays zero.
  Rebinding a borrowed Result continues to report S021/BH201. Do not free the
  caller's parameter storage at scope exit or convert parameters into owned
  locals. Existing contents mutation and independent match/try payload copies
  retain their current behavior; this is not a new deep-readonly type contract.

Observed baseline evidence is isolated in
build/backlog/safety-errors/result-parameters. probe.py records 23 sources checked
through each pinned existing frontend: 46 bounded checks, 36 accepted and 10
rejected. results.json preserves full original-file JSON diagnostics and both
binary hashes. manifest.json distinguishes proposed outcomes from observations.
The std probes import exact snapshots of std/generic.bh and std/text.bh inside
the fixture package; relative imports cannot escape that package root. Their
hashes are preserved separately. These are baseline checks, not passing tests
of an implemented parameter feature.

Both frontends currently accept a fresh R.error passed to an empty callee, a
parameter forwarded to a helper without direct handling, a parameter whose copy
alone is matched, a method's discarded Result argument, a Result worker input,
a zero-iteration handling loop, a branch with an unhandled early return, and two
sequential parameter tries. The proposed contract rejects those bodies. Direct
parameter match/return/try and generic identity are accepted now and should stay
accepted. Caller-bound discard already reports S037/BH221. A nested Result
payload bound by a match already reports the same codes when dropped. Record
contents and unused generic bodies remain outside scope and must not be claimed
as fully protected from discarded failures.

Compatibility audit and migration:

- Current std/io, std/result, std/text, std/json, std/http and packages/tls public
  helpers primarily return Result while taking buffers, scalars, Text, sockets
  or records. The scanned current signatures have no direct concrete Result
  parameter requiring a migration. Generic stream arguments must still be
  classified after concrete specialization; ordinary supported sockets are
  unaffected. A generic helper instantiated with a Result cannot receive a
  blanket exemption merely because its template parameter is T.
- The exact std option_or[T] snapshot accepts T=R today. On Option.some it
  returns the contained value while discarding its Result fallback. That
  specialization must reject. Migrate a Result fallback API by exhaustively
  matching the fallback on every branch that discards it, or redesign the
  helper so fallback construction is only needed on the none path. An empty
  exhaustive match is the existing explicit acknowledgement convention; no
  unchecked discard primitive is added. Scalar/Text fallback cases stay valid.
  Box[Result] self/content remains outside this increment.
- Direct try helpers in error_propagation_reference (forward, measure),
  error_propagation_bhasa (take/as_bytes), and immutable_bindings_bhasa (forward)
  already discharge their input; exhaustive readers do too. The native nested
  helper and host immutable propagate helper handle their Result input only
  inside loops, then have a fallback return. They need explicit handling on
  that fallback path, or an entry exhaustive match before the loop. Preserve
  the inner try operations so the original loop/arm cleanup coverage remains.
  Do not replace those cleanup tests with a try solely outside the loop.
- Opaque private Result APIs need a callee capable of directly handling its
  private parameter. A fresh inline argument can then pass this slice. A bound
  opaque caller Result still cannot be discharged by passing it; the earlier
  documented restriction remains. This contract does not restore compatibility
  for every opaque public API.
- Channel source-lowering.md specifies borrowed wrapper inputs and independent
  payload copies. Generated wrappers have no ordinary source body to inspect.
  A direct Result sent into a queue is a channel/aggregate-content boundary,
  outside this source-parameter increment. Keep that boundary explicit; do not
  infer that queueing handles the input failure. A bound caller Result stays
  pending after send. A received ReceiveResult must be handled under the existing
  local rule; its success Result payload becomes pending when locally bound.
  Rejected send payloads are nested inside SendFailure and need explicit nested
  matching to expose a Result binder. No channel schema, runtime or transfer
  policy changes are proposed here, and no full channel failure-safety claim
  is made. Any requirement to reject fresh Result channel payloads is a separate
  reviewed slice rather than an implicit part of this contract.

Precedence and public enforcement are implementation gates:

1. Run parameter checking only after original parse/type/privacy/worker checks
   succeed. Baseline malformed bodies retain S009/BH202 for unknown calls,
   S021/BH201 for borrowed rebinds, and S020/BH209 for a worker that calls an
   extern. Host ordinary worker validation precedes failure_use, but the module
   pipeline calls check_privacy after sema, so imported privacy precedence also
   needs a reviewed staging decision. Seeding sema parameters alone cannot
   satisfy that gate. Native bh_compile emits bodies before be_check_workers and stops on body errors;
   simply setting pending in be_bind would mask BH209 with BH221. The native
   implementation needs a staged obligation diagnostic or another reviewed
   ordering change, with invalid and valid worker probes proving precedence.
   Errors from other bodies, methods and imports must also win over a new
   parameter obligation. Extending that precedence to all existing local
   failures should be reviewed explicitly if needed; it is not silently done
   by this contract.
2. Recheck the obligation through public host codegen::emit_llvm on a mutated
   CheckedProgram. The standalone rustc probe linked to the pinned existing
   rlib shows parameter-handler removal currently emits successfully, while
   local-handler removal reports S037. Invalid parameter type ID and missing
   signatures both return C001 without panicking. Preserve C001 validation for
   malformed schemas/signatures and add parameter-handler removal rejection.
   Test duplicate/missing parameter names, malformed Result shape/type IDs and
   method arguments before exposing the change. Avoid indexing an unchecked
   parameter enum ID or treating malformed metadata as a valid Result.
3. The native public entry is source/token bh_compile, not host CheckedProgram.
   Seed validated signature parameters on every ordinary definition and method
   path; compiler-issued wrappers remain explicitly separate. Prove source
   rejection through --frontend bhasa and direct bh_compile boundary tests.
   Existing token length/span/EOF checks must retain BH200 on malformed tokens,
   including a source with otherwise pending parameters. No new token-corruption
   probe was executed in this contract-only audit.
4. Before integration, require both-frontend focused probes for direct handling,
   independent copies, caller obligations, all exits/branch joins/loops, multiple
   parameter try propagation, methods, concrete generics, imported UTF-8+CRLF
   original spans, dedicated and pooled workers, and cleanup with owned payloads.
   Parameter diagnostics should point to the original exit/operation and identify
   the pending parameter without raw byte offsets in diagnostic text. Root owns
   Cargo, runtime/sanitizer gates, shared documentation and publishing.

This handoff approves no completed parameter feature. It supplies the proposed
scope, explicit exclusions, observed compatibility changes and required gates
for the next implementation decision. No shared compiler/runtime/test/library
files, other lane drafts, Git state or Cargo builds were changed by this audit.

## Exact host staging seam proposal — awaiting root confirmation

This proposal targets the current alias-aware src/lib.rs, src/sema.rs,
src/failure_use.rs and src/codegen.rs. Exact read-source hashes and five focused
host precedence probes are stored under result-parameters/staging-seam. No source
or test implementation draft has been made for this seam.

Use a separate parameter-only traversal. Keep the existing local-obligation
entry point `failure_use::check(&Program)` and its diagnostics at their current
stage. Add crate-private `failure_use::check_parameters(&Program) -> Vec<Diagnostic>`.
Both traversals may share a walker with an explicit Local/Parameter mode, but
must never seed both kinds together. Local mode retains today's parameter=false
and local Result=true behavior. Parameter mode seeds only concrete direct Result
parameters; var/let/try success/match payload/for element binders are inserted
as nonpending lexical shadows, regardless of Result type. Assignments must not
create local obligations in parameter mode or invoke the local outer-loop
assignment refusal. Return, whole try, exhaustive match, branch joins and exit
checks retain the same traversal. Copy handling is therefore independent:
the local pass checks the copy, and the parameter pass checks its source.
Neither pass needs caller summaries, an AST field or ownership changes.

Proposed source APIs and exact ordering:

1. Extract today's complete sema::check body as
   `pub(crate) fn check_before_parameters(Program) -> Result<CheckedProgram, Vec<Diagnostic>>`.
   Keep enum normalization, record/enum/function/method declarations, every
   function and method body check, resolved call-graph construction, all worker
   checks and the existing local failure_use::check call in that body unchanged.
   It returns Err on any existing error anywhere in the program. Public
   `sema::check` calls this function, then check_parameters on the returned
   CheckedProgram, returning the same CheckedProgram only on success. Its raw
   Program API has no module ownership metadata; it cannot reconstruct import
   privacy. Existing direct sema callers keep their signature and receive the
   new obligation after all existing sema/local checks.
2. In ExpandedFiles, extract the current check into
   `fn check_before_parameters(&self) -> Result<CheckedProgram, Vec<Diagnostic>>`.
   Call sema::check_before_parameters(self.parse()?), using the existing
   display_diagnostics mapping, then check_privacy with the existing generic
   original-name callback. Public-to-the-crate ExpandedFiles::check calls that
   helper and only then check_parameters, mapping new diagnostics through
   display_diagnostics. This stages complete parse/type/worker/local checks,
   then all module privacy checks, then all new parameter errors. Preserve the
   old local-before-privacy ordering. bhasa::check/check_files, driver Rust
   check and compile_to_llvm already route through ExpandedFiles::check; no
   caller should call public sema::check and then privacy for this module path.
3. One native dispatch call site must deliberately use the prior-only helper:
   compile_files_with_bhasa currently calls expanded.check() when
   has_member_privacy() is true. Change that privacy preflight to
   expanded.check_before_parameters(). Otherwise the new host pass would leak
   S037 into native diagnostics and could mask native BH209/BH221 before native
   compilation. This is a host dispatch preservation change, not a native
   parameter implementation. Leave bhasa_lexer::compile and all native/channel
   drafts unchanged. The native lane owns its separate late-stage composition.
4. Public codegen::emit_llvm keeps today's failure_use::check first and returns
   old local errors immediately. Next call emit_module and retain its returned
   String privately. That existing operation validates target, declarations,
   type IDs/layouts, function/method parameter names/types, exact signatures,
   every function/method body and generated task/aggregate helpers. Only after
   emit_module succeeds run check_parameters; return Err for new obligations,
   otherwise return the saved IR. This avoids a new partial metadata validator
   and makes an existing C001 from a later body/signature win over a new
   parameter error in an earlier body. It produces a temporary IR String on
   rejected parameter programs; there is no stdout, file, Clang or runtime
   side effect. Do not call emit_llvm recursively from the parameter pass.

The emission order deliberately preserves old local failures ahead of C001,
as today. C001 must precede only the newly added parameter failures. Moving
validate_declarations or every body validator ahead of the old local pass would
change prior precedence and is outside this proposal. Public emission has no
ExpandedFiles module-owner information; source privacy is enforced by the source
pipeline, not newly reconstructed from mutated CheckedProgram. It still must
refuse malformed metadata before generating or returning IR with a new pending
parameter. A well-formed non-Result enum shape remains outside the rule; malformed
names, field types and nominal IDs are metadata errors, not a new Result shape.

Current concrete phase boundaries are sema.rs check (all bodies and graph),
lib.rs ExpandedFiles::check (privacy), lib.rs compile_files_with_bhasa (conditional
host privacy preflight), codegen.rs emit_llvm (old local pass), and codegen.rs
emit_module/validate_declarations (all emission validation). Aliases lower before
these stages and normalize to concrete Enum IDs; classify those IDs instead of
matching alias spellings. No signature table format or borrowed parameter cleanup
change is needed. Generated channel wrappers keep the previously documented
source-body exclusion.

Required host migration set, separate from the seam itself:

- tests/failure_use_reference.rs and tests/type_alias_failure_use.rs use empty
  observe(Result) helpers in otherwise accepted fixtures. Make those helpers
  exhaustively match their argument; caller-bound rejection cases must still
  reject the caller. The native tests/failure_use_bhasa.rs has the same prelude
  and an explicit old parameter exemption assertion; report that to the root/
  native lane without editing its draft. The exemption assertion becomes a
  parameter-rejection assertion when native integration is ready.
- The previously identified host immutable propagate loop needs an explicit
  parameter match on its fallback path or at entry, while retaining the inner
  try for cleanup coverage. The native nested cleanup fixture remains native-
  lane/root migration work.
- tests/error_propagation_reference.rs and tests/lsp.rs include contextual
  functions named try called with a Result parameter, then try on that call's
  return. That handles the returned temporary rather than the original input.
  Add explicit original-input handling while preserving contextual-name/try
  coverage; do not exempt a call because its callee returns its argument.
- std option_or[Result] remains a documented rejected specialization pending
  an explicit branch-handling API migration. Opaque consume already directly
  matches its parameter and needs no helper-body migration. Generic identity,
  direct try/match readers and aggregate-only inputs retain the earlier scope.

Five actual host baseline probes demonstrate the precedence to preserve:
parameter discard alongside a later unknown call reports S009; alongside a
later unhandled local reports S037; alongside a later unsafe worker reports
S020; alongside an imported private field access reports M005. Combining the
old local error with private access still reports S037, proving old local
precedence must remain unchanged. The pinned binary identifies the observed
baseline; current-source hashes identify the seam target. These observations
do not claim the pinned binary includes the root's latest alias commit.

After root confirmation, draft only the four host files above plus focused
host tests and required host fixture migrations, under a new isolated draft.
Prove prior-only APIs cannot leak into public host acceptance, shared native
privacy preflight cannot run new host parameter errors, all original diagnostics
win across later bodies/methods/imports, aliases classify correctly, and public
AST mutation rejects removed handlers while malformed metadata stays C001.
Root owns Cargo/broad verification and integration. No native implementation,
channel composition, TCP change or Git operation is part of this proposed delta.

## Approved host Result-parameter delta — frozen isolated handoff

Implemented the root-confirmed seam in
build/backlog/safety-errors/result-parameters/host-delta/draft. Only four host
source files are drafted: src/failure_use.rs, src/sema.rs, src/lib.rs and
src/codegen.rs. The contextual host-source.patch, host-tests.patch and combined
host-parameters.patch are frozen alongside original relative-path baselines,
baseline.sha256 and draft.sha256. source-drift.json is empty: each captured
baseline still matched the shared source at freeze time. TCP draft/guide hashes
were verified unchanged.

failure_use now has separate Local and Parameter traversal modes. Existing check
uses Local mode with nonpending parameters. New check_parameters seeds concrete
Result parameters and inserts all ordinary binders as nonpending shadows. It
shares direct match/return/try, branch, loop and exit logic while never creating
local obligations or local outer-loop-assignment refusals in Parameter mode.
Copies are checked by the prior Local pass; their source parameter remains an
independent obligation. Alias normalization uses concrete Enum IDs. S037 remains
the host diagnostic; native BH221 code was not changed. Existing message wording
and local ordering are retained.

Public sema::check delegates to crate-private check_before_parameters, then the
new pass. The prior checker still completes declarations, every ordinary and
method body, worker graphs and local obligations before returning. ExpandedFiles
uses that prior checker and completes module privacy before checking parameters.
Its native privacy preflight deliberately calls only check_before_parameters,
so it does not introduce host parameter errors into native dispatch. Public
emit_llvm keeps the old Local pass first, privately finishes emit_module's target,
metadata, body and helper validation, then checks parameters before returning IR.
This preserves old local precedence and makes existing C001 beat new parameter
errors without extracting a partial validator. Parameter ownership, signature
format, borrowed cleanup and runtime lowering are unchanged.

Six host test drafts accompany the sources. The new result_parameters_reference
suite covers direct/fresh parameter handling, copies/callers, implicit try exits,
shadows, zero loops, methods, concrete generics, alias chains, dedicated/pooled
workers, public sema, removed CheckedProgram handlers, malformed enum/type/
signature/parameter metadata, and older errors in later bodies/methods. Its
module tests verify imported privacy first and UTF-8+CRLF generic parameter
locations at the original library return (line 4, column 1). Existing host
failure_use and alias preludes now explicitly match observe's parameter.
Immutable propagate handles its fallback parameter while keeping the inner-loop
try. Contextual try and LSP fixtures handle the original input before calling
the function named try, preserving their original coverage. No native test or
channel draft was edited.

Verification used standalone rustc, not Cargo. probe-tree contains snapshots of
supporting host/runtime include sources with the four draft files overlaid;
probe-source.sha256 pins that exact compilation input. The isolated library and
CLI compiled successfully. All 23 contract matrix sources produced their proposed
host acceptance/rejection outcome, including the std generic fallback and Box
snapshots. Ten focused parameter tests pass. All eight existing host local-
obligation tests and three alias tests pass. The migrated contextual-try checker
and LSP semantic-token tests each pass. The migrated immutable owned-loop test
file syntax-compiles; its execution and sanitizer gate remain with root.

Evidence includes matrix-results.json, per-suite compile/test logs, verify.py,
binary/evidence hashes and the frozen patches. The final source rebuild only
incorporated comment clarifications after the passing checks. The feature-gated
native preflight call site was inspected, not executed through a new native
build; root owns native/channel composition and that gate. No shared source,
other-lane files, Cargo build, Git operation, commit or push occurred. This is
ready for root review/integration as a host-only delta, with the earlier aggregate,
channel-wrapper, unused-template and interprocedural exclusions intact.
