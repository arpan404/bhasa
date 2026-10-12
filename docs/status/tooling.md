# Tooling lane

## Methods milestone

October 11, 2026. Implementation, focused verification, and independent read-only review are complete. Freeze status: frozen and ready for root integration; no known blocker. Root owns combined verification and release rebuilding. Only assigned tooling files changed; no compiler/parser/checker/codegen files were edited by this lane.

### Delivered behavior

- Document symbols use Struct, Field, Method, and Function kinds. Fields and methods nest inside their record and preserve source order. Internal qualified method names never appear in the outline.
- Method definitions use the checker's lowered qualified callee, not a spelling guess. Same-named methods on different records and global functions resolve separately. `self`, nested fields, temporary constructors, and method-returned receivers are covered.
- Checked field accesses resolve through their receiver type; checked constructor calls resolve to the record declaration. Parsed declaration names can navigate to themselves even with unrelated semantic errors.
- Method/field/constructor call navigation requires successful checking of the requesting document's loaded graph. Invalid receivers, wrong arity and unresolved calls return null. The existing global-function fallback remains available for incomplete programs. No type-annotation or local-variable definition support is claimed.
- Saved import graphs keep open source overlays, original URIs, UTF-16 positions, and no source writes. Overlaid files parse independently, then use `Program::merge` and `sema::check`, preserving syntax boundaries and imported record type remapping. EOF diagnostics stay in their original file.
- Existing 4 MiB response limits, diagnostic omission warnings, stale-version rejection, full sync and shutdown behavior remain in place.

### AST integration

Read and used the records lane's `Methods milestone: interface handoff` before dependent implementation:

- `RecordDecl.methods: Vec<Function>` retains source names and spans.
- `ast::method_key(record, method)` supplies the qualified lookup key.
- Checked `ExprKind::Call` carries that key and prepends the typed receiver to args.
- Checked bodies stay in `record.methods`; raw unresolved MethodCall nodes never produce guessed targets.

No new root exports, CLI routing, dependencies, or runtime APIs are needed. The existing `crate::lsp::run() -> i32` and `serve<R: BufRead, W: Write>(reader, writer) -> i32` APIs are unchanged. `src/json.rs` was not changed in this milestone.

### Focused evidence

All Cargo commands used `CARGO_BUILD_JOBS=2` and `-- --test-threads=1`, adding `--exact` for single tests. No full suites or isolated target directories were run.

| Check | Result |
| --- | --- |
| `cargo test --test lsp method_` | 6 passed, 26 filtered out; checked resolution, hierarchy, invalid calls, declaration navigation with errors, imported overlays, module EOF boundary |
| `cargo test --test lsp record_symbols_preserve_field_hierarchy_source_order_and_utf16_ranges` | 1 passed, 31 filtered out |
| `cargo test --test lsp formatting_response_over_four_mib_fails_without_losing_the_connection` | 1 passed, 31 filtered out; also exercises indexing of 18,000 calls |
| `cargo test --test lsp imported_diagnostics_use_real_file_ranges_and_unsaved_entry_body` | 1 passed, 31 filtered out |
| `rustfmt --check --edition 2021 src/lsp.rs tests/lsp.rs` | Passed |

Seven new protocol tests bring `tests/lsp.rs` to 32 tests. The imported overlay case renames an unsaved method, observes unresolved old calls, updates the caller, resolves the new declaration, and checks that all saved source files are unchanged. Same-name tests cover globals and different record receivers. Record symbols and cross-file method locations retain UTF-16 ranges after supplementary Unicode characters and CRLF.

One early focused test compile hit the records lane's in-progress AST/parser/sema updates. That transient build failure cleared after the shared implementation caught up; no foreign files were repaired.

### Editor client

The sole internal helper stayed in `editors/vscode/**`. Existing provider conversion already supported nested DocumentSymbol children and all needed kinds, so no runtime client rewrite was needed. Changes add `self` grammar coloring, hierarchy/kind/range preservation tests, UTF-16 imported-method Location/LocationLink tests, grammar tests, and scoped README guidance.

- Focused provider command `node --test --test-name-pattern='hierarchical record|UTF-16 method|maps formatting' tests/extension.test.js`: 5 passed, including two definition forms.
- Grammar tests: 2 passed.
- `npm run check`: passed.
- After improving the method-call fixture, only the UTF-16 method tests were rerun: 3 passed.

Final independent review found no concrete issue in qualified method matching, nested/chained/parenthesized receiver spans, member/global separation, field matching, per-file EOF rebasing, imported type-ID remapping, original URI overlays, or response limits. The reviewer made no edits and ran no additional tests.

All tooling files are frozen. The VS Code GUI host remains unverified; client tests use mocked APIs. Node regex checks do not claim full TextMate tokenizer validation.

## Preserved baseline and limits

The previous integrated baseline included strict dependency-free JSON framing, 25 protocol tests, 25 Node tests, and a live stdio client/server smoke test. Root's completed baseline is in `docs/status/integration.md`; current method tests do not replace its combined verification.

Import declarations must be saved to refresh the disk-derived graph. Unsaved graph changes produce syntax checks and LSP001 rather than false unresolved-import diagnostics. There is no filesystem watcher, completion, hover, rename, workspace symbol search, general local-variable definitions, or semantic highlighting. Package manifests remain exactly lowercase `bhasa.toml`.

No commits, resets, worktrees, publishing, cache cleaning, dependency downloads, or tool installation occurred.
