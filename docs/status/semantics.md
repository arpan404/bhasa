# Semantics status

Implemented and verified. No integration blockers remain.

Changed files:

- `src/sema.rs`
- `docs/status/semantics.md`

`sema::check(program: Program) -> Result<CheckedProgram, Vec<Diagnostic>>` collects all signatures before checking bodies, so forward calls and recursion resolve. Successful checking populates every expression's `ty`, including standalone void calls. The signature map includes internal and external declarations; `print` is handled as a builtin.

The checker validates duplicate functions, parameters and locals; entry-point shape; extern signatures; scoped variable lookup and assignment; explicit local annotations; call arity and argument types; all unary and binary operators; bool conditions; return types; missing returns; unreachable statements; and invalid void values. It also rejects embedded NUL in a directly supplied string AST. Diagnostics use semantic `S` codes and the relevant source span. Independent errors are collected, including errors within unreachable statements and invalid call arguments.

Scope and name decisions:

- Parameters and the outer function body share one scope.
- Nested blocks can shadow locals. Initializers resolve before their binding is introduced, so a nested `var x = x` can read an outer `x`.
- Branch and loop scopes do not escape. Assignments can update an outer binding.
- `print` is reserved for functions, parameters and variables.
- Locals may shadow user function names, but a call resolving to a local is rejected because the milestone has no callable values. Bare function names are also rejected as values.
- Loops never establish a guaranteed return. An if/else does so only when both branches return. Statements following a guaranteed return are diagnosed, then still checked for independent errors.
- Failed initializers retain a poisoned binding to avoid misleading follow-up unknown-variable errors.

Verification performed:

- `rustfmt --edition 2021 src/sema.rs` completed successfully. No other module was formatted.
- The first `cargo test --lib sema::tests` attempt failed because `src/codegen.rs`, `src/driver.rs` and `src/parser.rs` had not arrived. It reported only those missing modules.
- Created a temporary harness under `/private/tmp/bhasa-sema.XXXXXX` using `mktemp -d`. It referenced the real shared `src/ast.rs`, `src/diagnostic.rs` and `src/sema.rs` through Rust `#[path]` attributes. Ran `rustc --edition 2021 --test "$sema_test_dir/harness.rs" -o "$sema_test_dir/sema-tests"` followed by `"$sema_test_dir/sema-tests" sema::tests`. All 26 semantic tests passed, with 3 diagnostic tests filtered out. No repository stubs were added.
- Re-ran `cargo test --lib sema::tests` after sibling modules arrived. All 26 tests passed, with 36 other library tests filtered out.
- Ran `cargo test --test language`. All 11 integration tests passed, including invalid fixtures, basic examples, extern restrictions, operator types, scope rules, calls, returns and the public wrappers.
- A separate read-only review of `src/sema.rs` against the bootstrap contract found no concrete correctness issues or integration blockers.

Tests are owned in `src/sema.rs`. They include a 117-case binary-operand type matrix, forward calls and recursion, extern signatures, main validation, scope isolation and shadowing, all void-value contexts, return flow, source spans, and recursive checks for expression annotations. Native execution and whole-project acceptance remain the integration owner's verification scope.

No changes to the shared AST or contract are required. No dependencies, branches, commits or additional chats were created.
