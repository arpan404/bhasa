# Native application artifact cache

`src/build_cache.rs` stores completed native executables and objects under the
entry project's `build/native-artifacts-v1/`. This caches the final artifact;
it does not provide per-module parsing, type-checking or code-generation caches.
The driver still loads/checks Bhasa source and emits LLVM. It checks Cargo
freshness before looking up native output, preserving the existing Rust adapter
cache and its transitive invalidation rules. An unchanged hit avoids native
compilation, selected runtime compilation and linking.

## Key and freshness

The SHA-256 key includes generated LLVM and native input contents, normalized
compiler arguments, target/optimization settings, effective environment, the
running compiler binary, configured native compiler and actual subprocess tool
binaries, Clang version/driver plan, native link inputs and resolved libraries.
Runtime C/Objective-C preprocessing uses the actual target/options and include
paths, including SDK and CPATH headers; preprocessed output contributes to the
key. TLS compile/link options, headers and selected OpenSSL libraries therefore
participate. Shell bookkeeping variables `_` and `SHLVL` are excluded.

Input fingerprints are checked again before storage, so changing inputs during
compilation prevents publishing under an obsolete key. Rust static libraries
are ordinary content-keyed native link inputs after Cargo has checked freshness.
An unchanged source comment can reuse an artifact when its generated LLVM is
unchanged. Different profiles, targets, tool binaries or effective inputs use
different entries.

Clang probes and preprocessing still run on hits. Arbitrary custom toolchain
wrappers must expose an accurate Clang subprocess plan. Unrecognized tool plans,
unresolved libraries, custom linker scripts/response files and thin archives
fall back to ordinary compilation. GNU system linker scripts with explicit
absolute inputs are followed. Native caching currently requires Unix descriptor
operations; unsupported hosts compile without this cache.

## Publication and safety

The driver validates destination/source protection before lookup and uses its
existing validated atomic output publication on both hits and misses. Cache
restore copies into a fresh private artifact; cache storage does not hard-link
public executables. A header records the key, artifact hash, size and permission
bits. Restores verify all fields and hash all copied bytes; missing, truncated,
corrupt or symlink entries are misses. Only a successful Clang invocation can
produce a stored artifact.

Cache reads, creation and rename use directory descriptors with no-follow flags.
Writers publish a complete, synced single file by atomic rename. Concurrent
writers can safely replace equivalent entries; readers retain an already-open
entry during replacement. A symlink at the cache directory disables caching;
a symlink at a cache entry is never followed. Cache write failure leaves the
successfully built application usable.

## Focused verification

`tests/incremental_builds.rs` contains real Clang wrapper counters for unchanged
build/run reuse, source/profile/tool changes, output protection, corrupt/missing
entries, simultaneous writers and symlink redirection attempts. Additional
fixtures cover runtime-source and CPATH-header edits, build flags, target/object
separation, native library changes and Rust adapter source changes before lookup.
The local SHA-256 implementation has known-vector and chunk-boundary checks.

A standalone module type-check and real-Clang prepare/store/restore probe have
completed. All three focused Cargo groups pass, including target/object separation with
an installed architecture and foreign object OS. Complete reference/hybrid
acceptance also passes. SHA vectors and the cache groups passed again after
Clippy cleanup; logs use build/verify-foundation-incremental-final.log and
build/verify-foundation-sha-final.log. No Go-like compilation-time claim follows from artifact reuse;
frontend work, hashing and toolchain probes still have measurable cost.
