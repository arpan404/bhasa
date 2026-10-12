#!/bin/sh
# Link Bhasa compiler components into the Rust host, reusing unchanged artifacts.
set -eu

feature=bhasa-lexer
run_tests=1
for option in "$@"; do
    case "$option" in
        --frontend) feature=bhasa-frontend ;;
        --build-only) run_tests=0 ;;
        *) printf '%s\n' 'usage: sh bootstrap/build-hybrid.sh [--frontend] [--build-only]' >&2; exit 2 ;;
    esac
done

bootstrap_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH='' cd -- "$bootstrap_dir/.." && pwd)
clang=${BHASA_CLANG:-clang}
rust_host=$(rustc -vV | sed -n 's/^host: //p')
case "$rust_host" in
    aarch64-apple-darwin) bhasa_target=macos-arm64; clang_target=arm64-apple-macosx ;;
    x86_64-apple-darwin) bhasa_target=macos-x64; clang_target=x86_64-apple-macosx ;;
    x86_64-unknown-linux-gnu) bhasa_target=linux-x64; clang_target=x86_64-unknown-linux-gnu ;;
    aarch64-unknown-linux-gnu) bhasa_target=linux-arm64; clang_target=aarch64-unknown-linux-gnu ;;
    *) printf '%s\n' "Hybrid lexer currently requires a macOS or GNU/Linux x64/ARM64 host; found $rust_host" >&2; exit 1 ;;
esac

mkdir -p "$repo_dir/build/hybrid"
build_dir=$(mktemp -d "$repo_dir/build/hybrid/run.XXXXXXXX")
stage0_dir="$repo_dir/build/hybrid/stage0"
hybrid_dir="$repo_dir/build/hybrid/compiler"
if [ "$feature" = bhasa-frontend ]; then hybrid_dir="$repo_dir/build/hybrid/frontend"; fi
printf '%s\n' "Hybrid build directory: $build_dir"

# Explicit target and separate target directories prevent Cargo target defaults
# or a previous feature-enabled build from bootstrapping the scanner recursively.
cargo build --offline --locked --release --no-default-features --manifest-path "$repo_dir/Cargo.toml" --target "$rust_host" --target-dir "$stage0_dir"
stage0="$stage0_dir/$rust_host/release/bhasa"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        file_digest=$(sha256sum "$1") || return 1
    elif command -v shasum >/dev/null 2>&1; then
        file_digest=$(shasum -a 256 "$1") || return 1
    else
        printf '%s\n' 'Hybrid caching requires sha256sum or shasum.' >&2
        exit 1
    fi
    printf '%s\n' "${file_digest%% *}"
}

cat "$bootstrap_dir/compiler/lexer.bh" > "$build_dir/frontend.bh"
if [ "$feature" = bhasa-frontend ]; then
    cat "$bootstrap_dir/compiler/emitter.bh" >> "$build_dir/frontend.bh"
fi
# Include effective SDK/header contents and target macros, including headers
# selected through CPATH. Header paths alone do not detect in-place changes.
"$clang" -target "$clang_target" -std=c11 -O2 -E "$repo_dir/runtime/core.c" -o "$build_dir/core.i"
# The content-addressed archive path stays stable across unchanged runs. Cargo
# therefore sees unchanged native dependencies instead of relinking the host.
{
    printf '%s\n' 'bhasa-native-archive-v3' "$feature" "$rust_host" "$clang_target" "$clang" '-O2 -std=c11 -Wall -Wextra -Werror'
    hash_file "$stage0"
    hash_file "$build_dir/frontend.bh"
    hash_file "$repo_dir/runtime/core.c"
    hash_file "$repo_dir/runtime/core.h"
    hash_file "$build_dir/core.i"
    hash_file "$(command -v "$clang")"
    hash_file "$(command -v ar)"
    "$clang" --version
} > "$build_dir/inputs"
key=$(hash_file "$build_dir/inputs")
cache_dir="$repo_dir/build/hybrid/native/$rust_host/$feature/$key"
archive="$cache_dir/libbhasa_lexer.a"
mkdir -p "$cache_dir"
if [ -f "$archive" ]; then
    printf '%s\n' "Reusing Bhasa native archive: $archive"
else
    "$stage0" build "$build_dir/frontend.bh" --target "$bhasa_target" --emit object --release -o "$build_dir/frontend.o"
    "$clang" -target "$clang_target" -std=c11 -O2 -Wall -Wextra -Werror -c "$build_dir/core.i" -o "$build_dir/core.o"
    ar crs "$build_dir/libbhasa_lexer.a" "$build_dir/frontend.o" "$build_dir/core.o"
    printf '%s\n' "$rust_host" > "$build_dir/libbhasa_lexer.target"
    printf '%s\n' 'bhasa-frontend-v2' > "$build_dir/libbhasa_lexer.frontend"
    # Each writer uses its own temporary files. Rename publishes a complete
    # artifact; concurrent identical builds cannot expose a partially written .a.
    mv "$build_dir/libbhasa_lexer.target" "$cache_dir/libbhasa_lexer.target"
    if [ "$feature" = bhasa-frontend ]; then
        mv "$build_dir/libbhasa_lexer.frontend" "$cache_dir/libbhasa_lexer.frontend"
    fi
    mv "$build_dir/libbhasa_lexer.a" "$archive"
fi

BHASA_LEXER_ARCHIVE="$archive" cargo build --offline --locked --release --features "$feature" --manifest-path "$repo_dir/Cargo.toml" --target "$rust_host" --target-dir "$hybrid_dir"
if [ "$run_tests" -eq 1 ]; then
    BHASA_LEXER_ARCHIVE="$archive" cargo test --offline --locked --features "$feature" --manifest-path "$repo_dir/Cargo.toml" --target "$rust_host" --target-dir "$hybrid_dir"
else
    printf '%s\n' 'Tests skipped by --build-only.'
fi

printf '%s\n' "Hybrid compiler: $hybrid_dir/$rust_host/release/bhasa"
printf '%s\n' "Scanner archive: $archive"
if [ "$feature" = bhasa-frontend ]; then
    printf '%s\n' 'Use --frontend bhasa to run the Bhasa lexer, parser, checker and LLVM emitter inside the Rust host.'
else
    printf '%s\n' 'The compiler uses the Bhasa scanner with the Rust parser, checker, LLVM emitter and native toolchain driver.'
fi
