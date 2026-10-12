# Compiler integration status

Transparent nongeneric type aliases, checked alias editor identity and inspection
metadata are integrated, verified and pushed as `591d1a7` on ARM64 macOS. The previous package/editor/
private-storage report is preserved in [integration-tools.md](integration-tools.md).
This report covers a bounded increment; the full [backlog](../../todo.md) remains open.

## Delivered

Top-level `type Name = Target` and `private type` declarations resolve to the
canonical target before generic specialization. Annotations, constructors, enum
cases and explicit generic arguments share the same target identity. Aliases
introduce no nominal wrapper or additional runtime allocation. Unused alias targets
are validated. Generic alias declarations and compile-time constants remain open.

The lowerer publishes original alias declaration/use spans and compiler-proven
nominal declaration/use anchors for nested alias right-hand sides. Checked editor
outline, completion, hover, definitions, references, versioned rename and semantic
tokens consume those exact identities. Qualified modules and equal-spelled private
names remain distinct. Invalid or incomplete graphs refuse checked analysis.
Inspection includes source aliases, declared targets and canonical targets.

Cow-based edits avoid cloning expanded targets per use. Existing limits bound
alias expansion to 64 levels, spelling to 64 KiB, cumulative canonical cache to
16 MiB and lowered module output to 128 MiB. These do not establish a whole-process
memory bound. Alias privacy, immutable bindings, pending local Results, task and
foreign call-graph checks retain their existing contracts and original locations.
Both selectable frontends use the accepted shared Rust module/alias/generic
lowering; the standalone Bhasa seed does not resolve aliases independently.

[aliases.md](../aliases.md) documents syntax and limits. The runnable
examples/aliases package exercises scalar, Text, record and concrete generic
aliases, including independent Text copies.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 597 tests pass in one uninterrupted command; build/verify-alias-reference-final.log. |
| Complete hybrid | 622 tests pass, exit zero; build/verify-alias-hybrid-final.log, run.2Eimop9O. |
| Editor | 93 LSP groups per compiler configuration and 93 Node client tests; build/verify-alias-focused-reference-final.log, -hybrid-final.log and -editor.log. |
| Independent identity review | 126 corrected-composition groups, including the original failing nested nominal RHS case, imported Unicode/CRLF identities, complete versioned rename, privacy and earlier package/editor regressions; build/alias-rhs-independent-review/review.md. |
| Expansion allocation | Independent 2,997-byte input renders 4,718,782 bytes with peak extra requested allocation 5,075,922 bytes, within output capacity plus 2 MiB allowance; same review and tests/type_alias_allocations.rs. |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-alias-fmt.log and -clippy-reference.log/-clippy-hybrid.log. |
| Refreshed releases | Both compilers execute 14 examples and five package tests, including alias output 42/Bhasa/Bhasa aliases; build/verify-alias-release-smoke.log. |
| Persistent caches | 34 unchanged native hits and four unchanged Rust adapter runs with zero compilations; two unchanged hybrid builds reuse the matching archive with zero Compiling lines. |
| A -> B -> C | Complete compiler LLVM and example outputs agree at build/selfhost/run.DqAvmtmF; build/verify-alias-selfhost.log. |

Matching scanner/emitter archive key:
`366a75f3c915ffe34aa5529155ad732dc7b5e977320d34a7bc527ad931538b69`.
Its source exactly matches the frozen seeds. Release/archive/source identities
are in build/verify-alias-artifact-identity.json; all 56 frozen source hashes
remain unchanged. Caches were preserved. Cargo used at most two build jobs and
tests one thread; broad suites were serialized. All scoped acceptance gates pass.

The initial focused Cargo invocation exposed four tests using the isolated
wrapper's crate:: namespace. Production integration tests now use bhasa::; the
original failure log and final passing focused/full runs are retained. No compiler
rule was disabled to pass acceptance.

## Remaining work

Owned buffer/ints transfer has independently reviewed corrected host/native drafts;
actual combined integration is next. Explicit read-only/mutable borrows have a
ratified host contract and isolated implementation awaiting independent review.
Streaming files, typed network/process/TLS operations, local workspaces, generated
API docs and server code actions remain isolated. TCP shared-owner copies await
combined runtime/TLS/task acceptance. Channel source authority and traversal pass
independent review; the real Clang target-admission P1 is corrected and native
manifest implementation is assigned. Draft readiness does not complete a checkbox.

Workspace review found output amplification; two corrections now bound construction
and public API depth/path conversion. Independent final correction review passes 16 groups and retains all original
evidence; actual workspace integration remains open.
Server/client action review found malformed diagnostics, transient overlay history
and contextual marker gaps; independent corrected replay passes eight server
groups, 37 independent client tests and 107 authored client tests. Shared action
integration remains open and the custom client still offers no live edit. Result-parameter checks have an approved late staging
seam preserving old type/local/worker/privacy precedence, but are unintegrated.
Aggregate and interprocedural failure handling remain outside the guarantee.
Fatal traps are uncatchable and do not unwind. No Rust-equivalent safety or complete
language claim is made.

The eight existing chats continue under [backlog-lanes.md](backlog-lanes.md), with
cursors in [backlog-state.json](backlog-state.json). Actual Metal checks cover only
the previously tested API. CUDA/Vulkan hardware, Linux/Windows execution and VS Code
GUI remain unverified or gated. Wasm Node probes are test tools; a reviewed no-GC
engine and guest compiler/linker remain production gates. Configured toolchain
capability inspection and bounded incremental compiler design are separate work.
No SDK was installed or registry/release published.
