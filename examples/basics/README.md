# Basic programs

Run these commands from the repository root:

```sh
cargo run -- check examples/basics/hello.bh
cargo run -- emit-llvm examples/basics/hello.bh -o build/hello.ll
cargo run -- build examples/basics/hello.bh -o build/hello
cargo run -- run examples/basics/hello.bh
cargo run -- run examples/basics/recursion.bh --release
```

`check` and `emit-llvm` work without Clang. `build` and `run` use installed Clang, or the executable path in `BHASA_CLANG`.

| Program | What it exercises | Output |
|---|---|---|
| `hello.bh` | Printing and UTF-8 literals | A greeting and a multilingual line |
| `recursion.bh` | Forward calls, recursion, and returns | `3628800`, then `55` |
| `scopes.bh` | Nested shadowing and outer-variable assignment | `20`, `30`, `20`, `10`, `11` |
| `loops.bh` | Nested loops and branching | `14`, then `true` |
| `arithmetic.bh` | Precedence, signed arithmetic, boundaries, and comparisons | Integer and boolean results |
| `short_circuit.bh` | Word and symbolic boolean operators | Skips trapping calls and prints evaluated call markers |
| `strings.bh` | String parameters/results, escapes, UTF-8, and literal percent signs | Exact literal contents with an appended newline |

The integration suite compares the complete output of every example in both development and release modes:

```sh
cargo test --tests -- --nocapture
```

Native tests report a skip only when the Clang executable is absent. The foreign object test also reports a skip if the probed target backend is unavailable. Compilation failures with available tools fail the tests. Foreign objects are inspected, never executed.
