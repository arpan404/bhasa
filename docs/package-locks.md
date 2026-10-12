# Loaded package graph locks

The optional `bhasa.lock` format `bhasa-loaded-graph-v1` records the entry source
path and SHA-256 checksums of the exact source and manifest bytes consumed by the
loader. It uses deterministic sorted JSON and portable paths relative to the
entry package root. Sibling package paths can start with `../`. Moving the whole
package layout preserves the lock. Filesystem aliases resolve to canonical
package identities before paths are recorded.

A present lock must match the loaded graph, including all transitively imported
Bhasa packages. Changed content, new graph members, missing members and a
changed entry point fail loading. A missing lock retains existing behavior.
Validation is shared by the parsed and unparsed loaders before native compilation
or Cargo invocation. Validation never rewrites the lock. Manifests retain exactly
lowercase `bhasa.toml` and sources retain `.bh`.

The Rust API is `package_lock::Snapshot::capture(&loaded, &root_manifest)`,
`encode()`, `decode(text)` and `verify(&actual)`.
Explicit lock-update tooling uses `project::load_unparsed_for_lock_update`,
which bypasses the old lock while retaining import and manifest validation.
Normal check/build/run callers must never use the update API. `project::Manifest::checksum`
is the SHA-256 of the raw manifest bytes consumed by its parser. Capture reads
`LoadedProgram` data, so a later disk mutation cannot silently replace the bytes
being locked. Lock generation and verification need no Cargo or network access.

The v1 lock describes one loaded entry graph. It includes the root sources and
manifest comments, so local edits require an explicit lock update. Unimported
Bhasa sources, build output, native caches and Rust crate source trees are outside
this format. The Bhasa manifests still hash their Rust dependency declarations.
Cargo retains ownership of Rust resolution, Cargo.lock and persistent offline
build freshness. The v1 format is not a registry solver or a no-GC certificate.

The CLI provides explicit modes:

```sh
bhasa lock create [FILE] [--json]
bhasa lock update [FILE] [--json]
bhasa lock check [FILE] [--json]
```

Without FILE these commands select the nearest package's declared entry. An
explicit FILE may select another source within its package. A single v1 lock
records one primary entry graph; normal check/build/run/inspect keep the exact
entry and graph checks. The loader records the selected canonical manifest as
LoadedProgram.root_manifest_path. Explicit source requests select their package
from the requested parent directory before resolving the final file symlink.
An outer entry alias therefore retains its outer package even when its canonical
target is near a nested dependency manifest. Directly naming that nested source
selects its nested package. All loaders, lock commands and inspection use this
selected identity; they do not infer authority from source traversal.

Create refuses an existing lock. Update requires an existing regular lock and
can repair a stale or malformed document up to the input limit. It leaves an
identical document and its modification time untouched. Writes stage a complete
synced document in the package directory and publish atomically. Create uses a
no-replace operation; update replaces the existing lock. Symlink and nonregular
lock destinations are rejected. This is not a cross-process compare-and-swap
protocol. Check and inspect never create, update or repair locks and never invoke
Cargo or Clang.

JSON mode writes one schema_version=1 object to stdout. Success includes ok,
command, canonical lock/entry paths, changed and an empty diagnostics array.
Failures include ok=false and located diagnostics. Successful operations exit 0,
load/verification/write failures exit 1 and usage errors exit 2. JSON failures
leave stderr empty; output I/O failures retain normal stderr reporting. --help
shows the modes. The -- argument delimiter permits a source path beginning with
'-'. Native frontend/target/build flags are not accepted by lock commands.

Inspect adds a lock object with path, present, verified and the current loaded
graph snapshot. A valid present lock is reverified before success is reported.
Manifest package metadata includes its raw-byte checksum. Standalone source
inspection without a manifest has lock=null. These fields are additive to
inspection schema version 1.

## Checked test graphs

The native test runner uses project::load_test_graph. For a locked package it
first verifies the declared primary entry through the normal strict loader,
including all dependencies even if the test does not import them. It then
permits additional canonical source files only below that root package's tests/
directory. Every loaded manifest and every source outside tests/ must match a
primary lock record. Test code may change freely, while unpinned production
modules, test-only new dependency manifests and changed dependency sources fail.
Dependencies' own tests/ trees have no exemption. The original package identity
is retained when loading the primary entry through an in-package symlink.

The primary v1 lock is not a checksum of test code. This explicit test policy
permits mutable test sources while retaining locked production dependencies.
Normal compiler and inspect entry points retain exact-entry enforcement and do
not call the update bypass or the test loader. Unlocked projects retain their
existing test loading behavior. General multi-entry/workspace lock formats remain
future work.

## Checked editor graphs

The language server uses project::load_editor_graph for read-only disk analysis.
An unchanged imported module or root test can be the analysis entry: the complete
primary disk graph is first verified exactly, then every production source and
manifest in the analysis graph must match a pinned record. Only the selected
root package's tests/ source directory has the test-code exemption above.
New production sources, new manifests and stale primary dependencies reject.

Unsaved editor overlays are applied after disk verification and retain ordinary
import-edge, privacy and type checks. They never rewrite the lock. Saving changed
production code makes the disk lock stale until an explicit update. The language
server keeps the requested file path for package selection and the canonical
path for overlay identity, including entry aliases. Normal compiler/inspection
commands retain strict exact-entry loading.

Compiler artifact destinations reserve bhasa.lock names even before a lock
exists. Existing locks, source/manifest files and declared Rust adapter source
trees are protected against direct, symlink and hardlink output aliases.
Rejected paths are checked before output directories are created and again at
publication. Every output requires a bounded check of declared Rust source trees,
including external file and directory symlink targets and future dangling targets.
The check visits at most 100,000 entries and follows at most 40 symlink hops.
Unresolvable cycles, exhausted bounds, I/O errors and ambiguous non-ASCII future
names fail closed. This does not provide a transaction against concurrent hostile
filesystem changes.

Locks are at most 1 MiB, have at most 2048 file records and require normalized
UTF-8 portable relative paths and lowercase 64-digit SHA-256 checksums.
Nonregular locks and lock symlinks are rejected.
