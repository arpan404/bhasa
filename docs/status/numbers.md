# Numeric types

Bhasa has two numeric scalar types: `int`, a checked signed 64-bit integer, and
`float`, an IEEE 754 binary64 value. There are no implicit conversions between
them. Fixed-width integer variants, unsigned integers, `f32`, and an `f64` spelling
alias are not implemented in this milestone.

```bh
def average(total: float, count: int) -> float {
    return total / float(count)
}

def main() {
    var value: float = 1.25
    print(value * 2.0)
    print(average(10.5, 3))
    print(int(-2.9))
}
```

Run `bhasa run examples/numbers/main.bh` for a complete example.

Float literals require decimal digits before the point, and digits after a point
when present. An exponent starts with `e` or `E`, may contain `+` or `-`, and must
contain digits. Examples include `0.1`, `1e3`, `1.25E-2`, and `0002.500`. Signs are
unary operators. `.5`, `1.`, separators such as `1_000.0`, and hexadecimal floats
are unsupported. An integer spelling without a point or exponent remains `int`.
`1.member` continues to tokenize as integer, dot, and member name.

Finite literals are rounded to binary64. Values too small to represent round to
zero or a subnormal; literals that round to infinity are rejected. No literal
spellings for NaN or infinity are provided. Rust diagnostics use `L008` for a
malformed float and `P005` for a nonfinite literal; the Bhasa emitter reports
`BH217` for the latter.

Float supports `+`, `-`, `*`, `/`, unary `-`, and comparisons. Both operands must
have the same numeric type. `%` remains an integer operation. Boolean operators
still require `bool`. Arithmetic follows IEEE 754, including signed zero,
infinity, and NaN. Division by floating-point zero follows those rules. Equality
with NaN is false, inequality with NaN is true, and ordered comparisons involving
NaN are false. The compiler emits no fast-math flags.

`float(value)` accepts one `int` and rounds to binary64. Integers above 2^53 may
lose precision. `int(value)` accepts one `float`, requires a finite value in
`[-2^63, 2^63)`, and truncates toward zero. A failed conversion traps before LLVM
can execute an invalid conversion. In particular, `int(float(9223372036854775807))`
traps because the intermediate float rounds to 2^63. Neither conversion accepts
strings, booleans, or a value already of its destination type.

`print(float)` uses the C runtime's `%.17g` formatting followed by a newline.
This preserves enough significant decimal digits to distinguish binary64 values,
so `print(0.1)` prints `0.10000000000000001`. The platform C runtime controls the
spelling of infinity and NaN. This is not a shortest-decimal formatting API.

Floats can be parameters, return values, variables, record fields, enum payloads,
and explicit generic arguments. They are copied scalars and add no allocation or
cleanup. Existing aggregate ownership and alias rules continue to apply. Foreign
functions keep the existing integer-only ABI, and task entrypoints remain
`int -> int`; their Bhasa bodies may use floats internally. `main` still returns
`void` or `int`.

Both compiler frontends implement these rules. The Rust parser converts literals
to binary64 and emits exact hexadecimal LLVM constants. The Bhasa compiler keeps
its own implementation integer-only: it normalizes decimal spelling, checks the
exact overflow midpoint `2^1024 - 2^970`, and lets LLVM round a finite decimal
constant. This permits bootstrapping from the previous integer-only compiler.
Scanner token code 4 represents float literals, keyword code 318 represents
`float`, and emitter type code 10 represents the scalar. Existing token codes and
runtime handle codes retain their meanings.

## Verification

Five focused reference tests passed in `tests/numbers.rs`:

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --no-default-features \
  --test numbers -- --test-threads=1
```

These tests also build a fresh standalone Bhasa seed and compare native results.
Coverage includes arithmetic, comparisons, NaN/infinity/signed-zero behavior,
checked conversion traps at O0 and O2, owned record and enum payloads, generics,
formatting, literal boundaries, and unchanged foreign/worker restrictions.
A separate native comparison matched 165 decimal cases between the frontends,
including subnormals, 5,000-digit exponent cancellation, and the integer directly
below the exact overflow midpoint. Both frontends rejected the midpoint and its
successor.

The scanner differential test includes the new token kinds and malformed
exponents. A sixth number test, enabled by `bhasa-frontend`, exercises the native
scanner/compiler FFI and requires a rebuilt frontend archive. Standalone seed
results do not establish that separately linked archive result.

Complete acceptance passed five reference and six full-hybrid number groups.
The sixth exercises the rebuilt native scanner/compiler FFI. Final logs are
build/verify-foundation-reference.log and build/verify-foundation-hybrid-frontend.log.
