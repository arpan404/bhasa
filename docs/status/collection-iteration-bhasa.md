# Collection snapshot iteration in the Bhasa frontend

The native emitter accepts `for value in expression` over `buffer`, `ints`,
`List[T]` and `Map[K,V]`. Buffers yield integer bytes from 0 through 255;
integer lists yield `int`; Maps yield their declared `int` or `str` key type.
Map order is unspecified. Existing range syntax and List iteration are retained.

The source expression executes once. Borrowed containers are deeply copied;
exclusive owned temporaries transfer into the loop's hidden snapshot binding.
Rebinding or mutating the original cannot affect traversal. The mutable visible
binding belongs to each iteration and cannot change the hidden cursor. Map
snapshots copy values too, including owned records, enums and nested collections.

Buffer and integer-list loops use the existing length/get runtime functions.
Map loops call the internal `bhasa_rt_map_next_key(snapshot,cursor,output)`
function from the loop condition. Cursor and output occupy separate, explicitly
eight-byte-aligned slots. Output is loaded only after the scan reports success.
Map continue returns to the scan without adding to its runtime-managed cursor.
The function is not a public language builtin.

Ordinary block cleanup destroys body values before continue or break. The
hidden snapshot survives continue and is destroyed on break, return or normal
exhaustion. Existing lexical task consumption and transitive extern/pool worker
checks apply to the source expression and body. This change adds no throwing,
unwinding, pair destructuring, borrowed iteration or custom iterator protocol.

Implementation is confined to `bootstrap/compiler/emitter.bh`; tokens and the
native compiler API are unchanged. Direct map scans select the collections
runtime through its existing generated-LLVM marker.

## Lane evidence

An existing reference release built a fresh native seed on arm64 macOS under
`build/collection-iteration-native`. The native fixture passed O0 and O2 with
ASan on generated LLVM, ASan/UBSan on runtime code and an atomic allocation
counter covering core, collection, task and generated aggregate allocations.
Every run exited successfully with zero live tracked allocations.

Coverage includes empty containers, bytes 0/127/128/255, signed integer extrema,
Map growth and deleted buckets, key-set preservation while removing/replacing
or rebinding the original, once-only source calls, mutable iteration bindings,
early returns, break/continue, owned Map values, nested collections and typed
pool workers using nested classic tasks. Fourteen invalid programs reject wrong
source/binding types, unconsumed or cross-scope tasks, transitive worker externs
and pool operations with BH201/BH208/BH209 and no emitted LLVM.

A fresh A/B/C self rebuild produced byte-identical compiler LLVM. Stages A and B
also produced identical fixture LLVM. Local evidence is in
`build/collection-iteration-native/probe.log`, `stage-b.ll` and `stage-c.ll`.
These standalone probes supplied concrete generated collection schemas directly;
they do not independently verify the host generic-expansion seam.

`tests/collection_iteration_bhasa.rs` adds shared generic expansion, reference
and native comparison, O0/O2 sanitizer/allocation checks, exact native diagnostic
code assertions, self rebuild and optional hybrid acceptance. The coordinator
owns Cargo runs, refreshed hybrid archives and combined integration results;
those results are not established by the lane probes above.
