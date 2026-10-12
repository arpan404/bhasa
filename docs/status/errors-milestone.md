# Typed errors and explicit propagation

Integrated and verified increment following collection snapshot iteration (`c4f8957`). Preserve the Rust
host, both frontends, existing Result APIs, native ABIs and caches.

## Propagation contract

```bhasa
var contents = try io.file_read_str("settings.json")
```

Initially supported only as an entire `var` initializer. Contextual `try` is
recognized when followed by an identifier operand at this grammatical position.
Existing `try(...)`, generic calls named try, member access and ordinary uses of
a variable named try retain their meaning. Parenthesized operand propagation and
nested expression propagation are outside this increment; parsing is independent
of ordinary spaces and does not skip newlines for the contextual lookahead.

The checked operand and enclosing return type must each be an enum with exactly
two variants named `ok` and `error`, each containing exactly one non-task payload.
Their error payloads must have identical concrete nominal types. Success payloads
and enum identities may differ. Variant declaration order is irrelevant. An
optional binding annotation applies to the success payload. Operand evaluation
occurs once. On success, bind an independently owned payload. On error, return
the enclosing enum's error variant and destroy the operand, locals and hidden
loop/match snapshots through normal cleanup. Corrupt runtime tags trap.

Direct join/await operands consume their handle before checking the potential
error exit. Any other live task in any scope rejects propagation. Worker graph,
privacy, generic lowering, original-file diagnostics and editor symbol identity
must include the operand and success binding.

Private enum declarations remain opaque outside their defining module. Implicit
try deconstruction of an inferred private enum is rejected at the try marker;
public functions may still return opaque values for use by that module's APIs.
The host retains enum ownership/privacy metadata for this check in both modes.

A bare expression yielding an enum with this exact shape must be used, like
existing status builtins. This is deliberately partial enforcement: assigning,
passing or reading a failure value does not prove eventual handling. Full pending
failure analysis remains unchecked in todo.md.

## Typed file errors

New `std/io.bh` defines ErrorDomain.os and an Error whose private fields store its
domain, native positive code, operation and Text message. Public getters return
values; returned messages are independent owned copies. The error factory stays
private in its defining module. New IoResult[T] carries ok(T) or error(Error),
without changing existing Result[T].error(Text) or integer runtime contracts.

file_read(buffer) / file_read_str(str) return IoResult[buffer].
file_write(buffer,buffer) / file_write_str(str,buffer) return IoResult[int],
carrying the contents length only after full write and successful close. Failures
preserve the native status, without claiming a partial byte count. Reads preserve
the destination on native failure. Writes can truncate or leave partial output;
successful close does not promise fsync durability. Buffer paths may contain
non-UTF-8 bytes but embedded NUL is an error.

## Acceptance and ownership

packages owns Rust parsing/checking/emission, AST helpers and focused reference
tests. frontend_parity owns matching native behavior and native tests. rust_cache
owns the typed I/O module, direct fault-injection/ownership acceptance and its
guide. Root owns host/module/generic/editor/formatter integration, independent
imported/generic/task tests, combined gates, checklist and commits.

Require source evaluation once, reversed variants, differing success types,
owned payloads, shadowing, contextual-name compatibility, corrupted checked AST,
wrong shapes/error types, cleanup on loop/match exits and pending tasks. Exercise
file binary/empty round trips, missing/directory/NUL paths, privacy and independent
messages, injected read/close/write/flush faults and native status preservation.
Run O0/O2 sanitizer/allocation checks, focused gates, frozen combined suites,
refreshed releases, self-rebuild and unchanged cache reuse. Only the coordinator
runs Cargo with two build jobs and one test thread; workers do not commit.

## Completion

All scoped acceptance criteria pass in the coordinator's final reference/hybrid
suites, 457/471 tests, and release/self-rebuild/cache gates. See integration.md
for evidence and limits. Assigned-failure tracking, nested propagation and typed
network/stream/process APIs remain separate backlog work.
