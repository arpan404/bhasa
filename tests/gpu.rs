#![cfg(target_os = "macos")]

mod common;

use common::*;
use std::process::Command;

const ADAPTER_CHECK: &str = r##"
#include "metal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static bhasa_buffer text(const char *value) {
    bhasa_buffer buffer = {(int64_t)strlen(value), (int64_t)strlen(value),
                           (unsigned char *)value};
    return buffer;
}

static void require(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "GPU TEST FAILED: %s\n", message);
        exit(1);
    }
}

int main(void) {
    bhasa_buffer source = text("#include <metal_stdlib>\n"
        "using namespace metal;\n"
        "kernel void transform(device uchar *data [[buffer(0)]], "
        "uint i [[thread_position_in_grid]]) { "
        "data[i] = uchar((uint(data[i]) * 3u + 7u) & 255u); }\n");
    bhasa_buffer entry = text("transform");
    unsigned char bytes[513];
    bhasa_buffer data = {513, 513, bytes};
    bhasa_buffer empty = {0, 0, NULL};
    bhasa_buffer invalid = {-1, 0, NULL};
    bhasa_buffer missing_storage = {1, 1, NULL};
    bhasa_buffer short_capacity = {2, 1, bytes};
    require(bhasa_rt_gpu_dispatch(NULL, &entry, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "null source must be rejected");
    require(bhasa_rt_gpu_dispatch(&empty, &entry, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "empty source must be rejected");
    require(bhasa_rt_gpu_dispatch(&source, &empty, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "empty entry must be rejected");
    require(bhasa_rt_gpu_dispatch(&source, &entry, &invalid) == BHASA_GPU_INVALID_ARGUMENT,
            "negative length must be rejected");
    require(bhasa_rt_gpu_dispatch(&source, &entry, &missing_storage) == BHASA_GPU_INVALID_ARGUMENT,
            "missing byte storage must be rejected");
    require(bhasa_rt_gpu_dispatch(&source, &entry, &short_capacity) == BHASA_GPU_INVALID_ARGUMENT,
            "length beyond capacity must be rejected");
    unsigned char nul_bytes[] = {'x', 0, 'y'};
    bhasa_buffer nul_text = {3, 3, nul_bytes};
    require(bhasa_rt_gpu_dispatch(&nul_text, &entry, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "embedded NUL source must be rejected");
    require(bhasa_rt_gpu_dispatch(&source, &nul_text, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "embedded NUL entry must be rejected");
    unsigned char bad_utf8[] = {0xff};
    bhasa_buffer utf8 = {1, 1, bad_utf8};
    require(bhasa_rt_gpu_dispatch(&utf8, &entry, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "invalid UTF-8 must be rejected before probing device");
    require(bhasa_rt_gpu_dispatch(&source, &utf8, &data) == BHASA_GPU_INVALID_ARGUMENT,
            "invalid UTF-8 entry must be rejected before probing device");
    int64_t available = bhasa_rt_gpu_available();
    require(available == 0 || available == 1, "availability must be boolean");
    if (!available) {
        memset(bytes, 0xa5, sizeof(bytes));
        require(bhasa_rt_gpu_dispatch(&source, &entry, &data) == BHASA_GPU_UNAVAILABLE,
                "missing device must return an explicit unavailable status");
        for (size_t i = 0; i < sizeof(bytes); ++i) {
            require(bytes[i] == 0xa5, "failed dispatch must preserve host data");
        }
        puts("SKIP GPU execution: no Metal device; argument/error checks passed");
        return 77;
    }
    bhasa_buffer too_large = {(int64_t)UINT32_MAX + 1, (int64_t)UINT32_MAX + 1, bytes};
    require(bhasa_rt_gpu_dispatch(&source, &entry, &too_large) == BHASA_GPU_LIMIT,
            "dispatch length must be checked before allocating or reading data");
    bhasa_buffer bad_source = text("this is not valid Metal source");
    memset(bytes, 0xa5, sizeof(bytes));
    require(bhasa_rt_gpu_dispatch(&bad_source, &entry, &data) == BHASA_GPU_COMPILE_ERROR,
            "invalid kernel must return a compile error");
    bhasa_buffer missing_entry = text("no_such_function");
    require(bhasa_rt_gpu_dispatch(&source, &missing_entry, &data) == BHASA_GPU_ENTRY_NOT_FOUND,
            "missing entry must be distinct from source compilation failure");
    for (size_t i = 0; i < sizeof(bytes); ++i) {
        require(bytes[i] == 0xa5, "compile/entry failure must preserve host data");
    }
    const int64_t lengths[] = {0, 1, 33, 513};
    for (size_t run = 0; run < sizeof(lengths) / sizeof(lengths[0]); ++run) {
        for (size_t i = 0; i < sizeof(bytes); ++i) {
            bytes[i] = (unsigned char)i;
        }
        data.len = lengths[run];
        require(bhasa_rt_gpu_dispatch(&source, &entry, &data) == BHASA_GPU_OK,
                "valid kernel must complete");
        for (size_t i = 0; i < sizeof(bytes); ++i) {
            unsigned char expected = i < (size_t)data.len
                ? (unsigned char)(((unsigned char)i * 3u + 7u) & 255u)
                : (unsigned char)i;
            require(bytes[i] == expected, "GPU byte must match CPU reference");
        }
    }
    puts("Metal adapter: real GPU results verified for 0, 1, 33 and 513 bytes");
    return 0;
}
"##;

fn require_or_report_skip(output: &std::process::Output) {
    if output.status.code() == Some(77) {
        assert!(
            std::env::var_os("BHASA_REQUIRE_GPU").is_none(),
            "GPU hardware was required but unavailable: {}",
            describe(output)
        );
        eprintln!("{}", stdout(output).trim());
    } else {
        succeeds(output);
        eprintln!("{}", stdout(output).trim());
    }
}

#[test]
fn metal_adapter_validates_arguments_and_executes_when_available() {
    let Some(clang) = clang() else { return };
    let workspace = Workspace::new("metal-adapter");
    let harness = workspace.write("gpu-check.c", ADAPTER_CHECK);
    let executable = workspace.file("gpu-check");
    let output = Command::new(clang)
        .args(["-Wall", "-Wextra", "-Werror", "-fno-objc-arc", "-I"])
        .arg(fixture("runtime"))
        .arg(harness)
        .arg(fixture("runtime/metal.m"))
        .args([
            "-framework",
            "Foundation",
            "-framework",
            "Metal",
            "-framework",
            "CoreGraphics",
            "-o",
        ])
        .arg(&executable)
        .output()
        .unwrap();
    succeeds(&output);
    let output = execute(&executable, &workspace);
    require_or_report_skip(&output);
}

#[test]
fn bhasa_gpu_builtins_use_static_and_owned_source_text() {
    if clang().is_none() {
        return;
    }
    let workspace = Workspace::new("metal-bhasa");
    let source = workspace.write(
        "gpu.bh",
        r##"
def main() -> int {
    var data = buffer("abcd")
    if not gpu_available() {
        print("SKIP Bhasa GPU execution: no Metal device")
        return 77
    }
    var source = "#include <metal_stdlib>\nusing namespace metal;\nkernel void inc(device uchar *b [[buffer(0)]], uint i [[thread_position_in_grid]]) { b[i] += 1; }"
    var status = gpu_dispatch(source, "inc", data)
    if status != 0 { return 1 }
    var owned_source = buffer(source)
    var owned_entry = buffer("inc")
    status = gpu_dispatch(owned_source, owned_entry, data)
    if status != 0 { return 2 }
    if not same(data, "cdef") { return 3 }
    print("Bhasa Metal dispatch verified")
    return 0
}
"##,
    );
    for release in [false, true] {
        let executable = build(&workspace, &source, release);
        let output = execute(&executable, &workspace);
        require_or_report_skip(&output);
    }
}
