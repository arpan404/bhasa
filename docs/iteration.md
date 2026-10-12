# Iteration

Use a half-open integer range for counted loops:

```bhasa
for number in range(5) { print(number) }       # 0 through 4
for number in range(-2, 2) { print(number) }   # -2 through 1
for number in range(6, 0, -2) { print(number) } # 6, 4, 2
```

`range(end)` starts at zero. `range(start, end)` starts at the first bound.
Both forms use a step of one. `range(start, end, step)` selects a signed step.
All arguments must be `int`; they evaluate once, left to right, before iteration.
A positive step visits values less than end; a negative step visits values
greater than end. Equal bounds or bounds facing away from the step are empty.
Zero step traps even for an empty range. The internal cursor advances
independently of changes to the loop binding. Advancement that would overflow
ends iteration, allowing a final step past the signed limit. There is no
first-class range value.

Use `for value in list` with `List[T]`:

```bhasa
var names = List[buffer]()
names.push(buffer("first"))
names.push(buffer("second"))
for name in names {
    names.clear()
    append(name, "!")
    print(name)
}
```

This prints both original names with an exclamation mark. The loop evaluates
its source once and acquires a deep snapshot. Each iteration owns a separate
element snapshot. Clearing, replacing or extending the source list cannot
invalidate iteration. Mutating a yielded buffer or aggregate cannot alter the
source or another iteration. A temporary returned list transfers its ownership
to the loop. Snapshot creation costs time and memory proportional to the stored
values; this is not a borrowed view. Map, buffer and ints iteration are not
supported by this syntax.

The binding is mutable and visible only inside its body. It follows ordinary
shadowing rules, and its ownership ends on each iteration.

`break` exits the nearest enclosing `while` or `for`. `continue` starts its next
iteration. Both statements destroy owned locals in every scope they leave.
A list snapshot survives `continue` and is destroyed on `break`, `return` or
normal completion. Nested branches and enum matches follow the same cleanup.

Task handles created in an exited scope must already have been consumed with
`join` or `await` in their creation block. Exiting a loop does not leave the
surrounding scope, so an outer task can remain live and be joined afterward.
Worker calls inside bounds, steps, list sources and bodies retain the normal transitive
extern and pooled-task restrictions.

The compiler rejects `break` or `continue` outside a loop and statements after
a guaranteed return or loop exit. A loop can perform no iterations, so it never
proves that a non-void function returns; add a return after it.

Run [the example](../examples/basics/iteration.bh) with:

```sh
bhasa run examples/basics/iteration.bh
```

Its output is `12`, `first!`, `second!`, `0` and `3`, one value per line.
