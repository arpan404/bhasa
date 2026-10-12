# Buffer and integer-vector ownership

Ordinary buffer and ints parameters borrow their caller's storage for a synchronous
call. A function can change the contents, but cannot rebind the borrowed handle.
Ordinary assignments and returns make independent copies.

Use an `owned` parameter to hand cleanup responsibility to a function. A named
owner needs `move`; a newly created value can be passed directly.

```bh
def decorate(owned bytes: buffer) -> buffer {
    append(bytes, "!")
    return move bytes
}

def main() {
    let input = buffer("hello")
    let output = decorate(move input)
    print(output)
}
```

The transfer makes no clone or allocation. The destination frees the value once
its scope ends. Reading or moving `input` after the call is a compile error.
A mutable `var` can be reinitialized in the same straight-line scope; a `let`
cannot be assigned again.

`move name` is allowed as a whole local initializer, an owned argument or a whole
return value. Arguments evaluate left to right. Borrowing and moving the same
owner in one call is rejected in either order. Ordinary parameters and returns
keep their existing behavior.

This first transfer feature supports buffer and ints, including their transparent
aliases, in monomorphic synchronous free functions. Independently acquired match
payloads of these types can also move. Aggregate containers, methods, generic
owned parameters, tasks and file/TCP/TLS handles do not support this transfer syntax.
Resource copies keep their documented resource-specific behavior.

An owner declared outside an `if`, match arm or loop cannot move from inside that
control-flow scope. A value created within the scope can move there. These rules
keep cleanup deterministic without introducing path-dependent ownership.

`owned` and `move` are contextual words; ordinary functions and binders with those
names remain valid. Invalid transfers report S038 in the reference frontend and
BH222 in the Bhasa frontend. Immutable assignment and pending Results have their
separate diagnostics. Explicit read-only/mutable borrow modes and general lifetime
analysis remain separate features.
