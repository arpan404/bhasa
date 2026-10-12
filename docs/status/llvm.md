# LLVM backend status

Implemented and verified. No shared AST changes or integration repairs are required.

Changed files:

- `src/codegen.rs`
- `docs/status/llvm.md`

`codegen::emit_llvm(&CheckedProgram, &str) -> Result<String, Vec<Diagnostic>>` emits a complete textual LLVM module using opaque pointers. It uses only the Rust standard library. It does not invoke Clang, download tools, or choose a different target.

The emitter implements function definitions and forward calls, recursion, parameters, mutable locals, nested scopes, conditionals, loops, scalar returns, static UTF-8 strings, and builtin printing. Local allocations appear in the entry block, including declarations inside loops. Internal functions use `bhasa.fn.` symbols; the dot cannot occur in source import names. Trusted imports preserve their C symbols and accept only int parameters and int or void results. A Bhasa main gets a separate C-compatible `i32 @main()` wrapper; an int result is truncated to i32. Libraries without main have no entry wrapper.

Addition, subtraction, multiplication and negation use signed overflow intrinsics followed by a trap branch. Division and remainder guard both zero divisors and `INT64_MIN / -1` before the operation. Short-circuit operators evaluate the right operand only on the required branch. PHI inputs use the actual predecessor blocks after nested boolean expressions or overflow checks.

Printing uses fixed `%lld\n` or `%s\n` formats. User strings never become format strings. UTF-8 constants use byte counts and escaped bytes plus a trailing NUL. Bool values print as `true` or `false`. Runtime printing declarations and constants are emitted only when used.

Malformed checked inputs return C diagnostics for missing or inconsistent signatures, invalid declarations, missing expression types, incorrect type tags, bad calls, invalid locals and assignments, invalid control conditions, missing returns, unreachable statements, void values, embedded NULs, and invalid target text.

## Runtime symbol constraint

A source `extern def printf(...)` conflicts with the C runtime function used by builtin `print`. Codegen reports a diagnostic when both occur in one module, with advice to rename the foreign adapter. A scalar source signature cannot describe printf's variadic C ABI. An imported printf without builtin printing remains allowed. A Bhasa-defined function named printf is allowed and tested because its symbol is mangled.

This restriction was added after review reproduced silently lost printing with an adapter exported as printf. Clang accepting calls through incompatible signatures does not make those calls correct. The integration owner may wish to mention this adapter-naming constraint in the interop documentation.

## Verification

Host tools were Rust 1.98.0 and Apple Clang 21.0.0. Clang reports `arm64-apple-darwin25.2.0`.

Commands run:

- `rustfmt --edition 2021 src/codegen.rs` and `rustfmt --edition 2021 --check src/codegen.rs` passed. No other source files were formatted.
- A temporary `/tmp/bhasa-codegen-harness.rs` referenced the real AST, diagnostic and codegen modules using `#[path]`. `rustc --edition 2021 --test /tmp/bhasa-codegen-harness.rs -o /tmp/bhasa-codegen-tests` compiled without sibling-module stubs. `/tmp/bhasa-codegen-tests --nocapture` passed all 12 tests at the final standalone run, including 8 backend tests and 4 diagnostic tests. The first native test run used an unsupported separated `--target` argument in the test helper; this was corrected to `-target` before the passing run.
- `cargo test --lib codegen::tests -- --nocapture` passed all 8 backend tests, with 63 unrelated tests filtered out.
- The backend tests invoke Clang with argument arrays using `-target <native-triple> -O0/-O2 -x ir <temporary.ll> -x none`, compile an optional C adapter when required, and run the native executable. Both optimization levels passed recursion, scope shadowing, loops, early returns, nested short-circuit logic, UTF-8 and empty strings, bool arguments and returns, int limits, signed quotient/remainder, scalar C ABI calls, internal libc-name collision avoidance, and the main wrapper's i64-to-i32 conversion.
- Eight arithmetic trap cases each compiled and terminated by signal at both `-O0` and `-O2`: add overflow, subtract overflow, multiply overflow, division by zero, remainder by zero, minimum int divided by -1, minimum int remainder -1, and minimum int negation.
- The pure-library test invoked `clang -target x86_64-unknown-linux-gnu -x ir -c <temporary.ll> -o <temporary.o>` successfully and checked the ELF object header. It did not run the foreign object.
- The same test attempted `clang -target wasm32-unknown-unknown -x ir -c <temporary.ll> -o <temporary.o>`. Installed Apple Clang reported that no available target supports wasm32. The test records this optional check as skipped. `clang --print-targets` confirms that this installation contains ARM/AArch64 and x86 backends, without wasm. No wasm object or wasm execution is claimed.

The standalone harness emitted only unused-item warnings caused by omitting sibling modules. The cargo backend run had no Rust warnings. A read-only review found the printf collision described above; it is fixed and covered by a regression test.

Whole-project acceptance belongs to the integration owner. No dependencies, branches, commits, or extra task chats were created. Repository edits stayed within the LLVM ownership boundary.
