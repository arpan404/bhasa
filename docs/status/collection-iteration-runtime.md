# Map snapshot iteration runtime

`runtime/collections.c` adds an internal compiler ABI for iterating an immutable
owned Map snapshot:

```c
int64_t bhasa_rt_map_next_key(
    const bhasa_map *snapshot, int64_t *cursor, void *output);
```

The cursor starts at zero. The function scans occupied buckets, clones one key
into uninitialized output storage, advances the cursor and returns one. It skips
empty buckets and tombstones. Exhaustion sets the cursor to capacity, returns
zero and leaves output untouched, including on repeated calls. Order is
unspecified. Independent cursors are supported; the snapshot cannot be mutated
while either cursor is in use.

The compiler owns the snapshot and each successful key clone. It must load or
drop the output only after a successful call. Cursor and key output slots are
independent and eight-byte aligned. The output has room for the padded key slot,
which is eight bytes for the current `int` and `str` source key types. An explicit
eight-byte slot also accommodates pointer keys on 32-bit targets. This runtime
change does not expose a public source builtin or add iterator allocations.

Before reading the cursor or invoking a clone callback, the runtime checks null
and misaligned storage, address-range overflow, and overlap with Map metadata,
bucket storage, keys and values. Cursor/output overlap is rejected, including
partial overlap. Cursor values outside zero through capacity stop the program.
Validation failures leave the container, cursor and output unchanged. Existing
collection entry points and ownership callbacks retain their ABI.

This is a trusted compiler interface. Callers must supply live writable storage
of the required size; arbitrary dangling or forged non-null pointers cannot be
validated by this C ABI. Clone/drop callbacks must not mutate or reenter the
container. The frontend's existing deep Map snapshot copies values as well as
keys. Borrowed iterators, pair destructuring, mutation during iteration and
allocation-failure recovery are outside this increment.

## Native evidence

`tests/collection_iteration_runtime.rs` embeds a C fixture that includes the real
runtime under an instrumented allocator. Direct strict C11 builds passed at O0
and O2 with `-Wall -Wextra -Werror -pedantic`, AddressSanitizer and
UndefinedBehaviorSanitizer. At each optimization level the fixture passed:

- Integer and string key-set oracles, including empty strings, UTF-8 strings,
  signed integer extremes, replacement, collisions, growth and tombstones.
- Empty, allocated-empty and tombstone-only maps, repeated exhaustion without
  output writes, independent cursors and early termination.
- Original-map mutation and destruction after cloning, independently owned
  values and mutable output keys. Repeated cleanup balances every key/value
  clone and drop and leaves zero counted allocations.
- Twenty isolated rejection cases covering null inputs, cursor bounds,
  misalignment, address overflow and all container/cursor/output alias classes.
  Exit handlers verify unchanged state and release all allocations.

The unchanged collection fixture also passed normal operation and its ten
failure modes at O0/O2 with ASan/UBSan against the updated runtime. These direct
checks ran on macOS ARM64. Linux, other targets and full frontend acceptance are
not established by this lane's probes. The coordinator owns serialized Cargo
and combined acceptance; runtime sources are frozen pending those results.

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --locked --test collection_iteration_runtime --test collections_runtime -- --test-threads=1
```
