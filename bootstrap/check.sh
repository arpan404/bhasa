#!/bin/sh
# Compile and execute the Bhasa seed checks with the Rust stage-0 compiler.
set -eu

if [ "$#" -gt 1 ]; then
    printf '%s\n' 'usage: sh bootstrap/check.sh [--release]' >&2
    exit 2
fi
mode=${1-}
if [ -n "$mode" ] && [ "$mode" != '--release' ]; then
    printf '%s\n' 'usage: sh bootstrap/check.sh [--release]' >&2
    exit 2
fi

bootstrap_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH='' cd -- "$bootstrap_dir/.." && pwd)
check_dir=$(mktemp -d "${TMPDIR:-/tmp}/bhasa-seed.XXXXXXXX")
trap 'rm -rf -- "$check_dir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cat "$bootstrap_dir/lexer.bh" "$bootstrap_dir/lexer-tests.bh" > "$check_dir/seed-checks.bh"

# Cargo needs no registry access. BHASA_CLANG, when set, is handled by the
# compiler driver. An explicit output keeps the check's artifacts temporary.
cargo run --quiet --offline --manifest-path "$repo_dir/Cargo.toml" -- check "$check_dir/seed-checks.bh"
if [ "$mode" = '--release' ]; then
    cargo run --quiet --offline --manifest-path "$repo_dir/Cargo.toml" -- build "$check_dir/seed-checks.bh" --release -o "$check_dir/seed-checks"
else
    cargo run --quiet --offline --manifest-path "$repo_dir/Cargo.toml" -- build "$check_dir/seed-checks.bh" -o "$check_dir/seed-checks"
fi
"$check_dir/seed-checks"
