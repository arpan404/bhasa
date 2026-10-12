# Local packages and modules

Create and run a project:

```sh
bhasa new hello
cd hello
bhasa check
bhasa run
bhasa build --release
```

The scaffold contains `bhasa.toml`, `src/main.bh`, and a `.gitignore` for the
build directory. `new` refuses to replace an existing directory. A project name
starts with an ASCII letter and uses letters, digits, underscores, or hyphens.
The manifest filename is exactly lowercase `bhasa.toml`, including on systems
with case-insensitive filesystems.

```text
hello/
  bhasa.toml
  src/
    main.bh
    math.bh
  build/
    hello
```

`check`, `build`, and `run` without a file find the nearest `bhasa.toml` in the
current directory or its ancestors. The project entry is checked or compiled,
and output defaults to `<project>/build/<package-name>` with the target's
extension. Explicit source-file commands still work, including `emit-llvm`.
Program arguments follow `--`: `bhasa run -- input.txt --verbose`.
Use `./-name.bh` for a source filename beginning with a hyphen.

## Manifest

```toml
[package]
name = "hello"
version = "0.1.0"
entry = "src/main.bh"

[dependencies]
math = "../math"

[rust-dependencies]
adapter = "../rust-adapter"
```

This is a deliberately small TOML subset: the three sections above, bare name
keys, double-quoted strings without escapes, blank lines, and `#` comments.
Unknown sections, unknown package keys, duplicate keys, and duplicate sections
are errors. All three package fields are required. Versions currently use
numeric `major.minor.patch` form and are metadata; dependency resolution does
not select versions. Bhasa dependency values are local directory paths relative
to the manifest. Rust dependencies can name a local Cargo package directory or
its `Cargo.toml`. There is no registry, version solver, or automatic download.

Rust dependencies are local Cargo packages exporting a `staticlib` with C ABI
functions. Bhasa's build/run host asks Cargo to build them offline and links
their archives automatically. Cargo uses a persistent cache under the entry
project's `build/rust/` directory, including adapters referenced by imported
packages. Repeated builds reuse unchanged Rust
artifacts; source, profile, target, and dependency changes are handled by
Cargo. Rust is needed to build these adapters, but an already linked Bhasa
program does not need a Rust installation. `check`, `emit-llvm`, and object
emission do not invoke Cargo. No-GC compatibility depends on the selected
Rust libraries; Bhasa does not inspect libraries to prove that policy.

## Source imports

`src/main.bh`:

```bhasa
import "math.bh"

def main() {
    print(double(21))
}
```

`src/math.bh`:

```bhasa
def double(value: int) -> int {
    return value * 2
}
```

Imports appear before declarations, each followed by a newline or semicolon.
A file import is relative to the importing file. All imports must remain
inside the package root. Relative `..` components may reach a sibling folder
inside that root; absolute paths and symlinks or parent paths that escape the
root are rejected. Manifest entry paths still cannot contain `..`.
Without a manifest, the entry file's directory
is the root. A canonical file is loaded once even when several modules import
it. Cycles are errors. Import traversal follows source order deterministically.
The loader permits at most 1024 files, 128 import levels, and 64 MiB of source.
Manifests are limited to 1 MiB. Both manifests and source modules must be regular
UTF-8 files; named pipes and device files are rejected before reading.

Unaliased imports expose public declarations directly; conflicting exports are
errors. An alias isolates the imported API:

```bh
import "@math" as math
def main() { var point: math.Point = math.Point(1, 2) }
```

Qualified calls, types, enum variants and explicit generic applications work.
Declarations are public by default. Add `private` before a function, struct,
enum, extern declaration, field or record method to restrict it to its defining
module. A record with private fields cannot be constructed outside that module;
provide a public factory. A private member stays private through chained access
and generic specialization. Alias imports do not expose their members unqualified.
There is no package initialization or implicit renaming.

Functions may call other functions regardless of their declaration order.
Diagnostics retain the actual file, line, column, and local byte span, including
in `check --json` output.

## Publishing a local library

A library uses the same manifest and points `entry` at its public source file:

```toml
[package]
name = "math"
version = "0.1.0"
entry = "src/lib.bh"
```

The consuming package declares `math = "../math"` under `[dependencies]` and
loads that library's entry with:

```bhasa
import "@math"
```

Each library resolves its own declared dependencies. Only imported Bhasa
dependencies are loaded. A library needs no `main` for checking or object
emission; a linked executable needs exactly one valid `main` in the combined
program. Share the library directory and its source or check it into version
control. A package registry and packaged release format remain future work.

The repository's `examples/packages/app` consumes the adjacent `math` library.
Run `bhasa run` from the app directory, or use
`bhasa run examples/packages/app/src/main.bh` from the repository root.

## Formatting

`bhasa fmt src/main.bh` formats a file. `bhasa fmt src/main.bh --check` reports
whether changes are needed without writing. Project-wide formatting is not
implicit; pass the file you intend to format.
