# Typed errors in the Bhasa frontend

The native emitter supports `var value = try expression` as an entire variable
initializer. The marker is contextual: at that grammar position, identifier
`try` must be followed immediately in the token stream by another identifier.
Lookahead does not skip newlines. Ordinary calls such as `try(value)`, generic
calls named `try`, member access, bare variables and infix uses retain their
meaning. There is no new keyword, token code or runtime API.

Operand and enclosing return types must each be an enum containing exactly
`ok(payload)` and `error(payload)`, with one payload per variant. Error payloads
must have the same concrete nominal type. Success payloads and enum identities
may differ, and declaration order does not matter. A binding annotation checks
the success payload. BH218 diagnoses an invalid operand shape; BH219 diagnoses
an incompatible enclosing return shape or error type, at the marker.

The operand evaluates once and becomes an owned snapshot. Both branches acquire
their active payload before releasing that snapshot. The error branch constructs
the enclosing enum using its own error tag and field, destroys all owned locals
including hidden loop/match snapshots, and returns normally. The success branch
binds an independent payload and continues. The snapshot stays in SSA instead of
using a hidden source name, so variables named `try` cannot collide with it.
Corrupt runtime tags trap before reading a variant payload.

A direct join/await operand consumes its task before the potential error exit
checks all remaining live tasks. Any other outstanding task rejects propagation
with BH208. Existing lexical task restrictions and transitive worker checks
remain active. This is explicit return propagation, without throwing or unwinding.

Bare expressions yielding an enum with the exact result shape require use and
produce BH207, including constructors, calls, variables, fields and join/await.
Assigning or passing a result is permitted; this check does not prove eventual
handling. Other enum shapes retain their existing discard behavior. General
nested propagation and parenthesized propagation operands remain unsupported.

The Rust host owns private-enum opacity, module/generic lowering and original
source mappings. Standalone native compilation does not implement module privacy
independently. Normal host compilation rejects implicit deconstruction of a
private enum outside its defining module.

## Native evidence

The implementation was first prepared under `build/errors-draft/native` while
collection integration remained frozen. An existing reference release built a
fresh native seed. After the coordinator applied the draft, comparison confirmed
that the tracked native emitter was byte-identical to the verified draft.

The native ownership fixture passed O0 and O2 with ASan on generated LLVM,
ASan/UBSan on runtime code and an atomic cross-thread allocation counter. Both
runs exited with zero live tracked allocations. It covers once-only evaluation,
reversed tags, differing success payloads, borrowed-value isolation, sequential
propagation, owned nested collections, loop/match cleanup, direct join/await,
and nested classic tasks within workers.

Twenty-three invalid probes produced the expected BH201/BH202/BH205/BH207/
BH208/BH209/BH218/BH219 diagnostics without LLVM output. Four contextual-name,
generic and non-result-discard compatibility programs passed native O2. Those
standalone generic probes used manually specialized source; they do not verify
the shared host expander. Injected corrupt temporary enum tags trapped at O0/O2.
A/B/C emitted byte-identical compiler LLVM, and stages A/B emitted identical
ownership-fixture LLVM.

Evidence is in `build/errors-draft/native/probe.log`, `stage-b.ll`, `stage-c.ll`,
`fixture.ll` and `fixture-san.ll`. `tests/error_propagation_bhasa.rs` provides the
integrated reference/native comparison using real shared generic expansion,
sanitizer/allocation checks, compatibility/rejection cases, self rebuild and
optional hybrid coverage.

## Integration review

A read-only comparison of the applied Rust parser/checker/emitter and native
emitter found no concrete mismatch in task consumption, result shape, ownership
cleanup or success-path state. Both check pending tasks after direct join/await,
compare concrete error types, acquire the active payload before freeing the
source and locals, and retain success-path ownership facts. Rust's next/return
flow and the native success block both keep following statements reachable.

These are lane probes and source-review results. The coordinator owns Cargo
runs, privacy/import/editor checks, refreshed hybrid archives, combined suites,
release/cache verification and final acceptance. This file does not claim those
gates from the draft evidence alone.
