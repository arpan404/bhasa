# Explicit generics

Bhasa supports generic structs, enums, and functions through compile-time
specialization. The Rust host expands a complete import graph before either the
Rust or Bhasa frontend checks it. Each concrete specialization uses the existing
ownership checker and LLVM code generator. There is no runtime type erasure,
generic dispatch table, or garbage collector.

```bh
struct Box[T] {
    value: T
    def copy(self) -> T { return self.value }
}

enum Option[T] { none; some(value: T) }

def identity[T](value: T) -> T { return value }

def main() {
    var value: Box[buffer] = identity[Box[buffer]](Box[buffer](buffer("hello")))
    match Option[Box[buffer]].some(value) {
        case Option[Box[buffer]].none { print("empty") }
        case Option[Box[buffer]].some(item) { print(item.copy()) }
    }
}
```

Run `bhasa run examples/generics/main.bh` for a complete example. Type arguments
are explicit at calls, constructors, annotations, and enum patterns. A declaration
can have several parameters, such as `Result[T, E]`. Nested applications are
supported. Methods inside a generic struct inherit its parameters; the receiver
and cleanup rules are the same as for ordinary structs.

Concrete arguments may be `int`, `bool`, `str`, `buffer`, `ints`, `tcp_listener`,
`tcp_socket`, `tls_socket`, or a named concrete record or enum. They cannot be
`void`, a linear `task`, or a function. Instantiating a generic with a transport
handle does not add runtime availability on unsupported targets.

Repeated uses of the same declaration and arguments share one specialization.
Different arguments produce distinct nominal types: `Box[int]` cannot be passed
where `Box[bool]` is required. Owned fields and enum payloads retain their existing
copy, borrow, and deterministic cleanup behavior. Transitive worker restrictions
also apply through specialized functions and methods.

Every template is syntax-checked, even when unused. Explicit generic references,
their arity, and their type arguments are validated in unused templates too.
Body type checking, ownership checking, and aggregate schema validation run for
each concrete specialization. Consequently, an unused template's body has not
been proved valid for arbitrary type arguments. Operations in a generic body
must be supported by the actual types with which it is instantiated.

This milestone does not support argument inference, default arguments, trait
bounds, generic extern declarations, parameters on individual methods, or type
parameters used as arbitrary constructor/member namespaces. Use concrete C ABI
adapter functions for external calls. The shared expansion pass is written in
Rust; the standalone Bhasa compiler does not independently implement this pass.

## Expansion and diagnostics

`generics::expand(source, boundaries)` returns an `Expansion` containing concrete
source and adjusted module EOF positions. Its scanner treats strings and comments
as opaque data. Original template declarations are blanked while preserving their
byte lengths and line endings. Concrete declarations are appended as separate
module units. An incomplete declaration cannot consume the next module's tokens.
Source without generic syntax retains its exact bytes and module boundaries.

`generics::parse_expansion(&expansion)` parses each unit separately, merges its
nominal type table, and maps AST spans back to the original virtual source before
semantic checking. Template-body errors point to the template body; errors at a
concrete use point to that use. Replacements may map a generated token to a whole
original application such as `Box[buffer]`. Source offsets remain UTF-8 byte
offsets; editor code converts them to UTF-16 positions.

`Expansion::map_diagnostics` maps both spans and generated names, for diagnostics
from the Bhasa frontend. After an AST has already been remapped, use
`Expansion::display_diagnostics` to restore names without remapping spans twice.
`display_names` provides readable concrete names, while `original_name` maps an
internal specialization name back to its declaration name and preserves method
or variant suffixes.

`generics::template_program(source, boundaries)` exposes original template
function, record, enum, field, and method shapes for editor tooling, including
unused templates. This syntax-only AST omits type-argument lists and retains
original spans. It is not a checked program and must not be sent directly to
code generation.

## Bounded expansion

The reserved `__bhg_` identifier prefix prevents collisions with generated names.
Generic declarations accept at most 32 type parameters. Expansion enforces a
64-level type-argument parser limit and a 64-generation instantiation-chain limit,
a maximum of 1,024 concrete specializations, a 16 MiB generated-source limit, and
a 64 KiB bound on the calculated spelling of a concrete generic name. The spelling
bound is checked before concatenation, including types that duplicate their
arguments recursively.

Ordinary recursive calls reuse an existing specialization. Recursive expansion
that keeps creating new argument types terminates with `G006`. Recursive owned
schemas remain invalid even when the specialization cache itself reaches a fixed
point. These are language implementation limits, not promises of a fixed peak
compiler memory budget.

## Verification

Final combined verification passed all 14 reference tests and all 15 full-hybrid
tests in `tests/generics.rs`, recorded in `build/verify-next-reference.log` and
`build/verify-next-hybrid-frontend.log`. The focused command is:

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --no-default-features \
  --test generics -- --test-threads=1
```

Coverage includes imported templates, nested records and enums, inherited methods,
nominal typing, explicit argument validation, unused-template syntax, UTF-8 span
mapping, per-module EOF isolation, recursion bounds, borrow and worker rejection,
and native execution at O0 and O2. Ownership tests instrument generated LLVM with
AddressSanitizer and the runtime with UndefinedBehaviorSanitizer; allocation
accounting reaches zero at both optimization levels.

The extra feature-gated test checks the same ownership program through the Bhasa
frontend and executes it at O0 and O2. The final scanner regression checks direct
API diagnostics for Unicode escapes, raw/escaped LF/CR/CRLF, NUL ordering and
module EOF isolation, comparing the native frontend when enabled. The example
prints `hello generics`. Final LSP tests preserve original template symbols and
return every dependent-method target sharing one template source span.
