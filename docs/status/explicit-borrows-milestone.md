# First explicit buffer/ints borrow contract

October 12, 2026. Ratified by the integration coordinator for isolated implementation.
This extends the frozen owned-transfer AST. It does not alter that patch, shared
sources, Cargo configuration or either frontend. The earlier unratified
`borrow local` caller-marker proposal is replaced by formal parameter modes in
this first increment. No borrowed local or borrowed return syntax is introduced.

## Scope and syntax

Add contextual `readonly name: buffer|ints` and `mutable name: buffer|ints`
parameters on defined, monomorphic, synchronous free functions. Resolve aliases
before checking eligibility. Other concrete types, externs, methods, generic mode
declarations and direct task targets reject. Unused generic declarations must
reject before erasure. Existing worker call-graph restrictions continue to apply
when a worker calls an eligible synchronous function.

Call arguments keep ordinary expression syntax. An explicit borrowed parameter
accepts an initialized whole local/parameter name or a fresh owned buffer/ints
call result. Exclude projections, explicit moves to borrowed formals, aggregates,
borrowed views and handles. Fresh-result ownership stays with the caller, which
releases the temporary after the synchronous call. No implicit copy is added to
make a rejected borrow succeed.

Recognize a modifier only in a parameter header when its identifier is followed
by a parameter identifier and colon. `def readonly(mutable: int)` and calls to
ordinary functions with these names retain their meaning. Reject stacked modes.
Module lowering protects contextual markers and binds the actual parameter name;
type-alias canonicalization and span remapping retain the declared mode. The
parser does not reserve these words globally.

## Public modes and independent state

Extend the existing enum, without changing its default:

| ParamMode | Meaning | Native mode ID |
| --- | --- | --- |
| Borrowed | Existing mutable, potentially aliasing synchronous borrow | 0 |
| Owned | Frozen cleanup transfer | 1 |
| ReadOnlyBorrow | Shared synchronous read loan | 2 |
| MutableBorrow | Exclusive synchronous read/write loan | 3 |

Keep `Param.mode` and the complete parallel `FunctionSig.modes` vector. Native
IDs are explicit mappings, not Rust enum discriminants. Buffer/ints still use
the same pointer ABI. Both new modes have borrowed cleanup, no rebinding and no
transfer ownership. Their ordinary returned values acquire independent copies.
An explicit mutable parameter may change contents but cannot replace its handle.
A `let` owner may lend mutable contents, because let controls binding reassignment.

Keep cleanup ownership, initialization, reassignability, data access, active loans,
pending results and task state separate. In the ownership pass, add read-only or
read/write access plus storage provenance to each buffer/ints binding. Derive
parameter access from mode. Acquired locals, match payload copies and copied returns own new writable
storage. The separate acquired match-payload correction establishes that existing
match binders own their acquisition copies. A move preserves storage provenance while invalidating its old binding;
reinitialization installs fresh provenance under the frozen straight-line rule.
Do not encode read-only access in Type::Buffer/Ints or `Type::is_owned()`.

Update the existing `borrowed_params` filter to include all borrowed modes.
The frozen emitter already binds only Owned parameters with cleanup ownership;
that condition remains exact equality with Owned. The formal slots do not clone or drop on entry or exit. Ordinary return
acquisition still clones a returned borrowed value.

## Data effects and helper calls

Read-only means no mutation of the borrowed buffer/ints storage during its loan,
including descriptor length/capacity and elements. It does not promise that the
function is globally pure or that a socket/process has no side effects.

Approved host helper shape in builtins.rs:

```rust
pub enum BuiltinArgEffect { Value, Read, Write }
pub fn argument_effects(name: &str, args: &[Type])
    -> Option<Result<Vec<BuiltinArgEffect>, String>>;
```

Its vector has exactly args.len() entries. None means this is not a builtin;
Some(Err) means the builtin overload is invalid or unaudited. No missing entry
means Read. Resolve ordinary user calls through FunctionSig.modes instead.
Extend the existing ownership::Analysis rather than trusting an independent
cached initialization map: keep Binding.owner/initialized/mutable/control and
add access/provenance. Replace loans: &[Key] with typed Loan entries carrying
root, provenance, data type, kind and call frame. Keep ownership::check and
check_checked as the public validation entry points.

The builtin effect table is per validated overload and argument position. Scalar
and str positions carry Value; buffer/ints positions carry Read or Write. Write
includes reading the destination. `builtin-effects.json` records the audited
positions in the frozen builtin API. Apply classification only after ordinary
signature validation; a recognized name with an unsupported signature rejects.

| Operations | Data effects |
| --- | --- |
| len, get, slice, same, print, UTF8 validity/count/get/slice | Read supplied data |
| set, push, utf8_append | Write argument 0 |
| append, args_push | Write argument 0; read a buffer argument 1 |
| read_file | Read a buffer path at 0; write destination 1 |
| write_file | Read buffer path 0 and contents 1 |
| read_stdin | Write argument 0 |
| write_stdout, write_stderr, process_run | Read argument 0 |
| gpu_dispatch | Read buffer source/entry at 0/1; write data 2 |
| tcp/tls/stream read | Write buffer destination 1 |
| tcp/tls/stream write | Read buffer contents 1 |
| tcp host/listen and TLS host/certificate/key buffer positions | Read only those data arguments |

Resource state remains outside this contract. The table classifies buffer
arguments of existing resource operations; it adds no resource parameter mode,
resource alias guarantee or new resource/task semantics. Unknown operations,
new overloads and unaudited method/foreign targets cannot receive a restricted
loan. Existing S037/BH221 must-use obligations remain unchanged.

A readonly parameter can forward to another readonly free-function parameter or
an audited Read builtin position. It cannot forward to Borrowed, MutableBorrow
or Owned, even if the current unqualified helper happens only to read. This is a
capability rule, not inferred function purity. To use a mutable helper, first
acquire an independent local copy and pass that copy. Readonly names cannot move,
rebind or occupy a Write builtin position. A copied local and a slice result can
mutate normally. An ordinary `return readonly_parameter` clones normally.

Validate every annotated body and every resolved call edge, including recursive
and mutually recursive functions. Mode declarations are available before body
checking, so recursive readonly forwarding needs no speculative effect summary.
At each edge compare the actual binding capability with the formal mode. This
also catches a readonly wrapper forwarding to a legacy helper that calls a
mutator several levels later. Reject at the first capability-widening edge.
Do not infer a readonly summary for an unannotated helper or trust a cached call
summary as permission to widen access. No new FunctionSig effect-summary field
is needed for this first contract. Editor hover/formatting and inspect API output
must expose the declared mode separately from binding reassignability; a readonly
data parameter does not turn every copied local into an immutable binding.

Readonly references cannot escape into captures, task submissions, aggregate
storage or method calls. Copying the data into a whole buffer/ints local is the
supported escape to independent ownership. Aggregate and borrowed-return rules
need separate contracts. This first increment does not broaden those semantics.

## Alias provenance

Distinct names do not prove disjoint storage. A function invoked with ordinary
borrowed parameters can receive the same owner more than once. Track a
conservative may-alias relation, separated by concrete buffer versus ints type:

- Acquired local owners and Owned parameters have distinct live storage identities.
- Explicit MutableBorrow parameters are disjoint from every other data parameter
  in that invocation, because the caller proves exclusivity.
- ReadOnlyBorrow parameters may alias each other; they cannot alias a Borrowed,
  MutableBorrow or Owned argument in the same invocation.
- Distinct Borrowed parameters of the same concrete data type may alias.
- A copied local has fresh provenance even when copied from a readonly or legacy
  parameter. The copy does not inherit its source's read-only capability.

For `outer(a:buffer,b:buffer)`, reject `exclusive(a,b)` when exclusive takes
mutable/read-only arguments, even if the outer caller happened to pass distinct
owners. This first analysis is intentionally conservative inside legacy callees.
Use a local copy to prove disjointness. A dormant possibly aliasing parameter does
not prohibit a single-argument synchronous reborrow: the callee cannot reach the
dormant parameter through a global function or closure in this slice.

## Argument ordering and loan lifetime

Prepare arguments left to right. After evaluating a data argument, register its
loan in the enclosing call frame. Retain loans through that invocation; release
them when the call returns. An inner call frame must consult parent-frame loans
and restore only its own loans. Completed scalar computations retain no loan.

For roots that may alias, apply this matrix whenever an explicit mode participates:

| Existing loan | New read access/shared loan | New legacy mutable loan or write | New exclusive mutable loan | Move |
| --- | --- | --- | --- | --- |
| Shared read-only | Allow | Reject | Reject | Reject |
| Exclusive mutable | Reject | Reject | Reject | Reject |
| Legacy builtin read argument retained for call | Allow | Preserve ordinary legacy writes | Reject | Frozen rejection |
| Legacy mutable argument retained for call | Reject a shared loan; preserve ordinary builtin reads | Preserve existing legacy behavior | Reject | Frozen rejection |

Retained builtin Read positions have a separate legacy-read loan kind. They can
coexist with shared read loans, preserve existing ordinary intrinsic aliasing,
and prevent a move or exclusive loan while their argument pointer is retained.
Two shared loans of one root are allowed. A legacy mutable formal and an explicit
readonly formal cannot receive possibly aliasing roots in either order. A
MutableBorrow formal conflicts with any other retained data argument on the same
or possibly aliasing storage, including an ordinary formal. Two purely legacy
borrows retain current compatibility. Existing builtin self-append and self args_push remain allowed outside conflicts with an active explicit loan; both
runtime implementations deliberately handle self-aliasing.

`order_ro(x, len(x))`, with a readonly first formal, permits the later read.
`order_mut(x, len(x))`, with a mutable first formal, rejects it.
`reverse_order(len(x), x)`, with a mutable second formal, can succeed, because the first computation completed
before the exclusive loan began. A later nested call that writes, borrows mutably
or moves an earlier shared root rejects. Reads also reject under an earlier
exclusive loan. A move reserves initialization immediately under the frozen
rules, so a later read or either new borrow mode rejects. Earlier borrowed loans
of any kind prevent a move. Direct borrows do not make an uninitialized variable
readable or relax mutable reinitialization rules.

Caller checking is deliberately conservative about retained legacy mutable
arguments before a nested explicit reborrow. For example, staging a legacy
mutable argument x before evaluating a later readonly helper on x rejects even
if that helper returns a copy before the outer call. No existing source changes
behavior, since the helper's explicit mode is new syntax.

## Control flow and copied results

Check all conditions, branch arms, loop sources/bodies, short-circuit operands,
match arms and early/try exits. An unreachable-looking branch still cannot write
through a readonly parameter. Loans are bounded by calls, so no borrowed local
state or path-dependent loan merge is introduced. A readonly condition call ends
before the branch body; a caller that owns the storage may mutate it afterward.
A readonly callee remains restricted throughout its body. Local copies inside
branches or iterations can mutate and use the frozen allowed local transfers.
The frozen prohibition on conditional/loop moves of outer owners is unchanged.

## Independent emission and native impact

The public host emitter must recompute the borrow/capability/loan analysis from
AST declarations and bodies, then compare every signature's complete mode vector
with the actual declaration. Do not trust semantic success or cached access facts.
It must reject a write through a readonly parameter, unsupported modes, widened
helper calls, possibly aliasing exclusive calls, moved reads and mismatched modes.
Host emission has no source token buffer; its authority is the supplied AST
contract, not a promise to reconstruct a mode from the original source text.

The native owned draft keeps signature parameter pairs at stride 2 and mode
records at `[next,signature,index,mode,marker-token,declared-type]`, rooted at ctx31.
Extend that side table to IDs 2/3 rather than guessing a new signature ABI.
`be_validate_modes` must reconstruct owned/readonly/mutable markers and validate
eligibility. `be_has_owned` currently tests mode != 0; change it to mode == 1 for
ownership decisions and add a separate any-explicit-mode predicate for task and
generic exclusions. Otherwise a readonly mode would be confused with owned-mode presence.
The existing call parser also writes parameter_mode directly to ctx33, allow-move,
and be_move tests only whether ctx33 is zero. IDs 2/3 would accidentally permit
move. Set that boolean from exact equality with Owned, not the numeric mode.
Only ID 1 may clear caller argument cleanup after pointer adoption; borrowed
fresh temporaries keep their caller release. Function entry cleanup likewise
remains mode == 1.

The existing active transfer-loan list at ctx36 stores `[next,local]`. It cannot
express shared versus exclusive loans or may-alias parameters. Replace/extend its
records with root/provenance, concrete type, loan kind and call-frame identity.
Keep initialization/local cleanup fields independent. Add provenance/access
metadata in a side table instead of silently repurposing the twelve-slot local
record. If another context head is appended, update every fixed header bound
check, including the existing minimum node offset 37. The native lane owns that
layout change after ratification. Tokens and native mode metadata must agree at
emission, including bounds/cycle/duplicate-record rejection.

S039/BH225 are assigned for data-access and explicit-loan violations. BH223/BH224
remain reserved for channel checks. Keep S038/BH222 for frozen move/initialization/transfer violations and
mode-vector consistency. Keep S036/BH220 for binding immutability and S037/BH221
for pending results. Borrowed-handle rebinding currently reports host S021, host
emitter C001 or native BH201; retain those existing paths rather than treating
rebinding as readonly access or immutable let assignment.

## Tests and delivery gate

`proposed-cases.json` supplies concrete sources and expected proposed behavior.
These are acceptance specifications, not passing evidence for unimplemented
syntax. `emitter-cases.json` supplies separate corruption/recheck specifications. `baseline-results.json` records checks against the frozen host core only.

Require both frontend acceptance/rejection and independent public-emitter
corruption tests. Include mode-vector truncation/mismatch, readonly body rewrites,
legacy parameter may-alias cases, loan-kind corruption, nested-frame restoration
and unsupported overloads. Test imported aliases, contextual names, unused
generic/method exclusions, branches/loops/short-circuit calls, both overlap orders,
readonly recursion and a deep legacy helper mutation chain.

At O0/O2, borrowed readonly and mutable calls must add zero clones and zero drops
of their formal arguments. One caller owner drops once after the call. Ordinary
returned copies and local snapshots must each clone once and drop independently;
mutation of a snapshot must leave the readonly source unchanged. Check temporary
borrow cleanup, early-return/try paths and mixed Owned calls with explicit loans.
Run address/undefined-behavior sanitizers plus explicit runtime allocation/drop
counters, preserving the frozen zero-clone transfer and pending/task suites.

Syntax, exclusivity/provenance rules and diagnostic allocation are ratified for
isolated implementation. Actual combined integration remains a separate gate. No external dependency blocks this proposal. The concrete work
needed after ratification is the host provenance/effect analysis and native typed
loan metadata; the frozen name-only loan list cannot enforce this contract.

## Coordinator staging decision

Keep the corrected owned-transfer draft frozen and build the borrow increment
as a separate contextual delta against it. The host may implement and publish
actual APIs now; native work is assigned only after that handoff and channel
parity coordination. All acquired storage identities must come from reviewed
copy/adoption operations, never distinct name spelling. Unknown operations and
unsupported overloads fail closed. Emission must recompute source modes and loan
facts, rather than trusting mutable cached analysis. No resource, task, aggregate
borrow, borrowed return, method or generic-mode permission is implied.

Preserve FileHandle tag 11 as denied and authenticated channel operation effects
when those increments are composed. Retain exact Owned predicates so modes 2/3
cannot adopt or move storage. Imported original spans, existing worker/foreign
checks and prior diagnostics remain acceptance gates. No new mode is integrated
or verified merely by this ratification.
