# Tagged enums in the Bhasa frontend

The Bhasa-written frontend supports named enums and exhaustive statement
matches with the same syntax and ownership rules as the Rust frontend.
The standalone seed version is 0.6. The host frontend ABI remains v2.

```bh
enum OptionInt { none; some(value: int) }

def value_or_zero(option: OptionInt) -> int {
    match option {
        case OptionInt.none { return 0 }
        case OptionInt.some(value) { return value }
    }
}
```

Constructors require parentheses, including `OptionInt.none()`. Empty variant
declarations and patterns may omit parentheses. A match must list every variant
exactly once and bind every payload. Bindings belong to their case block. If all
cases return, the match satisfies the enclosing function's return requirement.

Payloads support integers, booleans, strings, buffers, integer arrays, records,
other enums, and TCP listener/socket handles. Enums cannot contain `void` or
linear task handles. Aggregate names share a nominal namespace. Recursive
schemas and aggregate paths deeper than 128 declarations are rejected, including
paths that alternate between records and enums.

The emitter acquires constructor arguments from left to right, before later
arguments can replace an earlier argument's source. A match evaluates its value
once, owns a snapshot, and independently copies its bound payloads. Reassignment,
case exit, function return, and discarded temporaries use deterministic cleanup.
TCP copies retain the runtime's existing descriptor-duplication behavior. No
collector is used.

The current LLVM layout has a private integer tag followed by disjoint slots
for all variants. Only the active payload slots are initialized, cloned, or
freed. This differs from the Rust frontend's private tag/payload-pointer layout;
enum layouts are internal and cannot cross the scalar C ABI. Source programs
cannot read or write the tag or access payload fields outside a match.

Keyword codes 315, 316, and 317 represent `enum`, `match`, and `case`. The
zero-width source-file boundary token remains 314. BH215 reports invalid enum
constructors or patterns, BH216 reports missing or duplicate cases, and BH213
continues to report recursive or overly deep aggregate schemas. Match wrappers
do not consume an extra statement nesting level. The CLI accepts 128 nested
empty match cases and rejects 129. Rust references at this boundary run through
the CLI because the debug library's test-thread stack is smaller.

Worker call-graph checking visits match bodies. Task handles still require a
join in their creating lexical block. Enum support preserves method resolution,
record alias checks, and source-file declaration boundaries.

The emitter also recognizes the UTF-8 scalar builtins and TCP deadline builtins.
UTF-8 calls request the text runtime, and monotonic-clock calls request the net
runtime. Status-returning append/read/write calls must be used. Runtime behavior
and standard-library wrappers are maintained separately in `runtime/text.c`,
`runtime/net.c`, and `std`.

Verification in `tests/selfhost_enums.rs` covers valid and invalid schemas,
constructor and pattern typing, exhaustiveness, scope and return analysis,
recursive/deep layouts, task/worker restrictions, and file-boundary rejection.
The ownership fixture rebuilds A into B and B into C, compares their complete
compiler LLVM, and compares Rust and Bhasa native output at O0 and O2. Every
generated LLVM definition receives AddressSanitizer instrumentation. The C
runtimes use AddressSanitizer and UndefinedBehaviorSanitizer, with an allocation
counter requiring zero live allocations at exit. The fixture also exercises
UTF-8 validation/count/index/slice/append and deadline error handles.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test selfhost_enums -- --test-threads=1
BHASA_BOOTSTRAP="$PWD/target/debug/bhasa" sh bootstrap/compiler/check.sh
```

The shell gate passed in `build/selfhost/run.sv3dGKDk`, including identical B/C
LLVM and native enum, record, method, task, foreign-call, TCP-handle, and
GPU-unavailable fixtures. Its only later emitter change adds the text-runtime
marker; the focused sanitizer rebuild test covers that change.

This milestone provides concrete named optional/result types. Generics,
recursive enums, enum methods, wildcard/guard patterns, match expressions,
implicit error propagation, and enum equality remain unsupported.
