# Build caching

Bhasa delegates Rust builds to Cargo and preserves Cargo's target directory.
An unchanged Rust dependency is reused when its source, dependencies, compiler,
target, profile and effective build settings remain compatible. Cargo still
starts to check freshness; fresh crates do not invoke rustc for compilation.
Bhasa does not run `cargo clean`, use a new temporary target directory per run,
or rebuild a crate merely because Bhasa source changed.

Declare local adapters in `bhasa.toml`:

```toml
[rust-dependencies]
adapter = "rust/adapter"
```

Each value names a directory containing `Cargo.toml`, or the manifest itself,
relative to the Bhasa manifest. The Cargo package must expose a library with
`crate-type = ["staticlib"]` and compatible C ABI exports. Bhasa obtains archive
paths and `fresh` status from Cargo's JSON messages. It does not guess output
filenames or decide freshness from source modification times.

Artifacts remain under `<entry-project>/build/rust/<adapter-id>/target`. The ID is
derived from the canonical Cargo manifest path, with a checked identity file to
prevent accidental cache collisions. Each adapter has its own persistent target
directory so unrelated crates with the same library name cannot overwrite each
other. Cargo separates target triples and profiles within that directory.
Repeated imports of the same adapter across the entry project's dependencies
reuse that directory.
Different adapter roots have separate caches, including their transitive crates.

Changing Rust source, a transitive crate, Cargo manifests, the lockfile, compiler,
features or relevant environment settings can require rebuilding affected
crates. Cargo owns those decisions, including build-script rerun rules. A crate
with an inaccurate build script can still invalidate too often or miss its own
inputs; Bhasa cannot repair that contract. Changing only Bhasa source does not
invalidate Rust artifacts.

Builds use `cargo build --offline --lib`. Cargo may create or update `Cargo.lock`
when dependency manifests change; subsequent builds retain it. Keep lockfiles
under version control for reproducibility. Offline means required registry or
Git dependencies must already exist in Cargo's local cache. Bhasa never installs
toolchains, downloads crates or selects a different Rust target on failure.
Cross compilation requires the matching installed Rust target and linker/SDK.

Rust crates and their build scripts are trusted native code. Rust compatibility
currently means explicit C ABI adapters, not Rust ABI linking or arbitrary
Rust API imports. Adapters must meet the application's no-GC requirement. They
must not unwind across the FFI boundary. Native third-party libraries required
by an adapter also need compatible link inputs; platform runtime libraries are
added for Linux, but Bhasa does not discover every external SDK automatically.

`check`, LLVM emission and object emission do not build Rust dependencies.
Executable builds and `run` link the cached archives. Running a previously built
executable does not invoke Bhasa, Cargo or Rust at all.

The compiler itself uses Cargo's normal persistent `target` cache when built
with `cargo build`. The older `examples/rust-interop/run.sh` now uses a stable
Cargo target directory for its adapter as well. Deleting a target directory or
moving a crate to a different canonical path causes a cold build.

The hybrid builder also caches the compiled Bhasa frontend archive under
`build/hybrid/native/`. Its content key includes the bootstrap compiler,
Bhasa source, native toolchain, target, runtime source and preprocessed C input.
Changes to SDK or `CPATH` headers therefore invalidate the archive. Unchanged
runs reuse its stable path, so Cargo does not relink the host unnecessarily.
`sh bootstrap/build-hybrid.sh --frontend` builds and runs the full suite.
Use `--build-only` to rebuild without rerunning tests, for example when checking
that an unchanged build reuses artifacts. Tests create isolated Rust fixture
crates to verify invalidation; their compilations are separate from the cached
compiler and application adapters.

The cache regression test wraps rustc and counts real crate compilation calls.
It verifies zero additional calls for unchanged builds, then verifies rebuilding
after adapter edits, transitive source edits and `RUSTFLAGS` changes. It also
checks debug/release and target separation and runs linked Bhasa programs before
and after a Rust change.

## Native application artifacts

After Cargo checks adapter freshness, Bhasa may reuse a complete native output
from `<entry-project>/build/native-artifacts-v1`. The key includes effective
LLVM, target/profile/options, compiler/linker identity, relevant environment,
native runtime sources, preprocessed headers and tracked native libraries.
A comment-only source edit can reuse output when emitted LLVM is unchanged.

Cache entries publish atomically and are digest-verified before copying to the
requested output. Corruption or missing entries cause recompilation; output
protection still applies to a hit. Untrackable inputs disable reuse. The cache
preserves independent executable files rather than exposing mutable shared
hardlinks. This cache does not yet skip lexing/checking/emission per module.
See [incremental cache evidence](status/incremental.md) for supported inputs
and conservative exclusions.
