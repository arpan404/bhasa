# Typed bounded channel contract

The current increment adds compiler-issued `Channel[T]`, `Sender[T]` and
`Receiver[T]` families through the canonical `@std` module. It preserves the
existing typed task, owned buffer/ints, regular-file, pending Result, alias,
privacy and immutable-binding contracts.

Construction is `channel[T](capacity)` with positive capacity. Pair getters
produce owned endpoint copies. Sender operations are `send`, `try_send` and
`close`; receiver operations are `recv`, `try_recv` and `close`. Send acquires an
independent copy before enqueueing, returns that copy on full/closed failure and
leaves the caller's original intact. Receive success transfers the queued frame.
Explicit close or last sender drop prevents sends but allows draining. Last
receiver drop closes and destroys unread frames. Final-owner drop destroys the
queue. The runtime uses no garbage collector.

Payload checking follows concrete nominal records/enums and alias targets. Tasks,
void, regular-file handles and nested endpoint resources are refused transitively.
Existing approved TCP/TLS copies retain their own resource contracts. Worker
transfer is a separate context: issued endpoints are eligible when their payload
passes the strict payload check. Workers may use nonblocking operations and close;
blocking operations are forbidden through resolved free-function/method call
graphs and guarded again at LLVM emission and runtime entry.

Channel authority requires exact compiled-in standard-library bytes at the
canonical registered source path, original module evidence, specialization
receipts and unchanged declaration/signature schemas. Identical source at another
path, modified registered overlays, forged storage/getters and public-emitter
metadata corruption do not acquire authority. Installed-library relocation needs
an explicit future bundle registration contract. Editor and inspection metadata
must enumerate authority-derived operations from a successfully checked original
graph; recovery must never create intrinsic declarations or rename edits.

The Bhasa emitter uses a separate five-argument synchronous manifest entry when
channels are present, with family-bound nominal/schema identities and bounded
memoized DAG validation. Host-owned manifest storage is borrowed during that
call; the native frontend must not retain, mutate or free it. This C adapter is a
trusted consistency boundary, not cryptographic authentication for arbitrary raw
callers. The four-argument non-channel entry remains available.

Both runtime extraction and native manifest generation require the actual shared
`require_native_posix64` target admission. Unsupported targets fail before native
artifacts or Rust adapter builds. C source/header identity participates in the
persistent native cache. `BH223` structural channel errors and `BH224` blocking
worker errors remain distinct from immutable `S036/BH220`, pending Result
`S037/BH221`, owned transfer `S038/BH222` and future borrow `S039/BH225` errors.

Acceptance requires actual composition in both frontends, canonical loader and
overlay/privacy tests, O0/O2 task/channel/owned payload execution, bounded close
and full/empty behavior, sanitizer and exact allocation cleanup, independent
source/boundary review, original diagnostic preservation, full frozen-source
suites, editor protocol checks, matching archives/releases, A/B/C self-rebuild
and unchanged Cargo/native/archive reuse. A draft or finished chat is insufficient.

`select`, deadlines, cancellation, async suspension, arbitrary shared references,
Windows runtime support and unavailable hardware/SDK execution are outside this
increment. Actual Linux/Windows execution remains unverified.
