# Typed collections

`List[T]` and `Map[K,V]` are compiler-supported generic containers with native variable-length storage. The shared generic expansion pass assigns each concrete specialization a nominal type; each frontend checks calls and emits typed LLVM wrappers and element clone/drop callbacks. No garbage collector, tracing, runtime reflection, or serialized element representation is used.

```bhasa
var values = List[buffer]()
values.push(buffer("hello"))
var snapshot = values.get(0)
values.clear()
print(snapshot)

var scores = Map[str, float]()
scores.put("first", 2.5)
print(scores.get("first"))
```

| Container | Methods |
| --- | --- |
| `List[T]` | `push(T)`, `get(int) -> T`, `set(int,T)`, `count() -> int`, `clear()` |
| `Map[K,V]` | `put(K,V)`, `get(K) -> V`, `contains(K) -> bool`, `remove(K) -> bool`, `count() -> int`, `clear()` |

Constructors take zero arguments. Map keys are `int` or `str`; strings are immutable, static-lifetime UTF-8 values. Maps compare string contents. Dynamic buffer/Text keys, iteration, capacity APIs, slices/views, and insertion-order guarantees are absent in this milestone. Fixed arrays are not implemented.

Elements and values may be ordinary concrete scalars, buffers, integer arrays, records, enums, handles, or other concrete collections. `void`, task handles, recursive schemas, and schemas beyond the existing aggregate depth limit are rejected. Generic expansion retains its existing instantiation and source-size limits. Internal storage fields and generated type names cannot be used from source.

Assignments, returns, insertion, replacement, and `get` obey the existing owned-value rules. Each `get` produces an independent owned snapshot, copied into a caller-owned typed slot before the native call returns. Borrowed parameters can mutate a collection through its methods. Existing aggregate argument-alias checks apply, including calls that replace a receiver through another argument. Handles retain their existing reference-counted clone behavior; a cloned socket still refers to the same OS connection.

Invalid list indices and missing map keys stop the program. Use `contains` before a map read when absence is expected. Replacement drops the old value after copying the new one; `remove` reports whether the key existed. Allocation failure and size overflow stop the program. Cleanup is deterministic and recursive through compiler-generated callbacks. Collections themselves are not synchronized and cannot be passed to workers under the current `int -> int` worker contract. Worker-local collections work, and calls through collection arguments retain transitive extern/scheduler restrictions.

Both emitters request `runtime/collections.c` with the `; bhasa runtime: collections` marker. The host driver supplies that runtime when linking. Cross-target linking still needs a compatible target C toolchain and libraries.

Verification is in `tests/collections_generic.rs`: concrete typing and schema rejection; native O0/O2 execution through the reference and fresh Bhasa seed, with AddressSanitizer and UndefinedBehaviorSanitizer; nested owned snapshots and growth; bounds and missing-key failures; worker restrictions; and an imported generic helper through the driver. The focused reference run passed all three tests, including fresh Bhasa seed execution, and the standalone lexer parity test passed. Evidence: `build/verify-foundation-collections.log`. An additional receiver-invalidation case added after that run was checked directly against the existing reference CLI and fresh Bhasa seed; both rejected it with S028/BH214. The native reference example and a nested map/list/buffer seed smoke also passed.

Complete reference/full-hybrid acceptance passes these three groups. The public
template_program tooling helper now registers collection intrinsics, so unused
generic functions returning List/Map retain editor shapes. A direct regression
is included in the concrete/schema group.
