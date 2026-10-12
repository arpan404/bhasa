# Generics, TLS and async executor milestone

Approved by the user on October 11, 2026 after the completed backend milestone.
Preserve `.bh`, exactly lowercase `bhasa.toml`, deterministic no-GC ownership,
Rust host/Bhasa frontend, cached Rust/native artifacts and prior acceptance.
No installs, cache cleaning, commits, resets or publishing.

## Contracts

Explicit compile-time specialization: `struct Box[T] { value: T }`,
`enum Option[T] { none; some(value: T) }`,
`def identity[T](value: T) -> T { return value }`, `identity[int](42)`.
Support nested concrete applications, enclosing generic record methods and
cross-module templates. A shared host lowering pass specializes before either
frontend; checked ownership, schema limits and worker call graphs still apply.
Bound specialization count/depth/output; preserve diagnostic source mapping.
No implicit type argument inference, traits, erased runtime types or GC.

Native TLS uses installed OpenSSL 3 with verified client trust and hostname,
numeric connect address, explicit CA/certificate/private-key files, TLS 1.2 or
newer and total operation deadlines. `tls_socket` is owned; copies share one
serialized refcounted session, dropped deterministically. Failure constructors
still return owned status-bearing handles. Never expose a verify-off API.
Server authentication is required; mutual TLS is outside this API.

`async_spawn(worker(42))` queues a scalar int->int worker on a fixed bounded
executor. `await(task)` consumes its scoped linear handle. Existing spawn/join
remains available. No arbitrary capture, function values or GC. Worker-reachable
pooled scheduling/waiting and extern calls are rejected transitively, including
methods, matches and generic specializations. Queue saturation applies
backpressure; no thread per pooled task. This executor does not add async I/O
state machines or async function syntax.

## Ownership and acceptance

| Lane | Ownership |
| --- | --- |
| frontend_parity | Shared generics lowering/source map, focused tests/examples |
| packages | TLS C runtime, TCP borrowed-fd seam, native TLS tests |
| rust_cache | Fixed bounded executor runtime and native scheduling tests |
| root | Both frontend API/ownership/linkage, std/examples, combined acceptance/docs |

Focused checks while sources evolve, maximum two Cargo build jobs and one test
thread. Root serializes broad reference/hybrid verification after source freeze.
Require both frontends executing generics/owned TLS/scoped executor examples,
invalid generic and worker/task diagnostics, valid TLS and untrusted/wrong-host
rejection, deadlines, sanitizer cleanup, bounded executor concurrency/queue
backpressure, refreshed releases, A/B/C self-rebuild, unchanged native and Rust
adapter cache reuse. Preserve prior checks and report actual platform skips.
Old app-chat heartbeat remains paused; internal agents own this work.

Completed and verified October 11, 2026. Reference/full hybrid acceptance passed
350/358 tests, release examples and A/B/C passed, and unchanged Rust/native builds
reused their caches. See [integration.md](integration-generics.md) and
[next-state.json](generics-state.json) for final evidence and implementation limits.
