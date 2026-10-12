# Rust frontend collection iteration

The reference frontend accepts `for` over buffers, integer arrays, Lists and
Maps through `ForSource::Iterable`. Ranges retain their separate representation.
Buffers yield integer bytes from 0 through 255, integer arrays yield `int`, and
Maps yield their declared `int` or `str` key type. Map ordering is unspecified.

Each source runs once. A borrowed source is cloned before entering the body;
an owned temporary becomes the retained snapshot. Map snapshots include their
owned values. Loop bindings are independent mutable locals. Original-container
mutation cannot change traversal.

The existing private loop scope retains the snapshot across `continue`, frees
it at the shared exit for `break`, and participates in return cleanup. Map
conditions call the internal `bhasa_rt_map_next_key` ABI using separate aligned
eight-byte cursor and output slots. The output is loaded only on success;
exhaustion never reads uninitialized storage. Map cursor advancement belongs to
the runtime scan. Sequence iteration uses the existing length/get APIs.

Semantic checking validates source and element types. Source expressions remain
part of aggregate remapping, checked-type traversal and transitive worker-call
analysis. Existing lexical task-consumption rules apply inside these loops.
The Map scan ABI is not a source-level builtin.

## Verification status

Implementation and focused acceptance are ready for the coordinator's gate.
No Cargo checks have run in this lane. `rustfmt` has run on the owned Rust files.
`tests/collection_iteration_reference.rs` contains four groups:

- Source/binding types, loop scope, invalid sources and corrupted checked ASTs.
- Worker call graphs, pool restrictions and lexical task consumption.
- Bounded native O0/O2 sequence and key-set oracles, including empty containers,
  bytes above 127, signed integer limits, deletion/replacement, original-source
  mutation and reassigned loop bindings.
- O0/O2 ASan/UBSan with atomic allocation tracking for nested owned Map values,
  sources evaluated once, nested loops, early exits and typed tasks.

Combined frontend, release, self-host and cache checks remain coordinator gates.
Capacity controls, fallible lookup, destructuring and borrowed/custom iterators
are outside this increment. The complete backlog remains in
[todo.md](../../todo.md).
