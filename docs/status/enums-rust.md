# Tagged enums in the Rust frontend

The Rust frontend supports named tagged enums and exhaustive `match` statements.
Run the example with `bhasa run examples/enums/main.bh`.

```bh
enum OptionInt { none; some(value: int) }

def value_or_zero(option: OptionInt) -> int {
    match option {
        case OptionInt.none { return 0 }
        case OptionInt.some(value) { return value }
    }
}

def main() { print(value_or_zero(OptionInt.some(42))) }
```

Constructors require parentheses, including `OptionInt.none()`. Zero-payload
declarations and patterns may omit parentheses. Declarations accept newline,
semicolon, or comma separators. Constructor and pattern arguments are positional.
Every variant must appear exactly once in a match, and every payload must have a
binding. Bindings are local to their case block. A match is a statement; if every
case returns, it satisfies the enclosing function's return requirement.

Payloads may contain `int`, `bool`, `str`, `buffer`, `ints`, `tcp_listener`,
`tcp_socket`, records, and other enums. Type names are nominal and share the
record/module type namespace. Payloads cannot contain `void` or linear `task`
handles. Empty enums, duplicate variants or payload names, recursive schemas, and
aggregate schema paths deeper than 128 declarations are rejected. The depth check
includes paths that alternate between records and enums.

An enum owns its active payload. Copying an enum recursively copies owned data;
TCP handles follow the runtime's existing descriptor-duplication semantics.
Constructor arguments are copied as they are evaluated, before a later argument
can replace a field used by an earlier argument. Matching evaluates its scrutinee
once and creates independent owned payload bindings. Mutating a bound record or
buffer, or replacing the original enum inside an arm, cannot invalidate the other.
Case exit, return, reassignment, and function exit free the corresponding owned
values. No collector is involved.

The LLVM representation uses a private tag and active-payload pointer. Source
programs cannot read or write a tag or project unchecked payload fields. Imported
C functions retain their scalar ABI; enum values cannot cross that ABI directly.
Worker call-graph checks traverse match scrutinees and all case blocks, including
transitive calls through methods. Task handles must still be joined in the lexical
block that created them.

This milestone uses concrete named types to express optional values and results.
It does not add generic `Option<T>`/`Result<T, E>`, recursive enums, enum methods,
wildcards, guards, match expressions, implicit error propagation, or enum equality.

Verification is in `tests/enums.rs`: semantic rejection cases, nominal typing,
return analysis, schema depth, native execution at O0 and O2, TCP payloads, worker
execution, and LLVM-instrumented AddressSanitizer runs with runtime Undefined
Behavior Sanitizer and allocation accounting.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test enums -- --test-threads=1
```
