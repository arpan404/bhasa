# Interop status

Integration update: the compiler is now available, the source extension is `.bh`,
and `sh examples/rust-interop/run.sh` passed end to end after the rename. Its five
results matched `expected.txt`. The earlier missing-module notes below describe
the component's initial handoff and are resolved. See [integration.md](integration.md)
for final whole-project verification.

Implementation is written. Native Rust C ABI verification passes. The full
Bhasa caller run is pending bootstrap integration. No compiler files changed.

Changed files:

- `examples/rust-interop/adapter.rs`, one exported C ABI `i64 -> i64` function
  with saturating doubling.
- `examples/rust-interop/main.bh`, five calls covering signed and boundary
  values using `extern def`.
- `examples/rust-interop/expected.txt`, exact expected stdout.
- `examples/rust-interop/run.sh`, native macOS/Linux build, link, run and diff.
- `examples/rust-interop/.gitignore`, ignores local generated artifacts.
- `docs/interop.md`, ABI, panic, native build and narrow no-GC qualification.

Commands run and outcomes so far:

- `sh -n examples/rust-interop/run.sh` passed.
- `rustfmt --check --edition 2021 examples/rust-interop/adapter.rs` passed.
- `sh examples/rust-interop/run.sh` built the Rust static library with
  `--crate-type staticlib -C panic=abort -C opt-level=2`, then stopped at Cargo
  because `Cargo.lock` was absent and the script uses `--offline --locked`.
- Reran the script after `Cargo.lock` arrived. The native adapter again built
  successfully, now under `build/run.V2Jx1w/`. Cargo then failed because
  `src/main.rs` does not exist yet. The lockfile dependency is resolved.
- Created an untracked C smoke caller under the ignored
  `examples/rust-interop/build/adapter-smoke.c`. It calls the exported symbol at
  21, -21, 0, INT64_MAX, INT64_MIN and both half-range boundaries.
- `clang -Wall -Wextra -Werror examples/rust-interop/build/adapter-smoke.c examples/rust-interop/build/libbhasa_rust_adapter.a -o examples/rust-interop/build/adapter-smoke`
  passed. Running the resulting binary passed all seven cases.
- `nm -gU examples/rust-interop/build/adapter-smoke` showed
  `_bhasa_rust_double`. `otool -L examples/rust-interop/build/adapter-smoke`
  showed only `/usr/lib/libSystem.B.dylib`.
- `otool -tvV examples/rust-interop/build/adapter-smoke` showed the linked
  adapter as seven scalar instructions including return, with no calls or
  allocation path. This supports the source-level qualification of this tiny
  adapter; it does not audit arbitrary native dependencies.
- Repeated shell syntax and owned Rust formatting checks after final script
  edits; both passed.

Host tools recorded on October 11, 2026:

- rustc 1.98.0, commit `88d9e12ae178fab0fb5cc050a94da85685d449ea`,
  host `aarch64-apple-darwin`, LLVM 22.1.8.
- Cargo 1.98.0, commit `797e8a9bc`.
- Apple Clang 21.0.0, build `clang-2100.1.1.101`, target
  `arm64-apple-darwin25.2.0`.

Integration dependencies:

- The bootstrap must be buildable and implement `build SOURCE --target native
  -o OUTPUT --link ARCHIVE`, including the `-x none` reset before the archive.
  Latest concrete blocker is missing `src/main.rs`; codegen, parser, sema and
  driver modules also have not arrived at this check. Root `Cargo.lock` exists.
- Once those files exist, rerun `sh examples/rust-interop/run.sh`. A passing C
  caller verifies the adapter, but does not establish a passing Bhasa caller.

No branches, commits, nested repositories, external crate dependencies or
compiler modifications were created. Linux, Windows and cross-target execution
have not been verified.
