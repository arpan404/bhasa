# Editor and acceptance lane

The S036 diagnostic/client corrections are ready for coordinator integration.
The nested alias RHS repair has been applied by the coordinator; its isolated
correction artifacts remain frozen. Signature and semantic features have been integrated
by the coordinator; the original alias consumer handoff remains frozen. This lane keeps shared sources untouched. The coordinator
owns Cargo gates, integration, todo.md, release refresh and Git changes.

## Channel-aware explicit borrow source handoff ready

Fresh source-only preparation is ready under
`build/backlog/editor-acceptance/explicit-borrows-channel-aware`. Six preimages
match the accepted channel freeze manifest and current sources exactly. Three
contextual patches cover editor/protocol, inspect/CLI, and grammar changes. They
reuse explicit-borrows-correction behavior and its imported failed/incomplete
hint regressions while preserving current accepted channel-aware sources.

The borrow guards share the existing originally_checked flag before/after
recovery. Protected-region hashes prove typed SymbolOrigin, authority helpers,
ordinary URI/refactoring, source symbol/definition matching, channel methods,
source-map anchors and opaque storage suppression unchanged. Existing LSP and
agent-inspect test prefixes are byte-identical. Nine corrected borrow protocol
groups and the inspect unit contract are ported; a new permanent @std CLI source
regression covers modes 0/1/2/3 alongside eight Borrowed/0 channel methods, empty
storage fields, source locations, copy semantics and no unchecked API/build output.

All three patches dry-run with zero fuzz. Rust formatting and grammar JSON parsing
pass. No compiler/link, Cargo/native/runtime execution or test suite was run:
execution waits for the ownership lane's accepted mode/effect composition.
manifest.json and handoff-contract.md record exact inputs/results and root gates.
The frozen baseline's 104 LSP/six inspector/four bridge results are root acceptance,
not new results from this source-only increment. Original drafts/failures and
code-action/client/workspace/API-doc work remain separate and unchanged.

## Opaque channel storage inspect correction ready

A separate small additive correction is ready in
`build/backlog/editor-acceptance/channel-storage-correction`:
`channel-storage-inspect-correction.patch` changes src/inspect.rs and appends a
permanent actual CLI regression to tests/agent_inspect.rs. Original exposed JSON,
proposal files and prior editor corrections remain frozen and hash-verified.

The original Query::fields guessed private flags from mapped instantiation spans
and exposed issued __sender/__receiver/__payload storage as public. The correction
reuses validated BoundAuthority operation metadata with an exact owner ID match
to suppress opaque pair/endpoint fields. Ordinary fields/private flags retain their
original path, including ordinary fields with identical names. All eight intrinsic
methods, precise types, Borrowed modes and mapped owner locations remain intact.

The permanent real @std package-loader CLI test reproduced the leak before the
change and now passes. It verifies deterministic JSON, empty PATH/unavailable
Clang/Cargo, three opaque owner rows/eight exact methods, zero storage fields,
source spans, ordinary/private/no-channel controls, invalid graph no-API refusal
and no build output. corrected-inspect.json retains the successful actual CLI
output; original-exposed-storage.json and before-correction logs retain failure.
Current approved std bytes are a hashed isolated input, not patch payload.

The correction dry-runs against its retained preimage. manifest.json and
correction-contract.md record commands, hashes and coordinator application order.
No shared source, Cargo/native/runtime build, runtime slot, Git or GUI changes.
Root owns independent current-source replay and integrated acceptance.

## Checked channel operation tooling handoff ready

New isolated work under `build/backlog/editor-acceptance/channel-operations`
targets the exact retained `build/channel-current-composition/draft` snapshot.
Three contextual patches are ready: `channel-operation-enumerator-proposal.patch`
for root review, `channel-editor.patch`, and `channel-inspect.patch`. All dry-run
against retained preimages; baseline/source/patch/evidence hashes are in manifest.json.

The read-only enumerator validates BoundAuthority before resolving exact issued
operations. Tooling uses owner nominal identity and precise parameter/result types,
with synthetic Borrowed signatures. Inspect exposes eight owner-bound method rows
for seven operation variants. Completion, hover, signature help and semantic method
identity use checked authority, not source stubs or generated-prefix dispatch.
Intrinsics stay out of module exports and cannot be renamed. Ordinary same-named
methods retain their own signatures. Original mapped owner declaration locations
currently point to the generic instantiation span; no std method token is invented.

Six channel protocol groups, eight existing signature-help groups, exact eight-row
authority metadata/mutation refusal checks and actual inspect CLI JSON pass.
Incomplete/rejected graphs and an in-memory modified registered overlay refuse
intrinsic hints; registered bytes remain unchanged. Completion recovery retains
ordinary behavior without promoting channel guarantees. `tooling-contract.md`
records the proposal, source-map contract, reproducible commands and limits.

First-failure evidence is retained: the supplied composition's whole-library
unit-test build has four malformed sema Program literals involving channels.
The production library and external protocol harness compile; no core fixture
repair is included. Probes use bounded rustc with registration rooted at the
unchanged isolated draft/std bytes. Root owns fixture composition, shared merge,
borrow/channel intersections, full/native and actual GUI acceptance.

Earlier S036/client and borrow editor corrections remain frozen and separate.
No shared source, runtime slot, Cargo/native/runtime build, Git, new chat or todo
completion occurred. No integrated or GUI parity is claimed.

Independent review identified two P2 defects in the original channel proposal.
The refreshed editor patch uses typed SymbolOrigin provenance (Source or issued
ChannelOperation with owner/operation), so arbitrary URI fragments cannot affect
completion or rename. documentSymbol and declaration_at source matching exclude intrinsic metadata
while checked reference definition/semantic identity remains available. New real-protocol regressions
reproduced both failures before correction and now pass: source outline has only
ch/User/main with the ordinary nested User.send retained; control and marker URIs
both complete and rename ordinary work declarations/uses. The original proposal,
patches and evidence are retained under review-preimage, and reviewer files verify
unchanged. All three contextual patches/manifest are refreshed against the same
baseline; enumerator and inspect patch hashes remain unchanged.

## Explicit borrow editor correction ready

The original explicit-borrow editor/inspect/grammar handoff remains frozen under
`build/backlog/editor-acceptance/explicit-borrows`. Independent review reproduced
imported readonly signatures leaking through hover/completion in rejected and
incomplete graphs. A separate contextual correction is ready under
`build/backlog/editor-acceptance/explicit-borrows-correction`:
`imported-borrow-hints-correction.patch` changes only src/lsp.rs and tests/lsp.rs.

Refusal scans contextual readonly/mutable headers across all loaded files and
retains the original graph's checked status across completion recovery. Recovery
cannot promote borrow guarantees; lexical shapes only refuse hints. Two new
protocol groups cover invalid imported aliases and incomplete qualified access
for both modes. Checked binder/marker, copy/shadow and rename proofs remain intact.

The old unaudited-file negative fixture is migrated only in the correction copy
to the reviewed `borrow-composition/file-effects.patch` core input: readonly
file_write_chunk contents succeed; readonly file_read_chunk destinations reject.
That supplement is not editor payload. The core owner owns FileHandle effect
integration and separate authenticated-channel consumer composition.

All 9 explicit-borrow groups, 5 owned-tooling groups, 1 inspect unit test and actual
inspect JSON smoke pass. Both original reviewer framed requests replay with empty
completion/null hover; outputs are saved separately under correction/review-replay.
The correction dry-runs cleanly against its frozen preimage. Original handoff and
reviewer evidence hashes verify unchanged. `correction-contract.md` and
`manifest.json` record semantics, inputs, proof and coordinator application order.
Original grammar evidence remains 9 passed; no grammar changes in this correction.

Coordinator applies original editor patches plus this correction after core mode
integration and the reviewed FileHandle effects. Full Cargo, FileHandle/channel
composition, both frontend/native/runtime and GUI gates remain coordinator-owned.
No shared source, Cargo/Git, native build, GUI work or todo completion occurred;
prior S036 and alias correction artifacts remain frozen.

## S036 diagnostic and client correction handoff

Independent review found three contract gaps in the frozen S036 draft: malformed
context diagnostics could return an empty list or be mixed with a valid diagnostic;
final overlay membership missed a transient Bhasa open-close event; and dormant
client validation treated a let substring as a possible declaration marker.
The last two were helper defects, not a demonstrated live edit bypass, because
the provider already offered no actions or edits.

Separate additive corrections are ready under
`build/backlog/editor-acceptance/code-actions-correction`:

- `diagnostic-shape-correction.patch` changes only src/lsp.rs and tests/lsp.rs.
- `client-history-validation-correction.patch` changes only the custom VS Code
  adapter and its tests.

Both patches dry-run cleanly against an exact copy of frozen code-actions/draft.
manifest.json separately hashes baseline, corrected files and both patches.
frozen-checkpoint.json verifies original S036 patch/contract/evidence hashes are
unchanged. No shared sources, alias files, Cargo/Git, GUI or original evidence edits.
The reviewed nominal RHS correction remains independent and unchanged.

Actual supported diagnostic filters require every entry to be an object with a
string or signed 32-bit integer code and ordered range positions with nonnegative
32-bit integer line/character fields. Missing/null/wrong types, fractional numbers
and reversed ranges return -32602. The server validates the whole array before any
matching, unsupported-kind/stale-version refusal or graph analysis, so malformed
entries beside a matching diagnostic cannot produce an action. Optional triggerKind
supports integers 1 and 2 only. Other fields remain unused/untrusted. Shape-valid
stale ranges cannot match the actual recomputed server diagnostic. No declaration,
identity or repair proof comes from a client diagnostic; all original exact-version,
graph/refusal/repaired-binding requirements remain in force.

The client maintains a monotonic BigInt epoch for Bhasa document opens/closes.
Checked graph replies require unchanged history as well as the existing snapshots,
so a transient overlay cannot disappear from the final-membership check. Plaintext
history does not invalidate replies. The dormant response validator now accepts
only empty lists on open documents; every nonempty response is rejected, including
valid-looking server edits or let in identifiers/strings/comments. No trusted local
syntax proof or version-preserving application mechanism exists, so the provider
continues to return no actions/edits and never constructs/applies WorkspaceEdit.

Evidence under code-actions-correction/:

| Gate | Result | Evidence |
| --- | --- | --- |
| S036 protocol groups | 8 passed, including 2 new correction groups | probe-tests.log |
| Custom VS Code suite | 83 passed, zero failures | editor-tests.log |
| Reviewer-authored server cases replayed on copied current alias composition | 7 passed, including malformed context, alias metadata, locks and version/range guards | review-probe-tests.log |
| Both contextual patch dry-runs | Passed | diagnostic-shape-correction.patch.dry-run.log, client-history-validation-correction.patch.dry-run.log |

correction-contract.md records the precise API and fail-closed client behavior.
Replay with run-probe.py using BH_ACTION_PROBES=checked_code_action and
run-independent-probe.py. Both use bounded rustc cached-library probes only;
commands/input/library hashes are recorded in their build logs. The replay applies
only the server source hunk to a separate copy of reviewer composition, and runs
no reviewer test that writes original evidence. The original malformed forged
filter regression now expects -32602 in the correction copy only.

Coordinator should apply original S036 patches, then these separate corrections,
and run focused/full LSP, adapter Node and native gates. The seven replayed cases
are verification evidence; final independent review disposition remains with the
coordinator. No atomic version-safety claim or expansion of offered client edits.

## Nested alias RHS correction handoff

The coordinator's review found missing Point identity in `type Item=Box[Point]`.
The prior consumer indexed only the complete canonical alias target. It omitted
nested nominal arguments, so definition and semantic tokens were absent and
conservative rename correctly refused. The separate correction is ready under
`build/backlog/editor-acceptance/aliases-rhs-correction`.

- `alias-rhs-producer-correction.patch` changes only src/modules.rs and
  tests/type_aliases.rs. Apply this additive producer API first.
- `alias-rhs-editor-correction.patch` changes only src/lsp.rs and tests/lsp.rs.
  It replaces the incomplete whole-target lookup with source-anchor metadata.

Both patches dry-run cleanly against the saved coordinator
build/alias-current-merge/draft composition. manifest.json records exact baseline,
draft, wrapper and patch hashes. No code-action slice is composed into this
baseline. Its finished patches, contract and evidence remain at the verified atomic
checkpoint in code-actions-checkpoint.json. The original frozen alias patches also
still match their manifest hashes. Shared sources and frozen ownership drafts
remain untouched; only these new additive patches are handed off.

The producer API need was published before implementation in metadata-contract.md.
Actual new API is Expansion.type_alias_nominal_uses, containing
TypeAliasNominalUse { declaration_span: Span, span: Span }. These are original
virtual-offset anchors for the resolved nominal declaration's name token and the
RHS reference. Qualified uses retain the whole namespace/member span. The existing
authoritative module scope/exports resolution determines the declaration, after
alias validation/privacy/cycle checks. It records nominal owners and arguments at
every generic depth. Alias-to-alias RHS references remain TypeAliasUse; scalar and
builtin collection tokens produce no nominal entries. A single borrowed-name
lookup records two spans per use, with no canonical string clone per occurrence.
No generics.rs, lib.rs, runtime, ABI, ownership or grammar change.

The consumer uses the metadata only inside a successful checked reference graph.
Each exact declaration anchor must match one Struct/Enum symbol whose key exists
in the checked target index. Multiple concrete generic instances may share that
one source declaration key. No match or conflicting keys means no reference.
Qualified references select only the final identifier; existing namespace identity
handles the prefix. The consumer neither guesses unqualified names nor parses
canonical strings into declaration guesses. Alias use identity stays separate.
Existing lexical completeness and post-rename checked identity/capture checks
remain unchanged, including conservative refusal for unindexed generic shadows.
Current primary-lock, duplicate-overlay and SortedPositions fixes are preserved.

Focused cached-library probe results in probe-tests.log:

| Gate | Result |
| --- | --- |
| Checked alias editor/formatter protocol | 16 passed, including 4 new correction groups |
| Complete producer alias test module | 18 passed, including 1 new source-anchor metadata test |
| Contextual patch dry-runs | Both passed |

The new cases verify deep/repeated nominal arguments of unused aliases, definition,
semantic kind and complete versioned rename, generic owners, chained alias identity,
qualified same-name nominals in different modules, private local targets, invalid
public exposure failing closed, UTF-16/CRLF, and generic type-shadow rename refusal.
The producer test verifies exact declaration/use spans, deterministic ordering and
separate alias-link metadata. Existing producer checks cover privacy, unused target
validation, canonical identity, ownership/copy behavior and bounded alias spelling.

Replay with
`python3 build/backlog/editor-acceptance/aliases-rhs-correction/run-probe.py`.
BH_ALIAS_RHS_PROBES selects narrower filters. The bounded rustc probe uses the
coordinator's existing cached library snapshot; its path/hash and exact command
are in probe-build.log. This lane ran no Cargo/Git/shared-source edits or GUI gates.
Coordinator integration must apply the producer addition with the consumer repair,
then run `cargo test --test lsp checked_alias` and `cargo test --test type_aliases`,
repeat native/frontend variants and independently review the repair before broad
full gates. Do not replace source files wholesale or alter the frozen old patches.

## S036 code-action handoff

The ratified small server slice is ready in
`build/backlog/editor-acceptance/code-actions`, based on the exact saved integrated
sources in `code-actions/baseline-manifest.json`. The frozen alias consumer is not
applied. Preserve the coordinator's primary-lock, requested/canonical URI,
duplicate-overlay and SortedPositions implementation. Apply contextual patches,
not whole draft files:

- `code-actions/code-actions-server.patch` changes only src/lsp.rs and tests/lsp.rs.
- `code-actions/code-actions-client.patch` changes only the custom VS Code adapter
  and its tests. This adapter offers no actions or edits.

Both patches pass dry-run against the saved baseline. `code-actions/manifest.json`
records baseline, draft, patch and bounded-probe overlay hashes. Shared sources and
frozen alias artifacts remain untouched. The coordinator ratified the server slice,
its refusal rules, the corrected `let` to `var` edit and fail-closed client behavior.

The original proposed three-space replacement was wrong: it removes the declaration
and becomes an assignment. The repaired-graph check rejected it. The implementation
instead replaces exactly three bytes `let` with `var`, using existing grammar and
preserving every source offset. The corrected actual API is in
`code-actions/action-contract.md`.

The server advertises quickfix without resolve. A request must include the exact
open document version and a valid UTF-16 range/context. The server recomputes the
current diagnostic and requires exactly one graph diagnostic, S036, intersecting
the request. Client diagnostic code/range, if supplied, only filters that result.
It never supplies identity. One complete syntax walk proposes the actual preceding
lexically visible Var declaration. Same-name declarations/parameters, incomplete
source, additional errors, generic/method/TryVar/For/Match cases and ambiguous spans
refuse. Only one candidate and one repaired full graph check are attempted. Checked
references from the repaired graph must independently connect the assignment to
the candidate's source binder anchor, with mutable binding metadata. A second
lexical proof retains the scope anchor. No failed-graph checked identity is guessed.

Eligible graphs are standalone or fully loaded within one package. Package locks
must exist; analyze verifies the primary locked disk graph before overlays.
Cross-package graphs refuse pending retained per-package verification metadata.
New package files refuse because their local fallback has no verified disk graph.
Standalone new files retain the requested URI. Missing/invalid/stale locks, changed
unsaved imports, duplicate canonical aliases and erroneous dependency overlays
refuse. The edit is one versioned TextDocumentEdit for the requesting document;
no changes map, command, bulk fix, rename, runtime or diagnostic-schema changes.
Lexical walks cap recursion at 128 and carry one target anchor rather than cloned
scope maps. Existing graph bounds and an explicit response-size bound apply.

The custom VS Code provider synchronizes all open Bhasa overlays and uses existing
document-object/version/close-save-epoch/session snapshot guards. It validates the
server's sole versioned marker edit and diagnostic shape, then always returns an
empty action list. It never converts this response to WorkspaceEdit or applies it.
This is deliberate: the existing conversion loses document versions and cannot
protect delayed selection. No atomic version-safety or GUI application claim.

Focused evidence, all under code-actions/:

| Gate | Result | Evidence |
| --- | --- | --- |
| New S036 protocol groups | 6 passed | probe-tests.log |
| Existing signature/semantic/let and lock/overlay/refactor groups | 23 distinct tests passed, some overlap in filter replay | probe-tests.log |
| Custom VS Code suite | 79 passed, zero failures | editor-tests.log |
| Contextual patch dry-runs | Both passed | code-actions-server.patch.dry-run.log, code-actions-client.patch.dry-run.log |

Protocol coverage includes exact marker/version, successful repaired check,
UTF-16/CRLF/comments, ordinary names let, nested scope, shadow refusal, additional
errors, incomplete syntax, generics/methods/TryVar, stale/missing request versions,
forged diagnostic, unsupported action kind, invalid range/context, saved no-import
missing/corrupt locks, package new-file refusal, locked dependency overlays,
changed imports, stale disk dependency, duplicate canonical aliases and standalone
new-file URI. Client checks include response shape/version/URI/marker/command
rejection, no offered edit after a valid response or stale selection, capability
absence, synchronized dependency overlays and save/close/reopen/new-document/
cancellation/restart races. No unsafe edit is available to select.

Replay the bounded cached-library Rust probe with
`python3 build/backlog/editor-acceptance/code-actions/run-probe.py`; use
BH_ACTION_PROBES to select narrower filters. The saved host wrapper matches its
baseline lib.rs. probe-build.log records the exact cached library and command.
Run Node tests from code-actions/draft/editors/vscode/tests/extension.test.js.
This lane ran no Cargo or Git commands and changed no shared sources, SDKs or caches.

Coordinator gates after contextual integration include
`CARGO_BUILD_JOBS=2 cargo test --test lsp checked_code_action -- --test-threads=1`,
the full LSP suite, the adapter Node suite, native frontend variants and independent
review. Full Cargo/native gates and integration remain coordinator-owned and pending.
Alias handoff below remains frozen.

## Additive alias handoff

The alias increment is ready in `build/backlog/editor-acceptance/aliases`. It
builds on the saved corrected signature plus semantic composition and the frozen
ownership alias API. The earlier request for alias metadata is now fulfilled.
Apply these contextual patches after the coordinator integrates those dependencies:

- `aliases/alias-editor.patch` changes only `src/lsp.rs` and `tests/lsp.rs`.
- `aliases/alias-inspect.patch` changes only `src/inspect.rs`. This file is shared
  with the package lane. Preserve its package metadata changes when applying the
  small declaration additions and inspection test.

Both patches pass dry-run against the saved baseline. `aliases/manifest.json`
records exact baseline, draft, patch and ownership overlay SHA-256 hashes.
Do not replace shared files wholesale from this draft. The original signature
and semantic patches remain unchanged. No production formatter change is needed.

The editor reads `Expansion.type_aliases` and `type_alias_uses` only after a
successful checked graph. Alias identities use their source declaration anchors,
separately from canonical nominal targets. Document symbols, completion, hover,
definition, references, rename and semantic tokens include unused aliases.
Alias declarations and uses use the existing semantic `type` legend slot.
Contextual `type` markers use the keyword slot and have no hover/completion
identity. Ordinary functions and bindings named `type` retain their identities.
Qualified alias uses select the final identifier, so navigation and rename preserve
namespace prefixes. Original UTF-16 ranges, privacy and unsaved overlay versions
continue to govern replies and edits. Alias annotations take precedence over a
same-named value only at metadata-confirmed alias sites. Nominal RHS names and
enum variants retain their own identities. Invalid checked graphs fail closed.
Generic template type shadows retain the existing conservative rename refusal
when completeness cannot be established. No alias wrapper or runtime ABI change
is introduced, and the implementation does not clone the whole expansion.

Inspection adds deterministic `kind: "type_alias"` declarations with `name`,
`private`, `location`, `name_location`, `target_location`, `declared_target`,
`canonical_target` and `canonical_target_location`. The declared spelling remains
visible alongside the checked target. Nominal targets have a source location;
scalar and builtin collection targets have null. The declaration-scope description
now includes validated aliases. Existing declaration ordering and package fields
remain under the coordinator's shared-file merge checks.

Focused verification uses bounded rustc probes against an existing cached library:

| Gate | Result | Evidence under aliases/ |
| --- | --- | --- |
| Alias protocol/formatter plus inspection unit | 10 passed | probe-tests.log |
| Signature protocol regressions | 8 passed | probe-tests.log |
| Semantic protocol regressions | 6 passed | probe-tests.log |
| Existing let identity regressions | 2 passed | probe-tests.log |
| Existing symbol hierarchy and UTF-16 regressions | 5 passed | symbol-regressions.log |
| Standalone inspect JSON with empty PATH | Passed, including nominal target location and builtin null | inspect-json-smoke.json, inspect-json-smoke.log |
| Contextual patch dry-runs | Both passed | alias-editor.patch.dry-run.log, alias-inspect.patch.dry-run.log |

Formatter coverage checks declared alias spelling, comments, trailing commas,
idempotence, checked alias counts and identical emitted LLVM before/after formatting.
`src/format.rs` is unchanged. Replay with `aliases/run-probe.py` and
`aliases/run-inspect-probe.py`. Build logs record the exact cached-library hash
and command. The unit wrapper reports six unused inspector CLI-function warnings.
These probes do not establish full Cargo, native frontend or GUI acceptance.

The copied ownership overlay still matches the frozen owner draft byte for byte:

- modules.rs: `5a9017e6469f897c329f3bd3f7612a515fa2779b6b233c3101dc9aa66f36b23a`
- generics.rs: `506d1346ac94b6713a0bbf4dfd670aeaf2d2b18a5d56c1f41cba01c26e9fc9b1`
- lib.rs: `ceeb18e1d9e40aa94bdf7522e50aaf9ec993fa2978ca3c4f34ce1d81a146a3b4`

Coordinator gates after contextual integration:

```sh
CARGO_BUILD_JOBS=2 cargo test --test lsp checked_alias -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --lib alias_inspect_tests -- --test-threads=1
```

Repeat with the coordinator's native frontend configuration, run existing
agent-inspect/package checks to verify their metadata survived the shared-file
merge, and run broader editor/formatter/alias acceptance gates there. This lane
ran no Cargo or Git commands and changed no shared compiler sources, ownership
draft files, SDKs, caches, todo.md or release notes. Coordinator integration and
full gates remain pending.

## Signature and semantic handoff

Use `build/backlog/editor-acceptance/signature-on-let/signature-help.patch` for
signature help. This supersedes the original `signature-help.patch`, whose full
source copy predates immutable let. The corrected baseline contains let and the
root hover marker fix. Exact saved file hashes are in signature-on-let/manifest.json.

The signature correction includes both independently reviewed defects:

- Successful local analysis retains doc.path for new unsaved file-URI documents.
  A protocol test verifies hints and hover before save and hints again after save.
- Checked client queries synchronize all open Bhasa overlays before request and
  snapshot document objects, versions and close/save epochs. Changes, closes,
  same-object reopens, saves at unchanged versions, newly opened Bhasa documents,
  current-document edits, cancellation and session restarts discard delayed
  replies. The bound follows the existing 128-document synchronization limit.
  Invalidating on any open Bhasa overlay is deliberately conservative until
  workspace dependency generation metadata exists. Plaintext opens do not discard.

Signature support also now reads checked List/Map method schemas, alongside
user record methods, functions, concrete generics, externs and builtins. Added
coverage exercises externs, chained returned/collection receivers, CRLF and
non-BMP positions. No source/runtime/ABI semantics change.

Current focused evidence:

| Draft/gate | Result | Evidence |
| --- | --- | --- |
| Signature Rust protocol probe | Eight new groups pass; both existing immutable-marker and ordinary-function-let regressions pass | signature-on-let/probe-tests.log |
| Signature client | 64 pass, zero failures | signature-on-let/editor-tests.log |
| Semantic Rust protocol probe | Six new groups pass; the same two existing let regressions pass | semantic-highlighting/probe-tests.log |
| Semantic client | 63 pass, zero failures | semantic-highlighting/editor-tests.log |
| Combined client scratch composition | 69 pass, zero failures with signature, semantic and dependency-overlay guards together | combined-editor/editor-tests.log |

All paths in this table are relative to build/backlog/editor-acceptance. The Rust
probes compile only the draft LSP, a saved host module/source-map wrapper, and
protocol test harness against an existing cached compiler library. Exact command
and cached-library hashes are in each probe-build.log. Replay with
`python3 build/backlog/editor-acceptance/semantic-highlighting/run-probes.py`.
No Cargo/shared compiler archive build or broad test suite is performed. This is
focused draft execution evidence, not refreshed release, full native/frontend,
A/B/C, hardware or GUI acceptance.

Keep semantic-highlighting/semantic-highlighting.patch separate. Its baseline
also includes let/root hover fixes; its marker change extends the root guard to
completion while preserving hover behavior. Integrate the corrected signature
first, then merge only the semantic capability/provider/type-metadata additions,
retaining the signature's common graph guards. The combined-editor tree is a
scratch verification composition, not a wholesale source replacement. Both
feature patches dry-run against their saved exact baselines. Apply small hunks
contextually to current tracked files; never overwrite shared sources from drafts.
Copied transport.js and generated .orig/.rej/probe artifacts are not handoff
source files.

The independent review remains untouched at independent-review.md. Its two
findings are corrected and covered by the current evidence above. Review-era
first-signature and failed fixture/probe records are historical. One fixture
initially attempted to call a function after a same-named local shadowed it;
that fixture now calls before the local declaration. New extern fixture uses the
existing scalar-int C ABI contract. The unsaved-file test constructs a URI from
an existing canonical parent instead of canonicalizing a nonexistent file.

Coordinator gates remain the exact focused Cargo/reference/native commands
below, followed by broader editor gates and any requested GUI verification.
Alias editor/API work awaits the actual lowering metadata API requested below.

## Historical first increment and build slot request

Draft root: `build/backlog/editor-acceptance/draft`, using original relative paths.
Baseline copies: `build/backlog/editor-acceptance/baseline`. Patches are `build/backlog/editor-acceptance/signature-help.patch` and
`acceptance-harness.patch`; exact copied target-file hashes are in
`build/backlog/editor-acceptance/manifest.json`. The signature patch dry-runs
cleanly against its saved baseline. Baseline HEAD is
`c4f89575fd0f817f1d0cb02915ab841de166d05f`; the copied working files already include
the coordinator's uncommitted typed-error/editor changes. Apply small patches to
the current shared files, never replace shared files with the full draft copies.

Owned signature files are src/lsp.rs, tests/lsp.rs, editors/vscode/extension.js and
editors/vscode/tests/extension.test.js. A separate new prerequisite is
tools/acceptance/check_differential.py. transport.js is copied unchanged only so
the draft client tests resolve their dependency; do not integrate that copy.

Request a serialized coordinator slot after applying/reviewing patches:

```sh
CARGO_BUILD_JOBS=2 cargo test --test lsp checked_signature_help -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test lsp checked_signature_help -- --test-threads=1
node --test editors/vscode/tests/extension.test.js
```

Use the repository's current hybrid gate/target settings if the full native
frontend needs its existing archive setup. The lane has run no Cargo commands.

## Contract

- Advertise signatureHelpProvider with `(` and `,` trigger characters. Accept
  textDocument/signatureHelp using existing open-document and UTF-16 validation.
- Only a successful checked source graph supplies a signature. Invalid and
  incomplete source returns null. No recovery tree supplies callable identity.
- Resolve the innermost enclosing checked call using original call spans and
  lexer delimiter pairs. Count commas only at its argument depth.
- User functions, externs and concrete generic functions use checked named
  parameter types. Record methods omit implicit self. Builtins use concrete
  checked argument/result types and neutral arg1/arg2 labels.
- Return one signature with activeSignature=0 and zero-based activeParameter.
  Zero-argument calls return an empty parameters array and activeParameter=0.
  Specializations sharing a template span return null if signatures disagree.
- Reuse module/generic display names and checked privacy. Recheck unsaved overlays.
  Never return internal __bhm_/__bhg_ names or truncate a signature label.
- Client capability checks, synchronization, cancellation, version/session/close
  guards remain in query(). Labels are plain strings; no trusted Markdown.
  Malformed or oversized signature replies return null.

No compiler grammar, runtime ABI, frontend semantics, .bh naming, lowercase
bhasa.toml or deterministic no-GC ownership changes are required.

## Evidence

Draft client extension suite: 61 tests pass, 0 failures. Evidence:
`build/backlog/editor-acceptance/editor-tests-final.log`. This includes existing client
regressions and new signature conversion, missing-capability, malformed reply,
UTF-16 capability, edited/closed/canceled/restarted-response tests. Node syntax
check passes. Rustfmt parses the new source and test additions successfully. Existing
baseline formatting is retained so patches contain no unrelated reformatting. The original six protocol groups are superseded by the eight-group corrected
probe evidence above. Do not report full integrated/Cargo acceptance.
Real VS Code GUI/device/performance verification has not occurred.

## Coordinated acceptance inventory

| Increment | Dependency and ownership boundary | Concrete completion evidence |
| --- | --- | --- |
| Signature help | Existing checked graph; integration editor/error/binding changes | Protocol tests in reference and hybrid modes; client tests; manual VS Code argument hints with versioned unsaved dependencies |
| Semantic highlighting | Checked reference identities; immutable-binding metadata; ownership/types declaration additions | UTF-16 delta token encoding, deterministic sorting/no overlap, scope and private symbols, edit/restart invalidation; GUI color evidence |
| Code actions and safe refactors | Diagnostic spans/codes and checked rename infrastructure | Explicit diagnostic-specific fixes, versioned edits, recheck after applying, no private/capture/type changes; malformed/incomplete source refusal |
| Workspace indexing/watch | Packages workspace manifest/lockfile contract and module loader | New/delete/rename/dependency-save/overlay races; bounded graph and cancellation; stable indexed identities; missing/cyclic modules |
| Agent type/API queries | Existing inspect JSON; generic-template/type-alias contracts from ownership/native lanes | Deterministic unused generic APIs, original locations/privacy, structured type/member/edge query schemas; no native tool invocation |
| Project format and docs | Packages workspace/API export policy | Deterministic project traversal, lowercase manifest detection, idempotence and comment preservation, source equivalence; API privacy/template docs |
| Assertions/property tests | Failure-use and Result contracts; test runner concurrency/generic support | Owned expected/actual diagnostics; deterministic seeds, bounded generation/shrinking, minimized replay; same oracle in both frontends |
| Coverage/bench/debug | Native compiler span/emission/cache contract and GPU/platform target support | Source-mapped coverage; reproducible benchmark records; debug stepping/backtraces/profiler traces from executed artifacts |
| Compiler/runtime fuzzing | Frozen parser/checker/native-boundary drafts per lane | Seeded lexical/grammar differential cases, depth/size budgets, malformed input corpus, minimized cases and sanitizer logs; independent reviewer findings |
| Fault/soak acceptance | Safety/errors, std/backend, concurrency cancellation and shutdown | Injected I/O/allocation/channel/network faults; deadline-bound repeated services; allocation/handle balance and RSS series; retained seeds/logs |
| Cross-platform CI | GPU/platform target execution support, package toolchain policy | CI files plus actual Linux/macOS/Windows successful logs; no target support claim from cross compilation alone |
| Language/ABI versioning | All lane public contracts, packages stable-export ABI owner | Ratified version/migration rules, compatibility fixtures and deterministic ABI metadata; breaking-change diagnostics |
| Tutorials/installers/release | Packages release artifact/install policy and supported target matrix | Executed tutorial examples in both frontends; staged installer/uninstaller smoke; checksums/provenance; coordinator release verification/signing/publication |
| Agent efficiency | Stable query/action/index APIs and representative tasks | Recorded task set, baseline and new token/time/error data, reproducible harness; no performance or efficiency claim before measurement |

## Remaining criteria and next prerequisites

Draft syntax review and exact baseline/patch manifest are complete. Coordinator
must compile/run protocol tests and review method/generic span mapping. GUI, Linux,
Windows, CUDA/Vulkan, browser/WASI and long-run performance remain unverified.
The bounded differential corpus/replay prerequisite below is drafted. Next
independent work can prepare semantic-token fixtures while shared sources stay
frozen.
Ownership/native lanes must publish any changed checked AST/type metadata before
editor consumers can integrate those additions.


## Bounded corpus prerequisite

`draft/tools/acceptance/check_differential.py` generates seeded valid programs and
independently invalid mutations, invokes existing binaries with explicit
reference/native frontend selection and JSON diagnostics, records command/exit,
source/compiler checksums and timing, and retains all source/diagnostic files.
It refuses occupied output directories. Pair count and command timeout are
bounded. Output is spooled to files; diagnostic parsing has a 1 MiB limit.

A successful diagnostic rejection must include checked language diagnostic codes.
Unavailable frontend H002, command usage D001, signals, timeouts and other host
failures cannot satisfy the invalid-program oracle. The harness checks accepted
vs rejected source only; it does not compare generated output or prove memory
safety, fuzz coverage, performance, or soak readiness.

Executed small probe: eight cases, four valid/invalid pairs, seed 404. Both
frontends satisfy the oracles using existing compiler executables. Evidence:
`build/backlog/editor-acceptance/corpus-verified-smoke/report.json`. That report
records exact executable hashes; it is evidence for those binaries, not for the
uncompiled signature-help draft. An unavailable-frontend regression deliberately
fails both cases at `corpus-unavailable-regression/report.json`, as required.
Earlier `corpus-smoke` and `corpus-current-smoke` reports are retained failed
probe history and superseded by the corrected final report. The initial probe
caught a missing native frontend and then the native BH200/BH201/BH202 diagnostic
code distinction; the harness now handles both without counting H002 as success.

Five authored standalone signature fixture sources also pass a bounded existing
reference-compiler check; `fixture-check-current.log` records the binary hash.
The older `fixture-check.log` used an obsolete executable lacking generic lexing
and is not acceptance evidence. Full integrated protocol acceptance still requires the coordinator's Cargo slot. The imported overlay protocol group additionally needs
its on-disk temporary graph built by the test itself.


## Second increment: checked semantic highlighting

Prepared after root applied immutable let, before signature help is integrated.
The isolated root is `build/backlog/editor-acceptance/semantic-highlighting`:
`baseline` saves exact original-relative-path copies, `draft` contains the new
files, `manifest.json` records both hashes, and `semantic-highlighting.patch`
differs only in four editor-owned files. Its patch dry-runs cleanly against the
saved baseline. Do not replace current shared files with draft copies. Signature
and semantic patches independently extend capabilities/client registrations; if
signature help lands first, merge those small insertions and preserve both.
Neither increment changes runtime/compiler semantics. No Cargo/Git ran here.

Protocol contract:

- Advertise semanticTokensProvider with full=true, range=false and a fixed
  relative UTF-16 legend. No incremental/delta, range or resultId claims.
- Token types in order: namespace, struct, enum, enumMember, function, method,
  property, parameter, variable, keyword, type. Modifier bits: declaration=1,
  readonly=2. The final type slot is reserved for future checked alias metadata;
  no guessed alias tokens are emitted by this draft.
- Only checked reference identities produce symbol tokens. Locals carry
  BindingKind metadata derived from checked AST reassignable, separate from
  human-readable hover details. Parameters retain current mutable behavior.
  Immutable declaration/read tokens receive readonly; ordinary locals, loop
  binders and match binders retain mutable behavior.
- Valid checked graphs classify contextual let/try binding markers as keywords.
  Their spans cannot inherit a same-named local/function identity. Hover on a
  marker returns null and completion returns an empty list, while normal calls,
  binders and reads named let/try keep their checked identities.
- Encode sorted UTF-16 deltas, deduplicate repeated specialization spans and omit
  conflicting classifications/overlaps. Exclude comments, strings and incomplete
  or failing graphs. These retain existing TextMate lexical highlighting.
- Cap tokens at 65,536. A larger indexed result yields an empty semantic result.
  Existing protocol message/document limits remain in force.
- Client accepts only the exact supported legend, validates token count/types,
  modifiers, uint32 values, line bounds, lengths, overlap and surrogate boundaries.
  Existing synchronized query guards discard edited/closed/canceled/restarted
  replies. Client sends no request to missing or incompatible providers.

Evidence: 63 draft client tests pass, zero failures, in
`semantic-highlighting/editor-tests.log`. This comprises the current client
baseline plus five new semantic groups. Node syntax check passes. Rustfmt parses
new Rust additions, and the patch preserves unrelated baseline formatting. Six
new protocol groups in draft/tests/lsp.rs pass the bounded cached-library Rust
probe above; they are not Cargo-executed. They
cover marker/function/binder/read identity, explicit try vs ordinary try names,
UTF-16/CRLF and readonly shadowing, records/fields/methods/parameters/enums,
unsaved invalidation, and qualified imported identity with comment exclusion.
GUI theme behavior and full reference/hybrid acceptance remain unverified.

Coordinator slot requested after review/application and immutable gates:

```sh
CARGO_BUILD_JOBS=2 cargo test --test lsp checked_semantic_tokens -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --features bhasa-frontend --test lsp checked_semantic_tokens -- --test-threads=1
node --test editors/vscode/tests/extension.test.js
```

Root owns native archive environment and broader editor gates. Confirm the
existing immutable let/try hover/rename tests still pass after integration.

## Alias editor/API handoff

Read ownership-types status and its actual shared module-lowering draft. Aliases
are transparent non-generic contextual top-level `type Name = Target` entries,
resolved in their defining module before generic specialization. They introduce
no nominal AST ID. Private exports, local/generic shadowing, source spans and
canonical specialization names must remain accurate. The current draft keeps
TypeAlias and canonical_aliases private inside modules.rs and exposes no alias
use/declaration metadata through Expansion. Implementing an imagined AST alias
variant or matching arbitrary identifiers by spelling would lose checked alias
identity. No such handling is added here.

Coordinator should route this concrete metadata request to ownership-types:

1. Expose immutable module-expansion alias descriptors after successful alias
   resolution, with defining file index, original declaration/name/target spans,
   name, visibility, canonical target spelling, and a stable original declaration
   identity. Preserve direct aliases even when multiple aliases share a target.
2. Expose original alias-use spans linked to those descriptors after the existing
   protected-binding/scope analysis. Qualified/local/generic-shadowed identifiers
   must be resolved by the lowering owner, not guessed by the editor. Constructor,
   annotation, enum-owner and generic-argument alias sites should retain this
   link when canonical lowering replaces their spelling.
3. Distinguish alias-to-alias declaration edges from expanded canonical targets.
   Agent/API output needs both documented spelling and canonical representation;
   a target's nominal privacy and source location remain those of its owner.

After that real API is published, this lane can attach alias declaration/use
identities to checked graph symbols, use semantic type slot 10, navigate uses to
alias declarations, include unused validated aliases in inspect/API docs, and
review conservative rename completeness. Formatter must preserve contextual type
spelling. Alias cycles, private target leakage, unknown/unused invalid aliases,
instantiation-dependent target validation and native source mapping stay owned
by ownership-types/integration and remain acceptance prerequisites. Planned
editor regressions: imported aliases with identical names, direct/chained aliases,
local named type/alias shadowing, scalar/record/enum/concrete generic targets,
unsaved overlays, private targets, and deterministic original-file API locations.
