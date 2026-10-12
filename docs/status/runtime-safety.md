# Core runtime safety

## Methods/backend milestone review, complete

Independent review of method receiver lowering and explicit numeric-address
binding is complete for the source hashes recorded below. One concrete network
finding was repaired by its owner and independently verified. No findings remain
open. This milestone changed only this status file; core runtime sources remain
frozen at their baseline hashes below. Root owns combined verification.

The records handoff now specifies raw `MethodCall` nodes, receiver-first lowering
to qualified `Record.method` calls, and deferred worker-graph checking. Source
review and eighteen probes in `/private/tmp/bhasa-method-safety` verify these
behaviors in both frontends:

- Reject a receiver and its owned descendant in one call, for example
  `p.take(p.text)` where `take` replaces `self.text` before reading its argument.
- Reject an earlier nested receiver invalidated by a later argument, for example
  `p.leaf.read(p.replace())`, where `replace` assigns a new `p.leaf`.
- Treat method mutation inside later argument expressions just like free-function
  mutation, including `consume(p.leaf.text, p.replace())`, nested calls, arithmetic
  and short-circuit operands. Keep independent constructor copies permitted.
- Evaluate an effectful receiver once, before arguments. Keep temporary receivers
  alive through the call, clone returned borrowed fields before receiver cleanup,
  and preserve mutation through named/nested receiver storage.
- Traverse method call edges when checking a spawned scalar worker for reachable
  extern calls. Cover direct method-to-extern, free-function/method cycles, and
  same-named safe/unsafe methods on different records. Permit an extern call used
  only to compute a spawn argument in the calling thread.
- Keep method-local task handles subject to required joins, including early
  returns and nested scopes. A method receiver must not make a method eligible as
  the existing one-integer-parameter worker ABI.

The Rust task graph now includes qualified method bodies after expression
lowering. The Bhasa frontend now records resolved signature-node edges and checks
workers after every body is emitted. Review and probes confirm that recursive
method paths cannot hide an extern call, same-named methods remain distinct, and
extern spawn-argument evaluation in the calling thread remains permitted.

One read-only helper reviewed the bind implementation and focused native tests.
Checks include host validation before descriptor creation, IPv4/IPv6
address lengths and local-port extraction, preserved errno and exactly-once
cleanup on every setup failure, borrowed host buffers, and clone lifetime.
No broad tests ran for this milestone. This lane is frozen for integration.

### Resolved finding NET-SAFE-1: scoped IPv6 input reached bind

The network owner added numeric-byte validation before `inet_pton`. Independent
reverification passes for both overloads at `-O0` and `-O2` with ASan/UBSan.

On this macOS host, `inet_pton(AF_INET6, "::1%lo0", ...)` returns 1.
The original `listen_numeric` at `runtime/net.c:150-179` accepted this scoped host,
cleared its scope through the zero-initialized sockaddr, and proceeded to socket
and bind. The network contract/status explicitly requires scoped hosts to be
rejected. A bind-address spelling could therefore silently change meaning.
The independent ASan/UBSan intercepted-syscall harness reproduced this by failing
at the unexpected `socket(AF_INET6)` call for invalid `::1%lo0`. No real bind ran.
Reviewed source SHA-256:
`d54bfe5bf2e29c6d656c8235eaad38099cb0ab40fe42d8de03670122f9722f38`.
Exact reproduction, rerun by this lane's primary reviewer:

```text
UBSAN_OPTIONS=halt_on_error=1 /private/tmp/bhasa-bind-independent-review/scoped
str host=::1%lo0 actual_status=0 expected_EINVAL=22 socket_calls=1
buffer host=::1%lo0 actual_status=0 expected_EINVAL=22 total_socket_calls=2
```

The scratch sources are `scoped.c` and `check.c` in that directory. After repair,
the primary reviewer reran `UBSAN_OPTIONS=halt_on_error=1
/private/tmp/bhasa-bind-independent-review/scoped-fixed`:

```text
str host=::1%lo0 actual_status=22 expected_EINVAL=22 socket_calls=0
buffer host=::1%lo0 actual_status=22 expected_EINVAL=22 total_socket_calls=0
```

The independent helper's complete intercepted check now passes with all scoped
regressions enabled. Run `/private/tmp/bhasa-bind-independent-review/run.sh`.
It covers IPv4/IPv6, numeric/wildcard forms without real binds, port bounds,
borrowed-buffer immutability, binary/NUL inputs, every setup error and EINTR retry,
exactly-once close, errno preservation when close changes errno, and malformed
getsockname family/length. No further concrete bind defect remains. Actual
loopback execution stays with the network lane; Linux execution is unverified.
Reviewed `net.c` SHA-256 is
`ce4a6851b9f9a11a79cc2d332b4b8e879ca47ca78d711251d4dfb299bf5abcce`,
and `net.h` is
`63477a5d1e09a9ec96fec1c05c70be3d4d8e2a095a906539e72d072ced823b39`.

### Receiver/task review evidence

All 18 independent probes in `/private/tmp/bhasa-method-safety` pass against both
a standalone checker compiled from current Rust modules and a newly built Bhasa
frontend seed. The final extra probe rejects `tcp_listen_on(e.host, e.port())`
when the port method replaces `e.host`; both frontends reject the stale borrow.
Six positive cases also pass in each frontend at `-O0` and `-O2`, 24 native runs,
with generated LLVM functions
marked `sanitize_address`, ASan/UBSan C runtime code, and an atomic allocation
ledger asserting zero live record/container/resource/task allocations. They cover
temporary returned borrows, constructor snapshots before later mutation,
receiver-once/before-arguments order, nested receiver mutation, method-local task
joins, and temporary TCP error-handle returns. No concrete receiver lifetime,
evaluation-order, ownership, or task-graph defect was found.

Commands:

```text
python3 /private/tmp/bhasa-method-safety/run_probes.py rust /private/tmp/bhasa-method-safety/checker-snapshot --native
python3 /private/tmp/bhasa-method-safety/run_probes.py bhasa /private/tmp/bhasa-method-safety/seed --native
python3 /private/tmp/bhasa-method-safety/run_probes.py bhasa /private/tmp/bhasa-method-safety/seed-final
```

The tiny checker directly
compiles the relevant Rust modules, avoiding a broad Cargo build while other
lanes edit unrelated modules. Tests run sequentially with `CARGO_BUILD_JOBS=2`;
none invoke a broad Cargo suite. The original Bhasa sanitizer seed had emitter
hash `6b46a9f8d195578167f1a5760dd434e015de60c84b1040290ac6bab09f0bcc51`.
The only later emitter delta allowed the record wrapper outside the ordinary
method-body depth budget. That delta was reviewed, the latest seed was rebuilt,
and all 18 check/emission probes passed again. It does not change the tested
ownership or call-graph paths. Root owns full hybrid integration verification.

Reviewed source SHA-256 values:

```text
582e302e203dd51a48155abb475f520c0465f9624a0b257903069b26f5559cd2  src/ast.rs
ecc35dc6c65b70565ea8032aaf65e3e3c642713205dfef2e05f9361f3a1d0034  src/parser.rs
063f3dbb74bbae5c8c94f36b79cb178c4e3a1bac4446e6410c7a9772f4182e82  src/sema.rs
0c9702a97a34f11f3b98c481eb2b2dc69ff4e0b1f65fb35e35dbeec0a1c5287f  src/codegen.rs
5da2e4b033fe2ea49184885ef5ec3290b907fc18993c02a3c3ab9ce21f6ea09e  bootstrap/compiler/emitter.bh
107a057af2bb006910a2509048bbb3ac88a39dbd60d801240feb64bec75f0621  bootstrap/compiler/driver.bh
```

Module remapping was inspected in `Program::merge` and `remap_function`/
`remap_expression`; method parameter, return, body and receiver types are covered.
Imported-program execution and full hybrid archive validation remain with the
acceptance/integration lanes. No Linux execution or broader production-safety
claim is made by this review.

## Completed baseline

Complete and frozen for integration, 2026-10-11, macOS arm64.

This lane changed only `runtime/core.c`, `runtime/core.h`, `tests/runtime_core.c`,
`tests/runtime_core.rs`, and this status file. No reset, worktree or commit.

## Process descriptors and shell behavior

macOS `process_run` uses `POSIX_SPAWN_CLOEXEC_DEFAULT` with explicit inheritance
of open standard descriptors. Their close-on-exec flags remain effective and
the parent's flags are unchanged. Other descriptors are closed atomically at
spawn, covering the socket/accept/fopen inheritance window while workers run.
All initialized attribute and file-action objects are destroyed on setup and
spawn errors. Linux's process-spawn path is unchanged.

Verified the APIs against the installed Xcode macOS SDK's `spawn.h` and manuals
`posix_spawnattr_setflags.3` and `posix_spawn_file_actions_addinherit_np.3`.
The header requires `_DARWIN_C_SOURCE` when `_POSIX_C_SOURCE` is defined.

The actual child `fdcheck` regression failed before the fix because it could
inspect an intentionally non-CLOEXEC parent descriptor. It now passes without
thread timing or a probabilistic race. Tests also preserve binary child
stdin/stdout/stderr, environment, empty arguments, literal shell metacharacters,
exit status, and closed/close-on-exec standard streams. Fault injection covers
attribute init, attribute flags, action init, and each stdio inheritance action,
checking returned errno, destroyed objects and released argv allocation.

The audit also reproduced Apple's implicit shell fallback for executable text
without a shebang. The macOS helper now calls `posix_spawn` directly and searches
PATH itself. It returns `ENOEXEC` instead of invoking `/bin/sh`. Regressions cover
direct and PATH-based rejection, absent/empty PATH, empty path segments, missing
and non-directory entries, and permission-denied errors. The original fallback
is visible in [Apple's libc implementation](https://raw.githubusercontent.com/apple-oss-distributions/Libc/main/sys/posix_spawn.c).

## Broken pipes

The audit reproduced SIGPIPE exit 141 from a stdout write to a pipe without
readers. POSIX core writes now block SIGPIPE in the calling thread, consume only
a new pending signal associated with `EPIPE`, and restore the caller's mask.
They preserve an existing pending signal and the process signal disposition.
Borrowed stream operations hold the stream lock through failed-buffer cleanup.
macOS `fpurge` and Linux `__fpurge` discard unwritten bytes after `EPIPE` so exit
flushing cannot retry those bytes and deliver SIGPIPE again. Files remain closed
on write or flush failure.

Seven deterministic regressions cover stdout, stderr, large writes, FIFO close
flush failure, blocked SIGPIPE, an already pending signal, and ignored SIGPIPE.
The FIFO helper opens a reader first and closes it immediately after core's
`fopen`, before the first write. It needs no timing race or `/dev/fd` access.
Tests verify masks, signal disposition, pending signals, no buffered output
reappearing at exit, and zero tracked live allocations.

POSIX consumers now need pthread support for `pthread_sigmask`. Root added
`-pthread` driver integration for Linux core programs, including those without
task operations. macOS resolves this from libSystem.

## Verification

Passed on native macOS arm64 with Apple Clang 21.0.0:

```text
CARGO_BUILD_JOBS=2 cargo test --test runtime_core -- --test-threads=1 --nocapture
1 passed; 0 failed; finished in 13.35s
```

The suite builds with strict C11 warnings and AddressSanitizer plus
UndefinedBehaviorSanitizer at both `-O0` and `-O2`. Existing buffer ownership,
growth, self-append, bounds/overflow, allocation-failure, file I/O and process
regressions pass with the new tests. `rustfmt --check tests/runtime_core.rs`
passes. No further concrete buffer ownership defect was found in this audit.

Linux execution is unverified. Root owns the full combined Cargo suite and
content-addressed hybrid archive. `core.c` and `core.h` are frozen. The network
lane can remove the Bhasa `process_run` macOS descriptor-race limitation, while
retaining any limitation for foreign code that performs its own spawning.

Frozen SHA-256 values:

```text
3754d7af1e305fc7ea82657648fc77a33d9f6bd5a0d413b7b02107b5efbd1308  runtime/core.c
dd2dcc8b44334a7481b43cbd4dc30da2667cd565e730579fe58d41fb9b0e4c4b  runtime/core.h
```
