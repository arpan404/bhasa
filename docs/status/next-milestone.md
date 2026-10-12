# Iteration and editor milestone

Continued by user instruction October 11, 2026. Prior completed evidence is
archived in integration-foundation.md and foundation-milestone.md. Preserve .bh,
exactly lowercase bhasa.toml, deterministic no-GC ownership, Rust host/Bhasa
frontend and all persistent caches. No installs, cache cleaning, commits, resets
or publishing. The old app-chat heartbeat remains paused.

## Contract

- `for name in range(end)` or `range(start, end)` is ascending signed-int
  iteration with an exclusive upper bound. Bounds evaluate once, left to right.
  Empty/reversed ranges perform no iterations. No steps or first-class ranges.
- `for name in expression` accepts List[T]. Acquire one deep source snapshot;
  each iteration owns an independent element snapshot. Body mutations, clearing
  or rebinding the original list do not affect remaining iterations. The mutable
  body-local binding cannot alter the internal cursor. Map iteration is deferred.
- `break` and `continue` target the nearest lexical while/for. They clean every
  exited owned scope; continue preserves the iteration snapshot, and break,
  return and normal completion drop it. Reject outside-loop exits, unjoined tasks
  in exited scopes and unreachable statements. Loops never prove a function
  return. Worker safety covers calls in bounds, iterable and body.
- LSP completion and hover use lexical scopes, types, function/member signatures
  and module visibility. Results use readable source names, UTF-16 ranges and
  bounded deterministic output; incomplete documents degrade conservatively.

## Ownership and acceptance

| Lane | Ownership |
| --- | --- |
| root | Rust AST/parser/checker/emitter, host lowering, formatter, integration |
| frontend_parity | Bhasa lexer/parser/checker/emitter and parity tests |
| packages | LSP, VS Code grammar/client, editor tests |
| rust_cache | Independent iteration acceptance and ownership/task review |

Focused checks while sources evolve. Two Cargo jobs, one test thread, one Cargo
operation at a time. After freeze run complete reference and full-hybrid suites,
both Clippy configurations, editor tests, both releases, A/B/C self-rebuild,
release examples/native tests and unchanged Rust/native frontend cache reuse.
Run iteration ownership at O0/O2 with sanitizers. Record hardware/platform skips.

Completed and verified: 398 reference / 408 full-hybrid tests, 38 editor tests,
both Clippy configurations, formatting, both refreshed releases, A/B/C and
unchanged native/Rust caches. See integration.md for evidence and limits.
