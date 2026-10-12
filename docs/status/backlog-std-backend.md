# Standard library and backend lane

October 11, 2026. First increment implemented as isolated drafts. No shared source
integration, Cargo run, broad suite, installation, cache cleanup or Git mutation
was performed by this lane. The complete lane remains active; no todo boxes are
ready to mark complete.

## Draft and baseline

Baseline HEAD is `c4f89575fd0f817f1d0cb02915ab841de166d05f`. Exact source and
existing release compiler hashes are in
`build/backlog/std-backend/baseline/SHA256SUMS`; HEAD is saved alongside them.
Original `std/io.bh` and `std/http.bh` copies are retained in that directory.
The io baseline SHA-256 is
`95cfe757beb9612537ad9c15414b08ac7726ac485955794ceb76c40df2f3ae43`.
Apply the small append patch after checking current io changes, rather than
replacing a subsequently integrated module wholesale.

`build/backlog/std-backend/increment.patch` contains the new files and the io
append patch. The latter is also available separately as `io.patch`. Concrete
files under `build/backlog/std-backend/draft`, with original relative paths:

- `runtime/file.c`, `runtime/file.h`: POSIX regular-file streaming runtime.
- `std/io.bh`: existing API plus typed streaming wrappers.
- `std/http_client.bh`: bounded GET request encoder, response parser and client.
- `std/http_routes.bh`: checked URI decoding and bounded exact routing.
- `tests/file_stream_runtime.c`: native ownership and injected-failure assertions.
- `tests/file_stream_typed.bh`: pending compiler integration acceptance fixture.
- `tests/http_drafts.bh`: 35 executable HTTP/routing expectations.
- `tests/verify_std_backend.py`: bounded standalone verification harness.

The unchanged text/result/collections/http modules in draft/std are probe support
copies, not changes to promote. The harness copies those dependencies from the
current repository to its own output directory.

## File contract and compiler prerequisite

New native symbols use the `bhasa_rt_file_` prefix. Constructors return an owned
opaque handle even on failure; `status` returns zero or a positive errno.
`open` accepts buffer or string paths and mode 0 read, 1 create/truncate write,
2 create/append write, or 3 existing read/write. Permissions use 0666 subject to
umask. Buffer paths reject embedded NUL. Successfully opened directories and
other nonregular files are rejected with EISDIR/ENOTSUP. Earlier OS open errors
are preserved. CLOEXEC prevents inherited descriptors. Synchronous regular-file
I/O has no wall-clock deadline guarantee.

Copies retain one shared descriptor through deterministic reference counting.
Clone performs no allocation or OS operation and preserves status, including for
closed and failed handles. This avoids an implicit descriptor-duplication failure
turning a successful typed open into an unusable success payload. Copies share
the cursor. Explicit close invalidates every alias and remembers its close status;
repeated close never retries the descriptor. Final free closes a still-open
resource exactly once. Reference counts and cursor operations currently require
single-thread access. Do not admit this type into task/channel transfer yet.

Read/write calls each cap a chunk at 1048576 bytes. Read limit zero clears the
destination without a syscall. A successful positive-limit read with zero bytes
means EOF. A read error leaves the destination unchanged, although the cursor
may already have advanced. Write loops over short writes and can leave a prefix
on error. Interrupted read/write/seek/flush operations retry. Zero-byte writes
before completing a nonempty input return EIO. Seek uses absolute nonnegative
offsets; position returns a nonnegative offset or negative errno. Flush requests
fsync and does not promise storage-device durability. Close consumes the
descriptor on any reported failure, matching the existing native close policy;
portable real-device EINTR/close behavior still needs platform review.

The compiler prerequisite is a new owned builtin type named `file_handle`,
lowered to ptr, with `bhasa_rt_file_clone` and `bhasa_rt_file_free` in aggregate,
local, enum, return and exit cleanup. Register these borrowed operations in both
frontends, preserve original diagnostics, and link/cache-hash file.c/file.h:

| Builtin | Checked signature | Native result |
| --- | --- | --- |
| file_open | buffer or str, int -> file_handle | owned constructor |
| file_status | file_handle -> int | zero or positive errno |
| file_read_chunk | file_handle, buffer, int -> int | zero or positive errno |
| file_write_chunk | file_handle, buffer -> int | zero or positive errno |
| file_seek | file_handle, int -> int | zero or positive errno |
| file_position | file_handle -> int | offset or negative errno |
| file_flush | file_handle -> int | zero or positive errno |
| file_close | file_handle -> int | zero or positive errno |

These operation statuses require the same failure-use treatment as existing
native I/O statuses. Reject file operations and file_handle aggregates from
worker graphs/transfer until concurrency rules cover this shared state. Avoid
adding a reserved scanner keyword. Both parser/checker/emitter paths, host type
visitors, module/generic lowering, editor metadata and formatter type handling
need the new type. Tracked compiler sources remain frozen; this report requests
coordinator routing of that prerequisite to the compiler owners.

The io append defines `file_stream_open`/`file_stream_open_str` returning
IoResult[file_handle], read returning IoResult[buffer], and write, seek, position,
flush and close returning IoResult[int]. Seek succeeds with the requested offset;
flush/close succeed with zero. Wrappers use the existing private Error factory
and preserve positive native codes and operation names. Shared resource state
makes the typed close wrapper affect the caller even if an argument is copied.
Existing IoResult/Error, Result.error(Text), integer-status symbols and their
ABIs are unchanged. The first fixture awaited compiler integration. The follow-up
below records its execution through an isolated Rust host draft. Native frontend
parity and integrated compiler acceptance still remain pending.

## HTTP increment

The new client accepts GET only, numeric IPv4/IPv6 destination addresses, an
explicit validated Host authority, and origin-form targets up to 8192 bytes.
It uses one timeout budget across TCP connect, request write, headers and body.
Headers are capped at 16 KiB and 100 unique fields; body length is capped at
1 MiB. It sends Connection: close and accepts one final HTTP/1.0 or HTTP/1.1
response with mandatory Content-Length. It rejects transfer encoding, duplicate
headers, informational responses, upgrades, oversized lengths and surplus bytes
already received. HTTP 204/304 require zero length in this subset. It does not
wait for EOF after consuming the declared length and does not pool connections.
Timeout argument bounds preserve the existing native deadline ABI.

Response parsing reuses the existing strict header parser through a synthetic
HTTP/1.0 request line. Therefore it also validates a response Host header if
present and rejects repeated Set-Cookie. These deliberate subset limits must be
documented before promotion. HttpClientResult.error carries Text and does not
change the existing Result/Text or IoResult/Error contracts. The generic stream
function supports the existing tcp_socket/tls_socket stream ABI, but actual TLS
client execution has not been tested. Its caller owns cleanup; http_get owns its
TCP temporary through ordinary scope cleanup.

Targets decode percent escapes once. Query '+' becomes space; path '+' remains
literal. Reject malformed escapes, control bytes, invalid UTF-8, decoded path
backslashes/slashes from escapes, dot segments, duplicate decoded query keys,
empty query keys/fields and more than 100 query fields. Routes use exact
case-sensitive method and decoded UTF-8 path with nonnegative application IDs,
at most 256 entries, private storage and duplicate rejection. Query values stay
separate from route matching. No wildcard routes, callback dispatch, middleware,
keepalive, chunked/streamed response bodies or concurrent server claim is made.

## Evidence and next coordinated gates

The existing release compiler accepted the HTTP/routing program and emitted
LLVM. Strict C11 builds with -Wall/-Wextra/-Werror/-pedantic, ASan and UBSan
passed at O0 and O2. Native streaming tests finish with zero tracked allocations
and zero tracked descriptors. They exercise binary write/read, EOF, shared
cursor/copies, final-owner cleanup, close invalidating aliases, read EINTR,
failed read after cursor advance, partial ENOSPC writes, zero-write EIO,
write/flush EINTR, flush failure, close failure after real close, repeated close,
append, seek, invalid bounds/mode, NUL path, directory and missing file errors.
The 35 HTTP/routing assertions pass at both optimization levels. They cover
query decoding, Unicode, malformed/ambiguous targets, duplicate queries/routes,
method separation, valid/error response framing and request injection rejection.
HTTP tests only execute parsing and invalid-request preflight; socket response
execution remains pending.

Reproduce without Cargo or live network:

```sh
python3 build/backlog/std-backend/draft/tests/verify_std_backend.py \
  --repo . --draft build/backlog/std-backend/draft \
  --compiler target/release/bhasa \
  --output build/backlog/std-backend/evidence/final
```

Final command transcript is
`build/backlog/std-backend/evidence/final/verification.log`. The host is macOS
ARM64. The existing release compiler rejects --frontend bhasa because that binary
was built without the Bhasa frontend. This is a capability limit, not a parity
pass. No build slot was consumed to rebuild it.

Requested coordinated verification after the compiler prerequisite is integrated:

1. Compile the typed fixture beside draft io.bh/text.bh with both frontends,
   providing a temporary path. Expect nine true lines, then exercise propagation,
   private Error storage, malformed checked AST and nested aggregate cleanup.
2. Run the harness with each refreshed frontend executable, and add socket tests
   for fragmented headers/body, EOF, deadline expiry, zero-byte reads, keepalive
   peers, extra buffered bytes, malformed statuses and generic TLS cleanup.
3. Review shared-close/reference-count semantics independently, with allocation
   counters across enum return/copy/try paths. Verify task/worker rejection.
4. Coordinator owns Cargo integration gates, combined suites, cache/self-build
   checks, release binaries and acceptance on other platforms. POSIX implementation
   requires a guarded unsupported-target diagnostic or Windows implementation.

After these prerequisites, next owned work is bounded HTTP interoperability and
middleware dispatch, then private collection storage/capacity/algorithms and
paths/directories/environment/process capture. Wider Unicode/hash protection,
math/time/regex/compression/logging/config/metrics/serialization, concurrent
serving/WebSockets/database/UDP/Unix/TLS extensions remain unimplemented. They
depend on the lane APIs and the separate ownership/concurrency/package contracts.

## Follow-up: HTTP fixtures and Rust host draft

The coordinator requested live interoperability fixtures and a feasible Rust
host integration patch. Both are now implemented. No Cargo command or socket
listener was run by this lane. The loopback tests are authored for the authorized
coordinator gate and remain unexecuted here.

`draft/tests/http_client_interop.rs` is a Cargo integration-test draft. After
promoting the HTTP library modules and this test, run:

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test http_client_interop -- --test-threads=1
```

Repeat with `--features bhasa-frontend` in the coordinated hybrid build slot.
The fixture runs each available frontend, uses an isolated ephemeral loopback
port per scenario, caps accept waits at four seconds and socket read/write waits
at two seconds, kills/reaps client subprocesses after a four-second budget, and
bounds compiler subprocesses at forty seconds. Actual bind/accept failures fail
the test; they do not count as skipped network acceptance. Only absent Clang
permits a native-test skip.

Seven live scenarios check fragmented headers and binary body, header/body EOF,
header timeout while the peer stays open, slowly advancing body with one total
deadline, completion while a keepalive peer stays open, and malformed status.
The Rust peer checks the exact emitted GET/Host/Connection request. Error cases
exhaustively match HttpClientResult and compare the owned Text message. The
keepalive peer receives its release signal only after client final output, so
waiting for EOF cannot masquerade as successful length-framed completion.

The second test checks extra already-buffered bytes with the actual response
parser. It deliberately avoids asserting TCP read packet boundaries. A peer can
send surplus bytes after the client has consumed Content-Length; the current
Connection: close policy closes without reading those later bytes. This is the
documented policy, not proof of a stronger surplus-detection guarantee.

The HTTP Rust fixture passes `rustc --test --emit=metadata -D warnings`, using
the repository common test module through a path-adjusted evidence copy. The
embedded Bhasa client program passes the existing release compiler's check.
Metadata output is `evidence/http_client_interop.rmeta`; the temporary Rust
source is `evidence/http_client_interop_metadata.rs`. These checks do not bind
sockets and do not establish live interoperability.

`build/backlog/std-backend/file-host.patch` holds seven small source diffs:
ast.rs, parser.rs, builtins.rs, sema.rs, codegen.rs, generics.rs and modules.rs.
Full review copies are in `draft/src`. The exact pre-edit copies and hashes are
in `baseline/rust-host`, based on HEAD
`e24aa78ba10c146020e48d984d4267dbabe7c630` and the current binding-integration
working-tree contents. These source hashes, not HEAD alone, identify the baseline.
Preserve later coordinator changes by applying individual hunks. Do not replace
current tracked sources wholesale from draft/src.

The host patch adds FileHandle ownership/display/parser/generic/module handling,
all eight builtin signatures and native names, must-use checking for file open
and statuses, clone/free cleanup registration and a new `; bhasa runtime: file`
marker. It does not change the driver, linker, cache, native emitter, editor
completion list or grammar. Root owns those integration changes.

Program.contains_file_handle recursively inspects record/enum fields, including
instantiated List/Map records. Post-check worker graph validation rejects file
operations and file-containing input/result signatures for spawn and async_spawn,
including transitive global/method calls. Emission independently checks the graph
at every spawn, so a corrupted checked worker body cannot bypass the restriction.
The call graph and violation predicate are crate-private shared helpers. No new
native scanner token or reserved source keyword is added.

Focused standalone verification used existing rustc without Cargo. An isolated
library maps only the seven draft modules into the current host and keeps other
modules at their existing locations. `draft/tests/file_handles_reference.rs`
passes all four tests: checked types/ABI/cleanup markers, discarded statuses and
wrong types, direct/nested/transitive worker rejection, and a corrupted checked
worker body. Evidence is `evidence/file-handles-reference.log`. The fixture
initially omitted required enum/field separators; these were repaired before the
passing run. No rejected fixture was counted as a worker-restriction pass.

The isolated host also emitted LLVM for the imported typed streaming fixture.
It needed explicit IoResult[file_handle]/IoResult[int]/IoResult[buffer] match
qualifiers; the fixture is repaired in its draft path. Manually linking that
LLVM with the draft file runtime and current core/text runtimes passes strict
C11, ASan and UBSan at O0 and O2. Each execution prints exactly nine true lines.
The probe includes typed open/read/write/seek/position/close, shared-alias close
invalidation, preserved error code and binary/EOF behavior. Logs are
`evidence/typed-file-probe/O0.log` and `O2.log`. This establishes focused Rust
draft execution, not integrated Cargo, Bhasa frontend, typed allocation-counter,
real-device close-failure or cross-platform acceptance.

The original standalone native/HTTP harness was rerun after this follow-up and
passes. Its transcript is `evidence/final-followup/verification.log`. Native
allocation/descriptor counters remain zero. `increment.patch` now includes the
two new Rust tests and repaired typed fixture; file-host.patch stays separate.

Remaining coordinated gates:

- Promote/review file-host.patch while retaining current immutable-binding edits.
  Root must link/embed/cache-hash file.c/file.h and honor the file marker; add a
  guarded unsupported-target diagnostic for non-POSIX builds until implemented.
- Route the exact owned type, must-use and worker restrictions to the native
  frontend owner. Update host editor/type inspection lists. Verify both frontend
  cleanup paths, lexical exits, propagation, imports/generics and corrupted ASTs.
- Run `cargo test --offline --locked --test file_handles_reference` serially
  with the coordinated two-job environment, then typed sanitizer/counter fixtures
  and the live HTTP commands above. Full integration gates remain root-owned.
- Extend HTTP live acceptance to TLS streams, adversarial header/body limits and
  repeated deadlines. Keepalive reuse, chunked streaming, middleware callbacks
  and concurrent serving still require later contracts.

## Pending-aware correction and independent next contract

Root's independent review found the positive file reference fixture bound
`result = wrap(copied)` without discharging the exact ok/error Opened enum.
The fixture now directly matches that local. The ok arm retains the owned Holder
payload and checks its resource status; the error arm returns the code. File
copies/cleanup and the separate typed fixture's alias-close assertions remain.
This is real failure handling, with no shape change or discard helper.

The correction-only diff is `build/backlog/std-backend/file-reference-pending.patch`.
`baseline/reference-before-pending.rs` reconstructs its pre-edit source by
reversing precisely that insertion. `increment.patch` is refreshed. Final draft,
patch and reconstructed-baseline hashes are in
`evidence/pending-correction/final-draft.sha256`; probe source/compiler hashes are
in that directory's `SHA256SUMS`. The file runtime, wrappers, host patch and HTTP
sources remain unchanged by this correction.

No compiler rebuild or Cargo operation was performed. Existing executables ran
bounded checks with twenty-second process limits:

- The exact corrected file fixture checks and emits LLVM through the final native
  file seed at `build/backlog/native-compiler/file-handle/stage-a`.
- Removing the match from that exact fixture reproduces native BH221.
- Current `target/debug/bhasa` lacks integrated file_handle/file builtins and
  therefore rejects the exact file fixture with unknown-type/function diagnostics.
  This is not recorded as file acceptance or a pending-rule rejection.
- A separate source-check projection substitutes existing tcp_socket/native TCP
  APIs and a scalar seek stub, retaining the same Holder/Opened/result/match flow.
  The pending-aware debug compiler accepts it; removing the match produces S037.
  No projected program ran and no network operation was performed. The projection
  proves the pending obligation is discharged, not file ownership semantics.

Commands, statuses, source files, diagnostics and native LLVM are retained under
`evidence/pending-correction`. The fixture still requires root's integrated
`file_handles_reference` gate; earlier isolated-library test evidence predates
S037 and must not be substituted for that gate. Root owns LSP reserved rename
types, runtime linkage/cache/target gates and final both-frontend acceptance.

The separate next draft is `build/backlog/std-backend/typed-errors`. It does not
edit or depend on changes to the frozen file increment. Its
`io-network-process.append.bh` can be appended in the existing Error defining
module after file integration; `io-network-process.patch` records the same
append against the captured current io baseline. Full original-path copies are
in its draft directory; baseline source/compiler/runtime header hashes and HEAD
are retained separately. Apply the append contextually, never replace the io
module from that copy. `typed-errors/draft.sha256` identifies this draft.

The proposed APIs are tcp_receive(tcp_socket,int,int) -> IoResult[buffer],
tcp_send(tcp_socket,buffer,int) -> IoResult[int], and process_execute(buffer) ->
IoResult[ProcessOutcome]. TCP wrappers borrow existing sockets and preserve the
positive errno, exact operation name, native limit/deadline validation and empty
read/EOF behavior. Send success reports complete input length, with partial
transmission possible on error. No new memory bound or retry guarantee is added.
They reuse ErrorDomain.os without changing Error, IoResult, Result/Text or native
symbols/status conventions.

ProcessOutcome has private status storage with status() and succeeded() getters.
Negative process_run statuses become OS Error codes by sign inversion. Every
nonnegative status, including nonzero child exits, is a successful execution
outcome. The native ABI conflates exit code 128+signal with signal termination;
this draft preserves that integer and invents no termination tag. Capture,
environment overrides, cancellation and async process handles remain separate.

The checked-only imported fixture and six bounded source probes pass through the
existing pending-aware debug compiler. They cover handled receive/send/process
results, initializer try propagation, S037 on assigned unhandled receive/process
results, private ProcessOutcome construction/status storage and private status
factory access. Logs and expected diagnostics are in typed-errors/probe. No guest
network/process function executed; no subprocess wrapper runtime acceptance is
claimed. The next coordinator gate needs native-status comparison, zero/empty
read, deadline/partial-write faults, invalid process arguments, child exit0/nonzero,
signal ambiguity, original-file diagnostics and O0/O2 owned-error cleanup.

Two contract decisions remain before broader typed networking:

1. TCP constructors/clones duplicate descriptors and clone can return an error
   handle after an earlier constructor check. An IoResult[tcp_socket] success must
   not promise a usable resource after a fallible implicit enum/return copy. Await
   the ownership-transfer contract or a reviewed handle-copy policy; do not repeat
   the defect avoided by shared file resources.
2. TLS statuses include native constants 10001 certificate, 10002 protocol and
   10003 configuration errors, alongside OS statuses. Labeling them all as OS
   errno would be false. A separate network/TLS error type or a reviewed extension
   to ErrorDomain needs coordination with safety-errors and compatibility handling
   for exhaustive matches. No TLS typed error wrapper is published by this draft.

## Ratified typed networking/process and TLS errors, October 12

Root ratified tcp_receive/tcp_send and process_execute exactly as recorded above.
The isolated runtime acceptance is now implemented and executed. The frozen file
increment, host patch and file seed remain unchanged. No Cargo, compiler rebuild,
SDK installation, Git mutation or tracked shared-source edit was performed.

The actual next library patch is
`build/backlog/std-backend/typed-errors/typed-errors.patch`; its separate hash is
typed-errors.patch.sha256, and draft.sha256 pins the individual source/test files.
It includes the contextual io append, new std/tls_result.bh, scalar C classifier
runtime/tls_errors.c/h and acceptance fixtures/harness. Root must append IO changes
after file integration, export the new TLS module deliberately and link/cache-hash
the classifier. No existing IoResult/Error/Result/Text or integer native ABI is
replaced. Source/runtime snapshots are retained under typed-errors/baseline.

The actual TLS library contract is a separate exhaustive TlsErrorDomain with
os, certificate, protocol, configuration and unknown_native variants. TlsError
privately stores that domain, the exact native int code, str operation and owned
Text message. Public domain()/code()/operation()/message() getters return values;
message() returns an independent owned Text copy. TlsResult[T] is exactly ok(T)
or error(TlsError). It has no conversion into the existing OS Error domain.
Existing exhaustive ErrorDomain.os matches remain valid, as the acceptance
fixture demonstrates. TLS operation wrappers have not been exposed yet.

The public tls_status_result(status:int, operation:str) -> TlsResult[int] adapter
succeeds with zero only for native status zero. Every nonzero status preserves
its exact code and operation. Dedicated statuses 10001/10002/10003 map to
certificate/protocol/configuration before any errno matching. A scalar foreign
C helper bhasa_tls_error_domain recognizes the captured host's supported POSIX
errno symbol values using conditional compilation. It returns internal tags
0 OS, 1 certificate, 2 protocol, 3 configuration, 4 unknown native. It guesses
no positive-number errno range. Unrecognized values, including defensive negative
adapter input, map to unknown_native. Platform-specific errno values outside the
explicit symbol set remain unknown until reviewed. The helper requires no OpenSSL
or new SDK and does not call TLS, allocate, retain pointers or unwind.

The harness is draft/tests/verify_typed_errors.py. Reproduce the executed probe:

```sh
python3 build/backlog/std-backend/typed-errors/draft/tests/verify_typed_errors.py \
  --repo . --rust target/debug/bhasa \
  --native build/hybrid/frontend/aarch64-apple-darwin/release/bhasa \
  --output build/backlog/std-backend/typed-errors/probe/final-runtime
```

Both existing compiler frontends emit the actual imported guest programs. Eight
guest executions pass across Rust/native, O0/O2 and the two fixtures. Strict C11
warnings-as-errors, ASan and UBSan pass; LLVM guest allocations and core/text/net
allocations share one counted allocator. Every execution ends with zero counted
allocations; the network fixture also closes its active peer before exit. The
transcript is probe/final-runtime/verification.log and compiler/runtime identities
are in execution.sha256. These probes use the captured pre-copy-policy net.c
runtime, not an unexecuted future safety-errors TCP implementation.

Each TCP/process run produces 28 true assertions. An isolated constructor seam
creates configured nonblocking Unix socket pairs in place of TCP connect. Real
native send/recv/poll/read/write deadlines execute without listening, connecting
to an external endpoint or resolving DNS. Equivalent fresh streams compare typed
and native statuses/payloads. Coverage includes binary receive, peer EOF, limit0,
real twenty-millisecond timeout, invalid limit/deadline, complete write, injected
two-byte partial write followed by ENOSPC, and injected zero-send EIO. Both
native and typed partial writes leave the same observed prefix; errors preserve
the code and operation. Error message copy mutation does not change stored text.
Native failed-read destinations remain unchanged.

Process tests execute the real process_run and typed process_execute against empty
and unterminated argument records, plus /bin/sh -c exit0 and exit7. The argument
record includes argv0 and explicit NUL terminators. Native negative statuses map
to exact positive errors. Nonzero exit7 remains a successful ProcessOutcome with
succeeded()==false. The existing signal/exit ambiguity is retained; actual
signal-termination and subprocess capture are not claimed by these executions.

Each TLS error run produces 19 true assertions, covering zero success, the host's
EINVAL value, all three dedicated TLS codes, an unknown positive code and a
negative defensive input. Domain/code/operation/message copy and cleanup paths
execute in actual generated guests. This is TLS error-classification acceptance,
not a TLS handshake, certificate validation or TLS stream I/O test.

Fourteen negative checks across both frontend modes preserve original importing
paths at line4 after a multibyte comment. They exercise private TLS code/domain
fields, validly typed private TLS construction, private factory access, private
ProcessOutcome status and assigned pending TLS/process Results. Records are in
probe/final-runtime/original-diagnostics.json. Imported private modules trigger
the shared host privacy/checking gate before native emission, so S037 in native
mode is explicitly shared-host evidence, not a BH221 execution claim. An initial
maltyped constructor probe was corrected before privacy acceptance; its ordinary
type error was not counted as privacy enforcement.

TCP constructors remain withheld. Root assigned safety-errors the reviewed atomic
shared-descriptor copy-policy correction; this lane needs that actual API/lifetime
contract and integration evidence before publishing IoResult[tcp_socket] factories.
The current I/O wrappers borrow sockets and return only independently owned buffers
or scalar byte counts, so they do not depend on implicit socket-copy success.
The socketpair fixture's private constructor seam must be adapted and rerun after
that representation changes. It is deliberately pinned to the captured runtime.

Next coordinator acceptance remains integrated Cargo/frontend gates, networking
on real TCP endpoints, shared-copy-policy fault/resource tests, signal outcomes,
TLS stream statuses through the new adapter and O0/O2 shutdown/poisoned-session
cleanup. No backlog box or platform/hardware acceptance is inferred from this
bounded draft evidence.

## Borrowed TLS operation contract, October 12

Before implementation, the next contract is tls_receive(tls_socket,int,int) ->
TlsResult[buffer] and tls_send(tls_socket,buffer,int) -> TlsResult[int], in a new
std/tls_io.bh that imports only tls_result.bh. std/io.bh will not import this
module; ordinary OS/TCP/process users therefore acquire no TLS stream dependency.
The socket parameter is borrowed; no typed socket-returning constructor is added.

Receive uses the actual tls_read builtin and an owned temporary destination.
Success returns the bytes received, with empty success meaning limit0 or an
authenticated close_notify. Abrupt transport EOF is not silently relabeled as
authenticated shutdown. Failure returns the exact native code and operation
tls_read through the published TLS classifier; no partial destination is exposed.
Send uses tls_write and succeeds with the complete input length. A failed send
may already have transmitted an application prefix, without a returned byte count
or rollback/retry guarantee. Its error operation is tls_write.

Native timeout validation and the monotonic operation budget remain unchanged.
The budget covers session-lock acquisition and network progress; synchronous
cryptographic CPU work is checked afterward and is not preempted. Protocol/I/O
failures and timeouts after SSL calls poison the shared session; subsequent
operations preserve that stored status. A timeout while acquiring the session
lock does not poison it. The wrappers neither clear poison nor retry failed TLS
operations with changed buffers. Explicit tls_shutdown remains the native
close_notify operation, and last-owner cleanup retains its existing non-waiting
transport/session release. Planned acceptance will distinguish classifier tests,
failed-handle operations, actual handshake/stream scenarios and unexecuted gates.

## Resume and TLS stream follow-up, October 12

The interrupted implementation is packaged separately under
`build/backlog/std-backend/tls-streams`. Observed coordinator HEAD advanced from
217e33d to 621a977 during pickup. Only this lane status file and ignored lane
artifacts were written. No Cargo, compiler rebuild, listener, SDK installation,
shared production-source integration or Git mutation was performed. The root's
latest instruction confirms that real listeners remain root-owned until the
TCP shared-state copy runtime is integrated. Socket-returning typed constructors
remain deferred. No owned background process is left running.

The full previously frozen typed-errors draft manifest and patch hash still match.
The file increment and file-host patches were not edited. This follow-up does
not replace or regenerate those patches. New patches and hashes:

- `tls-io.patch` adds only std/tls_io.bh. SHA-256
  d8a21f4b0f0baf13868abe9705d915ad046cc0c5160e15030004aa3651b6d3bd.
- `endpoint-fixtures.patch` adds tests/tls_failed_handle.bh,
  process_signal.bh, endpoint_client.bh and verify_endpoints.py. SHA-256
  059600f1cec5fa390d197c7acf1193cddb923312ae566fda8e61b62d6479c7e2.
- `shared-policy-fixture.patch` contextually adapts the previously handed-off
  tests/network_process_runtime.c constructor seam and appends the host
  ETIMEDOUT helper to tests/tls_error_runtime.c. SHA-256
  2027fb599e6fec873a3fedb5e2dcb9be1aceb95e99a5e30379f1c4da89394091.

These depend on the frozen typed-errors patch, not a wholesale std/io replacement.
Apply the network seam correction only alongside the reviewed shared-state net
runtime. The copied fixture now uses new_shared(pair[0]) and shared->fd. Its
captured net.c/net.h match the actual safety-errors handoff byte-for-byte. The
original pre-policy fixture and its frozen evidence remain intact.

Twelve actual guest executions pass with the existing Rust and native frontends
at O0 and O2. Each optimization/frontend pair runs 28 TCP/process assertions,
four signal assertions and four failed-TLS assertions. Strict C11 warnings,
ASan and UBSan pass; guest and captured core/text/net/tls allocations use the
counted allocator and end at zero. OpenSSL's internal allocations are not part
of that counter; this is not an assertion about all OpenSSL process-global state.
The new failed-handle fixture loads a nonexistent CA path, producing configuration
10003 before network connection. It checks exact native/typed read and write
codes, configuration domain, failed-read destination preservation and operation.
The real subprocess fixture compares /bin/sh self-SIGTERM with explicit exit143.
Both native and typed outcomes are successful ProcessOutcome values with status143
and succeeded()==false. The native API still does not distinguish those cases.

Evidence is `probe/no-listener/verification.log` and `execution.sha256`.
`dependency.sha256` pins captured runtimes and prior dependencies; `draft.sha256`
and `patches.sha256` pin the follow-up. `checkpoint.json` records the remaining
coordinator gate. Reproduction uses only existing executables:

```sh
python3 build/backlog/std-backend/tls-streams/draft/tests/verify_streams.py \
  --repo . --rust target/debug/bhasa \
  --native build/hybrid/frontend/aarch64-apple-darwin/release/bhasa \
  --output build/backlog/std-backend/tls-streams/probe/no-listener
```

Four endpoint clients were compiled and linked at both frontend/optimization
pairs, without execution. The Python endpoint runner passes syntax compilation.
Its seven scenarios per client are real loopback TCP binary traffic and EOF,
verified TLS binary traffic and authenticated close_notify, post-handshake read
timeout, abrupt transport EOF, wrong server name, untrusted CA and a plain-text
peer. Timeout and abrupt-EOF cases check exact stored status across an alias,
unchanged native failed-read destination and rejected subsequent typed writes.
Certificate/protocol handshake errors check exact codes and domains. Each peer,
subprocess, handshake and join has a deadline. Peer failures propagate, and only
specific certificate-alert failures are accepted in rejection scenarios. Test
certificates are generated freshly through tests/fixtures/tls/generate.sh using
existing OpenSSL. They are temporary test assets, not production trust material.

The exact root-owned live gate against the compiled captured runtime is:

```sh
python3 build/backlog/std-backend/tls-streams/draft/tests/verify_endpoints.py \
  --repo . --openssl /opt/homebrew/Cellar/openssl@3/3.6.4/bin/openssl \
  --output build/backlog/std-backend/tls-streams/probe/live-endpoints \
  --clients \
  build/backlog/std-backend/tls-streams/probe/no-listener/endpoint_client-rust-O0 \
  build/backlog/std-backend/tls-streams/probe/no-listener/endpoint_client-rust-O2 \
  build/backlog/std-backend/tls-streams/probe/no-listener/endpoint_client-native-O0 \
  build/backlog/std-backend/tls-streams/probe/no-listener/endpoint_client-native-O2
```

This gate is authored and unexecuted. After contextual integration, root must
also compile/run these clients against the integrated runtime and current
frontends; captured-runtime acceptance alone does not establish integration.
No actual handshake, TLS stream timeout, abrupt-EOF or close_notify result is
claimed here. Deterministic partial TLS write and session-lock contention remain
additional runtime acceptance work; the borrowed send contract preserves native
prefix/poison semantics without promising rollback. Combined Cargo/frontend,
independent review and native shutdown/resource gates remain coordinator-owned.

## Contextual rebase onto integrated interfaces, October 12

Root requested a separate rebase while the current source freeze undergoes full
hybrid acceptance. The new handoff is
`build/backlog/std-backend/current-interface-rebase`. Observed HEAD is
60d7424d359021bf0d38bef5253f7356364b832b; current uncommitted std/runtime/driver
bytes are captured and pinned in baseline.sha256. All original draft sources,
patches, transcripts and hashes remain unchanged. Original patch identities are
also recorded in original-artifacts.sha256. This is a replacement contextual
handoff for overlapping hunks, not an instruction to apply both versions.

Four narrow patches, with hashes in patches.sha256:

- io-network-process-current.patch, SHA-256
  131fcbab5e062f5b82cd9d88393c0ff43e6badfef96da40022fe471d7099f8e6.
  It appends after the current file_stream_close function. All existing I/O and
  streaming file declarations remain byte-identical. Its obsolete descriptor
  duplication comment now describes current shared ownership. Socket constructors
  are still withheld pending root's final integration/acceptance decision.
- tls-support-current.patch, SHA-256
  c3c0534c469c0e5a9f21179bfc09edfb4496169ac30bc92e596d7db562e13cfc.
  New runtime/tls_errors.c/.h and std/tls_result.bh/tls_io.bh only.
- http-current.patch, SHA-256
  f6042762463e7fe7eeec6facce6a2a1538c0baed5a909590b5a2b597cff74e02.
  New client/routes and HTTP fixtures now call Text.bytes(); parser, deadline,
  routing and response policies are retained.
- typed-fixtures-current.patch, SHA-256
  0edf0203374028df8c29c060c8e05fdcf7052d7b6c41a4f6eab5e313c4ac08b5.
  It adds adapted network/process/TLS fixtures against the actual shared-owner
  representation and includes the previously prepared root-only endpoint gate.

These patches do not alter existing runtime/net.c, driver or shared test runners.
Their new-file paths were absent when generated. If an old handoff is integrated
before application, root must compare the relevant hunks, not replace whole files.

Actual source inspection confirms the native semantics: successful resolver and
numeric deadline constructors preserve status0 and acquire one shared fd;
clones retain that same fd and copy failed status without a new descriptor.
The actual net.c/net.h and tls.c are byte-identical to the previous follow-up's
captured shared-policy/TLS runtime. TCP read still replaces its destination only
on success and returns empty success for zero limit or transport EOF. TCP write
still loops to completion and can return its exact positive errno after an
observable prefix, without a rollback or byte count. Process run returns negative
OS errors, ordinary exit codes and 128+signal for termination; the adapter maps
only the negative values to positive-code IoResult.error. Nonzero exit and signal
statuses remain successful ProcessOutcome values.

TLS codes remain certificate10001, protocol10002 and configuration10003. The
classifier checks those first, then explicit host errno symbols; unknown or
negative defensive codes retain their exact value in unknown_native. No errno
range is inferred. TLS read/write temporary ownership, authenticated EOF,
post-SSL poison and shared-status preservation remain unchanged. These statements
are current source inspection plus prior executed evidence, not new runtime
acceptance during root's source freeze.

Fourteen small existing-compiler emissions pass, covering network operations,
signal outcomes, failed TLS handles, TLS classification, endpoint clients, HTTP
and the checked-only typed wrapper fixture across both frontend modes. Evidence
is probe/source-check/verification.log, runtime-markers.txt and source-check.sha256.
No Cargo, compiler rebuild, Clang runtime build, guest execution, live listener or
shared production-source edit was performed in this rebase slot. No process is
left running. Source snapshots were checked against actual files after packaging.

The new manual-link harness selects runtime/file.c whenever the emitted file
marker is present. Both frontend modes emit that marker for network_process,
process_signal, tls_error_runtime, endpoint_client and checked wrappers, because
current std/io imports the streaming declarations. The failed-TLS fixture uses
only TLS/text/core/net; HTTP fixtures emit core/text/net. This selection preserves
root's repaired file-runtime linking rather than using the old pre-streaming
manual link lists. The classifier is explicitly linked when its external symbol
is emitted. Actual src/driver.rs does not yet embed that helper; ordinary adapter
builds require --link runtime/tls_errors.c until root chooses reviewed conditional
embedding. Classifier-only programs must remain independent of OpenSSL.

Exact source-check reproduction, with no runtime execution:

```sh
python3 build/backlog/std-backend/current-interface-rebase/draft/tests/verify_rebased.py \
  --repo . --rust target/debug/bhasa \
  --native build/hybrid/frontend/aarch64-apple-darwin/release/bhasa \
  --output build/backlog/std-backend/current-interface-rebase/probe/source-check
```

The README in the handoff gives the exact deferred --execute command, four-client
live endpoint command and both Cargo HTTP interoperability commands. Those gates
remain unexecuted here and require root's later execution slot. The independent
review, integrated runtime execution, 35 HTTP expectations, real TCP/TLS scenarios,
current driver classifier link selection and combined suites remain root-owned.
The earlier isolated runtime passes retain their exact captured-source scope;
this rebase adds source compatibility evidence without relabeling them as new
integrated acceptance.

## Separate HTTP parser boundary correction, October 12

The independent current-interface review is
build/std-current-rebase-independent-review/report.md, SHA-256
 a4cc51180057145d42f48a42b27100f0913a240de09b7f66be4e0e1d7552598b.
Its two HTTP findings are addressed by separate contextual drafts under
`build/backlog/std-backend/http-boundary-correction`, without editing the reviewed
rebase or shared source. The exact available probe record is
http-public-boundary-counts.json, alongside http-public-boundary.bh and emitted
LLVM. It proves source/copy order and byte arithmetic, not runtime allocation.

Corrections, with hashes in patches.sha256:

- http-request-bound.patch, SHA-256
  e057753ec6d40bc31c5f6938814daea6e42948797be801e09d4256f952035370.
  This is a narrow delta atop actual std/http.bh: total-input preflight and
  received-length comparison before body slice.
- http-client-bound.patch, SHA-256
  9cf1f78b37ea9e7057874ac27b8801b2c1bfdd9028eed2ddd8756848988d2203.
  This applies after the reviewed HTTP new-file handoff: total-input preflight,
  header-only normalization and validation before response body copying.
- http-boundary-fixtures.patch, SHA-256
  68598a48048b834da21e8ac269d3e032914273a0af9bd393de24fe98e01f604e.
  Four new boundary/framing/copy-counter files only.

The total-input limit is 1,064,960 bytes, equal to the existing maximum 16 KiB
header plus 1 MiB body. Above-limit input now receives a specific preflight
error. Within the bound, surplus keeps the existing rejection message and is
checked before body copying. The response synthetic request contains only the
header fields; its 14-byte start line is shorter than every accepted response
start line. The original wire-header guard remains authoritative, so a valid
empty-reason response header ending at exactly16,384 is accepted by the reused
parser, while an original16,385-byte header is still rejected. Content-Length,
partial prefix, body-free status, duplicate-header, routing, deadline and socket
ownership policies remain unchanged. No TCP/TLS/process code was altered.

Eight existing-compiler emissions pass with both frontend modes: the review's
original boundary fixture, 22 framing assertions, 16 rejection/copy/allocation
assertions and the unchanged35 HTTP/routing fixture. Sources and compilers are
identified by draft.sha256, baseline.sha256 and probe/source-check/source-check.sha256;
probe/source-check/verification.log records commands. The three correction patches
replay exactly with zero fuzz and offsets under probe/replay. All previous rebase,
TLS-stream and frozen typed-error manifests still match. Actual std/http.bh still
matches the captured correction baseline. No shared source edit, runtime/Clang
build, guest execution, Cargo command, heavy native rebuild or listener ran.

The correction README provides exact source-only and deferred --execute commands.
Root's latter gate will run sixteen O0/O2/frontend guests with strict C11,
ASan/UBSan and zero counted allocations, then measure rejection copies and largest
allocation after building each input. Both131,072-byte surplus and combined-limit
plus-one are prepared for request/response rejection. The counters instrument
emitted slice/append calls and Bhasa malloc/realloc requests; they do not measure
all libc copies or process RSS. Runtime evidence for those bounds remains pending.
Root must overlay these corrections before using the earlier captured rebase
HTTP execution harness, or use the new correction harness and final integrated
sources. Integrated stdlib and HTTP interoperability acceptance remain root-owned.

The separate TLS classifier driver gate remains with root: select the helper
before the no-core early return, hash its C/header, and keep scalar-only classifier
use independent of net and OpenSSL. This correction introduces no driver policy.

## Authorized HTTP runtime slot passed, October 12

Root authorized exactly the prepared HTTP correction runtime command after the
independent correction review passed. The review is
build/std-current-rebase-independent-review/http-boundary-correction/report.md,
SHA-25652d4e10733b018bdd4051f3db9be27d9b314f1d85361323bbd9ff9f7493dcdc7.
The single first attempt completed with exit0 and output:

```text
PASS: eight source emissions; sixteen sanitizer/counter executions
```

Executed command:

```sh
python3 build/backlog/std-backend/http-boundary-correction/draft/tests/verify_http_bounds.py \
  --repo . --rust target/debug/bhasa \
  --native build/hybrid/frontend/aarch64-apple-darwin/release/bhasa \
  --output build/backlog/std-backend/http-boundary-correction/probe/runtime --execute
```

All sixteen guest invocations passed: the reviewer boundary/surplus fixture,
22 framing assertions, 16 rejection/copy/allocation assertions and the unchanged
35 HTTP/routing expectations, each with Rust/native and O0/O2. Strict C11
warnings-as-errors and nonrecovering ASan/UBSan completed without failure. Every
guest exits with zero counted allocations. The rejection fixture's observed
checks confirm copied bytes, largest copy and largest tracked malloc/realloc
request each stay below4096 for its short-header131,072-byte surplus and
combined-limit-plus-one cases, on both request and response paths. The exact
16,384-byte empty-reason response and maximum combined valid framing pass.
No oracle or fixture was changed, weakened or retried during this slot.

The first harness result and exact sixteen executable paths are preserved in
http-boundary-correction/probe/runtime/slot-result.json. Full command/output
transcript is probe/runtime/verification.log, SHA-256
66537d241d1102d63e241d550cf4f8ab500d41caa6a1a97a2a423eb1907d949d.
Compiler identities are in probe/runtime/source-check.sha256 and generated
LLVM/object/executable identities in runtime-artifacts.sha256. The correction
baseline, draft, patch and original-artifact manifests still match.

The harness exited successfully; no process from this slot remains. No Cargo,
heavy native compiler rebuild, shared-source/Git/SDK/cache change, live listener
or full-rebase runtime probe ran. These are actual executions of captured current
runtimes plus the separately corrected HTTP drafts, not final shared-source
integration proof. Root's integrated stdlib/HTTP interoperability, scalar TLS
classifier driver wiring, full typed/TLS rebase and live endpoint gates remain.
