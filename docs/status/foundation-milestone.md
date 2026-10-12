# Namespaces, types, testing and native build cache milestone

Approved October 11, 2026: implement the four next foundations recommended in
the integration chat. Preserve `.bh`, exactly lowercase `bhasa.toml`, deterministic
no-GC ownership, Rust host/Bhasa frontend, existing APIs and persistent caches.
No installs, cache cleaning, commits, resets or publishing.

## Contracts

- `import "file.bh" as math` and `import "@pkg" as math`; qualified calls/types
  and generic applications. Public by default; explicit `private` declarations,
  fields and methods. Existing unaliased graphs preserve their behavior.
  Modules lower in the Rust host before both frontends. Private member access
  uses an additional shared typed Rust-host verifier; native checking/emission
  still execute independently. C ABI extern symbols retain their names.
- `float` is binary64, explicit `float(int)` and checked truncating `int(float)`.
  No implicit numeric conversion or floating-point remainder. Narrow integer
  types and f32 are future work.
- `List[T]()` and `Map[K,V]()` specialize into opaque nominal collections.
  Lists: push/get/set/count/clear. Maps: put/get/contains/count/remove/clear.
  Keys are int or str. Typed deterministic clone/drop callbacks; reads and writes
  snapshot owned values. Invalid indices/missing keys trap. No GC, runtime type reflection, recursive aggregates or callable captures. Existing worker safety
  restrictions apply through intrinsic collection calls.
- `bhasa test`: entry-file `def test_name() -> bool`, native isolated processes,
  filters, per-test deadlines, bounded output, JSON reports. Package discovery
  uses tests/; files defining main require a separate test source.
- Cache complete native outputs after Rust dependency freshness checks. Fingerprint
  emitted LLVM, toolchain/options/environment, native runtimes, preprocessed
  headers and tracked libraries. Atomic, digest-verified copies; uncertain inputs
  disable reuse. This does not yet cache frontend work per module.

## Ownership and acceptance

| Lane | Ownership |
| --- | --- |
| frontend_parity | Float/collection semantics and emission in both frontends |
| packages | Module graph lowering, privacy, aliases and LSP navigation |
| rust_cache | Native artifact cache and typed collection C runtime |
| root | Shared lowering/linkage, test runner, docs and combined verification |

Focused checks while sources evolve, two Cargo jobs and one test thread. Serialize
broad reference/full-hybrid suites after freeze. Require parity, ownership/sanitizer
checks, private member bypass rejection, alias navigation, isolated test failures,
cache hits/invalidation/corruption recovery, both refreshed releases and A/B/C.
Record real platform skips. The old app-chat heartbeat stays paused. Earlier
completed work is preserved in integration-generics.md and generics-milestone.md.

Completed and verified: 384 reference / 393 full-hybrid tests, both Clippy
configurations, 33 editor tests, refreshed releases, A/B/C and unchanged caches.
See integration.md for evidence and remaining limits.
