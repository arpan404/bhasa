# Bhasa frontend iteration

The Bhasa-written scanner and emitter implement keyword codes 319 `for`, 320 `in`, 321 `break`, and 322 `continue`. Code 318 remains `float`. The Rust/native scanner adapter uses the same mapping; the original punctuation-only bootstrap lexer exposes the corresponding code helpers.

Supported forms are `for name in range(end)`, `for name in range(start,end)`, and `for name in list_expression`, followed by a block. Range bounds are signed integers, evaluated once from left to right. Iteration is ascending and excludes the end bound; reversed or empty ranges perform no iterations. There is no step argument or first-class range value. Parentheses around a range call are transparent, and a trailing argument comma is accepted consistently with ordinary calls.

A List source is evaluated once and acquired as an owned snapshot. A hidden local keeps that snapshot alive across `continue` and releases it on normal completion, `break`, or `return`. Each element comes from the existing typed copy-out `get` helper, so owned elements are independent snapshots. The body may mutate or replace the source list without changing the remaining iteration. The visible binding is an ordinary mutable local in the body's scope; changing it does not modify the hidden iteration index.

Loop contexts store the nearest lexical exit and continuation targets and the local boundary retained by an exit. `break` and `continue` clean up locals beyond that boundary, reject outstanding tasks in those scopes, and branch to the matching target. An outer task that remains in scope does not prevent `break`; the existing same-scope join restriction remains. Return cleanup also releases hidden iterator and match snapshots. Conditional and match branches propagate terminal control flow, so a statement after an unconditional exit is rejected. A loop never proves that a function returns.

The existing worker call graph records calls in bounds, iterable expressions, and bodies. Collection element restrictions, aggregate nesting limits, and source-file boundaries remain checked. Internal range increments cannot overflow: a body executes only while the index is less than its signed integer end bound.

`tests/iteration_bhasa.rs` checks reference/native acceptance and execution, signed endpoints, bound evaluation order, mutation and shadowing, owned cleanup through nested exits, outstanding tasks, worker restrictions, and a standalone compiler self-rebuild. Native executions use O0/O2 with AddressSanitizer and UndefinedBehaviorSanitizer. A feature-gated test covers the hybrid scanner/compiler. The focused reference run passed `fresh_bhasa_seed_matches_iteration_cleanup_rejection_and_self_rebuild`, including native O0/O2 sanitizer execution and identical LLVM from the standalone self-rebuild. Evidence: `build/verify-iteration-editor-native.log`. The hybrid test still requires a rebuilt native frontend archive before it can run. Direct native sanitizer smoke and the complete iteration program also passed for list snapshots, range endpoints, mutable bindings, tasks, and nested loop exits.

Final integration: both iteration_bhasa groups pass in the full hybrid suite;
398 reference / 408 hybrid tests pass. A/B/C passes at run.0i9MfkYr with the
new range example. Both refreshed releases execute ranges and typed List loops.
Archive b2ae9f5fb380dcf392025873346826116006fdc7e0a2ceddcabbd314e0f812d1
matches the frozen sources and is reused by two unchanged builds.
