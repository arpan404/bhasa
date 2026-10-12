# Packages and FFI backlog lane

Current handoff, October 12: the separate workspace host-boundary correction
closes the second independent review findings with 26 passing focused checks.
The workspace correction chain and pure API documentation renderer are ready
for independent review. The frozen
workspace/version increment requires its separate correction in
build/backlog/packages-ffi/workspace-output-correction before integration. Its
22 focused checks pass. The renderer in build/backlog/packages-ffi/api-docs passes
eight focused tests and real checked-report verification. The original seven patches and 45-check evidence remain
frozen in build/backlog/packages-ffi/workspaces. The
parent has integrated the prior authoritative-root correction. Its frozen draft
and earlier increment evidence remain unchanged. Integration owns combined
Cargo/frontend/native acceptance, source publication and Git operations.

October 11, 2026. The combined lock/adapter increment, CLI follow-up and four
independent-review repairs are implemented and ready for coordinator review.
Draft source is frozen for handoff. This is not integrated Cargo/frontend/native
acceptance. All tracked compiler/runtime sources and persistent caches remain
untouched. No Cargo, broad suites, SDK installation, Git mutation or publication
was run.

## Baselines and small patches

The first increment used HEAD c4f89575fd0f817f1d0cb02915ab841de166d05f with the
coordinator's then-uncommitted typed-error work. Baseline working-tree copies and
hashes are in build/backlog/packages-ffi/baseline/src and baseline-sha256.txt.
The CLI/review follow-up reads current driver/inspect/main/test_runner at
HEAD e24aa78ba10c146020e48d984d4267dbabe7c630. Those exact copies and hashes are in
baseline/src and cli-baseline-sha256.txt. Main already delegates to driver::run;
there is no main.rs patch.

All code, tests, docs and example metadata are under
build/backlog/packages-ffi/draft with original relative paths. The patch sets are:

- patches/ and patch-index.json contain 15 combined file patches against the
  recorded original sources or /dev/null for additions.
- cli-patches/ and cli-patch-index.json contain 13 follow-up deltas against the
  first delivered draft modules and current shared driver/inspect/test_runner.
  Use this set only if the first increment was already applied.
- first-increment/, first-increment-patches/ and first-increment-index.json
  preserve the first delivered source/patch set independently.

Each index records baseline SHA-256, draft SHA-256 and the exact patch path.
patch-validation.log confirms that all 15 combined patches apply to the recorded
baselines in an ignored scratch tree and reproduce exact draft bytes. Do not
replace newer shared files with whole baseline-derived copies. Apply the related
patches together, preserving unrelated integration changes.

Changed existing sources are src/project.rs, src/lib.rs, src/build_cache.rs,
src/rust_deps.rs, src/driver.rs, src/inspect.rs and src/test_runner.rs. New files
are src/package_lock.rs, src/ffi_contract.rs, src/lock_cli.rs,
tests/package_lock.rs, tests/package_lock_cli.rs, docs/package-locks.md,
docs/adapter-contracts.md and examples/rust-package/adapter/bhasa-adapter.json.

## Lock APIs and CLI contract

Manifest::checksum hashes exact manifest bytes consumed by the parser.
build_cache::sha256 exposes the existing internal digest within the crate;
cache fingerprints and native/Rust cache behavior are unchanged.

package_lock::Snapshot::capture, encode, decode and verify represent a single
primary loaded entry graph in deterministic sorted JSON, bhasa-loaded-graph-v1.
The graph contains root/imported/transitive sources and raw manifest checksums.
It excludes unused source files, build/cache outputs and Rust source trees.
Canonical file identities use normalized portable UTF-8 paths relative to the
root package, including leading parent components for sibling packages.
Relocation preserves identical relative layouts; identical package names/versions
at distinct canonical paths remain distinct. Limits are 1 MiB and 2048 records.
Malformed/duplicate JSON fields, bad checksums, nonnormalized paths and
symlink/nonregular lockfiles fail. Capture enforces its entry-membership invariant.

Normal parsed/unparsed loading validates a present lock exactly and never repairs
it. An absent lock retains legacy behavior. LoadedProgram.entry now always uses
the canonical first loaded source path, including a manifest entry symlink.
project::load_unparsed_for_lock_update skips only the old lock for explicit update
operations while retaining import/manifest validation. Normal compiler/inspect
callers do not use that bypass. package_lock::read_if_present is a read-only
reader; root_manifest chooses the canonical entry's containing root manifest.

CLI modes are:

```sh
bhasa lock create [FILE] [--json]
bhasa lock update [FILE] [--json]
bhasa lock check [FILE] [--json]
```

No FILE selects the nearest lowercase bhasa.toml declared entry. Explicit FILE
selects that source's package graph; the single primary lock remains exact-entry.
Create refuses an existing lock. Update requires one, can repair stale/malformed
bounded documents, and does no writes for identical bytes. Publication stages a
complete synced document beside its destination. Create publishes without
replacement; update atomically renames. Neither follows symlink/nonregular lock
destinations. This is not a cross-process compare-and-swap protocol.

Check is read-only and requires a present matching lock. --help lists the modes;
-- permits a source filename beginning with '-'. Frontend/target/build options
are rejected. All lock operations use only host source loading/hashing/filesystem
operations, with no Cargo or Clang invocation.

JSON success is schema_version=1, ok=true, command, canonical lock/entry paths,
changed and diagnostics=[]. Failure is ok=false with located diagnostics.
Success exits 0, operation/load/verification errors exit 1 and usage errors exit
2. JSON failures use stdout and leave stderr empty, except output I/O failures.
The existing native test runner retains its own prebuild-error exit code 2.

## Checked test-graph policy

The test runner now calls project::load_test_graph. For a present lock this
verifies the declared primary graph first through normal exact lock validation.
The original root package identity is retained even if its primary entry resolves
through an in-package symlink near another manifest. The initially read lock is
also compared against the primary graph snapshot before validating the test graph.

Additional canonical sources may be absent from the primary lock only below that
root package's tests/ directory. Every loaded manifest and every source outside
that tests/ directory must match a primary lock record. Test code remains mutable;
production sources and dependency closures remain pinned. A changed primary
dependency rejects a test even when the test does not import it. A test-only new
production module or dependency manifest outside the locked graph fails.
Dependencies' own tests/ trees have no exemption. Normal check/build/run/inspect
keep exact-entry loading. Unlocked tests retain legacy behavior. Multi-entry and
workspace graph locks remain later work.

## Adapter APIs, inspection and preflight

Optional bhasa-scalar-adapter-v1 metadata records src/lib.rs SHA-256, review
identity, native-no-gc runtime claims, scalar i64-copy/void ownership, explicit
infallible/status-i64 errors, abort/contained unwind and caller-only/reentrant
thread claims. Status success is an explicit decimal-string i64. It rejects
pointer/buffer/record/resource/callback ownership, unknown fields/contracts,
duplicate/invalid/reserved symbols and stale reviewed source.

ffi_contract::Adapter::parse, verify_source, to_value and c_header validate the
schema, bind source review, emit normalized JSON and generate the existing scalar
int64_t/void C ABI declarations. read_if_present reads a bounded regular optional
sidecar and confines canonical default src/lib.rs to its crate root. Header
symbols must begin with lowercase ASCII and contain ASCII letters/digits/_;
leading/double underscores, C/C++ keywords, stdint typedef namespaces, standard
header macros and known bare compiler/platform names are rejected both at parse
and after public Adapter mutation before header generation.

rust_deps::preflight validates every adapter in every loaded Bhasa manifest before
any Clang/Cargo call in the driver. Standalone rust_deps::build preflights its whole
dependency map before cache creation or Cargo. Immediate per-adapter rechecks
remain. resolve_manifest provides shared read-only Cargo manifest resolution.
Cargo flags, resolution, cache identities/output paths and freshness checks do
not change. Cargo.lock remains Cargo-owned.

Inspect adds lock {path,present,verified,snapshot}, adapters metadata, and package
manifest checksums to schema version 1. A standalone source has lock=null. Adapter
records identify aliases, owning Bhasa manifests, resolved Cargo manifests and
metadata paths. Missing metadata has contract=null. A valid sidecar has normalized
ownership/error/unwind/thread claims and source_review_verified=true. Invalid
sidecars fail inspection with located diagnostics and no successful declaration
report. Inspection creates no lock/Cargo.lock/cache/source files and invokes no
native tools.

These are review claims, not ABI or safety certification. no_gc_proven and
worker_extern_allowed remain false in inspection. Neither frontend's extern
worker restrictions change. Existing adapters without metadata retain their
trusted scalar ABI behavior. Python/TypeScript GC runtimes remain excluded.
Metadata does not inspect transitive Rust sources, enforce unwind/thread behavior,
compare real exports/signatures, or convert raw statuses to Result/IoResult.
Default src/lib.rs is the only reviewed source binding in v1. Rich resources,
complete provenance and custom Cargo lib paths need later contracts.

## Evidence

33 unique focused checks pass. A single coordinated-native acceptance fixture is
present but deliberately ignored pending the coordinator's build slot.

- cli-probe-tests.log: ten CLI groups pass, one native fixture ignored. They
  cover lifecycle/idempotent update, read-only tree bytes/mtime, strict JSON/help/
  exit/argument boundaries, malformed-lock repair, standalone-source rejection,
  default/explicit canonical entry aliases, symlink destination preservation,
  stale normal loading with both frontend flags, primary-lock test rejection and
  optional/stale adapter inspection.
- The CLI preflight group covers a valid earlier adapter and stale later adapter
  within one Bhasa manifest and across imported manifests. Fake Cargo and Clang
  invocation counters both remain zero; no build/cache/source mutations occur.
- cli-loader-tests.log: ten actual draft host-loader groups pass, including
  transitive same-name package identity, entry aliases, captured-byte stability,
  checked test policy, strict normal loading, explicit update, unused files/cache
  exclusion, raw manifest checksums and bounded/symlink/nonregular locks.
- cli-inspect-regressions.log: all three existing tests/agent_inspect.rs groups
  pass, preserving checked APIs, privacy, concrete generic names, deterministic
  output and original-file failure reports.
- probe-lock.log: three lock-format/content/path unit tests pass.
- probe-ffi.log: five adapter schema/source/sidecar/header-reservation unit tests
  pass, including mutated public values and normalized-JSON round trips.
- probe-sha256.log: existing SHA-256 known vectors/chunk boundaries pass.
- probe-header.log: the example sidecar matches its actual source and its
  generated header passes strict Clang C and C++ syntax checks.
- All draft Rust files pass rustfmt --check with skip_children=true.

The CLI probe is a default Rust-host overlay built directly with rustc, without
Cargo or native archive rebuilding. cli-probe-source-index.json records every
source path/SHA. It runs actual draft driver/inspect/test_runner/project code;
the stale-lock cases with --frontend bhasa stop at shared loading before frontend
availability or emission. This is not successful Bhasa frontend/native execution.
The earlier small library-unit harness uses the existing bd69e13e13c63319 rlib
for parsing and compiles draft units; its dead-code warnings reflect standalone
subsets. Sources, binaries and build/evidence logs remain under the lane directory.

A macOS /var-to-/private/var test-path assumption was repaired. A test initially
assumed prebuild native test-runner errors exit 1; its assertion now preserves
the existing exit 2 contract. Final focused checks pass.

## Coordinator verification slot requested

After review/integration, run the following serially with two Cargo jobs and one
test thread. Repeat focused gates with --features bhasa-frontend; only the
coordinator should schedule its archive refresh. Do not run both patch sets.

```sh
CARGO_BUILD_JOBS=2 cargo test --lib package_lock::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --lib ffi_contract::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --lib lock_cli::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock_cli -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test agent_inspect -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock_cli locked_main_graph_allows_a_separate_checked_project_test -- --ignored --test-threads=1
```

The final ignored fixture creates a main-entry lock and actually executes a
separate tests/basic.bh importing the locked dependency. Pure loader acceptance
already passes; compiled execution has not been claimed. Then schedule existing
package/adapter-cache acceptance, both complete compiler gates and Clippy as
appropriate. Confirm unchanged adapters still compile zero times and the example
scalar binary prints 42. No integrated cache, release, native-success or full
hybrid acceptance claim is made by this lane.

## Next dependency order

Complete no-GC review provenance/checksum closure for adapter manifests/sources/
dependencies, version resolution, workspaces/features, then generated API docs.
Rich ownership-aware C/Rust adapters need the ownership lane's transfer/borrow/
resource contract and the safety lane's foreign error/unwind/thread contract.
The scalar v1 schema preserves a reviewable starting point without inventing those
ABIs. Stable exports/library artifacts and registry/distribution packaging remain
open. Hardware/SDK verification and publication are not claimed.

## Authoritative-root correction in progress, October 12

Root reproduced a P1 where a canonical outer entry lies under a nested dependency
manifest. The current root_manifest heuristic selects that dependency, while
normal loading verifies the originally selected outer manifest. A new corrective
draft is isolated under build/backlog/packages-ffi/correction-authoritative-root.
Original draft/patch sets remain frozen. Baselines are exact copies of current
applied project/package_lock source at HEAD 217e33dbc3678b85fa1f2ef7b4ea80612743e321.

The correction records root_manifest_path in LoadedProgram and resolves that
identity consistently. Explicit source selection uses the requested directory
before resolving a file symlink, so an outer entry alias retains the outer
package. Directly naming a nested source still selects that nested package.
A read-only load_editor_graph API shares the checked primary/pinned-source/test
policy with load_test_graph; normal CLI/inspect loaders stay exact-entry strict.
Root/editor lane will wire the actual LSP caller and protocol regression. Loader
and CLI probes, corrective patch paths and final evidence follow here. Root owns
the separate lock/sidecar output-collision repair in driver.rs.

## Frozen authoritative-root correction ready, October 12

The corrective source/test set is frozen at
build/backlog/packages-ffi/correction-authoritative-root/draft. Three small patches
are published in that directory's patches/, with exact baseline/draft SHA-256
values in patch-index.json. patch-validation.log confirms scratch application
reproduces all draft bytes. Baseline project/package_lock files are current applied
source, not the old original drafts. Those original drafts/patch sets remain
unchanged. No shared sources, Cargo, Git or caches were modified by this correction.

LoadedProgram.root_manifest_path records the canonical selected manifest independently
of project, which still marks only implicit invocations/default output. root_manifest
resolves that path instead of guessing from source location/dependency traversal.
Snapshot capture rejects a different supplied root. Explicit source requests select
from the requested canonical parent directory before resolving the final file
symlink; an outer entry alias retains its outer package and confinement. Directly
requesting a nested file selects its own nested package. Normal loading, lock CLI,
inspect and checked test/editor graph APIs now use the same selected authority.

project::load_editor_graph is the new explicit read-only disk-policy API. It shares
primary exact-lock verification and pinned-production/root-tests graph checks with
load_test_graph. Normal CLI/inspect APIs remain strict exact-entry loaders. The
optional load_editor_graph_in_package(canonical_source, known_manifest_path) retains
an already checked package context after overlay-path canonicalization; it still
rereads the manifest, enforces root confinement and validates the complete primary
lock. It never performs lock-update repair. The parent chose to preserve the decoded
requested file path separately in LSP documents and pass that to load_editor_graph,
while retaining canonical doc.path for overlay matching. This avoids package guesses
from a canonical symlink target. No duplicate LSP edit was made by this lane.

editor-policy.md records the exact disk/overlay seam, contextual API and protocol
acceptance criteria. Root is wiring the LSP caller and testing checked imported
modules, root tests, unsaved overlays, stale disks, unpinned source additions and
outer-entry aliases. Successful LSP protocol behavior is not claimed by this lane.
Root separately owns the source_inputs lock/sidecar output-collision repair.

27 focused checks pass against an isolated snapshot of the current Rust host with
only corrective project/package_lock overlays. probe-source-index.json records
all Rust source input paths and hashes. Direct rustc builds used no Cargo or native
archive rebuild. Actual subprocess CLI tests use that corrected host executable.

- probe-tests.log: seven new package_root_identity groups pass. Default and
  explicit outer-alias lock/create/check/inspect agree on outer/bhasa.lock; nested
  source edits reject normal Rust/Bhasa flags, lock checks, inspection and test
  loading at the outer lock. Direct nested file invocations choose the nested
  manifest and independently enforce its lock. Wrong snapshot roots reject.
  Tests/editor modules share checked root-test admission; imported production
  analysis reroots only pinned sources after full primary graph verification.
  Unrelated stale primary dependencies and new unpinned production files reject.
  Contextual editor loading preserves an outer identity for a canonical nested
  source. Explicit aliases outside the selected root reject.
- probe-cli-regressions.log: ten existing package_lock_cli groups pass. The
  native execution fixture is explicitly filtered for the coordinator's slot.
- probe-loader-regressions.log: all ten existing package_lock loader groups pass.
- All three corrective Rust files pass rustfmt --check with skip_children=true.

After applying these three corrective patches alongside the parent's driver/LSP
repairs, coordinator gates should include:

```sh
CARGO_BUILD_JOBS=2 cargo test --test package_root_identity -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock_cli -- --test-threads=1
```

Repeat with --features bhasa-frontend and run the parent's LSP protocol regressions,
Clippy and coordinated combined/native/cache gates. No new native test execution,
full hybrid acceptance, release or hardware verification is claimed here.

## Local workspace/version slice in progress, October 12

Root confirms the authoritative-root correction is integrated and focused gates
pass, including independent reproductions of repaired root/imported-editor cases.
The frozen correction remains unchanged. New owned work is isolated under
build/backlog/packages-ffi/workspaces; contract.md publishes the explicit member,
local version and read-only CLI contract before new APIs are consumed.

Planned additive Manifest fields are workspace_members, dependency_versions and
source_bytes. No LoadedProgram/lock format changes. Workspace declarations remain
explicit canonical confined local members, while version requirements constrain
existing local dependency aliases. First slice is workspace list/check plus
import-time local requirement validation; solver/registry/download/publication
and multi-entry locks remain outside this slice. Exact current source baselines
are saved before edits. No Cargo/shared edits/Git are authorized for this lane.

## Local workspace/version draft ready, October 12

The ratified contract is implemented in isolated draft/src, with workspace tests
in draft/tests/workspaces.rs and user documentation in draft/docs/workspaces.md.
The reviewable combined patch is workspace-version.patch; patches/ contains seven
individual patches. patch-index.json records exact baseline/draft hashes.
patch-validation.log confirms the combined patch reconstructs all seven draft
files from saved baselines in scratch. No shared source was patched.

Manifest adds workspace_members, dependency_versions and source_bytes.
workspace::VersionRequirement::parse/matches compares bounded numeric
requirements without machine-integer overflow. workspace::discover returns the
root and explicit canonical members; workspace::check validates all declared
local Bhasa dependencies, including unused ones, with canonical deduplication.
The CLI provides workspace list/check with optional directory and JSON errors.
Inspect adds manifest_bytes, workspace_members and dependency_versions only for
loaded packages. Members do not become implicit source imports.

Members and manifest symlink targets stay confined to the root. Bounds precede
raw reads: 64 explicit members, 256 canonical manifests, 64 dependency levels,
16 MiB aggregate raw manifests, and the existing 1 MiB per-file limit. Discovery
includes its raw reads in the same budget and scans at most 128 ancestors and
65536 names per ancestor. Import-time requirement mismatches use M003 at the
declaring manifest, including transitive failures. Missing required versions,
malformed requirements, undeclared aliases, duplicate canonical members and
nested member workspaces fail explicitly. Workspace commands remain read-only
configuration queries and retain strict individual primary v1 locks.

Direct rustc probes passed against the isolated copied host source:

- Workspace integration: 14 passed, probe-tests.log.
- Numeric requirement unit test: 1 passed, probe-unit.log.
- Existing authoritative root identity: 7 passed, probe-package_root_identity.log.
- Existing package lock: 10 passed, probe-package_lock.log.
- Existing lock CLI: 10 passed, probe-package_lock_cli.log. The native execution
  test locked_main_graph_allows_a_separate_checked_project_test was excluded.
- Existing inspect: 3 passed, probe-agent_inspect.log.

probe-source-index.json hashes the actual source copy used to build the probe;
all five draft source overlays and the workspace test copy match their tested
files byte for byte. The loader mismatch tests exercise both frontend selections
before frontend dispatch. These probes do not establish acceptance of the
feature-enabled Bhasa frontend or native execution. No Cargo, native archive
rebuild, broad suites, Git mutation or external publication ran in this lane.

Shared project/inspect/lib still match their saved baselines. Shared driver
changed in the parent's output-source-alias guard after the snapshot. The new
workspace patch only touches help and initial command dispatch and applies to
the current driver without offsets or rejected hunks in current-driver-validation.
Its log and JSON retain that exact shared-driver hash. Integration should apply
small patches to current sources, preserving the parent's output guard, rather
than replace driver from this draft snapshot.

Coordinator next action: apply the seven patches, including the new workspace
module, test and documentation, then run the focused Cargo targets with the
normal and bhasa-frontend configurations:

```sh
CARGO_BUILD_JOBS=2 cargo test --test workspaces -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test workspace::tests:: --lib -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_root_identity -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test package_lock_cli -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --test agent_inspect -- --test-threads=1
```

Integration retains ownership of Clippy, LSP regressions, combined/native/cache
checks and commits. No lane-local build process remains running. There are no
open implementation failures in this draft. Source drift beyond the documented
driver change requires revalidating the saved-baseline patches before integration.

## Workspace output amplification correction ready, October 12

Independent root review found the frozen workspace check could clone repeated
long versions into an oversized edge inventory before its late CLI output cap.
The exact reproduction had 31,022 raw bytes, two canonical manifests and 1,000
aliases, producing 20,275,890 serialized edge bytes. It is recorded in
build/workspace-independent-review/amplification-review.md. This supersedes the
original no-open-failures handoff and blocks integration of the original draft
without this correction.

A contextual correction is isolated in
build/backlog/packages-ffi/workspace-output-correction. Its contract.md publishes
the inventory/output policy and API change. workspace-output-correction.patch and
two small patches apply after the original seven patches. patch-index.json records
baseline/draft hashes; scratch application reconstructs both files exactly.
All original seven draft hashes still match their frozen patch-index.json.

A separate 16 MiB inventory allowance accounts for every escaped string, repeated
edge, member and manifest before expanded Value construction. It reserves 1024
bytes for wrappers, 256 bytes per record and 16 structural bytes per requirement
pair. Edge reservations also account for exact JSON object structure. This can
reject just below the physical output limit and never truncates required records.
Raw-read, depth and canonical-manifest limits remain unchanged. Legal numeric
versions remain unchanged, including the 20,000-digit reproduction version.

Workspace::to_value now returns Result<Value, LocatedDiagnostic>.
CheckedWorkspace::to_value returns the complete checked report through the same
bounded preflight. Check charges new manifests and edges before constructing
expanded records. Discovery charges listing members as accumulated. Host callers
cannot bypass preflight by mutating public inventories. JSON serialization first
counts escaped bytes without allocating strings, then appends through a bounded
writer with depth 64 and newline accounted. Both error-output modes preflight
payloads; oversized diagnostics produce a short fixed W001 error. CLI usage errors
avoid copying arbitrary unexpected arguments into messages.

Direct rustc evidence against the original copied source plus this correction:

- Three output integration tests pass in probe-tests.log: independent repetition
  reproduction and recovery, repeated long original paths, public inventory
  mutation/escaping rejection and recovery.
- Four output unit tests plus the existing numeric comparator unit pass in
  probe-unit.log: exact allowance/failed-reservation atomicity, escaping parity,
  physical newline/depth bounds and bounded JSON/text diagnostic fallback.
- All 14 original workspace integration tests pass in probe-existing-workspaces.log.

The correction therefore has 22 passing focused checks. It has not run Cargo,
feature-enabled frontend/native acceptance or broad gates. Root should independently
rerun the amplification fixture and add workspace_output to coordinated normal
and bhasa-frontend Cargo verification after patch application. No shared source,
Git or persistent cache changed. No correction probe process remains running.

## Pure API documentation renderer in progress

Root accepted the contract in build/backlog/packages-ffi/api-docs/contract.md.
The isolated draft consumes only successful checked inspect schema-1 JSON and
returns Markdown or filtered JSON with validated public declarations, concrete
signatures, transparent alias targets and exact original source anchors. It adds
no CLI, files, editor resolution or generic template recovery. Additive metadata
is preserved without interpreting capabilities; absent parameter modes remain
unspecified legacy syntax. Six synthetic focused renderer tests pass. Current
inspect source is snapshotted and a host probe built, but real-report verification
and the same preconstruction output policy still need completion after the
priority workspace correction handoff. Do not integrate this renderer yet.

## Pure API documentation renderer ready, October 12

The accepted contract is now implemented as one new pure module in
build/backlog/packages-ffi/api-docs/draft/src/api_docs.rs. The contextual new-file
patch api-docs-renderer.patch reconstructs it exactly in scratch; patch-index.json
records its final hash. No lib.rs export, inspect refactor, CLI command or file
output seam is included. Root can review this module before ratifying wiring.
Current shared inspect.rs and json.rs still match their recorded source snapshots.

Actual API: Format::{Markdown, Json}, RenderError with public message and
Display/Error, and render_inspection(&str, Format) -> Result<String, RenderError>.
Only successful schema-1 checked inspect reports with empty diagnostics are
accepted. The caller must supply a real checked report; JSON shape alone cannot
establish provenance. Every required field, kind, privacy flag and source location
is validated before rendering. Unsupported or malformed reports fail completely.

The renderer sorts borrowed public declaration references by original file,
start byte, kind and name. It filters private declarations/fields/methods and
renders concrete functions, externs, records, methods, enums and validated
transparent aliases. JSON retains exact source anchors and additive metadata on
public objects. Markdown shows literal signatures and escaped original location
text with alias declared/canonical targets and nominal target locations when
available. It does not infer original aliases in lowered function annotations,
recover unused generic templates, read source comments or duplicate editor symbol
resolution. Extern signatures do not establish native acceptance. Parameter modes
absent from the schema remain unspecified legacy syntax; additive mode metadata
is retained in JSON without interpreting ownership or capabilities.

The renderer uses the same preconstruction principle as the workspace correction:
16 MiB input is checked before parsing, schema strings are bounded to 65536 bytes,
and traversed declarations/parameters/fields/methods/variants to 65536 items. It
performs a complete output counting pass over borrowed records before allocating
its text buffer. No expanded declaration inventory is cloned. JSON escaping is
streamed into counting/buffer writers without escaped-string temporaries;
Markdown temporaries have fixed schema-string bounds. Every output append checks
the 16 MiB limit, including the newline. The existing JSON parser limits depth 64.
The pure renderer performs no I/O, subprocess, network or cache operation.

Eight focused tests pass in probe-tests.log. They cover public/nested privacy,
additive mode preservation, alias spelling/canonical targets, malformed and failed
reports including private malformed entries, deterministic ordering, escaped
Markdown/multiline code, bounded input/string/item/output rejection, empty and
extern reports, Unicode JSON escaping and newline/budget boundary atomicity.

verify-real-report.py also passes against a direct rustc host snapshot of current
shared source, recorded in probe-source-index.json. real-inspect.json is the actual
successful checked report. Both real-api.md and real-api.json are deterministic;
private top-level alias/function/field names are omitted; concrete Box[int] and
identity[int] APIs appear; unused Unused/never templates remain absent. Alias
Count/Again/ItemAlias spelling, canonical targets and original declaration and
nominal target locations match the report. All retained declaration location
objects match inspection exactly. Fixture bytes and mtimes are unchanged and no
build/lock is created. real-report-verification.log records the result. This uses
the Rust host configuration and does not establish feature-enabled frontend or
native acceptance. No Cargo/shared source/Git operation was performed.

Root next action: independently review the pure module and its published input/
output contract, coordinate actual additive owned/borrowed metadata and richer
editor queries with their owners, then ratify checked-report construction and
stdout CLI seams. After exporting the reviewed module, the focused gate is
cargo test --lib api_docs::tests:: -- --test-threads=1, repeated under
bhasa-frontend. File-output paths and overwrite protection remain root-owned.
No renderer probe process remains running.

## Public workspace host boundary correction ready, October 12

Root independently confirmed the first amplification correction and found two
additional exposed-host boundary gaps in
build/workspace-independent-review/correction/api-boundary-findings.rs/log.
Individual edges were preflighted at depth 0 even though the complete report
adds two levels. A caller-mutated 6 MB invalid-UTF-8 root path allocated a 24 MB
lossy conversion before inventory rejection. Real canonical CLI paths and
normal generated edges were unaffected, but the stated public preflight contract
required these fixes.

The second contextual correction is isolated in
build/backlog/packages-ffi/workspace-host-boundary-correction. Apply
workspace-host-boundary.patch after the original seven workspace patches and
workspace-output-correction.patch. Two small patches contain only workspace.rs
and the new workspace_host_bounds test. baseline-sha256.json and patch-index.json
identify exact inputs/final drafts. patch-validation-final.log confirms both
reconstruct exactly in patch-validation-final. Original workspace, first
correction and API-docs draft hashes remain unchanged.

CheckedWorkspace::to_value starts each exposed edge's size/depth pass at depth 2.
Its complete report therefore stays within the shared JSON codec's depth 64.
Path costs now scan borrowed OsStr encoded bytes and valid UTF-8 fragments,
charging escaped valid text plus replacement characters without constructing
lossy strings. Costs are exact on Unix and conservative for Windows surrogate
encodings. Oversized paths saturate/reject before conversion; budget-error
construction uses a fixed inventory path when cloning the rejected path would
exceed the allowance. Ordinary admitted paths preserve their output spelling.
No numeric, raw-byte, manifest-count, dependency-depth, inventory-size or CLI
contract changed.

Direct rustc probes pass 26 focused checks:

- Two public-host integration tests, probe-host.log. Depths 61/62 produce reports
  accepted by the shared JSON parser; 63/64 reject before construction. Root,
  member-manifest and checked-manifest path mutations each use 6 MB invalid Unix
  bytes; the tracked largest preflight allocation is 51 bytes in all three
  cases, versus the independent 24 MB reproduction. Each failure recovers.
- Two new host boundary units plus five prior workspace units, probe-unit.log.
  Borrowed path sizes match existing lossy JSON encoding for short invalid,
  truncated, surrogate, Unicode and escaping cases; complete wrapper-depth
  accounting and prior output/numeric invariants pass.
- Three prior output-correction integrations, probe-output.log.
- All 14 original workspace integrations, probe-existing.log.

probe-source-index.json records the exact copied host source; its workspace.rs
matches the final source draft. Windows execution is unverified. No Cargo,
feature-enabled frontend/native acceptance, shared source, Git or cache operation
ran. No probe process remains running. Root should independently rerun its
api-boundary reproduction against this library and include workspace_host_bounds
in coordinated Cargo gates after applying the contextual correction chain.
API docs renderer remains frozen and ready for its separate independent review.

## Literal Markdown CR correction ready, October 12

Root independently validated both workspace corrections and reviewed API docs
against a real source fixture. Public/private APIs, qualified duplicate names,
aliases, concrete generic APIs and unspecified parameter modes passed. A synthetic
schema-valid return string containing bare CR could escape literal indentation
because the streaming Code writer only recognized LF. Root evidence is preserved
in build/api-docs-independent-review/bare-cr-*. Current checked type spellings do
not contain that payload, but the admitted-schema literal-code contract required
this fix.

The separate contextual correction is ready in
build/backlog/packages-ffi/api-docs-cr-correction/api-docs-cr-correction.patch.
Apply after the frozen renderer new-file patch. Both literal declaration code
and streaming signatures now share one Code writer that normalizes bare CR,
CRLF and LF to LF and indents every emitted line. It retains CR state across
formatting chunks to collapse split CRLF into one line break. JSON retains input
strings and original source anchors. Counting and buffer passes share the same
normalization and byte accounting, including expanded indentation/newlines.
No CLI/report/filesystem wiring or shared source edits are included.

Ten focused renderer tests pass in probe-tests.log: all eight frozen tests plus
literal CR/CRLF/LF in signatures and declaration code, split CRLF chunks and
exact normalized-byte allowance boundaries. The independent bare-CR-input.json
reproduction now emits no CR or unindented injected heading/HTML. Its corrected
Markdown and JSON are saved as bare-cr-corrected.md/json; JSON return strings and
locations match the original report exactly. Both formats of both real fixtures,
the lane's frozen report and root's independent qualified-name report, are
byte-identical to their frozen outputs. verification.json records exact input
and output hashes and these results. Original renderer/report files remain
unchanged, including the recorded original module hash.

baseline-sha256.json and patch-index.json identify exact contextual inputs and
final draft; patch-validation.log confirms scratch reconstruction byte for byte.
The pure module tests use the existing host JSON library with direct rustc only.
No Cargo/shared source/Git/cache operations ran; no probe remains running. Root
retains independent review, lib/report/stdout wiring and combined verification.

## Additive enum variant correction ready, October 12

Root independently reproduced a schema-valid additive variant payload with
kind enum and variants null that caused JSON rendering to panic. Markdown passed.
The renderer treated a variant's additive kind as a top-level declaration kind.
Independent inputs and failure logs remain preserved under
build/api-docs-independent-review/additive-variant-*.

A separate contextual correction is ready at
build/backlog/packages-ffi/api-docs-variant-correction/api-docs-variant-correction.patch.
Apply it after the frozen original renderer patch and literal CR correction.
The production change restricts top-level enum variants handling to non-variant
objects. Validated variant fields still receive private-field filtering; all
other additive variant values are serialized verbatim under the existing bounds.
No schema, source anchor, privacy, capability inference, CLI or resolver behavior
is added. The accepted parent context determines renderer behavior.

All 11 focused renderer tests pass in probe-tests.log, including the prior ten
and a regression spanning variant kind strings enum/struct/function/future_kind
with null/bool/string/array/object additive payloads. Kind, variants, methods,
private, source_mode and original location metadata survive verbatim; private
fields remain omitted. The independent exact panic input succeeds without stderr
in both formats. Corrected outputs are additive-variant-corrected.json/md;
verification.json records input/output hashes and checks. Both real-source
fixtures remain byte-identical in both formats, and both CR-corrected outputs
remain byte-identical to the prior correction. Original renderer and CR correction
module hashes are unchanged.

baseline-sha256.json and patch-index.json record exact contextual/final hashes;
patch-validation.log confirms scratch reconstruction byte for byte. No shared
source, Cargo, Git or cache changes ran and no probe remains running. Root retains
independent review and actual lib/checked-report/stdout CLI wiring.

## Independent review accepted, October 12

Root accepts the workspace correction chain and final API documentation renderer
plus contextual CR and variant corrections for actual integration. The final
renderer source matches this lane's corrected draft exactly. Root completed the
variant delta independently after the reviewer reached model capacity, reporting
17 passing groups: both exact original failure inputs, bounds, both real fixtures
in both formats byte-identical, additive variant metadata preservation and no
stderr or panic. Root's review and hashes are preserved in
build/api-docs-independent-review/variant-correction-root-review/review.md and
manifest.json. Independent CR replay was clean before that review continuation.
Workspace final independent review also accepted both host-boundary corrections,
with its evidence in build/workspace-independent-review/host-correction/review.md.

All lane artifacts remain frozen. Actual shared source integration, checked
report/CLI wiring, optional file-output protection, coordinated Cargo/frontend/
native gates and completion checkboxes remain root-owned. This acceptance is
independent draft review, not a claim that the requested commands are already
integrated. No new shared-source/Cargo/Git work is assigned to this lane.
