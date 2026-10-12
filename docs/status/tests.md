# Integration tests status

The tests worker owns only `tests/`, `examples/basics/`, and this file. No implementation modules were edited.

## Added files

- `tests/common/mod.rs` supplies isolated temporary directories, CLI execution, and native tool discovery. Child commands receive environment changes individually so parallel tests do not race on process environment.
- `tests/language.rs` covers examples, diagnostics, grammar, type rules, scope, returns, scalar extern declarations, literal limits, and library entry points.
- `tests/cli.rs` covers `check`, `emit-llvm`, target aliases and exact triples, help/version/targets, plain and JSON diagnostics, invalid arguments, and missing Clang.
- `tests/native.rs` covers native builds and execution in development and release modes, `run` exit status, default outputs, overflow and division traps, nested boolean control flow, internal symbol collisions, object-only libraries, C ABI linking with paths containing spaces, and foreign object emission.
- Seven runnable examples were added under `examples/basics/`: `hello.bh`, `recursion.bh`, `scopes.bh`, `loops.bh`, `arithmetic.bh`, `short_circuit.bh`, and `strings.bh`. `examples/basics/README.md` gives commands and expected results.
- `tests/fixtures/library.bh` has a function without main for object builds. Eight files under `tests/fixtures/invalid/` cover representative rejected programs.

## Verification

Host tools report Rust 1.98.0 and Apple Clang 21.0.0. Clang reports host triple `arm64-apple-darwin25.2.0`.

- `rustfmt --edition 2021 tests/common/mod.rs tests/language.rs tests/cli.rs tests/native.rs` completed. Formatting was restricted to owned files.
- `cargo test --tests --no-run` passed.
- `cargo test --test language --test cli -- --nocapture` passed, 11 language tests and 7 CLI tests.
- `cargo test --test native -- --nocapture` passed, 9 native tests. This includes all seven examples in both build modes and eight independent overflow/division cases in both modes. Each trap case first requires a successful build, then a nonzero executable exit with no continuation marker.
- `rustfmt --edition 2021 --check tests/common/mod.rs tests/language.rs tests/cli.rs tests/native.rs` passed.
- `cargo test -- --nocapture` passed, 70 module tests plus all 27 integration tests, with zero failures. No documentation tests were defined at this verification point.
- A Python standard-library verification used `subprocess.run(['target/debug/bhasa', 'check', 'tests/fixtures/invalid/unknown_name.bh', '--json'], text=True, capture_output=True)` and `json.loads` on stdout. It confirmed a nonzero compiler status and a valid JSON list containing string `code` and `message` fields. Python is not a dependency of the Rust test suite.

The foreign object test successfully emitted an x86_64 Linux ELF relocatable object using installed Clang. It checks the ELF class, byte order, object type, and target machine without executing the foreign object.

## Findings and integration blockers

No implementation defects were observed in these tests. No required integration work remains for the tests assignment. No native or foreign-object integration test skipped on this host.

The aggregate run reported one optional backend verification skip inside the LLVM owner's module test `codegen::tests::pure_library_has_no_entry_or_print_dependency_and_emits_wasm_object`. Installed Apple Clang returned `No available targets are compatible with triple "wasm32-unknown-unknown"`. This is an unavailable tool backend, not a source compilation regression. The required foreign-object acceptance was satisfied separately by the successful x86_64 Linux ELF emission.

All checks above refer to the shared checkout as observed during this run. Other workers may continue editing their files after this status update.
