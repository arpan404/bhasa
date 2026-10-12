#!/bin/sh
# Optional frontend seed rebuild check. Rust remains the supported compiler CLI.
set -eu

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'usage: sh bootstrap/compiler/check.sh' >&2
    exit 2
fi

compiler_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH='' cd -- "$compiler_dir/../.." && pwd)
stage0=${BHASA_BOOTSTRAP:-"$repo_dir/target/release/bhasa"}
clang=${BHASA_CLANG:-clang}

if [ ! -x "$stage0" ]; then
    printf '%s\n' "Bootstrap compiler not found: $stage0" 'Build it with cargo build --release, or set BHASA_BOOTSTRAP.' >&2
    exit 1
fi
if ! command -v "$clang" >/dev/null 2>&1; then
    printf '%s\n' "Clang not found: $clang" >&2
    exit 1
fi
for source in lexer.bh emitter.bh driver.bh; do
    if [ ! -f "$compiler_dir/$source" ]; then
        printf '%s\n' "Missing compiler seed source: $source" >&2
        exit 1
    fi
done

mkdir -p "$repo_dir/build/selfhost"
build_dir=$(mktemp -d "$repo_dir/build/selfhost/run.XXXXXXXX")
printf '%s\n' "Self-hosting build directory: $build_dir"
cat "$compiler_dir/lexer.bh" "$compiler_dir/emitter.bh" "$compiler_dir/driver.bh" > "$build_dir/compiler.bh"

# Resolve the frontend's actual triple, including the deployment OS version.
"$clang" -S -emit-llvm -x c /dev/null -o "$build_dir/target.ll"
target=$(sed -n 's/^target triple = "\([^"]*\)"/\1/p' "$build_dir/target.ll")
if [ -z "$target" ]; then
    printf '%s\n' 'Could not resolve Clang target triple.' >&2
    exit 1
fi

printf '%s\n' 'Building A with the Rust bootstrap.'
"$stage0" build "$build_dir/compiler.bh" --release -o "$build_dir/A"
printf '%s\n' 'Emitting B with A, then linking through the host C toolchain.'
"$build_dir/A" emit-llvm "$build_dir/compiler.bh" --target "$target" > "$build_dir/B.ll"
"$clang" -target "$target" -O2 -pthread -x ir "$build_dir/B.ll" -x c "$repo_dir/runtime/core.c" -o "$build_dir/B"
printf '%s\n' 'Building C with B.'
"$build_dir/B" emit-llvm "$build_dir/compiler.bh" --target "$target" > "$build_dir/C.ll"
"$clang" -target "$target" -O2 -pthread -x ir "$build_dir/C.ll" -x c "$repo_dir/runtime/core.c" -o "$build_dir/C"

cmp "$build_dir/B.ll" "$build_dir/C.ll"
"$build_dir/B" emit-llvm "$build_dir/compiler.bh" --target "$target" > "$build_dir/B.verify.ll"
"$build_dir/C" emit-llvm "$build_dir/compiler.bh" --target "$target" > "$build_dir/C.verify.ll"
cmp "$build_dir/B.verify.ll" "$build_dir/C.verify.ll"
printf '%s\n' 'B and C emit identical LLVM for their complete compiler source.'

# These expected outputs are committed language examples, not compiler output
# from stage 0. B and C compile and execute each program independently.
for name in hello arithmetic loops ranges bits stepped_ranges bindings recursion scopes short_circuit strings; do
    source="$repo_dir/examples/basics/$name.bh"
    case "$name" in
        hello) printf '%s\n' 'Hello, Bhasa!' 'नमस्ते • こんにちは • 🌍' ;;
        arithmetic) printf '%s\n' 14 20 12 10 -2 -1 1 9223372036854775807 -9223372036854775808 true true true true true true false true true ;;
        loops) printf '%s\n' 14 true ;;
        ranges) printf '%s\n' 12 6 3 ;;
        bits) printf '%s\n' 2 15 5 -11 8 -4 true -9223372036854775808 -9223372036854775808 ;;
        stepped_ranges) printf '%s\n' 12 8 2 ;;
        bindings) printf '%s\n' 'hello world' 2 12 ;;
        recursion) printf '%s\n' 3628800 55 ;;
        scopes) printf '%s\n' 20 30 20 10 11 ;;
        short_circuit) printf '%s\n' false true false true 10 true 20 true 30 true 40 true false true 50 true ;;
        strings) printf '%b' 'नमस्ते\n你好 🌍\nline one\nline two\ntab\tquote"slash\\end\ncarriage\rreturn\n100% literal %s %d\n\n' ;;
    esac > "$build_dir/$name.expected"
    for stage in B C; do
        "$build_dir/$stage" emit-llvm "$source" --target "$target" > "$build_dir/$stage-$name.ll"
        "$clang" -target "$target" -O2 -pthread -x ir "$build_dir/$stage-$name.ll" -x c "$repo_dir/runtime/core.c" -o "$build_dir/$stage-$name"
        "$build_dir/$stage-$name" > "$build_dir/$stage-$name.txt"
        cmp "$build_dir/$name.expected" "$build_dir/$stage-$name.txt"
    done
    cmp "$build_dir/B-$name.txt" "$build_dir/C-$name.txt"
done

# Exercise features that the frontend source itself does not need. Force an
# absent CUDA driver so this check is deterministic on hosts with or without GPUs.
cat > "$build_dir/native.bh" <<'BHASA'
enum NativeResult { empty; value(leaf: NativeLeaf); error(code: int) }
def native_result_text(value: NativeResult) -> buffer {
    match value {
        case NativeResult.empty { return buffer() }
        case NativeResult.value(leaf) { return leaf.text }
        case NativeResult.error(code) { return to_buffer(code) }
    }
}
struct NativeLeaf {
    text: buffer
    numbers: ints
    def suffix(self, text: str) { append(self.text, text) }
    def contents(self) -> buffer { return self.text }
}
struct NativeRecord {
    leaf: NativeLeaf
    listener: tcp_listener
    def copy(self) -> NativeRecord { return self }
}
def copy_record(value: NativeRecord) -> NativeRecord { return value }
extern def foreign(value: int) -> int
def worker(value: int) -> int { return value + 1 }
def typed_worker(value: NativeLeaf) -> NativeLeaf {
    value.suffix(" worker")
    push(value.numbers, 9)
    return value
}
def void_worker(value: buffer) { append(value, " consumed") }
def main() {
    var job = spawn(worker(foreign(20)))
    var result = join(job)
    print(result)
    print(gpu_available())
    var data = buffer("safe")
    var status = gpu_dispatch("source", "entry", data)
    print(status)
    print(data)
    var record = NativeRecord(NativeLeaf(buffer("record"), ints()), tcp_listen_on("invalid", 0))
    push(record.leaf.numbers, 7)
    var tagged = NativeResult.value(record.leaf)
    var tagged_copy = tagged
    print(native_result_text(tagged_copy))
    print(native_result_text(NativeResult.error(42)))
    var copied = copy_record(record)
    copied.leaf.text = copied.leaf.text
    copied.leaf.suffix("!")
    set(copied.leaf.numbers, 0, 8)
    print(record.leaf.text)
    print(copied.leaf.text)
    print(get(record.leaf.numbers, 0))
    print(get(copied.leaf.numbers, 0))
    print(tcp_listener_status(copied.listener) > 0)
    print(copied.copy().leaf.contents())
    var invalid = tcp_listen_on(buffer("invalid"), 0)
    print(tcp_listener_status(invalid) > 0)
    var typed_input = NativeLeaf(buffer("typed"), ints())
    push(typed_input.numbers, 7)
    var typed_job = async_spawn(typed_worker(typed_input))
    typed_input.suffix(" caller")
    var typed_result = await(typed_job)
    var void_job = spawn(void_worker(typed_result.text))
    join(void_job)
    print(typed_input.text)
    print(typed_result.text)
    print(len(typed_input.numbers))
    print(len(typed_result.numbers))
}
BHASA
cat > "$build_dir/adapter.c" <<'C'
#include <stdint.h>
int64_t foreign(int64_t value) { return value * 2; }
C
printf '%s\n' 41 false 1 safe record 42 record 'record!' 7 8 true 'record!' true 'typed caller' 'typed worker' 1 2 > "$build_dir/native.expected"
printf '%s\n' 'bhasa GPU: CUDA driver or NVRTC library is unavailable' > "$build_dir/native.stderr.expected"
for stage in B C; do
    "$build_dir/$stage" emit-llvm "$build_dir/native.bh" --target "$target" > "$build_dir/$stage-native.ll"
    set --
    case "$target" in *linux*) set -- -ldl ;; esac
    "$clang" -target "$target" -O2 -pthread -x ir "$build_dir/$stage-native.ll" -x c \
        "$repo_dir/runtime/core.c" "$repo_dir/runtime/net.c" "$repo_dir/runtime/concurrency.c" \
        "$repo_dir/runtime/cuda.c" "$build_dir/adapter.c" "$@" -o "$build_dir/$stage-native"
    BHASA_CUDA_DRIVER="$build_dir/absent-cuda" BHASA_NVRTC="$build_dir/absent-nvrtc" \
        "$build_dir/$stage-native" > "$build_dir/$stage-native.txt" 2> "$build_dir/$stage-native.stderr"
    cmp "$build_dir/native.expected" "$build_dir/$stage-native.txt"
    cmp "$build_dir/native.stderr.expected" "$build_dir/$stage-native.stderr"
done

printf '%s\n' 'Self-hosting seed gate passed: A built B, B built C, and B/C LLVM and example output agree.'
printf '%s\n' 'B and C also compiled and ran foreign calls, native tasks, nested records, methods, tagged enums and exhaustive matching, TCP handle copies, explicit-bind error handles and GPU-unavailable handling.'
printf '%s\n' 'This validates the tested frontend features, not complete language or GPU hardware coverage.'
