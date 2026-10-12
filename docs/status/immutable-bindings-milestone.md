# Immutable local bindings

Prepared follow-up to typed errors under the user's full-backlog authorization.
Workers prepare ignored drafts while error sources remain frozen; apply only
when that increment is verified and committed. Preserve both frontends, caches,
Rust host, `.bh` and exactly lowercase bhasa.toml.

## Contract

`let name[:Type] = expression` declares a local binding that cannot be reassigned.
It supports `let value = try result` and inferred task handles. The initializer,
copy ownership, lexical shadowing, cleanup and task consumption retain existing
rules. `var`, parameters, loop binders and match binders retain current behavior.

This is binding immutability. Record fields and container contents remain mutable;
mutating methods and borrowed calls stay allowed. Read-only borrows, deep
immutability, compile-time constants and type aliases remain separate work and
must not be marked complete by this increment. Network endpoint/session state
retains its existing shared-state semantics.

`let` is contextual at statement start followed immediately in the token stream
by an identifier. Calls such as `let(value)`, generic calls `let[T](value)`, member
access, assignments to an ordinary variable named let, and uses of that variable
remain valid. Do not add a reserved keyword or change scanner/FFI token codes.

AST Var and TryVar receive a `reassignable: bool` field. Rust parsing/checking
and code generation, native declaration/assignment checking, host/module/generic
lowering and editor metadata must preserve it. Rust scope and codegen local state
must enforce it even with corrupted checked AST. Native local mutability stays
separate from ownership, parameter and task-worker metadata. Private source
identity, generic names and original spans remain accurate.

## Ownership and acceptance

packages owns Rust AST/parser/checker/emitter and focused reference tests in an
ignored draft. frontend_parity owns matching native emitter behavior and native
parity tests in a separate ignored draft. rust_cache owns independent read-only
acceptance/review. Root owns host/editor/formatter integration, docs/example,
combined gates, release/self-build/cache evidence and progress commits.

Require scalar/aggregate reassignment rejection, mutable fields/contents,
independent copies, lexical shadowing, ordinary let identifiers, generic/imported
names, initializer-before-binding identity, typed task consumption, try success
and error cleanup, corrupt checked AST rejection, O0/O2 sanitizer/allocation
checks and unchanged runtime ABI. Coordinator alone runs Cargo with two build
jobs and one test thread. Workers do not change frozen tracked sources or Git.
