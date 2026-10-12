# Driver implementation status

Implemented and verified `src/main.rs`, `src/driver.rs`, and `src/target.rs`.

- All CLI commands in the bootstrap contract are present. `check --json` emits an array of diagnostic objects, or `[]` on success. Usage errors exit 2, compiler/tool errors exit 1, and `run` propagates the program exit status, including Unix signal status as 128 plus signal number.
- `check` and `emit-llvm` do not invoke Clang. Native IR uses the Rust host mapping. Native builds query the selected Clang with `-dumpmachine`.
- Clang receives argument arrays, `-O0` or `-O2`, and `-x none` before canonical native link paths. `BHASA_CLANG` selects one executable path, not a command string.
- Outputs are staged in unique directories beneath the output parent and renamed only after success. Source paths, link inputs, and their symlink/hardlink aliases cannot be overwritten. Temporary cleanup only owns directories created successfully by this process.
- Cross objects and linked executables have separate flags and messages. `run` accepts no explicit target and rejects a foreign default Clang target, including Android and incompatible Linux ABIs. It executes a private copy so another build cannot replace its executable with a cross-target artifact. wasm32 linking is rejected with guidance to emit objects.
- `Target` exposes `parse`, `is_native`, `triple_for_ir`, `alias`, and `output_extension`; its extensions omit the dot. The module also exposes `listing()` and `host_can_run()`.

Commands actually run:

- File-only `rustfmt src/driver.rs src/main.rs src/target.rs` succeeded. Driver-only formatting was repeated after edits.
- `cargo build` succeeded.
- `cargo test --lib driver::tests` passed 11 tests after final fixes.
- `cargo test --lib target::tests` passed 12 tests after final fixes.
- `cargo test --test cli` passed all 7 integration tests supplied by the tests owner.
- Target helper verification also ran standalone `rustc --test src/target.rs`, with 10 passing tests.
- A `python3` heredoc ran the compiled CLI in a temporary workspace. The final run passed all 22 checks: JSON success, JSON semantic errors, JSON missing-file errors, stdout and file IR emission with an unavailable Clang, native run, exit-code propagation, native release build and execution, Linux x64 objects, Linux arm64 objects, Windows x64 objects, unavailable wasm backend diagnostics, unavailable Clang diagnostics, IR source overwrite refusal, build source overwrite refusal, missing-main guidance, unknown target refusal, cross-target run option refusal, failed-link preservation of an existing output, foreign default Clang refusal, native linking through an object filename containing spaces and literal shell characters, and link-input overwrite refusal. The script also checked that temporary directories were removed and linked programs printed the expected output.
- `target/debug/bhasa run examples/basics/hello.bhasa` successfully printed the multilingual hello example.

Two earlier manual smoke attempts stopped at assertions that were adjusted to reflect the installed toolchain. Apple Clang lacks a wasm32 backend, and its native target normalization prints a module target override warning. The final smoke run expects the actionable wasm failure and permits that native Clang warning. It does not claim a wasm object was produced or that cross executables were linked.

Review fixes covered concurrent output replacement before execution and Linux/Android target confusion. Fake Clang tests prove that a foreign default target prevents compilation and that failed compilation preserves the prior output and removes partial temporary artifacts.

No integration blockers remain. The installed Apple Clang produced Linux x64, Linux arm64, and Windows x64 objects. Linked cross executables and wasm32 objects remain unverified on this machine. Native Clang reports `arm64-apple-darwin25.2.0` from `-dumpmachine` and normalizes that to a macOS triple during compilation, which can produce a harmless override warning that the driver leaves visible.

Only the assigned driver source files and this status file were edited. Build and test commands produced disposable or ignored artifacts. No branches, commits, or extra chats were created, and no messages were sent to other chats.
