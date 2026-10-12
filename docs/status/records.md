# Records lane

Complete. Records are implemented and verified. Files owned by this lane: `src/ast.rs`, `src/lexer.rs`, `src/parser.rs`, `src/sema.rs`, `src/codegen.rs`, `tests/records.rs`, `examples/records/`, `docs/records.md`. The package worker released `src/project.rs`; this lane made minimal loader changes to preserve record declarations and remap type IDs. No commits or resets.

## API and integration

- `Type::Record(usize)` keeps `Copy` and is owned. IDs index `Program.type_names`.
- `Program` derives `Default` and adds `records: Vec<RecordDecl>` and `type_names: Vec<String>`.
- `RecordDecl { name, fields, span }`, `RecordField { name, ty, span }`.
- `ExprKind::Field { base: Box<Expr>, name: String }`.
- `StmtKind::FieldAssign { target: Expr, value: Expr }`.
- Constructors remain `ExprKind::Call { name, args }`.
- `Program::type_name(Type)` resolves display names; `record(id)` resolves a declaration; `merge(Program)` interns and remaps module-local IDs throughout declarations, annotations and checked expressions.
- Loader now holds a full `Program` and calls `merge` for each parsed file. Imports, lowercase `bhasa.toml`, file/span diagnostics and `parser::parse_tokens` remain supported.

Root-owned hybrid scanner additions: Rust tokens `Struct` and `Dot`; Bhasa keyword code `313` and punctuation code `46`. Root has said these mappings are being applied. Needed exact mapping: `(313, "struct") => Some(Struct)`, `(46, ".") => Some(Dot)` in `fixed_token`, `if same(word, "struct") { return 313 }` in `bh_keyword`, ASCII 46 accepted in `bh_symbol`. Record syntax is not claimed for the experimental Bhasa parser/emitter. Formatter should avoid spaces around Dot.

Root requested TCP ownership integration, now implemented. `Type::TcpListener` and `Type::TcpSocket` are owned pointers named `tcp_listener` and `tcp_socket`; parser supports annotations and record fields. Cloning/freeing uses `bhasa_rt_tcp_listener_clone/free` and `bhasa_rt_tcp_socket_clone/free`. Codegen emits core and net runtime markers for TCP builtins and any ownership helper, including nested record fields. Root owns builtin signatures and driver runtime linkage. TCP copies duplicate descriptor ownership and share the underlying endpoint.

## Behavior

`struct Point { x: int; y: int }`, `Point(1, 2)`, `p.x` and `p.x = 3` work with typed function parameters/returns and nested records. Existing records/buffers/ints are deeply copied on binding, assignment, construction and returned borrows. Constructors acquire arguments left to right before evaluating the next argument. Function record parameters borrow mutable storage and cannot be rebound. Temporary projections acquire owned fields before destroying their parent. Field assignment acquires RHS before resolving the destination, so a RHS call may safely replace an enclosing record.

LLVM uses named record layouts and private allocation/clone/drop helpers. Allocations check null and trap. Fields and records clean up deterministically on reassignment, scope exit, early returns and loop iterations. Empty records allocate one byte. Wasm32 uses the 32-bit malloc size ABI; supported native targets use 64-bit size_t. Pure scalar-field records need libc allocation but no core runtime. Extern malloc/free collisions are rejected when record helpers need those symbols.

Checker rejects duplicate types/fields, constructor count/type errors, unknown fields/types, nominal type mismatches, void/task fields, recursive layouts and nesting beyond 128 levels. It rejects overlapping record/descendant borrows across call arguments, including later argument calls that could replace an earlier borrowed field. Disjoint fields and constructor copies remain permitted. This check is conservative, not a general borrow checker. Record fields must be assigned through a variable/parameter root.

The initial records phase omitted methods. The methods milestone below now implements and verifies them. Generics, inheritance, visibility and record equality remain unsupported.

## Verification

Passed on this host, Apple Clang 21 on arm64 macOS:

- `cargo check`.
- `cargo test --lib`: 121 passed after syntax/module integration. Other lanes have added more library tests since this run.
- `cargo test --test records -- --nocapture`: all 18 passed. Native scalar records run in debug/release. Nested-resource tests use ASan/UBSan at `-O0` and `-O2`, with `sanitize_address` on every generated LLVM function so its record accesses are instrumented. Test-only allocation wrappers count record handles and runtime containers, asserting zero live allocations and clean stderr.
- `cargo test --test owned --test concurrency --test language --test projects`: 30 passed, including existing ownership sanitizer tests.
- Both `examples/records/points.bh` and `examples/records/resources.bh` ran with exactly the output documented in `docs/records.md`.

The record tests cover constructor count/type errors, duplicate/reserved names, unknown types/fields, cyclic and over-128-level layouts, borrowed mutation/rebinding rejection, nested copies/returns, temporary projections, self-assignment, early returns, loops, overlapping function/builtin argument invalidation, TCP field typing, imported type merging/deduplication, and actual source-file diagnostic spans.

`cargo test --lib codegen::tests -- --nocapture` passed all 13 tests after test-only ASan instrumentation. This includes constructor evaluation order, RHS replacement of an assignment parent, empty layouts, allocator collision diagnostics, malformed checked-layout limits, and TCP helper/runtime markers. Installed Clang lacks a wasm32 backend, so native tests ran while wasm32 object verification explicitly skipped; i32 allocator ABI IR assertions passed. Independent adversarial review found no concrete ownership/type defect: 14 alias probes and 15 invalid type/field/constructor/return probes were rejected; an additional native stress program with 1000 loop iterations passed instrumented ASan/UBSan at both optimization levels. Temporary compile failures caused by concurrent LSP file edits have cleared. `rustfmt --check` passes for every lane-owned Rust file plus the minimal project.rs merge edits. There are no outstanding record implementation blockers. Root should run hybrid scanner parity/integration after its scanner patches; this lane did not edit root-owned scanner/formatter/driver files.

## Follow-up: argument evaluation

Root requested inspection of acceptance lane examples `consume(b, len(b.text))` and `consume(b, Wrap(b))`. Implemented a precise relaxation in sema: final borrowed arguments still reject equal/ancestor/descendant overlaps; borrows used only during evaluation of a later argument are checked for potential field replacement instead. A non-constructor call with a record argument can invalidate only strict descendants of that argument, because borrowed record parameters cannot rebind their own storage. Constructors copy their direct inputs; traversal still checks all nested argument calls for mutation. This also safely permits `consume(b, mutate(b))` while still rejecting `consume(b.child.text, replace(b))`.

Complete. Read-only independent reasoning review found no concrete safety issue under current independent-copy, no-global-reference, and no-parameter-rebind rules. With `CARGO_BUILD_JOBS=2` and `--test-threads=1`, both `later_argument` tests passed, the direct overlapping mutable record argument test passed, and the updated allocation-counted/instrumented native ASan/UBSan test passed at `-O0` and `-O2`. Formatting checks passed. No broad suite was run; root owns final combined verification. There are now 19 record tests; the earlier full 18/13 evidence predates this semantic refinement, and the four focused follow-up cases verify this change. Acceptance lane safe-call findings are resolved.

Focused follow-up static tests passed. The first sanitizer rerun exposed a test-wrapper integration issue after core.c started defining `_DARWIN_C_SOURCE`: the record allocation wrapper had already included system headers under only `_POSIX_C_SOURCE`, causing Apple's signal.h to fail with undeclared NSIG when core.c later enabled Darwin declarations. Fixed this lane's wrapper by defining `_DARWIN_C_SOURCE` on Apple BEFORE any header. `tests/owned.rs` has the same wrapper ordering and needs the same three-line Apple guard before its first include; that file is outside this lane's ownership. The runtime source itself needs no change. The focused rerun passed, including both sanitizer optimization levels with zero live allocations and clean program stderr. No test remains running in this lane.

# Methods milestone: interface handoff

SOURCE READY / FROZEN. This lane owns AST/parser/sema/codegen plus method tests/examples/docs. The one internal helper completed AST/parser and a read-only safety review. All verification stayed focused with `CARGO_BUILD_JOBS=2` and `--test-threads=1`. No test process remains running.

Chosen AST contract for tooling and self-host parity:

- `RecordDecl` adds `methods: Vec<Function>`. Functions retain source method names and source spans. The first `Param` is `self` with inferred `Type::Record(enclosing_id)`; remaining params keep their explicit types. All methods have bodies.
- Raw parse AST adds `ExprKind::MethodCall { receiver: Box<Expr>, name: String, args: Vec<Expr> }`. Dot without parentheses remains `Field`. No new lexer tokens.
- New `ast::method_key(record_name, method_name) -> String` returns `Record.method`. The dot keeps keys distinct from every source global, constructor, runtime and extern identifier. Different records may share method names; method names can match global/builtin names.
- Semantic checking lowers each method call into existing `ExprKind::Call` with the qualified key and receiver prepended to args. Receiver appears exactly once. Checked method declarations remain in `record.methods`, with bodies typed/lowered. `CheckedProgram.signatures` includes the qualified method keys. Global functions stay in `Program.functions`.
- Codegen emits method declarations from record.methods under `@bhasa.fn.Record.method`, preserving existing ordinary function borrow/return/temporary cleanup paths. It does not export a source-callable synthesized global.
- `Program::merge` remaps method parameters/returns/bodies and raw method receivers/arguments along with existing record fields and global functions.

Worker safety fix implemented: collect valid spawn sites during checking, then build the call graph AFTER every global and method body has been typed/lowered. Method graph nodes use the same qualified keys. Validate pending worker graphs against extern reachability only then, including recursive method cycles. Direct `spawn(obj.method(...))` remains rejected under the scalar worker ABI.

Tooling can inspect methods from the raw parser AST using `RecordDecl.methods` and `MethodCall`; checked bodies contain lowered calls. Root received the contract before dependent edits. No src/project.rs edit was necessary because Program::merge owns method remapping.

## Methods verification and handoff

- `CARGO_BUILD_JOBS=2 cargo test --lib method -- --test-threads=1`: 8 method AST/parser tests passed.
- `CARGO_BUILD_JOBS=2 cargo test --test methods -- --test-threads=1`: 7 tests passed. Covers method/global/builtin namespaces, malformed declarations, receivers, missing returns, types/arity, field/method collisions, transitive/recursive/imported worker-to-extern reachability in both declaration orders, alias rules and imported receiver/parameter/return type IDs.
- The method ownership test instruments every generated LLVM function with `sanitize_address` and compiles runtime C with ASan/UBSan at O0/O2. Both passed with zero live record/container allocations and clean program stderr. Covers exactly-once receiver evaluation, nested and temporary receivers, borrowed buffer/record returns, clones, mutation, RHS replacing an assignment parent, early returns and loops.
- `CARGO_BUILD_JOBS=2 cargo test --test concurrency workers_reject_foreign_calls_through_recursive_call_graphs -- --test-threads=1`: existing worker regression passed.
- Both `examples/records/methods.bh` and `examples/records/methods-resources.bh` ran with the documented output. `docs/records.md` now documents methods and limits.
- `rustfmt --check` passed for ast.rs, parser.rs, sema.rs, codegen.rs and tests/methods.rs.

Review found and fixed one malformed-CheckedProgram emission gap: changing a spawn target to a qualified method name could previously place a method pointer into the scalar worker ABI. Codegen now verifies the target belongs to Program.functions, is non-extern, and has exactly int -> int signature before emitting spawn. A dedicated regression passes. No source-level receiver/alias/worker-safety defect remains known.

Methods remain synchronous mutable borrows. Static/extern methods, overloading, inheritance, dynamic dispatch and method values are rejected. The checked AST retains source method declarations and lowers only call expressions. Root owns Bhasa parity, tooling integration, combined verification and release rebuilds. This lane is frozen pending concrete integration feedback.
