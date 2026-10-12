# Calling a Rust adapter

From the repository root, build and run the complete example with:

```sh
sh examples/rust-interop/run.sh
```

This builds the Rust adapter, links it into the Bhasa program, runs the result,
and checks the output. Running `bhasa run examples/rust-interop/main.bh` alone
cannot link the program. Its `extern def` declares `bhasa_rust_double`, but the
implementation lives in `adapter.rs` and must be built and supplied with `--link`.

The bootstrap supports explicit C ABI imports with `int` parameters and `int`
or `void` results. Bhasa `int` is signed 64-bit. An `extern def` declaration is a
trusted promise that the linked symbol has the declared signature. The compiler
does not inspect the foreign implementation or prove that the declaration is
correct.

The example in [`examples/rust-interop`](../examples/rust-interop/) exports one
Rust function and calls it from Bhasa:

```rust
#[no_mangle]
pub extern "C" fn bhasa_rust_double(value: i64) -> i64 {
    value.saturating_mul(2)
}
```

```bhasa
extern def bhasa_rust_double(value: int) -> int

def main() {
    print(bhasa_rust_double(21))
}
```

The adapter doubles a number and clamps an out-of-range result to `i64::MIN`
or `i64::MAX`. This is the adapter's own arithmetic contract. Ordinary Bhasa
arithmetic traps on overflow.

Give adapters names that do not collide with platform libraries. The bootstrap's
builtin `print` uses C `printf`; a module that also imports an adapter named
`printf` is rejected. Ordinary Bhasa functions have mangled symbols and may use
that name without colliding with the C runtime.

The export uses the target's C calling convention and a fixed symbol name.
Rust's native ABI has no stability guarantee. A `staticlib` is a library output
intended for linking into non-Rust programs. This example does not import Rust
crates or expose Rust's native ABI. [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html#abi),
[Rust library outputs](https://doc.rust-lang.org/reference/linkage.html)

## Run the example

Use installed Rust, Cargo, Clang and the host's native linker and system SDK.
The script supports native macOS and Linux hosts with matching Rust and Clang
targets. It does not download tools or dependencies. It builds `adapter.rs`
directly with `rustc`, then builds and invokes the bootstrap compiler with
`--link`, runs the native executable, and compares its entire output with
`expected.txt`. The five calls
cover a positive number, a negative number, zero, and saturation at both signed
64-bit boundaries. Successful output is:

```text
42
-42
0
9223372036854775807
-9223372036854775808
Rust C ABI interop passed.
```

All generated files go under `examples/rust-interop/build/`, including a separate
Cargo target directory. Each invocation prints its unique `run.*` directory to
standard error and leaves the archive, executable and captured output there.
The bootstrap's `Cargo.lock` must be present;
`--offline --locked` prevents downloads and lockfile updates. If the bootstrap
source is still being assembled, its build will fail and the script will stop.
The successfully built Rust archive remains available for inspection. The
script explicitly builds both Rust components for the installed Rust compiler's
host target, overriding any configured Cargo cross target. This command sequence
is reproducible with compatible installed tools; tool versions are not pinned.

With a bootstrap binary already built at `target/release/bhasa`, you can also
build the adapter and pass it to `run` yourself. These manual commands use a
stable archive path, separate from the script's unique `run.*` directories:

```sh
mkdir -p examples/rust-interop/build
rustc --edition=2021 --crate-type staticlib --crate-name bhasa_rust_adapter \
    -C panic=abort -C opt-level=2 \
    examples/rust-interop/adapter.rs \
    -o examples/rust-interop/build/libbhasa_rust_adapter.a

./target/release/bhasa run examples/rust-interop/main.bh \
    --link examples/rust-interop/build/libbhasa_rust_adapter.a
```

To keep the executable at a chosen path, use `build` instead:

```sh
./target/release/bhasa build examples/rust-interop/main.bh --target native \
    -o examples/rust-interop/build/rust-interop \
    --link examples/rust-interop/build/libbhasa_rust_adapter.a
examples/rust-interop/build/rust-interop
```

`BHASA_CLANG` may name an installed Clang executable. Use a native Clang for this
script. Cross-compilation requires a Rust archive for the exact target plus
matching system libraries, SDK and linker. The script does not cross-compile
or attempt to execute cross-target output. A different Rust adapter may also
require explicit native system libraries when linked from a non-Rust driver.
[Rust static library linkage](https://doc.rust-lang.org/reference/linkage.html)

## Panic and memory contract

`-C panic=abort` makes a Rust panic terminate the process. It does not turn a
panic into a Bhasa error or permit unwinding through the C boundary. The sample
operation uses saturating arithmetic and has no intended panic path.
[Rust panic option](https://doc.rust-lang.org/rustc/codegen-options/index.html#panic)

The reviewed adapter source performs one scalar operation. It has no external
Rust crates, heap allocations, reference-counted values, callbacks, runtime
loading or garbage collector. The Bhasa caller passes only integers. Its
`print` calls use the bootstrap's native output routine. The adapter adds
no managed runtime or collector to the application. Native system libraries
may allocate internal buffers; this is not a promise of an allocation-free
process.

That qualification applies to this source and its selected native build. It is
not a general safety certificate for Rust, C ABI libraries, arbitrary crates,
or every platform runtime. A new adapter needs review of its implementation,
transitive dependencies, native libraries and runtime loading before making a
no-GC claim. An `extern def` declaration performs no such review.

The bootstrap does not support pointers, structs, strings across FFI, callbacks,
unwinding, Rust collections, trait objects or arbitrary crate imports. Future
adapters involving resources will need explicit ownership, lifetime and error
contracts. The current compiler provides no Rust-like borrow checking at this
boundary.
