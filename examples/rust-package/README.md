# Rust package example

From the repository root:

```sh
./target/release/bhasa run examples/rust-package/main.bh
./target/release/bhasa run examples/rust-package/main.bh
```

Each run prints `42`. The first run builds the local Cargo adapter declared in
`bhasa.toml`. The second asks Cargo to check freshness and reuses the Rust
archive. The project keeps Rust artifacts under `build/rust`; Cargo separates
debug and release builds and each requested target.

The adapter exports a C ABI function. This does not import arbitrary Rust types,
traits, generics or macros into Bhasa. Write an adapter around the Rust API you
need, declare compatible `extern def` functions, and keep unwinding and foreign
ownership inside the adapter. See [interop](../../docs/interop.md) and
[caching](../../docs/caching.md).

Cargo runs offline. There are no external crate dependencies in this example.
An already linked executable needs neither Cargo nor Rust installed.
