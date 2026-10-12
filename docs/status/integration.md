# Compiler integration status

Owned buffer/ints transfer, regular-file streaming and atomic TCP shared-owner
copies are integrated, verified and pushed as `8316a89` on ARM64 macOS. The preceding alias report is
preserved in [integration-aliases.md](integration-aliases.md). This is a bounded
increment; the full [backlog](../../todo.md) remains open.

## Delivered

Contextual `owned` parameters and `move name` transfer buffer/ints cleanup without
cloning or allocating for the transfer. The first slice supports monomorphic,
synchronous free functions, whole initializers/arguments/returns, fresh-value
adoption and independently acquired match payloads. Moved reads, overlapping
borrow/transfer in either order, unsupported types and unsafe outer control-flow
moves are rejected. Mutable bindings can reinitialize in their straight-line
scope. Existing ordinary copies and synchronous mutable borrows keep their
behavior. Both frontends, public emitters, formatter, checked editor identity and
inspection expose the modes. [ownership.md](../ownership.md) documents the rules.

`file_handle` and eight native file operations have matching host/native support.
The standard library adds typed open/read/write/seek/position/flush/close wrappers
returning `IoResult[T]`. Errors are owned values preserving positive native codes.
File copies retain shared descriptor, cursor and close state without allocating.
Explicit close invalidates aliases and final cleanup closes once. Regular-file
operations are synchronous; handles and containing aggregates cannot enter task
or worker-pool inputs/results, including through transitive methods/helpers.
[io.md](../io.md) documents limits, partial-write behavior and the runnable example.

TCP copies now allocate a distinct owner and atomically retain shared descriptor
state. Copies preserve constructor status, listener port and CLOEXEC; they perform
no OS duplication. Distinct live owners can be used across threads. A single
owner's use/free race remains forbidden. Final owner release closes once without
shutdown; TLS retains its documented borrowed transport descriptor lifetime.

The driver links exact file runtime/header bytes from runtime-use markers through
the existing native cache. Unsupported file executable targets reject before
Cargo adapter builds, output/cache writes or native compilation. The canonical
resolved POSIX64 target policy now rejects spellings over 128 bytes before
splitting or formatting, with a bounded diagnostic. Object emission retains its
unlinked-runtime policy. Existing short/deployment triples are byte-preserved.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 649 tests pass in one uninterrupted command; build/verify-owned-reference-final.log. |
| Complete hybrid | 681 tests pass, exit zero; build/verify-owned-hybrid-final.log, run.rjx85Vqg. |
| Editor | 98 LSP groups per configuration and 94 Node tests; complete suite logs and build/verify-owned-editor-node-final.log. |
| Ownership counters | Both frontends execute O0/O2 transfer, early-return, try, copied-return and match-payload fixtures with exact constructor/copy/drop counts and zero remaining tracked allocations; tests/owned_transfer_runtime.rs. |
| Runtime and integration | Regular files, typed stream wrappers, TCP copy pressure/thread stress, TLS, typed tasks, target/cache guards and public-emitter corrupted metadata/worker graphs pass focused and complete gates. |
| Independent review | Core worker revalidation, runtime/driver targets, allocation instrumentation and existing fixture repairs are reviewed under build/owned-files-core-independent-review, driver-runtime-independent-review, owned-runtime-permanent-review and native-link-fixture-independent-review. |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-owned-fmt-acceptance.log, -clippy-reference-final.log and -clippy-matching-hybrid.log. |
| Refreshed releases | Both compilers execute 16 examples and five package tests; build/verify-owned-release-smoke-final.log. |
| Persistent caches | 38 scheduled unchanged native hits, four unchanged Rust adapter runs with zero Compiling lines, and two unchanged hybrid builds with zero compilations; build/verify-owned-release-reuse.json and -cache-reuse-1.log/-2.log. |
| A -> B -> C | Complete compiler LLVM agrees; B/C execute the owned example and shared file-handle guest in addition to all prior seed gates; build/selfhost/run.lWdKWpz3 and build/verify-owned-selfhost.log. |

After the complete suites, Clippy requested splitting a Return(Some)/None match
and moving the unchanged file-worker helper before the test module. Independent
review reverses precisely those two structural edits to the original source
hashes; the other 268 inputs are byte-identical. Twenty reference and 22 hybrid
focused groups pass after correction. Matching archives/releases were refreshed
and final self-rebuild/cache gates use those bytes. The example's directory-based
run command also failed; it now names examples/io-stream/main.bh. No language rule
was disabled. Original failures and corrected evidence remain under build.

Final matching archive key:
`cdd251ae2afa392702ef70570587f3500c3723875c4c6de2f8ed59419f096a8a`.
The complete hybrid suite used the preceding equivalent-source archive
`d825a66628aafcdcb8c8154449e667abac80bc35a52ccda02fcb3fccfe7a6459`.
Final seeds match the refreshed archive build inputs and A/B/C compiler source.
All 270 final source/test/example/editor identities match the frozen manifest.
Artifact hashes and exact counts are in build/verify-owned-artifact-identity.json.
Cargo used at most two jobs and tests one thread; broad suites were serialized.
Caches were preserved.

The old hand-linked backend and typed whole-file I/O fixtures needed file.c after
importing the expanded I/O module. The deadline fixture's old socket-layout mirror
read shared-pointer bytes as a descriptor; it now uses the production accessor and
requires POLLIN readiness. Independent review verifies these fixture repairs.
Production deadline behavior was unchanged. The initial C-only allocation ledger
missed generated enum payloads; corrected guest hooks detect deliberate leaks,
and ASan-marked LLVM functions detect deliberate out-of-bounds accesses. ASan
covers marked guest LLVM and C; UBSan covers compiled C. Internal libc allocations
and general guest UBSan are outside this instrumentation claim.

## Remaining work

Readonly/mutable borrows, channels and broader Result obligations remain isolated.
Channel source authority and corrected native target/DAG/family/schema traversal
have independent replay evidence, but are not wired into shared source. The raw
five-argument synchronous C manifest boundary checks consistency; it is not
cryptographic issuer authentication. Native explicit-borrow parity is now assigned
against that reviewed channel baseline. S036/BH220, S037/BH221, S038/BH222,
BH223/BH224 and the future S039/BH225 remain distinct.

Typed network/process/TLS wrappers and HTTP client/routes have a contextual rebase
against current files. Workspace/API documentation and server code actions remain
reviewed drafts; the custom editor client still provides no edits. Toolchain
inspection is tested with injected probes, with public parser corrections under
review; configured tool/SDK/linker evidence remains a separate gate. Draft
readiness does not complete a feature box.

This slice does not transfer aggregates/resources, add general lifetimes or user
cleanup, provide async I/O, or make allocation failure recoverable. Fatal traps
remain uncatchable and do not unwind. The Rust host remains accepted architecture.
No Rust-equivalent safety or complete-language claim is made. Actual Metal covers
the previously tested API; CUDA/Vulkan hardware, Linux/Windows execution and real
VS Code GUI remain unverified. Node Wasm probes are test tools; production still
needs a reviewed no-GC engine and compiler/linker. No SDK or registry/release was
installed or published.
