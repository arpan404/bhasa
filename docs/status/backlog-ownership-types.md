# Ownership and core types lane

October 11, 2026. The first draft implements transparent non-generic type aliases
in shared module lowering. Ownership transfer and read-only borrowing contracts
are below. Binding-only `let` remains reserved for coordinator integration.
No tracked compiler source was changed. No Cargo, native frontend build, broad
suite, Git mutation, SDK installation or cache cleanup ran in this lane.

## Updated handoff after contract approval

The coordinator approved the bounded alias slice and requested the unused
concrete-target validation gate. That gate is now implemented in the isolated
draft. Ownership/move/borrow implementation remains unapproved and was not
started.

- `build/backlog/ownership-types/draft/src/modules.rs` collects, resolves and
  substitutes aliases, preserving source-only declaration/use metadata.
- `build/backlog/ownership-types/draft/src/generics.rs` adds `TypeRoot` and
  `expand_with_type_roots`. Alias targets seed the same specialization identity
  table as ordinary source uses. The existing semantic checker/native checker
  sees the concrete target declaration even when the alias is unused.
- `build/backlog/ownership-types/draft/src/lib.rs` is a small host integration
  change: obtain module type roots and pass them into generic expansion.
- `build/backlog/ownership-types/draft/tests/type_aliases.rs` has seventeen alias
  acceptance groups, including unused generic rejection, original RHS spans,
  imported privacy, generic identity and source-only editor metadata.
- `build/backlog/ownership-types/type-aliases-current.patch` is the complete
  contextual patch against current integration sources. Individual patches are
  `modules.rs.patch`, `generics.rs.patch` and `lib.rs.patch` in the same directory.
  Copy the new test file separately. The old `type-aliases.patch` is historical;
  do not apply both patches.

Exact current baselines are `baseline/modules-current.rs`, `baseline/generics.rs`
and `baseline/lib.rs`; `current-baseline.sha256` records their source hashes plus
AST/parser/checker hashes. The source snapshot includes the coordinator's
`contextual_binding_markers` and bounds repairs. These helpers remain unchanged
in the new patch. The old `baseline/modules.rs` and `baseline.sha256` describe
only the first draft. Apply contextual diffs, never replace whole shared files.

The bounded probe passes twenty-one checks: seventeen alias groups, two unchanged
propagation-marker tests and two lowered-fixture groups. It compiles only the
drafted modules/tests with rustc and the existing cached Bhasa library, leaving
shared compiler caches untouched. `probe-build.log`, `probe-test.log` and
`evidence.sha256` record the artifacts. Replay with
`python3 build/backlog/ownership-types/run-probe.py`.

The existing hybrid release accepts three lowered fixtures for owned records,
generic records and List aliases. It rejects a seeded unused generic record
with a void field, using BH201 at the field name. `native-probe.log` records
these checks. The fixtures rename reserved generated prefixes so the existing
host can parse them as ordinary source. This confirms native acceptance/rejection
of lowering output, not a refreshed compiler containing the patch. No Cargo,
compiler/native archive rebuild, Git operation or broad suite ran in this lane.
Rustfmt checks pass for all four source/test drafts; lib formatting uses
`skip_children=true` because the isolated tree contains only changed files.

Unused `Map[buffer,int]`, `List[void]`, generic record/enum void arguments and
hard-coded invalid generic fields reject. Intrinsic failures point to the
original alias RHS, including UTF-8 prefixes, preceding namespace expansion
and imported-file offsets. Target schema errors retain original template field
spans. Multiple aliases to one target share one concrete target ID and introduce
no alias nominal ID or runtime wrapper. Existing restrictions on generic void/
task arguments and explicitly annotated task handles remain intact.

Integrated Rust/full-hybrid Cargo gates, refreshed native parity, alias-plus-let
cases, original-file native failures, ownership execution and cache/self-rebuild
acceptance remain coordinator work. The draft is ready for those gates; it is
not an integrated feature.

## Draft frozen for independent review

The alias draft is now frozen at the exact hashes in
`build/backlog/ownership-types/evidence.sha256`. The final bounded probe passes
21 functional checks plus a separate allocation regression, and formatting passes. The complete current-baseline patch applies
cleanly and reproduces the draft bytes. No changes will be made to these draft
sources/tests until the reviewer or coordinator requests a further repair.
Shared source hashes still match `current-baseline.sha256` for modules, generics
and lib at this freeze. The frozen proposal changes only alias behavior;
owned/readonly/move implementation remains unratified and absent.

All earlier independent-review findings and the later edit-list allocation finding
are repaired. Alias `main` is now renamed
like any other type alias; only an actual owner-zero Def named main preserves
the entry name. Canonical alias substitution is performed even if an internal
name happens to match its source name. A library probe with `type main=int` and
`identity(x:main)->main` passes without creating a main function or nominal alias.

Alias RHS generic arguments now accept one trailing comma before the closing
bracket, consistent with ordinary `Box[int,]`, `List[int,]` and `Map[int,str,]`.
The canonical target drops that comma, so direct and aliased types share identity.
Too few or excess arguments still reject before excess resolution/append.

`alias-review-fixes.patch` contains the combined append-bound, main-alias,
trailing-comma and edit-sharing repairs against `baseline/modules-before-append-fix.rs`. It is
already included in `type-aliases-current.patch`. Use the combined delta if the
coordinator applied the earlier alias draft; use the full patch if alias work is
still absent from shared sources. Copy the refreshed test file in either case.
The older `alias-bounds-fix.patch` contains only the first bounds repair and is
retained as evidence; it is not the complete final review repair.

## Alias use-site edit allocation repair

The later independent review identified that use-site replacement clones could
accumulate in the edit list before the existing 128 MiB rendering cap applied.
The draft now stores `Cow<str>` edits. Alias and namespace references borrow
canonical strings from the immutable resolution/module tables until rendering;
they no longer retain a separate expanded target string per use. Erased aliases
and private-modifier whitespace remain owned, bounded by their original source
spans. Span sorting, linear erasure mappings and the existing early output append
cap are unchanged. Output content and source-map functional tests still pass.

`draft/tests/type_alias_allocations.rs` is the standalone allocator regression
for integration. It measures requested live Rust allocation bytes, not allocator
internal overhead, C allocations or physical process memory. A 2,997-byte source
with 128 references to a large cached alias produces 4,718,782 rendered bytes
with a 4,772,480-byte output capacity. The exact pre-repair module fails the
regression at 9,791,726 extra peak requested bytes; the repaired draft passes at
5,075,042 bytes. The assertion separates required output capacity from a 2 MiB
metadata allowance, so the old per-use clones fail without a large stress run.
The isolated measurement is finite evidence, not a general compiler-memory or
runtime benchmark.

`allocation-probe.log` records the expected pre-repair failure and post-repair
success. Replay with `python3 build/backlog/ownership-types/run-allocation-probe.py`.
Both processes are bounded and compile only isolated passes/tests with cached
Rust library artifacts. The existing 21 functional checks and three native
acceptance/one rejection probes still pass. No Cargo, cache mutation or compiler
archive build ran.

`alias-edit-sharing-fix.patch` is the narrow delta against
`baseline/modules-before-edit-sharing.rs`. The refreshed `alias-review-fixes.patch`
combines all review repairs against the earlier pre-review snapshot, and
`type-aliases-current.patch` remains the complete current-shared-source patch.
Apply the appropriate single source patch and copy both refreshed test files.
The draft is frozen again at `evidence.sha256` for independent confirmation.

## Alias append-bound review repair

Independent review identified that the resolver previously appended every generic
argument before checking arity and the 64 KiB spelling limit. This could amplify
an invalid argument list through repeated references to cached large aliases.

The draft now rejects an excess argument before resolving it or appending its
separator. `append_alias_spelling` uses checked cumulative length before every
opening bracket, argument, separator and closing bracket append. The existing
final spelling check remains as a guard on base type names. No oversized
assembled spelling is constructed before rejection.

Two bounded regressions cover the fix. `Map[int,str,Missing]` must fail with an
arity error at Missing before unknown-target resolution. A compact Pair alias
produces a cached large target, followed by 4,096 references in excess Map
arguments; rejection occurs at the first extra reference. A separate legal-arity
Pair expansion fails the spelling cap before appending the oversized second
argument. All twenty-one isolated checks and existing native lowering probes pass.

`alias-bounds-fix.patch` is the small repair relative to
`baseline/modules-before-append-fix.rs`. It is already included in the refreshed
`type-aliases-current.patch`; apply one appropriate patch, not both. Both exact
baselines and current draft hashes are in `evidence.sha256`. The complete patch
still reproduces all three draft source files from their current-source baselines.
No coordinator let/contextual-marker/bounds helper was modified.

The reviewer owns `build/backlog/ownership-types/independent-review.md`; this lane
does not edit that report. Integrated Cargo and fresh native acceptance remain
coordinator gates.

## Separate pending-result alias acceptance delta

The implementation remains frozen. Independent review confirmed all 22 prior
source/graph cases and the bounded Cow edit fix with no remaining alias-specific
finding. Module SHA-256 remains
`5a9017e6469f897c329f3bd3f7612a515fa2779b6b233c3101dc9aa66f36b23a`.

`draft/tests/type_alias_failure_use.rs` is a separate three-group acceptance
increment. `alias-failure-use-tests.patch` adds only that test file. It covers
direct result aliases, aliases to concrete generic results and imported aliases.
Bound unhandled results, ordinary observation and handling only a copied result
remain rejected. Direct exhaustive match, return and whole-initializer try remain
accepted. Variant order is reversed, aliases share the target's canonical result
shape and the tests use immutable bindings without changing their semantics.

`pending-aliases/run.py` uses the existing frozen alias lowerer only to produce
actual lowered sources, then checks those sources with current pending-aware
`target/debug/bhasa` and the existing native
`build/backlog/native-compiler/pending-stage-a`. All 19 cases agree in both
compilers, for 38 bounded checks: pending rejections retain S037 and BH221.
Reserved generated prefixes are replaced with same-length safe prefixes before
feeding the already-lowered source to another host. This preserves byte offsets
and nominal/result identity within each probe. No private members force reference
checking before native rejection. Source, lowered-source hashes, complete
outputs and executable identities are retained in `pending-aliases/results.json`.

The existing reviewer lowerer was linked to an older checker. Its lower mode is
suitable for these fixtures; its check mode does not prove current S037. The new
three Rust acceptance groups were therefore compiled as an isolated test wrapper
against the current cached pending-aware library, using unchanged frozen alias
passes. All three pass; old source-local unit tests were filtered out. This builds
only a test executable, not a compiler or native archive. Evidence is
`pending-aliases/acceptance-build.log` and `acceptance.log`; replay with
`python3 build/backlog/ownership-types/pending-aliases/run-acceptance.py`.

Original-source reference diagnostics are checked in the imported library after
UTF-8 comments and CRLFs. Explicit return reports S037 at library byte 167..175,
line 7/column 1; implicit try error return reports byte 190..193, line 7/column 11.
Messages retain `pending` and contain no generated names. Native feature-gated
assertions require BH221 and the same original file/start positions through the
integrated public host pipeline. The existing seed checks prove BH221 on lowered
sources; fresh integrated original-file native mapping remains a coordinator gate.

Only new test/delta/evidence hashes were written to
`pending-aliases/test-evidence.sha256`; the frozen implementation manifest and
source files were not modified. Coordinator commands after alias integration:

```
CARGO_BUILD_JOBS=2 cargo test --test type_alias_failure_use -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test type_alias_failure_use -- --test-threads=1
```

No Cargo, compiler rebuild, Git mutation, shared-source edit or ownership/read-only
AST implementation ran. The editor lane's alias metadata increment is separate.
Owned/readonly/move mode ratification remains with the coordinator, including
its interaction with pending local result obligations.

## Alias contract

`type Name = Target` is a contextual top-level declaration. It accepts a scalar,
owned builtin, record, enum, another alias or concrete generic target. Generic
alias parameters are excluded. Declarations terminate at newline, semicolon or
file EOF. Targets currently occupy one logical token line. An ordinary function
or local named `type` keeps its existing meaning.

An alias introduces no nominal ID, runtime layout, constructor wrapper, cleanup
function or copy policy. `type IntBox = Box[int]` and `Box[int]` feed the same
canonical spelling to generic specialization. Annotation, constructor, enum
variant and generic argument references expand to the original concrete target.
Normal local and generic parameter shadowing remains in force.

Alias visibility follows module exports, qualified namespaces and `private`.
Resolve the target in the defining module, before substitution at a use site.
Preserve nominal owner names so private fields, methods, constructors and opaque
enum propagation retain existing checks. Public aliases cannot expose a private
top-level declaration, including through a private alias or generic argument.
A public record with private fields can have a public alias; construction and
member access still require its defining module or public methods/factories.

Validate every alias even when unused. Reject unknown or non-type targets,
duplicates with any declaration, cycles, incomplete generic arguments, generic
arguments on a concrete alias, malformed target syntax and reserved names.
Resolution/nesting is bounded at 64; expanded target spelling is capped at 64 KiB.
Canonical target strings have a cumulative 16 MiB cache budget to stop alias
chains from amplifying a small source into unbounded metadata. A focused compact
Pair/repeated-alias fixture verifies rejection at that budget.
Module output keeps its existing 128 MiB limit and source mapping. Diagnostic
locations point to the declaration target or original use, never generated names.

The implementation lives in the existing shared host pipeline. The Rust host is
accepted architecture; this supplies both frontends without changing native token
codes. Standalone raw native scanner/emitter entry points do not acquire aliases.
Alias symbol navigation, API inspection and formatter presentation still need
coordinator/editor integration before claiming full editor support.

## Ownership transfer contract proposed for ratification

Preserve current unqualified parameter behavior. Ordinary owned-container
parameters synchronously borrow caller storage, may mutate its contents and
cannot rebind their parameter handle. Returning a borrowed value clones it into
an independent owned result. Ordinary binding/assignment from an existing owned
value still copies; fresh owned call results retain existing acquisition rules.
Method `self`, argument overlap/invalidation checks and typed-task submission
copies keep their current behavior. The earlier proposal incorrectly described
ordinary function parameters as copies; that statement is withdrawn.

Propose additive contextual parameter forms `owned value: T` and
`readonly value: T`. Ordinary parameters named `owned` or `readonly` remain valid.
No modifier means the existing mutable synchronous borrow for an owned type.
The explicit owned mode transfers cleanup responsibility to the callee; the
explicit readonly mode grants a read-only synchronous loan. Neither mode changes
unqualified function signatures, scalar parameters, existing call syntax or
the existing unqualified-call ABI.
New signatures need mode metadata in both frontends and their call/return checks.

Add explicit `move value` only for a whole binding initializer, owned call
argument or return expression. Start with initialized local owners and explicit
owned parameters. Moving an ordinary borrowed parameter or readonly parameter
rejects because the caller retains ownership. A caller can bind an independent
snapshot from a borrowed value and then move that snapshot. Exclude projected
moves and implicit last-use moves. Task-handle consumption remains unchanged.

Baseline evidence, read directly for this correction:

- `docs/language.md:87` and `:119` document synchronous mutable borrowing,
  deep-copy binding/assignment, copied borrowed returns and record overlap rules.
- `docs/runtime.md:28` states that borrowed argument contents may mutate, while
  parameter handles cannot rebind and returned borrowed values are independent.
- `src/sema.rs:651` records owned-type parameters in `borrowed_params`;
  `:504` rejects overlapping records and later arguments that invalidate an
  earlier borrow; `:759` identifies the original parameter binding under shadowing.
- `src/codegen.rs:1024` binds function parameters without owned cleanup;
  `:1195` clones borrowed owned values when acquired; `:1310` rejects corrupt
  checked-AST rebinding of a borrowed parameter.
- `tests/owned.rs:9` mutates the original buffer through `changed(value)` and
  returns a copy. Its expected output at `:51` distinguishes the caller's
  `hello!` buffer from the independently changed `hello!?` result. This is
  existing test evidence, not a newly rerun sanitizer gate.

Moving transfers one live owner without clone or allocation. The source becomes
uninitialized and loses its cleanup entry only after successful argument
preparation. Owned parameters are initialized owners and drop on normal return,
early return and propagation. An explicit `return move value` transfers the
callee's owner to the caller's result slot. An ordinary `return value` retains
existing acquisition/copy behavior, including copied borrowed returns. Fresh owned results may initialize owned parameters without
an extra copy. Self-move assignment and duplicate moves in one call reject.

Use-before-initialization and use-after-move reject. A mutable binding may receive
a new value after a move. An immutable binding cannot be reinitialized. Branch
merges require a source initialized on every incoming path before reading it;
loops must preserve initialized state at every backedge and continue path.
Moving a source with an active loan rejects. Exception unwinding is absent;
existing checked arithmetic/runtime traps retain their current process policy.

The checked AST must carry parameter modes and explicit transfer operations.
Both checkers and emitters must independently reject corrupt ownership metadata.
Track initialized state separately from binding mutability and task consumption.
Lower transfer by clearing the source cleanup obligation and adopting its storage
in the destination. Preserve existing deterministic no-GC layout/drop routines.

## Read-only borrow contract proposed for ratification

A proposed `readonly value: T` parameter observes caller-owned storage during a
synchronous call. The proposed caller marker is `borrow local` at the argument.
Multiple read loans are allowed. Reads and independent copies are allowed; writes, reassignments, moves,
mutable projections and mutating calls through the borrowed value reject.
Any loan of a projection conservatively freezes the entire root owner initially.
Argument loans begin as arguments evaluate left to right and end when the call
returns. Reject a later argument that moves or mutates an earlier borrowed root.

Read-only loan identities cannot be stored in fields/containers, returned as
references, captured, spawned, sent to a worker or passed to asynchronous work.
Independent copies of their contents remain allowed. Returning a readonly
parameter as its owned nominal type follows existing acquisition and clones it;
no borrowed return type is introduced. Record construction and storage must
acquire independent owned copies rather than retain the loan. The first increment
exposes no borrowed local declaration or borrowed return. General lifetime
analysis and explicit mutable-borrow syntax remain later increments; ordinary
mutable synchronous borrowing remains supported.

A readonly parameter may forward to another readonly parameter. It cannot
forward its loan to an ordinary owned-container parameter because that callee
may mutate caller storage. To call such a function, bind an independent local
snapshot first, then pass that snapshot using existing mutable borrowed calls.
Direct mutation builtins and unknown extern calls cannot receive readonly-loaned
storage. Method receivers need an
explicit read-only mode and transitive checked call-graph effects; existing
mutating receiver semantics cannot be relabeled read-only by the caller.
Network/shared-state handles are excluded from the initial read-only borrow set:
shallow C handle aliases cannot prove read-only effects on shared endpoint state.

Acceptance must first preserve existing mutable borrowed calls, borrowed
parameter rebinding rejection, independent returned copies, overlapping record
arguments and typed-task submission copies in both frontends. New mode tests
must cover readonly-to-ordinary-call rejection, mutation of explicit snapshots,
aliasing argument order, nested/recursive calls, shadowing, branches/loops,
early return/try cleanup, destructor counts and zero-clone owned transfer at O0/O2. Both frontend tests and corrupt-checked-AST tests
are required. Borrowed returns and thread transfer remain unchecked backlog work.

## Coordinated verification request

After merging the patch into a suitable integration snapshot, run serially:

```
CARGO_BUILD_JOBS=2 cargo test --test type_aliases --test type_alias_allocations -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test type_aliases --test type_alias_allocations -- --test-threads=1
```

The native feature invocation must use the project's existing configured hybrid
build environment and a refreshed matching archive. No worker will start these
commands while combined gates are running. Broader module/generic/privacy/editor,
LLVM ownership, self-rebuild and cache-reuse checks remain coordinator gates.

The previously open unused-alias semantic criterion is addressed by explicit
generic roots. Do not add synthetic validation functions or alias record wrappers.
The concrete target declarations are the same declarations a normal use would
instantiate, and normal checker/ownership/privacy validation remains authoritative.

## Editor and API metadata contract for routing

`modules::Expansion.type_aliases: Vec<TypeAliasInfo>` is sorted by original
source declaration position. Each entry contains `name`, `internal_name`,
`canonical_target`, `owner`, `private`, `span`, `name_span` and `target_span`.
`owner` indexes the original loaded `SourceFile` list. All spans are original
virtual source offsets. The declaration span starts at the contextual `type`
marker; privacy is a separate flag. `internal_name` is a source-only module
identity, never a nominal AST type ID. Use it to connect definitions and uses
within a loaded graph; do not persist it across different graph orderings.
Display `canonical_target` through `modules.display_names` for user-facing text,
while retaining its concrete nominal identity internally.

`modules::Expansion.type_alias_uses: Vec<TypeAliasUse>` is sorted by original
source position. Entries contain `internal_name`, `span` and `type_context`.
This includes annotations, constructor/enum references, generic arguments and
alias RHS dependencies. Namespace-qualified uses span the whole reference.
For rename, edit the final identifier inside a qualified span; preserve its
namespace. Protected field/method labels, ordinary locals and generic parameter
shadows are excluded by normal module binding rules. Alias constructors have
`type_context=false` but still reference the alias declaration for navigation.

`modules::Expansion::type_roots()` returns a fallible vector of
`generics::TypeRoot { spelling, span }`. These spans are lowered offsets solely
for composing generic diagnostics; editor callers should use the original
metadata spans above. `generics::expand_with_type_roots(source,boundaries,roots)`
is the companion expansion entry point. Existing `generics::expand` retains its
previous behavior by passing no roots.

Route alias definition/reference/hover/rename, document/workspace symbols,
formatter declaration support and inspect API entries through this metadata.
Public/private alias flags and concrete generic target spelling must appear in
API output without inventing runtime functions or fields. Source-only alias
names will not appear in the checked runtime AST, so AST traversal alone cannot
provide those editor features. The included metadata tests cover qualified RHS
dependencies and source identity; full protocol/client/API tests remain with
the editor/coordinator lanes.

Ownership transfer/read-only syntax and ABI need coordinator ratification before
AST/dataflow implementation. The contracts preserve current copies, binding-only
let and typed-task rules. Mutable borrows, borrowed returns, resource destructors,
recursive heap ownership and subsequent type/function features remain owned
backlog work after these prerequisites.

## Owned buffer/ints draft API, October 12

The ratified contract is `docs/status/owned-bindings-milestone.md`. Work is isolated
under `build/backlog/ownership-types/owned-bindings`, with exact source snapshots
in `baseline/src` and their hashes in `baseline.sha256`. The alias draft remains
frozen. Nothing in the shared compiler or Cargo configuration was changed by this lane.

The draft host AST adds `ParamMode::{Borrowed, Owned}`, `Param.mode`,
`FunctionSig.modes: Vec<ParamMode>` and
`ExprKind::Move { name: String, name_span: Span }`. Unqualified parameters default
to Borrowed. The parser recognizes contextual `owned` before a parameter name
and contextual `move` before a whole local name. `move(x)` and a parameter named
`owned` keep ordinary parsing. The native diagnostic counterpart is BH222;
host transfer violations use S038. Initialization and cleanup ownership are
separate from existing reassignability, pending-result and task state.

The pointer calling convention stays unchanged. An Owned buffer/ints argument
transfers its pointer to the callee, which owns cleanup. The caller does not drop
that argument after the call. Explicit move loads the source pointer and marks
the source uninitialized immediately; its lexical cleanup is skipped. A mutable
source may acquire fresh storage in its original straight-line scope. Borrowed
parameters still mutate contents, cannot rebind and ordinary returns still clone.

`ownership::check(&Program)` is a separate lexical pass after ordinary typing.
It evaluates arguments left to right, reserves moves immediately, rejects both
borrow/transfer overlap orders, and rejects moves from an outer conditional or
loop scope. Short-circuit RHS and loop conditions count as controlled execution.
`ownership::check_checked(&CheckedProgram)` runs again at public LLVM emission
and compares signature modes against declarations. This API handoff is available
for native parity now. The isolated core compiles with rustc; functional probes,
independent emitter corruption checks and O0/O2 counters are still in progress.
No native parity, sanitizer or integrated acceptance claim is made yet.

## Owned buffer/ints implementation handoff

The concrete isolated host patch is ready at
`build/backlog/ownership-types/owned-bindings/owned-bindings-host.patch`, with
`owned-bindings-tests.patch`, exact baseline hashes and `draft.sha256` beside it.
Only nine compiler files change, including new `ownership.rs`; no Cargo or shared
compiler mutation occurred. `verify.sh` reproduces the bounded evidence without
building a compiler executable or native archive. The frozen alias modules SHA
remains `5a9017e6469f897c329f3bd3f7612a515fa2779b6b233c3101dc9aa66f36b23a`.

The source probes cover 41 cases. Eight corrupt checked-AST probes reject at the
public emitter. Imported contextual names and parameter shadowing pass, and the
owned lowering patch applies onto copies of the frozen alias draft. An imported
alias resolving to buffer accepts an owned parameter and explicit move. Unused
generic owned declarations reject before erasure. Parameter-mode declaration
validation also precedes ordinary typing, preserving S038 for unsupported types.
The unsupported Result move probe retains S037 pending obligations alongside S038.

Eight runtime executions pass at O0/O2 with address and undefined behavior
sanitizers. Transfer counts are 7 allocations, 0 clones, 7 drops; ordinary copy
counts are 5 allocations, 3 clones, 5 drops. Early-return counts are 3/0/3; try
error/success counts are 7/4/7, retaining existing Result payload/snapshot copies.
All runs finish with zero live allocations inside core.c. macOS sanitizer leak
detection is unsupported, so no general leak-detector claim is made. The checked
assignment `x = take(move x)` reloads initialization after RHS evaluation; its
counter regression passes without destroying the transferred pointer twice.

The supplied `tests/owned_bindings.rs` patch has focused host acceptance and
public-emitter tests plus a feature-gated native BH222 parity test. Only the host
tests have run in the isolated core. Native parser/checker/emitter parity, refreshed
archive, shared editor/API routing and integrated broader/self-rebuild/cache gates
remain coordinator work. This handoff completes the isolated host slice, not the
combined ownership acceptance gate. Mutable/read-only borrows, resource ownership,
Result transfer and tasks remain separate backlog slices.

## Acquired match payload correction, October 12

Prioritized the independent review finding before borrow-mode design. The separate
correction is `build/backlog/ownership-types/match-payload-correction`, frozen by
`correction.sha256`. Original owned host/test patches and all recorded draft hashes
remain unchanged. `host-match-payload.patch` changes ownership.rs match payload
`owner:false` to true, matching Rust codegen's acquire/copy and owned local bind.
Buffer/ints payloads are eligible initialized arm-local owners. Container moves
and conditional/loop moves of an outer arm binding remain excluded.

Thirteen new source probes and the original 41 cases pass in the corrected isolated
core. The focused source/pending test and corrupt checked-AST moved-payload-read
emitter test pass. Pending Result containers still reject with S038 plus S037;
Result payload obligations created by match bindings still produce S037.
Eight O0/O2 address/undefined sanitizer executions pass with zero live runtime
allocations. Direct payload moves and payload returns have counts 2 allocations,
1 acquisition clone, 2 drops; the copied-local workaround has 3/2/3. The transfer
adds zero clones and saves the workaround's extra clone/drop.

Native inspection confirms be_match acquires and owns payloads, then the original
draft explicitly clears transfer-owner at local+10. The comparison proposal removes
that reset while retaining be_failure_bind. The native lane's existing separate
`owned-bindings/correction/emitter-correction.patch` already contains this same
change; do not apply the comparison proposal twice. This lane has not changed native
sources or rebuilt/tested native parity. New tests include a feature-gated native
gate for the coordinator. Shared sources, Cargo and Git remain untouched.

## Explicit borrow contract proposal after payload correction

The concrete proposal is
`build/backlog/ownership-types/borrow-contract/contract.md`, with an audited
`builtin-effects.json`, 44 proposed source acceptance cases, 12 independent emitter
case specifications and seven executed frozen-baseline probes. `proposal.sha256`
records these artifacts. No new mode is implemented. This supersedes the older
unratified caller `borrow local` marker proposal for this first increment.

Proposed ParamMode additions are ReadOnlyBorrow and MutableBorrow, alongside
unchanged Borrowed/Owned; native mode IDs are explicitly 2/3. Signatures keep their
complete mode vector and unchanged pointer ABI. Modes apply only to defined
monomorphic synchronous buffer/ints free-function parameters and canonical aliases.
Readonly forwards only to readonly helpers or validated Read builtin positions;
legacy helpers retain mutation capability even if their current body only reads.
Explicit mutable loans are exclusive. Local/match acquisition copies are writable
owners; neither borrowed mode rebinds or transfers its formal handle.

The main additional proof is provenance. Distinct legacy borrowed parameter names
can alias, so `outer(a:buffer,b:buffer)` cannot prove an exclusive(a,b) call safe.
Locally acquired copies prove disjointness; shared readonly parameters may alias.
Typed call-frame loans must see nested arguments and restore only their own frame.
Existing default aliases and audited intrinsic self-append/self args_push remain
compatible when no explicit conflict participates. The proposal classifies all 55
frozen builtin names, including write destinations hidden behind read_file, UTF8,
GPU and network operations. It does not add resource or task borrowing semantics.

Actual baseline probes confirm legacy aliases/helper mutation and let content
mutation accept; borrowed handle rebinding remains host S021. readonly/mutable
words work as ordinary names, while proposed parameter forms currently reject P001.
These are baseline facts, not new syntax acceptance. The coordinator assigned proposed S039/BH225 access/loan diagnostics; BH223/BH224
are reserved for channel checks. This allocation does not ratify the semantics. Existing mode consistency and transfer
violations remain S038/BH222; immutability and pending obligations remain separate.

Native impact is concrete: retain stride-2 signature pairs and extend the existing
mode side table; split be_has_owned's current nonzero test from an any-explicit-mode
predicate. The current ctx36 [next,local] loan list cannot express kind, call frame
or may-alias provenance. The proposal requires typed loan records, separate access/
provenance metadata and updated header bounds if a new context head is appended.
Ratification of syntax, exclusivity and diagnostic allocation is the next gate;
there is no external dependency blocker. Frozen owned patches and the separately
hashed match-payload correction remain unchanged. Shared sources, Cargo and Git
were not edited.

A second concrete native mode pitfall is included in the proposal: the current
call parser writes parameter_mode directly to ctx33, allow-move, while be_move
checks only zero versus nonzero. Adding mode IDs 2/3 requires an exact Owned
predicate here, or a borrowed argument would accidentally permit move. Only Owned
may clear caller temporary cleanup. The proposed builtins::argument_effects API
returns a complete per-argument Value/Read/Write vector after overload validation;
unknown/invalid entries never imply read permission. No code implementing these
proposed APIs was added. The sole original independent-review P2 is addressed only
by the separately frozen match-payload delta; the reviewer can recheck that delta
against the original frozen source hashes.

## Ratified explicit-borrow host API in progress

Implementation is isolated at `build/backlog/ownership-types/explicit-borrows`,
against exact snapshots of the corrected owned host in baseline/src. Original
owned and match-payload correction hashes remain frozen. The source contract is
`docs/status/explicit-borrows-milestone.md`; no native implementation is started.

The actual draft AST adds ParamMode::ReadOnlyBorrow and ::MutableBorrow. Borrowed
remains the default, and all ownership/adoption checks remain exact Owned equality.
FunctionSig.modes retains one entry per parameter. `builtins::BuiltinArgEffect`
is Value/Read/Write; `argument_effects(name,args)` validates the exact overload and
returns a complete vector or failure, never implicit read permission.

The actual ownership APIs remain check, check_modes and check_checked. New enums
are DataAccess::{ReadOnly,ReadWrite}, Provenance::{Unique(id),LegacyParameters,
SharedParameters} and LoanKind::{LegacyRead,LegacyMutable,SharedReadOnly,
ExclusiveMutable}. Loan records carry root `(scope index,name)`, provenance,
concrete Type, kind and call-frame ID. Analysis clones the inherited loan vector
for each call, so nested calls consult outer loans and discard only their own
frame on return. Distinct legacy parameter names remain possibly aliasing;
local acquisition copies and exact Owned parameters have unique provenance.

The core compiles and 42 direct proposal cases pass. Imported alias/generic
lowering, expanded corrupt-emitter tests, aggregate escape and O0/O2 counter
checks are still in progress. This is an early API handoff, not integrated or
native acceptance. S039 is used for data-access/explicit-loan violations; native
routing is BH225 when that lane begins. S038/S036/S037 and borrowed rebind S021
remain separate. Shared sources, Cargo and Git are untouched.

## Explicit borrow isolated host handoff

The ratified host delta is ready at
`build/backlog/ownership-types/explicit-borrows/explicit-borrows-host.patch`, after
corrected owned host, with separate tests and `draft.sha256`. Seven compiler files
change: ast, parser, sema, builtins, ownership, modules and generics. Original owned
and acquired-payload source/patch hashes are verified unchanged. No shared compiler
source, Cargo, Git or native implementation was edited.

All 58 direct borrow cases pass, alongside 41 owned and 13 payload regressions.
Imported alias/contextual-name/parameter-shadowing checks and unused generic mode
rejection pass through the composed alias lowering copies. Eight corrupt checked-
AST cases reject in the public emitter, including consistent mode rewrites whose
body violates readonly access. Three focused host acceptance/emitter/effect tests
pass; native parity is supplied but unrun. The host recomputes all provenance and
loans from declarations and bodies and accepts no cached loan table as input.

Fourteen O0/O2 address/undefined sanitizer executions finish with zero live runtime
allocations. Borrow-only calls count 1 allocation/0 copies/1 drop; returned copies
and writable snapshots 2/1/2; temporaries 2/0/2; early return 3/1/3; try paths 1/0/1;
mixed Owned/readonly calls 2/0/2. Borrowed formals add no clone or callee drop.
Explicit runtime accounting replaces unsupported macOS sanitizer leak detection.

Actual effect/loan APIs are documented in the draft README and early API entry.
Nested calls consult parent loans; possibly aliasing legacy names are not treated
as disjoint. Readonly aggregate/method/task escape and unaudited effects fail closed.
New modes preserve exact Owned predicates and pointer ABI, and remain separate from
initialization, cleanup, mutability, S036/S037 and task checks. Native routing is
BH225, but the native implementation and archive are a later assignment. Shared
editor/API/source-map/private-module gates and authenticated-channel/FileHandle
composition checks remain coordinator gates; no unsupported effect silently
becomes readonly. This handoff is isolated host evidence, not combined acceptance.


## Narrow FileHandle/channel effect composition handoff

Root accepted the independent explicit-borrow host review. Original frozen borrow
patches remain unchanged. Separate integration preparation is ready under
`build/backlog/ownership-types/borrow-composition`, with README.md, two contextual
effect patches, exact input/baseline/draft hashes and verification.log. No shared
compiler, Cargo, Git or native edit occurred.

The file delta composes the accepted overload-validating effect helper with actual
FileHandle signatures. file_open buffer path is Read; file_read_chunk buffer
destination is Write; file_write_chunk buffer contents are Read. Other positions
are Value, granting no data permission or resource/thread-safety guarantee. Nine
valid overloads and 23,631 invalid vectors pass focused enumeration; unknown names
grant no authority.

The channel delta adds BoundAuthority::argument_effects(program,key,args). Each
request revalidates authority, resolves an issued nominal owner/operation and
checks the entire argument vector. Stale authority rejects C101; invalid recognized
signatures reject S039; unknown owners/methods return None. Only direct buffer/ints
payloads of authenticated send/try_send are Read, based on actual pack_copy
acquisition. Other positions and aggregate payloads are Value. Fifty-six operation
vectors pass through actual module/generic lowering across seven payload types,
with wrong-owner/arity/payload, unknown-name and declaration/factory mutation refusal
checks. Both patches apply exactly to retained preimages.

The README records ownership consumer prerequisites. Root binding types must agree
with validated argument types; forged expression metadata cannot grant read
permission. Existing readonly method/aggregate escape rules remain enforced. This
preparation introduces no exemption for channel sends or endpoint readonly modes.
A direct copied-payload exemption requires a separately reviewed ownership consumer
delta. Native/BH225, editor/real-loader, FileHandle tag11 and combined authority/borrow
acceptance remain coordinator gates. Adapter evidence does not claim combined
frontend or runtime acceptance.


## Explicit borrow host preparation against frozen channel sources

The next isolated contextual handoff is ready at
`build/backlog/ownership-types/host-integration-20261012-channels/README.md`.
All 301 coordinator inputs in build/verify-channels-frozen-sources.json match.
Exact current preimages/results and three contextual patches are hash-pinned.
Source-only replay applies with no offsets/fuzz and reproduces every result byte;
rustfmt parses the prepared source/tests. No Cargo, build/native/link/execution,
runtime/counter guest, shared-source or Git edit occurred. Tests are prepared but
unrun on this composition. Root retains complete acceptance ownership.

host-composition.patch carries the accepted seven-file explicit-borrow host delta
and only the reviewed three FileHandle buffer effects. Original .rej/.orig/log
artifacts preserve the two context repairs: builtin helper placement after current
FileHandle helpers, and current exhaustive MethodCall literal arms. Sema changes
only the non-Owned borrowed-formal predicate; existing authority/worker/file/owned/
local pending checks remain intact. Main host delta leaves registry unchanged.

host-tests.patch retains accepted borrow tests exactly and adds file data/resource/
worker guards, emitter argument corruption and readonly channel boundary sources.
channel-effects-api.patch is separate and appends the reviewed authority-validating
API/unit test sources while preserving all current SymbolOrigin/operations bytes.
It is not connected to ownership analysis. No readonly channel method/payload
exemption is granted; direct copied-payload permission needs a separate reviewed
consumer patch. Explicit local snapshots remain the supported preparation path.

Actual integration seams are documented: inspect.rs mode serialization and lsp.rs
parameter_label require new exhaustive mode arms before complete application
compilation. Existing owned-only markers must not imply transfer for borrowed modes.
Editor changes are left to root/editor ownership. Native/BH225, real loader/editor,
current combined tests, runtime/counter, archive/release/cache and full suite gates
remain separate. Original drafts and independent evidence stay frozen.


## Separate preparation test-import correction

Independent source review identified new-test -Dwarnings blockers. A separate
correction is ready at host-integration-20261012-channels/test-import-correction,
with test-imports.patch, exact preimage/result manifests, correction.sha256 and
source-validation.log. Only two import lines change: unused Type in the channel
boundary test, and unused ParamMode/redundant outer Type in the optional API test
module. All test bodies and production semantics remain unchanged. Original
preparation/review evidence and hashes are preserved. Exact patch replay and Rust
syntax checks pass; no compilation, Cargo/native, shared-source or Git edit was
performed. Root applies only the boundary test hunk for main host/test composition.
The unconnected optional channel API remains isolated; no dead-code allowance or
readonly method exemption is added. Actual -Dwarnings acceptance awaits root slot.
