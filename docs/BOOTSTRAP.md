# Bhasa bootstrap implementation contract

This is the implementation contract for the first compiler. The user authorized a Rust bootstrap compiler, then a compiler written in Bhasa, with LLVM code generation, easy cross-compilation, concise C/Python-inspired syntax, and no garbage collector in applications or their dependencies. CLI tools and backend services are the first eventual workloads. The current milestone is smaller: compile and run scalar programs correctly. No claim of complete Rust-like safety, production readiness, Go-like speed, universal Rust package compatibility, or completed self-hosting is permitted.

The source pipeline is Rust lexer/parser -> type-checked AST -> textual LLVM IR -> installed Clang LLVM backend and linker. Writing LLVM IR directly avoids a network dependency and a particular LLVM development-library installation. A bundled LLVM/LLD distribution and target SDK packs are future packaging work. Never silently substitute a target, run a cross-target executable, or report a cross object as a runnable cross executable.

## Ownership and integration

All chats share the checkout. Do not create worktrees, commits, branches, or additional chats. Do not overwrite other owners' changes. Each worker may edit only its paths below. Read shared contracts and use normal final responses plus `docs/status/<owner>.md` for findings. The integration chat reads these and owns cross-component repairs. Do not message other chats without user authorization. Do not change this contract or `src/ast.rs`; record requested changes in your status file. Unit tests belong in owned modules. Avoid whole-project formatting while others write; format only owned files.

| Owner | Paths | API / responsibility |
|---|---|---|
| integration | Cargo.toml, src/lib.rs, src/ast.rs, src/diagnostic.rs, .gitignore | Shared contracts and final integration |
| frontend | src/lexer.rs, src/parser.rs | `parser::parse(source: &str) -> Result<Program, Vec<Diagnostic>>` |
| semantics | src/sema.rs | `sema::check(program: Program) -> Result<CheckedProgram, Vec<Diagnostic>>` |
| llvm | src/codegen.rs | `codegen::emit_llvm(program: &CheckedProgram, target_triple: &str) -> Result<String, Vec<Diagnostic>>` |
| driver | src/main.rs, src/driver.rs, src/target.rs | `driver::run(args: &[String]) -> i32`; binary passes arguments without executable name |
| tests | tests/, examples/basics/ | Integration tests, successful and failing language fixtures |
| interop | examples/rust-interop/, docs/interop.md | Native Rust staticlib C ABI scalar example and reproducible verification |
| selfhost | bootstrap/, docs/self-hosting.md | Executable Bhasa compiler seed and honest staged port plan |
| docs | README.md, docs/language.md, docs/toolchain.md | Implemented subset, build instructions, supported vs proposed features |

Each worker owns its corresponding `docs/status/<owner>.md`. Other files can be read freely. Use std-only Rust; no external crates or network installation is needed. Missing sibling modules during early development are expected. Do useful independent work, then wait briefly if necessary, never replace missing sibling modules with permanent stubs.

## Language for milestone 0.1

The user confirmed `.bh` as the standard source extension. All checked-in Bhasa source files, examples, scripts, tests and documentation use `.bh`.

```bhasa
extern def native_double(value: int) -> int

def add(a: int, b: int) -> int {
    return a + b
}

def main() {
    var value = add(20, 22)
    print("Bhasa")
    print(value)
    while value > 0 {
        value = value - 1
    }
    if value == 0 {
        print(true)
    } else {
        print(false)
    }
}
```

- Braces delimit blocks, newline or `;` terminates simple statements; `}` and EOF can terminate a statement. Newlines inside parentheses are whitespace. Both `#` and `//` line comments are accepted. Identifiers are ASCII letters/underscore plus digits after the first character; non-ASCII strings are allowed. No implicit insertion across newline after `return`.
- Functions use `def`, typed parameters, optional `-> type`. Omitted return type means `void` in this bootstrap, not return inference. Calls can refer to later functions and recurse. Exactly zero or one main declaration; if present it must be a Bhasa definition with no parameters and return `void` or `int`. Library/object compilation can omit main; executable linking then reports missing entry point clearly.
- Types are signed 64-bit `int`, `bool`, immutable static UTF-8 `str`, and `void` as a return type only. `var` requires an initializer, has an optional `: type`, and permits reassignment of the same type. Strings are immutable literals/static references only, with no heap or raw pointer feature. Variables are scoped by block. Shadowing in nested scopes allowed; duplicates in same scope rejected.
- String escapes: `\n`, `\r`, `\t`, `\\`, `\"`; reject embedded NUL and unsupported escapes for this milestone. No string operators/comparisons in this milestone. `print(expr)` is a builtin accepting one int, bool, or str and appending a newline. It returns void and cannot be used as a value. Its name cannot be defined by a user.
- Binary arithmetic `+ - * / %`, comparisons `== != < <= > >=`, boolean short-circuit `and or` with aliases `&& ||`, unary `-`, `not` with alias `!`. Standard precedence, left associativity. Equality supports int/bool of identical type; ordered comparisons and arithmetic require int. Conditions require bool, with no implicit conversions.
- Signed overflow and division by zero trap in all build modes, including `INT64_MIN / -1` and remainder overflow. No LLVM poison or C signed-overflow assumptions. Negation of INT64_MIN traps. Parse the signed minimum literal correctly or issue a clear range diagnostic; never wrap a literal silently.
- Return types, function arity, argument types, variable existence, assignment types, duplicate symbols, invalid void values and missing returns are statically checked. Missing-return analysis can be conservative for loops. An if with an else returns on all paths only when both branches return. Reject unreachable statements following a guaranteed return with a clear diagnostic. Unknown features must fail clearly.
- `extern def` is an explicit trusted C ABI boundary, with int parameters and int/void results only. No bodies or overloads, pointers, structs, callbacks, unwinding, ABI guessing or auto crate import. These declarations do not certify foreign implementation safety or absence of GC. External main and print are rejected. Internal function symbols should be mangled to avoid libc/import collisions. The backend generates a C-compatible `main` wrapper returning i32. Bhasa int main return is converted to that exit code.
- Deliberately deferred: heap ownership/borrowing, collections, structs/methods, packages/imports, async/concurrency, GPU, exceptions, generics, arbitrary FFI and a compiler implemented entirely in Bhasa. Document these as future work rather than simulating support.

## Rust interfaces

Read `src/ast.rs` for exact shared data types. `Expr.ty` begins None; successful semantic checking populates every expression type, including void call expressions. `CheckedProgram.signatures` contains declared functions; `print` is special and need not be included. Codegen receives only successful checking results and should still return a Diagnostic rather than panic on malformed inputs.

Diagnostics use `Diagnostic { code: String, message: String, span: Span }`, with `Diagnostic::new(code, message, span)`. Span offsets are UTF-8 byte offsets. Integration supplies `render(&self, source, path) -> String` and `to_json(&self, source, path) -> String`. Lexical codes start L, parse P, semantic S, codegen C; no brittle dependence on specific numbering in other components. Standard output for emitted IR/JSON must remain machine-readable.

Public library wrappers will be `parse(&str)`, `check(&str) -> Result<CheckedProgram, Vec<Diagnostic>>`, `compile_to_llvm(&str, &str) -> Result<String, Vec<Diagnostic>>`.

## Driver contract

Implement `bhasa check FILE [--json]`; `bhasa emit-llvm FILE [--target TARGET] [-o FILE]`; `bhasa build FILE [-o FILE] [--target TARGET] [--release] [--emit object] [--link FILE ...]`; `bhasa run FILE [--release] [--link FILE ...]`; `bhasa targets`; `--help`; `--version`.

Use installed `clang` or explicit `BHASA_CLANG` executable path. Invoke with argument arrays, never shell strings. Clang consumes LLVM IR with `-x ir`; reset language with `-x none` before native link inputs. Use `-O0` development or `-O2` release. Temporary files must not overwrite source files, collide between processes, or destroy user-owned paths. Default output goes under build/, creates parents as needed. Do not execute build artifacts for cross targets. `check` and IR emission do not require clang. Report tool-not-found, unsupported-target and missing SDK errors with actionable text. No automatic downloads. Link only explicit user-supplied files.

Aliases: native, linux-x64 -> x86_64-unknown-linux-gnu, linux-arm64 -> aarch64-unknown-linux-gnu, windows-x64 -> x86_64-pc-windows-msvc, macos-arm64 -> arm64-apple-macosx, macos-x64 -> x86_64-apple-macosx, wasm32 -> wasm32-unknown-unknown. Accept these exact triples too; reject unknown targets for now. Builds resolve native and explicit targets by asking Clang to emit LLVM IR for empty C input, then use that module target triple for both Bhasa IR and compilation. This incorporates SDK/deployment defaults without suppressing warnings. Use Rust host cfg mapping for standalone native IR emission without clang. `targets` describes candidate code generation, not installed SDK certification. Wasm initially supports object emission for pure functions; print/host imports and runnable WASI are not promised. Cross linking depends on installed target libraries/SDK/linker. The driver must make this distinction explicit.

## Acceptance and stages

Stage 0 is a Rust bootstrap: `cargo test` plus actual native compilation/execution of hello, recursion, arithmetic, branching and loops; reject invalid programs; verify overflow traps and short-circuit effects; verify at least one foreign-target object if installed Clang supports it; verify the scalar Rust adapter if host tools allow it. Verify instructions against actual files and don't claim unrun tests.

Stage 1 grows the language enough to express a lexer/parser, owned buffers, strings, file I/O and compiler data structures. Stage 2 ports the compiler to Bhasa and builds it using stage 0. Stage 3 has the resulting compiler rebuild itself and pass the same corpus. The current selfhost assignment creates only a tested seed for this process, not fake self-hosting.
