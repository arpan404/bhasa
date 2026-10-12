# Owned buffer and ints transfer contract

October 12, 2026. Accepted implementation contract for the next ownership slice,
following independent review in build/backlog/ownership-types/ownership-contract-review.md.
This is an implementation assignment, not integrated acceptance or a completed
ownership checklist item.

Existing unqualified owned-data parameters remain synchronous mutable borrows.
Their contents may change, their borrowed handles cannot rebind, and ordinary
returned values are independently copied. No default call behavior changes.

Add contextual `owned name: buffer` and `owned name: ints` parameters to
monomorphic synchronous free functions. Transparent aliases resolve to these
same types. These parameters own their cleanup and may rebind their storage.
An initialized local owner or owned parameter can transfer through `move name`
only as an entire local initializer, an owned argument or an entire return
operand. An owned argument may also receive a fresh owned result directly.
A named owner requires move. Ordinary return acquisition still copies.

Moves reserve the source as soon as that expression evaluates. Later arguments
cannot read, mutate, borrow or move that source. Reject overlapping ordinary
mutable borrow and owned transfer in either argument order. Cleanup moves exactly
once to the destination; no clone or allocation is performed for the transfer.
Moved bindings cannot be read. A mutable binding may reinitialize in the same
straight-line scope; an immutable binding cannot. Self and duplicate moves reject.

For the first slice, forbid moving a source declared outside a conditional or
loop from inside that construct. Branch-local and iteration-local owners may
transfer within their own scope. This avoids path-dependent destructor flags
and loop fixed points. Ordinary copied returns remain available on those paths.
Track initialization, ownership cleanup, mutability, tasks and pending failures
as independent state. Preserve lexical shadowing, early-return/try cleanup and
existing worker/foreign call-graph restrictions.

Exclude Result/record/enum/List/Map transfer, methods and generic mode declarations,
projected moves, task handles/submission targets, file/TCP/TLS handles, borrowed
locals/returns and readonly modes. S037/BH221 obligations remain unchanged; an
unsupported result move cannot erase a pending result. Future owned Result
parameters must begin pending and transfer responsibility explicitly. Readonly
requires a separate audited effect and loan contract before implementation.

Keep owned/move contextual, preserving ordinary functions and parameters with
those names. Both frontends and public emitters independently enforce the same
modes and initialization. Reserve S038/BH222 for semantic transfer violations;
S036/BH220 immutability and S037/BH221 pending-result diagnostics remain distinct.
AST metadata/API handoff must precede native parity; no ABI convention is guessed.

Acceptance requires preserved mutable borrowed calls and copied returns, zero
clone/exactly-one-drop transfer, both overlap orders, moved reads, shadowing,
allowed local and forbidden outer branch/loop moves, var/let reinitialization,
return/try cleanup, task/resource exclusions, contextual names and corrupted
checked-AST rejection. Execute both frontends at O0/O2 with allocation/clone/drop
counters and nonrecovering sanitizers, then combined releases/self-build/cache
checks. General ownership/borrows and Rust-equivalent safety remain open.
