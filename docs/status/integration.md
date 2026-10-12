# Compiler integration status

Local package locks, scalar adapter review metadata, checked signature help and
semantic tokens, protected compiler outputs and private standard-library storage
are integrated and verified on ARM64 macOS. The prior local Result-obligation
report is preserved in [integration-failures.md](integration-failures.md).
This report covers a bounded increment; the full [backlog](../../todo.md) remains open.

## Delivered

Optional version-one `bhasa.lock` pins the selected entry and loaded local graph
with source and raw manifest checksums. Explicit create/update/check commands
are bounded, deterministic and read-only unless an update is requested. Normal
compilation and inspection require the exact pinned entry. Test/editor loaders
verify the primary disk graph before admitting their supported roots or overlays.
Requested package paths retain their root authority separately from canonical
source identity. Workspace and registry resolution remain outside this slice.

Optional `bhasa-adapter.json` records source-bound scalar C-ABI review claims and
generates declarations. All adapters are validated before build tools or caches
are touched. The inspector reports `no_gc_proven=false` and
`worker_extern_allowed=false`: metadata is not a transitive dependency proof or
permission to call foreign functions from workers. Legacy trusted scalar adapters
remain supported. Rich ownership-aware foreign ABI work stays open.

Compiler outputs cannot overwrite graph locks, manifests, adapter metadata or
Rust source trees. Checks follow directory/file symlinks, including external and
dangling targets, with bounded traversal and fail-closed uncertainty. Validation
runs before tools, directory creation and final replacement. This is not a
transaction against a hostile filesystem changing concurrently.

Checked signature help and semantic tokens use full validated graph identity,
privacy and UTF-16 positions. Invalid graphs return no checked analysis.
Duplicate canonical overlays refuse analysis and recover when the duplicate is
closed. Saved stale or unpinned locked graphs cannot fall back to a fully checked
local document. Only supported new documents receive local checked analysis.
Sorted semantic positions use one forward cursor instead of repeated prefix scans.
The editor adapter guards document versions, dependency snapshots and sessions.

Text, collection and JSON backing storage is private. Public buffer/key getters
return independent copies; mutations pass through supported validated APIs.
Standard modules, TLS/HTTP helpers, examples and fixtures use those APIs. Runtime
and compiler ownership ABIs remain unchanged.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference inventory | 558 passed: 506 completed groups before the app quit, then all 20 remaining targets rerun with 52 passes; build/verify-tools-reference-final.log, -resumed.log and -remaining-targets.json. Doc tests pass separately. |
| Complete hybrid | 583 passed, exit zero; build/verify-tools-hybrid-final.log, run.2U78HPMj. |
| Editor | 77 LSP groups in each compiler configuration and 93 Node client tests; build/verify-tools-editor-final.log. |
| Package/output review | 62 independent frozen CLI assertions, including historical overwrite bypasses, external/dangling aliases, default run output and traversal limits; build/tools-output-guard/corrected-independent-review/report.md. |
| Editor review | Nine frozen-binary graph/overlay scenarios and independent Unicode/CRLF/linear-position probes; build/tools-independent-review/final-editor/review.md and corrected-semantic/review.md. |
| Storage/privacy/cleanup | Private-access rejection and O0/O2 copy/exit/try/partial-JSON/invalid-UTF8 cleanup with atomic allocation counts and nonrecovering ASan/UBSan; build/tools-storage-acceptance/README.md. |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-tools-fmt-final.log, -clippy-reference-final.log and -clippy-hybrid.log. |
| Refreshed releases | Both compilers execute 13 examples and five package tests; build/verify-tools-release-smoke.log. |
| Persistent caches | 32 unchanged native hits and four unchanged Rust adapter runs with zero compilations; two unchanged hybrid builds reuse the same archive with zero compilations. |
| A -> B -> C | Complete compiler LLVM and example outputs agree at build/selfhost/run.lm8cj8Lt; build/verify-tools-selfhost.log. |

The reference run was resumed after the app quit; its completed groups and
remaining-target inventory are retained rather than presented as one uninterrupted
command. Complete hybrid acceptance ran from start to finish. Cargo used at most
two build jobs and tests one thread, with broad suites serialized and local
loopback networking authorized.

Matching scanner/emitter archive key:
`ccabbe47a921bfdd320a4844960bfd84f5db48f3f5ae7d3b9013fdffdebff3ec`.
Its source exactly matches the frozen seeds. Release/archive/source identities
are in `build/verify-tools-artifact-identity.json`; 56 frozen source hashes remain
unchanged. Both unchanged build logs confirm the same archive and zero Compiling
lines. Caches were preserved. All scoped acceptance criteria pass.

## Remaining work

Aliases and their editor metadata are isolated pending the reviewed nested nominal
RHS correction. Owned buffer/ints transfer has host/native drafts and corrected
match-payload/implicit-return cleanup evidence; actual combined integration is
still required. Streaming files, typed network/process/TLS operations, local
workspaces and server code actions remain isolated. TCP shared-owner copy policy
passes independent review and awaits runtime integration. Channel source authority,
bounded payload traversal and native manifest handoff require final review and
native parity before exposure.

Local Result parameters, aggregate contents and interprocedural handling remain
outside the current pending-result guarantee. Fatal traps remain uncatchable and
do not unwind. No Rust-equivalent safety or complete language claim is made.

The eight existing chats continue under [backlog-lanes.md](backlog-lanes.md), with
cursors in [backlog-state.json](backlog-state.json). Actual Metal checks from prior
increments remain evidence for their tested API only. CUDA/Vulkan hardware,
Linux/Windows execution and VS Code GUI remain unverified or gated. Wasm Node
probes are test tools; a reviewed no-GC engine and guest compiler/linker remain
real production gates. No SDK was installed or registry/release published.
