# Generics

Bhasa specializes explicitly parameterized functions, records and enums at
compile time. Each specialization gets concrete types and uses the ordinary
ownership and cleanup rules. It adds no erased values, runtime type lookup or GC.

```bhasa
struct Box[T] {
    value: T
    def get(self) -> T { return self.value }
}

enum Option[T] { none; some(value: T) }
def identity[T](value: T) -> T { return value }

def main() {
    var box = identity[Box[buffer]](Box[buffer](buffer("hello")))
    var result = Option[Box[buffer]].some(box)
    match result {
        case Option[Box[buffer]].none { print("empty") }
        case Option[Box[buffer]].some(value) { print(value.get()) }
    }
}
```

Run `bhasa run examples/generics/main.bh`. Type arguments are required at every
application; local variable types still infer normally. Multiple parameters use
`Pair[A, B]`. Nested applications, imported templates and methods that use their
enclosing record's parameters work. `std` provides `Option[T]`, `Result[T]`,
`Box[T]` and `option_or[T]`, alongside its existing concrete result types.

The Rust host resolves imports and lowers generic source before either the Rust
or Bhasa semantic frontend. Diagnostics map back to the original files and use
readable concrete names. The standalone Bhasa compiler seed does not yet perform
this lowering itself. Syntax and explicit generic references are checked in
unused templates; dependent body types, ownership and schema constraints are
checked when instantiated. Recursive aggregate layouts and unsafe worker call
graphs remain errors after specialization.

There are no inferred/default type arguments, traits, bounds, generic extern
functions or method-specific type parameters. Applications allow at most 32
parameters, nesting/instantiation chains of 64, 1024 concrete instances, 16 MiB
of generated source and 64 KiB of concrete type spelling. Expanding recursive
instantiations fail with a diagnostic. The `__bhg_` namespace is reserved.
