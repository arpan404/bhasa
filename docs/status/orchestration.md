# Current compiler integration

## Full backlog implementation, October 11, 2026

The user request authorizes implementation of the full `todo.md` backlog,
parallel internal agents and local progress commits. Earlier no-commit notes are
historical and superseded. SDK installation/cache cleaning remain outside this
request. A later explicit request authorized the completed GitHub push below.
Initial baseline commit: `62afd47`. The old heartbeat remains paused.

Completed initial increment: typed structured tasks and checked agent inspection.
Contract: typed-tasks-milestone.md. packages owns Rust checking/emission;
frontend_parity owns Bhasa parity; rust_cache owns C task runtime and independent
review. Root owns integration, inspection, guides, verification and commits.
Worker sources were frozen for acceptance. Focused checks passed runtime stress/guards,
both typed frontends, atomic ownership counters, O0/O2 sanitizers, imported
generics and TCP/TLS error-handle transfer. Independent review repaired malformed
extern rejection fixtures and an inspection privacy false positive.

Complete reference/full-hybrid validation passes 411/422 tests; editor tests pass
38. A/B/C passes at run.alQpR5Im, including typed workers. Both Clippy
configurations, formatting and refreshed release execution pass. Release smoke
has 22 native hits, four unchanged Rust adapter runs with zero compilations and
two unchanged hybrid builds with zero compilations. The default sandbox
prevented a loopback HTTP startup; both complete suites pass with loopback
permission. Sources are accepted for the typed-task integration commit.

Typed tasks are committed as `cdd7a5b`. The user subsequently authorized setting
origin to https://github.com/arpan404/bhasa.git and pushing completed commits.
The three baseline/inspection/typed-task commits are pushed to main through
`cdd7a5b`; unfinished source changes were preserved.

Implementation started for the next increment: packages owns Rust
bitwise/radix integers and stepped ranges; frontend_parity owns matching native
scanner/checker/emitter work; rust_cache owns checked references/local definitions
and rename. Ratified contracts use Python operator precedence, checked shift
counts, signed-i64 literal bounds, zero-step traps, overflow-ending cursors and
versioned rename edits only when every affected document is open. These lanes
are active after the typed-task integration commit. Contract:
systems-tooling-milestone.md. Root owns host traversals/formatter/bridge/grammar,
independent acceptance, guides and final validation.

The systems/tooling increment is integrated and verified. Complete reference
passes 433 tests; complete full hybrid passes 445. Editor gates pass 56 LSP and
80 Node tests. Both Clippy configurations and formatting pass. Refreshed release
compilers execute new/previous examples and five package tests. A/B/C passes at
run.xQ1qwOVL with new numeric/range examples and typed workers. Release smoke
records 26 native hits; four unchanged Rust adapter runs and two unchanged
hybrid builds perform zero compilations. Fresh archive key:
e1056ba67083f8c895adfeacf64eff9ff00162057017d1ae7f9ed44432dfcc81.
Archive source exactly matches the frozen scanner/emitter.

Independent review repaired importer completeness, stale unedited importers,
response amplification, repeated offset scans, contextual-name eligibility,
synchronization admission limits and retained closed-document URI history.
A first hybrid run exposed a test assumption about diagnostic tokens; original
mapping was correct. Exact per-frontend token/code/location checks and the full
hybrid rerun pass. No implementation or acceptance criteria remain open within
this increment. integration.md records evidence and platform/language limits.
The broader backlog remains open. Collections snapshot iteration is the next
prepared contract; no implementation assignment has started. All worker lanes
are completed; no extra app chats or monitoring loops were created.

## Completed iteration and editor milestone

The user requested continued Bhasa implementation. Root is implementing Rust
iteration and composing host lowering/cleanup; frontend_parity owns Bhasa
iteration; packages owns completion/hover/editor grammar; rust_cache owns
independent ownership/task acceptance and review. Contract: next-milestone.md.
Prior namespaces/types/tests/cache evidence is preserved in integration-foundation.md.
All implementation lanes are frozen. Focused checks pass 142 unit, five
independent iteration, one native seed/self-rebuild, 47 LSP, two formatter and
38 editor tests. Review fixes cover synthetic local names, parenthesized ranges,
loop-name reservation through lowering, capture-pipe deadlock and four editor
repros. Cargo checks are coordinated serially, with two build jobs and one test
thread. Complete reference acceptance passes 398 tests. Full hybrid acceptance passes 408 tests; both Clippy configurations and formatting
pass. Refreshed releases execute new/previous examples and five package tests.
A/B/C passes at run.0i9MfkYr. Release smoke records 20 native hits and four
zero-compilation Rust adapter runs. Two unchanged hybrid builds reuse archive
b2ae9f5fb380dcf392025873346826116006fdc7e0a2ceddcabbd314e0f812d1
with zero compilations. All scoped criteria are complete; integration.md records
review repairs, the stale-release delivery repair and real platform limits. The old heartbeat
remains PAUSED; no app chats or monitoring loops were created.

## Completed namespaces, types, tests and native cache milestone

The approved next-milestone.md contract is integrated. frontend_parity delivered
float/collections in both frontends; packages delivered module aliases/privacy
and LSP; rust_cache delivered native artifact caching/collection C storage and
runner review fixes. Root composed lowering/maps, runtime linkage, native test
runner, guides and acceptance. All sources are frozen; no outstanding criteria
remain within this bounded contract.

Reference acceptance passes 384, full hybrid 393, editor tests 33, formatting and
both Clippy configurations. Eight runner groups verify crash/deadline/pipe/error
handling; ten module groups and 39 LSP groups preserve identity/privacy. Both
release compilers execute examples/tests, A/B/C passes at run.75IuC7IS, and the
release smoke has 14 native hits and four zero-compilation Rust adapter runs.
Two unchanged hybrid builds reuse archive 2d994114af47078895ebe44fe861ccc4912b40d3c582716019a91f2955ffb938
without compilations. Archive source matches the frozen lexer/emitter.

Review repaired shared template intrinsic registration, runner cleanup/partial
reports/generic discovery, and diagnostic assertions that assumed Rust wording.
Sources preserve .bh, lowercase bhasa.toml, deterministic ownership and all
caches. integration.md records logs and real platform/language limits. Old
heartbeat remains PAUSED; no new app chats or monitoring loops were created.

## Completed generics, TLS and async executor milestone

The latest user instruction authorizes implementation of all three. Internal
agents frontend_parity, packages and rust_cache own generic lowering, native
TLS and a bounded executor respectively. Root owns language/host integration.
The contract is [generics-milestone.md](generics-milestone.md). All three agents are
completed and sources are frozen. Independent review fixes cover multiline
generic substitution, Unicode/newline scanner spans, generic method navigation,
TLS link order and positive deadline completion. Complete reference/full hybrid
acceptance passed 350/358 tests. Both Clippy configurations, formatting, 32 editor
tests, refreshed release compilers, A/B/C and live release HTTPS passed. Four
unchanged Rust adapter runs and two hybrid builds performed zero compilations.
The sole post-suite Clippy repair passed 14/15 focused generic checks; rebuilt
native frontend object bytes match the full-suite object.

Final archive key: 7e4882e9e616cb37cd4b421955fc2636fd648b5f0a93fbd55a91d8e4ece56b2c.
[integration-generics.md](integration-generics.md) records evidence and remaining platform/language
limits; [generics-state.json](generics-state.json) records completion. Current guides are
consistent with these contracts. There are no outstanding acceptance criteria.
Old app-chat heartbeat stays PAUSED; no new chats or monitoring loop were created.

## Completed backend milestone, October 11, 2026

The [approved backend contract](backend-milestone.md) is integrated and verified.
Three internal agents completed Rust enums, matching Bhasa frontend support,
and UTF-8/collections/deadline runtimes; root integrated Result/JSON/HTTP,
compiler/runtime linkage, editor features and a working HTTP service.
All sources are frozen and scoped review findings are resolved.

Complete reference acceptance passed 328 tests; full hybrid passed 335.
Formatting, both Clippy configurations, 32 editor tests, refreshed release
compilers, A/B/C and live release HTTP examples passed. Unchanged native and
Rust adapter artifacts were reused without compilation. Final native archive
key: afcb397123732b9a7557395d4405bf3f26e053b0301940058aa56c74104d7c6f.
Its source matches the frozen lexer/emitter. [integration-backend.md](integration-backend.md)
records logs, repaired findings, exact evidence and remaining language/platform
limits. There are no outstanding criteria within this bounded milestone.

The old methods heartbeat remains PAUSED. Six historical app chats remain
completed; monitor-state.json retains their historical cursors and evidence.
The current turn used internal agents, with no new app chats or monitoring loop.
[backend-state.json](backend-state.json) records this milestone's final state.

## Completed milestone, October 11, 2026

The user requested continued orchestration and monitoring. All six existing
chats completed their scoped assignments. The completed
contract is [methods-milestone.md](methods-milestone.md): record methods in both
frontends, explicit IPv4/IPv6 bind addresses, corresponding editor navigation,
independent acceptance tests and safety review. Root integrated the work and
verified both complete compiler suites after source freeze. There are no
outstanding acceptance criteria or open review findings for this milestone.

Heartbeat automation `orchestrate-bhasa-compiler-chats` is disabled (`PAUSED`),
confirmed through the app and saved configuration. [monitor-state.json](monitor-state.json) retains the last observed
per-chat cursors and verification evidence. Do not restart these completed
assignments or create empty monitoring loops. Future work needs a new bounded
scope; the experimental language still has the limits in [integration.md](integration.md).

Root integrated both `tcp_listen_on` overloads in src/builtins.rs after the
native ABI handoff. The compiled network fixtures pass with this registration.
The existing formatter passes a method/chained-call LLVM-equivalence and
idempotence fixture. Rust method sources, networking and editor tooling are
frozen; independent review found no remaining receiver/task safety issue.

The network review reproduced macOS accepting and discarding an IPv6 zone. The
owner now rejects scoped hosts before socket creation, and independent O0/O2
sanitizer checks confirm the repair. Actual IPv4 and IPv6 loopback tests pass.

Both frontends passed all seven independent method acceptance groups. All lanes
are source-frozen. The Bhasa lane corrected preliminary brace-depth counting;
globals and methods now accept 128 empty nested blocks and reject 129, matching
the Rust CLI. Root's final A/B/C gate passed at `build/selfhost/run.ORZKBDfu`.
Root verified the built native archive's concatenated source exactly matches
the frozen lexer/emitter. All 31 editor client tests and syntax checks passed.

The combined reference suite passed 300 tests and full hybrid passed 307;
logs are `build/verify-methods-reference.log` and
`build/verify-methods-hybrid-frontend.log`. They ran serially with two build jobs
and one test thread. Formatting, both Clippy configurations with warnings denied,
both refreshed release binaries and method example smoke checks passed.
The final unchanged hybrid build reused its native archive; Cargo finished in
0.03s and 0.05s. Two hybrid Rust-adapter runs printed 42 with no recompilation.
Release/cache logs and platform limits are in [integration.md](integration.md).

First routed handoff: the records lane published `RecordDecl.methods`, raw
`ExprKind::MethodCall`, inferred receiver parameter and `Record.method` internal
keys in status/records.md. Checked calls lower to ordinary calls with the
receiver first; checked method declarations remain on the record. Root forwarded
this to tooling, Bhasa frontend and safety review. Worker graph validation runs
after method resolution, including recursive qualified method edges. Combined
acceptance and independent safety probes verify these contracts.

The user asked for continued implementation, parallel dedicated Codex chats,
persistent Rust dependency builds, and an exactly lowercase `bhasa.toml`.
The Rust host remains part of the architecture. No commit, reset, worktree,
publication, or registry upload has been requested.

## Dedicated chats

| Chat | ID | Ownership |
| --- | --- | --- |
| Bhasa records and data types | 01a12c23-f9e1-7962-9cec-2fb33614489e | AST, Rust lexer/parser/checker/LLVM generation, record tests and module type merging |
| Bhasa backend networking runtime | 01a12c24-1180-7fb1-b9e2-d46ee47952d0 | net.c/h, native network tests, examples and docs |
| Bhasa editor and agent tooling | 01a12c24-3bab-7430-ad85-8ac6bac3cc2a | LSP/JSON modules, protocol tests and VSCode files |
| Bhasa compiler acceptance and fuzzing | 01a12c24-508e-74d3-a4ee-13958338f887 | Independent differential tests and verification script |
| Bhasa frontend records and networking | 01a12c28-ce31-73b0-8722-f446c803d4e2 | Bhasa emitter/seed driver/gate and self-host tests |
| Bhasa native runtime safety | 01a12c2c-918e-78c2-93b4-3a67719ccd36 | Core runtime and descriptor inheritance regression |

Each lane writes `docs/status/` evidence. The integration chat owns the Rust
driver, module exports, build features/scripts, scanner FFI, formatter, builtin
network declarations, and combined verification. All lanes are now frozen;
their status files distinguish current evidence from historical baselines.

## Completed work from the preceding milestone

- Local packages, imports, scaffolding and source mapping passed ten package
  tests. Names are exactly `bhasa.toml`, including on case-insensitive hosts.
- Persistent Cargo adapter builds passed real rustc-invocation regressions.
  Unchanged Rust and Bhasa-only edits caused no additional Rust compilations.
- Bhasa frontend extern/task/GPU support passed sanitizer tests and an expanded
  A/B/C rebuild before new records/network work started.
- Formatter implementation retains comments and significant statement
  boundaries; three unit and two CLI checks passed.
- Root added `--frontend bhasa|rust`, `compile_with_bhasa`, a full frontend build
  feature, and content-addressed native archive reuse. These passed final hybrid
  integration verification against the new record frontend.
- Root added TCP builtin signatures and driver net.c/h linkage. The record lane
  added owned handle types and lifetime code generation.
- Root added `bhasa lsp` routing and JSON/LSP exports. Protocol verification
  passed in both combined suites.

## Preceding milestone integration

Completed. The reference suite passed 265 tests and the full hybrid suite passed
273. The final module/newline change passed 16 focused tests. Both release
binaries, formatting, default/full hybrid Clippy, A/B/C self-rebuild, real Metal
and TCP checks, Rust adapter reuse and unchanged native archive reuse passed.
See [integration.md](integration.md) for logs, artifacts and exact limitations.

The completed Bhasa frontend includes records, externs, tasks, GPU calls and TCP.
A host-only file-boundary token prevents cross-file syntax stitching. The cache
hashes preprocessed C input to include SDK and CPATH header contents. Runtime
process/SIGPIPE fixes passed focused and combined sanitizers. LSP/JSON/formatter
integration passed the full suites. CUDA/Vulkan hardware and local Wasm execution
remain unverified; these finite checks do not establish production readiness.
