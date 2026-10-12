# Integer operations, stepped iteration and checked editor refactoring

This increment follows the verified typed-task commit. It implements entries
from `todo.md` under the user's full-backlog and local-commit authorization.
Preserve `.bh`, lowercase `bhasa.toml`, deterministic no-GC ownership, both
frontends, the Rust host and all caches. No SDK installs or release publication. The later explicit GitHub request
authorizes pushing completed progress commits.

## Integer and iteration contract

- Add int-only `&`, `|`, `^`, unary `~`, `<<` and `>>`. Precedence from weakest
  to strongest: logical or, logical and, equality, comparison, bitwise or, xor,
  and, shifts, addition/subtraction, multiplication/division/remainder, unary.
  `flags & mask != 0` therefore compares the masked integer to zero.
- Shift counts must be 0 through 63; invalid counts trap before an LLVM shift.
  Left shift operates on wrapping two's-complement bits; right shift is signed.
  Ordinary arithmetic remains checked.
- Accept `0x`/`0X` hexadecimal and `0b`/`0B` binary integer spellings without
  separators or suffixes. Positive literals must fit signed i64; directly
  negated magnitude 2^63 is allowed. Consume malformed prefixed spellings as a
  single lexical diagnostic with original spans. Do not reinterpret high-bit
  positive literals as negative values.
- Add `range(start, end, step)` while preserving existing one/two-argument
  forms, trailing commas and transparent parentheses. Evaluate start, end,
  step once, left to right. Step defaults to one. Zero traps even when the range
  is empty. Positive steps compare cursor < end; negative steps compare > end.
  Overflow during cursor advancement ends iteration. The mutable binding
  cannot change the cursor. Continue advances; all existing cleanup, lexical
  task and transitive worker restrictions apply, including calls in step.
- Rust AST uses optional Range.step. Native token codes preserve existing
  values: ASCII 38/124/94/126 and new 263/264 for shifts.

## Editor contract

Use checked semantic declaration identity for local definitions, references,
prepareRename and rename. Include lexical declarations, reads/writes, members,
aliases and concrete generic occurrences where identity can be proven.
Deduplicate specialization references and reject ambiguous template tokens.
Never authorize a refactor using incomplete-source recovery or syntax fallback.

References may include disk-loaded files in checked graphs rooted at open
documents. Rename requires every edited file open and returns versioned
documentChanges, without applying or writing edits. Validate every affected
graph in memory, reject collisions and silent capture, preserve aliases,
comments and strings, and reject reserved/invalid names. Check every edited
document's version, including close/reopen, before returning edits. The client
also validates the complete open Bhasa document inventory before conversion,
so a previously unedited importer cannot add a reference during the request.
Unknown unsaved import graphs block global rename; unrelated broken standalone
documents may remain open. Unsupported/synthetic/foreign ABI targets fail conservatively. UTF-16
ranges must match complete identifier tokens; cancellation and output bounds
remain enforced. Replacement names are bounded to 256 bytes; rebuilt checked
graphs are bounded to 64 MiB and refactor locations to 65,536.

## Ownership and gates

| Lane | Owned files and work |
| --- | --- |
| packages | Rust ast/lexer/parser/sema/codegen, focused integer/range tests and lane status |
| frontend_parity | Bhasa compiler lexer/emitter, standalone scanner and parity/native tests/status |
| rust_cache | LSP, LSP tests, VS Code providers/client tests and tooling/editor guides; LSP step traversals and scope/signature updates |
| root | Modules/generics traversals, Rust scanner bridge, formatter, VS Code syntax grammar, independent acceptance, examples/guides/checklist and commits |

Workers request ownership before edits outside their lane. Root serializes
Cargo operations with at most two build jobs and one test thread. Use focused
checks during implementation; freeze sources before complete reference and
full-hybrid suites. Require checked shift/range boundaries, original-file
diagnostics, worker/privacy graphs, O0/O2 ownership sanitizers, refactor capture
and stale-version tests, Clippy/fmt, editor tests, refreshed releases, A/B/C and
unchanged native/Rust cache reuse. Hardware/SDK limitations remain explicit.

Integrated after typed-task commit `cdd7a5b`. All lane implementations and
acceptance criteria above are complete. The reference/full-hybrid suites pass
433/445 tests, editor tests pass 80, both Clippy configurations and formatting
pass, and releases, A/B/C and unchanged caches are verified. See integration.md
for evidence and platform/language limits.
