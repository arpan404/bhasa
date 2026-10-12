# Module namespaces and privacy

The Rust host lowers import aliases and privacy before generic specialization.
Both compiler frontends consume that lowered source. This milestone does not
add namespace lowering to the standalone Bhasa seed.

```bhasa
import "math.bh" as math
import "@collections" as collections

private def helper(value: int) -> int {
    return value * 2
}

def main() {
    var point: math.Point = math.Point(21)
    print(math.identity[int](helper(point.x)))
}
```

Declarations remain public unless marked `private`. Unaliased imports retain
direct names and transitively export public declarations. An alias exposes
that module's exported functions, types, enum variants and generic templates
under the chosen namespace. Distinct unaliased exports with the same name are
errors; repeated imports of the same canonical declaration are deduplicated.
Import aliases are local to their importing source file and are not re-exported.

`private` may precede a top-level `def`, `extern def`, `struct` or `enum`, and
record fields and methods. Private declarations are callable or nameable only
inside the file that defines them. Enum variants share their enum's visibility.
Records with private fields must be constructed inside their defining file;
public factory functions can return them to callers. Public methods may expose
a chosen interface without exposing private fields or methods. Types inferred
from a public factory remain usable, including public methods on those values.

Member privacy uses the checked receiver type, including chained field access,
method calls, field assignment and concrete generic records. Both frontend
selections run this shared Rust-host typed privacy verification. The Bhasa
frontend still runs its own lexer, parser, checker and LLVM emitter afterward;
there is no compiler fallback. This extra host verification is a remaining
Rust dependency. It must not be described as native seed privacy support.

Namespaces rewrite only identifier tokens. String and comment contents stay
unchanged, and local variables, parameters, match bindings and generic type
parameters retain their scope. Source maps compose with generic maps so errors
refer to the original file and byte locations. Every file keeps a separate EOF
boundary. Names beginning with `__bhm_` or `__bhg_` are reserved. `List` and `Map`
are reserved top-level collection type names. C ABI extern names remain
unchanged, so incompatible or duplicate declarations of one native symbol
remain errors even if imported under different aliases.

File imports may use `..` to reach another folder inside the package root.
Canonical targets outside that root, absolute imports and escaping symlinks
are rejected. This permits `tests/main.bh` to import `../src/lib.bh` without a
self-dependency in `bhasa.toml`. Manifest entry paths still forbid `..`.

The example is [examples/namespaces](../../examples/namespaces/src/main.bh).
Focused coverage is in [module_namespaces.rs](../../tests/module_namespaces.rs).
All ten namespace groups and ten existing project groups pass in complete
reference/hybrid acceptance. Collection getter chains preserve private generic
receiver types. Native syntax and worker diagnostics use their own codes;
assertions now check exact EOF mapping and S020/BH209 rather than matching
reference wording. Logs are build/verify-foundation-reference.log,
build/verify-foundation-hybrid-frontend.log and the focused modules-hybrid-final log.

LSP definitions use declaration URI plus canonical identity, so two imported
modules may expose the same function, type or method name without navigation
crossing into the wrong file. Unsaved alias renames retain their resolved import
targets; actual import-path changes still require saving the graph. Private
modifiers and qualified types retain syntax-only symbol outlines even while an
import is unresolved. All 39 LSP protocol tests pass in complete reference/hybrid acceptance.
Alias-only unsaved edits retain resolved targets; its regression keeps the import
path unchanged while renaming the alias. Editor client/grammar tests pass 33.
