# Backend deadlines and foundational libraries

The deadline runtime, UTF-8 runtime, Text and concrete collections are implemented.
This report covers runtime/net.c/h, runtime/text.c/h, std/text.bh,
std/collections.bh and their focused tests. JSON, HTTP, Result/Option enums and
combined compiler verification have separate owners. The core runtime is unchanged.

## Runtime contract

The agreed deadline builtins are:

- `tcp_accept_timeout(tcp_listener, milliseconds) -> tcp_socket`
- `tcp_connect_timeout(str|buffer, port, milliseconds) -> tcp_socket`
- `tcp_read_timeout(tcp_socket, destination: buffer, limit, milliseconds) -> int`
- `tcp_write_timeout(tcp_socket, contents: buffer, milliseconds) -> int`
- `monotonic_millis() -> int`

Native names prepend `bhasa_rt_`, with `_str` for static-string connect. Timed
connect accepts numeric IPv4/IPv6 only so DNS cannot exceed the timeout. Existing
connect retains hostname resolution. Milliseconds must be 0..2147483647. Zero
makes an immediate nonblocking attempt. Expiry returns positive `ETIMEDOUT` or
an owned socket error handle with that status. Reads preserve destination bytes
and metadata on failure. Writes may already have sent a prefix when they fail.
Use a monotonic end time across several calls to bound a complete protocol.

All runtime-created sockets/listeners now remain O_NONBLOCK. Legacy operations
preserve blocking behavior by waiting with poll without a deadline. Timed calls
use one monotonic absolute deadline and recompute waits after EINTR or readiness
races. Socket creation and descriptor configuration also stop retrying EINTR
when the deadline expires. Descriptor clones share socket state but do not toggle O_NONBLOCK around
operations. The existing native fixture's old blocking-flag assertion was updated
to assert the new invariant. Socket flags remain close-on-exec.

Text builtins use strict Unicode scalar UTF-8:

- `utf8_valid(buffer) -> bool`, native result int64_t.
- `utf8_count(buffer) -> int`, -1 on invalid UTF-8/descriptors.
- `utf8_get(buffer, scalar_index) -> int`, -1 on invalid input or range.
- `utf8_slice(buffer, start, count) -> buffer`, owned byte copy; malformed input
  and invalid scalar ranges stop with a named runtime error, like core slices.
- `utf8_append(buffer, scalar) -> int`, zero or EINVAL. Invalid scalars leave the
  destination unchanged. This appends valid scalar bytes without rescanning or
  repairing existing bytes. Allocation failure follows core runtime rules.

Surrogates, overlong encodings, stray continuation bytes, truncated sequences
and values above U+10FFFF are rejected. NUL and Unicode noncharacters are valid
scalars. Indices count scalars, not grapheme clusters or display cells. There is
no normalization, collation or case folding.

## Bhasa library contract

`std/text.bh` defines `Text { bytes: buffer }`, `text(str) -> Text`, and methods
valid, count, codepoint, part, equals, append, append_codepoint. Public byte edits
can invalidate UTF-8; valid/count/codepoint report that and part checks before
slicing. Root's result.bh owns checked text_decode and concrete Result/Option enums.

`std/collections.bh` imports result.bh and supplies StringList, StringIntMap and
StringMap. Factories are string_list(), string_int_map(), string_map(). Lists
pack byte strings in a buffer with integer offsets. Maps use linear key lookup
and preserve insertion order; replacement retains the existing position. There
are no generic containers or hashing yet. get returns owned Text for a list and
OptionInt/OptionText for maps. find returns -1 when absent. value_at addresses a
map's insertion-order index. These are independent-copy owned records.

Use factories to establish packed-layout invariants. Public mutation of fields
can invalidate them; valid checks collection shape, and core checked accesses
prevent unsafe native memory access. Text content validity remains a separate
check. The reference frontend compiles and runs the library methods and optional
results in debug and release builds.

## Focused verification

- net.c and text.c pass strict C11 compilation with warnings denied.
- The UTF-8 fixture passes AddressSanitizer and UndefinedBehaviorSanitizer at
  O0 and O2. It round-trips every Unicode scalar, rejects malformed encodings,
  checks owned slices and finishes with zero tracked allocations. Invalid slice
  inputs exit with the expected named errors.
- The deadline fixture passes both sanitizers at O0 and O2. It checks zero and
  positive timeouts, invalid bounds, buffer preservation, interrupted setup and waits,
  partial progress, backpressure, IPv4/IPv6 loopback and concurrent descriptor
  clone races. It finishes with zero tracked allocations and no descriptor
  growth. Hosts without IPv6 loopback emit an explicit skip for that case only.
- Compiled deadline examples pass with the reference frontend in debug and
  release builds, including both connect overloads and owned error handles.
- Text and collection examples pass with the reference frontend in debug and
  release builds. They check copy independence, list resizing, map replacement,
  missing keys and invalid UTF-8 results.
- Existing native TCP tests and compiled examples still pass with permanently
  nonblocking descriptors.

The focused command used `CARGO_BUILD_JOBS=2 cargo test --offline --locked
--test network_runtime --test network_deadlines --test text_runtime --
--test-threads=1`. After adding compiled deadline and IPv6 cases, the two deadline
tests were rerun successfully. The current files contain six focused tests across
these suites. The deadline and existing network suites also passed after adding
bounded setup retries. Tests bind ephemeral loopback ports only. The OS sandbox denies
those binds, so these runs used approved local-network execution permissions.

These checks ran on macOS ARM64. Linux branches still need a Linux run. They do
not establish TLS, asynchronous I/O, Unicode grapheme support, generic containers
or production readiness. Combined Bhasa frontend validation remains part of the
integration report.
