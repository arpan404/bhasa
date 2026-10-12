# Lists and maps

Use `List[T]` for a variable-length sequence and `Map[K,V]` for key-based lookup. Types are explicit. Both containers allocate native storage and free their contents when ownership ends; they do not use a garbage collector.

```bhasa
struct Entry {
    title: buffer
    score: float
}

def main() {
    var entries = List[Entry]()
    entries.push(Entry(buffer("first"), 2.5))
    entries.push(Entry(buffer("second"), 4.0))
    print(entries.count())
    print(entries.get(1).title)

    var scores = Map[str, float]()
    scores.put("first", 2.5)
    if scores.contains("first") {
        print(scores.get("first"))
    }
}
```

## List operations

| Operation | Result | Behavior |
| --- | --- | --- |
| `List[T]()` | `List[T]` | Create an empty list |
| `values.push(value)` | `void` | Append a copy of `value: T` |
| `values.get(index)` | `T` | Read an independent snapshot at `index: int` |
| `values.set(index, value)` | `void` | Replace one element and clean up the old value |
| `values.count()` | `int` | Return the number of elements |
| `values.clear()` | `void` | Remove and clean up every element |

Indices start at zero. An index below zero or at least `count()` stops the program. `set` replaces an existing element; use `push` to append.

## Map operations

| Operation | Result | Behavior |
| --- | --- | --- |
| `Map[K,V]()` | `Map[K,V]` | Create an empty map |
| `values.put(key, value)` | `void` | Insert a copy, or replace the value for an existing key |
| `values.get(key)` | `V` | Read an independent snapshot |
| `values.contains(key)` | `bool` | Check whether the key exists |
| `values.remove(key)` | `bool` | Remove and clean up the entry; return whether it existed |
| `values.count()` | `int` | Return the number of entries |
| `values.clear()` | `void` | Remove and clean up every entry |

Keys must be `int` or `str`. String keys compare their UTF-8 contents. A missing `get` stops the program; use `contains` when absence is expected. Calling `put` twice with the same key updates one entry.

## Copies and mutation

Insertion and retrieval copy owned values. A retrieved buffer or record survives later changes to the container.

```bhasa
var values = List[buffer]()
var original = buffer("hello")
values.push(original)
set(original, 0, 88)

var saved = values.get(0)
values.clear()
print(saved) # hello
```

Assigning a collection to another variable also makes an independent copy. Passing it as a function argument borrows it, so a function can mutate that collection through its methods. Existing borrow checks reject argument combinations that could invalidate a borrowed record field during evaluation. Socket and listener values keep their usual shared-handle semantics when copied.

Nested collections and concrete generic functions work:

```bhasa
def singleton[T](value: T) -> List[T] {
    var result = List[T]()
    result.push(value)
    return result
}

def main() {
    var groups = Map[str, List[buffer]]()
    groups.put("greeting", singleton[buffer](buffer("hello")))
}
```

## Current limits

Elements and map values can be scalars, buffers, integer arrays, records, enums, handles, or other concrete collections. `void`, task handles, and recursive schemas are rejected. The existing aggregate-depth and generic-instantiation limits apply.

There are no collection iterators, borrowed element views, fixed arrays, dynamic buffer/Text keys, user-defined hashing, or capacity controls yet. Lists can be traversed with an integer loop and `count`/`get`. Map iteration and ordering are not part of the current contract. Allocation failures stop the program.

Collections are not synchronized. The current worker API accepts and returns integers, so collections remain local to a worker or to its calling thread. Worker-local collections are supported.

See [the complete example](../examples/collections-generic/main.bh) and [implementation and verification status](status/collections-generic.md).
