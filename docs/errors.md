# Error handling

Recoverable failures are values. File I/O returns `IoResult[T]`, containing
`ok(value)` or `error(Error)`. Existing Result[T], HTTP and JSON enums keep their
existing payload types. Use an exhaustive match to choose recovery behavior, or
propagate a compatible error from a function returning a result:

```bhasa
import "@std" as io

def file_count(path: buffer) -> io.IoResult[int] {
    var contents = try io.file_read(path)
    return io.IoResult[int].ok(len(contents))
}
```

This assumes the local std dependency in lowercase bhasa.toml. The
[file example](../examples/errors/main.bh) accepts a path argument:

```sh
bhasa run examples/errors/main.bh -- examples/errors/sample.txt
```

The successful example prints `6`. Missing files return an Error carrying the
native code, operation and owned Text message. [Typed I/O](io.md) explains byte
paths, write failure behavior and the private Error API.

`try` evaluates its operand once. Success binds an independently owned payload;
failure returns the enclosing function's error variant after normal owned
cleanup. The caller's success payload may differ. Both enums must have exactly
two one-payload variants named ok and error; their concrete error types must be
identical. Declaration order does not matter, and names such as Result are not
special. Private enums cannot be implicitly unpacked outside their module.

This first form is limited to an entire `var` or `let` initializer. An annotation
specifies the success type. Use `let value = try operation()` or
`var value = try result`.
General nested expressions, assignments, return operands and parenthesized
propagation are not supported. `try` remains a contextual identifier, preserving
ordinary calls such as try(value), try[T](value), member access and arithmetic
on a variable named try. The marker and identifier operand cannot be separated
by a newline.

Propagation can return early. Any live task other than the one consumed by a
direct `try join(task)` or `try await(task)` rejects the statement. Nested loop
and match snapshots, payloads and locals are destroyed on the error return;
normal success retains their scopes.

The compiler rejects immediately discarding a result-shaped expression,
including a call, getter, constructor, result variable or joined task result.
The existing integer status builtins also reject direct discard. A local binding
holding a result-shaped enum must be handled before a reachable scope exit,
return, break/continue or overwrite. Direct exhaustive `match binding`,
`return binding` and entire-initializer `try binding` discharge that binding.
Copies have separate obligations. Reading a value, passing it to a function or
storing it inside a container does not discharge the original binding.

```bhasa
def show_file(path: buffer) {
    let outcome = io.file_read(path)
    match outcome {
        case io.IoResult[buffer].ok(contents) { print(contents) }
        case io.IoResult[buffer].error(problem) { print(problem.code()) }
    }
}
```

Every continuing branch must handle the value. Handling an outer result inside
a loop does not prove handling after the loop, which may run zero times. Result
assignment to an outer binding from a loop is conservatively rejected; bind and
handle an iteration-local result instead. Propagation also checks every other
live result at its possible error return. Rust reports S037; Bhasa reports BH221.

This local rule deliberately excludes parameter obligations, field/container
tracking, directly nested argument results and interprocedural handling summaries.
In particular, an opaque private result bound by a caller cannot be discharged
by passing it to a public handler. The defining module may expose a public
result wrapper; an inline `handler(factory())` remains outside this local rule.
Assigned scalar status values are also unchanged. Full passed/aggregate failure
safety remains an open backlog item. Fatal traps remain fatal.

There are no thrown exceptions or try/catch. Integer overflow, invalid shifts,
bounds failures, invalid runtime tags/pointers and allocation failure stop the
process through traps or fatal exits. `try` handles returned error values and
cannot recover these faults. Fatal termination does not unwind or run ordinary
owned cleanup. A public panic() API and catchable panic mechanism are absent.
