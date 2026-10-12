# Type aliases example

Run from the repository root with a compiler containing type-alias support:

```sh
bhasa check examples/aliases/main.bh
bhasa run examples/aliases/main.bh
```

The lowercase `bhasa.toml` declares the bundled local `std` dependency. The
example aliases `int`, Text, a record and a concrete generic record. Constructors
use the same canonical types, and copying a Text alias retains independent owned
bytes. Text is created through its public factory because its storage is private.

Expected output:

```text
42
Bhasa
Bhasa aliases
```

The Rust host performs alias lowering for either frontend. Use the hybrid
compiler's executable with `--frontend bhasa` to select its native frontend.
See [type aliases](../../docs/aliases.md) for syntax, privacy and limits.
