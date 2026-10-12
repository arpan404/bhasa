# Compiler integration status

Verified October 11, 2026 on ARM64 macOS with Rust/Cargo 1.98.0 and Apple
Clang 21.0.0. Integer bit operations, stepped ranges and checked editor
refactoring are integrated. The user authorized the full backlog in `todo.md`,
parallel internal agents, progress commits and pushing completed commits to
https://github.com/arpan404/bhasa. Prior typed-task evidence is preserved in
[integration-typed-tasks.md](integration-typed-tasks.md).

## Delivered

- Both frontends support integer `&`, `|`, `^`, `~`, `<<` and `>>`, with the
  agreed precedence. Shift counts outside 0 through 63 trap before LLVM shifts.
  Left shift wraps the bit pattern; right shift preserves its sign. Ordinary
  arithmetic remains checked. Hexadecimal/binary integer spelling is retained,
  validated against signed-i64 bounds and reported at original source spans.
- `range(start, end, step)` evaluates arguments once, left to right. Positive
  and negative steps select the corresponding exclusive bound. Zero traps,
  including empty ranges. Cursor advancement overflow ends iteration through
  normal cleanup. Mutating the visible binding cannot change traversal.
  Continue, break, return, owned data and lexical typed tasks retain their rules.
  Host visitors include steps in generic/module maps, privacy and worker graphs.
- The language server resolves checked local definitions and references, and
  provides checked rename across loaded graphs rooted at open documents.
  It returns versioned edits only for open files, rechecks proposed programs
  and compares declaration bindings to reject collisions and silent capture.
  The extension verifies the entire open Bhasa document inventory, including
  previously unedited importers, before converting edits. It validates UTF-16
  tokens, synchronization admission limits, failed sends and close/reopen events.
  Closed-document epoch history uses weak document keys.
- Formatter/grammar support, examples, guides and `todo.md` reflect the new
  behavior. Snapshot iteration over buffers, ints and Map keys is the prepared
  next increment; implementation has not started.

## Verification

| Gate | Evidence |
| --- | --- |
| Complete reference | 433 passed; build/verify-systems-reference.log |
| Complete full hybrid | 445 passed; build/verify-systems-hybrid-final.log |
| Integer/range contracts | Four integer and five range reference groups cover radix spans, bounds, precedence, strict types, corrupted checked AST, shift traps and a signed-range oracle; tests/integer_bits_reference.rs and stepped_ranges_reference.rs |
| Independent integration | Three groups cover imported generics, source evaluation order, mutable bindings, nested exits, worker/privacy graphs, original-file diagnostic tokens and formatter preservation; tests/systems_integration.rs |
| Owned execution | O0/O2 ASan/UBSan and atomic allocation counters cover loop snapshots, owned scratch data, early exits and typed tasks; focused and combined suites |
| Native frontend parity | Fresh seed/native and hybrid acceptance, 26 rejection probes, six traps and equal self-rebuilt LLVM; tests/systems_bhasa.rs and build/systems-native |
| Independent review | 115 reference/native probes; reverse host traversal/privacy/formatting review; checked refactor and synchronization review with retained regressions |
| Editor | 56 LSP tests in both complete suites, 80 Node tests and syntax checks; build/verify-systems-lsp-final.log and build/verify-systems-editors-final.log |
| Formatting/Clippy | Both configurations pass with warnings denied; build/verify-systems-fmt-final.log, verify-systems-clippy-reference.log and verify-systems-clippy-hybrid.log |
| Refreshed releases | Both release compilers execute new/previous examples and five package tests; build/verify-systems-release-smoke.log |
| Cache reuse | 26 native hits, four unchanged Rust adapter runs and two unchanged hybrid builds with zero compilations; build/verify-systems-release-smoke.log and verify-systems-unchanged-build-1.log / -2.log |
| A → B → C | Compiler LLVM and committed example outputs match, including bitwise operations, stepped ranges and typed workers; build/verify-systems-selfhost.log; build/selfhost/run.xQ1qwOVL |

Complete suites ran serially with two Cargo build jobs and one test thread.
Compiler sources remained frozen. The first hybrid run exposed an integration
assertion that assumed identical diagnostic tokens in both frontends. Mapping
was correct: Rust highlights the invalid expression, while native highlights its
consumed-expression cursor. The stricter frontend-specific token/code/location
assertions pass both frontends, and the complete hybrid rerun passes.

Editor review repaired unknown unsaved importer completeness, stale unedited
importers, replacement amplification, repeated offset scans, contextual-name
eligibility, unsynchronized over-limit documents and retained URI history.
Final client checks ran after these repairs. Details and conservative refusals
are in [systems-editor.md](systems-editor.md).

Fresh archive key: `e1056ba67083f8c895adfeacf64eff9ff00162057017d1ae7f9ed44432dfcc81`.
Its concatenated scanner/emitter source exactly matches the frozen sources.
Both unchanged builds reuse that archive without compilation. `.bh`, exactly
lowercase `bhasa.toml`, deterministic no-GC ownership, Rust host architecture
and existing caches remain intact. The old heartbeat remains paused.

Real Metal dispatch and TCP/TLS loopback pass. CUDA/Vulkan hardware tests skip
because devices/drivers are unavailable. The local Clang lacks a wasm32 backend.
Node editor tests use mocked VS Code APIs. No Linux/Windows-device, manual
VS Code GUI or runnable WASI execution claim is made.

## Remaining work

The language remains experimental. The full backlog is in [todo.md](../../todo.md).
The next prepared contract is
[collections-iteration-milestone.md](collections-iteration-milestone.md).
Rename does not crawl unknown importers or provide a workspace-wide index;
ambiguous/uninstantiated generic tokens and foreign ABI declarations remain
conservative refusals. General borrowing/lifetimes, owned parameters, channels,
cancellation, coroutines/async I/O, fixed-width types, closures/traits, portable
GPU kernels, runnable Wasm/embedded runtimes and the wider library/package/
release ecosystem remain open. Foreign adapters are trusted native code with
no automatic no-GC or thread-safety proof. Per-module incremental compilation
and C/Go performance parity are not established.
