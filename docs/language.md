# Bhasa language reference

This reference describes the implemented syntax and type rules. The
[stage-0 contract](BOOTSTRAP.md) and [runtime contract](STAGE1.md) record earlier
milestones. Current guides cover [records](records.md), [packages](packages.md),
[networking](network.md), [generics](generics.md), [TLS](tls.md) and
[editor tooling](tooling.md), [collections](collections.md) and
[native tests](testing.md) and [iteration](iteration.md). General lifetime parameters, trait bounds and
arbitrary Rust API imports remain future work.

## Source and blocks

Source files use `.bh`. They can contain leading import declarations, named
`struct` and `enum` declarations, function definitions and `extern def` declarations.
Braces delimit record, enum, function, branch, match and loop bodies. Indentation is for
readability.

```bhasa
# A comment
def main() {
    var value = 40; value = value + 2 // Another comment
    print(value)
}
```

A newline, `;`, closing brace or end of file can end a simple statement. Newlines inside parentheses are whitespace. A newline after `return` ends that statement; it never attaches the next line as the return value.

Identifiers use ASCII letters and underscores, with digits allowed after the first character. String literals may contain non-ASCII UTF-8 text. Both `#` and `//` start comments through the end of the line.

The bootstrap limits syntax nesting and combined block/expression tree depth to 128. Split unusually long expressions into intermediate variables. Exceeding either limit produces a compiler diagnostic.

## Types and variables

| Type | Meaning |
| --- | --- |
| `int` | Signed 64-bit integer, from -9223372036854775808 to 9223372036854775807 |
| `float` | IEEE 754 binary64; explicit int/float conversions |
| `List[T]`, `Map[K,V]` | Specialized owned collections; map keys int or str |
| `bool` | `true` or `false` |
| `str` | Immutable static UTF-8 string |
| `buffer` | Owned mutable byte vector, allowing arbitrary bytes including NUL |
| `ints` | Owned mutable vector of signed 64-bit integers |
| A declared record name | Owned value containing the named record's fields |
| A declared enum name | Owned tagged value containing one variant's payload |
| `tcp_listener`, `tcp_socket` | Owned native TCP handles; a copied handle owns a duplicated descriptor for the same endpoint |
| `tls_socket` | Owned native TLS handle; copies retain the same refcounted session with serialized I/O |
| `task` | Inferred linear scalar task handle; no explicit annotation; consume once with `join` or `await` in its lexical block |
| `void` | No returned value; valid only as a function return type |

There are no implicit conversions. A condition must be `bool`; integers do not act as booleans.

Every `var` needs an initializer. An optional annotation checks the initializer's type. Otherwise, the compiler infers the variable's type from its initializer. Reassignment must preserve that type.

```bhasa
def main() {
    var count = 3
    var ready: bool = true
    var name: str = "Bhasa"
    count = count + 1
    name = "भाषा"
    if ready {
        var count = 10
        print(count)
    }
    print(count)
    print(name)
}
```

Variables have block scope. A nested block may shadow an outer variable, but duplicate declarations within one scope fail. Parameters and variables in the outer function body share one scope. There are no global variables or exposed raw pointers. Task handles cannot be shadowed.

Strings can be passed to Bhasa functions, returned from them and reassigned as static references. Reassigning a `str` variable changes which string it names; it does not mutate the text. Use `buffer(text)` for mutable or dynamic text. Buffer operations use byte offsets, not Unicode character offsets.

Owned containers are destroyed at block exit or return. Assigning or binding an
existing container makes a deep copy; assigning a newly returned container takes
its allocation. Function parameters borrow their containers synchronously and
may modify their contents. A borrowed parameter cannot be rebound. Returning a
borrowed container copies it so the result survives the call. See the
[runtime reference](runtime.md) for operations and examples.

## Records

```bhasa
struct Point { x: int; y: int }
struct Line { start: Point; end: Point }

def main() {
    var point = Point(2, 3)
    var line = Line(point, Point(10, 20))
    line.start.x = 42
    print(point.x)      # 2
    print(line.start.x) # 42
}
```

Constructors take one positional value per field in declaration order. Every
record name denotes a distinct type. Field reads and assignments use dots;
assignment must be rooted in a variable or parameter. Fields can contain scalar
types, owned containers, TCP/TLS handles, other records or enums. `void`, task
handles and recursive aggregate layouts are rejected.

Existing records copy on construction, binding, assignment and borrowed return.
Nested buffers and records are independent copies. TCP fields copy their
descriptor ownership while retaining the same underlying connection. TLS fields
retain one shared, serialized TLS session; they do not duplicate its stream.
Parameters borrow records and can mutate their fields, but cannot rebind the parameter.
The checker rejects overlapping record/descendant borrows and later argument
calls that could invalidate a previously borrowed field. These rules are
conservative and do not infer read-only functions. See [records](records.md)
for evaluation order, examples and cleanup rules. Private fields/methods are
module-scoped; see [packages](packages.md). Inheritance and record equality are
not implemented.

## Record methods

```bhasa
struct Point {
    x: int
    y: int
    def total(self) -> int { return self.x + self.y }
    def shift(self, amount: int) { self.x = self.x + amount }
}

def main() {
    var point = Point(20, 22)
    point.shift(1)
    print(point.total()) # 43
    print(Point(1, 2).total()) # 3
}
```

Declare methods inside their record. The first parameter must be untyped `self`;
the compiler infers its record type. Other parameters and returns follow ordinary
function rules. Methods borrow `self` and may mutate its fields, but cannot
rebind it. Fields require `self.field`; there is no implicit field lookup.

The receiver is evaluated once, before the explicit arguments. Temporary
receivers remain alive through the call and acquisition of an owned return.
Returning `self` or an owned field copies it as for an ordinary borrowed
parameter. Receiver and argument borrows use the same overlap rules; a later
argument cannot invalidate an earlier borrowed nested receiver.

Method names must be unique within their record and cannot match its field
names. Other records and global functions may reuse those names. Methods resolve
from the receiver's record type. Static methods, extern methods, overloads,
dynamic dispatch and storing method references as values are unsupported.
Workers may call methods, but their complete call graph must still satisfy the
extern-call restriction. Direct `spawn(value.method(...))` is unsupported; use
the existing global `int -> int` worker form.

The [record guide](records.md#methods) covers copies, temporary receivers and
examples. The [integration report](status/integration.md) records the completed
method verification and its limits.

## Enums and exhaustive matches

```bhasa
enum OptionInt { none; some(value: int) }

def value_or_zero(option: OptionInt) -> int {
    match option {
        case OptionInt.none { return 0 }
        case OptionInt.some(value) { return value }
    }
}

def main() { print(value_or_zero(OptionInt.some(42))) }
```

An enum is a named type with one or more variants. Constructors use
`Type.variant(...)`; even an empty constructor needs parentheses. Variant
declarations and patterns may omit empty parentheses. Payload arguments and
bindings are positional. Variants may be separated by newlines, semicolons or
commas. Each payload needs a name and type in its declaration.

A match must cover every variant exactly once and bind all its payloads. Those
bindings belong to the case block. The scrutinee runs once; the match owns a
snapshot, and bindings independently own copied payloads. Replacing the source
enum or mutating a bound buffer cannot invalidate the other. Constructor
arguments are acquired before evaluating later arguments.

Payloads support ordinary scalar types, containers, records, enums and TCP/TLS
handles. `void`, task handles, recursive schemas and aggregate paths deeper than
128 declarations are rejected. Enum and record names share a namespace. Tags
and unchecked payload fields are inaccessible. There are no enum methods,
equality, wildcard patterns, guards, match expressions or implicit error
propagation. A statement match whose every case returns establishes a return.

The local `std` package supplies concrete `OptionInt`, `OptionText`, `ResultInt`,
`ResultText` and `ResultBuffer` enums. These are ordinary declarations, not
generic language types. The package also supplies `Option[T]`, `Result[T]`,
`Box[T]` and `option_or[T]`; see [generics](generics.md). See
[backend libraries](backend.md) for their payloads and use with text,
collections, JSON and HTTP.

Generic functions, records and enums require explicit type arguments, such as
`identity[int](42)` or `Option[buffer].some(buffer("value"))`. The Rust host
specializes these declarations before either frontend parses and checks the
result. Concrete instances follow the ordinary ownership rules. There are no
inferred type arguments, traits, bounds or method-specific type parameters.
The standalone Bhasa compiler seed does not yet perform generic lowering.

## Modules and packages

Imports precede declarations:

```bhasa
import "math.bh"
import "@utilities"
```

A file import is relative to its source file and must stay within the package
root. `@utilities` loads the entry of a declared local dependency. The manifest
is exactly lowercase `bhasa.toml`. Unaliased imports expose public types and
functions to the importing scope;
aliases provide qualified namespaces. `private` restricts declarations and
record members to their defining module.
Conflicting unaliased exports and import cycles are errors; importing
the same canonical file repeatedly loads it once. There is no implicit source
directory scan. The [package guide](packages.md) describes manifests,
scaffolding and local library creation.

The repository's `std` directory is a local package with its own `bhasa.toml`.
Declare a relative path to it under `[dependencies]`, then `import "@std"`.
Its `Text`, `StringList`, `StringIntMap` and `StringMap` records use the same
owned-copy rules as application records. The [backend guide](backend.md)
contains a complete manifest and example.

## Functions and return annotations

```bhasa
def factorial(n: int) -> int {
    if n <= 1 {
        return 1
    } else {
        return n * factorial(n - 1)
    }
}

def main() {
    print(factorial(5))
}
```

Parameters require annotations, except the inferred `self` parameter of a record
method. Calls may refer to functions declared later in the file and may recurse.
The checker verifies function names, argument counts and argument types.
Functions cannot be stored or passed as values. A local variable that shadows a
function name prevents calls to that function within its scope.

Omitting `-> type` means `-> void`. Return types are not inferred from the body. This program fails:

```bhasa
def answer() {
    return 42
}
```

Write `def answer() -> int { return 42 }` to return a value. A `void` function can fall through its closing brace or use bare `return`. A function with a non-void return type must return a value of that type on every path the checker accepts.

An `if` with an `else` guarantees a return only when both branches do. An exhaustive match does so when all its cases return. Loops do not establish that a function returns, even if the condition is always true. Add a return after the loop when a non-void function needs one. Statements after a guaranteed return are rejected as unreachable.

An entry point, when present, must be a Bhasa definition named `main`, with no parameters and return type `void` or `int`. There can be at most one. Files used as libraries or objects can omit it; building an executable still needs an entry point. The backend adds a C-compatible entry wrapper. An integer result from Bhasa `main` is converted to a 32-bit process exit value, which the operating system may further limit.

## Expressions

Integer, float, boolean and string literals, variable references, record and enum constructors,
field access, function and method calls, and parenthesized expressions are
available. Calls use positional arguments. Field access binds before the
operators below.

Operators below run from highest to lowest precedence. Binary operators within one row associate to the left.

| Operators | Operands and result |
| --- | --- |
| Unary `-`, `not`, `!` | `-` takes `int` or `float`; `not` and `!` take `bool` |
| `*`, `/`, `%` | `*` and `/` take equal `int` or `float` operands and return that type; `%` takes only `int` |
| `+`, `-` | Equal `int` or `float` operands; return that type |
| `<`, `<=`, `>`, `>=` | Equal `int` or `float` operands, `bool` result |
| `==`, `!=` | Two equal numeric types or two `bool` operands, `bool` result |
| `and`, `&&` | `bool` operands, `bool` result |
| `or`, `\|\|` | `bool` operands, `bool` result |

`float(int)` converts explicitly; `int(float)` truncates a finite in-range value
or traps. Float arithmetic follows IEEE 754. See [numbers](status/numbers.md)
for literal rules, rounding, NaN and signed zero.

`and` and `or` short-circuit. Their right operand runs only if needed. The symbolic forms `&&` and `||` have the same behavior. `not` and `!` are equivalent.

```bhasa
def main() {
    var divisor = 0
    if divisor != 0 and 10 / divisor > 1 {
        print("large quotient")
    }
}
```

The division is skipped in this example. Short-circuiting does not skip type checking of the right operand.

Integer division truncates toward zero. Remainder follows the sign of the dividend. Integer arithmetic does not wrap. Overflow in addition, subtraction, multiplication or negation traps at runtime. Division or remainder by zero also traps, as do both `INT64_MIN / -1` and `INT64_MIN % -1`. A trap terminates execution; there is no exception handler or recovery mechanism. These checks apply in development and release builds.

Integer literals use decimal digits. A decimal fraction or exponent spelling
denotes `float`. Hexadecimal notation and digit separators are unsupported. Literals outside the supported signed range produce diagnostics rather than silently wrapping. The signed minimum literal `-9223372036854775808` is accepted.

## Branches and loops

```bhasa
def main() {
    var remaining = 3
    while remaining > 0 {
        if remaining == 1 {
            print("last")
        } else {
            print(remaining)
        }
        remaining = remaining - 1
    }
}
```

`if` has an optional `else` block. `else` requires braces; to chain conditions, nest an `if` inside `else { ... }`. `while` checks its boolean condition before each iteration. `break` exits the
nearest enclosing loop; `continue` starts its next iteration. Both clean owned
locals in the scopes they leave and require tasks in those scopes to be joined.

```bhasa
for index in range(5) {
    if index == 2 { continue }
    print(index)
}
var values = List[buffer]()
values.push(buffer("one"))
for value in values { print(value) }
```

`range(end)` starts at zero; `range(start, end)` uses explicit int bounds. The
upper bound is exclusive, both bounds run once, and reversed ranges are empty.
There is no step argument or first-class range value. List iteration owns one
deep source snapshot and a separate element snapshot per iteration. Mutating or
replacing the source or binding cannot invalidate the internal cursor. Loop
bindings are mutable and body-local. Other collection kinds cannot yet be
iterated with `for`. The [iteration guide](iteration.md) specifies ownership,
cleanup, task safety and costs. Exceptions are not implemented. Enum matching
is described above.

## Strings and printing

String literals use double quotes. The supported escapes are `\n`, `\r`, `\t`, `\\` and `\"`. Use `\n` for a newline in a string; raw line breaks inside a literal are rejected. Embedded NUL bytes and other escapes are also rejected.

```bhasa
def main() {
    print("hello\nBhasa")
    print(42)
    print(true)
}
```

`print` accepts exactly one `int`, `float`, `bool`, `str` or `buffer` and appends a newline. Buffer printing preserves its bytes. It returns `void`, so it cannot initialize a variable, serve as a condition or be passed as a value. Builtin names and the `bhasa_rt_` prefix are reserved for functions, parameters and locals.

## Foreign functions

```bhasa
extern def native_double(value: int) -> int

def main() {
    print(native_double(21))
}
```

`extern def` declares a trusted C ABI function. It has no body and allows only `int` parameters and `int` or `void` results. `int` at this boundary is a signed 64-bit integer; a foreign adapter must use the matching C ABI type, such as Rust `i64`. An omitted return annotation still means `void`.

The linker needs a native input that defines the imported symbol. Supply one
with `--link` or declare a local Cargo static-library adapter under
`[rust-dependencies]` in `bhasa.toml`. Executable builds then build the adapter
offline and reuse Cargo's cached artifacts. See [caching](caching.md), the
[interop guide](interop.md) and [native link inputs](toolchain.md#native-link-inputs).
External `main` and `print` declarations are rejected. There are no overloads,
automatic Rust API imports, record/pointer parameters, callbacks or unwinding
across this boundary.

The compiler checks the declaration and call types. It does not verify the foreign implementation, its memory safety or its absence of garbage collection. The no-GC claim for the Bhasa subset does not certify arbitrary linked dependencies.

## Diagnostics and limits

The compiler checks duplicate symbols, undeclared names, assignment types, call signatures, return types, missing returns, invalid uses of `void` and unreachable statements. Unsupported syntax or types produce a diagnostic. `bhasa check FILE --json` provides diagnostics for tooling.

Owned buffers, vectors, records, enums and TCP/TLS handles have automatic cleanup
and synchronous borrowed parameters. The compiler checks the implemented record
borrow restrictions, but has no general lifetime parameters, user-defined
destructors or complete borrow checker. Structured scalar threads are described
in [concurrency](concurrency.md), synchronous TCP in [networking](network.md),
and trusted GPU host dispatch in [GPU support](gpu.md). Fixed-width integers, f32,
closures, channels, event-loop async I/O and Bhasa GPU kernels remain
unimplemented.

`tcp_listen(port)` binds IPv4 loopback. `tcp_listen_on(host, port)` accepts a
numeric IPv4 or IPv6 `str`/`buffer`, including an explicit wildcard address.
Ports range from 0 through 65535; zero asks the OS for a free port. Unlike
`tcp_connect`, explicit bind does not resolve host names. Empty, binary,
NUL-containing or malformed addresses return an owned error handle. Check
`tcp_listener_status` before accepting. All TCP operations are synchronous;
accept/connect/read/write also have deadline variants. Timed connect accepts
numeric addresses only. The [network guide](network.md) specifies status values,
deadline ranges and stream behavior.

`bhasa fmt FILE` formats source; `--check` reports whether changes are needed.
`bhasa lsp` serves editor diagnostics, formatting, function/record/enum symbols,
nested fields/methods/variants, and definitions for checked calls, case variants and field access over
stdio. CLI JSON spans use bytes and one-based character
positions; LSP uses zero-based UTF-16 positions. See [tooling](tooling.md) for the
protocol and the saved-import-graph boundary.

Explicit [generics](generics.md), the [bounded task executor](concurrency.md),
and owned verified [TLS sockets](tls.md) are available in both host frontends.
TLS copies share a serialized session; they do not create independent streams.
`async_spawn(worker(value))` runs an `int -> int` worker on a pool of four
threads with space for 64 waiting jobs. Submission blocks when the queue is
full, and `await(task)` blocks its calling thread until the task finishes.
Workers cannot reach `async_spawn`, `await` or extern functions. This executor
does not provide coroutines, async I/O or shared aggregate arguments.
