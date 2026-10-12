# Full backlog lanes

The user requested dedicated Codex chats for all remaining todo.md work on
October 11, 2026. This supersedes the old bounded monitor's no-extra-chat note.
Historical six chats remain completed. The existing heartbeat now coordinates
these eight lanes every five minutes; it stays quiet on unchanged progress and
stops when no meaningful work remains without a real external change.

The integration chat at 01a12ba1-6cad-7f00-a505-1eb6a34b6e41 owns contracts,
source integration, Cargo gates, releases, todo.md and authorized progress
commits/pushes. Typed errors are committed and pushed as 86a99ed. Binding-only
let is verified and pushed as 4828aff. S037/BH221 local result obligations are
verified and pushed as 217e33d; do not assign either again. Package/editor/private
storage changes pass all combined gates (558 reference inventory, 583 hybrid,
77 LSP groups per configuration and 93 Node client tests), matching releases,
A/B/C and unchanged cache reuse. integration.md records evidence. Next reviewed
increments are aliases, TCP copy policy, owned transfer, files and channel parity;
their isolated readiness does not mark checklist boxes complete.
Transparent aliases and checked alias metadata now pass 597 reference/622 hybrid,
93 LSP per configuration/93 Node, both Clippy modes, releases, A/B/C at
run.DqAvmtmF and unchanged cache reuse. integration.md records final evidence;
next actual source composition is corrected owned transfer/TCP/files. Channel
target correction is independently approved for native manifest implementation.
Explicit borrow host review and late-stage Result-parameter implementation are
active. Workspace/public-host and S036 action corrections remain separate gates.

## Shared workflow

Read todo.md, docs/status/orchestration.md, docs/status/integration.md and the
current error/immutable-binding contracts. Preserve .bh, lowercase bhasa.toml,
no-GC deterministic ownership, Rust host, both frontends and persistent caches.

Each new chat owns the backlog sections below and implements them in dependency
order, beginning with its first concrete increment. Prepare code/tests under
build/backlog/<lane>/draft with original relative paths; tracked compiler/runtime
sources are frozen for integration. A draft must record its source baseline and
small patches so unrelated integrated updates can be preserved. Write progress
only in docs/status/backlog-<lane>.md. Share APIs/contracts and precise blockers
with the coordinator; do not modify another lane's files or mark todo boxes.

Use existing compilers for small probes and author meaningful tests. Cargo,
broad suites, SDK installation, cache cleaning and Git mutations belong to the
coordinator. No reset/clean/commit/push/registry publication by workers. Avoid
heavy concurrent native builds; request a verification slot with exact commands.
Never claim draft readiness proves end-to-end acceptance or unavailable hardware.

The human authorizes these chats to coordinate implementation with the integration
chat. Report actionable handoffs there, including draft paths, API changes,
tests, evidence and remaining acceptance criteria. Continue independently on
owned prerequisites while waiting for API or build coordination.

## Ownership

| Lane | Remaining scope | First increment |
| --- | --- | --- |
| safety-errors | Pending failure analysis, consistent typed runtime errors, library storage invariants, foreign error/unwind/thread contracts and independent safety review | Specify and implement a sound next slice of failure-use checking beyond immediate expression discard, preserving existing Result and try APIs |
| ownership-types | Ownership/borrows/recursive resources; systems types; functions/generics/matching/aliases/constants/compile-time evaluation | Ownership-transfer/read-only borrow contracts and transparent non-generic type aliases; binding-only let is reserved for root |
| native-compiler | Bhasa frontend parity for core/safety changes, host migration, incremental compiler work and reproducible caches | Mirror new approved core/safety contracts, prepare per-module incremental analysis with measurable correctness/cache oracles |
| concurrency-async | Channels/backpressure/select, cancellation/task groups/locks/atomics, executor controls, coroutine lowering/reactor/timers/cancellation-safe I/O | Typed bounded channels with deterministic close/drop and checked thread transfer |
| std-backend | Collections/text algorithms, streaming files/paths/env/process, math/time/logging/config/serialization, HTTP/routing/WebSocket/database/socket/TLS ecosystem | Streaming fallible file handles and typed operations, then bounded HTTP client/routing |
| packages-ffi | Foreign ownership-aware adapters, no-GC dependency policy, resolution/lockfiles/checksums/workspaces/features/docs, stable export/library ABI and registry/release packaging | Local Bhasa lockfile/checksum correctness and ownership/error metadata for C/Rust adapters |
| editor-acceptance | Signature help/actions/highlighting/indexing/refactors/agent queries, assertions/property/coverage/bench/debug tooling, fuzz/soak/CI/spec/tutorial/release acceptance | Checked signature help with protocol/client tests, plus coordinated acceptance inventory |
| gpu-platforms | GPU kernels/buffers/pipelines/async/WebGPU/debug; Windows/Linux/mobile, runnable Wasm/WASI/browser/embedded/freestanding targets and SDK distribution | Runnable Wasm host/ABI contract and portable kernel/device-buffer contract, implement testable host pieces before hardware gates |

No backlog section is considered complete merely by delegation. Thread IDs and
compact wait cursors are recorded by the coordinator after creation.
