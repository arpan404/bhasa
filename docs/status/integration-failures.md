# Compiler integration status

Local pending-result obligations are integrated and verified on ARM64 macOS.
Immutable-binding evidence is archived in [integration-immutable.md](integration-immutable.md),
and typed-error evidence in [integration-errors.md](integration-errors.md).
The user authorized remaining backlog implementation, dedicated chats, progress
commits and pushes. This report covers one bounded increment, not the full language.

## Delivered

Both frontends reject result-shaped local values left pending at reachable scope,
iteration or function exits, return, break/continue and overwrite. Direct exhaustive
match, direct return and whole-initializer try discharge their named operand.
Copies create separate obligations. Every continuing branch must handle its values;
loops preserve the outer pre-loop state because they may run zero times. Result
assignment to an outer binding inside a loop rejects conservatively. Try's implicit
error return checks all other live results.

The shape remains exactly two one-payload ok/error variants, with resolved nominal
payload types. Parameters, aggregate contents, directly nested arguments,
interprocedural handling summaries and assigned scalar statuses remain outside
this slice. Passing a bound opaque private result to a public handler does not
discharge it. [The error guide](../errors.md) describes this compatibility limit.
Fatal traps remain uncatchable and do not unwind. No Rust-equivalent safety claim
is made.

Rust reports S037 and rechecks the obligation pass at public LLVM emission, including
mutated checked programs. Native BH221 uses local field 8 separately from ownership,
task metadata and immutable field 7. Scanner/type/runtime ABIs are unchanged.
Existing ordinary checker, private-member and task/foreign call-graph errors retain
their rules and precedence. Imported generic diagnostics preserve original files,
Unicode/CRLF byte positions and offset-free messages.

Owned fixtures now handle original results while retaining copy/mutation assertions.
Two negative worker fixtures handle their joined result to isolate the original
BH209 rule. The contextual variable named try fixture handles its own result before
assertion returns. No check was disabled to migrate fixtures. Independent review
found no further defect in the integrated local contract.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 477 passed; build/verify-failures-reference-final.log |
| Complete hybrid | 502 passed; build/verify-failures-hybrid-final.log |
| Focused local obligations | Eight reference groups, ten native groups, opaque-result boundary and original-file diagnostics; tests/failure_use_reference.rs, failure_use_bhasa.rs and failure_use_opaque.rs |
| Owned cleanup and AST enforcement | O0/O2 nonrecovering ASan/UBSan and atomic allocation counters in the error/immutable suites; corrupted checked-AST regression |
| Independent review | 16 additional cases agree across both frontends, plus two original-file generic JSON probes; build/backlog/safety-errors/final-integration-review.md |
| Compatibility | Existing previously valid standalone examples/fixtures show no new failures; build/verify-failures-existing-sources.json |
| Editor | 59 LSP tests in each complete suite, 82 Node tests; build/verify-failures-editor-final.log |
| Formatting/Clippy | Formatting and both configurations pass with warnings denied; build/verify-failures-fmt.log and clippy-reference.log / -hybrid.log |
| Refreshed releases | Both compilers execute thirteen examples and five package tests; build/verify-failures-release-smoke.log |
| Cache reuse | 32 example/test native hits and four unchanged Rust adapter runs with zero Rust compilations; two unchanged hybrid builds reuse the same archive with zero compilations |
| A -> B -> C | Complete compiler LLVM and example outputs agree at build/selfhost/run.EbbPYcIp; build/verify-failures-selfhost.log |

The complete suites ran serially with two Cargo jobs and one test thread, with
local loopback networking authorized. Matching archive key is
cac13c4c9ed6aa67e5812514d19391babaca15f477783246d0c69d8bca43778e.
Its scanner/emitter bytes exactly match build/hybrid/run.DeJuY1kd/frontend.bh.
Release/source hashes are in build/verify-failures-artifact-identity.json.
Both unchanged build logs verify the same archive and zero Compiling lines.
Caches remain intact. All acceptance criteria for this local slice pass.

## Active backlog

[todo.md](../../todo.md) keeps parameter/aggregate/interprocedural failure safety
unchecked. The eight existing backlog chats continue under [backlog-lanes.md](backlog-lanes.md),
with cursors in [backlog-state.json](backlog-state.json). All other changes are isolated
until integrated and tested. Alias allocation/canonical-identity repairs pass
independent review; editor alias metadata consumption is in progress. Package
lock/FFI and signature/semantic-token drafts have corrected review findings and
await integration. Private Text/collection/JSON storage and streaming-file drafts
have independent runtime/privacy evidence but still need combined integration.

Channel review found exponential shared-type traversal, prefix-only intrinsic
authority and unauthenticated getter bodies. These are routed for correction
before exposing source-level channels. The next ownership slice is ratified in
[owned-bindings-milestone.md](owned-bindings-milestone.md), limited to synchronous
buffer/ints transfer with explicit modes and conservative control-flow rules.
It is not implemented or verified yet.

Wasm policies/native objects and test-only Node fixtures do not establish a
runnable compiled guest with no-GC production dependencies. A reviewed engine
and guest compiler/linker remain real gates. Actual prior Metal/TCP/TLS checks
pass; CUDA/Vulkan hardware, Linux/Windows execution and VS Code GUI remain
unverified or gated. The remaining language, runtime, ecosystem, compiler
performance and production acceptance backlog stays open.
