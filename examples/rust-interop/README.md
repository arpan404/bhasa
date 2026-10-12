# Rust adapter example

From the repository root, run:

```sh
sh examples/rust-interop/run.sh
```

The script builds the Cargo crate containing `adapter.rs` and the Bhasa compiler, links the adapter into
`main.bh`, runs the program, and checks its output against `expected.txt`.
It requires installed Rust, Cargo, Clang and native linking tools on macOS or
Linux. It does not download dependencies. Cargo reuses the compiled Rust adapter
under `build/rust-target` and the compiler under the repository's `target`
directory. Running the script again with unchanged inputs does not recompile
either Rust package.

Running `bhasa run examples/rust-interop/main.bh` alone fails with an undefined
`bhasa_rust_double` symbol. `extern def` declares the signature; the compiled
Rust archive supplies the implementation.

To declare Rust adapters in a Bhasa project and build them automatically, see
[the package example](../rust-package/README.md).

To use an already built `target/release/bhasa`, build the archive with Cargo:

```sh
rust_host=$(rustc -vV | sed -n 's/^host: //p')
cargo build --offline --locked --release \
    --manifest-path examples/rust-interop/Cargo.toml \
    --target "$rust_host" --target-dir examples/rust-interop/build/rust-target

./target/release/bhasa run examples/rust-interop/main.bh \
    --link "examples/rust-interop/build/rust-target/$rust_host/release/libbhasa_rust_adapter.a"
```

Both workflows reuse stable artifact paths. Deleting `build/rust-target` discards
the adapter cache and makes the next run compile it again.

Expected program output:

```text
42
-42
0
9223372036854775807
-9223372036854775808
```

The script also prints `Rust C ABI interop passed.` after checking the output.
See [the interop guide](../../docs/interop.md) for the ABI and memory contracts.
