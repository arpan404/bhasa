# Records

Declare a record with `struct`, then construct it by passing one value for each
field in declaration order. Field declarations use newlines or semicolons.

```bhasa
struct Point {
    x: int
    y: int
}

struct Line { start: Point; end: Point }

def main() {
    var point: Point = Point(2, 3)
    var line = Line(point, Point(10, 20))
    line.start.x = 42
    print(point.x)      # 2
    print(line.start.x) # 42
}
```

Read and assign fields with `value.field`. Nested fields use repeated dots.
Every record has its own type. Two records with identical fields still have
different types. Constructors require the exact field count and field types;
there are no implicit conversions or default field values.

Fields can hold `int`, `float`, `bool`, `str`, `buffer`, `ints`, `tcp_socket`,
`tcp_listener`, `tls_socket`, concrete `List[T]` or `Map[K,V]`, another record,
or an enum.
Declarations can refer to aggregate types that appear later in the file or in an
imported module. Aliased imports isolate same-named types from different
modules; conflicting unaliased public exports remain errors. `void` and `task` fields are
invalid. Direct and indirect recursive layouts are invalid too. A layout
may contain at most 128 nested aggregate types, including mixed record/enum paths.

## Copies and borrowed parameters

Constructors, bindings, and assignments copy existing record values. A copy
includes independent nested records, active enum payloads, byte buffers, and integer vectors.
Changing a copied field or container cannot change its source. Copying large
buffers has a cost; this compiler does not promise copy elimination.

Copying a TCP handle atomically retains shared descriptor state for the same
operating-system socket or listener. Copies share connection and network state
while preserving independent owner lifetimes. Dropping one owner leaves other
owners usable; final release closes the descriptor once. A record copy creates
no new connection or descriptor and cannot change a successful TCP constructor
status through descriptor exhaustion.

Copies of a `tls_socket` retain one shared TLS session with serialized I/O;
the last owned copy releases it. See [TLS ownership](tls.md).

Record parameters borrow the caller's value for the duration of the call.
A function can change its fields, including replacing a nested record or a
buffer field. It cannot assign a new record to the parameter itself.

```bhasa
struct Point { x: int; y: int }

def shift(point: Point, dx: int) -> Point {
    point.x = point.x + dx
    return point
}

def main() {
    var point = Point(2, 3)
    var moved = shift(point, 5)
    moved.x = 100
    print(point.x) # 7
    print(moved.x) # 100
}
```

Returning a borrowed record or one of its owned fields makes an independent
owned copy. A field extracted from a temporary record remains valid after that
temporary is destroyed. Assignment requires a field rooted in a local variable
or parameter. An expression such as `Point(1, 2).x = 3` is invalid.

A call cannot mutably borrow a record and one of its descendants at the same
time. It also cannot borrow the same record twice. For example,
`update(line, line.start)` is rejected when both arguments are borrowed record
parameters. Disjoint fields such as `update(line.start, line.end)` can be
borrowed together. Constructors copy their arguments, so
`Line(point, point)` creates two independent points.

Arguments are evaluated before the called function starts. A call such as
`consume(box, len(box.text))` can read a field while preparing a later argument.
Likewise, `consume(box, Wrap(box))` can construct an independent copy for a
later argument. Nested constructors and buffer operations follow the same rule.

A later argument cannot call a function that might replace a field already
borrowed by an earlier argument. For example,
`use_text(box.text, replace(box))` is rejected because `replace` can assign a
new buffer to `box.text`. Wrapping that call inside `len`, another function,
or a constructor does not make it safe. The checker treats every record
parameter as mutable; it does not infer whether a particular function only
reads the record. Thus `use_text(box.text, inspect(box))` is also rejected even
if `inspect` only reads fields. Borrowing `box` itself in
`consume(box, inspect(box))` or `consume(box, replace(box))` is valid because
the function can replace fields but cannot rebind the borrowed record handle.

The compiler destroys owned records and their fields at scope exit, including
early returns and each loop iteration. Reassignment destroys the previous
value after the replacement has been evaluated. Self-assignment is valid.
Applications need no garbage collector and expose no raw pointers.

## Run the examples

From the repository root:

```sh
cargo run -- run examples/records/points.bh
cargo run -- run examples/records/resources.bh
```

The points example prints `7`, `10`, `100`, and `2` on separate lines. The
resources example prints `Ada!`, `Grace`, `10`, `20`, `2`, and `returned`.

Run `cargo test --test records` for parser, type checking, module imports,
native execution, and ownership checks. On macOS and Linux with Clang
installed, the ownership test uses AddressSanitizer and UndefinedBehaviorSanitizer
at `-O0` and `-O2`. It also counts live record and container allocations at exit.
A missing Clang prints a native-test skip; a broken compiler or sanitizer
failure fails the test.

Records have named fields, positional constructors and methods. Explicit
[generic records](generics.md) specialize their fields and methods at compile
time. Use `private` for module-scoped fields and methods. Inheritance and record
equality are not supported.


## Methods

Declare methods inside a record. The first parameter must be exactly `self`,
without a type annotation. The compiler infers its enclosing record type.
Remaining parameters and return types follow ordinary function rules.

```bhasa
struct Point {
    x: int
    y: int
    def total(self) -> int { return self.x + self.y }
    def shift(self, amount: int) { self.x = self.x + amount }
    def copy(self) -> Point { return self }
}

def main() {
    var point = Point(20, 22)
    print(point.total()) # 42
    point.shift(1)
    print(point.total()) # 43
}
```

Methods borrow their receiver mutably and cannot reassign `self`. Refer to fields
through `self.field`. Forward calls and recursive methods are supported. A method
can have the same name as a global function or a method in another record. Within
one record, method names must be unique and cannot match field names. Static
methods, overloads, extern methods and method values are unsupported.

The receiver is evaluated once, before the explicit arguments. Temporary receivers
remain alive until the method finishes. A returned receiver or owned field becomes
an independent copy, so `Message(buffer("temporary")).bytes()` remains valid after
the temporary message is destroyed. Chained calls such as `point.copy().total()`
use the same rules.

The receiver counts as the first borrowed argument for alias checks.
`record.consume(record.field)` is rejected if the method could replace that owned
field while borrowing it. A later argument may read the receiver's fields or copy
them. It may mutate the receiver itself because its handle remains valid, but it
cannot replace an ancestor of a borrowed nested receiver. For example,
`parent.child.use(parent.replace_child())` is rejected.

Worker safety checks follow calls through methods, including recursive method
graphs. A worker that can reach an extern function through a method is rejected.
Direct `spawn(value.method(...))` is unsupported. A global typed worker can
receive independently copied records and return owned results under the existing
worker rules. See [concurrency](concurrency.md) for supported signatures and
handle consumption.

Run `bhasa run examples/records/methods.bh` for field mutation, copies and temporary
receivers. It prints `42`, `43`, `52`, and `3`. The
`examples/records/methods-resources.bh` example prints `hello!`, `Hello!`, `hello!?`,
and `temporary`. `cargo test --test methods -- --test-threads=1` checks parsing,
types, imports, worker safety and native receiver cleanup under ASan/UBSan.
