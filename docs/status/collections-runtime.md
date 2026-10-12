# Native generic collection runtime

`runtime/collections.c` implements the native storage behind specialized
`List[T]` and `Map[K, V]`. The compiler supplies each slot's size and typed
clone/drop callbacks. Containers own their slots and release them deterministically;
there is no garbage collector. Slots have at least eight-byte alignment.

The ABI in `runtime/collections.h` provides:

- Lists: `new`, `clone`, `free`, `count`, `get`, `push`, `set`, `clear`.
- Maps: `new`, `clone`, `free`, `count`, `get`, `put`, `contains`, `remove`, `clear`.
- Integer and string key hash/equality helpers.

All symbols have the `bhasa_rt_` prefix. Map creation also receives key hash and
equality callbacks. `get` clones into caller-owned, uninitialized output storage;
no pointer into a container escapes. The compiler's typed wrappers return that
owned value. List writes and map insertion/replacement snapshot their inputs
before reallocating or dropping existing slots, including aliased inputs.
Cloning follows each value's ownership rules: buffers and aggregates copy their
owned contents; network handles retain their existing session semantics.

Lists grow geometrically. Maps use open addressing with linear probing and
reclaim deleted slots. Growth checks arithmetic overflow. Missing map keys,
invalid list indices, invalid callbacks or slot sizes, and allocation failure
stop the program with a runtime error. Failed operations do not partially mutate
the container. `remove` reports whether a key existed. `clear` drops all stored
values while leaving the container reusable.

This initial source interface accepts `int` and `str` map keys. It does not add
iterators, user-defined hash callbacks, concurrent mutation, or stable references
into containers. Native callbacks are trusted compiler output and must not
reenter or mutate their container. Hashing is deterministic; this implementation
does not claim resistance to adversarial hash collisions.

## Verification

`tests/fixtures/collections/runtime.c`, driven by `tests/collections_runtime.rs`,
checks nested owned slots, independent copies, clone/drop accounting, list growth,
map replacement, forced collisions, tombstone reuse, and repeated cleanup. It
also exercises interior input aliases during replacement and growth. A counted
allocator verifies zero remaining allocations, including allocations owned by
nested slot values. Ten isolated failure modes check diagnostics and unchanged
container contents before process exit.

Direct C11 builds passed at O0 and O2 with `-Wall -Wextra -Werror -pedantic`,
AddressSanitizer and UndefinedBehaviorSanitizer. Both normal runs and all ten
failure modes passed at each optimization level. These are native runtime
checks; they do not by themselves establish frontend parity or cross-target
support. Root coordinates the Cargo integration and language-level suites.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test collections_runtime -- --test-threads=1
```

The Cargo runtime fixture and all three language-level collection groups pass
in complete reference/full-hybrid acceptance. Both compilers retain typed
wrappers and deterministic ownership. Logs use build/verify-foundation-reference.log
and build/verify-foundation-hybrid-frontend.log.
