# Methods and backend usability

The user asked the integration chat to orchestrate and watch the existing six
chats on October 11, 2026. Their preceding assignments are complete. This file
defines the next bounded milestone, not a claim that it already works.

## Shared contracts

Methods are declared inside records:

```bhasa
struct Point {
    x: int
    y: int
    def total(self) -> int { return self.x + self.y }
    def shift(self, amount: int) { self.x = self.x + amount }
}
def main() {
    var point = Point(20, 22)
    print(point.total())
    point.shift(1)
}
```

The first parameter is exactly `self`, with its enclosing record type inferred.
Remaining parameters follow existing annotation rules. Receiver parameters use
the existing mutable-borrow semantics and cannot be rebound. Methods have
ordinary function return rules and no access to fields without `self.`. There
are no static methods, overloads, inheritance or dynamic dispatch in this slice.
Method and field names in one record must not collide. Different records may
reuse method names. Internal method names must not collide with source globals,
constructors, runtime or extern symbols. Module type remapping, temporary
receivers, borrowed returns and overlapping argument checks must stay correct.
Reject unsupported forms clearly rather than silently changing ownership.
Evaluate a receiver once, then explicit arguments left to right. A temporary
receiver must survive the call and any owned return acquisition. Bare method
references are not function values. Resolve method calls before building the
worker call graph, or rebuild that graph afterward, so methods cannot hide
transitive extern calls. Direct `spawn(object.method(...))` remains unsupported
by the existing single-integer worker ABI.

The network addition is `tcp_listen_on(host, port) -> tcp_listener`, where host
is `str` or `buffer`. ABI symbols are `bhasa_rt_tcp_listen_on_str` and
`bhasa_rt_tcp_listen_on`. Accept explicit numeric IPv4/IPv6 addresses, including
wildcards; reject names, empty/binary/NUL hosts and invalid ports. Port zero is
allowed. Keep `tcp_listen(port)` as the existing IPv4-loopback convenience call.
Use the established allocated error-handle/status and descriptor ownership rules.
Tests bind only loopback addresses and ephemeral ports. No external listener is
started as part of development. IPv6 absence must be reported explicitly.

## Ownership and sequence

| Existing chat | Assignment | Owned files |
| --- | --- | --- |
| Bhasa records and data types | Rust method parsing, checking and LLVM lowering; publish AST contract early | src/ast.rs, parser.rs, sema.rs, codegen.rs; method tests/examples; docs/records.md and status/records.md; minimal project.rs type remapping only if required |
| Bhasa frontend records and networking | Matching Bhasa method frontend and listen-on builtin; preserve token 314 and ABI | bootstrap/compiler/emitter.bh, driver.bh, check.sh; selfhost method tests; status/selfhost-v2.md |
| Bhasa backend networking runtime | Explicit bind addresses and focused native validation | runtime/net.c/h; network tests/fixtures/examples; docs/network.md and status/network.md |
| Bhasa editor and agent tooling | Record/field/method symbols and method-aware definitions after AST handoff | src/lsp.rs; LSP tests; editors/vscode; docs/tooling.md and status/tooling.md |
| Bhasa compiler acceptance and fuzzing | Independent positive/negative method and import cases; diagnose parity failures | tests/frontend_differential.rs; dedicated method acceptance tests; tools/verify-compiler.sh; status/acceptance.md |
| Bhasa native runtime safety | Independent review of receiver lifetime and bind/error cleanup; reproduce concrete bugs | status/runtime-safety.md; own core.c/h and runtime_core tests only for proven core defects; otherwise report to owner |

The integration chat owns src/builtins.rs, src/driver.rs, src/lib.rs, scanner FFI,
formatter, build integration, this contract and final combined verification.
Agents must publish their AST/API handoff before dependent edits. They may read
all shared files but must not repair another lane's unfinished changes. Report
blocking interface conflicts in their status file. The integration chat routes
follow-ups; user authorization covers messaging these existing chats.

Keep `.bh`, exactly lowercase `bhasa.toml`, no-GC owned values and persistent Rust
caches. Do not clean Cargo caches, install SDKs, commit, reset or publish. Use
focused tests with CARGO_BUILD_JOBS=2 and --test-threads=1. Do not launch broad
suites or extra task chats. Root serializes combined suites once sources freeze.

## Acceptance and monitoring

Both frontends must run the same method examples and reject invalid receivers,
arity/types, collisions, missing returns and unsafe alias combinations. Exercise
cross-package record IDs, nested/temporary receivers, returned owned fields,
mutation during later argument evaluation and cleanup at O0/O2 with sanitizers.
Networking must retain existing behavior and pass IPv4/available-IPv6 loopback,
invalid-input, error-status and cleanup tests. Editor features must retain UTF-16
positions, unsaved overlays and saved import-graph behavior.

The monitor checks compact chat snapshots every five minutes, routes blockers
and actionable findings, and avoids restarting active work or rerunning passing
tests without changes. It stays quiet on unchanged progress. Completion requires
root review, feature integration, appropriate combined verification, refreshed
release artifacts and an accurate integration report. The monitor must not
describe unverified CUDA/Vulkan hardware, Wasm execution or production safety as
complete. It must not assign unrelated work merely to keep chats occupied.
