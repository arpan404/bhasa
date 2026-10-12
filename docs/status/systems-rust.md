# Integer operations and stepped ranges in the reference compiler

The Rust frontend accepts integer `&`, `|`, `^`, unary `~`, `<<` and `>>`.
Boolean logical operators keep their existing short-circuit behavior; integer
bitwise operands evaluate eagerly from left to right. The precedence contract
is recorded in [the milestone](systems-tooling-milestone.md). Operands must be
`int`, with no implicit float or bool conversions.

Shift counts are checked against 0 through 63 before the LLVM shift executes.
Left shift wraps the 64-bit pattern without LLVM overflow flags. Right shift
uses arithmetic sign extension. Ordinary integer arithmetic still traps on
overflow.

The lexer retains hexadecimal and binary spelling in integer tokens, accepts
both cases of `0x` and `0b`, and reports a malformed prefixed alphanumeric or
underscore sequence as one diagnostic. Separators and suffixes remain invalid.
The parser applies the same signed-i64 limits as decimal, including a directly
negated magnitude of 2^63. Positive high-bit patterns are not implicitly negative.

`ForSource::Range` now has an optional step expression. One- and two-argument
forms retain a step of one. An explicit step evaluates once after start and end,
then zero is rejected before the initial loop condition. Positive steps use an
exclusive upper bound; negative steps use an exclusive lower bound. Overflow in
the hidden cursor's checked addition branches to loop exit. The body binding
has independent storage, and `continue` still goes through cursor advancement.
Existing cleanup, lexical task rules and worker restrictions remain active.

All owned AST and semantic visitors traverse the step, including type-ID
remapping, enum normalization and the resolved worker call graph. No C runtime
API changes are required for these operators or ranges.

## Verification

The coordinator's compile-only `cargo check --all-targets` passed with the AST,
reference lowering and current host/editor integration. The reference focused
gate passed 142 library unit tests, four integer-operation tests, five stepped
range tests, five existing iteration tests and three independent integration
tests. Evidence is in `build/verify-systems-check.log` and
`build/verify-systems-reference-focused.log`.

The new tests cover radix token spans, signed literal boundaries,
precedence, type rejection and checked-AST validation; native shift/trap behavior
at O0/O2; a signed-range reference oracle covering overflow in both directions;
zero-step traps; step evaluation order; mutable bindings; and loop ownership
with typed tasks. Ownership execution uses ASan/UBSan and atomic allocation
accounting. The independent integration checks also passed step-call worker
safety, member privacy, original-file diagnostic mapping and formatting. This
report does not claim combined frontend or release acceptance.

Owned changes: `src/ast.rs`, `src/lexer.rs`, `src/parser.rs`, `src/sema.rs`,
`src/codegen.rs`, `tests/integer_bits_reference.rs`,
`tests/stepped_ranges_reference.rs`, and this report.

## Combined acceptance

The coordinator verified 433 reference and 445 complete full-hybrid tests,
80 editor tests, both Clippy configurations and formatting, refreshed releases,
A/B/C at run.xQ1qwOVL and unchanged native/Rust/hybrid cache reuse.
Final archive key: e1056ba67083f8c895adfeacf64eff9ff00162057017d1ae7f9ed44432dfcc81.
See integration.md for logs, review repairs and actual platform limitations.
The focused-only statements above describe earlier lane evidence.
