# Type aliases

Use `type Name = Target` to give a concrete type another source name.

```bhasa
struct Box[T] { value: T }
type Count = int
type IntBox = Box[Count]
private type Counters = Map[str, Count]

def main() {
    let value: IntBox = Box[int](42)
    print(value.value)
}
```

Aliases can name scalars, owned builtins, records, enums, other aliases and
concrete generic types. Use them in annotations, return types, constructors,
enum variant paths and explicit generic arguments. An alias declaration ends at
a newline, semicolon or file EOF. Its target must fit on one logical token line.
`type` is contextual; ordinary functions and local variables named `type` remain
valid. Alias declarations belong at the top level.

The alias itself takes no type parameters. `type Ints = List[int]` is supported;
`type Values[T] = List[T]` is rejected. Generic applications retain explicit type
arguments and accept a trailing comma, such as `type IntBox = Box[int,]`.

## Type identity and ownership

An alias has the target's exact type identity and runtime layout. `IntBox` and
`Box[int]` share one specialization. No conversion, wrapper, separate nominal ID
or extra runtime allocation is introduced by the alias. To define a distinct
type with its own fields, declare a `struct`.

Ownership, borrowing and cleanup follow the target. A Text alias still owns valid
UTF-8 data; copies remain independent. A result alias retains the underlying
result shape and local handling obligations. A task alias does not permit
explicit task annotations, and aliases do not relax worker call-graph checks.

The [runnable example](../examples/aliases/main.bh) combines aliases for `int`,
`Text`, a record and a concrete generic record. From the repository root:

```sh
bhasa check examples/aliases/main.bh
bhasa run examples/aliases/main.bh
```

Its manifest declares the local `std` package, and it constructs Text through
`text(...)`. The program prints:

```text
42
Bhasa
Bhasa aliases
```

## Modules and privacy

Aliases are public by default. `private type Name = Target` restricts the alias
name to its defining module. Imports expose public aliases under the same rules
as other type declarations, including `domain.Order` namespace qualification.
Targets resolve in the alias's defining module before substitution. A same-named
type in the importing module cannot change that target.

The target keeps its original nominal owner and privacy. An alias to `Text`
cannot call Text's private-field constructor or access its storage; use its
public factory and methods. An alias to a record with private members preserves
those restrictions. Public aliases cannot expose a private top-level type, even
through another alias or a nested generic argument. See [packages](packages.md).

## Validation and bounds

Every alias target is validated, including unused declarations. Unknown and
non-type targets, duplicate declarations, alias cycles, malformed targets and
wrong generic arity fail at compilation. Concrete generic targets undergo
semantic schema validation even when unused. For example,
`type Bad = Map[buffer, int]` is rejected because map keys must be `int` or `str`.
Diagnostics retain original source names, files and byte locations.

| Limit | Value |
| --- | --- |
| Alias resolution and target nesting | 64 levels |
| Expanded concrete target spelling | 64 KiB |
| Cumulative canonical alias target strings | 16 MiB |
| Lowered module source | 128 MiB |

Generic targets also obey the [generic limits](generics.md), including 1024
concrete instances and 16 MiB of generated source. These limits bound source
and spelling sizes; they are not a whole-process memory limit.

The Rust host resolves aliases before either semantic frontend runs. The
standalone Bhasa compiler seed does not perform this host lowering itself.

## Editor and inspection

A fully checked graph gives aliases their own source declaration identity. The
outline, completion, hover, definition, references, rename and semantic tokens
include unused aliases. `bhasa inspect --json` reports both the declared target
and the canonical target. Renaming an alias preserves the target's name; renaming
a nominal target updates its proven references, including nested generic RHS
arguments such as `Box[Point]`. Qualified edits replace only the final identifier.

The formatter preserves alias spelling and comments. Editor positions use the
original UTF-16 ranges, and rename returns versioned edits after checking the
proposed graph. Locks, module privacy and unsaved-buffer rules still apply.
Unchecked or incomplete graphs and unresolved generic shadows refuse refactoring;
no source spelling alone authorizes a rename. See [tooling](tooling.md).
