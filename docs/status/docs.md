# Docs status

Documentation is complete for the current bootstrap. No docs integration blocker remains. Verification below used the shared implementation available on October 11, 2026.

## Changed files

- `README.md`: language example, build/run instructions, scalar feature table, target and no-GC limits, Rust-to-Bhasa stages and guide links.
- `docs/language.md`: syntax, type rules, return annotations, operator precedence, static strings, traps, control flow, trusted scalar C ABI and deferred features.
- `docs/toolchain.md`: compiler/backend pipeline, every CLI command, JSON diagnostics, Clang selection, target aliases, cross-linking requirements, link-input rules and bootstrap stages.
- `docs/status/docs.md`: this handoff and verification record.

Only assigned documentation paths were edited. No implementation, research document, shared contract, branch or commit was changed by this worker. Cargo and CLI verification generated build artifacts. Temporary documentation fixtures and their outputs are under `/private/tmp/bhasa-docs-soz48aa1`.

## Verification actually run

| Command or check | Outcome |
| --- | --- |
| `rustc --version` | Rust 1.98.0. |
| `cargo --version` | Cargo 1.98.0. |
| `clang --version` | Apple Clang 21.0.0. |
| `clang -dumpmachine` | `arm64-apple-darwin25.2.0`. |
| `cargo build --release` during early assembly | Failed because `src/main.rs` had not arrived. This was resolved by the sibling implementation. |
| `cargo build --release --offline` after modules arrived | Passed. |
| `cargo test --offline` | Passed, 89 tests in that snapshot: 62 unit, 7 CLI, 11 language and 9 native tests. Native tests included arithmetic traps and exact example output in development and release modes. |
| `target/release/bhasa --version`, `--help`, `targets` | Passed; checked documentation against actual output and driver source. |
| Extract all 10 `bhasa` fenced blocks from the three guides, save as temporary source files, run `check` on each | Nine valid blocks passed. The intentionally invalid omitted-return example failed as documented. |
| Run the README example through `bhasa run`; build it with `build --release -o ...`; execute the output | Passed with exact stdout `Bhasa\n42\n`. |
| Run all six standalone valid language-reference examples | Passed with exact expected output, including recursion, nested scope, UTF-8 strings, loop branches and short-circuit avoidance of division by zero. The extern example was checked only. |
| `check ... --json` with `BHASA_CLANG=/bhasa-docs/nonexistent-clang` | Passed with `[]` and exit 0. |
| `emit-llvm ...` to stdout and with `-o ...`, using the same missing Clang path | Both passed. Stdout began with the LLVM module, without diagnostic contamination. |
| `check ... --json` on the intentionally invalid return example | Exited 1; stdout parsed as a nonempty JSON diagnostic array with documented fields. |
| `emit-llvm ... --target linux-x64 -o ...` | Passed for the toolchain guide's `square` example. |
| `build ... --target linux-x64 --emit object -o ...` | Passed. `file` identified the result as an x86-64 ELF relocatable object. It was not executed. |
| `build ... --target wasm32 --emit object -o ...` | Failed because installed Apple Clang has no compatible Wasm backend. No Wasm object was produced. The driver reported the backend requirement clearly. |
| Run `def main() { print(-9223372036854775808) }` | Passed and printed the exact signed minimum. The language guide states that this literal is accepted. |
| Python check of owned Markdown relative links and trailing whitespace | Passed after sibling guide files arrived. |

The documentation checks used the absolute compiler path with the temporary directory as the working directory. Generated source names are `README-1.bh`, `language-1.bh` through `language-8.bh`, `toolchain-1.bh` and `minimum.bh`. Commands in the table abbreviate those paths with `...`.

Source review covered `src/lexer.rs`, `src/parser.rs`, `src/sema.rs`, `src/codegen.rs`, `src/driver.rs`, `src/target.rs` and `src/diagnostic.rs`. A read-only reviewer checked the drafts against the contract and available implementation. The documentation includes the resulting details about signed minimum handling, braced `else`, function-name shadowing, reserved `print`, native target mapping and the incompatibility of `--link` with object emission.

## Integration notes

- Wasm object emission remains unverified on this host because its Clang lacks the backend. The docs describe it as conditional on backend availability. A successfully emitted Linux object does not certify cross-target executable linking.
- Cross-target executables and Windows execution were not tested by this worker.
- The linked Rust interop and self-hosting guides exist. Their owners retain responsibility for their scripts and detailed verification; this worker did not modify or rerun those scripts.
- The Bhasa seed is described as executable character and token helpers, not a compiler that reads or compiles source. Full self-hosting, ownership/borrowing, concurrency, GPU work, packages and full Rust compatibility remain explicitly future work.
- No implementation change is requested by the docs review. The integration chat should use its final full-suite result after all workers finish, since sibling source and tests were still arriving during verification.
