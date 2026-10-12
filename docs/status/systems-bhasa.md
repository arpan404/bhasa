# Native integer operations and stepped ranges

The Bhasa scanner and emitter implement the systems/iteration contract in
`systems-tooling-milestone.md`.

Integer `&`, `|`, `^`, `~`, `<<` and `>>` use signed 64-bit values. Shift counts
outside 0 through 63 trap before LLVM executes a shift. Left shifts wrap bits;
right shifts preserve the sign. Existing arithmetic still checks overflow.
Bitwise precedence follows the agreed Python-style ordering above comparisons.
Existing token codes are preserved: one-byte operators use ASCII codes and
shifts use 263/264.

Hexadecimal and binary literals retain token kind 2 and their source spelling.
Both cases of `0x` and `0b` are accepted. Malformed digits, prefixes, separators
and suffixes produce L002 over the complete alphanumeric/underscore spelling.
Positive values must fit signed i64. Only direct unary negation admits the
magnitude 2^63. The emitter checks the radix magnitude before accumulating a
negative value, so compiling the minimum signed integer cannot itself overflow.
LLVM receives normalized decimal constants. Existing decimal float handling is
unchanged.

`range(start,end,step)` evaluates its three integer arguments once, from left to
right. Omitted steps remain one. A zero step traps even when the range is empty.
Positive steps compare cursor < end; negative steps compare cursor > end.
Advancement uses LLVM signed-overflow detection and exits on overflow instead
of wrapping or trapping. Continue reaches advancement; the ordinary loop exit
path handles cleanup. Mutation of the visible loop binding cannot change the
cursor. List snapshots, lexical task rules and transitive worker checks retain
their existing behavior, including calls made while evaluating the step.

## Evidence

Local arm64 macOS probes used the previous verified release to build the new
native compiler in `build/systems-native`. The new compiler emitted a fixture
covering literal boundaries, operator precedence, signed/wrapping shifts, eager
operand order, descending and stepped ranges, cursor overflow, argument
evaluation order, iteration-binding mutation, tasks and owned cleanup on
continue/break/return.

The fixture passed O0 and O2 with generated-LLVM ASan, runtime ASan/UBSan and
atomic allocation tracking. All tracked allocations were released. Twenty-six
invalid source cases were rejected, including BH209 for a worker whose step
calls an extern function. Six trap cases failed as required at both O0 and O2.
The standalone scanner helper's exhaustive byte-pair and precedence checks
passed. A/B/C rebuilt compiler LLVM was byte-identical.

Local evidence: `build/systems-native/probe.log`, `invalid.log`, `traps.log`,
`helpers.log`, `stage-b.ll` and `stage-c.ll`.

`tests/systems_bhasa.rs` adds reference/native and optional hybrid acceptance,
sanitizer/allocation checks, rejection/trap checks and fresh self rebuilding.
`tests/selfhost_lexer.rs` covers the new token kinds and radix spelling/span
parity. Root coordinates their Cargo runs and the refreshed hybrid archive;
these local probes do not establish those integration results.

This increment adds neither unsigned/fixed-width types nor compound assignment,
numeric separators, octal notation or first-class/custom iterators.

## Combined acceptance

The coordinator verified 433 reference and 445 complete full-hybrid tests,
80 editor tests, both Clippy configurations and formatting, refreshed releases,
A/B/C at run.xQ1qwOVL and unchanged native/Rust/hybrid cache reuse.
Final archive key: e1056ba67083f8c895adfeacf64eff9ff00162057017d1ae7f9ed44432dfcc81.
See integration.md for logs, review repairs and actual platform limitations.
The focused-only statements above describe earlier lane evidence.
