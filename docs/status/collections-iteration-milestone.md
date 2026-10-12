# Snapshot iteration for buffers, integer lists and Map keys

Integrated and verified increment from `todo.md`. It follows the integer/stepped-range/editor integration. Preserve both frontends,
deterministic no-GC ownership, `.bh`, lowercase `bhasa.toml` and all caches.

## Contract

Extend the existing loop syntax without introducing borrowed iterators or
destructuring:

```bhasa
for byte in data { print(byte) }       # buffer yields int bytes, 0 through 255
for value in numbers { print(value) }  # ints yields int values
for key in scores { print(key) }       # Map yields its declared int/str key type
```

Evaluate the source exactly once and hold an independent owned snapshot before
the first body execution. Original-container mutation and reassignment of the
body-local loop binding cannot change traversal. Each snapshot element or key
is visited exactly once. Map order is unspecified. Existing List/range behavior,
continue/break/return cleanup, lexical task consumption and transitive worker
restrictions remain enforced.

## Implementation seams

Generalize the checked non-range iteration AST and its element type. Update all
host/module/generic/editor visitors. Buffer/ints use existing clone/length/get
runtime APIs. Map iteration needs an internal cursor/key primitive that clones
a key into uninitialized output on success, advances its cursor, and leaves
output untouched on exhaustion. Invalid cursors/output pointers trap. It is not
a public source-level builtin. Retaining the existing deep Map snapshot copies
values too; removing that cost requires a separate ownership optimization.

## Acceptance

Compare both frontends at O0/O2 against sequence/key-set oracles. Cover empty
containers, bytes above 127, integer extremes, Map deletion/replacement, source
evaluation once, mutation of the original, mutable bindings, nesting and early
exits. ASan/UBSan and allocation counters must cover owned Map values and typed
tasks. Reject invalid source types and corrupted checked-AST element types.
Run focused checks during implementation; freeze sources before combined
suites, refreshed releases, A/B/C and unchanged cache-reuse verification.

Capacity/reservation guarantees, fallible lookup, pair destructuring and a
custom/borrowed iterator protocol remain separate unchecked backlog entries.

## Ownership and progress

packages owns Rust AST/checking/emission and reference acceptance; frontend_parity
owns native emission and parity acceptance; rust_cache owns the Map cursor ABI
and independent C guards. Root owns host visitors, independent integration,
guides and verification. Cargo operations remain serialized with two build jobs
and one test thread. All lanes preserve caches and defer Git writes to root.

## Integration result

All acceptance criteria pass: 441 reference / 454 full-hybrid tests, 56 LSP /
80 editor checks, both Clippy configurations, formatting, refreshed releases,
A/B/C and unchanged native/Rust cache reuse. integration.md records independent
review repairs, exact logs and platform limits. Sources are frozen for commit.
