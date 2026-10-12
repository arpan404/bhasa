#!/bin/sh
set -eu

# Native macOS/Linux example. No downloads or external Rust crates are needed.
case "$(uname -s)" in
    Darwin|Linux) ;;
    *) printf '%s\n' 'This example script supports native macOS and Linux hosts.' >&2; exit 1 ;;
esac

example_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$example_dir/../.." && pwd)
for tool in rustc cargo; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool not found: %s\n' "$tool" >&2
        exit 1
    fi
done

rust_host=$(rustc -vV | sed -n 's/^host: //p')
if [ -z "$rust_host" ]; then
    printf '%s\n' 'Cannot determine the installed Rust host target.' >&2
    exit 1
fi

build_dir="$example_dir/build"
mkdir -p "$build_dir"
archive="$build_dir/rust-target/$rust_host/release/libbhasa_rust_adapter.a"
executable="$build_dir/rust-interop"
printf 'Interop build directory: %s\n' "$build_dir" >&2

cargo build --offline --locked --release \
    --manifest-path "$example_dir/Cargo.toml" \
    --target "$rust_host" --target-dir "$build_dir/rust-target"

# Reuse the normal compiler target directory as well as the adapter cache.
# Cargo checks freshness and serializes concurrent access to this directory.
cargo run --quiet --offline --locked \
    --manifest-path "$repo_dir/Cargo.toml" \
    --target-dir "$repo_dir/target" \
    -- build "$example_dir/main.bh" --target native \
    -o "$executable" --link "$archive"

"$executable" > "$build_dir/actual.txt"
diff -u "$example_dir/expected.txt" "$build_dir/actual.txt"
cat "$build_dir/actual.txt"
printf '%s\n' 'Rust C ABI interop passed.'
