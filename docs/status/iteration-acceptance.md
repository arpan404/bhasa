# Iteration acceptance

`tests/iteration.rs` exercises the iteration contract through the CLI with the
Rust frontend and, when enabled, the Bhasa frontend. It uses the same source and
expected results for both. Native tests require a working Clang installation;
only an absent executable permits a skip.

The native ownership fixture checks ascending, exclusive integer ranges,
including empty and reversed bounds and values near both signed 64-bit limits.
A mutable counter records that bounds execute once, from left to right. Assigning
to the loop binding must not change the hidden cursor. Nested `for` and `while`
loops exercise the nearest-loop behavior of `break` and `continue`.

List iteration checks a single source evaluation, independent element values,
mutation and replacement of the original collection, and temporary and empty
lists. Payloads include floats, buffers, records, enums and nested lists.
Repeated nested `match` and `if` paths exit through `continue`, `break` and
`return`. The test instruments emitted LLVM and the core/collection C runtime
with AddressSanitizer and UndefinedBehaviorSanitizer at O0 and O2. A counted
allocator requires every tracked allocation to be released at normal process
exit. Subprocess deadlines bound hangs caused by incorrect iteration.

Separate checks reject invalid range arity/types, unsupported iterable types,
first-class range use, bindings outside their scope, exits outside loops and
unreachable statements. A loop alone cannot establish a guaranteed function
return. Task cases require joining or awaiting every task in an exited scope,
retain outer tasks across inner loop exits, and preserve the existing same-block
consumption rule. Worker checks require the specific safety diagnostic for
unsafe calls in bounds, list-producing helpers and bodies, including methods and
generic specializations.

An imported generic fixture checks aliased factories, nested owned returns and a
loop binding that shadows the import alias only inside its body. Private fields
and methods remain inaccessible through loop bindings and bound expressions.

These tests do not add descending ranges, steps, first-class iterators,
iteration over maps, borrowed elements or parallel loops. They do not establish
cross-target runtime support or a general Rust-style ownership proof.

## Validation status

All five groups pass in the complete reference and full-hybrid suites. Both
host frontends execute the ownership fixture at O0/O2 with ASan/UBSan and counted
allocation cleanup. The full suites pass 398/408 tests; evidence is in
build/verify-iteration-reference.log and hybrid-frontend.log. Root verified
refreshed release execution and cache reuse afterward.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test iteration -- --test-threads=1
```
